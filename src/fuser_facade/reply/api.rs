use std::ffi::OsStr;
use std::ffi::OsString;
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
#[cfg(feature = "macos-api")]
use std::time::SystemTime;

use libc::c_int;

use super::super::types::{
    Errno, FileAttr, FileHandle, FileType, FopenFlags, Generation, INodeNo, PollEvents,
};
use super::support::BackingId;

type ReplyResult<T> = Arc<Mutex<Option<Result<T, c_int>>>>;
type DirEntryList = Arc<Mutex<Vec<DirectoryEntryPayload>>>;
type DirPlusEntryList = Arc<Mutex<Vec<DirectoryPlusEntryPayload>>>;
type StatfsPayload = (u64, u64, u64, u64, u64, u32, u32, u32);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReplyEntryPayload {
    pub attr: FileAttr,
    pub ttl: Duration,
    pub generation: Generation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReplyAttrPayload {
    pub attr: FileAttr,
    pub ttl: Duration,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReplyOpenPayload {
    pub fh: FileHandle,
    pub flags: FopenFlags,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ReplyCreatePayload {
    pub attr: FileAttr,
    pub ttl: Duration,
    pub generation: Generation,
    pub fh: FileHandle,
    pub flags: FopenFlags,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DirectoryEntryPayload {
    pub ino: INodeNo,
    pub offset: u64,
    pub kind: FileType,
    pub name: OsString,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DirectoryPlusEntryPayload {
    pub ino: INodeNo,
    pub offset: u64,
    pub name: OsString,
    pub ttl: Duration,
    pub attr: FileAttr,
    pub generation: Generation,
}

#[derive(Debug)]
pub struct ReplyEmpty {
    pub(crate) status: ReplyResult<()>,
}
#[derive(Debug)]
pub struct ReplyData {
    pub(crate) data: ReplyResult<Vec<u8>>,
}
#[derive(Debug)]
pub struct ReplyEntry {
    pub(crate) status: ReplyResult<ReplyEntryPayload>,
}
#[derive(Debug)]
pub struct ReplyAttr {
    pub(crate) status: ReplyResult<FileAttr>,
    pub(crate) payload: ReplyResult<ReplyAttrPayload>,
}
#[derive(Debug)]
pub struct ReplyOpen {
    pub(crate) opened: ReplyResult<ReplyOpenPayload>,
}
#[derive(Debug)]
pub struct ReplyWrite {
    pub(crate) written: ReplyResult<u32>,
}
#[derive(Debug)]
pub struct ReplyStatfs {
    pub(crate) status: ReplyResult<StatfsPayload>,
}
#[derive(Debug)]
pub struct ReplyCreate {
    pub(crate) status: ReplyResult<ReplyCreatePayload>,
}
#[derive(Debug)]
pub struct ReplyLock {
    pub(crate) status: ReplyResult<(u64, u64, i32, u32)>,
}
#[derive(Debug)]
pub struct ReplyBmap {
    pub(crate) status: Arc<Mutex<Option<Result<u64, c_int>>>>,
}
#[derive(Debug)]
pub struct ReplyIoctl {
    pub(crate) status: ReplyResult<(i32, Vec<u8>)>,
}
#[derive(Debug)]
pub struct ReplyLseek {
    pub(crate) status: Arc<Mutex<Option<Result<i64, c_int>>>>,
}
#[derive(Debug)]
pub struct ReplyXattr {
    pub(crate) status: ReplyResult<Vec<u8>>,
    pub(crate) size_hint: Arc<Mutex<Option<u32>>>,
}
#[derive(Debug)]
pub struct ReplyDirectory {
    pub(crate) status: ReplyResult<()>,
    pub(crate) entries: DirEntryList,
    pub(crate) full: Arc<Mutex<bool>>,
    pub(crate) max_size: usize,
    pub(crate) used_size: Arc<Mutex<usize>>,
}
#[derive(Debug)]
pub struct ReplyDirectoryPlus {
    pub(crate) status: ReplyResult<()>,
    pub(crate) entries: DirPlusEntryList,
    pub(crate) full: Arc<Mutex<bool>>,
    pub(crate) max_size: usize,
    pub(crate) used_size: Arc<Mutex<usize>>,
}
impl ReplyEmpty {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }

    pub fn ok(self) {
        *self.status.lock().expect("lock") = Some(Ok(()));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}

impl ReplyData {
    pub(crate) fn capture() -> Self {
        Self {
            data: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            data: Arc::clone(&self.data),
        }
    }
}

impl ReplyEntry {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

impl ReplyAttr {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
            payload: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
            payload: Arc::clone(&self.payload),
        }
    }
}

impl ReplyOpen {
    pub(crate) fn capture() -> Self {
        Self {
            opened: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            opened: Arc::clone(&self.opened),
        }
    }
}

impl ReplyWrite {
    pub(crate) fn capture() -> Self {
        Self {
            written: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            written: Arc::clone(&self.written),
        }
    }
}

impl ReplyStatfs {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

impl ReplyCreate {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

impl ReplyLock {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

impl ReplyBmap {
    #[cfg(test)]
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

impl ReplyIoctl {
    #[cfg(test)]
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

impl ReplyLseek {
    #[cfg(test)]
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

impl ReplyXattr {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
            size_hint: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
            size_hint: Arc::clone(&self.size_hint),
        }
    }
}

impl ReplyDirectory {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
            entries: Arc::new(Mutex::new(Vec::new())),
            full: Arc::new(Mutex::new(false)),
            max_size: usize::MAX,
            used_size: Arc::new(Mutex::new(0)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
            entries: Arc::clone(&self.entries),
            full: Arc::clone(&self.full),
            max_size: self.max_size,
            used_size: Arc::clone(&self.used_size),
        }
    }
}

impl ReplyDirectoryPlus {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
            entries: Arc::new(Mutex::new(Vec::new())),
            full: Arc::new(Mutex::new(false)),
            max_size: usize::MAX,
            used_size: Arc::new(Mutex::new(0)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
            entries: Arc::clone(&self.entries),
            full: Arc::clone(&self.full),
            max_size: self.max_size,
            used_size: Arc::clone(&self.used_size),
        }
    }
}

impl ReplyData {
    pub fn data(self, data: &[u8]) {
        *self.data.lock().expect("lock") = Some(Ok(data.to_vec()));
    }
    pub fn error(self, err: Errno) {
        *self.data.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyEntry {
    pub fn entry(self, ttl: &Duration, attr: &FileAttr, generation: Generation) {
        *self.status.lock().expect("lock") = Some(Ok(ReplyEntryPayload {
            attr: *attr,
            ttl: *ttl,
            generation,
        }));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyAttr {
    pub fn attr(self, ttl: &Duration, attr: &FileAttr) {
        *self.payload.lock().expect("lock") = Some(Ok(ReplyAttrPayload {
            attr: *attr,
            ttl: *ttl,
        }));
        *self.status.lock().expect("lock") = Some(Ok(*attr));
    }
    pub fn error(self, err: Errno) {
        *self.payload.lock().expect("lock") = Some(Err(err.raw_os_error()));
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyOpen {
    pub fn opened(self, fh: FileHandle, flags: FopenFlags) {
        assert!(!flags.contains(FopenFlags::FOPEN_PASSTHROUGH));
        *self.opened.lock().expect("lock") = Some(Ok(ReplyOpenPayload { fh, flags }));
    }
    /// Register a backing fd for kernel passthrough.
    ///
    /// Windows facade compatibility note: upstream fuser 0.17.0 takes
    /// `impl std::os::fd::AsFd` here. That trait cannot be named on the Windows
    /// target, so this facade uses the same short public name mapped to
    /// `rustix::fd::AsFd` and always reports the unsupported passthrough surface
    /// at runtime.
    pub fn open_backing(&self, _fd: impl rustix::fd::AsFd) -> io::Result<BackingId> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "kernel passthrough backing files are unsupported by the Windows Dokan facade",
        ))
    }
    pub fn opened_passthrough(self, fh: FileHandle, flags: FopenFlags, _backing_id: &BackingId) {
        let _fh = fh;
        let _flags = flags;
        *self.opened.lock().expect("lock") = Some(Err(Errno::ENOSYS.raw_os_error()));
    }
    pub fn error(self, err: Errno) {
        *self.opened.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyWrite {
    pub fn written(self, size: u32) {
        *self.written.lock().expect("lock") = Some(Ok(size));
    }
    pub fn error(self, err: Errno) {
        *self.written.lock().expect("lock") = Some(Err(err.raw_os_error()));
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
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyCreate {
    #[allow(clippy::too_many_arguments)]
    pub fn created(
        self, ttl: &Duration, attr: &FileAttr, generation: Generation, fh: FileHandle,
        flags: FopenFlags,
    ) {
        assert!(!flags.contains(FopenFlags::FOPEN_PASSTHROUGH));
        *self.status.lock().expect("lock") = Some(Ok(ReplyCreatePayload {
            attr: *attr,
            ttl: *ttl,
            generation,
            fh,
            flags,
        }));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
    /// Register a backing fd for kernel passthrough.
    ///
    /// Windows facade compatibility note: upstream fuser 0.17.0 takes
    /// `impl std::os::fd::AsFd` here. That trait cannot be named on the Windows
    /// target, so this facade uses the same short public name mapped to
    /// `rustix::fd::AsFd` and always reports the unsupported passthrough surface
    /// at runtime.
    pub fn open_backing(&self, _fd: impl rustix::fd::AsFd) -> io::Result<BackingId> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "kernel passthrough backing files are unsupported by the Windows Dokan facade",
        ))
    }
    #[allow(clippy::too_many_arguments)]
    pub fn created_passthrough(
        self, ttl: &Duration, attr: &FileAttr, generation: Generation, fh: FileHandle,
        flags: FopenFlags, _backing_id: &BackingId,
    ) {
        let _ttl = ttl;
        let _attr = attr;
        let _generation = generation;
        let _fh = fh;
        let _flags = flags;
        *self.status.lock().expect("lock") = Some(Err(Errno::ENOSYS.raw_os_error()));
    }
}
impl ReplyLock {
    pub fn locked(self, start: u64, end: u64, typ: i32, pid: u32) {
        *self.status.lock().expect("lock") = Some(Ok((start, end, typ, pid)));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyBmap {
    pub fn bmap(self, block: u64) {
        *self.status.lock().expect("lock") = Some(Ok(block));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyIoctl {
    pub fn ioctl(self, result: i32, data: &[u8]) {
        *self.status.lock().expect("lock") = Some(Ok((result, data.to_vec())));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyLseek {
    pub fn offset(self, offset: i64) {
        *self.status.lock().expect("lock") = Some(Ok(offset));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
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

impl ReplyXattr {
    pub fn size(self, size: u32) {
        *self.size_hint.lock().expect("lock") = Some(size);
        *self.status.lock().expect("lock") = Some(Ok(Vec::new()));
    }
    pub fn data(self, data: &[u8]) {
        *self.status.lock().expect("lock") = Some(Ok(data.to_vec()));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyDirectory {
    #[cfg(test)]
    pub(crate) fn new<S>(_unique: u64, _sender: S, _size: usize) -> ReplyDirectory {
        ReplyDirectory {
            max_size: _size,
            ..ReplyDirectory::capture()
        }
    }

    pub fn add<T: AsRef<OsStr>>(
        &mut self, ino: INodeNo, offset: u64, kind: FileType, name: T,
    ) -> bool {
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
        entries.push(DirectoryEntryPayload {
            ino,
            offset,
            kind,
            name: name.as_ref().to_os_string(),
        });
        *used_size += next_size;
        false
    }
    pub fn ok(self) {
        *self.status.lock().expect("lock") = Some(Ok(()));
    }
    pub fn error(self, err: Errno) {
        *self.full.lock().expect("lock") = true;
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
impl ReplyDirectoryPlus {
    #[cfg(test)]
    pub(crate) fn new<S>(_unique: u64, _sender: S, _size: usize) -> ReplyDirectoryPlus {
        ReplyDirectoryPlus {
            max_size: _size,
            ..ReplyDirectoryPlus::capture()
        }
    }

    pub fn add<T: AsRef<OsStr>>(
        &mut self, ino: INodeNo, offset: u64, name: T, ttl: &Duration, attr: &FileAttr,
        generation: Generation,
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
        entries.push(DirectoryPlusEntryPayload {
            ino,
            offset,
            name: name.as_ref().to_os_string(),
            ttl: *ttl,
            attr: *attr,
            generation,
        });
        *used_size += next_size;
        false
    }
    pub fn ok(self) {
        *self.status.lock().expect("lock") = Some(Ok(()));
    }
    pub fn error(self, err: Errno) {
        *self.full.lock().expect("lock") = true;
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}

#[derive(Debug)]
pub struct ReplyPoll {
    pub(crate) status: Arc<Mutex<Option<Result<PollEvents, c_int>>>>,
}

impl ReplyPoll {
    #[cfg(test)]
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    #[cfg(test)]
    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

impl ReplyPoll {
    pub fn poll(self, events: PollEvents) {
        *self.status.lock().expect("lock") = Some(Ok(events));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}

/// Reply to a `getxtimes` request — query extended times (`bkuptime`, `crtime`).
///
/// Set `fuse_init_out.flags` to `FUSE_XTIMES` during init to enable. Matches
/// upstream fuser 0.17.0 `reply.rs:275-294`: the public `xtimes` setter takes
/// **exactly 2 fields** (`bkuptime` + `crtime`). Do NOT add `chgtime` or
/// `flags` — those are not part of the FUSE xtimes protocol.
#[cfg(feature = "macos-api")]
#[derive(Debug)]
pub struct ReplyXTimes {
    pub(crate) status: ReplyResult<(SystemTime, SystemTime)>,
}

#[cfg(all(feature = "macos-api", test))]
impl ReplyXTimes {
    pub(crate) fn capture() -> Self {
        Self {
            status: Arc::new(Mutex::new(None)),
        }
    }

    pub(crate) fn duplicate(&self) -> Self {
        Self {
            status: Arc::clone(&self.status),
        }
    }
}

#[cfg(feature = "macos-api")]
impl ReplyXTimes {
    /// Reply to a `getxtimes` request with the given extended times.
    ///
    /// **Only 2 fields** (`bkuptime` + `crtime`); upstream fuser 0.17.0
    /// `ReplyXTimes::xtimes` at `reply.rs:291` accepts exactly 2. Do NOT add
    /// `chgtime` or `flags` (v4 correction — these are NOT in FUSE xtimes
    /// protocol).
    pub fn xtimes(self, bkuptime: SystemTime, crtime: SystemTime) {
        *self.status.lock().expect("lock") = Some(Ok((bkuptime, crtime)));
    }
    pub fn error(self, err: Errno) {
        *self.status.lock().expect("lock") = Some(Err(err.raw_os_error()));
    }
}
