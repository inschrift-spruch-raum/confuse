use std::ffi::OsString;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::SystemTime;

use confuse::{FileAttr, FileType, INodeNo};

pub(crate) const ROOT_INO_RAW: u64 = INodeNo::ROOT.0;

static NEXT_INO: AtomicU64 = AtomicU64::new(ROOT_INO_RAW + 1);

pub(crate) fn alloc_ino() -> u64 {
    NEXT_INO.fetch_add(1, Ordering::Relaxed)
}

/// Content stored for every inode.
#[derive(Debug)]
pub(crate) struct INode {
    pub(crate) ino: u64,
    pub(crate) parent: u64,
    pub(crate) name: OsString,
    pub(crate) kind: FileType,
    pub(crate) perm: u16,
    pub(crate) nlink: u32,
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) data: Vec<u8>,
    pub(crate) atime: SystemTime,
    pub(crate) mtime: SystemTime,
    pub(crate) ctime: SystemTime,
    crtime: SystemTime,
}

impl INode {
    pub(crate) fn new_dir(ino: u64, parent: u64, name: OsString, perm: u16) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            parent,
            name,
            kind: FileType::Directory,
            perm,
            nlink: 2,
            uid: 0,
            gid: 0,
            data: Vec::new(),
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
        }
    }

    pub(crate) fn new_file(ino: u64, parent: u64, name: OsString, perm: u16) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            parent,
            name,
            kind: FileType::RegularFile,
            perm,
            nlink: 1,
            uid: 0,
            gid: 0,
            data: Vec::new(),
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
        }
    }

    pub(crate) fn to_attr(&self) -> FileAttr {
        FileAttr {
            ino: INodeNo(self.ino),
            size: self.data.len() as u64,
            blocks: (self.data.len() as u64).div_ceil(512),
            atime: self.atime,
            mtime: self.mtime,
            ctime: self.ctime,
            crtime: self.crtime,
            kind: self.kind,
            perm: self.perm,
            nlink: self.nlink,
            uid: self.uid,
            gid: self.gid,
            rdev: 0,
            blksize: 4096,
            flags: 0,
        }
    }
}
