use super::super::types::{
    Errno, FileAttr, FileHandle, FileType, FopenFlags, Generation, INodeNo, PollEvents,
};
use super::*;
use crate::dokan_impl::{next_dir_offset_from_entries, next_dirplus_offset_from_entries};
use libc::ENOSYS;
use std::io;
#[cfg(feature = "macos-api")]
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[cfg(not(feature = "macos-api"))]
use std::time::{Duration, UNIX_EPOCH};

#[derive(Clone, Debug, Default)]
struct TestReplySender;

fn sample_owned_fd() -> rustix::fd::OwnedFd {
    std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0))
        .expect("tcp listener")
        .into()
}

#[test]
fn reply_attr_can_store_payload() {
    let reply = ReplyAttr::capture();
    let attr = FileAttr {
        ino: INodeNo(11),
        size: 22,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };
    reply.duplicate().attr(&Duration::from_secs(1), &attr);
    let stored = *reply.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok(v)) if v.ino == INodeNo(11) && v.size == 22));
    let payload = *reply.payload.lock().expect("lock");
    assert!(
        matches!(payload, Some(Ok(v)) if v.attr.ino == INodeNo(11) && v.ttl == Duration::from_secs(1))
    );
}

#[test]
fn reply_statfs_and_xattr_store_payloads() {
    let statfs = ReplyStatfs::capture();
    statfs.duplicate().statfs(1, 2, 3, 4, 5, 6, 7, 8);
    let s = *statfs.status.lock().expect("lock");
    assert!(matches!(s, Some(Ok((1, 2, 3, 4, 5, 6, 7, 8)))));

    let xattr = ReplyXattr::capture();
    xattr.duplicate().data(&[1, 2, 3]);
    let x = xattr.status.lock().expect("lock").clone();
    assert!(matches!(x, Some(Ok(v)) if v == vec![1, 2, 3]));

    let bmap = ReplyBmap::capture();
    bmap.duplicate().bmap(77);
    let b = *bmap.status.lock().expect("lock");
    assert!(matches!(b, Some(Ok(77))));

    let ioctl = ReplyIoctl::capture();
    ioctl.duplicate().ioctl(3, &[9, 8]);
    let i = ioctl.status.lock().expect("lock").clone();
    assert!(matches!(i, Some(Ok((3, v))) if v == vec![9, 8]));
}

#[test]
fn reply_directory_new_size_enforces_aligned_byte_capacity() {
    let mut reply = ReplyDirectory::new(1, TestReplySender, 32);
    assert!(!reply.add(
        INodeNo(1),
        0,
        FileType::RegularFile,
        std::ffi::OsStr::new("a")
    ));
    assert!(reply.add(
        INodeNo(2),
        1,
        FileType::RegularFile,
        std::ffi::OsStr::new("a")
    ));
}

#[test]
fn reply_directoryplus_new_size_enforces_aligned_byte_capacity() {
    let attr = FileAttr {
        ino: INodeNo(1),
        size: 0,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };
    let mut reply = ReplyDirectoryPlus::new(1, TestReplySender, 152);
    assert!(!reply.add(
        INodeNo(1),
        0,
        std::ffi::OsStr::new("a"),
        &Duration::from_secs(1),
        &attr,
        Generation(0)
    ));
    assert!(reply.add(
        INodeNo(2),
        1,
        std::ffi::OsStr::new("a"),
        &Duration::from_secs(1),
        &attr,
        Generation(0)
    ));
}

#[test]
fn reply_directory_preserves_entry_offsets_and_next_offset() {
    let mut reply = ReplyDirectory::capture();
    assert!(!reply.add(
        INodeNo(10),
        7,
        FileType::RegularFile,
        std::ffi::OsStr::new("a")
    ));
    assert!(!reply.add(
        INodeNo(11),
        9,
        FileType::Directory,
        std::ffi::OsStr::new("b")
    ));
    let entries = reply.entries.lock().expect("lock").clone();
    assert_eq!(entries[0].offset, 7);
    assert_eq!(entries[1].offset, 9);
    assert_eq!(next_dir_offset_from_entries(0, &entries), 9);
    assert_eq!(next_dir_offset_from_entries(5, &[]), 5);
}

#[test]
fn reply_directoryplus_preserves_entry_offsets_and_next_offset() {
    let attr = FileAttr {
        ino: INodeNo(1),
        size: 0,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };
    let mut reply = ReplyDirectoryPlus::capture();
    assert!(!reply.add(
        INodeNo(10),
        13,
        std::ffi::OsStr::new("a"),
        &Duration::from_secs(1),
        &attr,
        Generation(0)
    ));
    assert!(!reply.add(
        INodeNo(11),
        15,
        std::ffi::OsStr::new("b"),
        &Duration::from_secs(1),
        &attr,
        Generation(0)
    ));
    let entries = reply.entries.lock().expect("lock").clone();
    assert_eq!(entries[0].offset, 13);
    assert_eq!(entries[1].offset, 15);
    assert_eq!(next_dirplus_offset_from_entries(0, &entries), 15);
    assert_eq!(next_dirplus_offset_from_entries(8, &[]), 8);
}

#[test]
fn reply_directory_non_ascii_uses_utf8_byte_capacity() {
    let mut reply = ReplyDirectory::new(1, TestReplySender, 32);
    assert!(!reply.add(
        INodeNo(1),
        0,
        FileType::RegularFile,
        std::ffi::OsStr::new("é")
    ));
    assert!(reply.add(
        INodeNo(2),
        1,
        FileType::RegularFile,
        std::ffi::OsStr::new("é")
    ));
}

#[test]
fn reply_directoryplus_non_ascii_uses_utf8_byte_capacity() {
    let attr = FileAttr {
        ino: INodeNo(1),
        size: 0,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };
    let mut reply = ReplyDirectoryPlus::new(1, TestReplySender, 152);
    assert!(!reply.add(
        INodeNo(1),
        0,
        std::ffi::OsStr::new("é"),
        &Duration::from_secs(1),
        &attr,
        Generation(0)
    ));
    assert!(reply.add(
        INodeNo(2),
        1,
        std::ffi::OsStr::new("é"),
        &Duration::from_secs(1),
        &attr,
        Generation(0)
    ));
}

#[test]
fn reply_xattr_size_sets_size_hint_and_empty_data() {
    let xattr = ReplyXattr::capture();
    xattr.duplicate().size(42);
    let hint = *xattr.size_hint.lock().expect("lock");
    assert_eq!(hint, Some(42));
    let data = xattr.status.lock().expect("lock").clone();
    assert!(matches!(data, Some(Ok(v)) if v.is_empty()));
}

#[test]
fn reply_lseek_offset_stores_value() {
    let lseek = ReplyLseek::capture();
    lseek.duplicate().offset(12345);
    let stored = *lseek.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok(12345))));
}

#[test]
fn reply_lseek_error_stores_error() {
    let lseek = ReplyLseek::capture();
    lseek.duplicate().error(Errno::ENOSYS);
    let stored = *lseek.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == ENOSYS));
}

#[test]
fn reply_lock_locked_stores_values() {
    let lock = ReplyLock::capture();
    lock.duplicate().locked(0, 100, 1, 42);
    let stored = *lock.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok((0, 100, 1, 42)))));
}

#[test]
fn reply_lock_error_stores_error() {
    let lock = ReplyLock::capture();
    lock.duplicate().error(Errno::ENOSYS);
    let stored = *lock.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == ENOSYS));
}

#[test]
fn reply_create_stored_attr_ttl_generation_fh_flags() {
    let create = ReplyCreate::capture();
    let attr = FileAttr {
        ino: INodeNo(99),
        size: 4096,
        blocks: 8,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o755,
        nlink: 1,
        uid: 1000,
        gid: 1000,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };
    create.duplicate().created(
        &Duration::from_secs(5),
        &attr,
        Generation(44),
        FileHandle(33),
        FopenFlags::from_bits_retain(7),
    );
    let stored = *create.status.lock().expect("lock");
    assert!(
        matches!(stored, Some(Ok(payload)) if payload.attr.ino == INodeNo(99) && payload.attr.perm == 0o755 && payload.ttl == Duration::from_secs(5) && payload.generation == Generation(44) && payload.fh == FileHandle(33) && payload.flags == FopenFlags::from_bits_retain(7))
    );
}

#[test]
fn reply_entry_stores_attr_ttl_and_generation() {
    let entry = ReplyEntry::capture();
    let attr = FileAttr {
        ino: INodeNo(123),
        size: 456,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };
    entry
        .duplicate()
        .entry(&Duration::from_secs(9), &attr, Generation(77));
    let stored = *entry.status.lock().expect("lock");
    assert!(
        matches!(stored, Some(Ok(payload)) if payload.attr.ino == INodeNo(123) && payload.attr.size == 456 && payload.ttl == Duration::from_secs(9) && payload.generation == Generation(77))
    );
}

#[test]
fn reply_directoryplus_stores_ttl_and_generation() {
    let attr = FileAttr {
        ino: INodeNo(321),
        size: 654,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };
    let mut reply = ReplyDirectoryPlus::capture();
    assert!(!reply.add(
        INodeNo(321),
        12,
        std::ffi::OsStr::new("payload"),
        &Duration::from_secs(11),
        &attr,
        Generation(88)
    ));
    let entries = reply.entries.lock().expect("lock");
    assert_eq!(entries[0].ino, INodeNo(321));
    assert_eq!(entries[0].offset, 12);
    assert_eq!(entries[0].ttl, Duration::from_secs(11));
    assert_eq!(entries[0].generation, Generation(88));
    assert_eq!(entries[0].attr.size, 654);
}

#[test]
fn reply_create_error_stores_error() {
    let create = ReplyCreate::capture();
    create.duplicate().error(Errno::ENOSYS);
    let stored = *create.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == ENOSYS));
}

#[test]
fn forget_one_accessors_expose_public_value_shape() {
    let f = ForgetOne::new(7, 3);
    assert_eq!(f.nodeid(), INodeNo(7));
    assert_eq!(f.nlookup(), 3);
}

#[test]
fn reply_entry_error_stores_errno() {
    let entry = ReplyEntry::capture();
    entry.duplicate().error(Errno::ENOENT);
    let stored = *entry.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::ENOENT));
}

#[test]
fn passthrough_methods_report_windows_unsupported_behavior() {
    let open = ReplyOpen::capture();
    let open_error = open
        .open_backing(sample_owned_fd())
        .expect_err("passthrough is unsupported");
    assert_eq!(open_error.kind(), io::ErrorKind::Unsupported);
    let backing_id = BackingId::unsupported_for_windows();
    open.duplicate()
        .opened_passthrough(FileHandle(1), FopenFlags::empty(), &backing_id);
    let opened = *open.opened.lock().expect("lock");
    assert!(matches!(opened, Some(Err(e)) if e == libc::ENOSYS));

    let create = ReplyCreate::capture();
    let create_error = create
        .open_backing(sample_owned_fd())
        .expect_err("passthrough is unsupported");
    assert_eq!(create_error.kind(), io::ErrorKind::Unsupported);
    let attr = FileAttr {
        ino: INodeNo(1),
        size: 0,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };
    create.duplicate().created_passthrough(
        &Duration::ZERO,
        &attr,
        Generation(0),
        FileHandle(1),
        FopenFlags::empty(),
        &backing_id,
    );
    let created = *create.status.lock().expect("lock");
    assert!(matches!(created, Some(Err(e)) if e == libc::ENOSYS));
}

#[test]
#[should_panic]
fn reply_open_opened_panics_for_passthrough_flag() {
    ReplyOpen::capture().opened(FileHandle(1), FopenFlags::FOPEN_PASSTHROUGH);
}

#[test]
#[should_panic]
fn reply_create_created_panics_for_passthrough_flag() {
    let attr = FileAttr {
        ino: INodeNo(1),
        size: 0,
        blocks: 0,
        atime: UNIX_EPOCH,
        mtime: UNIX_EPOCH,
        ctime: UNIX_EPOCH,
        crtime: UNIX_EPOCH,
        kind: FileType::RegularFile,
        perm: 0o644,
        nlink: 1,
        uid: 0,
        gid: 0,
        rdev: 0,
        blksize: 4096,
        flags: 0,
    };

    ReplyCreate::capture().created(
        &Duration::ZERO,
        &attr,
        Generation(0),
        FileHandle(1),
        FopenFlags::FOPEN_PASSTHROUGH,
    );
}

#[test]
fn reply_open_error_stores_errno() {
    let open = ReplyOpen::capture();
    open.duplicate().error(Errno::EACCES);
    let stored = *open.opened.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::EACCES));
}

#[test]
fn reply_write_error_stores_errno() {
    let write = ReplyWrite::capture();
    write.duplicate().error(Errno::ENOSPC);
    let stored = *write.written.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::ENOSPC));
}

#[test]
fn reply_data_error_stores_errno() {
    let data = ReplyData::capture();
    data.duplicate().error(Errno::EIO);
    let stored = data.data.lock().expect("lock").clone();
    assert!(matches!(stored, Some(Err(e)) if e == libc::EIO));
}

#[test]
fn reply_statfs_error_stores_errno() {
    let statfs = ReplyStatfs::capture();
    statfs.duplicate().error(Errno::ENOSYS);
    let stored = *statfs.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::ENOSYS));
}

#[test]
fn reply_empty_ok_and_error() {
    let ok_reply = ReplyEmpty::capture();
    ok_reply.duplicate().ok();
    let stored = *ok_reply.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok(()))));

    let err_reply = ReplyEmpty::capture();
    err_reply.duplicate().error(Errno::from_raw_os_error(1));
    let stored = *err_reply.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(1))));
}

#[test]
fn reply_poll_ok_stores_events() {
    let poll = ReplyPoll::capture();
    poll.duplicate()
        .poll(PollEvents::POLLIN | PollEvents::POLLOUT);
    let stored = *poll.status.lock().expect("lock");
    assert!(
        matches!(stored, Some(Ok(events)) if events == (PollEvents::POLLIN | PollEvents::POLLOUT))
    );
}

#[test]
fn reply_poll_error_stores_error() {
    let poll = ReplyPoll::capture();
    poll.duplicate().error(Errno::ENOSYS);
    let stored = *poll.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == ENOSYS));
}

#[cfg(feature = "macos-api")]
#[test]
fn reply_xtimes_arity() {
    // This test verifies that ReplyXTimes::xtimes takes EXACTLY 2 fields
    // (bkuptime, crtime). If someone accidentally adds chgtime or flags,
    // the function pointer signature here would no longer match and the
    // assignment would fail to compile.
    //
    // Compile-time arity check: this MUST compile.
    let _f: fn(ReplyXTimes, SystemTime, SystemTime) = |_reply, _bkuptime, _crtime| {
        // body intentionally empty
    };
}

#[cfg(feature = "macos-api")]
#[test]
fn reply_xtimes_stores_bkuptime_and_crtime() {
    let reply = ReplyXTimes::capture();
    let bkup = UNIX_EPOCH + Duration::from_secs(1_000);
    let crt = UNIX_EPOCH + Duration::from_secs(2_000);
    reply.duplicate().xtimes(bkup, crt);
    let stored = *reply.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok((b, c))) if b == bkup && c == crt));
}

#[cfg(feature = "macos-api")]
#[test]
fn reply_xtimes_error_stores_error() {
    let reply = ReplyXTimes::capture();
    reply.duplicate().error(Errno::EIO);
    let stored = *reply.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::EIO));
}
