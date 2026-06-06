use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::time::Duration;

struct Dummy;
impl confuse::Filesystem for Dummy {}

fn sample_owned_fd() -> rustix::fd::OwnedFd {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("tcp listener")
        .into()
}

fn sample_attr() -> confuse::FileAttr {
    confuse::FileAttr {
        ino: confuse::INodeNo(1),
        size: 0,
        blocks: 0,
        atime: std::time::UNIX_EPOCH,
        mtime: std::time::UNIX_EPOCH,
        ctime: std::time::UNIX_EPOCH,
        crtime: std::time::UNIX_EPOCH,
        kind: confuse::FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 512,
        flags: 0,
    }
}

// ── Constants and core exports ──────────────────────────────────────

#[test]
fn constants_and_core_types() {
    let _: confuse::INodeNo = confuse::INodeNo::ROOT;
    let _: confuse::Errno = confuse::Errno::ENOSYS;
    let _: confuse::Errno = confuse::Errno::EFTYPE;
    #[cfg(feature = "macos-api")]
    {
        let _: confuse::Errno = confuse::Errno::ENOATTR;
        assert_eq!(
            confuse::Errno::NO_XATTR.raw_os_error(),
            confuse::Errno::ENOATTR.raw_os_error()
        );
    }
    #[cfg(not(feature = "macos-api"))]
    assert_eq!(
        confuse::Errno::NO_XATTR.raw_os_error(),
        confuse::Errno::ENODATA.raw_os_error()
    );
    let _: confuse::INodeNo = confuse::INodeNo(1);
    let _: confuse::FileHandle = confuse::FileHandle(2);
    let _: confuse::LockOwner = confuse::LockOwner(3);
    let _: confuse::Generation = confuse::Generation(4);
    let _: confuse::RequestId = confuse::RequestId(5);
    let _: confuse::Version = confuse::Version(7, 40);
    let _: confuse::TimeOrNow = confuse::TimeOrNow::Now;
    let _: confuse::FileType = confuse::FileType::Directory;
    let _from_std: fn(std::fs::FileType) -> Option<confuse::FileType> = confuse::FileType::from_std;
    let _: confuse::FileAttr = sample_attr();
    let _: u32 = confuse::consts::FUSE_LK_FLOCK;
    let _: u32 = confuse::consts::FUSE_IOCTL_MAX_IOV;
    let _: usize = confuse::consts::FUSE_MIN_READ_BUFFER;
    assert_eq!(confuse::consts::FUSE_LK_FLOCK, 1);
    assert_eq!(confuse::consts::FUSE_IOCTL_MAX_IOV, 256);
    assert_eq!(confuse::consts::FUSE_MIN_READ_BUFFER, 8192);
}

#[test]
fn flag_and_newtype_surfaces_match_task2_shape() {
    let _: confuse::InitFlags = confuse::InitFlags::empty();
    let _: confuse::OpenFlags = confuse::OpenFlags(0);
    let _: confuse::FopenFlags = confuse::FopenFlags::empty();
    let _: confuse::WriteFlags = confuse::WriteFlags::empty();
    let _: confuse::RenameFlags = confuse::RenameFlags::empty();
    let _: confuse::AccessFlags = confuse::AccessFlags::empty();
    let _: confuse::IoctlFlags = confuse::IoctlFlags::empty();
    let _: confuse::PollFlags = confuse::PollFlags::empty();
    let _: confuse::PollEvents = confuse::PollEvents::empty();
    let _: confuse::CopyFileRangeFlags = confuse::CopyFileRangeFlags::empty();
    let _: confuse::BsdFileFlags = confuse::BsdFileFlags::empty();
    let _: u64 = confuse::InitFlags::FUSE_REQUEST_TIMEOUT.bits();
    let _ = confuse::InitFlags::FUSE_ASYNC_READ
        .union(confuse::InitFlags::FUSE_BIG_WRITES)
        .difference(confuse::InitFlags::FUSE_ASYNC_READ);
    #[cfg(feature = "macos-api")]
    assert_eq!(confuse::FopenFlags::all().bits(), 0xc000_00ff);
    #[cfg(not(feature = "macos-api"))]
    assert_eq!(confuse::FopenFlags::all().bits(), 0xff);
    assert!(confuse::FopenFlags::from_bits(0x100).is_none());
    assert_eq!(confuse::FopenFlags::from_bits_truncate(0x101).bits(), 1);
    assert_eq!(confuse::CopyFileRangeFlags::all().bits(), 0);
    let mut flags = confuse::FopenFlags::FOPEN_DIRECT_IO;
    flags.insert(confuse::FopenFlags::FOPEN_KEEP_CACHE);
    flags.remove(confuse::FopenFlags::FOPEN_DIRECT_IO);
    assert!(flags.contains(confuse::FopenFlags::FOPEN_KEEP_CACHE));
    assert_eq!(confuse::WriteFlags::FUSE_WRITE_CACHE.bits(), 1);
    assert_eq!(confuse::RenameFlags::all().bits(), 0);
    assert_eq!(confuse::AccessFlags::R_OK.bits(), 4);
    assert_eq!(confuse::IoctlFlags::FUSE_IOCTL_DIR.bits(), 1 << 4);
    assert_eq!(confuse::PollFlags::FUSE_POLL_SCHEDULE_NOTIFY.bits(), 1);
    assert!(confuse::PollEvents::POLLIN.intersects(confuse::PollEvents::POLLIN));
    let _: u64 = confuse::CopyFileRangeFlags::empty().bits();
    let _forget_one_nodeid: fn(&confuse::ForgetOne) -> confuse::INodeNo =
        confuse::ForgetOne::nodeid;
    let _forget_one_nlookup: fn(&confuse::ForgetOne) -> u64 = confuse::ForgetOne::nlookup;
    assert_eq!(
        confuse::OpenFlags(libc::O_RDWR).acc_mode(),
        confuse::OpenAccMode::O_RDWR
    );

    let types_source = include_str!("../../src/fuser_facade/types.rs");
    assert!(types_source.contains("u64_newtype!(LockOwner, no_hash);"));
    assert!(!types_source.contains("impl From<u64> for $name"));
    assert!(!types_source.contains("impl From<i32> for OpenFlags"));
    assert!(!types_source.contains("impl From<OpenFlags> for i32"));
    assert!(!types_source.contains("impl From<std::fs::FileType> for FileType"));
    assert!(types_source.contains(
        "#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]\npub struct Generation"
    ));
    assert!(types_source.contains("#[derive(Clone, Copy, Debug)]\npub struct Errno"));
    assert!(
        types_source
            .contains("#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]\npub enum TimeOrNow")
    );
    assert!(!types_source.contains("#[cfg_attr(feature = \"serializable\", derive(Serialize, Deserialize))]\n#[repr(i32)]\npub enum OpenAccMode"));
    let notifier_source = include_str!("../../src/fuser_facade/notifier.rs");
    assert!(notifier_source.contains("#[derive(Copy, Clone, Debug)]\npub struct PollHandle"));
}

#[test]
fn cargo_feature_surface_matches_fuser_017() {
    let manifest = include_str!("../../Cargo.toml");
    for feature in [
        "abi-7-20",
        "abi-7-21",
        "abi-7-22",
        "abi-7-23",
        "abi-7-24",
        "abi-7-25",
        "abi-7-26",
        "abi-7-27",
        "abi-7-28",
        "abi-7-29",
        "abi-7-30",
        "abi-7-31",
        "abi-7-36",
        "abi-7-40",
        "experimental",
        "libfuse",
        "libfuse2",
        "libfuse3",
        "macfuse-4-compat",
        "macos-no-mount",
        "serializable",
    ] {
        assert!(
            manifest.contains(&format!("{feature} =")),
            "missing feature {feature}"
        );
    }
    assert!(manifest.contains("serializable = [\"serde\", \"fuser/serializable\"]"));
    assert!(
        manifest.contains("experimental = [\"async-trait\", \"tokio\", \"fuser/experimental\"]")
    );
}

// ── Filesystem trait shape ──────────────────────────────────────────

#[path = "windows/filesystem.rs"]
mod filesystem;
#[test]
fn config_mount_option_and_session_acl_surface() {
    let mut config = confuse::Config::default();
    config.mount_options = vec![
        confuse::MountOption::FSName("confuse".to_string()),
        confuse::MountOption::Subtype("dokan".to_string()),
        confuse::MountOption::CUSTOM("debug".to_string()),
        confuse::MountOption::AutoUnmount,
        confuse::MountOption::DefaultPermissions,
        confuse::MountOption::Dev,
        confuse::MountOption::NoDev,
        confuse::MountOption::Suid,
        confuse::MountOption::NoSuid,
        confuse::MountOption::RO,
        confuse::MountOption::RW,
        confuse::MountOption::Exec,
        confuse::MountOption::NoExec,
        confuse::MountOption::Atime,
        confuse::MountOption::NoAtime,
        confuse::MountOption::DirSync,
        confuse::MountOption::Sync,
        confuse::MountOption::Async,
    ];
    config.acl = confuse::SessionACL::Owner;
    config.n_threads = Some(1);
    config.clone_fd = false;
    let _all = confuse::SessionACL::All;
    let _root_and_owner = confuse::SessionACL::RootAndOwner;
    assert_eq!(config.acl, confuse::SessionACL::Owner);
    assert_eq!(confuse::SessionACL::All, confuse::SessionACL::All);
    assert_eq!(
        confuse::SessionACL::RootAndOwner,
        confuse::SessionACL::RootAndOwner
    );
}

#[test]
fn mount_function_signatures() {
    let _mount2: fn(Dummy, &Path, &confuse::Config) -> io::Result<()> =
        |filesystem, mountpoint, config| confuse::mount2(filesystem, mountpoint, config);
    let _spawn_mount2: fn(
        Dummy,
        &Path,
        &confuse::Config,
    ) -> io::Result<confuse::BackgroundSession> =
        |filesystem, mountpoint, config| confuse::spawn_mount2(filesystem, mountpoint, config);
    // mount() / spawn_mount() deprecated wrappers removed in fuser 0.17.0;
    // mount2 / spawn_mount2 are the only mount entry points (see Task #2 of
    // .omo/plans/fuser-facade-alignment.md).
    let source = include_str!("../../src/fuser_facade/session.rs");
    assert!(source.contains("fn run(&mut self)"));
    assert!(!source.contains("pub(crate) fn run(&mut self)"));
    assert!(!source.contains("pub fn run(&mut self)"));
    assert!(source.contains("fn mountpoint(&self)"));
    assert!(!source.contains("pub(crate) fn mountpoint(&self)"));
    assert!(!source.contains("pub fn mountpoint(&self)"));
    assert!(source.contains("fn new<FS: Filesystem + Send + 'static>"));
    assert!(!source.contains("pub(crate) fn new<FS: Filesystem + Send + 'static>"));
    assert!(!source.contains("pub fn new<FS: Filesystem + Send + 'static>"));
    assert!(source.contains("from_fd"));
}

// ── Reply types and methods ─────────────────────────────────────────

#[test]
fn reply_types_exist_with_strict_public_derives() {
    let source = include_str!("../../src/fuser_facade/reply/api.rs");
    for ty in [
        "ReplyEmpty",
        "ReplyData",
        "ReplyEntry",
        "ReplyAttr",
        "ReplyOpen",
        "ReplyWrite",
        "ReplyStatfs",
        "ReplyCreate",
        "ReplyLock",
        "ReplyBmap",
        "ReplyIoctl",
        "ReplyLseek",
        "ReplyXattr",
        "ReplyDirectory",
        "ReplyDirectoryPlus",
        "ReplyPoll",
    ] {
        assert!(source.contains(&format!("pub struct {ty}")), "missing {ty}");
    }
    let support_source = include_str!("../../src/fuser_facade/reply/support.rs");
    assert!(support_source.contains("#[derive(Debug)]\npub struct BackingId"));
    assert!(
        support_source.contains("#[derive(Debug)]\n#[repr(transparent)]\npub struct ForgetOne")
    );
    assert!(support_source.contains("struct fuse_forget_one"));
    assert!(source.contains("#[derive(Debug)]\npub struct ReplyEmpty"));
    assert!(source.contains("#[derive(Debug)]\npub struct ReplyPoll"));
    assert!(
        !source.contains("pub struct ReplyEmpty")
            || !source.contains("impl Default for ReplyEmpty")
    );
    assert!(!source.contains("#[derive(Clone, Debug, Default)]\npub struct Reply"));
}

#[test]
fn reply_method_signatures() {
    let _empty_ok: fn(confuse::ReplyEmpty) = confuse::ReplyEmpty::ok;
    let _empty_error: fn(confuse::ReplyEmpty, confuse::Errno) = confuse::ReplyEmpty::error;
    let _data: fn(confuse::ReplyData, &[u8]) = confuse::ReplyData::data;
    let _data_error: fn(confuse::ReplyData, confuse::Errno) = confuse::ReplyData::error;
    let _entry: fn(confuse::ReplyEntry, &Duration, &confuse::FileAttr, confuse::Generation) =
        confuse::ReplyEntry::entry;
    let _attr: fn(confuse::ReplyAttr, &Duration, &confuse::FileAttr) = confuse::ReplyAttr::attr;
    let _open: fn(confuse::ReplyOpen, confuse::FileHandle, confuse::FopenFlags) =
        confuse::ReplyOpen::opened;
    let _write: fn(confuse::ReplyWrite, u32) = confuse::ReplyWrite::written;
    let _statfs: fn(confuse::ReplyStatfs, u64, u64, u64, u64, u64, u32, u32, u32) =
        confuse::ReplyStatfs::statfs;
    let _create: fn(
        confuse::ReplyCreate,
        &Duration,
        &confuse::FileAttr,
        confuse::Generation,
        confuse::FileHandle,
        confuse::FopenFlags,
    ) = confuse::ReplyCreate::created;
    let _lock: fn(confuse::ReplyLock, u64, u64, i32, u32) = confuse::ReplyLock::locked;
    let _bmap: fn(confuse::ReplyBmap, u64) = confuse::ReplyBmap::bmap;
    let _ioctl: fn(confuse::ReplyIoctl, i32, &[u8]) = confuse::ReplyIoctl::ioctl;
    let _lseek: fn(confuse::ReplyLseek, i64) = confuse::ReplyLseek::offset;
    let _xattr_size: fn(confuse::ReplyXattr, u32) = confuse::ReplyXattr::size;
    let _xattr_data: fn(confuse::ReplyXattr, &[u8]) = confuse::ReplyXattr::data;
    let _directory_ok: fn(confuse::ReplyDirectory) = confuse::ReplyDirectory::ok;
    let _directory_plus_ok: fn(confuse::ReplyDirectoryPlus) = confuse::ReplyDirectoryPlus::ok;
    let _open_backing: fn(
        &confuse::ReplyOpen,
        rustix::fd::OwnedFd,
    ) -> io::Result<confuse::BackingId> = confuse::ReplyOpen::open_backing;
    let _opened_passthrough: fn(
        confuse::ReplyOpen,
        confuse::FileHandle,
        confuse::FopenFlags,
        &confuse::BackingId,
    ) = confuse::ReplyOpen::opened_passthrough;
    let _create_open_backing: fn(
        &confuse::ReplyCreate,
        rustix::fd::OwnedFd,
    ) -> io::Result<confuse::BackingId> = confuse::ReplyCreate::open_backing;
    let _created_passthrough: fn(
        confuse::ReplyCreate,
        &Duration,
        &confuse::FileAttr,
        confuse::Generation,
        confuse::FileHandle,
        confuse::FopenFlags,
        &confuse::BackingId,
    ) = confuse::ReplyCreate::created_passthrough;
    fn directory_add(
        reply: &mut confuse::ReplyDirectory, ino: confuse::INodeNo, offset: u64,
        kind: confuse::FileType, name: &OsStr,
    ) -> bool {
        reply.add(ino, offset, kind, name)
    }
    fn directory_plus_add(
        reply: &mut confuse::ReplyDirectoryPlus, ino: confuse::INodeNo, offset: u64, name: &OsStr,
        ttl: &Duration, attr: &confuse::FileAttr, generation: confuse::Generation,
    ) -> bool {
        reply.add(ino, offset, name, ttl, attr, generation)
    }
    let _ = (directory_add, directory_plus_add);
}

// ── Reply poll ───────────────────────────────────────────────────────

#[test]
fn reply_poll_is_exported_unconditionally() {
    let _poll: fn(confuse::ReplyPoll, confuse::PollEvents) = confuse::ReplyPoll::poll;
    let _error: fn(confuse::ReplyPoll, confuse::Errno) = confuse::ReplyPoll::error;
}

// ── Request ─────────────────────────────────────────────────────────

#[test]
fn request_accessors() {
    let _slot: Option<confuse::Request> = None;
    let _unique: fn(&confuse::Request) -> confuse::RequestId = |request| request.unique();
    let _uid: fn(&confuse::Request) -> u32 = |request| request.uid();
    let _gid: fn(&confuse::Request) -> u32 = |request| request.gid();
    let _pid: fn(&confuse::Request) -> u32 = |request| request.pid();
}

// ── KernelConfig ────────────────────────────────────────────────────

#[test]
fn kernel_config_type_exists() {
    fn takes_config(_config: &mut confuse::KernelConfig) {}
    let _ = takes_config;
}

#[test]
fn kernel_config_methods_are_unconditional() {
    fn set_max_background(config: &mut confuse::KernelConfig, value: u16) -> Result<u16, u16> {
        config.set_max_background(value)
    }
    fn set_congestion_threshold(
        config: &mut confuse::KernelConfig, value: u16,
    ) -> Result<u16, u16> {
        config.set_congestion_threshold(value)
    }
    fn set_time_granularity(
        config: &mut confuse::KernelConfig, value: std::time::Duration,
    ) -> Result<std::time::Duration, std::time::Duration> {
        config.set_time_granularity(value)
    }
    fn set_max_stack_depth(config: &mut confuse::KernelConfig, value: u32) -> Result<u32, u32> {
        config.set_max_stack_depth(value)
    }
    fn set_max_write(config: &mut confuse::KernelConfig, value: u32) -> Result<u32, u32> {
        config.set_max_write(value)
    }
    fn set_max_readahead(config: &mut confuse::KernelConfig, value: u32) -> Result<u32, u32> {
        config.set_max_readahead(value)
    }
    fn capabilities(config: &confuse::KernelConfig) -> confuse::InitFlags {
        config.capabilities()
    }
    fn kernel_abi(config: &confuse::KernelConfig) -> confuse::Version {
        config.kernel_abi()
    }
    fn add_capabilities(
        config: &mut confuse::KernelConfig, value: confuse::InitFlags,
    ) -> Result<(), confuse::InitFlags> {
        config.add_capabilities(value)
    }
    let _ = (
        set_max_background,
        set_congestion_threshold,
        set_time_granularity,
        set_max_stack_depth,
        set_max_write,
        set_max_readahead,
        capabilities,
        kernel_abi,
        add_capabilities,
    );
}

// ── Session lifecycle ───────────────────────────────────────────────

#[test]
fn session_lifecycle() {
    let mut session = confuse::Session::new(Dummy, Path::new("."), &confuse::Config::default())
        .expect("session new");
    let _session_fd = rustix::fd::AsFd::as_fd(&session);
    let from_fd = confuse::Session::from_fd(
        Dummy,
        sample_owned_fd(),
        confuse::SessionACL::Owner,
        confuse::Config::default(),
    );
    assert!(matches!(from_fd, Err(err) if err.kind() == io::ErrorKind::Unsupported));
    let _session_from_owned_path = confuse::Session::new(
        Dummy,
        std::path::PathBuf::from("."),
        &confuse::Config::default(),
    )
    .expect("session new from owned path");
    let mut unmounter = session.unmount_callable();
    let _unmount: fn(&mut confuse::SessionUnmounter) -> io::Result<()> =
        |unmounter| unmounter.unmount();
    let _unmount_session: fn(&mut confuse::Session<Dummy>) -> io::Result<()> =
        |session| session.unmount();
    let _spawn: fn(confuse::Session<Dummy>) -> io::Result<confuse::BackgroundSession> =
        |session| session.spawn();
    let _join: fn(confuse::BackgroundSession) -> io::Result<()> = |session| session.join();
    let _umount_and_join: fn(confuse::BackgroundSession) -> io::Result<()> =
        |session| session.umount_and_join();
    let _background_notifier: fn(&confuse::BackgroundSession) -> confuse::Notifier =
        |session| session.notifier();
    let _ = &mut unmounter;
}

// ── Notifier and poll ───────────────────────────────────────────────

#[test]
fn notifier_is_exported_and_callable() {
    let session = confuse::Session::new(Dummy, Path::new("."), &confuse::Config::default())
        .expect("session new");
    let notifier = session.notifier();
    let _poll_handle = confuse::PollHandle(1);
    let _poll: fn(&confuse::Notifier, confuse::PollHandle) -> io::Result<()> =
        confuse::Notifier::poll;
    let _ = notifier.poll(_poll_handle);

    let _handle: fn(&confuse::PollNotifier) -> confuse::PollHandle = confuse::PollNotifier::handle;
    let _notify: fn(confuse::PollNotifier) -> io::Result<()> = confuse::PollNotifier::notify;

    let _ = notifier.inval_inode(confuse::INodeNo(1), 0, 0);
    let _ = notifier.inval_entry(confuse::INodeNo(1), OsStr::new("x"));
    let _ = notifier.store(confuse::INodeNo(1), 0, &[]);
    let _ = notifier.delete(confuse::INodeNo(1), confuse::INodeNo(2), OsStr::new("x"));

    let source = include_str!("../../src/fuser_facade/notifier.rs");
    assert!(!source.contains("pub(crate) fn send_inval"));
    assert!(!source.contains("pub(crate) fn send(&self"));
    assert!(!source.contains("pub(crate) fn too_big_err"));
    assert!(!source.contains("pub(crate) enum NotifyCode"));
    assert!(!source.contains("pub(crate) mod fuse_notify_code"));
    assert!(!source.contains("pub(crate) struct Notification"));
    assert!(source.contains("#[cfg(test)]\n    pub(crate) fn new(cs: ChannelSender"));
}

// ── Batch forget ────────────────────────────────────────────────────

#[test]
fn forget_one_accessors_are_exported() {
    let support_source = include_str!("../../src/fuser_facade/reply/support.rs");
    assert!(support_source.contains("pub struct ForgetOne"));
    assert!(support_source.contains("#[repr(transparent)]\npub struct ForgetOne"));
    assert!(support_source.contains("struct fuse_forget_one"));
    let _nodeid: fn(&confuse::ForgetOne) -> confuse::INodeNo = confuse::ForgetOne::nodeid;
    let _nlookup: fn(&confuse::ForgetOne) -> u64 = confuse::ForgetOne::nlookup;
}
