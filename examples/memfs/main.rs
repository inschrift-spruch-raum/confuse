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
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use confuse::{
    BsdFileFlags, Errno, FileHandle, FileType, Filesystem, FopenFlags, Generation, INodeNo,
    LockOwner, MountOption, OpenFlags, RenameFlags, ReplyAttr, ReplyCreate, ReplyData,
    ReplyDirectory, ReplyEmpty, ReplyEntry, ReplyOpen, ReplyStatfs, ReplyWrite, Request, TimeOrNow,
    WriteFlags, mount2,
};

mod inode;
mod storage;
use inode::{INode, ROOT_INO_RAW};
use storage::NewNodeKind;

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

fn fopen_flags(bits: u32) -> FopenFlags {
    FopenFlags::from_bits_truncate(bits)
}

#[cfg(windows)]
fn errno(raw: i32) -> Errno {
    Errno::from_raw_os_error(raw)
}

#[cfg(not(windows))]
fn errno(raw: i32) -> Errno {
    Errno::from_i32(raw)
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
        let root = INode::new_dir(ROOT_INO_RAW, ROOT_INO_RAW, OsString::new(), 0o755);
        let mut inodes = BTreeMap::new();
        inodes.insert(ROOT_INO_RAW, root);
        Self {
            inodes: Mutex::new(inodes),
            children: Mutex::new(BTreeMap::new()),
        }
    }
}

// TTL for kernel attribute cache (1 s).
const TTL: Duration = Duration::from_secs(1);

impl Filesystem for MemFs {
    // -- lookup --------------------------------------------------------------

    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let name = name.to_os_string();
        if let Some(ino) = self
            .children
            .lock()
            .unwrap()
            .get(&(parent.0, name))
            .copied()
        {
            let inodes = self.inodes.lock().unwrap();
            if let Some(node) = inodes.get(&ino) {
                reply.entry(&TTL, &node.to_attr(), Generation(0));
                return;
            }
        }
        reply.error(Errno::ENOENT);
    }

    // -- getattr -------------------------------------------------------------

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        let inodes = self.inodes.lock().unwrap();
        if let Some(node) = inodes.get(&ino.0) {
            reply.attr(&TTL, &node.to_attr());
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    // -- setattr -------------------------------------------------------------

    fn setattr(
        &self, _req: &Request, ino: INodeNo, mode: Option<u32>, uid: Option<u32>, gid: Option<u32>,
        size: Option<u64>, atime: Option<TimeOrNow>, mtime: Option<TimeOrNow>,
        _ctime: Option<SystemTime>, _fh: Option<FileHandle>, _crtime: Option<SystemTime>,
        _chgtime: Option<SystemTime>, _bkuptime: Option<SystemTime>, _flags: Option<BsdFileFlags>,
        reply: ReplyAttr,
    ) {
        let mut inodes = self.inodes.lock().unwrap();
        let now = SystemTime::now();
        let node = match inodes.get_mut(&ino.0) {
            Some(n) => n,
            None => {
                reply.error(Errno::ENOENT);
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
        &self, _req: &Request, ino: INodeNo, _fh: FileHandle, offset: u64,
        mut reply: ReplyDirectory,
    ) {
        let inodes = self.inodes.lock().unwrap();
        let node = match inodes.get(&ino.0) {
            Some(n) => n,
            None => {
                reply.error(Errno::ENOENT);
                return;
            }
        };
        if node.kind != FileType::Directory {
            reply.error(errno(libc::ENOTDIR));
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
            let parent_ino = inodes.get(&ino.0).map(|n| n.parent).unwrap_or(ino.0);
            if reply.add(INodeNo(parent_ino), 2, FileType::Directory, "..") {
                reply.ok();
                return;
            }
        }

        let mut child_entries: Vec<_> = children
            .iter()
            .filter(|((p, _), _)| *p == ino.0)
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
                && reply.add(
                    INodeNo(child.ino),
                    (idx + 3) as u64,
                    child.kind,
                    &child.name,
                )
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
        &self, _req: &Request, ino: INodeNo, _fh: FileHandle, offset: u64, size: u32,
        _flags: OpenFlags, _lock_owner: Option<LockOwner>, reply: ReplyData,
    ) {
        let inodes = self.inodes.lock().unwrap();
        match inodes.get(&ino.0) {
            Some(node) if node.kind == FileType::RegularFile => {
                let offset = offset as usize;
                let end = std::cmp::min(offset + size as usize, node.data.len());
                if offset >= node.data.len() {
                    reply.data(&[]);
                } else {
                    reply.data(&node.data[offset..end]);
                }
            }
            Some(_) => reply.error(errno(libc::EISDIR)),
            None => reply.error(Errno::ENOENT),
        }
    }

    // -- write ---------------------------------------------------------------

    fn write(
        &self, _req: &Request, ino: INodeNo, _fh: FileHandle, offset: u64, data: &[u8],
        _write_flags: WriteFlags, _flags: OpenFlags, _lock_owner: Option<LockOwner>,
        reply: ReplyWrite,
    ) {
        let now = SystemTime::now();
        let mut inodes = self.inodes.lock().unwrap();
        let node = match inodes.get_mut(&ino.0) {
            Some(n) if n.kind == FileType::RegularFile => n,
            Some(_) => {
                reply.error(errno(libc::EISDIR));
                return;
            }
            None => {
                reply.error(Errno::ENOENT);
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
        &self, _req: &Request, parent: INodeNo, name: &OsStr, mode: u32, _umask: u32, flags: i32,
        reply: ReplyCreate,
    ) {
        let attr = match storage::insert_node(
            &self.inodes,
            &self.children,
            parent.0,
            name.to_os_string(),
            mode,
            NewNodeKind::File,
        ) {
            Ok(attr) => attr,
            Err(err) => {
                reply.error(err);
                return;
            }
        };

        reply.created(
            &TTL,
            &attr,
            Generation(0),
            FileHandle(attr.ino.0),
            fopen_flags(flags as u32),
        );
    }

    // -- mkdir ---------------------------------------------------------------

    fn mkdir(
        &self, _req: &Request, parent: INodeNo, name: &OsStr, mode: u32, _umask: u32,
        reply: ReplyEntry,
    ) {
        match storage::insert_node(
            &self.inodes,
            &self.children,
            parent.0,
            name.to_os_string(),
            mode,
            NewNodeKind::Directory,
        ) {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(err) => reply.error(err),
        }
    }

    // -- unlink --------------------------------------------------------------

    fn unlink(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let name = name.to_os_string();
        let now = SystemTime::now();

        let ino = match self.children.lock().unwrap().remove(&(parent.0, name)) {
            Some(ino) => ino,
            None => {
                reply.error(Errno::ENOENT);
                return;
            }
        };

        self.inodes.lock().unwrap().remove(&ino);

        if let Some(p) = self.inodes.lock().unwrap().get_mut(&parent.0) {
            p.mtime = now;
            p.ctime = now;
        }

        reply.ok();
    }

    // -- rmdir ---------------------------------------------------------------

    fn rmdir(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEmpty) {
        let name = name.to_os_string();
        let now = SystemTime::now();

        let ino = match self
            .children
            .lock()
            .unwrap()
            .remove(&(parent.0, name.clone()))
        {
            Some(ino) => ino,
            None => {
                reply.error(Errno::ENOENT);
                return;
            }
        };

        // Check directory is empty.
        let has_children = self.children.lock().unwrap().keys().any(|(p, _)| *p == ino);
        if has_children {
            // Re-insert.
            self.children.lock().unwrap().insert((parent.0, name), ino);
            reply.error(errno(libc::ENOTEMPTY));
            return;
        }

        self.inodes.lock().unwrap().remove(&ino);

        if let Some(p) = self.inodes.lock().unwrap().get_mut(&parent.0) {
            p.nlink = p.nlink.saturating_sub(1);
            p.mtime = now;
            p.ctime = now;
        }

        reply.ok();
    }

    // -- rename --------------------------------------------------------------

    fn rename(
        &self, _req: &Request, parent: INodeNo, name: &OsStr, newparent: INodeNo, newname: &OsStr,
        _flags: RenameFlags, reply: ReplyEmpty,
    ) {
        let old_key = (parent.0, name.to_os_string());
        let new_key = (newparent.0, newname.to_os_string());
        let now = SystemTime::now();

        let ino = match self.children.lock().unwrap().remove(&old_key) {
            Some(ino) => ino,
            None => {
                reply.error(Errno::ENOENT);
                return;
            }
        };

        // If target exists, remove it (simple replace).
        self.children.lock().unwrap().remove(&new_key);

        self.children.lock().unwrap().insert(new_key.clone(), ino);

        // Update the node's parent/name.
        if let Some(node) = self.inodes.lock().unwrap().get_mut(&ino) {
            node.parent = newparent.0;
            node.name = newname.to_os_string();
            node.ctime = now;
        }

        // Update parent mtimes.
        let mut inodes = self.inodes.lock().unwrap();
        if let Some(p) = inodes.get_mut(&parent.0) {
            p.mtime = now;
            p.ctime = now;
        }
        if parent != newparent
            && let Some(p) = inodes.get_mut(&newparent.0)
        {
            p.mtime = now;
            p.ctime = now;
        }

        reply.ok();
    }

    // -- open / release (minimal) -------------------------------------------

    fn open(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        // Verify the inode exists; stateless handles (fh=0).
        if self.inodes.lock().unwrap().contains_key(&ino.0) {
            reply.opened(FileHandle(0), fopen_flags(flags.0 as u32));
        } else {
            reply.error(Errno::ENOENT);
        }
    }

    fn release(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: OpenFlags,
        _lock_owner: Option<LockOwner>, _flush: bool, reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    // -- opendir / releasedir (minimal) --------------------------------------

    fn opendir(&self, _req: &Request, ino: INodeNo, flags: OpenFlags, reply: ReplyOpen) {
        let inodes = self.inodes.lock().unwrap();
        match inodes.get(&ino.0) {
            Some(n) if n.kind == FileType::Directory => {
                reply.opened(FileHandle(0), fopen_flags(flags.0 as u32))
            }
            Some(_) => reply.error(errno(libc::ENOTDIR)),
            None => reply.error(Errno::ENOENT),
        }
    }

    fn releasedir(
        &self, _req: &Request, _ino: INodeNo, _fh: FileHandle, _flags: OpenFlags, reply: ReplyEmpty,
    ) {
        reply.ok();
    }

    // -- statfs --------------------------------------------------------------

    fn statfs(&self, _req: &Request, _ino: INodeNo, reply: ReplyStatfs) {
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
        &self, _req: &Request, parent: INodeNo, name: &OsStr, mode: u32, _umask: u32, _rdev: u32,
        reply: ReplyEntry,
    ) {
        match storage::insert_node(
            &self.inodes,
            &self.children,
            parent.0,
            name.to_os_string(),
            mode,
            NewNodeKind::File,
        ) {
            Ok(attr) => reply.entry(&TTL, &attr, Generation(0)),
            Err(err) => reply.error(err),
        }
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

    let mut config = confuse::Config::default();
    config.mount_options = options;

    // On Windows, Ctrl-C normally terminates the process immediately
    // (STATUS_CONTROL_C_EXIT). Register a handler that actively unmounts, so
    // the blocking mount call can return through DokanWaitForFileSystemClosed.
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

    mount2(fs, mountpoint, &config)
}
