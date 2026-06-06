use super::*;

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

fn auto_probe_is_internal_custom_option_and_defaults_off() {
    assert!(!is_auto_probe_enabled(&[]));

    let parsed = parse_single_mount_option("auto_probe");

    assert!(
        matches!(parsed, ParsedMountOption::Mount(MountOption::CUSTOM(ref v)) if v == "auto_probe")
    );

    let opts = [MountOption::CUSTOM("auto_probe".to_string())];

    assert!(is_auto_probe_enabled(&opts));

    let mapped = to_dokan_mount_options(&opts).expect("auto_probe is accepted internally");

    let baseline = dokan::MountOptions::default();

    assert_eq!(mapped.flags, baseline.flags);

    assert_eq!(mapped.single_thread, baseline.single_thread);
}

#[test]

fn parses_fuser_style_mount_option_strings() {
    let input = [
        OsStr::new("ro"),
        OsStr::new("fsname=myfs"),
        OsStr::new("subtype=mysub"),
        OsStr::new("allow_other"),
        OsStr::new("custom_x"),
    ];

    let parsed = parse_mount_options(&input).expect("parse should succeed");

    assert_eq!(parsed.acl, SessionACL::All);

    assert!(matches!(parsed.mount_options[0], MountOption::RO));

    assert!(matches!(parsed.mount_options[1], MountOption::FSName(ref v) if v == "myfs"));

    assert!(matches!(parsed.mount_options[2], MountOption::Subtype(ref v) if v == "mysub"));

    assert!(matches!(parsed.mount_options[3], MountOption::CUSTOM(ref v) if v == "custom_x"));
}

#[test]

fn parses_fuser_style_dash_o_argument_shape() {
    let input = [
        OsStr::new("-o"),
        OsStr::new("ro,fsname=myfs"),
        OsStr::new("-odebug"),
    ];

    let parsed = parse_mount_options_from_args(&input).expect("parse should succeed");

    assert!(matches!(parsed.mount_options[0], MountOption::RO));

    assert!(matches!(parsed.mount_options[1], MountOption::FSName(ref v) if v == "myfs"));

    assert!(matches!(parsed.mount_options[2], MountOption::CUSTOM(ref v) if v == "debug"));
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

fn optional_probe_cache_policy_only_marks_enosys_unsupported() {
    let unsupported = classify_optional_probe_error(libc::ENOSYS);

    assert!(matches!(
        unsupported,
        OptionalProbeError::Unsupported { .. }
    ));
    assert_eq!(unsupported.ntstatus(), STATUS_NOT_IMPLEMENTED);

    for err in [libc::EPERM, libc::EACCES, libc::EINVAL, libc::ENOENT] {
        let outcome = classify_optional_probe_error(err);

        assert!(
            matches!(outcome, OptionalProbeError::RequestError { .. }),
            "err {err} must not disable capability"
        );
        assert_eq!(outcome.ntstatus(), errno_to_ntstatus(err));
    }
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
        fh: FileHandle(9),

        flags: crate::fuser_facade::types::FopenFlags::from_bits_truncate(7),

        ino: INodeNo(123),

        is_dir: true,

        lock_owner: None,

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

    assert_eq!(resolved.ino, INodeNo::ROOT);

    assert!(resolved.is_dir);
}

#[test]

fn ino_from_context_or_path_prefers_context_then_none() {
    let direct = AdapterContext {
        fh: FileHandle(1),

        flags: crate::fuser_facade::types::FopenFlags::empty(),

        ino: INodeNo(777),

        is_dir: false,

        lock_owner: None,

        request_ids: Default::default(),
    };

    assert_eq!(ino_from_context_or_path(&direct), Some(INodeNo(777)));

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
        MountOption::AutoUnmount,
        MountOption::DefaultPermissions,
        MountOption::Dev,
        MountOption::Suid,
        MountOption::Exec,
        MountOption::Atime,
        MountOption::DirSync,
        MountOption::Sync,
        MountOption::FSName("x".to_string()),
        MountOption::Subtype("y".to_string()),
    ];

    let mapped = to_dokan_mount_options(&opts);

    assert!(mapped.is_ok());
}

#[test]

fn windows_inexpressible_mount_options_are_explicit_and_noop_mapped() {
    let opts = [
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

    for opt in &opts {
        let mapped =
            to_dokan_mount_options(std::slice::from_ref(opt)).expect("mapping should succeed");

        assert_eq!(mapped.flags, baseline.flags);

        assert_eq!(mapped.single_thread, baseline.single_thread);

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

    assert_eq!(parsed.acl, SessionACL::All);

    assert_eq!(parsed.mount_options[0], MountOption::DefaultPermissions);

    assert_eq!(parsed.mount_options[1], MountOption::RW);
}

#[test]

fn parse_mount_options_from_args_skips_empty_segments_and_trims() {
    let input = [
        OsStr::new("-o"),
        OsStr::new("ro, ,fsname=myfs,,allow_other"),
    ];

    let parsed = parse_mount_options_from_args(&input).expect("parse should succeed");

    assert_eq!(parsed.acl, SessionACL::All);

    assert_eq!(parsed.mount_options.len(), 2);

    assert_eq!(parsed.mount_options[0], MountOption::RO);

    assert!(matches!(parsed.mount_options[1], MountOption::FSName(ref v) if v == "myfs"));
}

#[test]

fn parse_mount_options_from_args_maps_allow_root_to_acl() {
    let parsed =
        parse_mount_options_from_args(&[OsStr::new("allow_root")]).expect("parse should succeed");

    assert_eq!(parsed.acl, SessionACL::RootAndOwner);

    assert!(parsed.mount_options.is_empty());
}

#[test]

fn parse_mount_options_from_args_rejects_conflicting_acl() {
    let err = parse_mount_options_from_args(&[OsStr::new("allow_other,allow_root")])
        .expect_err("ACL conflict must be rejected");

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    assert!(err.to_string().contains("allow_other and allow_root"));
}

#[test]

fn rejects_conflicting_read_only_and_read_write_options() {
    let err = match to_dokan_mount_options(&[MountOption::RO, MountOption::RW]) {
        Ok(_) => panic!("RO/RW conflict must be rejected"),

        Err(err) => err,
    };

    assert_eq!(err.kind(), std::io::ErrorKind::InvalidInput);

    assert!(err.to_string().contains("ro and rw"));
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
        fh: FileHandle(11),

        flags: crate::fuser_facade::types::FopenFlags::empty(),

        ino: INodeNo(1),

        is_dir: false,

        lock_owner: Some(LockOwner(99)),

        request_ids: Default::default(),
    };

    assert_eq!(lock_owner_from_context(with_owner), Some(LockOwner(99)));

    let with_fh = AdapterContext {
        fh: FileHandle(11),

        flags: crate::fuser_facade::types::FopenFlags::empty(),

        ino: INodeNo(1),

        is_dir: false,

        lock_owner: None,

        request_ids: Default::default(),
    };

    assert_eq!(lock_owner_from_context(with_fh), None);

    let none = AdapterContext::default();

    assert_eq!(lock_owner_from_context(none), None);
}
