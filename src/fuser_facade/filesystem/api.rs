use super::super::reply::*;
use super::super::request::Request;
use super::super::types::*;
use libc::ENOSYS;
use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::time::SystemTime;

#[allow(clippy::too_many_arguments)]
pub trait Filesystem: Send + Sync + 'static {
    fn init(&mut self, _req: &Request, _config: &mut KernelConfig) -> io::Result<()> {
        Ok(())
    }
    fn destroy(&mut self) {}
    fn lookup(&mut self, _req: &Request, _parent: INodeNo, _name: &OsStr, reply: ReplyEntry) {
        reply.error(ENOSYS);
    }
    fn forget(&mut self, _req: &Request, _ino: INodeNo, _nlookup: u64) {}
    fn getattr(&mut self, _req: &Request, _ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        reply.error(ENOSYS);
    }
    fn setattr(
        &mut self, _req: &Request, _ino: INodeNo, _mode: Option<u32>, _uid: Option<u32>,
        _gid: Option<u32>, _size: Option<u64>, _atime: Option<TimeOrNow>,
        _mtime: Option<TimeOrNow>, _ctime: Option<SystemTime>, _fh: Option<FileHandle>,
        _crtime: Option<SystemTime>, _chgtime: Option<SystemTime>, _bkuptime: Option<SystemTime>,
        _flags: Option<BsdFileFlags>, reply: ReplyAttr,
    ) {
        reply.error(ENOSYS);
    }
    fn readlink(&mut self, _req: &Request, _ino: INodeNo, reply: ReplyData) {
        reply.error(ENOSYS);
    }
    fn mknod(
        &mut self, _req: &Request, _parent: INodeNo, _name: &OsStr, _mode: u32, _umask: u32,
        _rdev: u32, reply: ReplyEntry,
    ) {
        reply.error(ENOSYS);
    }
    fn mkdir(
        &mut self, _req: &Request, _parent: INodeNo, _name: &OsStr, _mode: u32, _umask: u32,
        reply: ReplyEntry,
    ) {
        reply.error(ENOSYS);
    }
    fn unlink(&mut self, _req: &Request, _parent: INodeNo, _name: &OsStr, reply: ReplyEmpty) {
        reply.error(ENOSYS);
    }
    fn rmdir(&mut self, _req: &Request, _parent: INodeNo, _name: &OsStr, reply: ReplyEmpty) {
        reply.error(ENOSYS);
    }
    fn symlink(
        &mut self, _req: &Request, _parent: INodeNo, _link_name: &OsStr, _target: &Path,
        reply: ReplyEntry,
    ) {
        reply.error(libc::EPERM);
    }
    fn rename(
        &mut self, _req: &Request, _parent: INodeNo, _name: &OsStr, _newparent: INodeNo,
        _newname: &OsStr, _flags: RenameFlags, reply: ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }
    fn link(
        &mut self, _req: &Request, _ino: INodeNo, _newparent: INodeNo, _newname: &OsStr,
        reply: ReplyEntry,
    ) {
        reply.error(libc::EPERM);
    }
    fn open(&mut self, _req: &Request, _ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        reply.opened(0, 0);
    }
    fn read(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, _size: u32,
        _flags: OpenFlags, _lock_owner: Option<LockOwner>, reply: ReplyData,
    ) {
        reply.error(ENOSYS);
    }
    fn write(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, _data: &[u8],
        _write_flags: WriteFlags, _flags: OpenFlags, _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        reply.error(ENOSYS);
    }
    fn flush(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _lock_owner: LockOwner,
        reply: ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }
    fn release(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: OpenFlags,
        _lock_owner: Option<LockOwner>, _flush: bool, reply: ReplyEmpty,
    ) {
        reply.ok();
    }
    fn fsync(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _datasync: bool,
        reply: ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }
    fn opendir(&mut self, _req: &Request, _ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        reply.opened(0, 0);
    }
    fn readdir(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64,
        reply: ReplyDirectory,
    ) {
        reply.error(ENOSYS);
    }
    fn readdirplus(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64,
        reply: ReplyDirectoryPlus,
    ) {
        reply.error(ENOSYS);
    }
    fn releasedir(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: OpenFlags,
        reply: ReplyEmpty,
    ) {
        reply.ok();
    }
    fn fsyncdir(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _datasync: bool,
        reply: ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }
    fn statfs(&mut self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
        reply.statfs(0, 0, 0, 0, 0, 512, 255, 0);
    }
    fn setxattr(
        &mut self, _req: &Request, _ino: INodeNo, _name: &OsStr, _value: &[u8], _flags: i32,
        _position: u32, reply: ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }
    fn getxattr(
        &mut self, _req: &Request, _ino: INodeNo, _name: &OsStr, _size: u32, reply: ReplyXattr,
    ) {
        reply.error(ENOSYS);
    }
    fn listxattr(&mut self, _req: &Request, _ino: INodeNo, _size: u32, reply: ReplyXattr) {
        reply.error(ENOSYS);
    }
    fn removexattr(&mut self, _req: &Request, _ino: INodeNo, _name: &OsStr, reply: ReplyEmpty) {
        reply.error(ENOSYS);
    }
    fn access(&mut self, _req: &Request, _ino: INodeNo, _mask: AccessFlags, reply: ReplyEmpty) {
        reply.error(ENOSYS);
    }
    fn create(
        &mut self, _req: &Request, _parent: INodeNo, _name: &OsStr, _mode: u32, _umask: u32,
        _flags: i32, reply: ReplyCreate,
    ) {
        reply.error(ENOSYS);
    }
    #[allow(clippy::too_many_arguments)]
    fn getlk(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _lock_owner: LockOwner, _start: u64,
        _end: u64, _typ: i32, _pid: u32, reply: ReplyLock,
    ) {
        reply.error(ENOSYS);
    }
    #[allow(clippy::too_many_arguments)]
    fn setlk(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _lock_owner: LockOwner, _start: u64,
        _end: u64, _typ: i32, _pid: u32, _sleep: bool, reply: ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }
    fn bmap(
        &mut self, _req: &Request, _ino: INodeNo, _blocksize: u32, _idx: u64, reply: ReplyBmap,
    ) {
        reply.error(ENOSYS);
    }
    fn ioctl(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: IoctlFlags, _cmd: u32,
        _in_data: &[u8], _out_size: u32, reply: ReplyIoctl,
    ) {
        reply.error(ENOSYS);
    }
    fn fallocate(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, _length: u64, _mode: i32,
        reply: ReplyEmpty,
    ) {
        reply.error(ENOSYS);
    }
    fn lseek(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: i64, _whence: i32,
        reply: ReplyLseek,
    ) {
        reply.error(ENOSYS);
    }
    #[allow(clippy::too_many_arguments)]
    fn copy_file_range(
        &mut self, _req: &Request, _ino_in: INodeNo, _fh_in: FileHandle, _offset_in: u64,
        _ino_out: INodeNo, _fh_out: FileHandle, _offset_out: u64, _len: u64,
        _flags: CopyFileRangeFlags, reply: ReplyWrite,
    ) {
        reply.error(ENOSYS);
    }

    #[cfg(feature = "abi-7-11")]
    fn poll(
        &mut self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _kh: u64, _events: PollEvents, _flags: PollFlags,
        reply: ReplyPoll,
    ) {
        reply.error(ENOSYS);
    }

    #[cfg(feature = "abi-7-16")]
    fn batch_forget(&mut self, req: &Request, nodes: &[fuse_forget_one]) {
        for node in nodes {
            self.forget(req, node.nodeid, node.nlookup);
        }
    }
}
