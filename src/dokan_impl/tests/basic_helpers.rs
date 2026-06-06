use super::*;

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

    assert_eq!(cfg.capabilities, InitFlags::empty());

    assert_eq!(cfg.requested, InitFlags::empty());

    assert_eq!(cfg.max_background, 16);

    assert!(cfg.congestion_threshold.is_none());

    assert_eq!(cfg.time_gran, Duration::new(0, 1));
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
    assert!(matches!(
        parse_single_mount_option("ro"),
        ParsedMountOption::Mount(MountOption::RO)
    ));

    assert!(matches!(
        parse_single_mount_option("rw"),
        ParsedMountOption::Mount(MountOption::RW)
    ));

    assert!(matches!(
        parse_single_mount_option("allow_other"),
        ParsedMountOption::Acl(SessionACL::All)
    ));

    assert!(matches!(
        parse_single_mount_option("allow_root"),
        ParsedMountOption::Acl(SessionACL::RootAndOwner)
    ));

    assert!(matches!(
        parse_single_mount_option("auto_unmount"),
        ParsedMountOption::Mount(MountOption::AutoUnmount)
    ));

    assert!(matches!(
        parse_single_mount_option("default_permissions"),
        ParsedMountOption::Mount(MountOption::DefaultPermissions)
    ));

    assert!(matches!(
        parse_single_mount_option("dev"),
        ParsedMountOption::Mount(MountOption::Dev)
    ));

    assert!(matches!(
        parse_single_mount_option("nodev"),
        ParsedMountOption::Mount(MountOption::NoDev)
    ));

    assert!(matches!(
        parse_single_mount_option("suid"),
        ParsedMountOption::Mount(MountOption::Suid)
    ));

    assert!(matches!(
        parse_single_mount_option("nosuid"),
        ParsedMountOption::Mount(MountOption::NoSuid)
    ));

    assert!(matches!(
        parse_single_mount_option("exec"),
        ParsedMountOption::Mount(MountOption::Exec)
    ));

    assert!(matches!(
        parse_single_mount_option("noexec"),
        ParsedMountOption::Mount(MountOption::NoExec)
    ));

    assert!(matches!(
        parse_single_mount_option("atime"),
        ParsedMountOption::Mount(MountOption::Atime)
    ));

    assert!(matches!(
        parse_single_mount_option("noatime"),
        ParsedMountOption::Mount(MountOption::NoAtime)
    ));

    assert!(matches!(
        parse_single_mount_option("dirsync"),
        ParsedMountOption::Mount(MountOption::DirSync)
    ));

    assert!(matches!(
        parse_single_mount_option("sync"),
        ParsedMountOption::Mount(MountOption::Sync)
    ));

    assert!(matches!(
        parse_single_mount_option("async"),
        ParsedMountOption::Mount(MountOption::Async)
    ));

    assert!(
        matches!(parse_single_mount_option("fsname=myfs"), ParsedMountOption::Mount(MountOption::FSName(ref v)) if v == "myfs")
    );

    assert!(
        matches!(parse_single_mount_option("subtype=mysub"), ParsedMountOption::Mount(MountOption::Subtype(ref v)) if v == "mysub")
    );

    assert!(
        matches!(parse_single_mount_option("unknown_opt"), ParsedMountOption::Mount(MountOption::CUSTOM(ref v)) if v == "unknown_opt")
    );
}

// -- close/flush route tests (need Filesystem impl) --
