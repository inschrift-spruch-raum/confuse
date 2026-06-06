// ── non-windows: fuser re-exports ──────────────────────────────────────────

#[cfg(not(windows))]
#[test]
fn exposes_fuser_core_types() {
    fn assert_filesystem_trait<T: confuse::Filesystem>() {}
    struct Dummy;
    impl confuse::Filesystem for Dummy {}
    assert_filesystem_trait::<Dummy>();

    let _options: Vec<confuse::MountOption> = vec![confuse::MountOption::RO];
    let _file_type: confuse::FileType = confuse::FileType::Directory;
    let _errno: confuse::Errno = confuse::Errno::ENOSYS;
    let _request_id: confuse::RequestId = confuse::RequestId(1);
    let _inode: confuse::INodeNo = confuse::INodeNo(1);
    let _file_handle: confuse::FileHandle = confuse::FileHandle(2);
    let _lock_owner: confuse::LockOwner = confuse::LockOwner(3);
    let _generation: confuse::Generation = confuse::Generation(4);
    let _version = confuse::Version(7, 40);
    let _open_flags = confuse::OpenFlags(0);
    let _fopen_flags = confuse::FopenFlags::empty();
    let _poll_events = confuse::PollEvents::empty();
    let _poll_handle = confuse::PollHandle(0);
    let _attr = confuse::FileAttr {
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
    };
}

#[cfg(not(windows))]
#[test]
fn non_windows_is_direct_fuser_017_reexport_for_mount_and_session() {
    use std::io;
    use std::path::Path;

    struct Dummy;
    impl confuse::Filesystem for Dummy {}

    let _config = confuse::Config::default();
    let _acl = confuse::SessionACL::Owner;
    let _mount_option = confuse::MountOption::AutoUnmount;
    let _new_session: fn(Dummy, &Path, &confuse::Config) -> io::Result<confuse::Session<Dummy>> =
        |filesystem, mountpoint, config| confuse::Session::new(filesystem, mountpoint, config);
    let _mount2: fn(Dummy, &Path, &confuse::Config) -> io::Result<()> =
        |filesystem, mountpoint, config| confuse::mount2(filesystem, mountpoint, config);
    let _spawn_mount2: fn(
        Dummy,
        &Path,
        &confuse::Config,
    ) -> io::Result<confuse::BackgroundSession> =
        |filesystem, mountpoint, config| confuse::spawn_mount2(filesystem, mountpoint, config);
}
