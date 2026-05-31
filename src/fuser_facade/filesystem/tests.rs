use super::super::reply::*;
use super::super::request::Request;
use super::super::request::request_kernel;
use super::super::types::{FileAttr, FileType};
use super::*;
use crate::dokan_impl::LOCK_TYPE_WRLCK;
use libc::ENOSYS;
use std::ffi::OsStr;
use std::path::Path;
use std::time::{Duration, UNIX_EPOCH};

#[derive(Default)]
struct DefaultFs;

impl Filesystem for DefaultFs {}

#[test]
fn filesystem_default_methods_match_fuser_style_error_contract() {
    let mut fs = DefaultFs;
    let req = request_kernel();

    let lookup = ReplyEntry::default();
    fs.lookup(&req, 1, OsStr::new("x"), lookup.clone());
    assert!(*lookup.status.lock().expect("lock") == Some(Err(ENOSYS)));

    let getattr = ReplyAttr::default();
    fs.getattr(&req, 1, None, getattr.clone());
    assert!(*getattr.status.lock().expect("lock") == Some(Err(ENOSYS)));

    let symlink = ReplyEntry::default();
    fs.symlink(
        &req,
        1,
        OsStr::new("ln"),
        Path::new("target"),
        symlink.clone(),
    );
    assert!(*symlink.status.lock().expect("lock") == Some(Err(libc::EPERM)));

    let read = ReplyData::default();
    fs.read(&req, 1, 0, 0, 16, 0, None, read.clone());
    assert!(*read.data.lock().expect("lock") == Some(Err(ENOSYS)));

    let write = ReplyWrite::default();
    fs.write(&req, 1, 0, 0, b"abc", 0, 0, None, write.clone());
    assert!(*write.written.lock().expect("lock") == Some(Err(ENOSYS)));

    let open = ReplyOpen::default();
    fs.open(&req, 1, 0, open.clone());
    assert!(*open.opened.lock().expect("lock") == Some(Ok((0, 0))));

    let opendir = ReplyOpen::default();
    fs.opendir(&req, 1, 0, opendir.clone());
    assert!(*opendir.opened.lock().expect("lock") == Some(Ok((0, 0))));

    let statfs = ReplyStatfs::default();
    fs.statfs(&req, 1, statfs.clone());
    assert!(*statfs.status.lock().expect("lock") == Some(Ok((0, 0, 0, 0, 0, 512, 255, 0))));
}

#[test]
fn filesystem_default_contract_matrix_covers_public_operation_paths() {
    let mut fs = DefaultFs;
    let req = request_kernel();

    let entry = ReplyEntry::default();
    fs.lookup(&req, 1, OsStr::new("n"), entry.clone());
    assert!(matches!(*entry.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let attr = ReplyAttr::default();
    fs.getattr(&req, 1, None, attr.clone());
    assert!(matches!(*attr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let setattr = ReplyAttr::default();
    fs.setattr(
        &req,
        1,
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
        setattr.clone(),
    );
    assert!(matches!(*setattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let data = ReplyData::default();
    fs.readlink(&req, 1, data.clone());
    assert!(matches!(*data.data.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let mknod = ReplyEntry::default();
    fs.mknod(&req, 1, OsStr::new("a"), 0o644, 0, 0, mknod.clone());
    assert!(matches!(*mknod.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let mkdir = ReplyEntry::default();
    fs.mkdir(&req, 1, OsStr::new("d"), 0o755, 0, mkdir.clone());
    assert!(matches!(*mkdir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let unlink = ReplyEmpty::default();
    fs.unlink(&req, 1, OsStr::new("x"), unlink.clone());
    assert!(matches!(*unlink.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let rmdir = ReplyEmpty::default();
    fs.rmdir(&req, 1, OsStr::new("x"), rmdir.clone());
    assert!(matches!(*rmdir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let symlink = ReplyEntry::default();
    fs.symlink(
        &req,
        1,
        OsStr::new("ln"),
        Path::new("target"),
        symlink.clone(),
    );
    assert!(matches!(*symlink.status.lock().expect("lock"), Some(Err(e)) if e == libc::EPERM));

    let rename = ReplyEmpty::default();
    fs.rename(
        &req,
        1,
        OsStr::new("a"),
        1,
        OsStr::new("b"),
        0,
        rename.clone(),
    );
    assert!(matches!(*rename.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let link = ReplyEntry::default();
    fs.link(&req, 1, 1, OsStr::new("l"), link.clone());
    assert!(matches!(*link.status.lock().expect("lock"), Some(Err(e)) if e == libc::EPERM));

    let open = ReplyOpen::default();
    fs.open(&req, 1, 0, open.clone());
    assert!(matches!(
        *open.opened.lock().expect("lock"),
        Some(Ok((0, 0)))
    ));

    let read = ReplyData::default();
    fs.read(&req, 1, 0, 0, 10, 0, None, read.clone());
    assert!(matches!(*read.data.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let write = ReplyWrite::default();
    fs.write(&req, 1, 0, 0, b"w", 0, 0, None, write.clone());
    assert!(matches!(*write.written.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let flush = ReplyEmpty::default();
    fs.flush(&req, 1, 0, 0, flush.clone());
    assert!(matches!(*flush.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let release = ReplyEmpty::default();
    fs.release(&req, 1, 0, 0, None, false, release.clone());
    assert!(matches!(
        *release.status.lock().expect("lock"),
        Some(Ok(()))
    ));

    let fsync = ReplyEmpty::default();
    fs.fsync(&req, 1, 0, false, fsync.clone());
    assert!(matches!(*fsync.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let opendir = ReplyOpen::default();
    fs.opendir(&req, 1, 0, opendir.clone());
    assert!(matches!(
        *opendir.opened.lock().expect("lock"),
        Some(Ok((0, 0)))
    ));

    let readdir = ReplyDirectory::default();
    fs.readdir(&req, 1, 0, 0, readdir.clone());
    assert!(matches!(*readdir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let readdirplus = ReplyDirectoryPlus::default();
    fs.readdirplus(&req, 1, 0, 0, readdirplus.clone());
    assert!(matches!(*readdirplus.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let releasedir = ReplyEmpty::default();
    fs.releasedir(&req, 1, 0, 0, releasedir.clone());
    assert!(matches!(
        *releasedir.status.lock().expect("lock"),
        Some(Ok(()))
    ));

    let fsyncdir = ReplyEmpty::default();
    fs.fsyncdir(&req, 1, 0, false, fsyncdir.clone());
    assert!(matches!(*fsyncdir.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let statfs = ReplyStatfs::default();
    fs.statfs(&req, 1, statfs.clone());
    assert!(matches!(
        *statfs.status.lock().expect("lock"),
        Some(Ok((0, 0, 0, 0, 0, 512, 255, 0)))
    ));

    let setxattr = ReplyEmpty::default();
    fs.setxattr(&req, 1, OsStr::new("k"), b"v", 0, 0, setxattr.clone());
    assert!(matches!(*setxattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let getxattr = ReplyXattr::default();
    fs.getxattr(&req, 1, OsStr::new("k"), 0, getxattr.clone());
    assert!(matches!(*getxattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let listxattr = ReplyXattr::default();
    fs.listxattr(&req, 1, 0, listxattr.clone());
    assert!(matches!(*listxattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let removexattr = ReplyEmpty::default();
    fs.removexattr(&req, 1, OsStr::new("k"), removexattr.clone());
    assert!(matches!(*removexattr.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let access = ReplyEmpty::default();
    fs.access(&req, 1, 0, access.clone());
    assert!(matches!(*access.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let create = ReplyCreate::default();
    fs.create(&req, 1, OsStr::new("c"), 0o644, 0, 0, create.clone());
    assert!(matches!(*create.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let getlk = ReplyLock::default();
    fs.getlk(&req, 1, 0, 0, 0, 1, LOCK_TYPE_WRLCK, 0, getlk.clone());
    assert!(matches!(*getlk.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let setlk = ReplyEmpty::default();
    fs.setlk(
        &req,
        1,
        0,
        0,
        0,
        1,
        LOCK_TYPE_WRLCK,
        0,
        false,
        setlk.clone(),
    );
    assert!(matches!(*setlk.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let bmap = ReplyBmap::default();
    fs.bmap(&req, 1, 4096, 0, bmap.clone());
    assert!(matches!(*bmap.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let ioctl = ReplyIoctl::default();
    fs.ioctl(&req, 1, 0, 0, 0, &[], 0, ioctl.clone());
    assert!(matches!(*ioctl.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let fallocate = ReplyEmpty::default();
    fs.fallocate(&req, 1, 0, 0, 0, 0, fallocate.clone());
    assert!(matches!(*fallocate.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let lseek = ReplyLseek::default();
    fs.lseek(&req, 1, 0, 0, 0, lseek.clone());
    assert!(matches!(*lseek.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));

    let copy = ReplyWrite::default();
    fs.copy_file_range(&req, 1, 0, 0, 2, 0, 0, 1, 0, copy.clone());
    assert!(matches!(*copy.written.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
}

#[derive(Default)]
struct CoreBehaviorFs {
    lookup_called: usize,
    getattr_called: usize,
    readdir_called: usize,
    open_called: usize,
    read_called: usize,
    write_called: usize,
    create_called: usize,
    unlink_called: usize,
    rename_called: usize,
    statfs_called: usize,
}

impl Filesystem for CoreBehaviorFs {
    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        self.lookup_called += 1;
        let attr = FileAttr {
            ino: parent + 1,
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
        reply.entry(&Duration::from_secs(1), &attr, 0);
    }

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        self.getattr_called += 1;
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
        &mut self, _req: &Request, _ino: u64, _fh: u64, _offset: u64, mut reply: ReplyDirectory,
    ) {
        self.readdir_called += 1;
        let _ = reply.add(2, 0, FileType::RegularFile, OsStr::new("f.txt"));
        reply.ok();
    }

    fn open(&mut self, _req: &Request, _ino: u64, flags: i32, reply: ReplyOpen) {
        self.open_called += 1;
        reply.opened(100, flags as u32);
    }

    fn read(
        &mut self, _req: &Request, _ino: u64, _fh: u64, _offset: u64, _size: u32, _flags: i32,
        _lock_owner: Option<u64>, reply: ReplyData,
    ) {
        self.read_called += 1;
        reply.data(b"abc");
    }

    fn write(
        &mut self, _req: &Request, _ino: u64, _fh: u64, _offset: u64, data: &[u8],
        _write_flags: u32, _flags: i32, _lock_owner: Option<u64>, reply: ReplyWrite,
    ) {
        self.write_called += 1;
        reply.written(data.len() as u32);
    }

    fn create(
        &mut self, _req: &Request, parent: u64, name: &OsStr, _mode: u32, _umask: u32,
        flags: i32, reply: ReplyCreate,
    ) {
        self.create_called += 1;
        assert_eq!(name, OsStr::new("new.txt"));
        let attr = FileAttr {
            ino: parent + 10,
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
        reply.created(&Duration::from_secs(1), &attr, 0, 33, flags as u32);
    }

    fn unlink(&mut self, _req: &Request, _parent: u64, _name: &OsStr, reply: ReplyEmpty) {
        self.unlink_called += 1;
        reply.ok();
    }

    fn rename(
        &mut self, _req: &Request, _parent: u64, _name: &OsStr, _newparent: u64,
        _newname: &OsStr, _flags: u32, reply: ReplyEmpty,
    ) {
        self.rename_called += 1;
        reply.ok();
    }

    fn statfs(&mut self, _req: &Request, _ino: u64, reply: ReplyStatfs) {
        self.statfs_called += 1;
        reply.statfs(10, 5, 4, 3, 2, 512, 255, 0);
    }
}

#[test]
fn core_behavior_contract_for_key_fuser_operations() {
    let mut fs = CoreBehaviorFs::default();
    let req = request_kernel();

    let lookup = ReplyEntry::default();
    fs.lookup(&req, 1, OsStr::new("f.txt"), lookup.clone());
    assert!(matches!(*lookup.status.lock().expect("lock"), Some(Ok(attr)) if attr.ino == 2));

    let getattr = ReplyAttr::default();
    fs.getattr(&req, 2, None, getattr.clone());
    assert!(
        matches!(*getattr.status.lock().expect("lock"), Some(Ok(attr)) if attr.ino == 2 && attr.perm == 0o444)
    );

    let readdir = ReplyDirectory::default();
    fs.readdir(&req, 1, 0, 0, readdir.clone());
    assert!(matches!(
        *readdir.status.lock().expect("lock"),
        Some(Ok(()))
    ));

    let open = ReplyOpen::default();
    fs.open(&req, 2, 3, open.clone());
    assert!(matches!(
        *open.opened.lock().expect("lock"),
        Some(Ok((100, 3)))
    ));

    let read = ReplyData::default();
    fs.read(&req, 2, 0, 0, 3, 0, None, read.clone());
    assert!(matches!(read.data.lock().expect("lock").clone(), Some(Ok(v)) if v == b"abc"));

    let write = ReplyWrite::default();
    fs.write(&req, 2, 0, 0, b"hello", 0, 0, None, write.clone());
    assert!(matches!(*write.written.lock().expect("lock"), Some(Ok(5))));

    let create = ReplyCreate::default();
    fs.create(&req, 1, OsStr::new("new.txt"), 0o644, 0, 0, create.clone());
    assert!(
        matches!(*create.status.lock().expect("lock"), Some(Ok((attr, 33, _flags))) if attr.ino == 11)
    );

    let unlink = ReplyEmpty::default();
    fs.unlink(&req, 1, OsStr::new("old.txt"), unlink.clone());
    assert!(matches!(*unlink.status.lock().expect("lock"), Some(Ok(()))));

    let rename = ReplyEmpty::default();
    fs.rename(
        &req,
        1,
        OsStr::new("a"),
        1,
        OsStr::new("b"),
        0,
        rename.clone(),
    );
    assert!(matches!(*rename.status.lock().expect("lock"), Some(Ok(()))));

    let statfs = ReplyStatfs::default();
    fs.statfs(&req, 1, statfs.clone());
    assert!(matches!(
        *statfs.status.lock().expect("lock"),
        Some(Ok((10, 5, 4, 3, 2, 512, 255, 0)))
    ));

    assert_eq!(fs.lookup_called, 1);
    assert_eq!(fs.getattr_called, 1);
    assert_eq!(fs.readdir_called, 1);
    assert_eq!(fs.open_called, 1);
    assert_eq!(fs.read_called, 1);
    assert_eq!(fs.write_called, 1);
    assert_eq!(fs.create_called, 1);
    assert_eq!(fs.unlink_called, 1);
    assert_eq!(fs.rename_called, 1);
    assert_eq!(fs.statfs_called, 1);
}

#[cfg(feature = "abi-7-11")]
#[test]
fn filesystem_poll_default_returns_enosys() {
    let mut fs = DefaultFs;
    let req = request_kernel();
    let reply = ReplyPoll::default();
    fs.poll(&req, 1, 0, 0, 0, 0, reply.clone());
    assert!(matches!(*reply.status.lock().expect("lock"), Some(Err(e)) if e == ENOSYS));
}

#[cfg(feature = "abi-7-16")]
#[test]
fn filesystem_batch_forget_delegates_to_forget() {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct ForgetFs(Arc<AtomicUsize>);
    impl Filesystem for ForgetFs {
        fn forget(&mut self, _req: &Request, ino: u64, nlookup: u64) {
            self.0.fetch_add(1, Ordering::SeqCst);
            assert_eq!(ino, 42);
            assert_eq!(nlookup, 5);
        }
    }
    let count = Arc::new(AtomicUsize::new(0));
    let mut fs = ForgetFs(Arc::clone(&count));
    let req = request_kernel();
    fs.batch_forget(
        &req,
        &[
            fuse_forget_one {
                nodeid: 42,
                nlookup: 5,
            },
            fuse_forget_one {
                nodeid: 42,
                nlookup: 5,
            },
        ],
    );
    assert_eq!(count.load(Ordering::SeqCst), 2);
}
