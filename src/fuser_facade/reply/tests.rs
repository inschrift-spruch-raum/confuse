use super::super::types::{FileAttr, FileType};
use super::*;
use crate::dokan_impl::{next_dir_offset_from_entries, next_dirplus_offset_from_entries};
use libc::ENOSYS;
use std::io;
use std::io::IoSlice;
use std::time::{Duration, UNIX_EPOCH};

#[derive(Clone, Debug, Default)]
struct TestReplySender;

impl ReplySender for TestReplySender {
    fn send(&self, _data: &[IoSlice<'_>]) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn reply_attr_can_store_payload() {
    let reply = ReplyAttr::default();
    let attr = FileAttr {
        ino: 11,
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
    reply.clone().attr(&Duration::from_secs(1), &attr);
    let stored = *reply.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok(v)) if v.ino == 11 && v.size == 22));
}

#[test]
fn reply_statfs_and_xattr_store_payloads() {
    let statfs = ReplyStatfs::default();
    statfs.clone().statfs(1, 2, 3, 4, 5, 6, 7, 8);
    let s = *statfs.status.lock().expect("lock");
    assert!(matches!(s, Some(Ok((1, 2, 3, 4, 5, 6, 7, 8)))));

    let xattr = ReplyXattr::default();
    xattr.clone().data(&[1, 2, 3]);
    let x = xattr.status.lock().expect("lock").clone();
    assert!(matches!(x, Some(Ok(v)) if v == vec![1, 2, 3]));

    let bmap = ReplyBmap::default();
    bmap.clone().bmap(77);
    let b = *bmap.status.lock().expect("lock");
    assert!(matches!(b, Some(Ok(77))));

    let ioctl = ReplyIoctl::default();
    ioctl.clone().ioctl(3, &[9, 8]);
    let i = ioctl.status.lock().expect("lock").clone();
    assert!(matches!(i, Some(Ok((3, v))) if v == vec![9, 8]));
}

#[test]
fn reply_directory_new_size_enforces_aligned_byte_capacity() {
    let mut reply = ReplyDirectory::new(1, TestReplySender, 32);
    assert!(!reply.add(1, 0, FileType::RegularFile, std::ffi::OsStr::new("a")));
    assert!(reply.add(2, 1, FileType::RegularFile, std::ffi::OsStr::new("a")));
}

#[test]
fn reply_directoryplus_new_size_enforces_aligned_byte_capacity() {
    let attr = FileAttr {
        ino: 1,
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
        1,
        0,
        std::ffi::OsStr::new("a"),
        &Duration::from_secs(1),
        &attr,
        0
    ));
    assert!(reply.add(
        2,
        1,
        std::ffi::OsStr::new("a"),
        &Duration::from_secs(1),
        &attr,
        0
    ));
}

#[test]
fn reply_directory_preserves_entry_offsets_and_next_offset() {
    let mut reply = ReplyDirectory::default();
    assert!(!reply.add(10, 7, FileType::RegularFile, std::ffi::OsStr::new("a")));
    assert!(!reply.add(11, 9, FileType::Directory, std::ffi::OsStr::new("b")));
    let entries = reply.entries.lock().expect("lock").clone();
    assert_eq!(entries[0].1, 7);
    assert_eq!(entries[1].1, 9);
    assert_eq!(next_dir_offset_from_entries(0, &entries), 9);
    assert_eq!(next_dir_offset_from_entries(5, &[]), 5);
}

#[test]
fn reply_directoryplus_preserves_entry_offsets_and_next_offset() {
    let attr = FileAttr {
        ino: 1,
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
    let mut reply = ReplyDirectoryPlus::default();
    assert!(!reply.add(
        10,
        13,
        std::ffi::OsStr::new("a"),
        &Duration::from_secs(1),
        &attr,
        0
    ));
    assert!(!reply.add(
        11,
        15,
        std::ffi::OsStr::new("b"),
        &Duration::from_secs(1),
        &attr,
        0
    ));
    let entries = reply.entries.lock().expect("lock").clone();
    assert_eq!(entries[0].1, 13);
    assert_eq!(entries[1].1, 15);
    assert_eq!(next_dirplus_offset_from_entries(0, &entries), 15);
    assert_eq!(next_dirplus_offset_from_entries(8, &[]), 8);
}

#[test]
fn reply_directory_non_ascii_uses_utf8_byte_capacity() {
    let mut reply = ReplyDirectory::new(1, TestReplySender, 32);
    assert!(!reply.add(1, 0, FileType::RegularFile, std::ffi::OsStr::new("é")));
    assert!(reply.add(2, 1, FileType::RegularFile, std::ffi::OsStr::new("é")));
}

#[test]
fn reply_directoryplus_non_ascii_uses_utf8_byte_capacity() {
    let attr = FileAttr {
        ino: 1,
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
        1,
        0,
        std::ffi::OsStr::new("é"),
        &Duration::from_secs(1),
        &attr,
        0
    ));
    assert!(reply.add(
        2,
        1,
        std::ffi::OsStr::new("é"),
        &Duration::from_secs(1),
        &attr,
        0
    ));
}

#[test]
fn reply_xattr_size_sets_size_hint_and_empty_data() {
    let xattr = ReplyXattr::default();
    xattr.clone().size(42);
    let hint = *xattr.size_hint.lock().expect("lock");
    assert_eq!(hint, Some(42));
    let data = xattr.status.lock().expect("lock").clone();
    assert!(matches!(data, Some(Ok(v)) if v.is_empty()));
}

#[test]
fn reply_lseek_offset_stores_value() {
    let lseek = ReplyLseek::default();
    lseek.clone().offset(12345);
    let stored = *lseek.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok(12345))));
}

#[test]
fn reply_lseek_error_stores_error() {
    let lseek = ReplyLseek::default();
    lseek.clone().error(ENOSYS);
    let stored = *lseek.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == ENOSYS));
}

#[test]
fn reply_lock_locked_stores_values() {
    let lock = ReplyLock::default();
    lock.clone().locked(0, 100, 1, 42);
    let stored = *lock.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok((0, 100, 1, 42)))));
}

#[test]
fn reply_lock_error_stores_error() {
    let lock = ReplyLock::default();
    lock.clone().error(ENOSYS);
    let stored = *lock.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == ENOSYS));
}

#[test]
fn reply_create_stored_attr_fh_flags() {
    let create = ReplyCreate::default();
    let attr = FileAttr {
        ino: 99,
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
    create
        .clone()
        .created(&Duration::from_secs(1), &attr, 0, 33, 7);
    let stored = *create.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok((a, 33, 7))) if a.ino == 99 && a.perm == 0o755));
}

#[test]
fn reply_create_error_stores_error() {
    let create = ReplyCreate::default();
    create.clone().error(ENOSYS);
    let stored = *create.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == ENOSYS));
}

#[test]
fn fuse_forget_one_fields_are_accessible() {
    let f = fuse_forget_one {
        nodeid: 7,
        nlookup: 3,
    };
    assert_eq!(f.nodeid, 7);
    assert_eq!(f.nlookup, 3);
}

#[test]
fn reply_entry_error_stores_errno() {
    let entry = ReplyEntry::default();
    entry.clone().error(libc::ENOENT);
    let stored = *entry.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::ENOENT));
}

#[test]
fn reply_open_error_stores_errno() {
    let open = ReplyOpen::default();
    open.clone().error(libc::EACCES);
    let stored = *open.opened.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::EACCES));
}

#[test]
fn reply_write_error_stores_errno() {
    let write = ReplyWrite::default();
    write.clone().error(libc::ENOSPC);
    let stored = *write.written.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::ENOSPC));
}

#[test]
fn reply_data_error_stores_errno() {
    let data = ReplyData::default();
    data.clone().error(libc::EIO);
    let stored = data.data.lock().expect("lock").clone();
    assert!(matches!(stored, Some(Err(e)) if e == libc::EIO));
}

#[test]
fn reply_statfs_error_stores_errno() {
    let statfs = ReplyStatfs::default();
    statfs.clone().error(libc::ENOSYS);
    let stored = *statfs.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == libc::ENOSYS));
}

#[test]
fn reply_empty_ok_and_error() {
    let ok_reply = ReplyEmpty::default();
    ok_reply.clone().ok();
    let stored = *ok_reply.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok(()))));

    let err_reply = ReplyEmpty::default();
    err_reply.clone().error(1);
    let stored = *err_reply.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(1))));
}

#[cfg(feature = "abi-7-11")]
#[test]
fn reply_poll_ok_stores_unit() {
    let poll = ReplyPoll::default();
    poll.clone().poll(0);
    let stored = *poll.status.lock().expect("lock");
    assert!(matches!(stored, Some(Ok(()))));
}

#[cfg(feature = "abi-7-11")]
#[test]
fn reply_poll_error_stores_error() {
    let poll = ReplyPoll::default();
    poll.clone().error(ENOSYS);
    let stored = *poll.status.lock().expect("lock");
    assert!(matches!(stored, Some(Err(e)) if e == ENOSYS));
}
