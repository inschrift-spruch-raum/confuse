use std::ffi::OsStr;
use std::io::IoSlice;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use libc::c_int;

use super::super::types::{FileAttr, FileType};

type ReplyResult<T> = Arc<Mutex<Option<Result<T, c_int>>>>;
type DirEntryList = Arc<Mutex<Vec<(u64, i64, FileType, std::ffi::OsString)>>>;
type DirPlusEntryList = Arc<Mutex<Vec<(u64, i64, std::ffi::OsString, FileAttr)>>>;
type StatfsPayload = (u64, u64, u64, u64, u64, u32, u32, u32);

pub(crate) trait ReplySender: Send + Sync + Unpin + 'static {
    #[allow(dead_code)]
    fn send(&self, _data: &[IoSlice<'_>]) -> std::io::Result<()>;
}

#[allow(private_bounds)]
pub trait Reply {
    fn new<S: ReplySender>(unique: u64, sender: S) -> Self;
}

#[derive(Clone, Debug, Default)]
pub struct ReplyEmpty {
    pub(crate) status: ReplyResult<()>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyData {
    pub(crate) data: ReplyResult<Vec<u8>>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyEntry {
    pub(crate) status: ReplyResult<FileAttr>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyAttr {
    pub(crate) status: ReplyResult<FileAttr>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyOpen {
    pub(crate) opened: ReplyResult<(u64, u32)>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyWrite {
    pub(crate) written: ReplyResult<u32>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyStatfs {
    pub(crate) status: ReplyResult<StatfsPayload>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyCreate {
    pub(crate) status: ReplyResult<(FileAttr, u64, u32)>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyLock {
    pub(crate) status: ReplyResult<(u64, u64, i32, u32)>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyBmap {
    pub(crate) status: Arc<Mutex<Option<Result<u64, c_int>>>>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyIoctl {
    pub(crate) status: ReplyResult<(i32, Vec<u8>)>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyLseek {
    pub(crate) status: Arc<Mutex<Option<Result<i64, c_int>>>>,
}
#[derive(Clone, Debug, Default)]
pub struct ReplyXattr {
    pub(crate) status: ReplyResult<Vec<u8>>,
    pub(crate) size_hint: Arc<Mutex<Option<u32>>>,
}
#[derive(Clone, Debug)]
pub struct ReplyDirectory {
    pub(crate) status: ReplyResult<()>,
    pub(crate) entries: DirEntryList,
    pub(crate) full: Arc<Mutex<bool>>,
    pub(crate) max_size: usize,
    pub(crate) used_size: Arc<Mutex<usize>>,
}
#[derive(Clone, Debug)]
pub struct ReplyDirectoryPlus {
    pub(crate) status: ReplyResult<()>,
    pub(crate) entries: DirPlusEntryList,
    pub(crate) full: Arc<Mutex<bool>>,
    pub(crate) max_size: usize,
    pub(crate) used_size: Arc<Mutex<usize>>,
}
#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug)]
pub struct fuse_forget_one {
    pub nodeid: u64,
    pub nlookup: u64,
}

impl ReplyEmpty {
    pub fn ok(self) {
        *self.status.lock().expect("lock") = Some(Ok(()));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}

macro_rules! impl_reply_new_default {
    ($($ty:ty),+ $(,)?) => {
        $(
            #[allow(private_bounds)]
            impl Reply for $ty {
                fn new<S: ReplySender>(_unique: u64, _sender: S) -> Self {
                    <$ty>::default()
                }
            }
        )+
    };
}

impl_reply_new_default!(
    ReplyEmpty,
    ReplyData,
    ReplyEntry,
    ReplyAttr,
    ReplyOpen,
    ReplyWrite,
    ReplyStatfs,
    ReplyCreate,
    ReplyLock,
    ReplyBmap,
    ReplyIoctl,
    ReplyLseek,
    ReplyXattr,
    ReplyDirectory,
    ReplyDirectoryPlus,
);
impl ReplyData {
    pub fn data(self, data: &[u8]) {
        *self.data.lock().expect("lock") = Some(Ok(data.to_vec()));
    }
    pub fn error(self, err: c_int) {
        *self.data.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyEntry {
    pub fn entry(self, _ttl: &Duration, attr: &FileAttr, _generation: u64) {
        *self.status.lock().expect("lock") = Some(Ok(*attr));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyAttr {
    pub fn attr(self, _ttl: &Duration, attr: &FileAttr) {
        *self.status.lock().expect("lock") = Some(Ok(*attr));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyOpen {
    pub fn opened(self, fh: u64, flags: u32) {
        *self.opened.lock().expect("lock") = Some(Ok((fh, flags)));
    }
    pub fn error(self, err: c_int) {
        *self.opened.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyWrite {
    pub fn written(self, size: u32) {
        *self.written.lock().expect("lock") = Some(Ok(size));
    }
    pub fn error(self, err: c_int) {
        *self.written.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyStatfs {
    #[allow(clippy::too_many_arguments)]
    pub fn statfs(
        self, blocks: u64, bfree: u64, bavail: u64, files: u64, ffree: u64, bsize: u32,
        namelen: u32, frsize: u32,
    ) {
        *self.status.lock().expect("lock") = Some(Ok((
            blocks, bfree, bavail, files, ffree, bsize, namelen, frsize,
        )));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyCreate {
    #[allow(clippy::too_many_arguments)]
    pub fn created(self, _ttl: &Duration, attr: &FileAttr, _generation: u64, fh: u64, flags: u32) {
        *self.status.lock().expect("lock") = Some(Ok((*attr, fh, flags)));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyLock {
    pub fn locked(self, start: u64, end: u64, typ: i32, pid: u32) {
        *self.status.lock().expect("lock") = Some(Ok((start, end, typ, pid)));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyBmap {
    pub fn bmap(self, block: u64) {
        *self.status.lock().expect("lock") = Some(Ok(block));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyIoctl {
    pub fn ioctl(self, result: i32, data: &[u8]) {
        *self.status.lock().expect("lock") = Some(Ok((result, data.to_vec())));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyLseek {
    pub fn offset(self, offset: i64) {
        *self.status.lock().expect("lock") = Some(Ok(offset));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}

const FUSE_DIRENT_BASE_SIZE: usize = 24;
const FUSE_DIRENTPLUS_BASE_SIZE: usize = 144;

fn align_8(size: usize) -> usize {
    (size + 7) & !7
}

fn dirent_entry_size(name: &OsStr) -> usize {
    let name_bytes = name.to_string_lossy().len();
    align_8(FUSE_DIRENT_BASE_SIZE + name_bytes)
}

fn direntplus_entry_size(name: &OsStr) -> usize {
    let name_bytes = name.to_string_lossy().len();
    align_8(FUSE_DIRENTPLUS_BASE_SIZE + name_bytes)
}

impl Default for ReplyDirectory {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
            entries: Arc::new(Mutex::new(Vec::new())),
            full: Arc::new(Mutex::new(false)),
            max_size: usize::MAX,
            used_size: Arc::new(Mutex::new(0)),
        }
    }
}

impl Default for ReplyDirectoryPlus {
    fn default() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
            entries: Arc::new(Mutex::new(Vec::new())),
            full: Arc::new(Mutex::new(false)),
            max_size: usize::MAX,
            used_size: Arc::new(Mutex::new(0)),
        }
    }
}

impl ReplyXattr {
    pub fn size(self, size: u32) {
        *self.size_hint.lock().expect("lock") = Some(size);
        *self.status.lock().expect("lock") = Some(Ok(Vec::new()));
    }
    pub fn data(self, data: &[u8]) {
        *self.status.lock().expect("lock") = Some(Ok(data.to_vec()));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyDirectory {
    #[allow(private_bounds)]
    pub fn new<S: ReplySender>(_unique: u64, _sender: S, _size: usize) -> ReplyDirectory {
        ReplyDirectory {
            max_size: _size,
            ..ReplyDirectory::default()
        }
    }

    pub fn add<T: AsRef<OsStr>>(&mut self, ino: u64, offset: i64, kind: FileType, name: T) -> bool {
        let mut full = self.full.lock().expect("lock");
        if *full {
            return true;
        }
        let next_size = dirent_entry_size(name.as_ref());
        let mut used_size = self.used_size.lock().expect("lock");
        if used_size.saturating_add(next_size) > self.max_size {
            *full = true;
            return true;
        }
        let mut entries = self.entries.lock().expect("lock");
        entries.push((ino, offset, kind, name.as_ref().to_os_string()));
        *used_size += next_size;
        false
    }
    pub fn ok(self) {
        *self.status.lock().expect("lock") = Some(Ok(()));
    }
    pub fn error(self, err: c_int) {
        *self.full.lock().expect("lock") = true;
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
impl ReplyDirectoryPlus {
    #[allow(private_bounds)]
    pub fn new<S: ReplySender>(_unique: u64, _sender: S, _size: usize) -> ReplyDirectoryPlus {
        ReplyDirectoryPlus {
            max_size: _size,
            ..ReplyDirectoryPlus::default()
        }
    }

    pub fn add<T: AsRef<OsStr>>(
        &mut self, ino: u64, offset: i64, name: T, _ttl: &Duration, attr: &FileAttr,
        _generation: u64,
    ) -> bool {
        let mut full = self.full.lock().expect("lock");
        if *full {
            return true;
        }
        let next_size = direntplus_entry_size(name.as_ref());
        let mut used_size = self.used_size.lock().expect("lock");
        if used_size.saturating_add(next_size) > self.max_size {
            *full = true;
            return true;
        }
        let mut entries = self.entries.lock().expect("lock");
        entries.push((ino, offset, name.as_ref().to_os_string(), *attr));
        *used_size += next_size;
        false
    }
    pub fn ok(self) {
        *self.status.lock().expect("lock") = Some(Ok(()));
    }
    pub fn error(self, err: c_int) {
        *self.full.lock().expect("lock") = true;
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ChannelSender;

impl ChannelSender {
    pub(crate) fn send(&self, _data: &[IoSlice<'_>]) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReplyPoll {
    pub(crate) status: Arc<Mutex<Option<Result<(), c_int>>>>,
}

impl_reply_new_default!(ReplyPoll,);

impl ReplyPoll {
    pub fn poll(self, _events: u32) {
        *self.status.lock().expect("lock") = Some(Ok(()));
    }
    pub fn error(self, err: c_int) {
        *self.status.lock().expect("lock") = Some(Err(err));
    }
}
