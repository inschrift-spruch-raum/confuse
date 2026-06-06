use super::super::reply::*;
use super::super::request::Request;
use super::super::request::request_kernel;
use super::super::types::*;
use super::super::{PollHandle, PollNotifier};
use super::*;
use crate::dokan_impl::{LOCK_TYPE_WRLCK, default_kernel_config};
use libc::ENOSYS;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, UNIX_EPOCH};

#[derive(Default)]
struct DefaultFs;

impl Filesystem for DefaultFs {}

#[test]
fn filesystem_default_methods_match_fuser_style_error_contract() {
    let mut fs = DefaultFs;
    let req = request_kernel();

    let mut config = default_kernel_config();
    let init = fs
        .init(&req, &mut config)
        .expect_err("default init is unimplemented");
    assert_eq!(init.raw_os_error(), Some(ENOSYS));

    let lookup = ReplyEntry::capture();
    fs.lookup(&req, INodeNo(1), OsStr::new("x"), lookup.duplicate());
    assert!(*lookup.status.lock().expect("lock") == Some(Err(ENOSYS)));

    let getattr = ReplyAttr::capture();
    fs.getattr(&req, INodeNo(1), None, getattr.duplicate());
    assert!(*getattr.status.lock().expect("lock") == Some(Err(ENOSYS)));

    let symlink = ReplyEntry::capture();
    fs.symlink(
        &req,
        INodeNo(1),
        OsStr::new("ln"),
        Path::new("target"),
        symlink.duplicate(),
    );
    assert!(*symlink.status.lock().expect("lock") == Some(Err(ENOSYS)));

    let read = ReplyData::capture();
    fs.read(
        &req,
        INodeNo(1),
        FileHandle(0),
        0,
        16,
        OpenFlags(0),
        None,
        read.duplicate(),
    );
    assert!(*read.data.lock().expect("lock") == Some(Err(ENOSYS)));

    let write = ReplyWrite::capture();
    fs.write(
        &req,
        INodeNo(1),
        FileHandle(0),
        0,
        b"abc",
        WriteFlags::empty(),
        OpenFlags(0),
        None,
        write.duplicate(),
    );
    assert!(*write.written.lock().expect("lock") == Some(Err(ENOSYS)));

    let open = ReplyOpen::capture();
    fs.open(&req, INodeNo(1), OpenFlags(0), open.duplicate());
    assert!(matches!(*open.opened.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let opendir = ReplyOpen::capture();
    fs.opendir(&req, INodeNo(1), OpenFlags(0), opendir.duplicate());
    assert!(matches!(*opendir.opened.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let statfs = ReplyStatfs::capture();
    fs.statfs(&req, INodeNo(1), statfs.duplicate());
    assert!(matches!(*statfs.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
}

#[test]
fn filesystem_default_contract_matrix_covers_public_operation_paths() {
    default_contract_metadata_operations();
    default_contract_node_operations();
    default_contract_io_directory_operations();
    default_contract_xattr_lock_misc_operations();
    default_contract_macos_operations();
}

fn default_contract_metadata_operations() {
    let mut fs = DefaultFs;
    let req = request_kernel();

    let mut config = default_kernel_config();
    let init = fs
        .init(&req, &mut config)
        .expect_err("default init is unimplemented");
    assert_eq!(init.raw_os_error(), Some(ENOSYS));

    let entry = ReplyEntry::capture();
    fs.lookup(&req, INodeNo(1), OsStr::new("n"), entry.duplicate());
    assert!(matches!(*entry.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let attr = ReplyAttr::capture();
    fs.getattr(&req, INodeNo(1), None, attr.duplicate());
    assert!(matches!(*attr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let setattr = ReplyAttr::capture();
    fs.setattr(
        &req,
        INodeNo(1),
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        None,
        setattr.duplicate(),
    );
    assert!(matches!(*setattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
}

fn default_contract_node_operations() {
    let fs = DefaultFs;
    let req = request_kernel();

    let data = ReplyData::capture();
    fs.readlink(&req, INodeNo(1), data.duplicate());
    assert!(matches!(*data.data.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let mknod = ReplyEntry::capture();
    fs.mknod(
        &req,
        INodeNo(1),
        OsStr::new("a"),
        0o644,
        0,
        0,
        mknod.duplicate(),
    );
    assert!(matches!(*mknod.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let mkdir = ReplyEntry::capture();
    fs.mkdir(
        &req,
        INodeNo(1),
        OsStr::new("d"),
        0o755,
        0,
        mkdir.duplicate(),
    );
    assert!(matches!(*mkdir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let unlink = ReplyEmpty::capture();
    fs.unlink(&req, INodeNo(1), OsStr::new("x"), unlink.duplicate());
    assert!(matches!(*unlink.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let rmdir = ReplyEmpty::capture();
    fs.rmdir(&req, INodeNo(1), OsStr::new("x"), rmdir.duplicate());
    assert!(matches!(*rmdir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let symlink = ReplyEntry::capture();
    fs.symlink(
        &req,
        INodeNo(1),
        OsStr::new("ln"),
        Path::new("target"),
        symlink.duplicate(),
    );
    assert!(matches!(*symlink.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let rename = ReplyEmpty::capture();
    fs.rename(
        &req,
        INodeNo(1),
        OsStr::new("a"),
        INodeNo(1),
        OsStr::new("b"),
        RenameFlags::empty(),
        rename.duplicate(),
    );
    assert!(matches!(*rename.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let link = ReplyEntry::capture();
    fs.link(
        &req,
        INodeNo(1),
        INodeNo(1),
        OsStr::new("l"),
        link.duplicate(),
    );
    assert!(matches!(*link.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
}

fn default_contract_io_directory_operations() {
    let fs = DefaultFs;
    let req = request_kernel();

    let open = ReplyOpen::capture();
    fs.open(&req, INodeNo(1), OpenFlags(0), open.duplicate());
    assert!(matches!(*open.opened.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let read = ReplyData::capture();
    fs.read(
        &req,
        INodeNo(1),
        FileHandle(0),
        0,
        10,
        OpenFlags(0),
        None,
        read.duplicate(),
    );
    assert!(matches!(*read.data.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let write = ReplyWrite::capture();
    fs.write(
        &req,
        INodeNo(1),
        FileHandle(0),
        0,
        b"w",
        WriteFlags::empty(),
        OpenFlags(0),
        None,
        write.duplicate(),
    );
    assert!(matches!(*write.written.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let flush = ReplyEmpty::capture();
    fs.flush(
        &req,
        INodeNo(1),
        FileHandle(0),
        LockOwner(0),
        flush.duplicate(),
    );
    assert!(matches!(*flush.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let release = ReplyEmpty::capture();
    fs.release(
        &req,
        INodeNo(1),
        FileHandle(0),
        OpenFlags(0),
        None,
        false,
        release.duplicate(),
    );
    assert!(matches!(*release.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let fsync = ReplyEmpty::capture();
    fs.fsync(&req, INodeNo(1), FileHandle(0), false, fsync.duplicate());
    assert!(matches!(*fsync.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let opendir = ReplyOpen::capture();
    fs.opendir(&req, INodeNo(1), OpenFlags(0), opendir.duplicate());
    assert!(matches!(*opendir.opened.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let readdir = ReplyDirectory::capture();
    fs.readdir(&req, INodeNo(1), FileHandle(0), 0, readdir.duplicate());
    assert!(matches!(*readdir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let readdirplus = ReplyDirectoryPlus::capture();
    fs.readdirplus(&req, INodeNo(1), FileHandle(0), 0, readdirplus.duplicate());
    assert!(matches!(*readdirplus.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let releasedir = ReplyEmpty::capture();
    fs.releasedir(
        &req,
        INodeNo(1),
        FileHandle(0),
        OpenFlags(0),
        releasedir.duplicate(),
    );
    assert!(matches!(*releasedir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let fsyncdir = ReplyEmpty::capture();
    fs.fsyncdir(&req, INodeNo(1), FileHandle(0), false, fsyncdir.duplicate());
    assert!(matches!(*fsyncdir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let statfs = ReplyStatfs::capture();
    fs.statfs(&req, INodeNo(1), statfs.duplicate());
    assert!(matches!(*statfs.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
}

fn default_contract_xattr_lock_misc_operations() {
    let fs = DefaultFs;
    let req = request_kernel();

    let setxattr = ReplyEmpty::capture();
    fs.setxattr(
        &req,
        INodeNo(1),
        OsStr::new("k"),
        b"v",
        0,
        0,
        setxattr.duplicate(),
    );
    assert!(matches!(*setxattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let getxattr = ReplyXattr::capture();
    fs.getxattr(&req, INodeNo(1), OsStr::new("k"), 0, getxattr.duplicate());
    assert!(matches!(*getxattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let listxattr = ReplyXattr::capture();
    fs.listxattr(&req, INodeNo(1), 0, listxattr.duplicate());
    assert!(matches!(*listxattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let removexattr = ReplyEmpty::capture();
    fs.removexattr(&req, INodeNo(1), OsStr::new("k"), removexattr.duplicate());
    assert!(matches!(*removexattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let access = ReplyEmpty::capture();
    fs.access(&req, INodeNo(1), AccessFlags::empty(), access.duplicate());
    assert!(matches!(*access.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let create = ReplyCreate::capture();
    fs.create(
        &req,
        INodeNo(1),
        OsStr::new("c"),
        0o644,
        0,
        0,
        create.duplicate(),
    );
    assert!(matches!(*create.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let getlk = ReplyLock::capture();
    fs.getlk(
        &req,
        INodeNo(1),
        FileHandle(0),
        LockOwner(0),
        0,
        1,
        LOCK_TYPE_WRLCK,
        0,
        getlk.duplicate(),
    );
    assert!(matches!(*getlk.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let setlk = ReplyEmpty::capture();
    fs.setlk(
        &req,
        INodeNo(1),
        FileHandle(0),
        LockOwner(0),
        0,
        1,
        LOCK_TYPE_WRLCK,
        0,
        false,
        setlk.duplicate(),
    );
    assert!(matches!(*setlk.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let bmap = ReplyBmap::capture();
    fs.bmap(&req, INodeNo(1), 4096, 0, bmap.duplicate());
    assert!(matches!(*bmap.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let ioctl = ReplyIoctl::capture();
    fs.ioctl(
        &req,
        INodeNo(1),
        FileHandle(0),
        IoctlFlags::empty(),
        0,
        &[],
        0,
        ioctl.duplicate(),
    );
    assert!(matches!(*ioctl.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let fallocate = ReplyEmpty::capture();
    fs.fallocate(
        &req,
        INodeNo(1),
        FileHandle(0),
        0,
        0,
        0,
        fallocate.duplicate(),
    );
    assert!(matches!(*fallocate.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let lseek = ReplyLseek::capture();
    fs.lseek(&req, INodeNo(1), FileHandle(0), 0, 0, lseek.duplicate());
    assert!(matches!(*lseek.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let copy = ReplyWrite::capture();
    fs.copy_file_range(
        &req,
        INodeNo(1),
        FileHandle(0),
        0,
        INodeNo(2),
        FileHandle(0),
        0,
        1,
        CopyFileRangeFlags::empty(),
        copy.duplicate(),
    );
    assert!(matches!(*copy.written.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
}

fn default_contract_macos_operations() {
    #[cfg(feature = "macos-api")]
    {
        let fs = DefaultFs;
        let req = request_kernel();
        let setvolname = ReplyEmpty::capture();
        fs.setvolname(&req, OsStr::new("volume"), setvolname.duplicate());
        assert!(matches!(*setvolname.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

        let exchange = ReplyEmpty::capture();
        fs.exchange(
            &req,
            INodeNo(1),
            OsStr::new("old"),
            INodeNo(2),
            OsStr::new("new"),
            0,
            exchange.duplicate(),
        );
        assert!(matches!(*exchange.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

        let getxtimes = ReplyXTimes::capture();
        fs.getxtimes(&req, INodeNo(1), getxtimes.duplicate());
        assert!(matches!(*getxtimes.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
    }
}

#[derive(Default)]
struct CoreBehaviorFs {
    lookup_called: AtomicUsize,
    getattr_called: AtomicUsize,
    readdir_called: AtomicUsize,
    open_called: AtomicUsize,
    read_called: AtomicUsize,
    write_called: AtomicUsize,
    create_called: AtomicUsize,
    unlink_called: AtomicUsize,
    rename_called: AtomicUsize,
    statfs_called: AtomicUsize,
}

impl Filesystem for CoreBehaviorFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.lookup_called.fetch_add(1, Ordering::SeqCst);
        let attr = FileAttr {
            ino: INodeNo(parent.0 + 1),
            size: 3,
            blocks: 1,
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        };
        assert_eq!(name, OsStr::new("f.txt"));
        reply.entry(&Duration::from_secs(1), &attr, Generation(0));
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        self.getattr_called.fetch_add(1, Ordering::SeqCst);
        let attr = FileAttr {
            ino,
            size: 7,
            blocks: 1,
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o444,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        };
        reply.attr(&Duration::from_secs(1), &attr);
    }

    fn readdir(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64,
        mut reply: ReplyDirectory,
    ) {
        self.readdir_called.fetch_add(1, Ordering::SeqCst);
        let _ = reply.add(INodeNo(2), 0, FileType::RegularFile, OsStr::new("f.txt"));
        reply.ok();
    }

    fn open(&self, _req: &Request, _ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        self.open_called.fetch_add(1, Ordering::SeqCst);
        reply.opened(
            FileHandle(100),
            FopenFlags::from_bits_retain(flags.0 as u32),
        );
    }

    fn read(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, _size: u32,
        _flags: OpenFlags, _lock_owner: Option<LockOwner>, reply: ReplyData,
    ) {
        self.read_called.fetch_add(1, Ordering::SeqCst);
        reply.data(b"abc");
    }

    fn write(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, data: &[u8],
        _write_flags: WriteFlags, _flags: OpenFlags, _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        self.write_called.fetch_add(1, Ordering::SeqCst);
        reply.written(data.len() as u32);
    }

    fn create(
        &self, _req: &Request, parent: INodeNo, name: &OsStr, _mode: u32, _umask: u32, flags: i32,
        reply: ReplyCreate,
    ) {
        self.create_called.fetch_add(1, Ordering::SeqCst);
        assert_eq!(name, OsStr::new("new.txt"));
        let attr = FileAttr {
            ino: INodeNo(parent.0 + 10),
            size: 0,
            blocks: 0,
            atime: UNIX_EPOCH,
            mtime: UNIX_EPOCH,
            ctime: UNIX_EPOCH,
            crtime: UNIX_EPOCH,
            kind: FileType::RegularFile,
            perm: 0o644,
            nlink: 1,
            uid: 0,
            gid: 0,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        };
        reply.created(
            &Duration::from_secs(1),
            &attr,
            Generation(0),
            FileHandle(33),
            FopenFlags::from_bits_retain(flags as u32),
        );
    }

    fn unlink(&self, _req: &Request, _parent: INodeNo, _name: &OsStr, reply: ReplyEmpty) {
        self.unlink_called.fetch_add(1, Ordering::SeqCst);
        reply.ok();
    }

    fn rename(
        &self, _req: &Request, _parent: INodeNo, _name: &OsStr, _newparent: INodeNo,
        _newname: &OsStr, _flags: RenameFlags, reply: ReplyEmpty,
    ) {
        self.rename_called.fetch_add(1, Ordering::SeqCst);
        reply.ok();
    }

    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        self.statfs_called.fetch_add(1, Ordering::SeqCst);
        reply.statfs(10, 5, 4, 3, 2, 512, 255, 0);
    }
}

#[test]
fn core_behavior_contract_for_key_fuser_operations() {
    let fs = CoreBehaviorFs::default();
    let req = request_kernel();

    let lookup = ReplyEntry::capture();
    fs.lookup(&req, INodeNo(1), OsStr::new("f.txt"), lookup.duplicate());
    assert!(
        matches!(*lookup.status.lock().expect("lock"), Some(Ok(payload)) if payload.attr.ino == INodeNo(2))
    );

    let getattr = ReplyAttr::capture();
    fs.getattr(&req, INodeNo(2), None, getattr.duplicate());
    assert!(
        matches!(*getattr.status.lock().expect("lock"), Some(Ok(attr)) if attr.ino == INodeNo(2) && attr.perm == 0o444)
    );

    let readdir = ReplyDirectory::capture();
    fs.readdir(&req, INodeNo(1), FileHandle(0), 0, readdir.duplicate());
    assert!(matches!(
        *readdir.status.lock().expect("lock"),
        Some(Ok(()))
    ));

    let open = ReplyOpen::capture();
    fs.open(&req, INodeNo(2), OpenFlags(3), open.duplicate());
    assert!(matches!(
        *open.opened.lock().expect("lock"),
        Some(Ok(payload)) if payload.fh == FileHandle(100) && payload.flags == FopenFlags::from_bits_retain(3)
    ));

    let read = ReplyData::capture();
    fs.read(
        &req,
        INodeNo(2),
        FileHandle(0),
        0,
        3,
        OpenFlags(0),
        None,
        read.duplicate(),
    );
    assert!(matches!(read.data.lock().expect("lock").clone(), Some(Ok(v)) if v == b"abc"));

    let write = ReplyWrite::capture();
    fs.write(
        &req,
        INodeNo(2),
        FileHandle(0),
        0,
        b"hello",
        WriteFlags::empty(),
        OpenFlags(0),
        None,
        write.duplicate(),
    );
    assert!(matches!(*write.written.lock().expect("lock"), Some(Ok(5))));

    let create = ReplyCreate::capture();
    fs.create(
        &req,
        INodeNo(1),
        OsStr::new("new.txt"),
        0o644,
        0,
        0,
        create.duplicate(),
    );
    assert!(
        matches!(*create.status.lock().expect("lock"), Some(Ok(payload)) if payload.attr.ino == INodeNo(11) && payload.fh == FileHandle(33))
    );

    let unlink = ReplyEmpty::capture();
    fs.unlink(&req, INodeNo(1), OsStr::new("old.txt"), unlink.duplicate());
    assert!(matches!(*unlink.status.lock().expect("lock"), Some(Ok(()))));

    let rename = ReplyEmpty::capture();
    fs.rename(
        &req,
        INodeNo(1),
        OsStr::new("a"),
        INodeNo(1),
        OsStr::new("b"),
        RenameFlags::empty(),
        rename.duplicate(),
    );
    assert!(matches!(*rename.status.lock().expect("lock"), Some(Ok(()))));

    let statfs = ReplyStatfs::capture();
    fs.statfs(&req, INodeNo(1), statfs.duplicate());
    assert!(matches!(
        *statfs.status.lock().expect("lock"),
        Some(Ok((10, 5, 4, 3, 2, 512, 255, 0)))
    ));

    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.getattr_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.readdir_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.open_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.read_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.write_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.create_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.unlink_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.rename_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.statfs_called.load(Ordering::SeqCst), 1);
}

#[test]
fn filesystem_poll_default_returns_enosys() {
    let fs = DefaultFs;
    let req = request_kernel();
    let reply = ReplyPoll::capture();
    fs.poll(
        &req,
        INodeNo(1),
        FileHandle(0),
        PollNotifier::new(ChannelSender, PollHandle(0)),
        PollEvents::empty(),
        PollFlags::empty(),
        reply.duplicate(),
    );
    assert!(matches!(*reply.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
}

#[test]
fn filesystem_batch_forget_delegates_to_forget() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct ForgetFs(Arc<AtomicUsize>);
    impl Filesystem for ForgetFs {
        fn forget(&self, _req: &Request, ino: INodeNo, nlookup: u64) {
            self.0.fetch_add(1, Ordering::SeqCst);
            assert_eq!(ino, INodeNo(42));
            assert_eq!(nlookup, 5);
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let fs = ForgetFs(Arc::clone(&count));
    let req = request_kernel();
    fs.batch_forget(&req, &[ForgetOne::new(42, 5), ForgetOne::new(42, 5)]);
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
