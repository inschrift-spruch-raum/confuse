use std::time::Duration;
use std::time::SystemTime;

pub type INodeNo = u64;
pub type FileHandle = u64;
pub type LockOwner = u64;
pub type Generation = u64;
pub type InitFlags = u32;
pub type OpenFlags = i32;
pub type FopenFlags = u32;
pub type WriteFlags = u32;
pub type RenameFlags = u32;
pub type AccessFlags = i32;
pub type IoctlFlags = u32;
pub type PollFlags = u32;
pub type PollEvents = u32;
pub type CopyFileRangeFlags = u32;
pub type BsdFileFlags = u32;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct RequestId(pub u64);

impl From<RequestId> for u64 {
    fn from(value: RequestId) -> Self {
        value.0
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct Version {
    pub major: u32,
    pub minor: u32,
}

#[derive(Default, Debug, Eq, PartialEq, Clone, Copy)]
pub enum SessionACL {
    All,
    RootAndOwner,
    #[default]
    Owner,
}

#[derive(Debug, Clone, Default, Eq, PartialEq)]
pub struct Config {
    pub mount_options: Vec<MountOption>,
    pub acl: SessionACL,
    pub n_threads: Option<usize>,
    pub clone_fd: bool,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum FileType {
    NamedPipe,
    CharDevice,
    BlockDevice,
    Directory,
    RegularFile,
    Symlink,
    Socket,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct FileAttr {
    pub ino: u64,
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
    pub(crate) capabilities: u32,
    pub(crate) requested: u32,
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
        self.capabilities
    }

    pub fn kernel_abi(&self) -> Version {
        self.kernel_abi
    }

    pub fn add_capabilities(&mut self, capabilities_to_add: InitFlags) -> Result<(), InitFlags> {
        if capabilities_to_add & self.capabilities != capabilities_to_add {
            return Err(capabilities_to_add - (capabilities_to_add & self.capabilities));
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
        let previous = self.congestion_threshold.unwrap_or(self.max_background);
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
    AllowOther,
    AllowRoot,
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

#[derive(Clone, Copy, Debug)]
pub enum TimeOrNow {
    SpecificTime(SystemTime),
    Now,
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dokan_impl::default_kernel_config;
    use std::time::Duration;
    use std::time::UNIX_EPOCH;

    #[test]
    fn kernel_config_set_max_write_happy_path() {
        let mut cfg = default_kernel_config();
        assert_eq!(cfg.set_max_write(4096), Ok(128 * 1024));
        assert_eq!(cfg.set_max_write(8192), Ok(4096));
        assert_eq!(cfg.max_write, 8192);
    }

    #[test]
    fn kernel_config_set_max_write_rejects_zero() {
        let mut cfg = default_kernel_config();
        assert_eq!(cfg.set_max_write(0), Err(1));
    }

    #[test]
    fn kernel_config_set_max_write_rejects_too_large() {
        let mut cfg = default_kernel_config();
        assert_eq!(
            cfg.set_max_write(16 * 1024 * 1024 + 1),
            Err(16 * 1024 * 1024)
        );
        assert!(cfg.set_max_write(16 * 1024 * 1024).is_ok());
    }

    #[test]
    fn kernel_config_set_max_readahead_happy_path() {
        let mut cfg = default_kernel_config();
        let previous = cfg.set_max_readahead(4096).expect("should succeed");
        assert_eq!(previous, 128 * 1024);
        assert_eq!(cfg.max_readahead, 4096);
    }

    #[test]
    fn kernel_config_set_max_readahead_rejects_zero() {
        let mut cfg = default_kernel_config();
        assert_eq!(cfg.set_max_readahead(0), Err(1));
    }

    #[test]
    fn kernel_config_set_max_readahead_rejects_exceeds_max() {
        let mut cfg = KernelConfig {
            max_write: 128 * 1024,
            max_readahead: 128 * 1024,
            max_max_readahead: 4096,
            capabilities: 0,
            requested: 0,
            max_background: 16,
            congestion_threshold: None,
            time_gran: Duration::new(0, 1),
            max_stack_depth: 0,
            kernel_abi: Version { major: 7, minor: 40 },
        };
        assert_eq!(cfg.set_max_readahead(8192), Err(4096));
    }

    #[test]
    fn kernel_config_add_capabilities_rejects_incompatible() {
        let mut cfg = KernelConfig {
            max_write: 128 * 1024,
            max_readahead: 128 * 1024,
            max_max_readahead: 1024 * 1024,
            capabilities: 0b01,
            requested: 0,
            max_background: 16,
            congestion_threshold: None,
            time_gran: Duration::new(0, 1),
            max_stack_depth: 0,
            kernel_abi: Version { major: 7, minor: 40 },
        };
        assert_eq!(cfg.add_capabilities(0b10), Err(0b10));
    }

    #[test]
    fn kernel_config_add_capabilities_accepts_compatible() {
        let mut cfg = KernelConfig {
            max_write: 128 * 1024,
            max_readahead: 128 * 1024,
            max_max_readahead: 1024 * 1024,
            capabilities: 0b11,
            requested: 0,
            max_background: 16,
            congestion_threshold: None,
            time_gran: Duration::new(0, 1),
            max_stack_depth: 0,
            kernel_abi: Version { major: 7, minor: 40 },
        };
        assert_eq!(cfg.add_capabilities(0b01), Ok(()));
        assert_eq!(cfg.requested, 0b01);
    }

    #[test]
    fn time_or_now_variants_exist_and_match() {
        let specific = TimeOrNow::SpecificTime(UNIX_EPOCH);
        let now = TimeOrNow::Now;
        assert!(matches!(specific, TimeOrNow::SpecificTime(_)));
        assert!(matches!(now, TimeOrNow::Now));
    }

    #[cfg(feature = "abi-7-13")]
    #[test]
    fn kernel_config_set_max_background_happy_path() {
        let mut cfg = default_kernel_config();
        assert_eq!(cfg.set_max_background(32), Ok(16));
        assert_eq!(cfg.max_background, 32);
    }

    #[cfg(feature = "abi-7-13")]
    #[test]
    fn kernel_config_set_max_background_rejects_zero() {
        let mut cfg = default_kernel_config();
        assert_eq!(cfg.set_max_background(0), Err(1));
    }

    #[cfg(feature = "abi-7-13")]
    #[test]
    fn kernel_config_set_congestion_threshold_happy_path() {
        let mut cfg = default_kernel_config();
        assert_eq!(cfg.set_congestion_threshold(8), Ok(16));
        assert_eq!(cfg.congestion_threshold, Some(8));
    }

    #[cfg(feature = "abi-7-13")]
    #[test]
    fn kernel_config_set_congestion_threshold_rejects_zero() {
        let mut cfg = default_kernel_config();
        assert_eq!(cfg.set_congestion_threshold(0), Err(1));
    }

    #[cfg(feature = "abi-7-23")]
    #[test]
    fn kernel_config_set_time_granularity_valid_1ns() {
        let mut cfg = default_kernel_config();
        assert_eq!(
            cfg.set_time_granularity(Duration::new(0, 1)),
            Ok(Duration::new(0, 1))
        );
    }

    #[cfg(feature = "abi-7-23")]
    #[test]
    fn kernel_config_set_time_granularity_rejects_zero() {
        let mut cfg = default_kernel_config();
        assert_eq!(
            cfg.set_time_granularity(Duration::ZERO),
            Err(Duration::new(0, 1))
        );
    }

    #[cfg(feature = "abi-7-23")]
    #[test]
    fn kernel_config_set_time_granularity_rejects_gt_one_sec() {
        let mut cfg = default_kernel_config();
        assert_eq!(
            cfg.set_time_granularity(Duration::new(2, 0)),
            Err(Duration::new(1, 0))
        );
    }

    #[cfg(feature = "abi-7-23")]
    #[test]
    fn kernel_config_set_time_granularity_rejects_non_power_of_10() {
        let mut cfg = default_kernel_config();
        assert!(cfg.set_time_granularity(Duration::new(0, 3)).is_err());
    }

    #[cfg(feature = "abi-7-23")]
    #[test]
    fn kernel_config_set_time_granularity_accepts_1sec_exact() {
        let mut cfg = default_kernel_config();
        let previous = cfg
            .set_time_granularity(Duration::new(1, 0))
            .expect("should succeed");
        assert_eq!(previous, Duration::new(0, 1));
        assert_eq!(cfg.time_gran, Duration::new(1, 0));
    }
}
