//! In-memory filesystem example using the confuse shim layer.
//!
//! This demonstrates how to write a single FUSE-like filesystem that works on
//! both Linux (via fuser) and Windows (via Dokan) using confuse's unified,
//! fuser-compatible API.
//!
//! Usage:
//!   cargo run --example memfs -- <mountpoint>
//!
//! On Windows the mountpoint should be a drive letter like `M:\` or an empty
//! directory path.  On Linux it can be any directory.

use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

use confuse::{
    FUSE_ROOT_ID, FileAttr, FileType, Filesystem, MountOption, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request, Session,
    TimeOrNow,
};

#[cfg(windows)]
use std::sync::atomic::{AtomicPtr, Ordering as AtomicOrdering};

#[cfg(windows)]
use widestring::U16CStr;

#[cfg(windows)]
static CTRL_MOUNTPOINT_WIDE: AtomicPtr<u16> = AtomicPtr::new(std::ptr::null_mut());

#[cfg(windows)]
unsafe extern "system" fn memfs_ctrl_handler(kind: u32) -> i32 {
    if kind == winapi::um::wincon::CTRL_C_EVENT || kind == winapi::um::wincon::CTRL_BREAK_EVENT {
        let ptr = CTRL_MOUNTPOINT_WIDE.load(AtomicOrdering::SeqCst);
        if !ptr.is_null() {
            let mp = unsafe { U16CStr::from_ptr_str(ptr) };
            eprintln!("Unmounting...");
            let _ = dokan::unmount(mp);
        }
        1
    } else {
        0
    }
}

// ---------------------------------------------------------------------------
// Inode table
// ---------------------------------------------------------------------------

static NEXT_INO: AtomicU64 = AtomicU64::new(FUSE_ROOT_ID + 1);

fn alloc_ino() -> u64 {
    NEXT_INO.fetch_add(1, Ordering::Relaxed)
}

/// Content stored for every inode.
#[derive(Debug)]
struct INode {
    ino: u64,
    parent: u64,
    name: OsString,
    kind: FileType,
    perm: u16,
    nlink: u32,
    uid: u32,
    gid: u32,
    data: Vec<u8>,
    atime: SystemTime,
    mtime: SystemTime,
    ctime: SystemTime,
    crtime: SystemTime,
}

impl INode {
    fn new_dir(ino: u64, parent: u64, name: OsString, perm: u16) -> Self {
        let now = SystemTime::now();
        Self {
            ino,
            parent,
            name,
            kind: FileType::Directory,
            perm,
            nlink: 2, // self + "."
            uid: 0,
            gid: 0,
            data: Vec::new(),
            atime: now,
            mtime: now,
            ctime: now,
            crtime: now,
        }
    }

    fn new_file(ino: u64, parent: u64, name: OsString, perm: u16) -> Self {
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

    fn to_attr(&self) -> FileAttr {
        FileAttr {
            ino: self.ino,
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

// ---------------------------------------------------------------------------
// MemFs
// ---------------------------------------------------------------------------

struct MemFs {
    /// ino -> INode
    inodes: Mutex<BTreeMap<u64, INode>>,
    /// (parent_ino, child_name) -> child_ino
    children: Mutex<BTreeMap<(u64, OsString), u64>>,
}

impl MemFs {
    fn new() -> Self {
        let root = INode::new_dir(FUSE_ROOT_ID, FUSE_ROOT_ID, OsString::new(), 0o755);
        let mut inodes = BTreeMap::new();
        inodes.insert(FUSE_ROOT_ID, root);
        Self {
            inodes: Mutex::new(inodes),
            children: Mutex::new(BTreeMap::new()),
        }
    }

    /// Lookup a child by name under a given parent.
    #[allow(dead_code)]
    fn lookup_child(&self, parent: u64, name: &OsStr) -> Option<u64> {
        self.children
            .lock()
            .unwrap()
            .get(&(parent, name.to_os_string()))
            .copied()
    }
}

// TTL for kernel attribute cache (1 s).
const TTL: Duration = Duration::from_secs(1);

impl Filesystem for MemFs {
    // -- lookup --------------------------------------------------------------

    fn lookup(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEntry) {
        let name = name.to_os_string();
        if let Some(ino) = self.children.lock().unwrap().get(&(parent, name)).copied() {
            let inodes = self.inodes.lock().unwrap();
            if let Some(node) = inodes.get(&ino) {
                reply.entry(&TTL, &node.to_attr(), 0);
                return;
            }
        }
        reply.error(libc::ENOENT);
    }

    // -- getattr -------------------------------------------------------------

    fn getattr(&mut self, _req: &Request, ino: u64, _fh: Option<u64>, reply: ReplyAttr) {
        let inodes = self.inodes.lock().unwrap();
        if let Some(node) = inodes.get(&ino) {
            reply.attr(&TTL, &node.to_attr());
        } else {
            reply.error(libc::ENOENT);
        }
    }

    // -- setattr -------------------------------------------------------------

    fn setattr(
        &mut self, _req: &Request, ino: u64, mode: Option<u32>, uid: Option<u32>,
        gid: Option<u32>, size: Option<u64>, atime: Option<TimeOrNow>, mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>, _fh: Option<u64>, _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>, _bkuptime: Option<SystemTime>, _flags: Option<u32>,
        reply: ReplyAttr,
    ) {
        let mut inodes = self.inodes.lock().unwrap();
        let now = SystemTime::now();
        let node = match inodes.get_mut(&ino) {
            Some(n) => n,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        if let Some(m) = mode {
            node.perm = m as u16;
        }
        if let Some(u) = uid {
            node.uid = u;
        }
        if let Some(g) = gid {
            node.gid = g;
        }
        if let Some(s) = size {
            let s = s as usize;
            if s < node.data.len() {
                node.data.truncate(s);
            } else {
                node.data.resize(s, 0);
            }
            node.mtime = now;
            node.ctime = now;
        }
        if let Some(t) = atime {
            node.atime = match t {
                TimeOrNow::SpecificTime(t) => t,
                TimeOrNow::Now => now,
            };
        }
        if let Some(t) = mtime {
            node.mtime = match t {
                TimeOrNow::SpecificTime(t) => t,
                TimeOrNow::Now => now,
            };
            node.ctime = now;
        }

        reply.attr(&TTL, &node.to_attr());
    }

    // -- readdir -------------------------------------------------------------

    fn readdir(
        &mut self, _req: &Request, ino: u64, _fh: u64, offset: u64, mut reply: ReplyDirectory,
    ) {
        let inodes = self.inodes.lock().unwrap();
        let node = match inodes.get(&ino) {
            Some(n) => n,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };
        if node.kind != FileType::Directory {
            reply.error(libc::ENOTDIR);
            return;
        }
        drop(inodes);

        let children = self.children.lock().unwrap();

        // Standard "." and ".." entries.
        if offset == 0 && reply.add(ino, 1, FileType::Directory, ".") {
            reply.ok();
            return;
        }
        if offset <= 1 {
            let inodes = self.inodes.lock().unwrap();
            let parent_ino = inodes.get(&ino).map(|n| n.parent).unwrap_or(ino);
            if reply.add(parent_ino, 2, FileType::Directory, "..") {
                reply.ok();
                return;
            }
        }

        let mut child_entries: Vec<_> = children
            .iter()
            .filter(|((p, _), _)| *p == ino)
            .map(|(_, &ino)| ino)
            .collect();

        // Stable order.
        child_entries.sort();

        let inodes = self.inodes.lock().unwrap();
        let mut entry_offset = 3u64; // 0=".", 1="..", real entries start at 3
        for (idx, child_ino) in child_entries.iter().enumerate() {
            if entry_offset <= offset {
                entry_offset += 1;
                continue;
            }
            if let Some(child) = inodes.get(child_ino)
                && reply.add(child.ino, (idx + 3) as i64, child.kind, &child.name)
            {
                break;
            }
            entry_offset += 1;
        }
        drop(inodes);

        reply.ok();
    }

    // -- read ----------------------------------------------------------------

    fn read(
        &mut self, _req: &Request, ino: u64, _fh: u64, offset: u64, size: u32, _flags: i32,
        _lock_owner: Option<u64>, reply: ReplyData,
    ) {
        let inodes = self.inodes.lock().unwrap();
        match inodes.get(&ino) {
            Some(node) if node.kind == FileType::RegularFile => {
                let offset = offset as usize;
                let end = std::cmp::min(offset + size as usize, node.data.len());
                if offset >= node.data.len() {
                    reply.data(&[]);
                } else {
                    reply.data(&node.data[offset..end]);
                }
            }
            Some(_) => reply.error(libc::EISDIR),
            None => reply.error(libc::ENOENT),
        }
    }

    // -- write ---------------------------------------------------------------

    fn write(
        &mut self, _req: &Request, ino: u64, _fh: u64, offset: u64, data: &[u8],
        _write_flags: u32, _flags: i32, _lock_owner: Option<u64>, reply: ReplyWrite,
    ) {
        let now = SystemTime::now();
        let mut inodes = self.inodes.lock().unwrap();
        let node = match inodes.get_mut(&ino) {
            Some(n) if n.kind == FileType::RegularFile => n,
            Some(_) => {
                reply.error(libc::EISDIR);
                return;
            }
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        let offset = offset as usize;
        let len = data.len();
        if offset + len > node.data.len() {
            node.data.resize(offset + len, 0);
        }
        node.data[offset..offset + len].copy_from_slice(data);
        node.mtime = now;
        node.ctime = now;
        reply.written(len as u32);
    }

    // -- create --------------------------------------------------------------

    fn create(
        &mut self, _req: &Request, parent: u64, name: &OsStr, mode: u32, _umask: u32,
        flags: i32, reply: ReplyCreate,
    ) {
        let name = name.to_os_string();
        let now = SystemTime::now();

        // Check if already exists.
        {
            let children = self.children.lock().unwrap();
            if children.contains_key(&(parent, name.clone())) {
                reply.error(libc::EEXIST);
                return;
            }
        }

        let ino = alloc_ino();
        let node = INode::new_file(ino, parent, name.clone(), mode as u16);
        let attr = node.to_attr();

        self.inodes.lock().unwrap().insert(ino, node);
        self.children.lock().unwrap().insert((parent, name), ino);

        // Update parent mtime.
        if let Some(p) = self.inodes.lock().unwrap().get_mut(&parent) {
            p.mtime = now;
            p.ctime = now;
        }

        reply.created(&TTL, &attr, 0, ino, flags as u32);
    }

    // -- mkdir ---------------------------------------------------------------

    fn mkdir(
        &mut self, _req: &Request, parent: u64, name: &OsStr, mode: u32, _umask: u32,
        reply: ReplyEntry,
    ) {
        let name = name.to_os_string();
        let now = SystemTime::now();

        {
            let children = self.children.lock().unwrap();
            if children.contains_key(&(parent, name.clone())) {
                reply.error(libc::EEXIST);
                return;
            }
        }

        let ino = alloc_ino();
        let node = INode::new_dir(ino, parent, name.clone(), mode as u16);
        let attr = node.to_attr();

        self.inodes.lock().unwrap().insert(ino, node);
        self.children.lock().unwrap().insert((parent, name), ino);

        // Bump parent nlink and mtime.
        if let Some(p) = self.inodes.lock().unwrap().get_mut(&parent) {
            p.nlink += 1;
            p.mtime = now;
            p.ctime = now;
        }

        reply.entry(&TTL, &attr, 0);
    }

    // -- unlink --------------------------------------------------------------

    fn unlink(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let name = name.to_os_string();
        let now = SystemTime::now();

        let ino = match self.children.lock().unwrap().remove(&(parent, name)) {
            Some(ino) => ino,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        self.inodes.lock().unwrap().remove(&ino);

        if let Some(p) = self.inodes.lock().unwrap().get_mut(&parent) {
            p.mtime = now;
            p.ctime = now;
        }

        reply.ok();
    }

    // -- rmdir ---------------------------------------------------------------

    fn rmdir(&mut self, _req: &Request, parent: u64, name: &OsStr, reply: ReplyEmpty) {
        let name = name.to_os_string();
        let now = SystemTime::now();

        let ino = match self
            .children
            .lock()
            .unwrap()
            .remove(&(parent, name.clone()))
        {
            Some(ino) => ino,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // Check directory is empty.
        let has_children = self.children.lock().unwrap().keys().any(|(p, _)| *p == ino);
        if has_children {
            // Re-insert.
            self.children.lock().unwrap().insert((parent, name), ino);
            reply.error(libc::ENOTEMPTY);
            return;
        }

        self.inodes.lock().unwrap().remove(&ino);

        if let Some(p) = self.inodes.lock().unwrap().get_mut(&parent) {
            p.nlink = p.nlink.saturating_sub(1);
            p.mtime = now;
            p.ctime = now;
        }

        reply.ok();
    }

    // -- rename --------------------------------------------------------------

    fn rename(
        &mut self, _req: &Request, parent: u64, name: &OsStr, newparent: u64, newname: &OsStr,
        _flags: u32, reply: ReplyEmpty,
    ) {
        let old_key = (parent, name.to_os_string());
        let new_key = (newparent, newname.to_os_string());
        let now = SystemTime::now();

        let ino = match self.children.lock().unwrap().remove(&old_key) {
            Some(ino) => ino,
            None => {
                reply.error(libc::ENOENT);
                return;
            }
        };

        // If target exists, remove it (simple replace).
        self.children.lock().unwrap().remove(&new_key);

        self.children.lock().unwrap().insert(new_key.clone(), ino);

        // Update the node's parent/name.
        if let Some(node) = self.inodes.lock().unwrap().get_mut(&ino) {
            node.parent = newparent;
            node.name = newname.to_os_string();
            node.ctime = now;
        }

        // Update parent mtimes.
        let mut inodes = self.inodes.lock().unwrap();
        if let Some(p) = inodes.get_mut(&parent) {
            p.mtime = now;
            p.ctime = now;
        }
        if parent != newparent
            && let Some(p) = inodes.get_mut(&newparent)
        {
            p.mtime = now;
            p.ctime = now;
        }

        reply.ok();
    }

    // -- open / release (minimal) -------------------------------------------

    fn open(&mut self, _req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
        // Verify the inode exists; stateless handles (fh=0).
        if self.inodes.lock().unwrap().contains_key(&ino) {
            reply.opened(0, flags as u32);
        } else {
            reply.error(libc::ENOENT);
        }
    }

    fn release(
        &mut self, _req: &Request, _ino: u64, _fh: u64, _flags: i32, _lock_owner: Option<u64>,
        _flush: bool, reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    // -- opendir / releasedir (minimal) --------------------------------------

    fn opendir(&mut self, _req: &Request, ino: u64, flags: i32, reply: ReplyOpen) {
        let inodes = self.inodes.lock().unwrap();
        match inodes.get(&ino) {
            Some(n) if n.kind == FileType::Directory => reply.opened(0, flags as u32),
            Some(_) => reply.error(libc::ENOTDIR),
            None => reply.error(libc::ENOENT),
        }
    }

    fn releasedir(
        &mut self, _req: &Request, _ino: u64, _fh: u64, _flags: i32, reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    // -- statfs --------------------------------------------------------------

    fn statfs(&mut self, _req: &Request, _ino: u64, reply: ReplyStatfs) {
        // Fabricated values for a 1 GiB volume.
        reply.statfs(
            1024 * 1024, // blocks
            512 * 1024,  // bfree
            512 * 1024,  // bavail
            1000000,     // files
            999000,      // ffree
            4096,        // bsize
            255,         // namelen
            4096,        // frsize
        );
    }

    // -- write via mknod + write is also supported ---------------------------

    fn mknod(
        &mut self, _req: &Request, parent: u64, name: &OsStr, mode: u32, _umask: u32,
        _rdev: u32, reply: ReplyEntry,
    ) {
        let name = name.to_os_string();
        let now = SystemTime::now();

        {
            let children = self.children.lock().unwrap();
            if children.contains_key(&(parent, name.clone())) {
                reply.error(libc::EEXIST);
                return;
            }
        }

        let ino = alloc_ino();
        let node = INode::new_file(ino, parent, name.clone(), mode as u16);
        let attr = node.to_attr();

        self.inodes.lock().unwrap().insert(ino, node);
        self.children.lock().unwrap().insert((parent, name), ino);

        if let Some(p) = self.inodes.lock().unwrap().get_mut(&parent) {
            p.mtime = now;
            p.ctime = now;
        }

        reply.entry(&TTL, &attr, 0);
    }
}

// ---------------------------------------------------------------------------
// main
// ---------------------------------------------------------------------------

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: {} <mountpoint>", args[0]);
        std::process::exit(1);
    }

    let mountpoint = &args[1];

    let options = vec![
        MountOption::FSName("confuse-memfs".to_string()),
        MountOption::RW,
    ];

    let fs = MemFs::new();

    println!("confuse memfs: mounting on {}", mountpoint);
    println!("Press Ctrl-C to unmount.");

    let mut session = Session::new(fs, Path::new(mountpoint), &confuse::Config { mount_options: options, ..confuse::Config::default() })?;

    // On Windows, Ctrl-C normally terminates the process immediately
    // (STATUS_CONTROL_C_EXIT). Register a handler that actively unmounts, so
    // Session::run() can return through DokanWaitForFileSystemClosed.
    #[cfg(windows)]
    {
        use winapi::um::consoleapi::SetConsoleCtrlHandler;

        let wide: Vec<u16> = mountpoint
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let leaked = wide.into_boxed_slice();
        CTRL_MOUNTPOINT_WIDE.store(Box::into_raw(leaked) as *mut u16, AtomicOrdering::SeqCst);

        unsafe {
            SetConsoleCtrlHandler(Some(memfs_ctrl_handler), 1);
        }
    }

    session.run()
}
