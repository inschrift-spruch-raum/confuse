use std::ffi::OsStr;
use std::time::SystemTime;

use widestring::{U16CStr, U16CString};
use winapi::shared::ntstatus::*;

use super::{AdapterContext, CreatedPath, DokanAdapter};
use crate::dokan_impl::{
    advance_offset_on_emitted, errno_to_ntstatus, filetype_to_windows_attr,
    find_files_attr_from_kind_and_perm, ino, ino_from_context_or_path, join_child_path,
};
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::{
    DirectoryEntryPayload, DirectoryPlusEntryPayload, ReplyAttr, ReplyDirectory, ReplyDirectoryPlus,
};
use crate::fuser_facade::request::Request;
use crate::fuser_facade::types::{FileAttr, INodeNo};

impl<FS: Filesystem> DokanAdapter<FS> {
    pub(super) fn directory_ino(
        &self, fs: &FS, req: &Request, path_key: &str, context: &AdapterContext,
    ) -> dokan::OperationResult<INodeNo> {
        match ino_from_context_or_path(context).filter(|v| *v != ino(0)) {
            Some(ino) => Ok(ino),
            None => self
                .resolve_path_ino(fs, req, OsStr::new(path_key))
                .map_err(errno_to_ntstatus),
        }
    }

    pub(super) fn directory_offset(&self, path_key: &str) -> i64 {
        self.dir_offsets
            .lock()
            .ok()
            .and_then(|m| m.get(path_key).copied())
            .unwrap_or(0)
    }

    pub(super) fn remember_directory_offset(
        &self, path_key: String, req_offset: i64, emitted_offset: Option<i64>,
    ) {
        let next_offset = advance_offset_on_emitted(req_offset, emitted_offset);
        if let Ok(mut m) = self.dir_offsets.lock() {
            m.insert(path_key, next_offset);
        }
    }

    pub(super) fn emit_readdir_entries(
        &self, fs: &FS, req: &Request, entries: &[DirectoryEntryPayload],
        fill_find_data: &mut impl FnMut(&dokan::FindData) -> dokan::FillDataResult,
    ) -> dokan::OperationResult<Option<i64>> {
        let mut emitted_offset = None;
        for entry in entries {
            let perm = self
                .readdir_entry_attr(fs, req, entry.ino)
                .map(|attr| attr.perm);
            let find_data = dokan::FindData {
                attributes: find_files_attr_from_kind_and_perm(entry.kind, perm),
                creation_time: SystemTime::UNIX_EPOCH,
                last_access_time: SystemTime::UNIX_EPOCH,
                last_write_time: SystemTime::UNIX_EPOCH,
                file_size: 0,
                file_name: U16CString::from_str(entry.name.to_string_lossy().as_ref())
                    .map_err(|_| STATUS_NOT_IMPLEMENTED)?,
            };
            if fill_find_data(&find_data).is_err() {
                break;
            }
            emitted_offset = Some(entry.offset as i64);
        }
        Ok(emitted_offset)
    }

    pub(super) fn emit_readdir_pattern_entries(
        &self, fs: &FS, req: &Request, pattern: &U16CStr, entries: &[DirectoryEntryPayload],
        fill_find_data: &mut impl FnMut(&dokan::FindData) -> dokan::FillDataResult,
    ) -> dokan::OperationResult<Option<i64>> {
        let mut emitted_offset = None;
        for entry in entries {
            let name_u16 = U16CString::from_str(entry.name.to_string_lossy().as_ref())
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            if !dokan::is_name_in_expression(pattern, name_u16.as_ucstr(), true) {
                continue;
            }
            let attr = self.readdir_entry_attr(fs, req, entry.ino);
            let size = attr.map_or(0, |attr| attr.size);
            let times = attr.map_or(
                (
                    SystemTime::UNIX_EPOCH,
                    SystemTime::UNIX_EPOCH,
                    SystemTime::UNIX_EPOCH,
                ),
                |attr| (attr.ctime, attr.atime, attr.mtime),
            );
            let perm = attr.map(|attr| attr.perm);
            let find_data = dokan::FindData {
                attributes: find_files_attr_from_kind_and_perm(entry.kind, perm),
                creation_time: times.0,
                last_access_time: times.1,
                last_write_time: times.2,
                file_size: size,
                file_name: name_u16,
            };
            if fill_find_data(&find_data).is_err() {
                break;
            }
            emitted_offset = Some(entry.offset as i64);
        }
        Ok(emitted_offset)
    }

    fn readdir_entry_attr(&self, fs: &FS, req: &Request, ino: INodeNo) -> Option<FileAttr> {
        let attr_reply = ReplyAttr::capture();
        fs.getattr(req, ino, None, attr_reply.duplicate());
        match *attr_reply.status.lock().ok()? {
            Some(Ok(attr)) => Some(attr),
            Some(Err(_)) | None => None,
        }
    }

    pub(super) fn emit_readdirplus_pattern_entries(
        &self, fs: &FS, req: &Request, route: (&U16CStr, &U16CStr, INodeNo),
        entries: &[DirectoryPlusEntryPayload],
        fill_find_data: &mut impl FnMut(&dokan::FindData) -> dokan::FillDataResult,
    ) -> dokan::OperationResult<Option<i64>> {
        let (file_name, pattern, ino) = route;
        let mut emitted_offset = None;
        for entry in entries {
            let attr = entry.attr;
            self.remember_created_path(
                fs,
                req,
                CreatedPath {
                    path: U16CString::from_str(join_child_path(file_name, &entry.name))
                        .map_err(|_| STATUS_NOT_IMPLEMENTED)?
                        .as_ucstr(),
                    parent: ino,
                    name: &entry.name,
                    attr,
                    generation: entry.generation,
                    ttl: entry.ttl,
                },
            );
            let name_u16 = U16CString::from_str(entry.name.to_string_lossy().as_ref())
                .map_err(|_| STATUS_NOT_IMPLEMENTED)?;
            if !dokan::is_name_in_expression(pattern, name_u16.as_ucstr(), true) {
                continue;
            }
            let find_data = dokan::FindData {
                attributes: filetype_to_windows_attr(attr.kind, attr.perm),
                creation_time: attr.ctime,
                last_access_time: attr.atime,
                last_write_time: attr.mtime,
                file_size: attr.size,
                file_name: name_u16,
            };
            if fill_find_data(&find_data).is_err() {
                break;
            }
            emitted_offset = Some(entry.offset as i64);
        }
        Ok(emitted_offset)
    }

    pub(super) fn reply_directory_status(
        &self, reply: &ReplyDirectory,
    ) -> dokan::OperationResult<()> {
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(())) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(crate::dokan_impl::missing_reply_status()),
        }
    }

    pub(super) fn reply_directoryplus_status(
        &self, reply: &ReplyDirectoryPlus,
    ) -> dokan::OperationResult<()> {
        match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(())) => Ok(()),
            Some(Err(err)) => Err(errno_to_ntstatus(err)),
            None => Err(crate::dokan_impl::missing_reply_status()),
        }
    }
}
