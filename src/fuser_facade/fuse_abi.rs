pub mod consts {
    /// FUSE file locking flags (subset of `flock(2)`).
    pub const FUSE_LK_FLOCK: u32 = 1 << 0;

    /// Maximum number of iovecs in a single ioctl request.
    pub const FUSE_IOCTL_MAX_IOV: u32 = 256;

    /// Minimum read buffer size (in bytes) that the kernel must guarantee.
    pub const FUSE_MIN_READ_BUFFER: usize = 8192;
}
