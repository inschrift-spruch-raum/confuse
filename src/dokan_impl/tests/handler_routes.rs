use super::*;

#[derive(Default)]
struct RouteFs {
    release_called: AtomicUsize,
    releasedir_called: AtomicUsize,
    flush_called: AtomicUsize,
    last_flush_owner: Mutex<Option<LockOwner>>,
    fsync_called: AtomicUsize,
    fsyncdir_called: AtomicUsize,
}

impl Filesystem for RouteFs {
    fn release(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: OpenFlags,
        _lock_owner: Option<LockOwner>, _flush: bool, _reply: ReplyEmpty,
    ) {
        self.release_called.fetch_add(1, Ordering::SeqCst);
    }

    fn releasedir(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: OpenFlags,
        _reply: ReplyEmpty,
    ) {
        self.releasedir_called.fetch_add(1, Ordering::SeqCst);
    }

    fn fsync(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _datasync: bool, _reply: ReplyEmpty,
    ) {
        self.fsync_called.fetch_add(1, Ordering::SeqCst);
    }

    fn flush(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, lock_owner: LockOwner,
        _reply: ReplyEmpty,
    ) {
        self.flush_called.fetch_add(1, Ordering::SeqCst);
        *self.last_flush_owner.lock().expect("flush owner lock") = Some(lock_owner);
    }

    fn fsyncdir(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _datasync: bool, _reply: ReplyEmpty,
    ) {
        self.fsyncdir_called.fetch_add(1, Ordering::SeqCst);
    }
}

#[test]
fn close_route_splits_file_vs_directory() {
    let fs = RouteFs::default();
    let req = request_kernel();
    close_with_context(
        &fs,
        &req,
        AdapterContext {
            fh: FileHandle(1),
            flags: crate::fuser_facade::types::FopenFlags::empty(),
            ino: INodeNo(2),
            is_dir: false,
            lock_owner: None,
            request_ids: Default::default(),
        },
    );
    close_with_context(
        &fs,
        &req,
        AdapterContext {
            fh: FileHandle(1),
            flags: crate::fuser_facade::types::FopenFlags::empty(),
            ino: INodeNo(2),
            is_dir: true,
            lock_owner: None,
            request_ids: Default::default(),
        },
    );
    assert_eq!(fs.release_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.releasedir_called.load(Ordering::SeqCst), 1);
}

#[test]
fn flush_route_splits_file_vs_directory() {
    let fs = RouteFs::default();
    let req = request_kernel();
    flush_with_context(
        &fs,
        &req,
        AdapterContext {
            fh: FileHandle(1),
            flags: crate::fuser_facade::types::FopenFlags::empty(),
            ino: INodeNo(2),
            is_dir: false,
            lock_owner: Some(LockOwner(77)),
            request_ids: Default::default(),
        },
        ReplyEmpty::capture(),
    );
    flush_with_context(
        &fs,
        &req,
        AdapterContext {
            fh: FileHandle(1),
            flags: crate::fuser_facade::types::FopenFlags::empty(),
            ino: INodeNo(2),
            is_dir: true,
            lock_owner: None,
            request_ids: Default::default(),
        },
        ReplyEmpty::capture(),
    );
    assert_eq!(fs.fsync_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.flush_called.load(Ordering::SeqCst), 0);
    assert_eq!(*fs.last_flush_owner.lock().expect("flush owner lock"), None);
    assert_eq!(fs.fsyncdir_called.load(Ordering::SeqCst), 1);
}

struct DeletePrecheckFs {
    kind: Mutex<FileType>,
    children: Mutex<Vec<(INodeNo, FileType, String)>>,
    access_errno: Mutex<Option<Errno>>,
}

impl Default for DeletePrecheckFs {
    fn default() -> Self {
        Self {
            kind: Mutex::new(FileType::RegularFile),
            children: Mutex::new(Vec::new()),
            access_errno: Mutex::new(None),
        }
    }
}

impl Filesystem for DeletePrecheckFs {
    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        let kind = *self.kind.lock().expect("kind lock");
        reply.attr(&Duration::from_secs(1), &test_attr_with_kind(ino.0, kind));
    }

    fn access(
        &self, _req: &Request, _ino: INodeNo, _mask: crate::fuser_facade::types::AccessFlags,
        reply: ReplyEmpty,
    ) {
        match *self.access_errno.lock().expect("access lock") {
            Some(err) => reply.error(err),
            None => reply.ok(),
        }
    }

    fn readdir(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64,
        mut reply: ReplyDirectory,
    ) {
        for (ino, kind, name) in self.children.lock().expect("children lock").iter() {
            reply.add(*ino, 1, *kind, OsStr::new(name));
        }
        reply.ok();
    }
}

#[test]
fn delete_prechecks_reject_wrong_type_permissions_and_non_empty_dirs() {
    let req = request_kernel();
    let fs = DeletePrecheckFs::default();
    *fs.kind.lock().expect("kind lock") = FileType::RegularFile;
    assert_eq!(precheck_file_delete(&fs, &req, INodeNo(2)), Ok(()));

    *fs.access_errno.lock().expect("access lock") = Some(Errno::EACCES);
    assert_eq!(
        precheck_file_delete(&fs, &req, INodeNo(2)),
        Err(STATUS_ACCESS_DENIED)
    );
    *fs.access_errno.lock().expect("access lock") = None;

    *fs.kind.lock().expect("kind lock") = FileType::Directory;
    assert_eq!(
        precheck_file_delete(&fs, &req, INodeNo(3)),
        Err(STATUS_FILE_IS_A_DIRECTORY)
    );
    assert_eq!(precheck_directory_delete(&fs, &req, INodeNo(3)), Ok(()));

    fs.children.lock().expect("children lock").push((
        INodeNo(4),
        FileType::RegularFile,
        "child.txt".to_string(),
    ));
    assert_eq!(
        precheck_directory_delete(&fs, &req, INodeNo(3)),
        Err(STATUS_DIRECTORY_NOT_EMPTY)
    );
}

#[derive(Default)]
struct WritePlanFs {
    size: Mutex<u64>,
}

impl Filesystem for WritePlanFs {
    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        let mut attr = test_file_attr(ino.0);
        attr.size = *self.size.lock().expect("size lock");
        reply.attr(&Duration::from_secs(1), &attr);
    }
}

#[test]
fn write_plan_handles_eof_and_paging_io_without_extending() {
    let fs = WritePlanFs {
        size: Mutex::new(10),
    };
    let req = request_kernel();
    let ctx = AdapterContext {
        ino: INodeNo(2),
        fh: FileHandle(3),
        ..Default::default()
    };

    assert_eq!(
        dokan_write_plan(&fs, &req, ctx, 4, 8, false, false),
        Ok((4, 8))
    );
    assert_eq!(
        dokan_write_plan(&fs, &req, ctx, 4, 8, true, false),
        Ok((10, 8))
    );
    assert_eq!(
        dokan_write_plan(&fs, &req, ctx, 7, 8, false, true),
        Ok((7, 3))
    );
    assert_eq!(
        dokan_write_plan(&fs, &req, ctx, 10, 8, false, true),
        Ok((10, 0))
    );
}

#[derive(Default)]
struct AllocationRouteFs {
    fallocate_called: AtomicUsize,
    setattr_called: AtomicUsize,
    last_fallocate: Mutex<Option<(u64, i32)>>,
}

impl Filesystem for AllocationRouteFs {
    fn setattr(
        &self, _req: &Request, _ino: INodeNo, _mode: Option<u32>, _uid: Option<u32>,
        _gid: Option<u32>, _size: Option<u64>,
        _atime: Option<crate::fuser_facade::types::TimeOrNow>,
        _mtime: Option<crate::fuser_facade::types::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>, _fh: Option<FileHandle>,
        _crtime: Option<std::time::SystemTime>, _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<crate::fuser_facade::types::BsdFileFlags>, reply: ReplyAttr,
    ) {
        self.setattr_called.fetch_add(1, Ordering::SeqCst);
        reply.attr(&Duration::from_secs(1), &test_file_attr(_ino.0));
    }

    fn fallocate(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, _length: u64,
        _mode: i32, reply: ReplyEmpty,
    ) {
        self.fallocate_called.fetch_add(1, Ordering::SeqCst);
        *self.last_fallocate.lock().expect("fallocate lock") = Some((_length, _mode));
        reply.ok();
    }
}

#[test]
fn allocation_size_routes_to_fallocate_not_setattr_size() {
    let fs = AllocationRouteFs::default();
    let req = request_kernel();
    let reply = ReplyEmpty::capture();
    allocation_size_with_context(
        &fs,
        &req,
        AdapterContext {
            ino: INodeNo(2),
            fh: FileHandle(3),
            ..Default::default()
        },
        4096,
        reply.duplicate(),
    )
    .expect("nonnegative allocation size");

    assert_eq!(fs.fallocate_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.setattr_called.load(Ordering::SeqCst), 0);
    assert_eq!(
        *fs.last_fallocate.lock().expect("fallocate lock"),
        Some((4096, 1))
    );
    assert!(matches!(
        *reply.status.lock().expect("reply lock"),
        Some(Ok(()))
    ));
}

#[test]
fn split_parent_and_name_accepts_slash_separated_windows_paths() {
    let path = U16CString::from_str("C:/dir/file.txt").expect("path");
    let (parent, leaf) = split_parent_and_name(path.as_ucstr());

    assert_eq!(parent, OsStr::new("\\dir"));
    assert_eq!(leaf, OsStr::new("file.txt"));
}
