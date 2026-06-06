use super::*;

#[derive(Default)]
struct RenamePolicyFs {
    destination_exists: bool,
    lookup_called: AtomicUsize,
    rename_called: AtomicUsize,
}

impl Filesystem for RenamePolicyFs {
    fn lookup(&self, _req: &Request, _parent: INodeNo, _name: &OsStr, reply: ReplyEntry) {
        self.lookup_called.fetch_add(1, Ordering::SeqCst);
        if self.destination_exists {
            reply.entry(
                &std::time::Duration::from_secs(1),
                &test_file_attr(99),
                Generation(0),
            );
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn rename(
        &self, _req: &Request, _parent: INodeNo, _name: &OsStr, _newparent: INodeNo,
        _newname: &OsStr, _flags: RenameFlags, reply: ReplyEmpty,
    ) {
        self.rename_called.fetch_add(1, Ordering::SeqCst);
        reply.ok();
    }
}

#[derive(Default)]
struct LinkRouteFs {
    lookup_called: AtomicUsize,
    getattr_called: AtomicUsize,
    readlink_called: AtomicUsize,
    symlink_called: AtomicUsize,
    link_called: AtomicUsize,
    last_symlink: Mutex<Option<(INodeNo, String, String)>>,
    last_link: Mutex<Option<(INodeNo, INodeNo, String)>>,
}

impl Filesystem for LinkRouteFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.lookup_called.fetch_add(1, Ordering::SeqCst);
        assert_eq!(parent, INodeNo::ROOT);
        assert_eq!(name, OsStr::new("dir"));
        reply.entry(
            &Duration::from_secs(1),
            &test_attr_with_kind(41, FileType::Directory),
            Generation(0),
        );
    }

    fn readlink(&self, _req: &Request, ino: INodeNo, reply: ReplyData) {
        self.readlink_called.fetch_add(1, Ordering::SeqCst);
        assert_eq!(ino, INodeNo(77));
        reply.data(b"target.txt");
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        self.getattr_called.fetch_add(1, Ordering::SeqCst);
        let mut attr = test_attr_with_kind(ino.0, FileType::Directory);
        attr.size = (self.symlink_called.load(Ordering::SeqCst)
            + self.link_called.load(Ordering::SeqCst)) as u64;
        reply.attr(&Duration::from_secs(60), &attr);
    }

    fn symlink(
        &self, _req: &Request, parent: INodeNo, link_name: &OsStr, target: &Path, reply: ReplyEntry,
    ) {
        self.symlink_called.fetch_add(1, Ordering::SeqCst);
        *self.last_symlink.lock().expect("last symlink lock") = Some((
            parent,
            link_name.to_string_lossy().into_owned(),
            target.to_string_lossy().into_owned(),
        ));
        reply.entry(
            &Duration::from_secs(1),
            &test_attr_with_kind(78, FileType::Symlink),
            Generation(0),
        );
    }

    fn link(
        &self, _req: &Request, ino: INodeNo, newparent: INodeNo, newname: &OsStr, reply: ReplyEntry,
    ) {
        self.link_called.fetch_add(1, Ordering::SeqCst);
        *self.last_link.lock().expect("last link lock") =
            Some((ino, newparent, newname.to_string_lossy().into_owned()));
        reply.entry(&Duration::from_secs(1), &test_file_attr(79), Generation(0));
    }
}

#[test]
fn readlink_impl_routes_to_downstream_filesystem() {
    let adapter = test_adapter(LinkRouteFs::default());
    let req = request_kernel();

    let reply = adapter.readlink_impl(&req, INodeNo(77));

    assert!(matches!(reply.data.lock().expect("lock").clone(), Some(Ok(v)) if v == b"target.txt"));
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.readlink_called.load(Ordering::SeqCst), 1);
}

#[test]
fn symlink_path_impl_resolves_parent_and_routes_to_downstream_filesystem() {
    let adapter = test_adapter(LinkRouteFs::default());
    let req = request_kernel();
    let link_path = U16CString::from_str("\\dir\\ln").expect("link path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let parent_path = U16CString::from_str("\\dir").expect("parent path");
    let parent_ctx = AdapterContext {
        ino: INodeNo(41),
        ..Default::default()
    };

    let before = dokan::FileSystemHandler::get_file_information(
        &adapter,
        parent_path.as_ucstr(),
        &info,
        &parent_ctx,
    )
    .expect("seed parent attr cache");
    assert_eq!(before.file_size, 0);

    let reply = adapter.symlink_path_impl(&req, link_path.as_ucstr(), Path::new("target.txt"));

    let after = dokan::FileSystemHandler::get_file_information(
        &adapter,
        parent_path.as_ucstr(),
        &info,
        &parent_ctx,
    )
    .expect("symlink invalidates parent attr cache");
    assert_eq!(after.file_size, 1);

    assert!(
        matches!(*reply.status.lock().expect("lock"), Some(Ok(payload)) if payload.attr.ino == INodeNo(78))
    );
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
    assert_eq!(fs.symlink_called.load(Ordering::SeqCst), 1);
    assert_eq!(
        *fs.last_symlink.lock().expect("last symlink lock"),
        Some((INodeNo(41), "ln".to_string(), "target.txt".to_string()))
    );
}

#[test]
fn link_path_impl_resolves_parent_and_routes_to_downstream_filesystem() {
    let adapter = test_adapter(LinkRouteFs::default());
    let req = request_kernel();
    let new_path = U16CString::from_str("\\dir\\hard").expect("new path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let parent_path = U16CString::from_str("\\dir").expect("parent path");
    let parent_ctx = AdapterContext {
        ino: INodeNo(41),
        ..Default::default()
    };

    let before = dokan::FileSystemHandler::get_file_information(
        &adapter,
        parent_path.as_ucstr(),
        &info,
        &parent_ctx,
    )
    .expect("seed parent attr cache");
    assert_eq!(before.file_size, 0);

    let reply = adapter.link_path_impl(&req, INodeNo(9), new_path.as_ucstr());

    let after = dokan::FileSystemHandler::get_file_information(
        &adapter,
        parent_path.as_ucstr(),
        &info,
        &parent_ctx,
    )
    .expect("link invalidates parent attr cache");
    assert_eq!(after.file_size, 1);

    assert!(
        matches!(*reply.status.lock().expect("lock"), Some(Ok(payload)) if payload.attr.ino == INodeNo(79))
    );
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 2);
    assert_eq!(fs.link_called.load(Ordering::SeqCst), 1);
    assert_eq!(
        *fs.last_link.lock().expect("last link lock"),
        Some((INodeNo(9), INodeNo(41), "hard".to_string()))
    );
}

#[test]
fn rename_policy_routes_rename_without_collision_lookup() {
    let fs = RenamePolicyFs {
        destination_exists: true,
        ..Default::default()
    };
    let req = request_kernel();

    let result = rename_with_replace_policy(
        &fs,
        &req,
        INodeNo(1),
        OsStr::new("file.txt"),
        INodeNo(2),
        OsStr::new("file.txt"),
    );

    assert_eq!(result, Ok(()));
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 0);
    assert_eq!(fs.rename_called.load(Ordering::SeqCst), 1);
}

#[test]
fn rename_policy_renames_when_destination_is_missing() {
    let fs = RenamePolicyFs::default();
    let req = request_kernel();

    let result = rename_with_replace_policy(
        &fs,
        &req,
        INodeNo(1),
        OsStr::new("file.txt"),
        INodeNo(2),
        OsStr::new("file.txt"),
    );

    assert_eq!(result, Ok(()));
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 0);
    assert_eq!(fs.rename_called.load(Ordering::SeqCst), 1);
}

// -- facade lifecycle tests --

#[derive(Default)]
struct InitDestroyFs {
    init_called: usize,
    destroy_called: usize,
}

impl Filesystem for InitDestroyFs {
    fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> std::io::Result<()> {
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
        fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> std::io::Result<()> {
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

#[test]
fn dokan_signed_offsets_reject_negative_values_before_u64_conversion() {
    assert_eq!(
        nonnegative_i64_to_u64(-1),
        Err(winapi::shared::ntstatus::STATUS_INVALID_PARAMETER)
    );
    assert_eq!(
        checked_lock_range(-1, 1),
        Err(winapi::shared::ntstatus::STATUS_INVALID_PARAMETER)
    );
    assert_eq!(
        checked_lock_range(1, -1),
        Err(winapi::shared::ntstatus::STATUS_INVALID_PARAMETER)
    );
    assert_eq!(
        checked_lock_range(i64::MAX, 1),
        Err(winapi::shared::ntstatus::STATUS_INVALID_PARAMETER)
    );
    assert_eq!(checked_lock_range(5, 10), Ok((5, 15)));
}

#[test]
fn dokan_buffer_lengths_are_checked_before_u32_boundary_conversion() {
    assert_eq!(checked_dokan_len(7), Ok(7));
    assert_eq!(checked_dokan_len(u32::MAX as usize), Ok(u32::MAX));
    assert_eq!(
        checked_dokan_len(u32::MAX as usize + 1),
        Err(winapi::shared::ntstatus::STATUS_INVALID_PARAMETER)
    );
}
