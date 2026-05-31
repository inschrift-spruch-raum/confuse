use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;
use std::time::SystemTime;

use libc::c_int;
use winapi::shared::ntstatus::*;
use winapi::um::winnt::*;

use dokan_sys::win32::{FILE_CREATE, FILE_DIRECTORY_FILE, FILE_SUPERSEDE};

use crate::dokan_impl::AdapterContext;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::fuse_abi::consts::FUSE_ROOT_ID;
use crate::fuser_facade::reply::*;
use crate::fuser_facade::request::Request;
use crate::fuser_facade::types::{FileType, KernelConfig, MountOption};

// ---------------------------------------------------------------------------
// Helper: derive inode from context or fall back to path-based lookup
// ---------------------------------------------------------------------------

pub(crate) fn ino_from_context_or_path(context: &AdapterContext) -> Option<u64> {
    if context.ino != 0 {
        return Some(context.ino);
    }
    None
}

// ---------------------------------------------------------------------------
// Error / attribute conversion helpers
// ---------------------------------------------------------------------------

pub(crate) fn errno_to_ntstatus(err: c_int) -> i32 {
    match err {
        libc::ENOSYS => STATUS_NOT_IMPLEMENTED,
        libc::ENOENT => STATUS_OBJECT_NAME_NOT_FOUND,
        libc::EEXIST => STATUS_OBJECT_NAME_COLLISION,
        libc::ENOSPC => STATUS_DISK_FULL,
        libc::EACCES | libc::EPERM => STATUS_ACCESS_DENIED,
        libc::EINVAL => winapi::shared::ntstatus::STATUS_INVALID_PARAMETER,
        libc::EBUSY => STATUS_ALREADY_COMMITTED,
        _ => STATUS_UNSUCCESSFUL,
    }
}

pub(crate) fn filetype_to_windows_attr(kind: FileType, perm: u16) -> u32 {
    let mut attr = match kind {
        FileType::Directory => FILE_ATTRIBUTE_DIRECTORY,
        _ => FILE_ATTRIBUTE_NORMAL,
    };
    if perm & 0o222 == 0 {
        attr |= FILE_ATTRIBUTE_READONLY;
    }
    attr
}

pub(crate) fn find_files_attr_from_kind_and_perm(
    kind: FileType, perm_from_getattr: Option<u16>,
) -> u32 {
    filetype_to_windows_attr(kind, perm_from_getattr.unwrap_or(0o666))
}

// ---------------------------------------------------------------------------
// Path splitting and parent-inode resolution
// ---------------------------------------------------------------------------

pub(crate) fn split_parent_and_name(
    path: &widestring::U16CStr,
) -> (std::ffi::OsString, std::ffi::OsString) {
    let raw = path.to_string_lossy();
    let mut parts: Vec<&str> = raw.split('\\').filter(|s| !s.is_empty()).collect();

    if parts.is_empty() {
        return (std::ffi::OsString::new(), std::ffi::OsString::new());
    }

    let leaf = parts.pop().unwrap_or_default();
    let parent_path = if parts.is_empty() {
        "\\".to_string()
    } else {
        format!("\\{}", parts.join("\\"))
    };

    (
        std::ffi::OsString::from(parent_path),
        std::ffi::OsString::from(leaf),
    )
}

/// Resolve a full Windows-style path to an inode by walking the path hierarchy
/// using `fs.lookup()` for each component. This replaces the previous approach
/// of caching inodes in the handles table.
pub(crate) fn path_to_ino<FS: Filesystem>(
    fs: &mut FS, req: &Request, path: &OsStr,
) -> Result<u64, c_int> {
    let s = path.to_string_lossy();
    if s == "\\" || s.is_empty() {
        return Ok(FUSE_ROOT_ID);
    }
    let components: Vec<&str> = s.split('\\').filter(|c| !c.is_empty()).collect();
    let mut ino = FUSE_ROOT_ID;
    for component in components {
        let reply = ReplyEntry::default();
        fs.lookup(req, ino, OsStr::new(component), reply.clone());
        match *reply.status.lock().map_err(|_| libc::EIO)? {
            Some(Ok(attr)) => ino = attr.ino,
            Some(Err(err)) => return Err(err),
            None => return Err(libc::EIO),
        }
    }
    Ok(ino)
}

/// Resolve parent directory inode via `path_to_ino`.
/// Falls back to `FUSE_ROOT_ID` on error.
pub(crate) fn resolve_parent_ino<FS: Filesystem>(
    fs: &mut FS, req: &Request, parent_path: &OsStr,
) -> u64 {
    path_to_ino(fs, req, parent_path).unwrap_or(FUSE_ROOT_ID)
}

pub(crate) fn rename_with_replace_policy<FS: Filesystem>(
    fs: &mut FS, req: &Request, old_parent_ino: u64, old_name: &OsStr, new_parent_ino: u64,
    new_name: &OsStr, replace_if_existing: bool,
) -> Result<(), i32> {
    if !replace_if_existing {
        let lookup = ReplyEntry::default();
        fs.lookup(req, new_parent_ino, new_name, lookup.clone());
        match *lookup.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
            Some(Ok(_)) => return Err(STATUS_OBJECT_NAME_COLLISION),
            Some(Err(err)) if err != libc::ENOENT && err != libc::ENOSYS => {
                return Err(errno_to_ntstatus(err));
            }
            _ => {}
        }
    }

    let reply = ReplyEmpty::default();
    fs.rename(
        req,
        old_parent_ino,
        old_name,
        new_parent_ino,
        new_name,
        0,
        reply.clone(),
    );
    match *reply.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
        Some(Ok(())) => Ok(()),
        Some(Err(err)) => Err(errno_to_ntstatus(err)),
        None => Err(missing_reply_status()),
    }
}

// ---------------------------------------------------------------------------
// Create-disposition and directory helpers
// ---------------------------------------------------------------------------

pub(crate) enum CreateDispositionPlan {
    CreateOnly,
    Supersede,
    OpenOnly,
    CreateThenOpenOnExists,
}

pub(crate) fn create_disposition_plan(disposition: u32) -> CreateDispositionPlan {
    match disposition {
        FILE_CREATE => CreateDispositionPlan::CreateOnly,
        FILE_SUPERSEDE => CreateDispositionPlan::Supersede,
        dokan_sys::win32::FILE_OPEN_IF | dokan_sys::win32::FILE_OVERWRITE_IF => {
            CreateDispositionPlan::CreateThenOpenOnExists
        }
        _ => CreateDispositionPlan::OpenOnly,
    }
}

pub(crate) fn is_directory_open(create_options: u32) -> bool {
    (create_options & FILE_DIRECTORY_FILE) != 0
}

// ---------------------------------------------------------------------------
// Context resolution and file-time helpers
// ---------------------------------------------------------------------------

pub(crate) fn resolve_ctx(
    file_name: &widestring::U16CStr, context: &AdapterContext,
) -> Option<AdapterContext> {
    if context.ino != 0 {
        return Some(*context);
    }
    // Root directory fallback: when context has no inode (first access),
    // root path "\" always maps to FUSE_ROOT_ID
    if file_name.to_string_lossy() == "\\" {
        return Some(AdapterContext {
            fh: 0,
            flags: 0,
            ino: FUSE_ROOT_ID,
            is_dir: true,
            lock_owner: 0,
            request_ids: context.request_ids,
        });
    }
    None
}

pub(crate) fn filetime_op_to_option(op: dokan::FileTimeOperation) -> Option<SystemTime> {
    match op {
        dokan::FileTimeOperation::SetTime(t) => Some(t),
        dokan::FileTimeOperation::DontChange
        | dokan::FileTimeOperation::DisableUpdate
        | dokan::FileTimeOperation::ResumeUpdate => None,
    }
}

// ---------------------------------------------------------------------------
// Kernel config default
// ---------------------------------------------------------------------------

pub(crate) fn default_kernel_config() -> KernelConfig {
    KernelConfig {
        max_write: 128 * 1024,
        max_readahead: 128 * 1024,
        max_max_readahead: 1024 * 1024,
        capabilities: 0,
        requested: 0,
        max_background: 16,
        congestion_threshold: None,
        time_gran: Duration::new(0, 1),
        max_stack_depth: 0,
        kernel_abi: crate::fuser_facade::types::Version { major: 7, minor: 40 },
    }
}

// ---------------------------------------------------------------------------
// Facade lifecycle helpers (close, flush, mount, unmount)
// ---------------------------------------------------------------------------

pub(crate) fn close_with_context<FS: Filesystem>(
    fs: &mut FS, req: &Request, context: AdapterContext,
) {
    if context.is_dir {
        fs.releasedir(
            req,
            context.ino,
            context.fh,
            context.flags as i32,
            ReplyEmpty::default(),
        );
    } else {
        fs.release(
            req,
            context.ino,
            context.fh,
            context.flags as i32,
            lock_owner_from_context(context),
            false,
            ReplyEmpty::default(),
        );
    }
}

pub(crate) fn flush_with_context<FS: Filesystem>(
    fs: &mut FS, req: &Request, context: AdapterContext, reply: ReplyEmpty,
) {
    if context.is_dir {
        fs.fsyncdir(req, context.ino, context.fh, false, reply);
    } else {
        let lock_owner = lock_owner_from_context(context).unwrap_or(0);
        fs.flush(req, context.ino, context.fh, lock_owner, reply);
    }
}

pub(crate) fn facade_mounted_with<FS: Filesystem>(
    fs: &mut FS, req: &Request,
) -> Result<(), i32> {
    let mut cfg = default_kernel_config();
    fs.init(req, &mut cfg)
        .map_err(|err| err.raw_os_error().map_or(STATUS_UNSUCCESSFUL, errno_to_ntstatus))
}

pub(crate) fn facade_unmounted_with<FS: Filesystem>(fs: &mut FS, destroyed: &AtomicBool) {
    fs.destroy();
    destroyed.store(true, Ordering::SeqCst);
}

// ---------------------------------------------------------------------------
// Access-mask, lock-owner, and reply-status helpers
// ---------------------------------------------------------------------------

pub(crate) fn access_mask_to_open_flags(desired_access: winapi::um::winnt::ACCESS_MASK) -> i32 {
    let can_read = (desired_access & GENERIC_READ) != 0 || (desired_access & FILE_READ_DATA) != 0;
    let can_write = (desired_access & GENERIC_WRITE) != 0
        || (desired_access & FILE_WRITE_DATA) != 0
        || (desired_access & FILE_APPEND_DATA) != 0;
    match (can_read, can_write) {
        (true, true) => libc::O_RDWR,
        (false, true) => libc::O_WRONLY,
        _ => libc::O_RDONLY,
    }
}

pub(crate) fn lock_owner_from_context(context: AdapterContext) -> Option<u64> {
    if context.lock_owner != 0 {
        return Some(context.lock_owner);
    }
    None
}

pub(crate) fn missing_reply_status() -> i32 {
    STATUS_UNSUCCESSFUL
}

// ---------------------------------------------------------------------------
// Path and offset utilities
// ---------------------------------------------------------------------------

pub(crate) fn advance_offset_on_emitted(current: i64, emitted_offset: Option<i64>) -> i64 {
    emitted_offset.unwrap_or(current)
}

#[allow(dead_code)]
pub(crate) fn join_child_path(parent: &widestring::U16CStr, name: &OsStr) -> String {
    let mut base = parent.to_string_lossy();
    if !base.ends_with('\\') {
        base.push('\\');
    }
    base.push_str(&name.to_string_lossy());
    base
}

pub(crate) fn rename_descendant_path_key(
    old_root: &str, new_root: &str, key: &str,
) -> Option<String> {
    if key == old_root {
        return Some(new_root.to_string());
    }
    let mut old_prefix = old_root.to_string();
    if !old_prefix.ends_with('\\') {
        old_prefix.push('\\');
    }
    if let Some(rest) = key.strip_prefix(&old_prefix) {
        let mut next = new_root.to_string();
        if !next.ends_with('\\') {
            next.push('\\');
        }
        next.push_str(rest);
        return Some(next);
    }
    None
}

// POSIX lock type values used by fuser setlk/getlk APIs.
pub(crate) const LOCK_TYPE_WRLCK: i32 = 1;
pub(crate) const LOCK_TYPE_UNLCK: i32 = 2;

// ---------------------------------------------------------------------------
// Volume name derivation from mount options
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

// ---------------------------------------------------------------------------
// Directory offset helpers (test-only)
// ---------------------------------------------------------------------------

#[cfg(test)]
pub(crate) fn next_dir_offset_from_entries(
    current: i64, entries: &[(u64, i64, FileType, std::ffi::OsString)],
) -> i64 {
    entries.last().map(|(_, off, _, _)| *off).unwrap_or(current)
}

#[cfg(test)]
pub(crate) fn next_dirplus_offset_from_entries(
    current: i64,
    entries: &[(
        u64,
        i64,
        std::ffi::OsString,
        crate::fuser_facade::types::FileAttr,
    )],
) -> i64 {
    entries.last().map(|(_, off, _, _)| *off).unwrap_or(current)
}
