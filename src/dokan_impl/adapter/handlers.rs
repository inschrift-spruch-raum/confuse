use std::ffi::OsStr;
use std::time::SystemTime;

use widestring::U16CString;
use winapi::shared::ntstatus::*;
use winapi::um::winnt::*;

use super::{AdapterContext, DokanAdapter};
use crate::dokan_impl::*;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::fuse_abi::consts::FUSE_ROOT_ID;
use crate::fuser_facade::reply::*;
use crate::fuser_facade::request::Request;
use crate::fuser_facade::request::{
    RequestIds, request_from_ids, request_from_info, request_ids_from_create_info,
};
use crate::fuser_facade::types::{FileType, TimeOrNow};

// ---------------------------------------------------------------------------
// Helper: recursively delete directory contents
// ---------------------------------------------------------------------------

/// Recursively delete all entries under `dir_ino` (depth-first).
fn rmdir_recursive<FS: Filesystem>(fs: &mut FS, req: &Request, dir_ino: u64) {
    let reply = ReplyDirectory::default();
    fs.readdir(req, dir_ino, 0, 0, reply.clone());
    if let Ok(entries) = reply.entries.lock() {
        let entry_list: Vec<(u64, FileType, std::ffi::OsString)> = entries
            .iter()
            .filter_map(|(ino, _offset, kind, name)| {
                let n = name.to_string_lossy();
                if n == "." || n == ".." {
                    None
                } else {
                    Some((*ino, *kind, name.to_os_string()))
                }
            })
            .collect();
        drop(entries);
        for (entry_ino, kind, name) in entry_list {
            if matches!(kind, FileType::Directory) {
                rmdir_recursive(fs, req, entry_ino);
                let rmdir_reply = ReplyEmpty::default();
                fs.rmdir(req, dir_ino, &name, rmdir_reply.clone());
            } else {
                let unlink_reply = ReplyEmpty::default();
                fs.unlink(req, dir_ino, &name, unlink_reply.clone());
            }
        }
    }
}

// ---------------------------------------------------------------------------
// FileSystemHandler implementation — translates Dokan callbacks to fuser calls
// ---------------------------------------------------------------------------

impl<'c, 'h: 'c, FS: Filesystem + 'h> dokan::FileSystemHandler<'c, 'h> for DokanAdapter<FS> {
    type Context = AdapterContext;

    fn delete_file(
        &'h self, _file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        // Dokan: just check if deletable, actual deletion happens in cleanup
        Ok(())
    }

    fn delete_directory(
        &'h self, _file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        // Dokan: just check if deletable, actual deletion happens in cleanup
        Ok(())
    }

    fn move_file(
        &'h self, file_name: &widestring::U16CStr, new_file_name: &widestring::U16CStr,
        replace_if_existing: bool, _info: &dokan::OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let (old_parent_path, old_leaf) = split_parent_and_name(file_name);
        let (new_parent_path, new_leaf) = split_parent_and_name(new_file_name);
        let req = request_from_ids(_context.request_ids);
        let old_parent_ino = resolve_parent_ino(&mut *fs, &req, &old_parent_path);
        let new_parent_ino = resolve_parent_ino(&mut *fs, &req, &new_parent_path);
        match rename_with_replace_policy(
            &mut *fs,
            &req,
            old_parent_ino,
            old_leaf.as_os_str(),
            new_parent_ino,
            new_leaf.as_os_str(),
            replace_if_existing,
        ) {
            Ok(()) => {
                if let Ok(mut handles) = self.handles.lock() {
                    let old_key = file_name.to_string_lossy();
                    let new_key = new_file_name.to_string_lossy();
                    let mut remapped: Vec<(String, AdapterContext)> = Vec::new();
                    let keys: Vec<String> = handles.keys().cloned().collect();
                    for key in keys {
                        if let Some(next_key) = rename_descendant_path_key(&old_key, &new_key, &key)
                            && let Some(ctx) = handles.remove(&key)
                        {
                            remapped.push((next_key, ctx));
                        }
                    }
                    for (k, v) in remapped {
                        handles.insert(k, v);
                    }
                }
                if let Ok(mut offsets) = self.dir_offsets.lock() {
                    let old_key = file_name.to_string_lossy();
                    let new_key = new_file_name.to_string_lossy();
                    let mut remapped: Vec<(String, i64)> = Vec::new();
                    let keys: Vec<String> = offsets.keys().cloned().collect();
                    for key in keys {
                        if let Some(next_key) = rename_descendant_path_key(&old_key, &new_key, &key)
                            && let Some(off) = offsets.remove(&key)
                        {
                            remapped.push((next_key, off));
                        }
                    }
                    for (k, v) in remapped {
                        offsets.insert(k, v);
                    }
                }
                Ok(())
            }
            Err(status) => Err(status),
        }
    }

    fn set_end_of_file(
        &'h self, file_name: &widestring::U16CStr, offset: i64,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyAttr::default();
        let resolved = resolve_ctx(file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);
        fs.setattr(
            &req,
            resolved.ino,
            None,
            None,
            None,
            Some(offset as u64),
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            reply.clone(),
        );
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(_)) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn set_allocation_size(
        &'h self, file_name: &widestring::U16CStr, alloc_size: i64,
        info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        self.set_end_of_file(file_name, alloc_size, info, context)
    }

    fn set_file_attributes(
        &'h self, file_name: &widestring::U16CStr, file_attributes: u32,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyAttr::default();
        let resolved = resolve_ctx(file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let mode = if (file_attributes & FILE_ATTRIBUTE_READONLY) != 0 {
            Some(0o444_u32)
        } else {
            Some(0o644_u32)
        };
        let req = request_from_ids(context.request_ids);
        fs.setattr(
            &req,
            resolved.ino,
            mode,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            None,
            reply.clone(),
        );
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(_)) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn set_file_time(
        &'h self, file_name: &widestring::U16CStr, creation_time: dokan::FileTimeOperation,
        last_access_time: dokan::FileTimeOperation, last_write_time: dokan::FileTimeOperation,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyAttr::default();
        let resolved = resolve_ctx(file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);
        fs.setattr(
            &req,
            resolved.ino,
            None,
            None,
            None,
            None,
            filetime_op_to_option(last_access_time).map(TimeOrNow::SpecificTime),
            filetime_op_to_option(last_write_time).map(TimeOrNow::SpecificTime),
            filetime_op_to_option(creation_time),
            None,
            filetime_op_to_option(creation_time),
            None,
            None,
            None,
            reply.clone(),
        );
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(_)) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn get_disk_free_space(
        &'h self, _info: &dokan::OperationInfo<'c, 'h, Self>,
    ) -> dokan::OperationResult<dokan::DiskSpaceInfo> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyStatfs::default();
        let req = request_from_info(_info);
        fs.statfs(&req, FUSE_ROOT_ID, reply.clone());
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok((blocks, bfree, bavail, _files, _ffree, bsize, _namelen, _frsize))) => {
                Ok(dokan::DiskSpaceInfo {
                    byte_count: blocks.saturating_mul(bsize as u64),
                    free_byte_count: bfree.saturating_mul(bsize as u64),
                    available_byte_count: bavail.saturating_mul(bsize as u64),
                })
            }
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn get_volume_information(
        &'h self, _info: &dokan::OperationInfo<'c, 'h, Self>,
    ) -> dokan::OperationResult<dokan::VolumeInfo> {
        let name =
            U16CString::from_str(self.volume_name.as_str()).map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let fs_name =
            U16CString::from_str(self.fs_name.as_str()).map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        Ok(dokan::VolumeInfo {
            name,
            serial_number: 0xC0FFEE,
            max_component_length: 255,
            fs_flags: 0,
            fs_name,
        })
    }

    fn find_files(
        &'h self, file_name: &widestring::U16CStr,
        mut fill_find_data: impl FnMut(&dokan::FindData) -> dokan::FillDataResult,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyDirectory::default();
        let path_key = file_name.to_string_lossy();
        let req = request_from_ids(context.request_ids);
        let ino = ino_from_context_or_path(context)
            .filter(|v| *v != 0)
            .unwrap_or_else(|| {
                path_to_ino(&mut *fs, &req, OsStr::new(&path_key)).unwrap_or(FUSE_ROOT_ID)
            });
        let fh = context.fh;
        let req_offset = self
            .dir_offsets
            .lock()
            .ok()
            .and_then(|m| m.get(&path_key).copied())
            .unwrap_or(0);
        fs.readdir(&req, ino, fh, req_offset.max(0) as u64, reply.clone());

        let entries = reply
            .entries
            .lock()
            .map_err(|_| STATUS_NOT_IMPLEMENTED)?
            .clone();
        let mut emitted_offset: Option<i64> = None;
        for (entry_ino, entry_offset, kind, name) in &entries {
            let mut perm: Option<u16> = None;
            let attr_reply = ReplyAttr::default();
            fs.getattr(&req, *entry_ino, None, attr_reply.clone());
            if let Ok(Some(Ok(attr))) = attr_reply.status.lock().map(|g| *g) {
                perm = Some(attr.perm);
            }
            let attributes = find_files_attr_from_kind_and_perm(*kind, perm);
            let f = dokan::FindData {
                attributes,
                creation_time: SystemTime::UNIX_EPOCH,
                last_access_time: SystemTime::UNIX_EPOCH,
                last_write_time: SystemTime::UNIX_EPOCH,
                file_size: 0,
                file_name: U16CString::from_str(name.to_string_lossy().as_ref())
                    .map_err(|_| STATUS_NOT_IMPLEMENTED)?,
            };
            if fill_find_data(&f).is_err() {
                break;
            }
            emitted_offset = Some(*entry_offset);
        }
        let next_offset = advance_offset_on_emitted(req_offset, emitted_offset);
        if let Ok(mut m) = self.dir_offsets.lock() {
            m.insert(path_key, next_offset);
        }

        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(())) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn find_files_with_pattern(
        &'h self, file_name: &widestring::U16CStr, pattern: &widestring::U16CStr,
        mut fill_find_data: impl FnMut(&dokan::FindData) -> dokan::FillDataResult,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyDirectoryPlus::default();
        let path_key = file_name.to_string_lossy();
        let req = request_from_ids(context.request_ids);
        let ino = ino_from_context_or_path(context)
            .filter(|v| *v != 0)
            .unwrap_or_else(|| {
                path_to_ino(&mut *fs, &req, OsStr::new(&path_key)).unwrap_or(FUSE_ROOT_ID)
            });
        let fh = context.fh;
        let req_offset = self
            .dir_offsets
            .lock()
            .ok()
            .and_then(|m| m.get(&path_key).copied())
            .unwrap_or(0);
        fs.readdirplus(&req, ino, fh, req_offset.max(0) as u64, reply.clone());

        let entries = reply
            .entries
            .lock()
            .map_err(|_| STATUS_NOT_IMPLEMENTED)?
            .clone();
        let mut emitted_offset: Option<i64> = None;
        for (_entry_ino, entry_offset, name, attr) in &entries {
            let name_u16 = U16CString::from_str(name.to_string_lossy().as_ref())
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            if !dokan::is_name_in_expression(pattern, name_u16.as_ucstr(), true) {
                continue;
            }
            let attributes = filetype_to_windows_attr(attr.kind, attr.perm);
            let f = dokan::FindData {
                attributes,
                creation_time: attr.ctime,
                last_access_time: attr.atime,
                last_write_time: attr.mtime,
                file_size: attr.size,
                file_name: name_u16,
            };
            if fill_find_data(&f).is_err() {
                break;
            }
            emitted_offset = Some(*entry_offset);
        }
        let next_offset = advance_offset_on_emitted(req_offset, emitted_offset);
        if let Ok(mut m) = self.dir_offsets.lock() {
            m.insert(path_key, next_offset);
        }

        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(())) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn cleanup(
        &'h self, file_name: &widestring::U16CStr, info: &dokan::OperationInfo<'c, 'h, Self>,
        context: &'c Self::Context,
    ) {
        // Actual deletion happens here when DeleteOnClose is set
        if info.delete_on_close()
            && let Ok(mut fs) = self.fs.lock()
        {
            let req = request_from_ids(context.request_ids);
            let (parent_path, leaf) = split_parent_and_name(file_name);
            let parent_ino = resolve_parent_ino(&mut *fs, &req, &parent_path);
            let reply = ReplyEmpty::default();
            if context.is_dir {
                rmdir_recursive(&mut *fs, &req, context.ino);
                fs.rmdir(&req, parent_ino, leaf.as_os_str(), reply.clone());
            } else {
                fs.unlink(&req, parent_ino, leaf.as_os_str(), reply.clone());
            }
        }
    }

    fn close_file(
        &'h self, file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
        context: &'c Self::Context,
    ) {
        if let Ok(mut fs) = self.fs.lock() {
            let req = request_from_ids(context.request_ids);
            close_with_context(&mut *fs, &req, *context);
        }
        if let Ok(mut handles) = self.handles.lock() {
            handles.remove(&file_name.to_string_lossy());
        }
        if let Ok(mut offsets) = self.dir_offsets.lock() {
            offsets.remove(&file_name.to_string_lossy());
        }
    }

    fn mounted(
        &'h self, _mount_point: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_info(_info);
        facade_mounted_with(&mut *fs, &req)?;
        Ok(())
    }

    fn unmounted(
        &'h self, _info: &dokan::OperationInfo<'c, 'h, Self>,
    ) -> dokan::OperationResult<()> {
        if let Ok(mut fs) = self.fs.lock() {
            facade_unmounted_with(&mut *fs, &self.destroyed);
        }
        Ok(())
    }

    fn flush_file_buffers(
        &'h self, _file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
        context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyEmpty::default();
        let resolved = resolve_ctx(_file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);
        flush_with_context(&mut *fs, &req, resolved, reply.clone());
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(())) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn create_file(
        &'h self, file_name: &widestring::U16CStr, _security_context: &dokan::IO_SECURITY_CONTEXT,
        desired_access: winapi::um::winnt::ACCESS_MASK, file_attributes: u32, _share_access: u32,
        create_disposition: u32, create_options: u32,
        _info: &mut dokan::OperationInfo<'c, 'h, Self>,
    ) -> dokan::OperationResult<dokan::CreateFileInfo<Self::Context>> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let request_ids = request_ids_from_create_info(_info)
            .unwrap_or_else(|| RequestIds::unavailable(_info.pid()));
        let req = request_from_ids(request_ids);
        let (parent_path, leaf) = split_parent_and_name(file_name);
        let parent_ino = resolve_parent_ino(&mut *fs, &req, &parent_path);
        let path_ino: u64 = if file_name.to_string_lossy() == "\\" {
            FUSE_ROOT_ID
        } else {
            0
        };
        let ino = path_ino;

        // Empty leaf means root path — return existing root context without creating anything.
        if leaf.is_empty() {
            let ctx = AdapterContext {
                fh: 0,
                flags: 0,
                ino: FUSE_ROOT_ID,
                is_dir: true,
                lock_owner: 0,
                request_ids,
            };
            if let Ok(mut handles) = self.handles.lock() {
                handles.insert(file_name.to_string_lossy(), ctx);
            }
            return Ok(dokan::CreateFileInfo {
                context: ctx,
                is_dir: true,
                new_file_created: false,
            });
        }

        let create_plan = create_disposition_plan(create_disposition);
        let is_dir_open = is_directory_open(create_options);

        if matches!(
            create_plan,
            CreateDispositionPlan::CreateOnly
                | CreateDispositionPlan::Supersede
                | CreateDispositionPlan::CreateThenOpenOnExists
        ) {
            let reply = ReplyCreate::default();
            let mode = if (file_attributes & FILE_ATTRIBUTE_READONLY) != 0 {
                0o444_u32
            } else {
                0o644_u32
            };
            let open_flags = access_mask_to_open_flags(desired_access);
            if is_dir_open {
                let entry_reply = ReplyEntry::default();
                fs.mkdir(
                    &req,
                    parent_ino,
                    leaf.as_os_str(),
                    mode,
                    0,
                    entry_reply.clone(),
                );
                let state = *entry_reply
                    .status
                    .lock()
                    .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
                match state {
                    Some(Ok(attr)) => {
                        let ino = if attr.ino != 0 { attr.ino } else { path_ino };
                        let ctx = AdapterContext {
                            fh: 0,
                            flags: 0,
                            ino,
                            is_dir: true,
                            lock_owner: 0,
                            request_ids,
                        };
                        if let Ok(mut handles) = self.handles.lock() {
                            handles.insert(file_name.to_string_lossy(), ctx);
                        }
                        Ok(dokan::CreateFileInfo {
                            context: ctx,
                            is_dir: true,
                            new_file_created: true,
                        })
                    }
                    Some(Err(err)) => Err(errno_to_ntstatus(err)),
                    None => Err(missing_reply_status()),
                }
            } else {
                fs.create(
                    &req,
                    parent_ino,
                    leaf.as_os_str(),
                    mode,
                    0,
                    open_flags,
                    reply.clone(),
                );
                let state = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
                match state {
                    Some(Ok((attr, fh, flags))) => {
                        let ino = if attr.ino != 0 { attr.ino } else { path_ino };
                        let ctx = AdapterContext {
                            fh,
                            flags,
                            ino,
                            is_dir: matches!(attr.kind, FileType::Directory) || is_dir_open,
                            lock_owner: 0,
                            request_ids,
                        };
                        if let Ok(mut handles) = self.handles.lock() {
                            handles.insert(file_name.to_string_lossy(), ctx);
                        }
                        Ok(dokan::CreateFileInfo {
                            context: ctx,
                            is_dir: ctx.is_dir,
                            new_file_created: true,
                        })
                    }
                    Some(Err(err)) => {
                        if matches!(
                            create_plan,
                            CreateDispositionPlan::CreateThenOpenOnExists
                                | CreateDispositionPlan::Supersede
                        ) && err == libc::EEXIST
                        {
                            // Resolve the actual inode of the existing entry
                            let actual_ino = if ino == 0 {
                                let lookup_reply = ReplyEntry::default();
                                fs.lookup(&req, parent_ino, leaf.as_os_str(), lookup_reply.clone());
                                let lookup_result = *lookup_reply
                                    .status
                                    .lock()
                                    .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
                                match lookup_result {
                                    Some(Ok(attr)) => attr.ino,
                                    Some(Err(lookup_err)) => {
                                        return Err(errno_to_ntstatus(lookup_err));
                                    }
                                    None => return Err(missing_reply_status()),
                                }
                            } else {
                                ino
                            };
                            let open_reply = ReplyOpen::default();
                            let open_flags = access_mask_to_open_flags(desired_access);
                            if is_dir_open {
                                fs.opendir(&req, actual_ino, open_flags, open_reply.clone());
                            } else {
                                fs.open(&req, actual_ino, open_flags, open_reply.clone());
                            }
                            let opened = *open_reply
                                .opened
                                .lock()
                                .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
                            match opened {
                                Some(Ok((fh, flags))) => {
                                    let ctx = AdapterContext {
                                        fh,
                                        flags,
                                        ino: actual_ino,
                                        is_dir: is_dir_open,
                                        lock_owner: 0,
                                        request_ids,
                                    };
                                    if let Ok(mut handles) = self.handles.lock() {
                                        handles.insert(file_name.to_string_lossy(), ctx);
                                    }
                                    Ok(dokan::CreateFileInfo {
                                        context: ctx,
                                        is_dir: ctx.is_dir,
                                        new_file_created: false,
                                    })
                                }
                                Some(Err(open_err)) => Err(errno_to_ntstatus(open_err)),
                                None => Err(missing_reply_status()),
                            }
                        } else {
                            Err(errno_to_ntstatus(err))
                        }
                    }
                    None => Err(missing_reply_status()),
                }
            }
        } else {
            // OpenOnly: resolve the real inode before calling fs.open/fs.opendir
            let actual_ino = if ino == 0 {
                let lookup_reply = ReplyEntry::default();
                fs.lookup(&req, parent_ino, leaf.as_os_str(), lookup_reply.clone());
                let lookup_result = *lookup_reply
                    .status
                    .lock()
                    .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
                match lookup_result {
                    Some(Ok(attr)) => attr.ino,
                    Some(Err(err)) => return Err(errno_to_ntstatus(err)),
                    None => return Err(missing_reply_status()),
                }
            } else {
                ino
            };
            let reply = ReplyOpen::default();
            let open_flags = access_mask_to_open_flags(desired_access);
            if is_dir_open {
                fs.opendir(&req, actual_ino, open_flags, reply.clone());
            } else {
                fs.open(&req, actual_ino, open_flags, reply.clone());
            }
            let opened = *reply.opened.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            match opened {
                Some(Ok((fh, flags))) => {
                    let existing_ino = self
                        .handles
                        .lock()
                        .ok()
                        .and_then(|handles| handles.get(&file_name.to_string_lossy()).copied())
                        .map(|ctx| ctx.ino)
                        .filter(|v| *v != 0);
                    let ctx = AdapterContext {
                        fh,
                        flags,
                        ino: existing_ino.unwrap_or(actual_ino),
                        is_dir: is_dir_open,
                        lock_owner: 0,
                        request_ids,
                    };
                    if let Ok(mut handles) = self.handles.lock() {
                        handles.insert(file_name.to_string_lossy(), ctx);
                    }
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
    }

    fn read_file(
        &'h self, _file_name: &widestring::U16CStr, offset: i64, buffer: &mut [u8],
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<u32> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyData::default();
        let resolved = resolve_ctx(_file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);
        fs.read(
            &req,
            resolved.ino,
            resolved.fh,
            offset.max(0) as u64,
            buffer.len() as u32,
            resolved.flags as i32,
            lock_owner_from_context(resolved),
            reply.clone(),
        );
        match reply
            .data
            .lock()
            .map_err(|_| STATUS_NOT_IMPLEMENTED)?
            .clone()
        {
            Some(Ok(data)) => {
                let n = data.len().min(buffer.len());
                buffer[..n].copy_from_slice(&data[..n]);
                Ok(n as u32)
            }
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn write_file(
        &'h self, _file_name: &widestring::U16CStr, offset: i64, buffer: &[u8],
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<u32> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyWrite::default();
        let resolved = resolve_ctx(_file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);
        fs.write(
            &req,
            resolved.ino,
            resolved.fh,
            offset.max(0) as u64,
            buffer,
            0,
            resolved.flags as i32,
            lock_owner_from_context(resolved),
            reply.clone(),
        );
        match *reply.written.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(n)) => Ok(n),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn get_file_information(
        &'h self, file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
        _context: &'c Self::Context,
    ) -> dokan::OperationResult<dokan::FileInfo> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyAttr::default();
        let resolved = resolve_ctx(file_name, _context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(_context.request_ids);
        fs.getattr(&req, resolved.ino, Some(resolved.fh), reply.clone());
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(attr)) => Ok(dokan::FileInfo {
                attributes: filetype_to_windows_attr(attr.kind, attr.perm),
                creation_time: attr.ctime,
                last_access_time: attr.atime,
                last_write_time: attr.mtime,
                file_size: attr.size,
                number_of_links: attr.nlink,
                file_index: attr.ino,
            }),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn get_file_security(
        &'h self, _file_name: &widestring::U16CStr, _security_information: u32,
        _security_descriptor: winapi::um::winnt::PSECURITY_DESCRIPTOR, _buffer_length: u32,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<u32> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyEmpty::default();
        let resolved = resolve_ctx(_file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);
        fs.access(&req, resolved.ino, 0, reply.clone());
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(())) => Ok(0),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn lock_file(
        &'h self, _file_name: &widestring::U16CStr, offset: i64, length: i64,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyEmpty::default();
        let resolved = resolve_ctx(_file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);

        let query = ReplyLock::default();
        fs.getlk(
            &req,
            resolved.ino,
            resolved.fh,
            lock_owner_from_context(resolved).unwrap_or(0),
            offset as u64,
            (offset + length).max(0) as u64,
            LOCK_TYPE_WRLCK,
            req.pid(),
            query.clone(),
        );

        match *query.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Err(err)) => return Err(errno_to_ntstatus(err)),
            Some(Ok((_start, _end, typ, _pid))) if typ != LOCK_TYPE_UNLCK => {
                return Err(errno_to_ntstatus(libc::EAGAIN));
            }
            _ => {}
        }

        fs.setlk(
            &req,
            resolved.ino,
            resolved.fh,
            lock_owner_from_context(resolved).unwrap_or(0),
            offset as u64,
            (offset + length).max(0) as u64,
            LOCK_TYPE_WRLCK,
            req.pid(),
            false,
            reply.clone(),
        );
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(())) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn unlock_file(
        &'h self, _file_name: &widestring::U16CStr, offset: i64, length: i64,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyEmpty::default();
        let resolved = resolve_ctx(_file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);
        fs.setlk(
            &req,
            resolved.ino,
            resolved.fh,
            lock_owner_from_context(resolved).unwrap_or(0),
            offset as u64,
            (offset + length).max(0) as u64,
            LOCK_TYPE_UNLCK,
            req.pid(),
            false,
            reply.clone(),
        );
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(())) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }

    fn find_streams(
        &'h self, _file_name: &widestring::U16CStr,
        mut _fill_find_stream_data: impl FnMut(&dokan::FindStreamData) -> dokan::FillDataResult,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let mut fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyXattr::default();
        let resolved = resolve_ctx(_file_name, context).ok_or(STATUS_NOT_IMPLEMENTED)?;
        let req = request_from_ids(context.request_ids);
        fs.listxattr(&req, resolved.ino, 0, reply.clone());
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(_)) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(missing_reply_status()),
        }
    }
}
