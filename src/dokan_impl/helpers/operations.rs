use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, Ordering};

use winapi::shared::ntstatus::*;

use crate::dokan_impl::AdapterContext;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::*;
use crate::fuser_facade::request::Request;
use crate::fuser_facade::types::{FileType, INodeNo, MountOption};

use super::{
    access_flags, default_kernel_config, errno_to_ntstatus, fh, lock_owner_from_context,
    missing_reply_status, nonnegative_i64_to_u64, open_flags_from_fopen_flags, rename_flags,
};

// ---------------------------------------------------------------------------
// Delete and rename operation prechecks
// ---------------------------------------------------------------------------

pub(crate) fn precheck_file_delete<FS: Filesystem>(
    fs: &FS, req: &Request, ino: INodeNo,
) -> Result<(), i32> {
    let attr = ReplyAttr::capture();
    fs.getattr(req, ino, None, attr.duplicate());
    match *attr.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
        Some(Ok(attr)) if matches!(attr.kind, FileType::Directory) => {
            return Err(STATUS_FILE_IS_A_DIRECTORY);
        }
        Some(Ok(_)) => {}
        Some(Err(err)) => return Err(errno_to_ntstatus(err)),
        None => return Err(missing_reply_status()),
    }

    let access = ReplyEmpty::capture();
    fs.access(req, ino, access_flags(2), access.duplicate());
    match *access.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
        Some(Ok(())) => Ok(()),
        Some(Err(err)) if err == libc::ENOSYS => Ok(()),
        Some(Err(err)) => Err(errno_to_ntstatus(err)),
        None => Err(missing_reply_status()),
    }
}

pub(crate) fn precheck_directory_delete<FS: Filesystem>(
    fs: &FS, req: &Request, ino: INodeNo,
) -> Result<(), i32> {
    let attr = ReplyAttr::capture();
    fs.getattr(req, ino, None, attr.duplicate());
    match *attr.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
        Some(Ok(attr)) if !matches!(attr.kind, FileType::Directory) => {
            return Err(STATUS_NOT_A_DIRECTORY);
        }
        Some(Ok(_)) => {}
        Some(Err(err)) => return Err(errno_to_ntstatus(err)),
        None => return Err(missing_reply_status()),
    }

    let reply = ReplyDirectory::capture();
    fs.readdir(req, ino, fh(0), 0, reply.duplicate());
    let entries = reply
        .entries
        .lock()
        .map_err(|_| STATUS_NOT_IMPLEMENTED)?
        .clone();
    let has_child = entries.iter().any(|entry| {
        let name = entry.name.to_string_lossy();
        name != "." && name != ".."
    });
    if has_child {
        return Err(STATUS_DIRECTORY_NOT_EMPTY);
    }
    match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
        Some(Ok(())) => Ok(()),
        Some(Err(err)) => Err(errno_to_ntstatus(err)),
        None => Err(missing_reply_status()),
    }
}

pub(crate) fn rename_with_replace_policy<FS: Filesystem>(
    fs: &FS, req: &Request, old_parent_ino: INodeNo, old_name: &OsStr, new_parent_ino: INodeNo,
    new_name: &OsStr,
) -> Result<(), i32> {
    let reply = ReplyEmpty::capture();
    fs.rename(
        req,
        old_parent_ino,
        old_name,
        new_parent_ino,
        new_name,
        rename_flags(0),
        reply.duplicate(),
    );
    match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
        Some(Ok(())) => Ok(()),
        Some(Err(err)) => Err(errno_to_ntstatus(err)),
        None => Err(missing_reply_status()),
    }
}

// ---------------------------------------------------------------------------
// Facade lifecycle and I/O planning
// ---------------------------------------------------------------------------

pub(crate) fn close_with_context<FS: Filesystem>(fs: &FS, req: &Request, context: AdapterContext) {
    if context.is_dir {
        fs.releasedir(
            req,
            context.ino,
            context.fh,
            open_flags_from_fopen_flags(context.flags),
            ReplyEmpty::capture(),
        );
    } else {
        fs.release(
            req,
            context.ino,
            context.fh,
            open_flags_from_fopen_flags(context.flags),
            lock_owner_from_context(context),
            false,
            ReplyEmpty::capture(),
        );
    }
}

pub(crate) fn flush_with_context<FS: Filesystem>(
    fs: &FS, req: &Request, context: AdapterContext, reply: ReplyEmpty,
) {
    if context.is_dir {
        fs.fsyncdir(req, context.ino, context.fh, false, reply);
    } else {
        fs.fsync(req, context.ino, context.fh, false, reply);
    }
}

pub(crate) fn allocation_size_with_context<FS: Filesystem>(
    fs: &FS, req: &Request, context: AdapterContext, alloc_size: i64, reply: ReplyEmpty,
) -> Result<(), i32> {
    const FALLOC_FL_KEEP_SIZE: i32 = 1;
    fs.fallocate(
        req,
        context.ino,
        context.fh,
        0,
        nonnegative_i64_to_u64(alloc_size)?,
        FALLOC_FL_KEEP_SIZE,
        reply,
    );
    Ok(())
}

pub(crate) fn dokan_write_plan<FS: Filesystem>(
    fs: &FS, req: &Request, context: AdapterContext, offset: i64, len: usize, write_to_eof: bool,
    paging_io: bool,
) -> Result<(u64, usize), i32> {
    if !write_to_eof && offset < 0 {
        return Err(winapi::shared::ntstatus::STATUS_INVALID_PARAMETER);
    }

    if !write_to_eof && !paging_io {
        return Ok((nonnegative_i64_to_u64(offset)?, len));
    }

    let attr = ReplyAttr::capture();
    fs.getattr(req, context.ino, Some(context.fh), attr.duplicate());
    let size = match *attr.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
        Some(Ok(attr)) => attr.size,
        Some(Err(err)) => return Err(errno_to_ntstatus(err)),
        None => return Err(missing_reply_status()),
    };
    let start = if write_to_eof {
        size
    } else {
        nonnegative_i64_to_u64(offset)?
    };

    if paging_io {
        if start >= size {
            return Ok((start, 0));
        }
        let remaining = usize::try_from(size - start).unwrap_or(usize::MAX);
        return Ok((start, len.min(remaining)));
    }

    Ok((start, len))
}

pub(crate) fn facade_mounted_with<FS: Filesystem>(fs: &mut FS, req: &Request) -> Result<(), i32> {
    let mut cfg = default_kernel_config();
    fs.init(req, &mut cfg).map_err(|err| {
        err.raw_os_error()
            .map_or(STATUS_UNSUCCESSFUL, errno_to_ntstatus)
    })
}

pub(crate) fn facade_unmounted_with<FS: Filesystem>(fs: &mut FS, destroyed: &AtomicBool) {
    fs.destroy();
    destroyed.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// Volume and directory emission helpers
// ---------------------------------------------------------------------------

pub(crate) fn derive_volume_names(options: &[MountOption]) -> (String, String) {
    let mut volume_name = "confuse".to_string();
    let mut fs_name = "FUSER".to_string();
    for opt in options {
        match opt {
            MountOption::FSName(v) => volume_name = v.clone(),
            MountOption::Subtype(v) => fs_name = v.clone(),
            _ => {}
        }
    }
    (volume_name, fs_name)
}

#[cfg(test)]
pub(crate) fn next_dir_offset_from_entries(
    current: i64, entries: &[crate::fuser_facade::reply::DirectoryEntryPayload],
) -> i64 {
    entries
        .last()
        .map(|entry| entry.offset as i64)
        .unwrap_or(current)
}

#[cfg(test)]
pub(crate) fn next_dirplus_offset_from_entries(
    current: i64, entries: &[crate::fuser_facade::reply::DirectoryPlusEntryPayload],
) -> i64 {
    entries
        .last()
        .map(|entry| entry.offset as i64)
        .unwrap_or(current)
}
