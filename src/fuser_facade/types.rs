#[cfg(feature = "serializable")]
use serde::Deserialize;
#[cfg(feature = "serializable")]
use serde::Serialize;
use std::fmt;
use std::num::NonZeroI32;
use std::time::Duration;
use std::time::SystemTime;

use bitflags::bitflags;

macro_rules! u64_newtype {
    ($name:ident) => {
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        #[cfg_attr(feature = "serializable", derive(Serialize, Deserialize))]
        pub struct $name(pub u64);

        impl From<$name> for u64 {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
    ($name:ident, no_hash) => {
        #[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
        #[cfg_attr(feature = "serializable", derive(Serialize, Deserialize))]
        pub struct $name(pub u64);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                fmt::Display::fmt(&self.0, f)
            }
        }
    };
}

u64_newtype!(INodeNo);
u64_newtype!(FileHandle);
u64_newtype!(LockOwner, no_hash);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Generation(pub u64);

impl From<Generation> for u64 {
    fn from(value: Generation) -> Self {
        value.0
    }
}

impl INodeNo {
    pub const ROOT: Self = Self(1);
}

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct InitFlags: u64 {
        const FUSE_ASYNC_READ = 1 << 0;
        const FUSE_POSIX_LOCKS = 1 << 1;
        const FUSE_FILE_OPS = 1 << 2;
        const FUSE_ATOMIC_O_TRUNC = 1 << 3;
        const FUSE_EXPORT_SUPPORT = 1 << 4;
        const FUSE_BIG_WRITES = 1 << 5;
        const FUSE_DONT_MASK = 1 << 6;
        const FUSE_SPLICE_WRITE = 1 << 7;
        const FUSE_SPLICE_MOVE = 1 << 8;
        const FUSE_SPLICE_READ = 1 << 9;
        const FUSE_FLOCK_LOCKS = 1 << 10;
        const FUSE_HAS_IOCTL_DIR = 1 << 11;
        const FUSE_AUTO_INVAL_DATA = 1 << 12;
        const FUSE_DO_READDIRPLUS = 1 << 13;
        const FUSE_READDIRPLUS_AUTO = 1 << 14;
        const FUSE_ASYNC_DIO = 1 << 15;
        const FUSE_WRITEBACK_CACHE = 1 << 16;
        const FUSE_NO_OPEN_SUPPORT = 1 << 17;
        const FUSE_PARALLEL_DIROPS = 1 << 18;
        const FUSE_HANDLE_KILLPRIV = 1 << 19;
        const FUSE_POSIX_ACL = 1 << 20;
        const FUSE_ABORT_ERROR = 1 << 21;
        const FUSE_MAX_PAGES = 1 << 22;
        const FUSE_CACHE_SYMLINKS = 1 << 23;
        const FUSE_NO_OPENDIR_SUPPORT = 1 << 24;
        const FUSE_EXPLICIT_INVAL_DATA = 1 << 25;
        const FUSE_MAP_ALIGNMENT = 1 << 26;
        const FUSE_SUBMOUNTS = 1 << 27;
        const FUSE_HANDLE_KILLPRIV_V2 = 1 << 28;
        const FUSE_SETXATTR_EXT = 1 << 29;
        const FUSE_INIT_EXT = 1 << 30;
        const FUSE_INIT_RESERVED = 1 << 31;
        const FUSE_SECURITY_CTX = 1 << 32;
        const FUSE_HAS_INODE_DAX = 1 << 33;
        const FUSE_CREATE_SUPP_GROUP = 1 << 34;
        const FUSE_HAS_EXPIRE_ONLY = 1 << 35;
        const FUSE_DIRECT_IO_ALLOW_MMAP = 1 << 36;
        const FUSE_PASSTHROUGH = 1 << 37;
        const FUSE_NO_EXPORT_SUPPORT = 1 << 38;
        const FUSE_HAS_RESEND = 1 << 39;
        const FUSE_ALLOW_IDMAP = 1 << 40;
        const FUSE_OVER_IO_URING = 1 << 41;
        const FUSE_REQUEST_TIMEOUT = 1 << 42;
        #[cfg(feature = "macos-api")]
        const FUSE_ALLOCATE = 1 << 27;
        #[cfg(feature = "macos-api")]
        const FUSE_EXCHANGE_DATA = 1 << 28;
        #[cfg(feature = "macos-api")]
        const FUSE_CASE_INSENSITIVE = 1 << 29;
        #[cfg(feature = "macos-api")]
        const FUSE_VOL_RENAME = 1 << 30;
        #[cfg(feature = "macos-api")]
        const FUSE_XTIMES = 1 << 31;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct FopenFlags: u32 {
        const FOPEN_DIRECT_IO = 1 << 0;
        const FOPEN_KEEP_CACHE = 1 << 1;
        const FOPEN_NONSEEKABLE = 1 << 2;
        const FOPEN_CACHE_DIR = 1 << 3;
        const FOPEN_STREAM = 1 << 4;
        const FOPEN_NOFLUSH = 1 << 5;
        const FOPEN_PARALLEL_DIRECT_WRITES = 1 << 6;
        const FOPEN_PASSTHROUGH = 1 << 7;
        #[cfg(feature = "macos-api")]
        const FOPEN_PURGE_ATTR = 1 << 30;
        #[cfg(feature = "macos-api")]
        const FOPEN_PURGE_UBC = 1 << 31;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct WriteFlags: u32 {
        const FUSE_WRITE_CACHE = 1 << 0;
        const FUSE_WRITE_LOCKOWNER = 1 << 1;
        const FUSE_WRITE_KILL_SUIDGID = 1 << 2;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct RenameFlags: u32 {
        #[cfg(target_os = "linux")]
        const RENAME_NOREPLACE = libc::RENAME_NOREPLACE;
        #[cfg(target_os = "linux")]
        const RENAME_EXCHANGE = libc::RENAME_EXCHANGE;
        #[cfg(target_os = "linux")]
        const RENAME_WHITEOUT = libc::RENAME_WHITEOUT;
    }

    #[derive(Copy, Clone, Debug, Eq, PartialEq, Hash)]
    pub struct AccessFlags: i32 {
        const F_OK = 0;
        const R_OK = 4;
        const W_OK = 2;
        const X_OK = 1;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct IoctlFlags: u32 {
        const FUSE_IOCTL_COMPAT = 1 << 0;
        const FUSE_IOCTL_UNRESTRICTED = 1 << 1;
        const FUSE_IOCTL_RETRY = 1 << 2;
        const FUSE_IOCTL_32BIT = 1 << 3;
        const FUSE_IOCTL_DIR = 1 << 4;
        const FUSE_IOCTL_COMPAT_X32 = 1 << 5;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PollFlags: u32 {
        const FUSE_POLL_SCHEDULE_NOTIFY = 1 << 0;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct PollEvents: u32 {
        const POLLIN = 0x0001;
        const POLLPRI = 0x0002;
        const POLLOUT = 0x0004;
        const POLLERR = 0x0008;
        const POLLHUP = 0x0010;
        const POLLNVAL = 0x0020;
        const POLLRDNORM = 0x0040;
        const POLLRDBAND = 0x0080;
        const POLLWRNORM = 0x0100;
        const POLLWRBAND = 0x0200;
    }

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct CopyFileRangeFlags: u64 {}

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub struct BsdFileFlags: u32 {
        #[cfg(feature = "macos-api")]
        const UF_NODUMP = 0x0000_0001;
        #[cfg(feature = "macos-api")]
        const UF_IMMUTABLE = 0x0000_0002;
        #[cfg(feature = "macos-api")]
        const UF_APPEND = 0x0000_0004;
        #[cfg(feature = "macos-api")]
        const UF_OPAQUE = 0x0000_0008;
        #[cfg(feature = "macos-api")]
        const UF_HIDDEN = 0x0000_8000;
        #[cfg(feature = "macos-api")]
        const SF_ARCHIVED = 0x0001_0000;
        #[cfg(feature = "macos-api")]
        const SF_IMMUTABLE = 0x0002_0000;
        #[cfg(feature = "macos-api")]
        const SF_APPEND = 0x0004_0000;
    }
}

impl fmt::Display for AccessFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.bits(), f)
    }
}

impl fmt::Display for RenameFlags {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.bits(), f)
    }
}

impl fmt::Display for PollEvents {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.bits(), f)
    }
}

#[allow(non_camel_case_types)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[repr(i32)]
pub enum OpenAccMode {
    O_RDONLY = libc::O_RDONLY,
    O_WRONLY = libc::O_WRONLY,
    O_RDWR = libc::O_RDWR,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OpenFlags(pub i32);

impl OpenFlags {
    pub fn acc_mode(self) -> OpenAccMode {
        const O_ACCMODE: i32 = libc::O_RDONLY | libc::O_WRONLY | libc::O_RDWR;
        match self.0 & O_ACCMODE {
            libc::O_WRONLY => OpenAccMode::O_WRONLY,
            libc::O_RDWR => OpenAccMode::O_RDWR,
            _ => OpenAccMode::O_RDONLY,
        }
    }
}

impl std::fmt::LowerHex for OpenFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::LowerHex::fmt(&self.0, f)
    }
}

impl std::fmt::UpperHex for OpenFlags {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::UpperHex::fmt(&self.0, f)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Errno(NonZeroI32);

impl Errno {
    pub const EPERM: Self = Self::from_raw_os_error(libc::EPERM);
    pub const ENOENT: Self = Self::from_raw_os_error(libc::ENOENT);
    pub const ESRCH: Self = Self::from_raw_os_error(libc::ESRCH);
    pub const EINTR: Self = Self::from_raw_os_error(libc::EINTR);
    pub const EIO: Self = Self::from_raw_os_error(libc::EIO);
    pub const ENXIO: Self = Self::from_raw_os_error(libc::ENXIO);
    pub const E2BIG: Self = Self::from_raw_os_error(libc::E2BIG);
    pub const ENOEXEC: Self = Self::from_raw_os_error(libc::ENOEXEC);
    pub const EBADF: Self = Self::from_raw_os_error(libc::EBADF);
    pub const ECHILD: Self = Self::from_raw_os_error(libc::ECHILD);
    pub const EAGAIN: Self = Self::from_raw_os_error(libc::EAGAIN);
    pub const ENOMEM: Self = Self::from_raw_os_error(libc::ENOMEM);
    pub const EACCES: Self = Self::from_raw_os_error(libc::EACCES);
    pub const EFAULT: Self = Self::from_raw_os_error(libc::EFAULT);
    pub const ENOTBLK: Self = Self::from_raw_os_error(15);
    pub const EBUSY: Self = Self::from_raw_os_error(libc::EBUSY);
    pub const EEXIST: Self = Self::from_raw_os_error(libc::EEXIST);
    pub const EXDEV: Self = Self::from_raw_os_error(libc::EXDEV);
    pub const ENODEV: Self = Self::from_raw_os_error(libc::ENODEV);
    pub const ENOTDIR: Self = Self::from_raw_os_error(libc::ENOTDIR);
    pub const EISDIR: Self = Self::from_raw_os_error(libc::EISDIR);
    pub const EINVAL: Self = Self::from_raw_os_error(libc::EINVAL);
    pub const ENFILE: Self = Self::from_raw_os_error(libc::ENFILE);
    pub const EMFILE: Self = Self::from_raw_os_error(libc::EMFILE);
    pub const ENOTTY: Self = Self::from_raw_os_error(libc::ENOTTY);
    pub const ETXTBSY: Self = Self::from_raw_os_error(libc::ETXTBSY);
    pub const EFBIG: Self = Self::from_raw_os_error(libc::EFBIG);
    pub const ENOSPC: Self = Self::from_raw_os_error(libc::ENOSPC);
    pub const ESPIPE: Self = Self::from_raw_os_error(libc::ESPIPE);
    pub const EROFS: Self = Self::from_raw_os_error(libc::EROFS);
    pub const EMLINK: Self = Self::from_raw_os_error(libc::EMLINK);
    pub const EPIPE: Self = Self::from_raw_os_error(libc::EPIPE);
    pub const EDOM: Self = Self::from_raw_os_error(libc::EDOM);
    pub const ERANGE: Self = Self::from_raw_os_error(libc::ERANGE);
    pub const EDEADLK: Self = Self::from_raw_os_error(libc::EDEADLK);
    pub const ENAMETOOLONG: Self = Self::from_raw_os_error(libc::ENAMETOOLONG);
    pub const ENOLCK: Self = Self::from_raw_os_error(libc::ENOLCK);
    pub const ENOSYS: Self = Self::from_raw_os_error(libc::ENOSYS);
    pub const ENOTEMPTY: Self = Self::from_raw_os_error(libc::ENOTEMPTY);
    pub const ELOOP: Self = Self::from_raw_os_error(libc::ELOOP);
    pub const EWOULDBLOCK: Self = Self::from_raw_os_error(libc::EWOULDBLOCK);
    pub const ENOMSG: Self = Self::from_raw_os_error(libc::ENOMSG);
    pub const EIDRM: Self = Self::from_raw_os_error(libc::EIDRM);
    pub const EREMOTE: Self = Self::from_raw_os_error(66);
    pub const ENOLINK: Self = Self::from_raw_os_error(libc::ENOLINK);
    pub const EPROTO: Self = Self::from_raw_os_error(libc::EPROTO);
    pub const EMULTIHOP: Self = Self::from_raw_os_error(72);
    pub const EBADMSG: Self = Self::from_raw_os_error(libc::EBADMSG);
    pub const EOVERFLOW: Self = Self::from_raw_os_error(libc::EOVERFLOW);
    pub const EILSEQ: Self = Self::from_raw_os_error(libc::EILSEQ);
    pub const EUSERS: Self = Self::from_raw_os_error(87);
    pub const ENOTSOCK: Self = Self::from_raw_os_error(libc::ENOTSOCK);
    pub const EDESTADDRREQ: Self = Self::from_raw_os_error(libc::EDESTADDRREQ);
    pub const EMSGSIZE: Self = Self::from_raw_os_error(libc::EMSGSIZE);
    pub const EPROTOTYPE: Self = Self::from_raw_os_error(libc::EPROTOTYPE);
    pub const ENOPROTOOPT: Self = Self::from_raw_os_error(libc::ENOPROTOOPT);
    pub const EPROTONOSUPPORT: Self = Self::from_raw_os_error(libc::EPROTONOSUPPORT);
    pub const ESOCKTNOSUPPORT: Self = Self::from_raw_os_error(94);
    pub const EOPNOTSUPP: Self = Self::from_raw_os_error(libc::EOPNOTSUPP);
    pub const EPFNOSUPPORT: Self = Self::from_raw_os_error(96);
    pub const EAFNOSUPPORT: Self = Self::from_raw_os_error(libc::EAFNOSUPPORT);
    pub const EADDRINUSE: Self = Self::from_raw_os_error(libc::EADDRINUSE);
    pub const EADDRNOTAVAIL: Self = Self::from_raw_os_error(libc::EADDRNOTAVAIL);
    pub const ENETDOWN: Self = Self::from_raw_os_error(libc::ENETDOWN);
    pub const ENETUNREACH: Self = Self::from_raw_os_error(libc::ENETUNREACH);
    pub const ENETRESET: Self = Self::from_raw_os_error(libc::ENETRESET);
    pub const ECONNABORTED: Self = Self::from_raw_os_error(libc::ECONNABORTED);
    pub const ECONNRESET: Self = Self::from_raw_os_error(libc::ECONNRESET);
    pub const ENOBUFS: Self = Self::from_raw_os_error(libc::ENOBUFS);
    pub const EISCONN: Self = Self::from_raw_os_error(libc::EISCONN);
    pub const ENOTCONN: Self = Self::from_raw_os_error(libc::ENOTCONN);
    pub const ESHUTDOWN: Self = Self::from_raw_os_error(108);
    pub const ETOOMANYREFS: Self = Self::from_raw_os_error(109);
    pub const ETIMEDOUT: Self = Self::from_raw_os_error(libc::ETIMEDOUT);
    pub const ECONNREFUSED: Self = Self::from_raw_os_error(libc::ECONNREFUSED);
    pub const EHOSTDOWN: Self = Self::from_raw_os_error(112);
    pub const EHOSTUNREACH: Self = Self::from_raw_os_error(libc::EHOSTUNREACH);
    pub const EALREADY: Self = Self::from_raw_os_error(libc::EALREADY);
    pub const EINPROGRESS: Self = Self::from_raw_os_error(libc::EINPROGRESS);
    pub const ESTALE: Self = Self::from_raw_os_error(116);
    pub const EDQUOT: Self = Self::from_raw_os_error(122);
    pub const ECANCELED: Self = Self::from_raw_os_error(libc::ECANCELED);
    pub const EOWNERDEAD: Self = Self::from_raw_os_error(libc::EOWNERDEAD);
    pub const ENOTRECOVERABLE: Self = Self::from_raw_os_error(libc::ENOTRECOVERABLE);
    pub const ENOTSUP: Self = Self::from_raw_os_error(libc::ENOTSUP);
    pub const EFTYPE: Self = Self::from_raw_os_error(79);
    pub const ENODATA: Self = Self::from_raw_os_error(libc::ENODATA);
    #[cfg(feature = "macos-api")]
    pub const ENOATTR: Self = Self::from_raw_os_error(93);
    #[cfg(feature = "macos-api")]
    pub const NO_XATTR: Self = Self::ENOATTR;
    #[cfg(not(feature = "macos-api"))]
    pub const NO_XATTR: Self = Self::ENODATA;

    pub const fn from_raw_os_error(error: i32) -> Self {
        match NonZeroI32::new(error) {
            Some(value) => Self(value),
            None => panic!("errno must be non-zero"),
        }
    }

    pub fn from_i32(error: i32) -> Self {
        NonZeroI32::new(error)
            .filter(|error| error.get() > 0)
            .map(Self)
            .unwrap_or(Self::EIO)
    }

    pub const fn code(self) -> i32 {
        self.0.get()
    }

    pub const fn raw_os_error(self) -> i32 {
        self.code()
    }
}

impl From<std::io::Error> for Errno {
    fn from(value: std::io::Error) -> Self {
        value
            .raw_os_error()
            .map(Self::from_i32)
            .unwrap_or(Self::EIO)
    }
}

impl From<std::io::ErrorKind> for Errno {
    fn from(value: std::io::ErrorKind) -> Self {
        std::io::Error::from(value).into()
    }
}

impl From<Errno> for i32 {
    fn from(value: Errno) -> Self {
        value.raw_os_error()
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serializable", derive(Serialize, Deserialize))]
pub struct RequestId(pub u64);

impl From<RequestId> for u64 {
    fn from(value: RequestId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[cfg_attr(feature = "serializable", derive(Serialize, Deserialize))]
pub struct Version(pub u32, pub u32);

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.0, self.1)
    }
}

#[derive(Default, Debug, Eq, PartialEq, Clone, Copy)]
pub enum SessionACL {
    All,
    RootAndOwner,
    #[default]
    Owner,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
#[non_exhaustive]
pub struct Config {
    pub mount_options: Vec<MountOption>,
    pub acl: SessionACL,
    pub n_threads: Option<usize>,
    pub clone_fd: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[cfg_attr(feature = "serializable", derive(Serialize, Deserialize))]
pub enum FileType {
    NamedPipe,
    CharDevice,
    BlockDevice,
    Directory,
    RegularFile,
    Symlink,
    Socket,
}

impl FileType {
    pub fn from_std(file_type: std::fs::FileType) -> Option<Self> {
        if file_type.is_file() {
            Some(Self::RegularFile)
        } else if file_type.is_dir() {
            Some(Self::Directory)
        } else if file_type.is_symlink() {
            Some(Self::Symlink)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "serializable", derive(Serialize, Deserialize))]
pub struct FileAttr {
    pub ino: INodeNo,
    pub size: u64,
    pub blocks: u64,
    pub atime: SystemTime,
    pub mtime: SystemTime,
    pub ctime: SystemTime,
    pub crtime: SystemTime,
    pub kind: FileType,
    pub perm: u16,
    pub nlink: u32,
    pub uid: u32,
    pub gid: u32,
    pub rdev: u32,
    pub blksize: u32,
    pub flags: u32,
}

#[derive(Debug)]
pub struct KernelConfig {
    pub(crate) max_write: u32,
    pub(crate) max_readahead: u32,
    pub(crate) max_max_readahead: u32,
    pub(crate) capabilities: InitFlags,
    pub(crate) requested: InitFlags,
    pub(crate) max_background: u16,
    pub(crate) congestion_threshold: Option<u16>,
    pub(crate) time_gran: Duration,
    pub(crate) max_stack_depth: u32,
    pub(crate) kernel_abi: Version,
}

impl KernelConfig {
    pub fn set_max_stack_depth(&mut self, value: u32) -> Result<u32, u32> {
        const FILESYSTEM_MAX_STACK_DEPTH: u32 = 2;
        if value > FILESYSTEM_MAX_STACK_DEPTH {
            return Err(FILESYSTEM_MAX_STACK_DEPTH);
        }
        let previous = self.max_stack_depth;
        self.max_stack_depth = value;
        Ok(previous)
    }

    pub fn set_max_write(&mut self, value: u32) -> Result<u32, u32> {
        if value == 0 {
            return Err(1);
        }
        const MAX_WRITE_SIZE: u32 = 16 * 1024 * 1024;
        if value > MAX_WRITE_SIZE {
            return Err(MAX_WRITE_SIZE);
        }
        let previous = self.max_write;
        self.max_write = value;
        Ok(previous)
    }

    pub fn set_max_readahead(&mut self, value: u32) -> Result<u32, u32> {
        if value == 0 {
            return Err(1);
        }
        if value > self.max_max_readahead {
            return Err(self.max_max_readahead);
        }
        let previous = self.max_readahead;
        self.max_readahead = value;
        Ok(previous)
    }

    pub fn capabilities(&self) -> InitFlags {
        self.capabilities & !InitFlags::FUSE_INIT_EXT
    }

    pub fn kernel_abi(&self) -> Version {
        self.kernel_abi
    }

    pub fn add_capabilities(&mut self, capabilities_to_add: InitFlags) -> Result<(), InitFlags> {
        if !self.capabilities.contains(capabilities_to_add) {
            return Err(capabilities_to_add & !self.capabilities);
        }
        self.requested |= capabilities_to_add;
        Ok(())
    }

    pub fn set_max_background(&mut self, value: u16) -> Result<u16, u16> {
        if value == 0 {
            return Err(1);
        }
        let previous = self.max_background;
        self.max_background = value;
        Ok(previous)
    }

    pub fn set_congestion_threshold(&mut self, value: u16) -> Result<u16, u16> {
        if value == 0 {
            return Err(1);
        }
        let previous = self
            .congestion_threshold
            .unwrap_or_else(|| (u32::from(self.max_background) * 3 / 4) as u16)
            .min(self.max_background);
        self.congestion_threshold = Some(value);
        Ok(previous)
    }

    pub fn set_time_granularity(&mut self, value: Duration) -> Result<Duration, Duration> {
        if value.as_nanos() == 0 {
            return Err(Duration::new(0, 1));
        }
        if value.as_secs() > 1 || (value.as_secs() == 1 && value.subsec_nanos() > 0) {
            return Err(Duration::new(1, 0));
        }
        let mut power_of_10: u128 = 1;
        let nanos = value.as_nanos();
        while power_of_10 < nanos {
            if nanos < power_of_10 * 10 {
                return Err(Duration::new(0, power_of_10 as u32));
            }
            power_of_10 *= 10;
        }
        let previous = self.time_gran;
        self.time_gran = value;
        Ok(previous)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MountOption {
    FSName(String),
    Subtype(String),
    CUSTOM(String),
    AutoUnmount,
    DefaultPermissions,
    Dev,
    NoDev,
    Suid,
    NoSuid,
    RO,
    RW,
    Exec,
    NoExec,
    Atime,
    NoAtime,
    DirSync,
    Sync,
    Async,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum TimeOrNow {
    SpecificTime(SystemTime),
    Now,
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests;
