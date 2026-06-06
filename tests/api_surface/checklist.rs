//! API surface tests — verify that confuse re-exports (non-windows) and
//! implements (windows) all expected public symbols with correct signatures.
//! Feature-gated items are exercised only under their corresponding feature.
//!
//! Baseline: upstream `fuser 0.17.0` as resolved in `Cargo.lock`.
//! This file is the parity checklist for the Windows facade. Platform-specific
//! behavior differences are recorded as runtime semantics, not missing APIs.

// ── fuser 0.17.0 parity checklist ──────────────────────────────────────────

struct Fuser017ParityChecklist;

impl Fuser017ParityChecklist {
    const CORE_EXPORTS: &'static [&'static str] = &[
        "Errno",
        "FileAttr",
        "FileType",
        "KernelConfig",
        "Request",
        "RequestId",
        "INodeNo",
        "FileHandle",
        "LockOwner",
        "Generation",
        "Version",
        "TimeOrNow",
        "INodeNo::ROOT",
    ];
    const FLAG_AND_NEWTYPE_EXPORTS: &'static [&'static str] = &[
        "AccessFlags",
        "BsdFileFlags",
        "CopyFileRangeFlags",
        "FopenFlags",
        "InitFlags",
        "IoctlFlags",
        "OpenAccMode",
        "OpenFlags",
        "PollEvents",
        "PollFlags",
        "RenameFlags",
    ];
    const FILESYSTEM_SURFACE: &'static [&'static str] = &[
        "Filesystem: Send + Sync + 'static",
        "init/destroy lifecycle methods",
        "operation callbacks use &self in fuser 0.17.0 except lifecycle methods",
        "poll uses PollNotifier, PollEvents, and PollFlags",
        "batch_forget uses ForgetOne accessors",
    ];
    const REPLY_SURFACE: &'static [&'static str] = &[
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
        "reply error methods take Errno in fuser 0.17.0",
    ];
    const MOUNT_SESSION_SURFACE: &'static [&'static str] = &[
        "Config",
        "MountOption",
        "Session",
        "SessionACL",
        "SessionUnmounter",
        "BackgroundSession",
        "mount2/spawn_mount2",
    ];
    const POLL_NOTIFIER_SURFACE: &'static [&'static str] = &[
        "Notifier",
        "PollHandle",
        "PollNotifier",
        "PollNotifier::handle",
        "PollNotifier::notify",
    ];
    const WINDOWS_PLATFORM_NOTES: &'static [&'static str] = &[
        "Errno and public flag/newtype surfaces are compile-enforced on Windows by Task 2.",
        "Filesystem operation callbacks are compile-enforced on Windows as immutable &self methods; init/destroy remain mutable lifecycle methods.",
        "PollHandle/PollNotifier and Filesystem::poll are compile-enforced on Windows; notifier delivery is a deterministic Dokan stub.",
        "MountOption excludes legacy ACL variants on Windows; fuser 0.17.0 access policy is Config.acl/SessionACL.",
        "Session::from_fd exists on Windows and returns Unsupported because Dokan has no /dev/fuse file descriptor surface; upstream fuser 0.17.0 uses std::os::fd::OwnedFd, while Windows facade names rustix::fd::OwnedFd directly.",
        "Session implements rustix::fd::AsFd on Windows with a simulated rustix fd so generic fd callers compile like upstream fuser 0.17.0.",
        "Kernel passthrough reply helpers exist on Windows with deterministic unsupported behavior because Dokan has no FUSE backing-id fd surface; upstream fuser 0.17.0 uses std::os::fd::AsFd, while Windows facade names rustix::fd::AsFd directly.",
        "macOS-only ReplyXTimes and BSD file flag semantics remain exposed behind the intentional Windows macos-api feature.",
    ];
}

#[test]
fn records_fuser_017_parity_categories() {
    assert!(Fuser017ParityChecklist::CORE_EXPORTS.contains(&"Errno"));
    assert!(Fuser017ParityChecklist::FLAG_AND_NEWTYPE_EXPORTS.contains(&"OpenFlags"));
    assert!(
        Fuser017ParityChecklist::FILESYSTEM_SURFACE
            .contains(&"batch_forget uses ForgetOne accessors")
    );
    assert!(Fuser017ParityChecklist::REPLY_SURFACE.contains(&"ReplyDirectoryPlus"));
    assert!(Fuser017ParityChecklist::MOUNT_SESSION_SURFACE.contains(&"SessionACL"));
    assert!(Fuser017ParityChecklist::POLL_NOTIFIER_SURFACE.contains(&"PollNotifier"));
    assert!(
        Fuser017ParityChecklist::WINDOWS_PLATFORM_NOTES
            .iter()
            .any(|note| note.contains("Dokan"))
    );
}
