use std::ffi::OsStr;
use std::time::Duration;
use std::time::SystemTime;

use libc::c_int;
use winapi::shared::ntstatus::*;
use winapi::um::winnt::*;

use dokan_sys::win32::{FILE_CREATE, FILE_DIRECTORY_FILE, FILE_SUPERSEDE};

use crate::dokan_impl::AdapterContext;
use crate::fuser_facade::types::{
    AccessFlags, FileHandle, FileType, FopenFlags, INodeNo, InitFlags, KernelConfig, LockOwner,
    OpenFlags, RenameFlags, Version, WriteFlags,
};

mod operations;
pub(crate) use operations::*;
mod security;
pub(crate) use security::*;

// ---------------------------------------------------------------------------
// Helper: derive inode from context or fall back to path-based lookup
// ---------------------------------------------------------------------------

pub(crate) fn ino_from_context_or_path(context: &AdapterContext) -> Option<INodeNo> {
    if context.ino != ino(0) {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum OptionalProbeError {
    Unsupported { ntstatus: i32 },
    RequestError { ntstatus: i32 },
}

impl OptionalProbeError {
    pub(crate) fn ntstatus(self) -> i32 {
        match self {
            Self::Unsupported { ntstatus } | Self::RequestError { ntstatus } => ntstatus,
        }
    }
}

pub(crate) fn classify_optional_probe_error(err: c_int) -> OptionalProbeError {
    let ntstatus = errno_to_ntstatus(err);
    if err == libc::ENOSYS {
        OptionalProbeError::Unsupported { ntstatus }
    } else {
        OptionalProbeError::RequestError { ntstatus }
    }
}

pub(crate) fn missing_reply_status() -> i32 {
    STATUS_UNSUCCESSFUL
}

pub(crate) fn ino(value: u64) -> INodeNo {
    INodeNo(value)
}

pub(crate) fn fh(value: u64) -> FileHandle {
    FileHandle(value)
}

pub(crate) fn lock_owner(value: u64) -> LockOwner {
    LockOwner(value)
}

pub(crate) fn open_flags(value: i32) -> OpenFlags {
    OpenFlags(value)
}

pub(crate) fn open_flags_from_fopen_flags(value: FopenFlags) -> OpenFlags {
    open_flags(value.bits() as i32)
}

pub(crate) fn fopen_flags(value: u32) -> FopenFlags {
    FopenFlags::from_bits_truncate(value)
}

pub(crate) fn access_flags(value: i32) -> AccessFlags {
    AccessFlags::from_bits_truncate(value)
}

pub(crate) fn rename_flags(value: u32) -> RenameFlags {
    RenameFlags::from_bits_truncate(value)
}

pub(crate) fn write_flags(value: u32) -> WriteFlags {
    WriteFlags::from_bits_truncate(value)
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
    let mut parts: Vec<&str> = raw
        .split(['\\', '/'])
        .filter(|part| !part.is_empty() && !part.ends_with(':'))
        .collect();

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
    if context.ino != ino(0) {
        return Some(*context);
    }
    // Root directory fallback: when context has no inode (first access),
    // root path "\" always maps to INodeNo::ROOT.
    if file_name.to_string_lossy() == "\\" {
        return Some(AdapterContext {
            fh: fh(0),
            flags: fopen_flags(0),
            ino: INodeNo::ROOT,
            is_dir: true,
            lock_owner: None,
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
        capabilities: InitFlags::empty(),
        requested: InitFlags::empty(),
        max_background: 16,
        congestion_threshold: None,
        time_gran: Duration::new(0, 1),
        max_stack_depth: 0,
        kernel_abi: Version(7, 40),
    }
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

pub(crate) fn lock_owner_from_context(context: AdapterContext) -> Option<LockOwner> {
    context.lock_owner
}

pub(crate) fn nonnegative_i64_to_u64(value: i64) -> Result<u64, i32> {
    u64::try_from(value).map_err(|_| winapi::shared::ntstatus::STATUS_INVALID_PARAMETER)
}

pub(crate) fn checked_dokan_len(value: usize) -> Result<u32, i32> {
    u32::try_from(value).map_err(|_| winapi::shared::ntstatus::STATUS_INVALID_PARAMETER)
}

pub(crate) fn checked_lock_range(offset: i64, length: i64) -> Result<(u64, u64), i32> {
    if offset < 0 || length < 0 {
        return Err(winapi::shared::ntstatus::STATUS_INVALID_PARAMETER);
    }
    let start = nonnegative_i64_to_u64(offset)?;
    let end = offset
        .checked_add(length)
        .ok_or(winapi::shared::ntstatus::STATUS_INVALID_PARAMETER)
        .and_then(nonnegative_i64_to_u64)?;
    Ok((start, end))
}

// ---------------------------------------------------------------------------
// Path and offset utilities
// ---------------------------------------------------------------------------

pub(crate) fn advance_offset_on_emitted(current: i64, emitted_offset: Option<i64>) -> i64 {
    emitted_offset.unwrap_or(current)
}

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
