use std::ffi::OsStr;

use winapi::shared::ntstatus::*;
use winapi::um::winnt::*;

use super::{AdapterContext, CreatedPath, DokanAdapter};
use crate::dokan_impl::*;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::*;
use crate::fuser_facade::request::{Request, RequestIds, request_from_ids};
use crate::fuser_facade::types::{FileAttr, FileType, Generation, INodeNo};

struct CreateEntryRequest<'a, FS: Filesystem> {
    fs: &'a FS,
    req: &'a Request,
    file_name: &'a widestring::U16CStr,
    parent_ino: INodeNo,
    leaf: &'a OsStr,
    path_ino: INodeNo,
    request_ids: RequestIds,
    mode: u32,
    is_dir_open: bool,
}

struct OpenEntryRequest<'a, FS: Filesystem> {
    fs: &'a FS,
    req: &'a Request,
    file_name: &'a widestring::U16CStr,
    path_ino: INodeNo,
    request_ids: RequestIds,
    open_flags: i32,
    is_dir_open: bool,
}

struct CreatedEntry<'a, FS: Filesystem> {
    fs: &'a FS,
    req: &'a Request,
    file_name: &'a widestring::U16CStr,
    parent_ino: INodeNo,
    leaf: &'a OsStr,
    attr: FileAttr,
    generation: Generation,
    ttl: std::time::Duration,
    ctx: AdapterContext,
}

impl<FS: Filesystem> DokanAdapter<FS> {
    pub(super) fn create_file_impl(
        &self, file_name: &widestring::U16CStr, desired_access: winapi::um::winnt::ACCESS_MASK,
        file_attributes: u32, create_disposition: u32, create_options: u32,
        request_ids: RequestIds,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<AdapterContext>> {
        let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(request_ids);
        let (parent_path, leaf) = split_parent_and_name(file_name);
        let parent_ino = match self.resolve_parent_ino(&*fs, &req, &parent_path) {
            Ok(parent_ino) => parent_ino,
            Err(err) => {
                self.drain_resolver_forgets(&*fs, &req);
                return Err(errno_to_ntstatus(err));
            }
        };
        let path_ino = if file_name.to_string_lossy() == "\\" {
            INodeNo::ROOT
        } else {
            ino(0)
        };
        if leaf.is_empty() {
            return self.create_root_context(file_name, request_ids);
        }
        let create_plan = create_disposition_plan(create_disposition);
        let is_dir_open = is_directory_open(create_options);
        let open_flags = access_mask_to_open_flags(desired_access);
        if !create_plan.creates_entry() {
            return self.open_existing_entry(OpenEntryRequest {
                fs: &*fs,
                req: &req,
                file_name,
                path_ino,
                request_ids,
                open_flags,
                is_dir_open,
            });
        }
        let mode = mode_from_file_attributes(file_attributes);
        if is_dir_open {
            return self.create_directory_entry(CreateEntryRequest {
                fs: &*fs,
                req: &req,
                file_name,
                parent_ino,
                leaf: leaf.as_os_str(),
                path_ino,
                request_ids,
                mode,
                is_dir_open,
            });
        }
        self.create_file_entry_or_open_existing(
            CreateEntryRequest {
                fs: &*fs,
                req: &req,
                file_name,
                parent_ino,
                leaf: leaf.as_os_str(),
                path_ino,
                request_ids,
                mode,
                is_dir_open,
            },
            create_plan,
            open_flags,
        )
    }

    fn create_directory_entry(
        &self, create: CreateEntryRequest<'_, FS>,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<AdapterContext>> {
        let entry_reply = ReplyEntry::capture();
        create.fs.mkdir(
            create.req,
            create.parent_ino,
            create.leaf,
            create.mode,
            0,
            entry_reply.duplicate(),
        );
        let state = *entry_reply
            .status
            .lock()
            .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        self.drain_resolver_forgets(create.fs, create.req);
        match state {
            Some(Ok(payload)) => {
                let attr = payload.attr;
                let entry_ino = if attr.ino != ino(0) {
                    attr.ino
                } else {
                    create.path_ino
                };
                let ctx = AdapterContext {
                    fh: fh(0),
                    flags: fopen_flags(0),
                    ino: entry_ino,
                    is_dir: true,
                    lock_owner: None,
                    request_ids: create.request_ids,
                };
                self.finish_created_entry(CreatedEntry {
                    fs: create.fs,
                    req: create.req,
                    file_name: create.file_name,
                    parent_ino: create.parent_ino,
                    leaf: create.leaf,
                    attr,
                    generation: payload.generation,
                    ttl: payload.ttl,
                    ctx,
                });
                Ok(dokan::CreateFileInfo {
                    context: ctx,
                    is_dir: true,
                    new_file_created: true,
                })
            }
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn create_file_entry_or_open_existing(
        &self, create: CreateEntryRequest<'_, FS>, create_plan: CreateDispositionPlan,
        open_flags: i32,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<AdapterContext>> {
        let reply = ReplyCreate::capture();
        create.fs.create(
            create.req,
            create.parent_ino,
            create.leaf,
            create.mode,
            0,
            open_flags,
            reply.duplicate(),
        );
        let state = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        self.drain_resolver_forgets(create.fs, create.req);
        match state {
            Some(Ok(payload)) => self.finish_created_file(create, payload),
            Some(Err(err)) if create_plan.opens_existing_after_collision(err) => {
                self.open_collision_entry(OpenEntryRequest::from_create(&create, open_flags))
            }
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn create_root_context(
        &self, file_name: &widestring::U16CStr, request_ids: RequestIds,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<AdapterContext>> {
        let ctx = AdapterContext {
            fh: fh(0),
            flags: fopen_flags(0),
            ino: INodeNo::ROOT,
            is_dir: true,
            lock_owner: None,
            request_ids,
        };
        self.remember_open_context(file_name, ctx);
        Ok(dokan::CreateFileInfo {
            context: ctx,
            is_dir: true,
            new_file_created: false,
        })
    }

    fn finish_created_file(
        &self, create: CreateEntryRequest<'_, FS>,
        payload: crate::fuser_facade::reply::ReplyCreatePayload,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<AdapterContext>> {
        let attr = payload.attr;
        let entry_ino = if attr.ino != ino(0) {
            attr.ino
        } else {
            create.path_ino
        };
        let ctx = AdapterContext {
            fh: payload.fh,
            flags: payload.flags,
            ino: entry_ino,
            is_dir: matches!(attr.kind, FileType::Directory) || create.is_dir_open,
            lock_owner: None,
            request_ids: create.request_ids,
        };
        self.finish_created_entry(CreatedEntry {
            fs: create.fs,
            req: create.req,
            file_name: create.file_name,
            parent_ino: create.parent_ino,
            leaf: create.leaf,
            attr,
            generation: payload.generation,
            ttl: payload.ttl,
            ctx,
        });
        Ok(dokan::CreateFileInfo {
            context: ctx,
            is_dir: ctx.is_dir,
            new_file_created: true,
        })
    }

    fn open_collision_entry(
        &self, open: OpenEntryRequest<'_, FS>,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<AdapterContext>> {
        if open.path_ino == ino(0) {
            self.invalidate_path_cache(open.fs, open.req, open.file_name);
        }
        let actual_ino = self.resolve_open_ino(&open)?;
        self.open_resolved_path(open, actual_ino)
    }

    fn open_existing_entry(
        &self, open: OpenEntryRequest<'_, FS>,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<AdapterContext>> {
        let actual_ino = self.resolve_open_ino(&open)?;
        self.open_resolved_path(open, actual_ino)
    }

    fn resolve_open_ino(&self, open: &OpenEntryRequest<'_, FS>) -> Result<INodeNo, i32> {
        if open.path_ino != ino(0) {
            return Ok(open.path_ino);
        }
        self.resolve_path_ino(
            open.fs,
            open.req,
            OsStr::new(&open.file_name.to_string_lossy()),
        )
        .map_err(|err| {
            self.drain_resolver_forgets(open.fs, open.req);
            errno_to_ntstatus(err)
        })
    }

    fn open_resolved_path(
        &self, open: OpenEntryRequest<'_, FS>, actual_ino: INodeNo,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<AdapterContext>> {
        let reply = ReplyOpen::capture();
        if open.is_dir_open {
            open.fs.opendir(
                open.req,
                actual_ino,
                crate::dokan_impl::open_flags(open.open_flags),
                reply.duplicate(),
            );
        } else {
            open.fs.open(
                open.req,
                actual_ino,
                crate::dokan_impl::open_flags(open.open_flags),
                reply.duplicate(),
            );
        }
        let opened = *reply.opened.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        self.drain_resolver_forgets(open.fs, open.req);
        match opened {
            Some(Ok(payload)) => {
                let ctx = AdapterContext {
                    fh: payload.fh,
                    flags: payload.flags,
                    ino: actual_ino,
                    is_dir: open.is_dir_open,
                    lock_owner: None,
                    request_ids: open.request_ids,
                };
                self.remember_open_context(open.file_name, ctx);
                Ok(dokan::CreateFileInfo {
                    context: ctx,
                    is_dir: ctx.is_dir,
                    new_file_created: false,
                })
            }
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn finish_created_entry(&self, created: CreatedEntry<'_, FS>) {
        self.remember_open_context(created.file_name, created.ctx);
        self.invalidate_inode_attr(created.parent_ino);
        self.invalidate_inode_attr(created.ctx.ino);
        self.remember_created_path(
            created.fs,
            created.req,
            CreatedPath {
                path: created.file_name,
                parent: created.parent_ino,
                name: created.leaf,
                attr: created.attr,
                generation: created.generation,
                ttl: created.ttl,
            },
        );
    }

    fn remember_open_context(&self, file_name: &widestring::U16CStr, ctx: AdapterContext) {
        if let Ok(mut handles) = self.handles.lock() {
            handles.insert(file_name.to_string_lossy(), ctx);
        }
    }
}

impl<'a, FS: Filesystem> OpenEntryRequest<'a, FS> {
    fn from_create(create: &CreateEntryRequest<'a, FS>, open_flags: i32) -> Self {
        Self {
            fs: create.fs,
            req: create.req,
            file_name: create.file_name,
            path_ino: create.path_ino,
            request_ids: create.request_ids,
            open_flags,
            is_dir_open: create.is_dir_open,
        }
    }
}

impl CreateDispositionPlan {
    fn creates_entry(&self) -> bool {
        matches!(
            self,
            Self::CreateOnly | Self::Supersede | Self::CreateThenOpenOnExists
        )
    }

    fn opens_existing_after_collision(&self, err: i32) -> bool {
        err == libc::EEXIST && matches!(self, Self::CreateThenOpenOnExists | Self::Supersede)
    }
}

fn mode_from_file_attributes(file_attributes: u32) -> u32 {
    if (file_attributes & FILE_ATTRIBUTE_READONLY) != 0 {
        0o444
    } else {
        0o644
    }
}
