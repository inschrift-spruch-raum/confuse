use std::collections::HashMap;
#[cfg(test)]
use std::path::Path;

#[cfg(test)]
use super::CreatedPath;
use super::DokanAdapter;
use crate::dokan_impl::*;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::*;
#[cfg(test)]
use crate::fuser_facade::request::Request;
use crate::fuser_facade::request::request_from_ids;
#[cfg(test)]
use crate::fuser_facade::types::{Errno, INodeNo};
#[cfg(test)]
use crate::fuser_facade::types::{FileAttr, Generation};

#[cfg(test)]
struct CreatedLink<'a, FS: Filesystem> {
    fs: &'a FS,
    req: &'a Request,
    path: &'a widestring::U16CStr,
    parent_ino: INodeNo,
    name: &'a std::ffi::OsStr,
    attr: FileAttr,
    generation: Generation,
    ttl: std::time::Duration,
}

impl<FS: Filesystem> DokanAdapter<FS> {
    pub(super) fn cleanup_delete_on_close(
        &self, file_name: &widestring::U16CStr, context: &AdapterContext,
    ) {
        let Ok(fs) = self.fs.lock() else {
            return;
        };
        let req = request_from_ids(context.request_ids);
        let (parent_path, leaf) = split_parent_and_name(file_name);
        let Ok(parent_ino) = self.resolve_parent_ino(&*fs, &req, &parent_path) else {
            self.drain_resolver_forgets(&*fs, &req);
            return;
        };
        let reply = ReplyEmpty::capture();
        if context.is_dir {
            fs.rmdir(&req, parent_ino, leaf.as_os_str(), reply.duplicate());
        } else {
            fs.unlink(&req, parent_ino, leaf.as_os_str(), reply.duplicate());
        }
        let status = *reply.status.lock().unwrap_or_else(|err| err.into_inner());
        self.drain_resolver_forgets(&*fs, &req);
        if !matches!(status, Some(Ok(()))) {
            return;
        }
        self.invalidate_path_cache(&*fs, &req, file_name);
        if context.ino != ino(0) {
            self.invalidate_inode_attr(context.ino);
        }
        self.invalidate_inode_attr(parent_ino);
    }
    pub(super) fn remap_open_state_after_rename(
        &self, file_name: &widestring::U16CStr, new_file_name: &widestring::U16CStr,
    ) {
        let old_key = file_name.to_string_lossy();
        let new_key = new_file_name.to_string_lossy();
        if let Ok(mut handles) = self.handles.lock() {
            Self::remap_descendant_keys(&mut handles, &old_key, &new_key);
        }
        if let Ok(mut offsets) = self.dir_offsets.lock() {
            Self::remap_descendant_keys(&mut offsets, &old_key, &new_key);
        }
    }

    fn remap_descendant_keys<T>(items: &mut HashMap<String, T>, old_root: &str, new_root: &str) {
        let keys: Vec<String> = items.keys().cloned().collect();
        let remapped: Vec<(String, T)> = keys
            .into_iter()
            .filter_map(|key| {
                let next_key = rename_descendant_path_key(old_root, new_root, &key)?;
                items.remove(&key).map(|value| (next_key, value))
            })
            .collect();
        items.extend(remapped);
    }

    #[cfg(test)]
    pub(crate) fn readlink_impl(&self, req: &Request, ino: INodeNo) -> ReplyData {
        let reply = ReplyData::capture();
        match self.fs.lock() {
            Ok(fs) => fs.readlink(req, ino, reply.duplicate()),
            Err(_) => reply.duplicate().error(Errno::EIO),
        }
        reply
    }

    #[cfg(test)]
    pub(crate) fn symlink_path_impl(
        &self, req: &Request, link_path: &widestring::U16CStr, target: &Path,
    ) -> ReplyEntry {
        let reply = ReplyEntry::capture();
        let Ok(fs) = self.fs.lock() else {
            reply.duplicate().error(Errno::EIO);
            return reply;
        };
        let (parent_path, link_name) = split_parent_and_name(link_path);
        let Ok(parent_ino) = self.resolve_parent_ino(&*fs, req, &parent_path) else {
            reply.duplicate().error(Errno::ENOENT);
            return reply;
        };
        fs.symlink(
            req,
            parent_ino,
            link_name.as_os_str(),
            target,
            reply.duplicate(),
        );
        if let Ok(Some(Ok(payload))) = reply.status.lock().map(|status| *status) {
            self.finish_link_created_path(CreatedLink {
                fs: &*fs,
                req,
                path: link_path,
                parent_ino,
                name: link_name.as_os_str(),
                attr: payload.attr,
                generation: payload.generation,
                ttl: payload.ttl,
            });
        }
        reply
    }

    #[cfg(test)]
    pub(crate) fn link_path_impl(
        &self, req: &Request, ino: INodeNo, new_path: &widestring::U16CStr,
    ) -> ReplyEntry {
        let reply = ReplyEntry::capture();
        let Ok(fs) = self.fs.lock() else {
            reply.duplicate().error(Errno::EIO);
            return reply;
        };
        let (new_parent_path, new_name) = split_parent_and_name(new_path);
        let Ok(new_parent_ino) = self.resolve_parent_ino(&*fs, req, &new_parent_path) else {
            reply.duplicate().error(Errno::ENOENT);
            return reply;
        };
        fs.link(
            req,
            ino,
            new_parent_ino,
            new_name.as_os_str(),
            reply.duplicate(),
        );
        if let Ok(Some(Ok(payload))) = reply.status.lock().map(|status| *status) {
            self.invalidate_inode_attr(ino);
            self.finish_link_created_path(CreatedLink {
                fs: &*fs,
                req,
                path: new_path,
                parent_ino: new_parent_ino,
                name: new_name.as_os_str(),
                attr: payload.attr,
                generation: payload.generation,
                ttl: payload.ttl,
            });
        }
        reply
    }

    #[cfg(test)]
    fn finish_link_created_path(&self, created: CreatedLink<'_, FS>) {
        self.invalidate_inode_attr(created.parent_ino);
        self.remember_created_path(
            created.fs,
            created.req,
            CreatedPath {
                path: created.path,
                parent: created.parent_ino,
                name: created.name,
                attr: created.attr,
                generation: created.generation,
                ttl: created.ttl,
            },
        );
    }
}
