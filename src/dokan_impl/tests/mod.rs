use super::*;
use crate::dokan_impl::AdapterContext;
use crate::fuser_facade::FsCell;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::*;
use crate::fuser_facade::request::{Request, request_kernel};
use crate::fuser_facade::types::{
    Errno, FileHandle, FileType, Generation, INodeNo, InitFlags, KernelConfig, LockOwner,
    MountOption, OpenFlags, RenameFlags, SessionACL,
};
use dokan_sys::win32::{FILE_CREATE, FILE_SUPERSEDE};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::Path;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
use widestring::U16CString;
use winapi::shared::ntstatus::*;
use winapi::um::winnt::*;

mod basic;
mod basic_helpers;
mod handler_routes;
mod link_lifecycle;
mod resolver_core;
mod resolver_mutation;
mod security_streams;

#[derive(Default)]
struct TtlResolverFs {
    lookup_called: AtomicUsize,
    readdir_called: AtomicUsize,
    getattr_called: AtomicUsize,
    read_called: AtomicUsize,
    fsync_called: AtomicUsize,
    entry_ttl_secs: u64,
    attr_ttl_secs: u64,
    missing: bool,
    lookup_error: Option<Errno>,
    forget_called: AtomicUsize,
    forget_lookup_total: AtomicUsize,
    rename_called: AtomicUsize,
    create_called: AtomicUsize,
    open_called: AtomicUsize,
    opendir_called: AtomicUsize,
    access_called: AtomicUsize,
    write_called: AtomicUsize,
    setattr_called: AtomicUsize,
    fallocate_called: AtomicUsize,
    attr_kind: Option<FileType>,
    assert_no_forget_before_getattr: bool,
    assert_no_forget_before_open: bool,
    assert_no_forget_before_setattr: bool,
    assert_no_forget_before_fallocate: bool,
}

impl Filesystem for TtlResolverFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.lookup_called.fetch_add(1, Ordering::SeqCst);
        if let Some(err) = self.lookup_error {
            reply.error(err);
            return;
        }
        match (parent, name.to_string_lossy().as_ref()) {
            (INodeNo::ROOT, "dir") if !self.missing => reply.entry(
                &Duration::from_secs(self.entry_ttl_secs),
                &test_file_attr(42),
                Generation(7),
            ),
            (INodeNo::ROOT, "target") if !self.missing => reply.entry(
                &Duration::from_secs(self.entry_ttl_secs),
                &test_file_attr(45),
                Generation(8),
            ),
            _ => reply.error(Errno::ENOENT),
        }
    }

    fn readdir(
        &self, _req: &Request, ino: INodeNo, _fh: FileHandle, _offset: u64,
        mut reply: ReplyDirectory,
    ) {
        self.readdir_called.fetch_add(1, Ordering::SeqCst);
        assert_eq!(ino, INodeNo(42));
        reply.add(
            INodeNo(43),
            1,
            FileType::RegularFile,
            OsStr::new("child.txt"),
        );
        reply.ok();
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        if self.assert_no_forget_before_getattr && ino == INodeNo(42) {
            assert_eq!(self.forget_lookup_total.load(Ordering::SeqCst), 0);
        }
        self.getattr_called.fetch_add(1, Ordering::SeqCst);
        let mut attr = test_file_attr(ino.0);
        if let Some(kind) = self.attr_kind {
            attr.kind = kind;
        }
        attr.size = self.write_called.load(Ordering::SeqCst) as u64;
        reply.attr(&Duration::from_secs(self.attr_ttl_secs), &attr);
    }

    fn access(
        &self, _req: &Request, _ino: INodeNo, _mask: crate::fuser_facade::types::AccessFlags,
        reply: ReplyEmpty,
    ) {
        self.access_called.fetch_add(1, Ordering::SeqCst);
        reply.ok();
    }

    fn read(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, size: u32,
        _flags: OpenFlags, _lock_owner: Option<LockOwner>, reply: ReplyData,
    ) {
        self.read_called.fetch_add(1, Ordering::SeqCst);
        reply.data(&vec![b'x'; size as usize]);
    }

    fn fsync(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _datasync: bool, reply: ReplyEmpty,
    ) {
        self.fsync_called.fetch_add(1, Ordering::SeqCst);
        reply.ok();
    }

    fn write(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _offset: u64, data: &[u8],
        _write_flags: crate::fuser_facade::types::WriteFlags,
        _flags: crate::fuser_facade::types::OpenFlags, _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        self.write_called.fetch_add(1, Ordering::SeqCst);
        reply.written(data.len() as u32);
    }

    fn setattr(
        &self, _req: &Request, ino: INodeNo, _mode: Option<u32>, _uid: Option<u32>,
        _gid: Option<u32>, _size: Option<u64>,
        _atime: Option<crate::fuser_facade::types::TimeOrNow>,
        _mtime: Option<crate::fuser_facade::types::TimeOrNow>,
        _ctime: Option<std::time::SystemTime>, _fh: Option<FileHandle>,
        _crtime: Option<std::time::SystemTime>, _chgtime: Option<std::time::SystemTime>,
        _bkuptime: Option<std::time::SystemTime>,
        _flags: Option<crate::fuser_facade::types::BsdFileFlags>, reply: ReplyAttr,
    ) {
        if self.assert_no_forget_before_setattr && ino == INodeNo(42) {
            assert_eq!(self.forget_lookup_total.load(Ordering::SeqCst), 0);
        }
        self.setattr_called.fetch_add(1, Ordering::SeqCst);
        reply.attr(
            &Duration::from_secs(self.attr_ttl_secs),
            &test_file_attr(ino.0),
        );
    }

    fn fallocate(
        &self, _req: &Request, ino: INodeNo, _fh: FileHandle, _offset: u64, _length: u64,
        _mode: i32, reply: ReplyEmpty,
    ) {
        if self.assert_no_forget_before_fallocate && ino == INodeNo(42) {
            assert_eq!(self.forget_lookup_total.load(Ordering::SeqCst), 0);
        }
        self.fallocate_called.fetch_add(1, Ordering::SeqCst);
        reply.ok();
    }

    fn rename(
        &self, _req: &Request, _parent: INodeNo, _name: &OsStr, _newparent: INodeNo,
        _newname: &OsStr, _flags: RenameFlags, reply: ReplyEmpty,
    ) {
        self.rename_called.fetch_add(1, Ordering::SeqCst);
        reply.ok();
    }

    fn create(
        &self, _req: &Request, parent: INodeNo, name: &OsStr, _mode: u32, _umask: u32, _flags: i32,
        reply: ReplyCreate,
    ) {
        self.create_called.fetch_add(1, Ordering::SeqCst);
        assert_eq!(parent, INodeNo::ROOT);
        assert_eq!(name, OsStr::new("dir"));
        let mut attr = test_file_attr(42);
        attr.size = self.write_called.load(Ordering::SeqCst) as u64;
        reply.created(
            &Duration::from_secs(self.entry_ttl_secs),
            &attr,
            Generation(7),
            FileHandle(99),
            crate::fuser_facade::types::FopenFlags::empty(),
        );
    }

    fn open(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        if self.assert_no_forget_before_open {
            assert_eq!(self.forget_lookup_total.load(Ordering::SeqCst), 0);
        }
        self.open_called.fetch_add(1, Ordering::SeqCst);
        assert_eq!(ino, INodeNo(42));
        reply.opened(
            FileHandle(99),
            crate::fuser_facade::types::FopenFlags::empty(),
        );
    }

    fn opendir(&self, _req: &Request, ino: INodeNo, _flags: OpenFlags, reply: ReplyOpen) {
        self.opendir_called.fetch_add(1, Ordering::SeqCst);
        assert_eq!(ino, INodeNo(42));
        reply.opened(
            FileHandle(99),
            crate::fuser_facade::types::FopenFlags::empty(),
        );
    }

    fn forget(&self, _req: &Request, ino: INodeNo, nlookup: u64) {
        assert!(matches!(ino, INodeNo(42) | INodeNo(44) | INodeNo(45)));
        self.forget_called.fetch_add(1, Ordering::SeqCst);
        self.forget_lookup_total
            .fetch_add(nlookup as usize, Ordering::SeqCst);
    }
}

fn test_file_attr(ino: u64) -> crate::fuser_facade::types::FileAttr {
    crate::fuser_facade::types::FileAttr {
        ino: INodeNo(ino),
        size: 0,
        blocks: 0,
        atime: std::time::SystemTime::UNIX_EPOCH,
        mtime: std::time::SystemTime::UNIX_EPOCH,
        ctime: std::time::SystemTime::UNIX_EPOCH,
        crtime: std::time::SystemTime::UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    }
}

fn test_attr_with_kind(ino: u64, kind: FileType) -> crate::fuser_facade::types::FileAttr {
    let mut attr = test_file_attr(ino);
    attr.kind = kind;
    attr
}

fn test_adapter<FS: Filesystem>(fs: FS) -> DokanAdapter<FS> {
    DokanAdapter {
        fs: Arc::new(FsCell(Mutex::new(fs))),
        handles: Arc::new(Mutex::new(HashMap::new())),
        resolver: Arc::new(Mutex::new(PathResolver::default())),
        dir_offsets: Arc::new(Mutex::new(HashMap::new())),
        volume_name: "confuse".to_string(),
        fs_name: "FUSER".to_string(),
        destroyed: Arc::new(AtomicBool::new(false)),
    }
}
