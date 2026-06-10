macro_rules! handler_metadata {
    () => {
        fn set_end_of_file(
            &'h self, file_name: &widestring::U16CStr, offset: i64,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyAttr::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, file_name, context)
                .map_err(errno_to_ntstatus)?;
            fs.setattr(
                &req,
                resolved.ino,
                None,
                None,
                None,
                Some(nonnegative_i64_to_u64(offset)?),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
                reply.duplicate(),
            );
            let status = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match status {
                Some(Ok(_)) => {
                    self.finish_metadata_mutation(&*fs, &req, file_name, resolved.ino);
                    Ok(())
                }
                Some(Err(err)) => Err(errno_to_ntstatus(err)),
                None => Err(missing_reply_status()),
            }
        }

        fn set_allocation_size(
            &'h self, file_name: &widestring::U16CStr, alloc_size: i64,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyEmpty::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, file_name, context)
                .map_err(errno_to_ntstatus)?;
            allocation_size_with_context(&*fs, &req, resolved, alloc_size, reply.duplicate())?;
            let status = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match status {
                Some(Ok(())) => {
                    self.finish_metadata_mutation(&*fs, &req, file_name, resolved.ino);
                    Ok(())
                }
                Some(Err(err)) => Err(classify_optional_probe_error(err).ntstatus()),
                None => Err(missing_reply_status()),
            }
        }

        fn set_file_attributes(
            &'h self, file_name: &widestring::U16CStr, file_attributes: u32,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyAttr::capture();
            let mode = if (file_attributes & FILE_ATTRIBUTE_READONLY) != 0 {
                Some(0o444_u32)
            } else {
                Some(0o644_u32)
            };
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, file_name, context)
                .map_err(errno_to_ntstatus)?;
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
                reply.duplicate(),
            );
            let status = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match status {
                Some(Ok(_)) => {
                    self.finish_metadata_mutation(&*fs, &req, file_name, resolved.ino);
                    Ok(())
                }
                Some(Err(err)) => Err(errno_to_ntstatus(err)),
                None => Err(missing_reply_status()),
            }
        }

        fn set_file_time(
            &'h self, file_name: &widestring::U16CStr, creation_time: dokan::FileTimeOperation,
            last_access_time: dokan::FileTimeOperation, last_write_time: dokan::FileTimeOperation,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyAttr::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, file_name, context)
                .map_err(errno_to_ntstatus)?;
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
                reply.duplicate(),
            );
            let status = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match status {
                Some(Ok(_)) => {
                    self.finish_metadata_mutation(&*fs, &req, file_name, resolved.ino);
                    Ok(())
                }
                Some(Err(err)) => Err(errno_to_ntstatus(err)),
                None => Err(missing_reply_status()),
            }
        }

        fn get_disk_free_space(
            &'h self, _info: &dokan::OperationInfo<'c, 'h, Self>,
        ) -> dokan::OperationResult<dokan::DiskSpaceInfo> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyStatfs::capture();
            let req = request_from_info(_info);
            fs.statfs(&req, INodeNo::ROOT, reply.duplicate());
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
            let name = U16CString::from_str(self.volume_name.as_str())
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let fs_name =
                U16CString::from_str(self.fs_name.as_str()).map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            Ok(dokan::VolumeInfo {
                name,
                serial_number: 0xC0FFEE,
                max_component_length: 255,
                fs_flags: self.volume_flags,
                fs_name,
            })
        }
    };
}

macro_rules! handler_io_info {
    () => {
        fn read_file(
            &'h self, _file_name: &widestring::U16CStr, offset: i64, buffer: &mut [u8],
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<u32> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyData::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, _file_name, context)
                .map_err(errno_to_ntstatus)?;
            fs.read(
                &req,
                resolved.ino,
                resolved.fh,
                nonnegative_i64_to_u64(offset)?,
                checked_dokan_len(buffer.len())?,
                open_flags_from_fopen_flags(resolved.flags),
                lock_owner_from_context(resolved),
                reply.duplicate(),
            );
            let data_result = reply
                .data
                .lock()
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?
                .clone();
            self.drain_resolver_forgets(&*fs, &req);
            match data_result {
                Some(Ok(data)) => {
                    let n = data.len().min(buffer.len());
                    buffer[..n].copy_from_slice(&data[..n]);
                    Ok(checked_dokan_len(n)?)
                }
                Some(Err(err)) => Err(errno_to_ntstatus(err)),
                None => Err(missing_reply_status()),
            }
        }

        fn write_file(
            &'h self, _file_name: &widestring::U16CStr, offset: i64, buffer: &[u8],
            info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<u32> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyWrite::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, _file_name, context)
                .map_err(errno_to_ntstatus)?;
            let (offset, len) = dokan_write_plan(
                &*fs,
                &req,
                resolved,
                offset,
                buffer.len(),
                info.write_to_eof(),
                info.paging_io(),
            )?;
            let data = &buffer[..len];
            fs.write(
                &req,
                resolved.ino,
                resolved.fh,
                offset,
                data,
                write_flags(0),
                open_flags_from_fopen_flags(resolved.flags),
                lock_owner_from_context(resolved),
                reply.duplicate(),
            );
            let written = *reply.written.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match written {
                Some(Ok(n)) => {
                    self.finish_metadata_mutation(&*fs, &req, _file_name, resolved.ino);
                    Ok(n)
                }
                Some(Err(err)) => Err(errno_to_ntstatus(err)),
                None => Err(missing_reply_status()),
            }
        }

        fn get_file_information(
            &'h self, file_name: &widestring::U16CStr, _info: &dokan::OperationInfo<'c, 'h, Self>,
            _context: &'c Self::Context,
        ) -> dokan::OperationResult<dokan::FileInfo> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyAttr::capture();
            let req = request_from_ids(_context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, file_name, _context)
                .map_err(errno_to_ntstatus)?;
            let cached_attr = {
                self.resolver
                    .lock()
                    .map_err(|_| STATUS_NOT_IMPLEMENTED)?
                    .cached_attr(resolved.ino)
            };
            if let Some(attr) = cached_attr {
                self.drain_resolver_forgets(&*fs, &req);
                return Ok(dokan::FileInfo {
                    attributes: filetype_to_windows_attr(attr.kind, attr.perm),
                    creation_time: attr.ctime,
                    last_access_time: attr.atime,
                    last_write_time: attr.mtime,
                    file_size: attr.size,
                    number_of_links: attr.nlink,
                    file_index: attr.ino.0,
                });
            }
            fs.getattr(&req, resolved.ino, Some(resolved.fh), reply.duplicate());
            let payload = *reply.payload.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match payload {
                Some(Ok(payload)) => {
                    self.resolver
                        .lock()
                        .map_err(|_| STATUS_NOT_IMPLEMENTED)?
                        .remember_attr(payload.attr.ino, payload.attr, payload.ttl);
                    Ok(dokan::FileInfo {
                        attributes: filetype_to_windows_attr(payload.attr.kind, payload.attr.perm),
                        creation_time: payload.attr.ctime,
                        last_access_time: payload.attr.atime,
                        last_write_time: payload.attr.mtime,
                        file_size: payload.attr.size,
                        number_of_links: payload.attr.nlink,
                        file_index: payload.attr.ino.0,
                    })
                }
                Some(Err(err)) => Err(errno_to_ntstatus(err)),
                None => Err(missing_reply_status()),
            }
        }
    };
}
