macro_rules! handler_delete_move {
    () => {
        fn delete_file(
            &'h self, file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
            context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, file_name, context)
                .map_err(errno_to_ntstatus)?;
            let result = precheck_file_delete(&*fs, &req, resolved.ino);
            self.drain_resolver_forgets(&*fs, &req);
            result
        }

        fn delete_directory(
            &'h self, file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
            context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, file_name, context)
                .map_err(errno_to_ntstatus)?;
            let result = precheck_directory_delete(&*fs, &req, resolved.ino);
            self.drain_resolver_forgets(&*fs, &req);
            result
        }

        fn move_file(
            &'h self, file_name: &widestring::U16CStr, new_file_name: &widestring::U16CStr,
            replace_if_existing: bool, _info: &dokan::OperationInfo<'c, 'h, Self>,
            _context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let (old_parent_path, old_leaf) = split_parent_and_name(file_name);
            let (new_parent_path, new_leaf) = split_parent_and_name(new_file_name);
            let req = request_from_ids(_context.request_ids);
            let old_parent_ino = self
                .resolve_parent_ino(&*fs, &req, &old_parent_path)
                .map_err(errno_to_ntstatus)?;
            let new_parent_ino = self
                .resolve_parent_ino(&*fs, &req, &new_parent_path)
                .map_err(errno_to_ntstatus)?;
            let moved_ino = self
                .resolve_context_or_path(&*fs, &req, file_name, _context)
                .ok()
                .map(|resolved| resolved.ino);
            if !replace_if_existing {
                match self.resolve_path_ino(
                    &*fs,
                    &req,
                    OsStr::new(&new_file_name.to_string_lossy()),
                ) {
                    Ok(_) => {
                        self.drain_resolver_forgets(&*fs, &req);
                        return Err(STATUS_OBJECT_NAME_COLLISION);
                    }
                    Err(err) if err == libc::ENOENT => {
                        self.drain_resolver_forgets(&*fs, &req);
                    }
                    Err(err) => {
                        self.drain_resolver_forgets(&*fs, &req);
                        return Err(errno_to_ntstatus(err));
                    }
                }
            }
            let rename_result = rename_with_replace_policy(
                &*fs,
                &req,
                old_parent_ino,
                old_leaf.as_os_str(),
                new_parent_ino,
                new_leaf.as_os_str(),
            );
            self.drain_resolver_forgets(&*fs, &req);
            match rename_result {
                Ok(()) => {
                    if let Some(ino) = moved_ino {
                        self.invalidate_inode_attr(ino);
                    }
                    self.remap_open_state_after_rename(file_name, new_file_name);
                    self.invalidate_path_cache(&*fs, &req, file_name);
                    self.invalidate_path_cache(&*fs, &req, new_file_name);
                    self.invalidate_inode_attr(old_parent_ino);
                    self.invalidate_inode_attr(new_parent_ino);
                    Ok(())
                }
                Err(status) => Err(status),
            }
        }
    };
}

macro_rules! handler_directory_find {
    () => {
        fn find_files(
            &'h self, file_name: &widestring::U16CStr,
            mut fill_find_data: impl FnMut(&dokan::FindData) -> dokan::FillDataResult,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyDirectory::capture();
            let path_key = file_name.to_string_lossy();
            let req = request_from_ids(context.request_ids);
            let ino = self.directory_ino(&*fs, &req, &path_key, context)?;
            let fh = context.fh;
            let req_offset = self.directory_offset(&path_key);
            fs.readdir(&req, ino, fh, req_offset.max(0) as u64, reply.duplicate());
            self.drain_resolver_forgets(&*fs, &req);

            let entries = reply
                .entries
                .lock()
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?
                .clone();
            let emitted_offset =
                self.emit_readdir_entries(&*fs, &req, &entries, &mut fill_find_data)?;
            self.remember_directory_offset(path_key, req_offset, emitted_offset);

            self.reply_directory_status(&reply)
        }
    };
}

macro_rules! handler_directory_find_pattern {
    () => {
    fn find_files_with_pattern(
        &'h self, file_name: &widestring::U16CStr, pattern: &widestring::U16CStr,
        mut fill_find_data: impl FnMut(&dokan::FindData) -> dokan::FillDataResult,
        _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
    ) -> dokan::OperationResult<()> {
        let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let reply = ReplyDirectoryPlus::capture();
        let path_key = file_name.to_string_lossy();
        let req = request_from_ids(context.request_ids);
        let ino = self.directory_ino(&*fs, &req, &path_key, context)?;
        let fh = context.fh;
        let req_offset = self.directory_offset(&path_key);
        fs.readdirplus(&req, ino, fh, req_offset.max(0) as u64, reply.duplicate());
        self.drain_resolver_forgets(&*fs, &req);

        if matches!(*reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?, Some(Err(err)) if err == libc::ENOSYS)
        {
            let fallback = ReplyDirectory::capture();
            fs.readdir(
                &req,
                ino,
                fh,
                req_offset.max(0) as u64,
                fallback.duplicate(),
            );
            self.drain_resolver_forgets(&*fs, &req);
            let entries = fallback
                .entries
                .lock()
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?
                .clone();
            let emitted_offset = self.emit_readdir_pattern_entries(
                &*fs,
                &req,
                pattern,
                &entries,
                &mut fill_find_data,
            )?;
            self.remember_directory_offset(path_key, req_offset, emitted_offset);
            return self.reply_directory_status(&fallback);
        }

        let entries = reply
            .entries
            .lock()
            .map_err(|_| STATUS_NOT_IMPLEMENTED)?
            .clone();
        let emitted_offset = self.emit_readdirplus_pattern_entries(
            &*fs,
            &req,
            (file_name, pattern, ino),
            &entries,
            &mut fill_find_data,
        )?;
        self.remember_directory_offset(path_key, req_offset, emitted_offset);

        self.reply_directoryplus_status(&reply)
    }
    };
}

macro_rules! handler_directory_cleanup {
    () => {
        fn cleanup(
            &'h self, file_name: &widestring::U16CStr, info: &dokan::OperationInfo<'c, 'h, Self>,
            context: &'c Self::Context,
        ) {
            if info.delete_on_close() {
                self.cleanup_delete_on_close(file_name, context);
            }
        }
    };
}

macro_rules! handler_lifecycle_create {
    () => {
        fn close_file(
            &'h self, file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
            context: &'c Self::Context,
        ) {
            if let Ok(fs) = self.fs.lock() {
                let req = request_from_ids(context.request_ids);
                close_with_context(&*fs, &req, *context);
            }
            if let Ok(mut handles) = self.handles.lock() {
                handles.remove(&file_name.to_string_lossy());
            }
            if let Ok(mut offsets) = self.dir_offsets.lock() {
                offsets.remove(&file_name.to_string_lossy());
            }
        }

        fn mounted(
            &'h self, _mount_point: &widestring::U16CStr,
            _info: &dokan::OperationInfo<'c, 'h, Self>,
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
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyEmpty::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, _file_name, context)
                .map_err(errno_to_ntstatus)?;
            flush_with_context(&*fs, &req, resolved, reply.duplicate());
            let status = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match status {
                Some(Ok(())) => Ok(()),
                Some(Err(err)) => Err(errno_to_ntstatus(err)),
                None => Err(missing_reply_status()),
            }
        }

        fn create_file(
            &'h self, file_name: &widestring::U16CStr,
            _security_context: &dokan::IO_SECURITY_CONTEXT,
            desired_access: winapi::um::winnt::ACCESS_MASK, file_attributes: u32,
            _share_access: u32, create_disposition: u32, create_options: u32,
            _info: &mut dokan::OperationInfo<'c, 'h, Self>,
        ) -> dokan::OperationResult<dokan::CreateFileInfo<Self::Context>> {
            let request_ids = request_ids_from_create_info(_info)
                .unwrap_or_else(|| RequestIds::unavailable(_info.pid()));
            self.create_file_impl(
                file_name,
                desired_access,
                file_attributes,
                create_disposition,
                create_options,
                request_ids,
            )
        }
    };
}
