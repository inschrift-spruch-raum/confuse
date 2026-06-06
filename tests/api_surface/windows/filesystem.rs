use super::*;

#[test]
fn filesystem_operation_callbacks_accept_immutable_receivers() {
    assert_filesystem_receivers_compile();
    assert_filesystem_metadata_callbacks();
    assert_filesystem_entry_callbacks();
    assert_filesystem_io_directory_callbacks();
    assert_filesystem_xattr_lock_misc_callbacks();
    assert_filesystem_macos_callbacks();
}

struct ImmutableReceiverFs;
impl confuse::Filesystem for ImmutableReceiverFs {
    fn lookup(
        &self, _req: &confuse::Request, _parent: confuse::INodeNo, _name: &OsStr,
        reply: confuse::ReplyEntry,
    ) {
        reply.error(confuse::Errno::ENOSYS);
    }

    fn read(
        &self, _req: &confuse::Request, _ino: confuse::INodeNo, _fh: confuse::FileHandle,
        _offset: u64, _size: u32, _flags: confuse::OpenFlags,
        _lock_owner: Option<confuse::LockOwner>, reply: confuse::ReplyData,
    ) {
        reply.error(confuse::Errno::ENOSYS);
    }

    fn write(
        &self, _req: &confuse::Request, _ino: confuse::INodeNo, _fh: confuse::FileHandle,
        _offset: u64, _data: &[u8], _write_flags: confuse::WriteFlags, _flags: confuse::OpenFlags,
        _lock_owner: Option<confuse::LockOwner>, reply: confuse::ReplyWrite,
    ) {
        reply.error(confuse::Errno::ENOSYS);
    }

    fn poll(
        &self, _req: &confuse::Request, _ino: confuse::INodeNo, _fh: confuse::FileHandle,
        _ph: confuse::PollNotifier, _events: confuse::PollEvents, _flags: confuse::PollFlags,
        reply: confuse::ReplyPoll,
    ) {
        reply.error(confuse::Errno::ENOSYS);
    }

    fn batch_forget(&self, _req: &confuse::Request, _nodes: &[confuse::ForgetOne]) {}
}

fn assert_filesystem_receivers_compile() {
    fn assert_filesystem_trait<T: confuse::Filesystem>() {}
    assert_filesystem_trait::<Dummy>();
    assert_filesystem_trait::<ImmutableReceiverFs>();
}

fn assert_filesystem_metadata_callbacks() {
    let _init: fn(
        &mut ImmutableReceiverFs,
        &confuse::Request,
        &mut confuse::KernelConfig,
    ) -> io::Result<()> = <ImmutableReceiverFs as confuse::Filesystem>::init;
    let _destroy: fn(&mut ImmutableReceiverFs) =
        <ImmutableReceiverFs as confuse::Filesystem>::destroy;
    let _lookup: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        confuse::ReplyEntry,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::lookup;
    let _forget: fn(&ImmutableReceiverFs, &confuse::Request, confuse::INodeNo, u64) =
        <ImmutableReceiverFs as confuse::Filesystem>::forget;
    let _batch_forget: fn(&ImmutableReceiverFs, &confuse::Request, &[confuse::ForgetOne]) =
        <ImmutableReceiverFs as confuse::Filesystem>::batch_forget;
    let _getattr: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        Option<confuse::FileHandle>,
        confuse::ReplyAttr,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::getattr;
    type SetattrFn = fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        Option<u32>,
        Option<u32>,
        Option<u32>,
        Option<u64>,
        Option<confuse::TimeOrNow>,
        Option<confuse::TimeOrNow>,
        Option<std::time::SystemTime>,
        Option<confuse::FileHandle>,
        Option<std::time::SystemTime>,
        Option<std::time::SystemTime>,
        Option<std::time::SystemTime>,
        Option<confuse::BsdFileFlags>,
        confuse::ReplyAttr,
    );
    let _setattr: SetattrFn = <ImmutableReceiverFs as confuse::Filesystem>::setattr;
    let _readlink: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::ReplyData,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::readlink;
}

fn assert_filesystem_entry_callbacks() {
    let _mknod: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        u32,
        u32,
        u32,
        confuse::ReplyEntry,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::mknod;
    let _mkdir: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        u32,
        u32,
        confuse::ReplyEntry,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::mkdir;
    let _unlink: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::unlink;
    let _rmdir: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::rmdir;
    let _symlink: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        &Path,
        confuse::ReplyEntry,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::symlink;
    let _rename: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        confuse::INodeNo,
        &OsStr,
        confuse::RenameFlags,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::rename;
    let _link: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::INodeNo,
        &OsStr,
        confuse::ReplyEntry,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::link;
    let _open: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::OpenFlags,
        confuse::ReplyOpen,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::open;
}

fn assert_filesystem_io_directory_callbacks() {
    type ReadFn = fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        u64,
        u32,
        confuse::OpenFlags,
        Option<confuse::LockOwner>,
        confuse::ReplyData,
    );
    let _read: ReadFn = <ImmutableReceiverFs as confuse::Filesystem>::read;
    type WriteFn = fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        u64,
        &[u8],
        confuse::WriteFlags,
        confuse::OpenFlags,
        Option<confuse::LockOwner>,
        confuse::ReplyWrite,
    );
    let _write: WriteFn = <ImmutableReceiverFs as confuse::Filesystem>::write;
    let _flush: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        confuse::LockOwner,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::flush;
    let _release: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        confuse::OpenFlags,
        Option<confuse::LockOwner>,
        bool,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::release;
    let _fsync: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        bool,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::fsync;
    let _opendir: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::OpenFlags,
        confuse::ReplyOpen,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::opendir;
    let _readdir: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        u64,
        confuse::ReplyDirectory,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::readdir;
    let _readdirplus: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        u64,
        confuse::ReplyDirectoryPlus,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::readdirplus;
    let _releasedir: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        confuse::OpenFlags,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::releasedir;
    let _fsyncdir: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        bool,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::fsyncdir;
    let _statfs: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::ReplyStatfs,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::statfs;
}

fn assert_filesystem_xattr_lock_misc_callbacks() {
    let _setxattr: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        &[u8],
        i32,
        u32,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::setxattr;
    let _getxattr: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        u32,
        confuse::ReplyXattr,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::getxattr;
    let _listxattr: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        u32,
        confuse::ReplyXattr,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::listxattr;
    let _removexattr: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::removexattr;
    let _access: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::AccessFlags,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::access;
    let _create: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        &OsStr,
        u32,
        u32,
        i32,
        confuse::ReplyCreate,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::create;
    type GetlkFn = fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        confuse::LockOwner,
        u64,
        u64,
        i32,
        u32,
        confuse::ReplyLock,
    );
    let _getlk: GetlkFn = <ImmutableReceiverFs as confuse::Filesystem>::getlk;
    type SetlkFn = fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        confuse::LockOwner,
        u64,
        u64,
        i32,
        u32,
        bool,
        confuse::ReplyEmpty,
    );
    let _setlk: SetlkFn = <ImmutableReceiverFs as confuse::Filesystem>::setlk;
    let _bmap: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        u32,
        u64,
        confuse::ReplyBmap,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::bmap;
    type IoctlFn = fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        confuse::IoctlFlags,
        u32,
        &[u8],
        u32,
        confuse::ReplyIoctl,
    );
    let _ioctl: IoctlFn = <ImmutableReceiverFs as confuse::Filesystem>::ioctl;
    let _fallocate: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        u64,
        u64,
        i32,
        confuse::ReplyEmpty,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::fallocate;
    let _lseek: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        i64,
        i32,
        confuse::ReplyLseek,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::lseek;
    type CopyFileRangeFn = fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        u64,
        confuse::INodeNo,
        confuse::FileHandle,
        u64,
        u64,
        confuse::CopyFileRangeFlags,
        confuse::ReplyWrite,
    );
    let _copy_file_range: CopyFileRangeFn =
        <ImmutableReceiverFs as confuse::Filesystem>::copy_file_range;
    let _poll: fn(
        &ImmutableReceiverFs,
        &confuse::Request,
        confuse::INodeNo,
        confuse::FileHandle,
        confuse::PollNotifier,
        confuse::PollEvents,
        confuse::PollFlags,
        confuse::ReplyPoll,
    ) = <ImmutableReceiverFs as confuse::Filesystem>::poll;
}

fn assert_filesystem_macos_callbacks() {
    #[cfg(feature = "macos-api")]
    {
        let _setvolname: fn(&ImmutableReceiverFs, &confuse::Request, &OsStr, confuse::ReplyEmpty) =
            <ImmutableReceiverFs as confuse::Filesystem>::setvolname;
        let _exchange: fn(
            &ImmutableReceiverFs,
            &confuse::Request,
            confuse::INodeNo,
            &OsStr,
            confuse::INodeNo,
            &OsStr,
            u64,
            confuse::ReplyEmpty,
        ) = <ImmutableReceiverFs as confuse::Filesystem>::exchange;
        let _getxtimes: fn(
            &ImmutableReceiverFs,
            &confuse::Request,
            confuse::INodeNo,
            confuse::ReplyXTimes,
        ) = <ImmutableReceiverFs as confuse::Filesystem>::getxtimes;
    }
}
