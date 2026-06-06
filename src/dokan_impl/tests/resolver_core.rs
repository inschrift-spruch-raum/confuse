use super::*;

#[test]
fn contextless_delete_prechecks_resolve_through_path_resolver() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::delete_file(&adapter, path.as_ucstr(), &info, &ctx)
        .expect("contextless delete_file precheck resolves path");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.access_called.load(Ordering::SeqCst), 1);
}

#[test]
fn contextless_delete_directory_precheck_resolves_through_path_resolver() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        attr_kind: Some(FileType::Directory),
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let err = dokan::FileSystemHandler::delete_directory(&adapter, path.as_ucstr(), &info, &ctx)
        .expect_err(
            "contextless delete_directory precheck resolves path before rejecting non-empty dir",
        );
    assert_eq!(err, STATUS_DIRECTORY_NOT_EMPTY);

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.readdir_called.load(Ordering::SeqCst), 1);
}

#[test]
fn no_context_path_entries_reuse_entry_ttl_cache_until_expiry() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    for _ in 0..2 {
        let mut names = Vec::new();
        dokan::FileSystemHandler::find_files(
            &adapter,
            path.as_ucstr(),
            |entry| {
                names.push(entry.file_name.to_string_lossy());
                Ok(())
            },
            &info,
            &ctx,
        )
        .expect("path-only find_files resolves through TTL-aware resolver");
        assert_eq!(names, vec!["child.txt".to_string()]);
    }

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.readdir_called.load(Ordering::SeqCst), 2);
}

#[test]
fn zero_ttl_path_entries_are_not_cached() {
    let adapter = test_adapter(TtlResolverFs::default());
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    for _ in 0..2 {
        dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
            .expect("zero TTL path still resolves successfully");
    }

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 2);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 2);
}

#[test]
fn terminal_zero_ttl_lookup_is_forgotten_after_getattr_consumes_inode() {
    let adapter = test_adapter(TtlResolverFs {
        assert_no_forget_before_getattr: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
        .expect("terminal ttl=0 inode is consumed before forget");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn terminal_zero_ttl_lookup_is_forgotten_on_cached_attr_fast_path() {
    let adapter = test_adapter(TtlResolverFs {
        attr_ttl_secs: 60,
        assert_no_forget_before_getattr: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let context_bound = AdapterContext {
        ino: INodeNo(42),
        fh: FileHandle(7),
        ..Default::default()
    };
    let no_context = AdapterContext::default();

    dokan::FileSystemHandler::get_file_information(
        &adapter,
        path.as_ucstr(),
        &info,
        &context_bound,
    )
    .expect("seed live attr ttl through context-bound getattr");
    dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &no_context)
        .expect("terminal ttl=0 resolver lookup hits cached attr fast path then forgets");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn terminal_zero_ttl_lookup_is_forgotten_after_set_end_of_file_consumes_inode() {
    let adapter = test_adapter(TtlResolverFs {
        assert_no_forget_before_setattr: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::set_end_of_file(&adapter, path.as_ucstr(), 5, &info, &ctx)
        .expect("terminal ttl=0 path resolves then setattr consumes inode before forget");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.setattr_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn terminal_zero_ttl_lookup_is_forgotten_after_allocation_consumes_inode() {
    let adapter = test_adapter(TtlResolverFs {
        assert_no_forget_before_fallocate: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::set_allocation_size(&adapter, path.as_ucstr(), 5, &info, &ctx)
        .expect("terminal ttl=0 path resolves then fallocate consumes inode before forget");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.fallocate_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn terminal_zero_ttl_lookup_is_forgotten_after_setattr_consumes_inode() {
    let adapter = test_adapter(TtlResolverFs {
        assert_no_forget_before_setattr: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::set_end_of_file(&adapter, path.as_ucstr(), 12, &info, &ctx)
        .expect("terminal ttl=0 inode is consumed by setattr before forget");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.setattr_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn terminal_zero_ttl_lookup_is_forgotten_after_fallocate_consumes_inode() {
    let adapter = test_adapter(TtlResolverFs {
        assert_no_forget_before_fallocate: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::set_allocation_size(&adapter, path.as_ucstr(), 12, &info, &ctx)
        .expect("terminal ttl=0 inode is consumed by fallocate before forget");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.fallocate_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn terminal_zero_ttl_lookup_is_forgotten_after_set_attributes_consumes_inode() {
    let adapter = test_adapter(TtlResolverFs {
        assert_no_forget_before_setattr: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::set_file_attributes(
        &adapter,
        path.as_ucstr(),
        FILE_ATTRIBUTE_READONLY,
        &info,
        &ctx,
    )
    .expect("terminal ttl=0 path resolves then setattr consumes inode before forget");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.setattr_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn terminal_zero_ttl_lookup_is_forgotten_after_set_time_consumes_inode() {
    let adapter = test_adapter(TtlResolverFs {
        assert_no_forget_before_setattr: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::set_file_time(
        &adapter,
        path.as_ucstr(),
        dokan::FileTimeOperation::DontChange,
        dokan::FileTimeOperation::DontChange,
        dokan::FileTimeOperation::DontChange,
        &info,
        &ctx,
    )
    .expect("terminal ttl=0 path resolves then setattr consumes inode before forget");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.setattr_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn missing_path_uses_ttl_bound_negative_cache() {
    let adapter = test_adapter(TtlResolverFs {
        missing: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    for _ in 0..2 {
        let err = dokan::FileSystemHandler::find_files(
            &adapter,
            path.as_ucstr(),
            |_| Ok(()),
            &info,
            &ctx,
        )
        .expect_err("missing path remains not found");
        assert_eq!(err, STATUS_OBJECT_NAME_NOT_FOUND);
    }

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
}

#[test]
fn entry_ttl_expiry_refreshes_lookup_and_forgets_old_entry() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 1,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("initial lookup caches entry");
    std::thread::sleep(Duration::from_millis(1100));
    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("expired entry TTL refreshes lookup");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 2);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn negative_path_cache_expires_and_allows_later_lookup() {
    let adapter = test_adapter(TtlResolverFs {
        missing: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let err =
        dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
            .expect_err("initial missing path seeds negative cache");
    assert_eq!(err, STATUS_OBJECT_NAME_NOT_FOUND);
    {
        let mut fs = adapter.fs.lock().expect("fs lock");
        fs.missing = false;
    }
    std::thread::sleep(Duration::from_millis(1100));
    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("expired negative cache allows fresh lookup");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 2);
}

#[test]
fn attr_ttl_cache_is_separate_from_entry_ttl_cache() {
    let adapter = test_adapter(TtlResolverFs {
        attr_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext {
        ino: INodeNo(42),
        fh: FileHandle(7),
        ..Default::default()
    };

    for _ in 0..2 {
        let file_info =
            dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
                .expect("context-bound get_file_information uses attr TTL cache");
        assert_eq!(file_info.file_index, 42);
    }

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 0);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 1);
}

#[test]
fn attr_ttl_expiry_refreshes_getattr() {
    let adapter = test_adapter(TtlResolverFs {
        attr_ttl_secs: 1,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext {
        ino: INodeNo(42),
        fh: FileHandle(7),
        ..Default::default()
    };

    dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
        .expect("initial getattr caches attr TTL");
    std::thread::sleep(Duration::from_millis(1100));
    dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
        .expect("expired attr TTL refreshes getattr");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
}

#[test]
fn lookup_entry_ttl_does_not_seed_attr_ttl_cache() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        attr_ttl_secs: 0,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    for _ in 0..2 {
        let file_info =
            dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
                .expect("entry TTL resolves path but does not cache attr TTL");
        assert_eq!(file_info.file_index, 42);
    }

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
}

#[test]
fn lookup_entry_refresh_does_not_overwrite_live_attr_ttl_cache() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        attr_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let attr_ctx = AdapterContext {
        ino: INodeNo(42),
        fh: FileHandle(7),
        ..Default::default()
    };
    let path_ctx = AdapterContext::default();

    {
        let fs = adapter.fs.lock().expect("fs lock");
        fs.write_called.store(9, Ordering::SeqCst);
    }
    let cached_attr =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &attr_ctx)
            .expect("initial getattr seeds live attr TTL");
    assert_eq!(cached_attr.file_size, 9);

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &path_ctx)
        .expect("lookup entry refresh must not replace live attr TTL payload");
    let after_entry_refresh =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &attr_ctx)
            .expect("live attr TTL still returns original getattr payload");

    assert_eq!(after_entry_refresh.file_size, 9);
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
}

#[test]
fn positive_to_negative_transition_invalidates_live_attr_ttl_cache() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        attr_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let path_ctx = AdapterContext::default();
    let attr_ctx = AdapterContext {
        ino: INodeNo(42),
        fh: FileHandle(7),
        ..Default::default()
    };

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &path_ctx)
        .expect("initial lookup caches positive entry");
    dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &attr_ctx)
        .expect("initial getattr seeds attr TTL");
    adapter
        .resolver
        .lock()
        .expect("resolver lock")
        .remember_negative(
            OsStr::new("\\dir"),
            INodeNo::ROOT,
            Generation(0),
            OsStr::new("dir"),
        );
    let pending_forgets = adapter
        .resolver
        .lock()
        .expect("resolver lock")
        .take_pending_forgets();
    assert_eq!(pending_forgets, vec![(INodeNo(42), 1)]);
    {
        let fs = adapter.fs.lock().expect("fs lock");
        fs.write_called.store(3, Ordering::SeqCst);
    }

    let refreshed =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &attr_ctx)
            .expect("negative transition invalidates stale attr TTL");

    assert_eq!(refreshed.file_size, 3);
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 3);
}

#[test]
fn contextless_flush_read_and_write_resolve_through_path_resolver() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::flush_file_buffers(&adapter, path.as_ucstr(), &info, &ctx)
        .expect("contextless flush resolves path");
    let mut buf = [0_u8; 3];
    let read =
        dokan::FileSystemHandler::read_file(&adapter, path.as_ucstr(), 0, &mut buf, &info, &ctx)
            .expect("contextless read resolves path");
    let written =
        dokan::FileSystemHandler::write_file(&adapter, path.as_ucstr(), 0, b"abc", &info, &ctx)
            .expect("contextless write resolves path");

    assert_eq!(read, 3);
    assert_eq!(buf, *b"xxx");
    assert_eq!(written, 3);
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.fsync_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.read_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.write_called.load(Ordering::SeqCst), 1);
}

#[test]
fn write_invalidates_handle_bound_attr_ttl_cache() {
    let adapter = test_adapter(TtlResolverFs {
        attr_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext {
        ino: INodeNo(42),
        fh: FileHandle(7),
        ..Default::default()
    };

    let before =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
            .expect("initial getattr seeds attr cache");
    assert_eq!(before.file_size, 0);

    dokan::FileSystemHandler::write_file(&adapter, path.as_ucstr(), 0, b"x", &info, &ctx)
        .expect("write succeeds and invalidates attr cache");

    let after =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
            .expect("post-write getattr refreshes stale attr");
    assert_eq!(after.file_size, 1);

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
}

#[test]
fn write_invalidates_cached_path_and_forgets_lookup() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("initial path lookup populates resolver cache");
    dokan::FileSystemHandler::write_file(&adapter, path.as_ucstr(), 0, b"x", &info, &ctx)
        .expect("write invalidates cached path");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn write_invalidates_parent_directory_attr_ttl_cache() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        attr_ttl_secs: 60,
        ..Default::default()
    });
    let root = U16CString::from_str("\\").expect("root path");
    let child = U16CString::from_str("\\dir").expect("child path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let root_ctx = AdapterContext::default();
    let child_ctx = AdapterContext::default();

    let before =
        dokan::FileSystemHandler::get_file_information(&adapter, root.as_ucstr(), &info, &root_ctx)
            .expect("initial root getattr seeds parent attr cache");
    assert_eq!(before.file_size, 0);

    dokan::FileSystemHandler::write_file(&adapter, child.as_ucstr(), 0, b"x", &info, &child_ctx)
        .expect("write invalidates parent attr cache");

    let after =
        dokan::FileSystemHandler::get_file_information(&adapter, root.as_ucstr(), &info, &root_ctx)
            .expect("parent getattr refreshes after child write");
    assert_eq!(after.file_size, 1);

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
}
