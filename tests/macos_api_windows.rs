//! Windows-side macOS-only public facade API checks.

#[cfg(all(windows, feature = "macos-api"))]
mod macos_api {
    use std::ffi::OsStr;
    use std::time::SystemTime;

    use confuse::Errno;
    use confuse::Filesystem;
    use confuse::INodeNo;
    use confuse::ReplyEmpty;
    use confuse::ReplyXTimes;
    use confuse::Request;

    struct MacOsOnlyFs;

    impl Filesystem for MacOsOnlyFs {
        fn setvolname(&self, _req: &Request, _name: &OsStr, reply: ReplyEmpty) {
            reply.error(Errno::ENOSYS);
        }

        fn exchange(
            &self, _req: &Request, _parent: INodeNo, _name: &OsStr, _newparent: INodeNo,
            _newname: &OsStr, _options: u64, reply: ReplyEmpty,
        ) {
            reply.error(Errno::ENOSYS);
        }

        fn getxtimes(&self, _req: &Request, _ino: INodeNo, reply: ReplyXTimes) {
            reply.error(Errno::ENOSYS);
        }
    }

    #[test]
    fn filesystem_trait_macos_only_methods_have_spec_signatures() {
        let _ = MacOsOnlyFs;
    }

    #[test]
    fn reply_xtimes_xtimes_accepts_exactly_two_system_time_args() {
        let _xtimes: fn(ReplyXTimes, SystemTime, SystemTime) = ReplyXTimes::xtimes;
    }

    #[test]
    fn reply_xtimes_error_takes_errno() {
        let _error: fn(ReplyXTimes, Errno) = ReplyXTimes::error;
    }
}
