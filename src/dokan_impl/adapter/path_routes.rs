use std::ffi::OsStr;

use super::{AdapterContext, DokanAdapter, PathResolver, PositivePathRecord, ResolvedEntry};
use crate::dokan_impl::*;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::ReplyEntry;
use crate::fuser_facade::request::Request;
use crate::fuser_facade::types::{FileAttr, Generation, INodeNo};

struct LookupComponent<'a> {
    path: &'a OsStr,
    parent: INodeNo,
    parent_generation: Generation,
    name: &'a OsStr,
}

pub(crate) struct CreatedPath<'a> {
    pub(crate) path: &'a widestring::U16CStr,
    pub(crate) parent: INodeNo,
    pub(crate) name: &'a OsStr,
    pub(crate) attr: FileAttr,
    pub(crate) generation: Generation,
    pub(crate) ttl: std::time::Duration,
}

impl<FS: Filesystem> DokanAdapter<FS> {
    pub(super) fn resolve_path_ino(
        &self, fs: &FS, req: &Request, path: &OsStr,
    ) -> Result<INodeNo, i32> {
        if PathResolver::normalize_path(path) == "\\" {
            return Ok(INodeNo::ROOT);
        }

        let mut current_ino = INodeNo::ROOT;
        let mut current_generation = Generation(0);
        let mut current_path = String::from("\\");
        self.drain_expired_resolver_forgets(fs, req);
        for component in PathResolver::normalized_components(path) {
            let child_path = child_path_from_parent(&current_path, &component);
            let child_os = OsStr::new(&child_path);
            if let Some(entry) =
                self.cached_path_component(fs, req, child_os, current_ino, current_generation)?
            {
                current_ino = entry.ino;
                current_generation = entry.generation;
                current_path = child_path;
                continue;
            }
            let resolved = self.lookup_path_component(
                fs,
                req,
                LookupComponent {
                    path: child_os,
                    parent: current_ino,
                    parent_generation: current_generation,
                    name: OsStr::new(&component),
                },
            )?;
            current_ino = resolved.ino;
            current_generation = resolved.generation;
            current_path = child_path;
        }
        Ok(current_ino)
    }

    fn cached_path_component(
        &self, fs: &FS, req: &Request, path: &OsStr, parent: INodeNo, parent_generation: Generation,
    ) -> Result<Option<ResolvedEntry>, i32> {
        let cached =
            self.resolver
                .lock()
                .map_err(|_| libc::EIO)?
                .cached(path, parent, parent_generation);
        match cached {
            Some(Ok(entry)) => Ok(Some(entry)),
            Some(Err(err)) => {
                self.drain_resolver_forgets(fs, req);
                Err(err)
            }
            None => Ok(None),
        }
    }

    fn lookup_path_component(
        &self, fs: &FS, req: &Request, component: LookupComponent<'_>,
    ) -> Result<ResolvedEntry, i32> {
        let reply = ReplyEntry::capture();
        fs.lookup(req, component.parent, component.name, reply.duplicate());
        match *reply.status.lock().map_err(|_| libc::EIO)? {
            Some(Ok(payload)) => self
                .resolver
                .lock()
                .map_err(|_| libc::EIO)
                .map(|mut resolver| {
                    resolver.remember_lookup(
                        component.path,
                        component.parent,
                        component.parent_generation,
                        component.name,
                        payload,
                    )
                }),
            Some(Err(err)) => {
                self.remember_negative_lookup(fs, req, err, &component)?;
                Err(err)
            }
            None => {
                self.drain_resolver_forgets(fs, req);
                Err(libc::EIO)
            }
        }
    }

    fn remember_negative_lookup(
        &self, fs: &FS, req: &Request, err: i32, component: &LookupComponent<'_>,
    ) -> Result<(), i32> {
        if err == libc::ENOENT {
            self.resolver
                .lock()
                .map_err(|_| libc::EIO)?
                .remember_negative(
                    component.path,
                    component.parent,
                    component.parent_generation,
                    component.name,
                );
        }
        self.drain_resolver_forgets(fs, req);
        Ok(())
    }

    pub(super) fn resolve_parent_ino(
        &self, fs: &FS, req: &Request, parent_path: &OsStr,
    ) -> Result<INodeNo, i32> {
        self.resolve_path_ino(fs, req, parent_path)
    }

    pub(super) fn resolve_context_or_path(
        &self, fs: &FS, req: &Request, path: &widestring::U16CStr, context: &AdapterContext,
    ) -> Result<AdapterContext, i32> {
        if let Some(ctx) = resolve_ctx(path, context) {
            return Ok(ctx);
        }
        Ok(AdapterContext {
            ino: self.resolve_path_ino(fs, req, OsStr::new(&path.to_string_lossy()))?,
            request_ids: context.request_ids,
            ..Default::default()
        })
    }

    pub(super) fn invalidate_path_cache(&self, fs: &FS, req: &Request, path: &widestring::U16CStr) {
        let forgets = self
            .resolver
            .lock()
            .map(|mut resolver| resolver.invalidate_subtree(OsStr::new(&path.to_string_lossy())))
            .unwrap_or_default();
        for (ino, nlookup) in forgets {
            fs.forget(req, ino, nlookup);
        }
    }

    #[cfg(test)]
    pub(crate) fn invalidate_entry_cache(
        &self, fs: &FS, req: &Request, parent: INodeNo, name: &OsStr,
    ) {
        let forgets = self
            .resolver
            .lock()
            .map(|mut resolver| resolver.invalidate_entry(parent, name))
            .unwrap_or_default();
        for (ino, nlookup) in forgets {
            fs.forget(req, ino, nlookup);
        }
    }

    pub(super) fn invalidate_inode_attr(&self, ino: INodeNo) {
        if let Ok(mut resolver) = self.resolver.lock() {
            resolver.invalidate_inode_attr(ino);
        }
    }

    pub(super) fn invalidate_parent_attr_for_path(
        &self, fs: &FS, req: &Request, path: &widestring::U16CStr,
    ) {
        let (parent_path, _) = split_parent_and_name(path);
        if let Ok(parent_ino) = self.resolve_parent_ino(fs, req, &parent_path) {
            self.invalidate_inode_attr(parent_ino);
        }
    }

    pub(super) fn finish_metadata_mutation(
        &self, fs: &FS, req: &Request, path: &widestring::U16CStr, ino: INodeNo,
    ) {
        self.invalidate_inode_attr(ino);
        self.invalidate_parent_attr_for_path(fs, req, path);
        self.invalidate_path_cache(fs, req, path);
    }

    pub(super) fn remember_created_path(&self, fs: &FS, req: &Request, created: CreatedPath<'_>) {
        if let Ok(mut resolver) = self.resolver.lock() {
            let parent_generation = resolver.generation_for(created.parent);
            resolver.remember_positive(PositivePathRecord {
                path: OsStr::new(&created.path.to_string_lossy()),
                parent: created.parent,
                parent_generation,
                name: created.name,
                attr: created.attr,
                generation: created.generation,
                ttl: created.ttl,
            });
        }
        self.drain_resolver_forgets(fs, req);
    }

    pub(super) fn drain_resolver_forgets(&self, fs: &FS, req: &Request) {
        let forgets = self
            .resolver
            .lock()
            .map(|mut resolver| resolver.take_pending_forgets())
            .unwrap_or_default();
        for (ino, nlookup) in forgets {
            fs.forget(req, ino, nlookup);
        }
    }

    pub(super) fn drain_expired_resolver_forgets(&self, fs: &FS, req: &Request) {
        let forgets = self
            .resolver
            .lock()
            .map(|mut resolver| resolver.reap_expired())
            .unwrap_or_default();
        for (ino, nlookup) in forgets {
            fs.forget(req, ino, nlookup);
        }
    }
}

fn child_path_from_parent(parent: &str, component: &str) -> String {
    if parent == "\\" {
        format!("\\{component}")
    } else {
        format!("{parent}\\{component}")
    }
}
