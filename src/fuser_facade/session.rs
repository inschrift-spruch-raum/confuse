use std::collections::HashMap;
use std::ffi::OsStr;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use widestring::U16CString;

use crate::dokan_impl::adapter::DokanAdapter;
use crate::dokan_impl::{
    derive_volume_names, mountpoint_to_u16, parse_mount_options_from_args, to_dokan_mount_options,
};

use super::filesystem::Filesystem;
use super::notifier::Notifier;
use super::reply::ChannelSender;
use super::types::Config;

pub(crate) struct FsCell<FS>(pub(crate) Mutex<FS>);

impl<FS> FsCell<FS> {
    pub(crate) fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, FS>> {
        self.0.lock()
    }
}

unsafe impl<FS> Send for FsCell<FS> {}
unsafe impl<FS> Sync for FsCell<FS> {}

pub struct Session<FS: Filesystem> {
    pub(crate) filesystem: Arc<FsCell<FS>>,
    pub(crate) mountpoint: U16CString,
    pub(crate) mountpoint_path: PathBuf,
    pub(crate) options: Config,
    pub(crate) mount_state: Arc<Mutex<bool>>,
    pub(crate) destroyed: Arc<AtomicBool>,
    pub(crate) sender: ChannelSender,
}

impl<FS: Filesystem> Session<FS> {
    pub fn new(
        filesystem: FS, mountpoint: &Path, options: &Config,
    ) -> io::Result<Session<FS>> {
        let mountpoint_path = mountpoint.to_path_buf();
        let mountpoint = mountpoint_to_u16(mountpoint_path.as_os_str())?;
        let _ = to_dokan_mount_options(options.mount_options.as_ref())?;
        Ok(Self {
            filesystem: Arc::new(FsCell(Mutex::new(filesystem))),
            mountpoint,
            mountpoint_path,
            options: options.clone(),
            mount_state: Arc::new(Mutex::new(true)),
            destroyed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sender: ChannelSender,
        })
    }

    pub fn run(&mut self) -> io::Result<()> {
        let (volume_name, fs_name) = derive_volume_names(self.options.mount_options.as_ref());
        let handler = DokanAdapter {
            fs: Arc::clone(&self.filesystem),
            handles: Arc::new(Mutex::new(HashMap::new())),
            dir_offsets: Arc::new(Mutex::new(HashMap::new())),
            volume_name,
            fs_name,
            destroyed: Arc::clone(&self.destroyed),
        };
        let options = to_dokan_mount_options(self.options.mount_options.as_ref())?;
        dokan::init();
        let mut mounter =
            dokan::FileSystemMounter::new(&handler, self.mountpoint.as_ucstr(), &options);
        let file_system = mounter
            .mount()
            .map_err(|err| io::Error::other(format!("dokan mount failed: {err}")))?;
        // FileSystem::drop calls DokanWaitForFileSystemClosed, which blocks until
        // the volume is unmounted.  We must drop it *before* calling dokan::shutdown(),
        // otherwise shutdown() destroys Dokan internals while callbacks are still running.
        drop(file_system);
        dokan::shutdown();
        Ok(())
    }

    pub fn unmount_callable(&mut self) -> SessionUnmounter {
        SessionUnmounter {
            mountpoint: self.mountpoint.clone(),
            mount_state: Arc::clone(&self.mount_state),
        }
    }

    pub fn mountpoint(&self) -> &Path {
        self.mountpoint_path.as_path()
    }

    pub fn unmount(&mut self) {
        let _ = unmount_once(&self.mountpoint, &self.mount_state);
    }

    pub fn notifier(&self) -> Notifier {
        Notifier::new(self.sender.clone())
    }
}

impl<FS: 'static + Filesystem + Send> Session<FS> {
    pub fn spawn(self) -> io::Result<BackgroundSession> {
        BackgroundSession::new(self)
    }
}

pub struct BackgroundSession {
    pub mountpoint: PathBuf,
    pub guard: JoinHandle<io::Result<()>>,
    sender: ChannelSender,
    _mount: BackgroundMountGuard,
}

pub(crate) struct BackgroundMountGuard {
    mountpoint: U16CString,
    mount_state: Arc<Mutex<bool>>,
}

#[derive(Debug)]
pub struct SessionUnmounter {
    mountpoint: U16CString,
    mount_state: Arc<Mutex<bool>>,
}

pub(crate) fn unmount_once(mountpoint: &U16CString, state: &Arc<Mutex<bool>>) -> io::Result<()> {
    let mut mounted = state
        .lock()
        .map_err(|_| io::Error::other("mount state poisoned"))?;
    if !*mounted {
        return Ok(());
    }
    if !dokan::unmount(mountpoint.as_ucstr()) {
        return Err(io::Error::other("dokan unmount failed"));
    }
    *mounted = false;
    Ok(())
}

impl SessionUnmounter {
    pub fn unmount(&mut self) -> io::Result<()> {
        unmount_once(&self.mountpoint, &self.mount_state)
    }
}

impl BackgroundSession {
    pub fn new<FS: Filesystem + Send + 'static>(se: Session<FS>) -> io::Result<BackgroundSession> {
        let mountpoint = se.mountpoint().to_path_buf();
        let sender = se.sender.clone();
        let mountpoint_u16 = se.mountpoint.clone();
        let mount_state = Arc::clone(&se.mount_state);
        let guard = thread::spawn(move || {
            let mut s = se;
            s.run()
        });
        Ok(BackgroundSession {
            guard,
            mountpoint,
            sender,
            _mount: BackgroundMountGuard {
                mountpoint: mountpoint_u16,
                mount_state,
            },
        })
    }

    pub fn join(self) {
        let Self {
            mountpoint: _,
            guard,
            sender: _,
            _mount,
        } = self;
        drop(_mount);
        match guard.join() {
            Ok(result) => result.expect("background mount failed"),
            Err(_) => panic!("background mount thread panicked"),
        }
    }

    pub fn notifier(&self) -> Notifier {
        Notifier::new(self.sender.clone())
    }
}

impl std::fmt::Debug for BackgroundSession {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> Result<(), std::fmt::Error> {
        write!(
            f,
            "BackgroundSession {{ mountpoint: {:?}, guard: JoinHandle<io::Result<()>> }}",
            self.mountpoint
        )
    }
}

impl<FS: Filesystem> Drop for Session<FS> {
    fn drop(&mut self) {
        if !self.destroyed.swap(true, Ordering::SeqCst)
            && let Ok(mut fs) = self.filesystem.lock()
        {
            fs.destroy();
        }
    }
}

impl Drop for BackgroundMountGuard {
    fn drop(&mut self) {
        let _ = unmount_once(&self.mountpoint, &self.mount_state);
    }
}

pub fn mount2<FS: Filesystem, P: AsRef<Path>>(
    filesystem: FS, mountpoint: P, options: &Config,
) -> io::Result<()> {
    Session::new(filesystem, mountpoint.as_ref(), options)?.run()
}

pub fn spawn_mount2<'a, FS: Filesystem + Send + 'static + 'a, P: AsRef<Path>>(
    filesystem: FS, mountpoint: P, options: &Config,
) -> io::Result<BackgroundSession> {
    Session::new(filesystem, mountpoint.as_ref(), options)?.spawn()
}

#[deprecated(note = "use mount2() instead")]
pub fn mount<FS: Filesystem, P: AsRef<Path>>(
    filesystem: FS, mountpoint: P, options: &[&OsStr],
) -> io::Result<()> {
    let parsed = parse_mount_options_from_args(options)?;
    mount2(filesystem, mountpoint, &Config { mount_options: parsed, ..Config::default() })
}

#[deprecated(note = "use spawn_mount2() instead")]
pub fn spawn_mount<'a, FS: Filesystem + Send + 'static + 'a, P: AsRef<Path>>(
    filesystem: FS, mountpoint: P, options: &[&OsStr],
) -> io::Result<BackgroundSession> {
    let parsed = parse_mount_options_from_args(options)?;
    spawn_mount2(filesystem, mountpoint, &Config { mount_options: parsed, ..Config::default() })
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::filesystem::Filesystem;
    use super::super::types::{Config, MountOption};
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Default)]
    struct DefaultFs;

    impl Filesystem for DefaultFs {}

    #[test]
    fn session_new_accepts_allow_other_and_allow_root_for_fuser_parity() {
        let res = Session::new(
            DefaultFs,
            Path::new("."),
            &Config { mount_options: vec![MountOption::AllowOther, MountOption::AllowRoot], ..Config::default() },
        );
        assert!(res.is_ok());
    }

    struct DestroyFs {
        calls: Arc<AtomicU64>,
    }

    impl Filesystem for DestroyFs {
        fn destroy(&mut self) {
            self.calls.fetch_add(1, Ordering::SeqCst);
        }
    }

    #[test]
    fn session_drop_calls_destroy_once() {
        let calls = Arc::new(AtomicU64::new(0));
        {
            let fs = DestroyFs {
                calls: Arc::clone(&calls),
            };
            let session = Session::new(fs, Path::new("."), &Config::default()).expect("session new");
            drop(session);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn session_mountpoint_returns_constructor_path() {
        let session = Session::new(DefaultFs, Path::new("."), &Config::default()).expect("session new");
        assert_eq!(session.mountpoint(), Path::new("."));
    }
}
