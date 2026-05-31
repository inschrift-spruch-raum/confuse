//! API surface tests — verify that confuse re-exports (non-windows) and
//! implements (windows) all expected public symbols with correct signatures.
//! Feature-gated items are exercised only under their corresponding feature.

// ── non-windows: fuser re-exports ──────────────────────────────────────────

#[cfg(not(windows))]
#[test]
fn exposes_fuser_core_types() {
    fn assert_filesystem_trait<T: confuse::Filesystem>() {}
    struct Dummy;
    impl confuse::Filesystem for Dummy {}
    assert_filesystem_trait::<Dummy>();

    let _opts: Vec<confuse::MountOption> = vec![confuse::MountOption::RO];
    let _ft: confuse::FileType = confuse::FileType::Directory;
}

// ── windows: full public surface ───────────────────────────────────────────

#[cfg(windows)]
mod fuser_surface {
    use std::ffi::OsStr;
    use std::io;
    use std::path::Path;

    struct Dummy;
    impl confuse::Filesystem for Dummy {}

    // ── Constants ───────────────────────────────────────────────────────

    #[test]
    fn constants() {
        let _: u64 = confuse::FUSE_ROOT_ID;
        let _: u64 = confuse::consts::FUSE_ROOT_ID;
    }

    // ── Mount functions ─────────────────────────────────────────────────

    #[test]
    fn mount_function_signatures() {
        let _m2: fn(Dummy, &Path, &confuse::Config) -> io::Result<()> =
            |fs, p, opts| confuse::mount2(fs, p, opts);
        let _sm2: fn(
            Dummy,
            &Path,
            &confuse::Config,
        ) -> io::Result<confuse::BackgroundSession> =
            |fs, p, opts| confuse::spawn_mount2(fs, p, opts);
        #[allow(deprecated)]
        let _m1: fn(Dummy, &Path, &[&OsStr]) -> io::Result<()> =
            |fs, p, opts| confuse::mount(fs, p, opts);
        #[allow(deprecated)]
        let _sm1: fn(Dummy, &Path, &[&OsStr]) -> io::Result<confuse::BackgroundSession> =
            |fs, p, opts| confuse::spawn_mount(fs, p, opts);
    }

    // ── Reply types ─────────────────────────────────────────────────────

    #[test]
    fn reply_types_exist_and_impl_default() {
        fn _assert_reply_trait<T: confuse::Reply>() {}
        _assert_reply_trait::<confuse::ReplyEmpty>();

        let _ = confuse::ReplyEmpty::default();
        let _ = confuse::ReplyData::default();
        let _ = confuse::ReplyEntry::default();
        let _ = confuse::ReplyAttr::default();
        let _ = confuse::ReplyOpen::default();
        let _ = confuse::ReplyWrite::default();
        let _ = confuse::ReplyStatfs::default();
        let _ = confuse::ReplyCreate::default();
        let _ = confuse::ReplyLock::default();
        let _ = confuse::ReplyBmap::default();
        let _ = confuse::ReplyIoctl::default();
        let _ = confuse::ReplyLseek::default();
        let _ = confuse::ReplyXattr::default();
        let _ = confuse::ReplyDirectory::default();
        let _ = confuse::ReplyDirectoryPlus::default();
    }

    #[test]
    fn reply_method_signatures() {
        confuse::ReplyEmpty::default().ok();
        confuse::ReplyLock::default().locked(0, 1, 1, 42);
        confuse::ReplyBmap::default().bmap(7);
        confuse::ReplyIoctl::default().ioctl(0, &[]);
    }

    // ── Feature-gated: abi-7-11 ────────────────────────────────────────

    #[cfg(feature = "abi-7-11")]
    #[test]
    fn reply_poll_is_exported() {
        let _ = confuse::ReplyPoll::default();
        confuse::ReplyPoll::default().poll(0);
    }

    // ── Request ─────────────────────────────────────────────────────────

    #[test]
    fn request_accessors() {
        let _slot: Option<confuse::Request> = None;
        let _unique: fn(&confuse::Request) -> confuse::RequestId = |r| r.unique();
        let _uid: fn(&confuse::Request) -> u32 = |r| r.uid();
        let _gid: fn(&confuse::Request) -> u32 = |r| r.gid();
        let _pid: fn(&confuse::Request) -> u32 = |r| r.pid();
    }

    // ── KernelConfig ────────────────────────────────────────────────────

    #[test]
    fn kernel_config_type_exists() {
        fn _takes_config(_cfg: &mut confuse::KernelConfig) {}
    }

    #[cfg(feature = "abi-7-13")]
    #[test]
    fn kernel_config_abi_7_13_methods() {
        fn _set_max_background(c: &mut confuse::KernelConfig, v: u16) -> Result<u16, u16> {
            c.set_max_background(v)
        }
        fn _set_congestion_threshold(c: &mut confuse::KernelConfig, v: u16) -> Result<u16, u16> {
            c.set_congestion_threshold(v)
        }
    }

    #[cfg(feature = "abi-7-23")]
    #[test]
    fn kernel_config_abi_7_23_methods() {
        fn _set_time_granularity(
            c: &mut confuse::KernelConfig, v: std::time::Duration,
        ) -> Result<std::time::Duration, std::time::Duration> {
            c.set_time_granularity(v)
        }
    }

    // ── Session lifecycle ───────────────────────────────────────────────

    #[test]
    fn session_lifecycle() {
        let mut session = confuse::Session::new(Dummy, Path::new("."), &confuse::Config::default()).expect("session new");
        let mut unmounter = session.unmount_callable();
        let _unmount: fn(&mut confuse::SessionUnmounter) -> io::Result<()> = |u| u.unmount();
        let _run: fn(&mut confuse::Session<Dummy>) -> io::Result<()> = |s| s.run();
        let _unmount_session: fn(&mut confuse::Session<Dummy>) = |s| s.unmount();
        let _join: fn(confuse::BackgroundSession) = |s| s.join();
        let _ = &mut unmounter;
    }

    // ── Feature-gated: abi-7-11 notifier ────────────────────────────────

    #[cfg(feature = "abi-7-11")]
    #[test]
    fn notifier_is_exported_and_callable() {
        let session = confuse::Session::new(Dummy, Path::new("."), &confuse::Config::default()).expect("session new");
        let notifier = session.notifier();
        let _ = notifier.poll(1);

        #[cfg(feature = "abi-7-12")]
        {
            let _ = notifier.inval_inode(1, 0, 0);
            let _ = notifier.inval_entry(1, OsStr::new("x"));
        }
        #[cfg(feature = "abi-7-15")]
        {
            let _ = notifier.store(1, 0, &[]);
        }
        #[cfg(feature = "abi-7-18")]
        {
            let _ = notifier.delete(1, 2, OsStr::new("x"));
        }
    }

    // ── Feature-gated: abi-7-16 ────────────────────────────────────────

    #[cfg(feature = "abi-7-16")]
    #[test]
    fn fuse_forget_one_is_exported() {
        let _ = confuse::fuse_forget_one {
            nodeid: 1,
            nlookup: 1,
        };
    }
}
