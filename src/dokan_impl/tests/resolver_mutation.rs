use super::*;
use crate::dokan_impl::adapter::PositivePathRecord;

#[derive(Default)]
struct CasePreservingResolverFs {
    lookup_called: AtomicUsize,
}

impl Filesystem for CasePreservingResolverFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.lookup_called.fetch_add(1, Ordering::SeqCst);
        assert_eq!(parent, INodeNo::ROOT);
        assert_eq!(name, OsStr::new("MiXeD"));
        reply.entry(
            &Duration::from_secs(60),
            &test_attr_with_kind(42, FileType::Directory),
            Generation(7),
        );
    }

    fn readdir(
        &self, _req: &Request, ino: INodeNo, _fh: FileHandle, _offset: u64,
        mut reply: ReplyDirectory,
    ) {
        assert_eq!(ino, INodeNo(42));
        reply.add(
            INodeNo(43),
            1,
            FileType::RegularFile,
            OsStr::new("child.txt"),
        );
        reply.ok();
    }
}

#[test]
fn contextless_get_file_information_resolves_through_path_resolver() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let file_info =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
            .expect("contextless get_file_information resolves by path");

    assert_eq!(file_info.file_index, 42);
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
}

#[test]
fn create_clears_negative_path_cache_and_remembers_created_entry() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        missing: true,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let mut info = dokan::OperationInfo::new(&mut raw_info);
    let security_context: dokan::IO_SECURITY_CONTEXT = unsafe { std::mem::zeroed() };
    let ctx = AdapterContext::default();

    let err =
        dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
            .expect_err("initial missing path seeds negative cache");
    assert_eq!(err, STATUS_OBJECT_NAME_NOT_FOUND);

    dokan::FileSystemHandler::create_file(
        &adapter,
        path.as_ucstr(),
        &security_context,
        GENERIC_READ,
        0,
        0,
        FILE_CREATE,
        0,
        &mut info,
    )
    .expect("create records new resolver entry");

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("created path is served from resolver despite previous negative cache");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.create_called.load(Ordering::SeqCst), 1);
}

#[test]
fn create_success_invalidates_existing_inode_attr_cache() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        attr_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let mut info = dokan::OperationInfo::new(&mut raw_info);
    let security_context: dokan::IO_SECURITY_CONTEXT = unsafe { std::mem::zeroed() };
    let ctx = AdapterContext {
        ino: INodeNo(42),
        fh: FileHandle(7),
        ..Default::default()
    };

    let before =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
            .expect("seed pre-create attr cache");
    assert_eq!(before.file_size, 0);
    {
        let fs = adapter.fs.lock().expect("fs lock");
        fs.write_called.store(5, Ordering::SeqCst);
    }
    dokan::FileSystemHandler::create_file(
        &adapter,
        path.as_ucstr(),
        &security_context,
        GENERIC_READ,
        0,
        0,
        FILE_SUPERSEDE,
        0,
        &mut info,
    )
    .expect("create/supersede invalidates inode attr cache");
    let after =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
            .expect("post-create getattr refreshes invalidated attr");

    assert_eq!(after.file_size, 5);
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.create_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
}

#[test]
fn open_existing_uses_cached_resolver_entry_for_leaf() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let mut info = dokan::OperationInfo::new(&mut raw_info);
    let security_context: dokan::IO_SECURITY_CONTEXT = unsafe { std::mem::zeroed() };
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("initial path lookup populates resolver cache");
    dokan::FileSystemHandler::create_file(
        &adapter,
        path.as_ucstr(),
        &security_context,
        GENERIC_READ,
        0,
        0,
        dokan_sys::win32::FILE_OPEN,
        0,
        &mut info,
    )
    .expect("open existing uses resolver cache");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.open_called.load(Ordering::SeqCst), 1);
}

#[test]
fn resolver_preserves_original_component_spelling_for_backend_lookup() {
    let adapter = test_adapter(CasePreservingResolverFs::default());
    let path = U16CString::from_str("\\MiXeD").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("mixed-case component is passed unchanged to backend lookup");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
}

#[test]
fn rename_invalidates_path_cache_and_best_effort_forgets_cached_lookup() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let old_path = U16CString::from_str("\\dir").expect("old path");
    let new_path = U16CString::from_str("\\renamed").expect("new path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::find_files(&adapter, old_path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("initial path lookup populates resolver cache");
    assert_eq!(
        adapter
            .fs
            .lock()
            .expect("fs lock")
            .lookup_called
            .load(Ordering::SeqCst),
        1
    );

    dokan::FileSystemHandler::move_file(
        &adapter,
        old_path.as_ucstr(),
        new_path.as_ucstr(),
        true,
        &info,
        &ctx,
    )
    .expect("rename succeeds and invalidates resolver cache");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.rename_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_called.load(Ordering::SeqCst), 1);
}

#[test]
fn no_context_rename_invalidates_moved_inode_attr_cache() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        attr_ttl_secs: 60,
        ..Default::default()
    });
    let old_path = U16CString::from_str("\\dir").expect("old path");
    let new_path = U16CString::from_str("\\renamed").expect("new path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let no_ctx = AdapterContext::default();
    let moved_ctx = AdapterContext {
        ino: INodeNo(42),
        ..Default::default()
    };

    let before = dokan::FileSystemHandler::get_file_information(
        &adapter,
        old_path.as_ucstr(),
        &info,
        &no_ctx,
    )
    .expect("seed moved inode attr cache through path resolver");
    assert_eq!(before.file_size, 0);

    {
        let fs = adapter.fs.lock().expect("fs lock");
        fs.write_called.store(1, Ordering::SeqCst);
    }
    dokan::FileSystemHandler::move_file(
        &adapter,
        old_path.as_ucstr(),
        new_path.as_ucstr(),
        true,
        &info,
        &no_ctx,
    )
    .expect("no-context rename captures moved inode before mutation");

    let after = dokan::FileSystemHandler::get_file_information(
        &adapter,
        new_path.as_ucstr(),
        &info,
        &moved_ctx,
    )
    .expect("moved inode attr cache refreshes after rename");
    assert_eq!(after.file_size, 1);

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.rename_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
}

#[test]
fn rename_collision_check_uses_resolver_and_keeps_lookup_ref_accounted() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let source_path = U16CString::from_str("\\dir").expect("source path");
    let target_path = U16CString::from_str("\\target").expect("target path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let err = dokan::FileSystemHandler::move_file(
        &adapter,
        source_path.as_ucstr(),
        target_path.as_ucstr(),
        false,
        &info,
        &ctx,
    )
    .expect_err("existing target reports collision");
    assert_eq!(err, STATUS_OBJECT_NAME_COLLISION);

    dokan::FileSystemHandler::move_file(
        &adapter,
        target_path.as_ucstr(),
        U16CString::from_str("\\renamed")
            .expect("rename path")
            .as_ucstr(),
        true,
        &info,
        &ctx,
    )
    .expect("cached collision target remains tracked and forgettable");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 2);
    assert_eq!(fs.rename_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 1);
}

#[test]
fn zero_ttl_rename_collision_probe_forgets_before_returning_collision() {
    let adapter = test_adapter(TtlResolverFs::default());
    let source_path = U16CString::from_str("\\dir").expect("source path");
    let target_path = U16CString::from_str("\\target").expect("target path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let err = dokan::FileSystemHandler::move_file(
        &adapter,
        source_path.as_ucstr(),
        target_path.as_ucstr(),
        false,
        &info,
        &ctx,
    )
    .expect_err("existing ttl=0 target reports collision");

    assert_eq!(err, STATUS_OBJECT_NAME_COLLISION);
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 2);
    assert_eq!(fs.rename_called.load(Ordering::SeqCst), 0);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 2);
}

#[test]
fn rename_clears_stale_negative_target_cache() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        missing: true,
        ..Default::default()
    });
    let target_path = U16CString::from_str("\\dir").expect("target path");
    let source_path = U16CString::from_str("\\source").expect("source path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let err = dokan::FileSystemHandler::find_files(
        &adapter,
        target_path.as_ucstr(),
        |_| Ok(()),
        &info,
        &ctx,
    )
    .expect_err("initial missing target seeds negative cache");
    assert_eq!(err, STATUS_OBJECT_NAME_NOT_FOUND);

    {
        let mut fs = adapter.fs.lock().expect("fs lock");
        fs.missing = false;
    }
    dokan::FileSystemHandler::move_file(
        &adapter,
        source_path.as_ucstr(),
        target_path.as_ucstr(),
        true,
        &info,
        &ctx,
    )
    .expect("rename invalidates stale negative target");
    dokan::FileSystemHandler::find_files(&adapter, target_path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("target path is looked up after negative invalidation");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 3);
    assert_eq!(fs.rename_called.load(Ordering::SeqCst), 1);
}

#[test]
fn repeated_lookup_replacement_forgets_every_cached_lookup_ref() {
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
    adapter
        .resolver
        .lock()
        .expect("resolver lock")
        .remember_positive(PositivePathRecord {
            path: OsStr::new("\\dir"),
            parent: INodeNo::ROOT,
            parent_generation: Generation(0),
            name: OsStr::new("dir"),
            attr: test_file_attr(42),
            generation: Generation(8),
            ttl: Duration::from_secs(60),
        });

    dokan::FileSystemHandler::move_file(
        &adapter,
        path.as_ucstr(),
        U16CString::from_str("\\renamed")
            .expect("new path")
            .as_ucstr(),
        true,
        &info,
        &ctx,
    )
    .expect("rename invalidates resolver cache");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 2);
}

#[test]
fn replacing_cached_path_with_different_inode_drains_old_lookup_ref() {
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
    adapter
        .resolver
        .lock()
        .expect("resolver lock")
        .remember_positive(PositivePathRecord {
            path: OsStr::new("\\dir"),
            parent: INodeNo::ROOT,
            parent_generation: Generation(0),
            name: OsStr::new("dir"),
            attr: test_file_attr(44),
            generation: Generation(9),
            ttl: Duration::from_secs(60),
        });
    dokan::FileSystemHandler::move_file(
        &adapter,
        path.as_ucstr(),
        U16CString::from_str("\\renamed")
            .expect("new path")
            .as_ucstr(),
        true,
        &info,
        &ctx,
    )
    .expect("rename invalidates resolver cache");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 2);
}

#[test]
fn expired_positive_drains_forget_even_when_refresh_lookup_fails() {
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
    {
        let mut fs = adapter.fs.lock().expect("fs lock");
        fs.lookup_error = Some(Errno::EIO);
    }
    adapter
        .resolver
        .lock()
        .expect("resolver lock")
        .remember_positive(PositivePathRecord {
            path: OsStr::new("\\dir"),
            parent: INodeNo::ROOT,
            parent_generation: Generation(0),
            name: OsStr::new("dir"),
            attr: test_file_attr(42),
            generation: Generation(8),
            ttl: Duration::ZERO,
        });

    let err =
        dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
            .expect_err("lookup refresh failure is still surfaced");
    assert_eq!(err, STATUS_UNSUCCESSFUL);

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 2);
}

#[test]
fn expired_positive_is_reaped_before_unrelated_path_resolution() {
    let adapter = test_adapter(TtlResolverFs {
        entry_ttl_secs: 60,
        ..Default::default()
    });
    let path = U16CString::from_str("\\dir").expect("path");
    let other = U16CString::from_str("\\target").expect("other path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("initial path lookup populates resolver cache");
    adapter
        .resolver
        .lock()
        .expect("resolver lock")
        .remember_positive(PositivePathRecord {
            path: OsStr::new("\\dir"),
            parent: INodeNo::ROOT,
            parent_generation: Generation(0),
            name: OsStr::new("dir"),
            attr: test_file_attr(42),
            generation: Generation(8),
            ttl: Duration::ZERO,
        });

    dokan::FileSystemHandler::get_file_information(&adapter, other.as_ucstr(), &info, &ctx)
        .expect("unrelated path resolution reaps expired resolver entries");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.forget_lookup_total.load(Ordering::SeqCst), 2);
}

#[test]
fn notifier_style_entry_invalidation_clears_negative_cache() {
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
    {
        let fs = adapter.fs.lock().expect("fs lock");
        let req = request_kernel();
        adapter.invalidate_entry_cache(&*fs, &req, INodeNo::ROOT, OsStr::new("dir"));
    }

    dokan::FileSystemHandler::find_files(&adapter, path.as_ucstr(), |_| Ok(()), &info, &ctx)
        .expect("entry invalidation clears stale negative cache");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 2);
}

#[derive(Default)]
struct ChangingParentFs {
    lookup_called: AtomicUsize,
    parent_lookup_called: AtomicUsize,
    child_lookup_called: AtomicUsize,
    child_parent_seen: Mutex<Vec<INodeNo>>,
}

#[derive(Default)]
struct ZeroTtlParentFs {
    child_lookup_called: AtomicUsize,
    forget_called: AtomicUsize,
}

impl Filesystem for ZeroTtlParentFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        match (parent, name.to_string_lossy().as_ref()) {
            (INodeNo::ROOT, "dir") => reply.entry(
                &Duration::ZERO,
                &test_attr_with_kind(42, FileType::Directory),
                Generation(1),
            ),
            (INodeNo(42), "child") => {
                assert_eq!(self.forget_called.load(Ordering::SeqCst), 0);
                self.child_lookup_called.fetch_add(1, Ordering::SeqCst);
                reply.entry(&Duration::from_secs(60), &test_file_attr(45), Generation(2));
            }
            _ => reply.error(Errno::ENOENT),
        }
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        reply.attr(&Duration::ZERO, &test_file_attr(ino.0));
    }

    fn forget(&self, _req: &Request, ino: INodeNo, nlookup: u64) {
        assert_eq!(ino, INodeNo(42));
        assert_eq!(nlookup, 1);
        self.forget_called.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn zero_ttl_parent_lookup_is_forgotten_after_descendant_resolution() {
    let adapter = test_adapter(ZeroTtlParentFs::default());
    let path = U16CString::from_str("\\dir\\child").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let file_info =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
            .expect("ttl=0 parent survives until child lookup completes");

    assert_eq!(file_info.file_index, 45);
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.child_lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.forget_called.load(Ordering::SeqCst), 1);
}

impl Filesystem for ChangingParentFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.lookup_called.fetch_add(1, Ordering::SeqCst);
        match (parent, name.to_string_lossy().as_ref()) {
            (INodeNo::ROOT, "dir") => {
                let call = self.parent_lookup_called.fetch_add(1, Ordering::SeqCst);
                let ino = if call == 0 { 42 } else { 44 };
                reply.entry(
                    &Duration::from_secs(60),
                    &test_attr_with_kind(ino, FileType::Directory),
                    Generation(call as u64),
                );
            }
            (INodeNo(42) | INodeNo(44), "child") => {
                self.child_lookup_called.fetch_add(1, Ordering::SeqCst);
                self.child_parent_seen
                    .lock()
                    .expect("child parent lock")
                    .push(parent);
                reply.entry(&Duration::from_secs(60), &test_file_attr(45), Generation(7));
            }
            _ => reply.error(Errno::ENOENT),
        }
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        reply.attr(&Duration::from_secs(1), &test_file_attr(ino.0));
    }
}

#[test]
fn descendant_cache_is_rejected_when_refreshed_parent_inode_changes() {
    let adapter = test_adapter(ChangingParentFs::default());
    let path = U16CString::from_str("\\dir\\child").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let first =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
            .expect("initial descendant resolves");
    assert_eq!(first.file_index, 45);

    adapter
        .resolver
        .lock()
        .expect("resolver lock")
        .remember_positive(PositivePathRecord {
            path: OsStr::new("\\dir"),
            parent: INodeNo::ROOT,
            parent_generation: Generation(0),
            name: OsStr::new("dir"),
            attr: test_attr_with_kind(44, FileType::Directory),
            generation: Generation(1),
            ttl: Duration::from_secs(60),
        });

    let second =
        dokan::FileSystemHandler::get_file_information(&adapter, path.as_ucstr(), &info, &ctx)
            .expect("descendant refreshes below changed parent");
    assert_eq!(second.file_index, 45);

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.child_lookup_called.load(Ordering::SeqCst), 2);
    assert_eq!(
        *fs.child_parent_seen.lock().expect("child parent lock"),
        vec![INodeNo(42), INodeNo(44)]
    );
}
