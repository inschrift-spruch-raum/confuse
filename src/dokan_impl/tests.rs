use super::*;
use crate::dokan_impl::AdapterContext;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::fuse_abi::consts::FUSE_ROOT_ID;
use crate::fuser_facade::reply::*;
use crate::fuser_facade::request::{Request, request_kernel};
use crate::fuser_facade::types::{FileType, KernelConfig, MountOption};
use dokan_sys::win32::{FILE_CREATE, FILE_SUPERSEDE};
use std::ffi::OsStr;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(feature = "abi-7-23")]
use std::time::Duration;
use widestring::U16CString;
use winapi::shared::ntstatus::*;
use winapi::um::winnt::*;

#[test]
fn maps_basic_supported_options() {
    let opts = vec![
        MountOption::RO,
        MountOption::CUSTOM("single_thread".to_string()),
        MountOption::CUSTOM("debug".to_string()),
    ];

    let mapped = to_dokan_mount_options(&opts).expect("mapping should succeed");
    assert!(mapped.single_thread);
    assert!(mapped.flags.contains(dokan::MountFlags::WRITE_PROTECT));
    assert!(mapped.flags.contains(dokan::MountFlags::DEBUG));
}

#[test]
fn accepts_unknown_custom_option() {
    let result = to_dokan_mount_options(&[MountOption::CUSTOM("foo".to_string())]);
    assert!(result.is_ok());
}

#[test]
fn parses_fuser_style_mount_option_strings() {
    let input = [
        OsStr::new("ro"),
        OsStr::new("rw"),
        OsStr::new("fsname=myfs"),
        OsStr::new("subtype=mysub"),
        OsStr::new("allow_other"),
        OsStr::new("custom_x"),
    ];
    let parsed = parse_mount_options(&input);

    assert!(matches!(parsed[0], MountOption::RO));
    assert!(matches!(parsed[1], MountOption::RW));
    assert!(matches!(parsed[2], MountOption::FSName(ref v) if v == "myfs"));
    assert!(matches!(parsed[3], MountOption::Subtype(ref v) if v == "mysub"));
    assert!(matches!(parsed[4], MountOption::AllowOther));
    assert!(matches!(parsed[5], MountOption::CUSTOM(ref v) if v == "custom_x"));
}

#[test]
fn parses_fuser_style_dash_o_argument_shape() {
    let input = [
        OsStr::new("-o"),
        OsStr::new("ro,fsname=myfs"),
        OsStr::new("-odebug"),
    ];
    let parsed = parse_mount_options_from_args(&input).expect("parse should succeed");

    assert!(matches!(parsed[0], MountOption::RO));
    assert!(matches!(parsed[1], MountOption::FSName(ref v) if v == "myfs"));
    assert!(matches!(parsed[2], MountOption::CUSTOM(ref v) if v == "debug"));
}

#[test]
fn errno_mapping_has_known_cases() {
    assert_eq!(errno_to_ntstatus(libc::ENOSYS), STATUS_NOT_IMPLEMENTED);
    assert_eq!(errno_to_ntstatus(libc::EPERM), STATUS_ACCESS_DENIED);
    assert_eq!(
        errno_to_ntstatus(libc::ENOENT),
        STATUS_OBJECT_NAME_NOT_FOUND
    );
    assert_eq!(
        errno_to_ntstatus(libc::EEXIST),
        STATUS_OBJECT_NAME_COLLISION
    );
    assert_eq!(errno_to_ntstatus(libc::ENOSPC), STATUS_DISK_FULL);
    assert_eq!(errno_to_ntstatus(libc::EBUSY), STATUS_ALREADY_COMMITTED);
    assert_eq!(errno_to_ntstatus(123456), STATUS_UNSUCCESSFUL);
}

#[test]
fn create_disposition_classifier_behaves_as_expected() {
    assert!(matches!(
        create_disposition_plan(FILE_CREATE),
        CreateDispositionPlan::CreateOnly
    ));
    assert!(matches!(
        create_disposition_plan(FILE_SUPERSEDE),
        CreateDispositionPlan::Supersede
    ));
    assert!(matches!(
        create_disposition_plan(dokan_sys::win32::FILE_OPEN_IF),
        CreateDispositionPlan::CreateThenOpenOnExists
    ));
    assert!(matches!(
        create_disposition_plan(dokan_sys::win32::FILE_OVERWRITE_IF),
        CreateDispositionPlan::CreateThenOpenOnExists
    ));
    assert!(matches!(
        create_disposition_plan(u32::MAX),
        CreateDispositionPlan::OpenOnly
    ));
}

#[test]
fn split_parent_and_name_parses_windows_style_path() {
    let path = U16CString::from_str("\\a\\b\\file.txt").expect("path");
    let (parent_path, leaf) = split_parent_and_name(path.as_ucstr());
    assert_eq!(leaf.to_string_lossy(), "file.txt");
    assert_eq!(parent_path.to_string_lossy(), "\\a\\b");
}

#[test]
fn resolve_ctx_prefers_context_ino() {
    let path = U16CString::from_str("\\x\\y").expect("path");
    let expected = AdapterContext {
        fh: 9,
        flags: 7,
        ino: 123,
        is_dir: true,
        lock_owner: 0,
        request_ids: Default::default(),
    };

    let resolved = resolve_ctx(path.as_ucstr(), &expected).expect("resolved from context");
    assert_eq!(resolved.fh, expected.fh);
    assert_eq!(resolved.flags, expected.flags);
    assert_eq!(resolved.ino, expected.ino);
    assert_eq!(resolved.is_dir, expected.is_dir);
}

#[test]
fn resolve_ctx_returns_none_for_non_root_without_ino() {
    let path = U16CString::from_str("\\missing\\entry").expect("path");
    let fallback = AdapterContext::default();
    let resolved = resolve_ctx(path.as_ucstr(), &fallback);
    assert!(resolved.is_none());
}

#[test]
fn resolve_ctx_returns_root_for_root_path_without_ino() {
    let path = U16CString::from_str("\\").expect("path");
    let fallback = AdapterContext::default();
    let resolved = resolve_ctx(path.as_ucstr(), &fallback).expect("resolved root");
    assert_eq!(resolved.ino, FUSE_ROOT_ID);
    assert!(resolved.is_dir);
}

#[test]
fn ino_from_context_or_path_prefers_context_then_none() {
    let direct = AdapterContext {
        fh: 1,
        flags: 0,
        ino: 777,
        is_dir: false,
        lock_owner: 0,
        request_ids: Default::default(),
    };
    assert_eq!(ino_from_context_or_path(&direct), Some(777));

    let no_ino = AdapterContext::default();
    assert_eq!(ino_from_context_or_path(&no_ino), None);
}

#[test]
fn derive_volume_names_prefers_fsname_and_subtype() {
    let opts = vec![
        MountOption::FSName("volA".to_string()),
        MountOption::Subtype("typeB".to_string()),
    ];
    let (vol, fs) = derive_volume_names(&opts);
    assert_eq!(vol, "volA");
    assert_eq!(fs, "typeB");
}

#[test]
fn to_dokan_mount_options_accepts_common_fuser_options_on_windows() {
    let opts = vec![
        MountOption::AllowOther,
        MountOption::AllowRoot,
        MountOption::AutoUnmount,
        MountOption::DefaultPermissions,
        MountOption::Dev,
        MountOption::NoDev,
        MountOption::Suid,
        MountOption::NoSuid,
        MountOption::Exec,
        MountOption::NoExec,
        MountOption::Atime,
        MountOption::NoAtime,
        MountOption::DirSync,
        MountOption::Sync,
        MountOption::Async,
        MountOption::FSName("x".to_string()),
        MountOption::Subtype("y".to_string()),
    ];
    let mapped = to_dokan_mount_options(&opts);
    assert!(mapped.is_ok());
}

#[test]
fn windows_inexpressible_mount_options_are_explicit_and_noop_mapped() {
    let opts = vec![
        MountOption::AllowOther,
        MountOption::AllowRoot,
        MountOption::AutoUnmount,
        MountOption::DefaultPermissions,
        MountOption::Dev,
        MountOption::NoDev,
        MountOption::Suid,
        MountOption::NoSuid,
        MountOption::Exec,
        MountOption::NoExec,
        MountOption::Atime,
        MountOption::NoAtime,
        MountOption::DirSync,
        MountOption::Sync,
        MountOption::Async,
    ];
    let baseline = dokan::MountOptions::default();
    let mapped = to_dokan_mount_options(&opts).expect("mapping should succeed");
    assert_eq!(mapped.flags, baseline.flags);
    assert_eq!(mapped.single_thread, baseline.single_thread);
    for opt in &opts {
        assert!(is_dokan_inexpressible_mount_option(opt));
    }
}

#[test]
fn windows_representable_mount_options_map_deterministically() {
    let ro = to_dokan_mount_options(&[MountOption::RO]).expect("ro maps");
    assert!(ro.flags.contains(dokan::MountFlags::WRITE_PROTECT));

    let rw = to_dokan_mount_options(&[MountOption::RW]).expect("rw maps");
    assert!(!rw.flags.contains(dokan::MountFlags::WRITE_PROTECT));

    let debug =
        to_dokan_mount_options(&[MountOption::CUSTOM("debug".to_string())]).expect("debug maps");
    assert!(debug.flags.contains(dokan::MountFlags::DEBUG));

    let st = to_dokan_mount_options(&[MountOption::CUSTOM("single_thread".to_string())])
        .expect("single_thread maps");
    assert!(st.single_thread);
}

#[test]
fn parse_mount_options_from_args_accepts_plain_tokens() {
    let input = [
        OsStr::new("allow_other,default_permissions"),
        OsStr::new("rw"),
    ];
    let parsed = parse_mount_options_from_args(&input).expect("parse should succeed");
    assert_eq!(parsed[0], MountOption::AllowOther);
    assert_eq!(parsed[1], MountOption::DefaultPermissions);
    assert_eq!(parsed[2], MountOption::RW);
}

#[test]
fn parse_mount_options_from_args_skips_empty_segments_and_trims() {
    let input = [
        OsStr::new("-o"),
        OsStr::new("ro, ,fsname=myfs,,allow_other"),
    ];
    let parsed = parse_mount_options_from_args(&input).expect("parse should succeed");
    assert_eq!(parsed.len(), 3);
    assert_eq!(parsed[0], MountOption::RO);
    assert!(matches!(parsed[1], MountOption::FSName(ref v) if v == "myfs"));
    assert_eq!(parsed[2], MountOption::AllowOther);
}

#[test]
fn access_mask_to_open_flags_maps_generic_and_specific_bits() {
    assert_eq!(access_mask_to_open_flags(0), libc::O_RDONLY);
    assert_eq!(access_mask_to_open_flags(GENERIC_READ), libc::O_RDONLY);
    assert_eq!(access_mask_to_open_flags(GENERIC_WRITE), libc::O_WRONLY);
    assert_eq!(
        access_mask_to_open_flags(GENERIC_READ | GENERIC_WRITE),
        libc::O_RDWR
    );
    assert_eq!(access_mask_to_open_flags(FILE_READ_DATA), libc::O_RDONLY);
    assert_eq!(access_mask_to_open_flags(FILE_WRITE_DATA), libc::O_WRONLY);
    assert_eq!(access_mask_to_open_flags(FILE_APPEND_DATA), libc::O_WRONLY);
    assert_eq!(
        access_mask_to_open_flags(FILE_READ_DATA | FILE_WRITE_DATA),
        libc::O_RDWR
    );
}

#[test]
fn lock_owner_from_context_uses_owner_then_none() {
    let with_owner = AdapterContext {
        fh: 11,
        flags: 0,
        ino: 1,
        is_dir: false,
        lock_owner: 99,
        request_ids: Default::default(),
    };
    assert_eq!(lock_owner_from_context(with_owner), Some(99));

    let with_fh = AdapterContext {
        fh: 11,
        flags: 0,
        ino: 1,
        is_dir: false,
        lock_owner: 0,
        request_ids: Default::default(),
    };
    assert_eq!(lock_owner_from_context(with_fh), None);

    let none = AdapterContext::default();
    assert_eq!(lock_owner_from_context(none), None);
}

#[test]
fn find_files_attr_uses_getattr_perm_and_fallback() {
    let ro_file = find_files_attr_from_kind_and_perm(FileType::RegularFile, Some(0o444));
    assert_ne!(ro_file & FILE_ATTRIBUTE_READONLY, 0);

    let rw_file = find_files_attr_from_kind_and_perm(FileType::RegularFile, Some(0o666));
    assert_eq!(rw_file & FILE_ATTRIBUTE_READONLY, 0);

    let fallback = find_files_attr_from_kind_and_perm(FileType::RegularFile, None);
    assert_eq!(fallback & FILE_ATTRIBUTE_READONLY, 0);

    let dir_attr = find_files_attr_from_kind_and_perm(FileType::Directory, Some(0o555));
    assert_ne!(dir_attr & FILE_ATTRIBUTE_DIRECTORY, 0);
    assert_ne!(dir_attr & FILE_ATTRIBUTE_READONLY, 0);
}

#[test]
fn advance_offset_only_when_entries_emitted() {
    assert_eq!(advance_offset_on_emitted(7, None), 7);
    assert_eq!(advance_offset_on_emitted(7, Some(11)), 11);
}

#[test]
fn default_kernel_config_has_expected_values() {
    let cfg = default_kernel_config();
    assert_eq!(cfg.max_write, 128 * 1024);
    assert_eq!(cfg.max_readahead, 128 * 1024);
    assert_eq!(cfg.max_max_readahead, 1024 * 1024);
    assert_eq!(cfg.capabilities, 0);
    assert_eq!(cfg.requested, 0);
    #[cfg(feature = "abi-7-13")]
    {
        assert_eq!(cfg.max_background, 16);
        assert!(cfg.congestion_threshold.is_none());
    }
    #[cfg(feature = "abi-7-23")]
    {
        assert_eq!(cfg.time_gran, Duration::new(0, 1));
    }
}

#[test]
fn missing_reply_status_returns_unsuccessful() {
    assert_eq!(missing_reply_status(), STATUS_UNSUCCESSFUL);
}

#[test]
fn filetype_to_windows_attr_directory_has_dir_flag() {
    let attr = filetype_to_windows_attr(FileType::Directory, 0o666);
    assert_ne!(attr & FILE_ATTRIBUTE_DIRECTORY, 0);
}

#[test]
fn filetype_to_windows_attr_non_directory_types_are_normal() {
    for (kind, label) in [
        (FileType::RegularFile, "RegularFile"),
        (FileType::Symlink, "Symlink"),
        (FileType::Socket, "Socket"),
        (FileType::NamedPipe, "NamedPipe"),
        (FileType::CharDevice, "CharDevice"),
        (FileType::BlockDevice, "BlockDevice"),
    ] {
        let attr = filetype_to_windows_attr(kind, 0o666);
        assert_eq!(
            attr & !FILE_ATTRIBUTE_READONLY,
            FILE_ATTRIBUTE_NORMAL,
            "failed for {label}"
        );
    }
}

#[test]
fn filetype_to_windows_attr_readonly_perm_sets_readonly_flag() {
    let attr = filetype_to_windows_attr(FileType::RegularFile, 0o444);
    assert_ne!(attr & FILE_ATTRIBUTE_READONLY, 0);
}

#[test]
fn filetype_to_windows_attr_writable_perm_no_readonly_flag() {
    let attr = filetype_to_windows_attr(FileType::RegularFile, 0o666);
    assert_eq!(attr & FILE_ATTRIBUTE_READONLY, 0);
}

#[test]
fn filetype_to_windows_attr_directory_readonly_combo() {
    let attr = filetype_to_windows_attr(FileType::Directory, 0o444);
    assert_ne!(attr & FILE_ATTRIBUTE_DIRECTORY, 0);
    assert_ne!(attr & FILE_ATTRIBUTE_READONLY, 0);
}

#[test]
fn filetime_op_to_option_set_time_returns_some() {
    let t = std::time::SystemTime::UNIX_EPOCH;
    let op = dokan::FileTimeOperation::SetTime(t);
    assert_eq!(filetime_op_to_option(op), Some(t));
}

#[test]
fn filetime_op_to_option_dont_change_returns_none() {
    let op = dokan::FileTimeOperation::DontChange;
    assert_eq!(filetime_op_to_option(op), None);
}

#[test]
fn filetime_op_to_option_disable_update_returns_none() {
    let op = dokan::FileTimeOperation::DisableUpdate;
    assert_eq!(filetime_op_to_option(op), None);
}

#[test]
fn filetime_op_to_option_resume_update_returns_none() {
    let op = dokan::FileTimeOperation::ResumeUpdate;
    assert_eq!(filetime_op_to_option(op), None);
}

#[test]
fn join_child_path_appends_separator_and_name() {
    let parent = U16CString::from_str("\\parent").expect("parent");
    let result = join_child_path(parent.as_ucstr(), OsStr::new("child.txt"));
    assert_eq!(result, "\\parent\\child.txt");
}

#[test]
fn join_child_path_parent_with_trailing_backslash() {
    let parent = U16CString::from_str("\\parent\\").expect("parent");
    let result = join_child_path(parent.as_ucstr(), OsStr::new("child.txt"));
    assert_eq!(result, "\\parent\\child.txt");
    assert!(!result.contains("\\\\"));
}

#[test]
fn rename_descendant_path_key_exact_match() {
    let result = rename_descendant_path_key("\\old", "\\new", "\\old");
    assert_eq!(result, Some("\\new".to_string()));
}

#[test]
fn rename_descendant_path_key_descendant_match() {
    let result = rename_descendant_path_key("\\old", "\\new", "\\old\\child.txt");
    assert_eq!(result, Some("\\new\\child.txt".to_string()));
}

#[test]
fn rename_descendant_path_key_non_descendant_returns_none() {
    let result = rename_descendant_path_key("\\old", "\\new", "\\other");
    assert_eq!(result, None);
}

#[test]
fn rename_descendant_path_key_partial_prefix_no_match() {
    let result = rename_descendant_path_key("\\old", "\\new", "\\oldx");
    assert_eq!(result, None);
}

#[test]
fn is_directory_open_with_directory_bit_set() {
    assert!(is_directory_open(dokan_sys::win32::FILE_DIRECTORY_FILE));
}

#[test]
fn is_directory_open_without_directory_bit() {
    assert!(!is_directory_open(0));
}

#[test]
fn parse_single_mount_option_exhaustive_known_strings() {
    assert!(matches!(parse_single_mount_option("ro"), MountOption::RO));
    assert!(matches!(parse_single_mount_option("rw"), MountOption::RW));
    assert!(matches!(
        parse_single_mount_option("allow_other"),
        MountOption::AllowOther
    ));
    assert!(matches!(
        parse_single_mount_option("allow_root"),
        MountOption::AllowRoot
    ));
    assert!(matches!(
        parse_single_mount_option("auto_unmount"),
        MountOption::AutoUnmount
    ));
    assert!(matches!(
        parse_single_mount_option("default_permissions"),
        MountOption::DefaultPermissions
    ));
    assert!(matches!(parse_single_mount_option("dev"), MountOption::Dev));
    assert!(matches!(
        parse_single_mount_option("nodev"),
        MountOption::NoDev
    ));
    assert!(matches!(
        parse_single_mount_option("suid"),
        MountOption::Suid
    ));
    assert!(matches!(
        parse_single_mount_option("nosuid"),
        MountOption::NoSuid
    ));
    assert!(matches!(
        parse_single_mount_option("exec"),
        MountOption::Exec
    ));
    assert!(matches!(
        parse_single_mount_option("noexec"),
        MountOption::NoExec
    ));
    assert!(matches!(
        parse_single_mount_option("atime"),
        MountOption::Atime
    ));
    assert!(matches!(
        parse_single_mount_option("noatime"),
        MountOption::NoAtime
    ));
    assert!(matches!(
        parse_single_mount_option("dirsync"),
        MountOption::DirSync
    ));
    assert!(matches!(
        parse_single_mount_option("sync"),
        MountOption::Sync
    ));
    assert!(matches!(
        parse_single_mount_option("async"),
        MountOption::Async
    ));
    assert!(
        matches!(parse_single_mount_option("fsname=myfs"), MountOption::FSName(ref v) if v == "myfs")
    );
    assert!(
        matches!(parse_single_mount_option("subtype=mysub"), MountOption::Subtype(ref v) if v == "mysub")
    );
    assert!(
        matches!(parse_single_mount_option("unknown_opt"), MountOption::CUSTOM(ref v) if v == "unknown_opt")
    );
}

// -- close/flush route tests (need Filesystem impl) --

#[derive(Default)]
struct RouteFs {
    release_called: usize,
    releasedir_called: usize,
    flush_called: usize,
    last_flush_owner: u64,
    fsync_called: usize,
    fsyncdir_called: usize,
}

impl Filesystem for RouteFs {
    fn release(
        &mut self, _req: &Request, _ino: u64, _fh: u64, _flags: i32, _lock_owner: Option<u64>,
        _flush: bool, _reply: ReplyEmpty,
    ) {
        self.release_called += 1;
    }

    fn releasedir(
        &mut self, _req: &Request, _ino: u64, _fh: u64, _flags: i32, _reply: ReplyEmpty,
    ) {
        self.releasedir_called += 1;
    }

    fn fsync(
        &mut self, _req: &Request, _ino: u64, _fh: u64, _datasync: bool, _reply: ReplyEmpty,
    ) {
        self.fsync_called += 1;
    }

    fn flush(
        &mut self, _req: &Request, _ino: u64, _fh: u64, lock_owner: u64, _reply: ReplyEmpty,
    ) {
        self.flush_called += 1;
        self.last_flush_owner = lock_owner;
    }

    fn fsyncdir(
        &mut self, _req: &Request, _ino: u64, _fh: u64, _datasync: bool, _reply: ReplyEmpty,
    ) {
        self.fsyncdir_called += 1;
    }
}

#[test]
fn close_route_splits_file_vs_directory() {
    let mut fs = RouteFs::default();
    let req = request_kernel();
    close_with_context(
        &mut fs,
        &req,
        AdapterContext {
            fh: 1,
            flags: 0,
            ino: 2,
            is_dir: false,
            lock_owner: 0,
            request_ids: Default::default(),
        },
    );
    close_with_context(
        &mut fs,
        &req,
        AdapterContext {
            fh: 1,
            flags: 0,
            ino: 2,
            is_dir: true,
            lock_owner: 0,
            request_ids: Default::default(),
        },
    );
    assert_eq!(fs.release_called, 1);
    assert_eq!(fs.releasedir_called, 1);
}

#[test]
fn flush_route_splits_file_vs_directory() {
    let mut fs = RouteFs::default();
    let req = request_kernel();
    flush_with_context(
        &mut fs,
        &req,
        AdapterContext {
            fh: 1,
            flags: 0,
            ino: 2,
            is_dir: false,
            lock_owner: 77,
            request_ids: Default::default(),
        },
        ReplyEmpty::default(),
    );
    flush_with_context(
        &mut fs,
        &req,
        AdapterContext {
            fh: 1,
            flags: 0,
            ino: 2,
            is_dir: true,
            lock_owner: 0,
            request_ids: Default::default(),
        },
        ReplyEmpty::default(),
    );
    assert_eq!(fs.fsync_called, 0);
    assert_eq!(fs.flush_called, 1);
    assert_eq!(fs.last_flush_owner, 77);
    assert_eq!(fs.fsyncdir_called, 1);
}

#[derive(Default)]
struct RenamePolicyFs {
    destination_exists: bool,
    lookup_called: usize,
    rename_called: usize,
}

impl Filesystem for RenamePolicyFs {
    fn lookup(&mut self, _req: &Request, _parent: u64, _name: &OsStr, reply: ReplyEntry) {
        self.lookup_called += 1;
        if self.destination_exists {
            reply.entry(&std::time::Duration::from_secs(1), &test_file_attr(99), 0);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn rename(
        &mut self, _req: &Request, _parent: u64, _name: &OsStr, _newparent: u64,
        _newname: &OsStr, _flags: u32, reply: ReplyEmpty,
    ) {
        self.rename_called += 1;
        reply.ok();
    }
}

fn test_file_attr(ino: u64) -> crate::fuser_facade::types::FileAttr {
    crate::fuser_facade::types::FileAttr {
        ino,
        size: 0,
        blocks: 0,
        atime: std::time::SystemTime::UNIX_EPOCH,
        mtime: std::time::SystemTime::UNIX_EPOCH,
        ctime: std::time::SystemTime::UNIX_EPOCH,
        crtime: std::time::SystemTime::UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    }
}

#[test]
fn rename_policy_reports_collision_when_replace_is_false() {
    let mut fs = RenamePolicyFs {
        destination_exists: true,
        ..Default::default()
    };
    let req = request_kernel();

    let result = rename_with_replace_policy(
        &mut fs,
        &req,
        1,
        OsStr::new("file.txt"),
        2,
        OsStr::new("file.txt"),
        false,
    );

    assert_eq!(result, Err(STATUS_OBJECT_NAME_COLLISION));
    assert_eq!(fs.lookup_called, 1);
    assert_eq!(fs.rename_called, 0);
}

#[test]
fn rename_policy_allows_replace_when_requested() {
    let mut fs = RenamePolicyFs {
        destination_exists: true,
        ..Default::default()
    };
    let req = request_kernel();

    let result = rename_with_replace_policy(
        &mut fs,
        &req,
        1,
        OsStr::new("file.txt"),
        2,
        OsStr::new("file.txt"),
        true,
    );

    assert_eq!(result, Ok(()));
    assert_eq!(fs.lookup_called, 0);
    assert_eq!(fs.rename_called, 1);
}

#[test]
fn rename_policy_renames_when_destination_is_missing() {
    let mut fs = RenamePolicyFs::default();
    let req = request_kernel();

    let result = rename_with_replace_policy(
        &mut fs,
        &req,
        1,
        OsStr::new("file.txt"),
        2,
        OsStr::new("file.txt"),
        false,
    );

    assert_eq!(result, Ok(()));
    assert_eq!(fs.lookup_called, 1);
    assert_eq!(fs.rename_called, 1);
}

// -- facade lifecycle tests --

#[derive(Default)]
struct InitDestroyFs {
    init_called: usize,
    destroy_called: usize,
}

impl Filesystem for InitDestroyFs {
    fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> std::io::Result<()>  {
        self.init_called += 1;
        Ok(())
    }

    fn destroy(&mut self) {
        self.destroy_called += 1;
    }
}

#[test]
fn mounted_unmounted_facade_calls_init_destroy_and_maps_errno() {
    let req = request_kernel();
    let mut fs = InitDestroyFs::default();
    let mounted = facade_mounted_with(&mut fs, &req);
    assert!(mounted.is_ok());
    assert_eq!(fs.init_called, 1);

    let destroyed = AtomicBool::new(false);
    facade_unmounted_with(&mut fs, &destroyed);
    assert_eq!(fs.destroy_called, 1);
    assert!(destroyed.load(Ordering::SeqCst));

    struct InitErrFs;
    impl Filesystem for InitErrFs {
        fn init(
            &mut self, _req: &Request, _config: &mut KernelConfig,
        ) -> std::io::Result<()>  {
            Err(std::io::Error::from_raw_os_error(libc::EACCES))
        }
    }

    let mut err_fs = InitErrFs;
    let err = facade_mounted_with(&mut err_fs, &req).expect_err("must map init errno");
    assert_eq!(err, STATUS_ACCESS_DENIED);
}

#[test]
fn lock_type_constants_match_fuser_values() {
    assert_eq!(LOCK_TYPE_WRLCK, 1);
    assert_eq!(LOCK_TYPE_UNLCK, 2);
}
