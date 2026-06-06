use super::super::notifier::PollNotifier;
use super::super::reply::*;
use super::super::request::Request;
use super::super::types::*;
use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::time::SystemTime;

#[allow(clippy::too_many_arguments)]
pub trait Filesystem: Send + Sync + 'static {
    fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> io::Result<()> {
        Err(io::Error::from_raw_os_error(Errno::ENOSYS.raw_os_error()))
    }
    fn destroy(&mut self) {}
    fn lookup(&self, _req: &Request, _parent: INodeNo, _name: &OsStr, reply: ReplyEntry) {
        reply.error(Errno::ENOSYS);
    }
    fn forget(&self, _req: &Request, _ino: INodeNo, _nlookup: u64) {}
    fn getattr(&self, _req: &Request, _ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        reply.error(Errno::ENOSYS);
    }
    fn setattr(
        &self, _req: &Request, _ino: INodeNo, _mode: Option<u32>, _uid: Option<u32>,
        _gid: Option<u32>, _size: Option<u64>, _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>, _ctime: Option<SystemTime>, _fh: Option<FileHandle>,
        _crtime: Option<SystemTime>, _chgtime: Option<SystemTime>, _bkuptime: Option<SystemTime>,
        _flags: Option<BsdFileFlags>, reply: ReplyAttr,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn readlink(&self, _req: &Request, _ino: INodeNo, reply: ReplyData) {
        reply.error(Errno::ENOSYS);
    }
    fn mknod(
        &self, _req: &Request, _parent: INodeNo, _name: &OsStr, _mode: u32, _umask: u32,
        _rdev: u32, reply: ReplyEntry,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn mkdir(
        &self, _req: &Request, _parent: INodeNo, _name: &OsStr, _mode: u32, _umask: u32,
        reply: ReplyEntry,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn unlink(&self, _req: &Request, _parent: INodeNo, _name: &OsStr, reply: ReplyEmpty) {
        reply.error(Errno::ENOSYS);
    }
    fn rmdir(&self, _req: &Request, _parent: INodeNo, _name: &OsStr, reply: ReplyEmpty) {
        reply.error(Errno::ENOSYS);
    }
    fn symlink(
        &self, _req: &Request, _parent: INodeNo, _link_name: &OsStr, _target: &Path,
        reply: ReplyEntry,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn rename(
        &self, _req: &Request, _parent: INodeNo, _name: &OsStr, _newparent: INodeNo,
        _newname: &OsStr, _flags: RenameFlags, reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn link(
        &self, _req: &Request, _ino: INodeNo, _newparent: INodeNo, _newname: &OsStr,
        reply: ReplyEntry,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn open(&self, _req: &Request, _ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        reply.error(Errno::ENOSYS);
    }
    fn read(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, _size: u32,
        _flags: OpenFlags, _lock_owner: Option<LockOwner>, reply: ReplyData,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn write(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, _data: &[u8],
        _write_flags: WriteFlags, _flags: OpenFlags, _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn flush(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn release(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: OpenFlags,
        _lock_owner: Option<LockOwner>, _flush: bool, reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn fsync(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _datasync: bool, reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn opendir(&self, _req: &Request, _ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        reply.error(Errno::ENOSYS);
    }
    fn readdir(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, reply: ReplyDirectory,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn readdirplus(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64,
        reply: ReplyDirectoryPlus,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn releasedir(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: OpenFlags, reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn fsyncdir(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _datasync: bool, reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        reply.error(Errno::ENOSYS);
    }
    fn setxattr(
        &self, _req: &Request, _ino: INodeNo, _name: &OsStr, _value: &[u8], _flags: i32,
        _position: u32, reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn getxattr(
        &self, _req: &Request, _ino: INodeNo, _name: &OsStr, _size: u32, reply: ReplyXattr,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn listxattr(&self, _req: &Request, _ino: INodeNo, _size: u32, reply: ReplyXattr) {
        reply.error(Errno::ENOSYS);
    }
    fn removexattr(&self, _req: &Request, _ino: INodeNo, _name: &OsStr, reply: ReplyEmpty) {
        reply.error(Errno::ENOSYS);
    }
    fn access(&self, _req: &Request, _ino: INodeNo, _mask: AccessFlags, reply: ReplyEmpty) {
        reply.error(Errno::ENOSYS);
    }
    fn create(
        &self, _req: &Request, _parent: INodeNo, _name: &OsStr, _mode: u32, _umask: u32,
        _flags: i32, reply: ReplyCreate,
    ) {
        reply.error(Errno::ENOSYS);
    }
    #[allow(clippy::too_many_arguments)]
    fn getlk(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _lock_owner: LockOwner, _start: u64,
        _end: u64, _typ: i32, _pid: u32, reply: ReplyLock,
    ) {
        reply.error(Errno::ENOSYS);
    }
    #[allow(clippy::too_many_arguments)]
    fn setlk(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _lock_owner: LockOwner, _start: u64,
        _end: u64, _typ: i32, _pid: u32, _sleep: bool, reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn bmap(&self, _req: &Request, _ino: INodeNo, _blocksize: u32, _idx: u64, reply: ReplyBmap) {
        reply.error(Errno::ENOSYS);
    }
    fn ioctl(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: IoctlFlags, _cmd: u32,
        _in_data: &[u8], _out_size: u32, reply: ReplyIoctl,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn fallocate(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, _length: u64,
        _mode: i32, reply: ReplyEmpty,
    ) {
        reply.error(Errno::ENOSYS);
    }
    fn lseek(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: i64, _whence: i32,
        reply: ReplyLseek,
    ) {
        reply.error(Errno::ENOSYS);
    }
    #[allow(clippy::too_many_arguments)]
    fn copy_file_range(
        &self, _req: &Request, _ino_in: INodeNo, _fh_in: FileHandle, _offset_in: u64,
        _ino_out: INodeNo, _fh_out: FileHandle, _offset_out: u64, _len: u64,
        _flags: CopyFileRangeFlags, reply: ReplyWrite,
    ) {
        reply.error(Errno::ENOSYS);
    }

    fn poll(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _ph: PollNotifier,
        _events: PollEvents, _flags: PollFlags, reply: ReplyPoll,
    ) {
        reply.error(Errno::ENOSYS);
    }

    fn batch_forget(&self, req: &Request, nodes: &[ForgetOne]) {
        for node in nodes {
            self.forget(req, node.nodeid(), node.nlookup());
        }
    }

    /// macOS only: Rename the volume. Set `fuse_init_out.flags` during init to
    /// `FUSE_VOL_RENAME` to enable.
    #[cfg(feature = "macos-api")]
    fn setvolname(&self, _req: &Request, name: &OsStr, reply: ReplyEmpty) {
        let _ = name;
        reply.error(Errno::ENOSYS);
    }

    /// macOS only (undocumented).
    #[cfg(feature = "macos-api")]
    fn exchange(
        &self, _req: &Request, parent: INodeNo, name: &OsStr, newparent: INodeNo, newname: &OsStr,
        options: u64, reply: ReplyEmpty,
    ) {
        let _ = (parent, name, newparent, newname, options);
        reply.error(Errno::ENOSYS);
    }

    /// macOS only: Query extended times (`bkuptime` and `crtime`).
    /// Set `fuse_init_out.flags` to `FUSE_XTIMES` to enable.
    #[cfg(feature = "macos-api")]
    fn getxtimes(&self, _req: &Request, ino: INodeNo, reply: ReplyXTimes) {
        let _ = ino;
        reply.error(Errno::ENOSYS);
    }
}
