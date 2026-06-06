macro_rules! handler_security {
    () => {
        fn get_file_security(
            &'h self, _file_name: &widestring::U16CStr, _security_information: u32,
            security_descriptor: winapi::um::winnt::PSECURITY_DESCRIPTOR, buffer_length: u32,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<u32> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, _file_name, context)
                .map_err(errno_to_ntstatus)?;
            let reply = ReplyXattr::capture();
            fs.getxattr(
                &req,
                resolved.ino,
                OsStr::new(SECURITY_DESCRIPTOR_XATTR),
                0,
                reply.duplicate(),
            );
            let data = reply
                .status
                .lock()
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?
                .clone();
            let size_hint = *reply.size_hint.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match data {
                Some(Ok(data)) => self.copy_existing_or_fetched_security_descriptor(
                    &*fs,
                    &req,
                    resolved,
                    SecurityDescriptorCopy {
                        data,
                        size_hint,
                        security_descriptor,
                        buffer_length,
                    },
                ),
                Some(Err(err)) if err == Errno::NO_XATTR.raw_os_error() => {
                    let data = synthesized_security_descriptor_from_fs(
                        &*fs,
                        &req,
                        resolved.ino,
                        resolved.fh,
                    )?;
                    copy_security_descriptor(&data, security_descriptor, buffer_length)
                }
                Some(Err(err)) => Err(classify_optional_probe_error(err).ntstatus()),
                None => Err(missing_reply_status()),
            }
        }

        fn set_file_security(
            &'h self, _file_name: &widestring::U16CStr, _security_information: u32,
            security_descriptor: winapi::um::winnt::PSECURITY_DESCRIPTOR, buffer_length: u32,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, _file_name, context)
                .map_err(errno_to_ntstatus)?;
            let descriptor = if security_descriptor.is_null() {
                &[]
            } else {
                unsafe {
                    std::slice::from_raw_parts(
                        security_descriptor.cast::<u8>(),
                        buffer_length as usize,
                    )
                }
            };
            let reply = ReplyEmpty::capture();
            fs.setxattr(
                &req,
                resolved.ino,
                OsStr::new(SECURITY_DESCRIPTOR_XATTR),
                descriptor,
                0,
                0,
                reply.duplicate(),
            );
            let status = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match status {
                Some(Ok(())) => {
                    self.invalidate_inode_attr(resolved.ino);
                    Ok(())
                }
                Some(Err(err)) => Err(classify_optional_probe_error(err).ntstatus()),
                None => Err(missing_reply_status()),
            }
        }
    };
}

macro_rules! handler_locks {
    () => {
        fn lock_file(
            &'h self, _file_name: &widestring::U16CStr, offset: i64, length: i64,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyEmpty::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, _file_name, context)
                .map_err(errno_to_ntstatus)?;

            let (start, end) = checked_lock_range(offset, length)?;
            let query = ReplyLock::capture();
            fs.getlk(
                &req,
                resolved.ino,
                resolved.fh,
                lock_owner_from_context(resolved).unwrap_or(lock_owner(0)),
                start,
                end,
                LOCK_TYPE_WRLCK,
                req.pid(),
                query.duplicate(),
            );

            match *query.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
                Some(Err(err)) => return Err(classify_optional_probe_error(err).ntstatus()),
                Some(Ok((_start, _end, typ, _pid))) if typ != LOCK_TYPE_UNLCK => {
                    return Err(errno_to_ntstatus(libc::EAGAIN));
                }
                _ => {}
            }

            fs.setlk(
                &req,
                resolved.ino,
                resolved.fh,
                lock_owner_from_context(resolved).unwrap_or(lock_owner(0)),
                start,
                end,
                LOCK_TYPE_WRLCK,
                req.pid(),
                false,
                reply.duplicate(),
            );
            let status = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match status {
                Some(Ok(())) => Ok(()),
                Some(Err(err)) => Err(classify_optional_probe_error(err).ntstatus()),
                None => Err(missing_reply_status()),
            }
        }

        fn unlock_file(
            &'h self, _file_name: &widestring::U16CStr, offset: i64, length: i64,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyEmpty::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, _file_name, context)
                .map_err(errno_to_ntstatus)?;
            let (start, end) = checked_lock_range(offset, length)?;
            fs.setlk(
                &req,
                resolved.ino,
                resolved.fh,
                lock_owner_from_context(resolved).unwrap_or(lock_owner(0)),
                start,
                end,
                LOCK_TYPE_UNLCK,
                req.pid(),
                false,
                reply.duplicate(),
            );
            let status = *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            self.drain_resolver_forgets(&*fs, &req);
            match status {
                Some(Ok(())) => Ok(()),
                Some(Err(err)) => Err(classify_optional_probe_error(err).ntstatus()),
                None => Err(missing_reply_status()),
            }
        }
    };
}

macro_rules! handler_streams {
    () => {
        fn find_streams(
            &'h self, _file_name: &widestring::U16CStr,
            mut _fill_find_stream_data: impl FnMut(&dokan::FindStreamData) -> dokan::FillDataResult,
            _info: &dokan::OperationInfo<'c, 'h, Self>, context: &'c Self::Context,
        ) -> dokan::OperationResult<()> {
            let fs = self.fs.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let reply = ReplyXattr::capture();
            let req = request_from_ids(context.request_ids);
            let resolved = self
                .resolve_context_or_path(&*fs, &req, _file_name, context)
                .map_err(errno_to_ntstatus)?;
            let attr = ReplyAttr::capture();
            fs.getattr(&req, resolved.ino, Some(resolved.fh), attr.duplicate());
            let default_size = match *attr.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
                Some(Ok(attr)) => i64::try_from(attr.size).unwrap_or(i64::MAX),
                Some(Err(_)) => 0,
                None => 0,
            };
            let default = U16CString::from_str("::$DATA").map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            if _fill_find_stream_data(&dokan::FindStreamData {
                size: default_size,
                name: default,
            })
            .is_err()
            {
                return Ok(());
            }

            fs.listxattr(&req, resolved.ino, 0, reply.duplicate());
            let list_data = self.fetch_stream_xattr_list(&*fs, &req, resolved.ino, reply)?;
            self.drain_resolver_forgets(&*fs, &req);
            match list_data {
                Some(Ok(data)) => self.emit_named_streams(
                    &*fs,
                    &req,
                    resolved.ino,
                    data,
                    &mut _fill_find_stream_data,
                ),
                Some(Err(err)) if err == libc::ENOSYS => Ok(()),
                Some(Err(err)) => Err(classify_optional_probe_error(err).ntstatus()),
                None => Err(missing_reply_status()),
            }
        }
    };
}

use std::ffi::OsStr;

use widestring::U16CString;
use winapi::shared::ntstatus::STATUS_NOT_IMPLEMENTED;

use super::super::{AdapterContext, DokanAdapter};
use crate::dokan_impl::*;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::*;
use crate::fuser_facade::request::Request;
use crate::fuser_facade::types::{Errno, INodeNo};

pub(crate) struct SecurityDescriptorCopy {
    pub(crate) data: Vec<u8>,
    pub(crate) size_hint: Option<u32>,
    pub(crate) security_descriptor: winapi::um::winnt::PSECURITY_DESCRIPTOR,
    pub(crate) buffer_length: u32,
}

impl<FS: Filesystem> DokanAdapter<FS> {
    pub(super) fn fetch_stream_xattr_list(
        &self, fs: &FS, req: &Request, ino: INodeNo, reply: ReplyXattr,
    ) -> dokan::OperationResult<Option<Result<Vec<u8>, i32>>> {
        let list_data = reply
            .status
            .lock()
            .map_err(|_| STATUS_NOT_IMPLEMENTED)?
            .clone();
        let Some(Ok(ref data)) = list_data else {
            return Ok(list_data);
        };
        let size_hint = *reply.size_hint.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)?;
        let Some(fetch_size) = xattr_needs_data_fetch(data, size_hint) else {
            return Ok(list_data);
        };
        let fetch = ReplyXattr::capture();
        fs.listxattr(req, ino, fetch_size, fetch.duplicate());
        fetch
            .status
            .lock()
            .map_err(|_| STATUS_NOT_IMPLEMENTED)
            .map(|status| status.clone())
    }

    pub(super) fn emit_named_streams(
        &self, fs: &FS, req: &Request, ino: INodeNo, data: Vec<u8>,
        fill_find_stream_data: &mut impl FnMut(&dokan::FindStreamData) -> dokan::FillDataResult,
    ) -> dokan::OperationResult<()> {
        for raw_name in data.split(|b| *b == 0).filter(|part| !part.is_empty()) {
            let Some(stream_name) = stream_name_from_xattr(raw_name) else {
                continue;
            };
            let size_reply = ReplyXattr::capture();
            fs.getxattr(
                req,
                ino,
                OsStr::new(std::str::from_utf8(raw_name).unwrap_or_default()),
                0,
                size_reply.duplicate(),
            );
            let size = *size_reply
                .size_hint
                .lock()
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            let name = U16CString::from_str(stream_name).map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            if fill_find_stream_data(&dokan::FindStreamData {
                size: size.map(i64::from).unwrap_or(0),
                name,
            })
            .is_err()
            {
                break;
            }
        }
        Ok(())
    }

    pub(super) fn copy_existing_or_fetched_security_descriptor(
        &self, fs: &FS, req: &Request, resolved: AdapterContext, copy: SecurityDescriptorCopy,
    ) -> dokan::OperationResult<u32> {
        let needed = xattr_reported_len(copy.data.len(), copy.size_hint)?;
        if copy.buffer_length < needed {
            return Ok(needed);
        }
        let data = self.fetch_security_descriptor_data(fs, req, resolved, &copy)?;
        copy_security_descriptor(&data, copy.security_descriptor, copy.buffer_length)?;
        Ok(needed)
    }

    fn fetch_security_descriptor_data(
        &self, fs: &FS, req: &Request, resolved: AdapterContext, copy: &SecurityDescriptorCopy,
    ) -> dokan::OperationResult<Vec<u8>> {
        let Some(fetch_size) = xattr_needs_data_fetch(&copy.data, copy.size_hint) else {
            return Ok(copy.data.clone());
        };
        let fetch = ReplyXattr::capture();
        fs.getxattr(
            req,
            resolved.ino,
            OsStr::new(SECURITY_DESCRIPTOR_XATTR),
            fetch_size.max(copy.buffer_length),
            fetch.duplicate(),
        );
        match fetch
            .status
            .lock()
            .map_err(|_| STATUS_NOT_IMPLEMENTED)?
            .clone()
        {
            Some(Ok(data)) => Ok(data),
            Some(Err(err)) if err == Errno::NO_XATTR.raw_os_error() => {
                synthesized_security_descriptor_from_fs(fs, req, resolved.ino, resolved.fh)
            }
            Some(Err(err)) => Err(classify_optional_probe_error(err).ntstatus()),
            None => Err(missing_reply_status()),
        }
    }
}
