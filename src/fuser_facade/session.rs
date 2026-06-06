use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

use std::net::TcpListener;
use widestring::U16CString;

use crate::dokan_impl::adapter::{DokanAdapter, PathResolver};
use crate::dokan_impl::{derive_volume_names, mountpoint_to_u16, to_dokan_mount_options};

use super::filesystem::Filesystem;
use super::notifier::Notifier;
use super::reply::ChannelSender;
use super::types::{Config, MountOption, SessionACL};

pub(crate) struct FsCell<FS>(pub(crate) Mutex<FS>);

impl<FS> FsCell<FS> {
    pub(crate) fn lock(&self) -> std::sync::LockResult<std::sync::MutexGuard<'_, FS>> {
        self.0.lock()
    }
}

pub struct Session<FS: Filesystem> {
    filesystem: Arc<FsCell<FS>>,
    resolver: Arc<Mutex<PathResolver>>,
    fd: rustix::fd::OwnedFd,
    mountpoint: U16CString,
    mountpoint_path: PathBuf,
    options: Config,
    mount_state: Arc<Mutex<bool>>,
    destroyed: Arc<AtomicBool>,
    sender: ChannelSender,
}

impl<FS: Filesystem> Session<FS> {
    pub fn new<P: AsRef<Path>>(
        filesystem: FS, mountpoint: P, options: &Config,
    ) -> io::Result<Session<FS>> {
        let mountpoint_path = mountpoint.as_ref().to_path_buf();
        let mountpoint = mountpoint_to_u16(mountpoint_path.as_os_str())?;
        validate_session_config(options)?;
        let _ = to_dokan_mount_options(options.mount_options.as_ref())?;
        Ok(Self {
            filesystem: Arc::new(FsCell(Mutex::new(filesystem))),
            resolver: Arc::new(Mutex::new(configured_path_resolver(options))),
            fd: new_simulated_fd()?,
            mountpoint,
            mountpoint_path,
            options: options.clone(),
            mount_state: Arc::new(Mutex::new(true)),
            destroyed: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            sender: ChannelSender,
        })
    }

    /// Wrap an existing `/dev/fuse` file descriptor.
    ///
    /// Windows facade compatibility note: upstream fuser 0.17.0 takes
    /// `std::os::fd::OwnedFd` here. That type cannot be named on the Windows
    /// target, so this facade uses `rustix::fd::OwnedFd` directly and always
    /// reports the unsupported `/dev/fuse` surface at runtime.
    pub fn from_fd(
        _filesystem: FS, _fd: rustix::fd::OwnedFd, _acl: SessionACL, _config: Config,
    ) -> io::Result<Self> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "wrapping an existing /dev/fuse file descriptor is unsupported by the Windows Dokan facade",
        ))
    }

    fn run(&mut self) -> io::Result<()> {
        let (volume_name, fs_name) = derive_volume_names(self.options.mount_options.as_ref());
        let handler = DokanAdapter {
            fs: Arc::clone(&self.filesystem),
            handles: Arc::new(Mutex::new(HashMap::new())),
            resolver: Arc::clone(&self.resolver),
            dir_offsets: Arc::new(Mutex::new(HashMap::new())),
            volume_name,
            fs_name,
            destroyed: Arc::clone(&self.destroyed),
        };
        let mut options = to_dokan_mount_options(self.options.mount_options.as_ref())?;
        apply_session_threading(&self.options, &mut options);
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

    fn mountpoint(&self) -> &Path {
        self.mountpoint_path.as_path()
    }

    pub fn unmount(&mut self) -> io::Result<()> {
        unmount_once(&self.mountpoint, &self.mount_state)
    }

    pub fn notifier(&self) -> Notifier {
        Notifier::with_resolver(self.sender.clone(), Arc::clone(&self.resolver))
    }
}

impl<FS: Filesystem> rustix::fd::AsFd for Session<FS> {
    fn as_fd(&self) -> rustix::fd::BorrowedFd<'_> {
        rustix::fd::AsFd::as_fd(&self.fd)
    }
}

impl<FS: 'static + Filesystem + Send> Session<FS> {
    pub fn spawn(self) -> io::Result<BackgroundSession> {
        BackgroundSession::new(self)
    }
}

pub struct BackgroundSession {
    mountpoint: PathBuf,
    pub guard: JoinHandle<io::Result<()>>,
    sender: ChannelSender,
    resolver: Arc<Mutex<PathResolver>>,
    _mount: BackgroundMountGuard,
}

struct BackgroundMountGuard {
    mountpoint: U16CString,
    mount_state: Arc<Mutex<bool>>,
}

#[derive(Debug)]
pub struct SessionUnmounter {
    mountpoint: U16CString,
    mount_state: Arc<Mutex<bool>>,
}

fn unmount_once(mountpoint: &U16CString, state: &Arc<Mutex<bool>>) -> io::Result<()> {
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

fn validate_session_config(config: &Config) -> io::Result<()> {
    if config.acl == SessionACL::Owner
        && config
            .mount_options
            .iter()
            .any(|option| matches!(option, MountOption::AutoUnmount))
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "auto_unmount requires allow_other or allow_root access policy",
        ));
    }
    if let Some(0) = config.n_threads {
        return Err(io::Error::other("n_threads"));
    }
    Ok(())
}

fn apply_session_threading(config: &Config, options: &mut dokan::MountOptions<'_>) {
    options.single_thread = config.n_threads == Some(1);
}

fn new_simulated_fd() -> io::Result<rustix::fd::OwnedFd> {
    TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).map(Into::into)
}

fn configured_path_resolver(config: &Config) -> PathResolver {
    let mut resolver = PathResolver::default();
    for option in &config.mount_options {
        if let MountOption::CUSTOM(value) = option {
            if value == "negative_ttl=0" || value == "negative_ttl=off" {
                resolver.set_negative_ttl(std::time::Duration::ZERO);
            } else if let Some(ms) = value.strip_prefix("negative_ttl_ms=")
                && let Ok(ms) = ms.parse::<u64>()
            {
                resolver.set_negative_ttl(std::time::Duration::from_millis(ms));
            }
        }
    }
    resolver
}

impl SessionUnmounter {
    pub fn unmount(&mut self) -> io::Result<()> {
        unmount_once(&self.mountpoint, &self.mount_state)
    }
}

impl BackgroundMountGuard {
    fn unmount(&mut self) -> io::Result<()> {
        unmount_once(&self.mountpoint, &self.mount_state)
    }
}

impl BackgroundSession {
    fn new<FS: Filesystem + Send + 'static>(se: Session<FS>) -> io::Result<BackgroundSession> {
        let mountpoint = se.mountpoint().to_path_buf();
        let sender = se.sender.clone();
        let resolver = Arc::clone(&se.resolver);
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
            resolver,
            _mount: BackgroundMountGuard {
                mountpoint: mountpoint_u16,
                mount_state,
            },
        })
    }

    pub fn join(self) -> io::Result<()> {
        let Self {
            mountpoint: _,
            guard,
            sender: _,
            resolver: _,
            _mount,
        } = self;
        drop(_mount);
        guard
            .join()
            .map_err(|_| io::Error::other("background mount thread panicked"))?
    }

    pub fn umount_and_join(mut self) -> io::Result<()> {
        self._mount.unmount()?;
        self.join()
    }

    pub fn notifier(&self) -> Notifier {
        Notifier::with_resolver(self.sender.clone(), Arc::clone(&self.resolver))
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
    Session::new(filesystem, mountpoint, options)?.run()
}

pub fn spawn_mount2<'a, FS: Filesystem + Send + 'static + 'a, P: AsRef<Path>>(
    filesystem: FS, mountpoint: P, options: &Config,
) -> io::Result<BackgroundSession> {
    Session::new(filesystem, mountpoint, options)?.spawn()
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::super::filesystem::Filesystem;
    use super::super::types::{Config, SessionACL};
    use super::*;
    use std::path::Path;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[derive(Default)]
    struct DefaultFs;

    impl Filesystem for DefaultFs {}

    #[test]
    fn session_new_accepts_acl_config_for_fuser_parity() {
        let res = Session::new(
            DefaultFs,
            Path::new("."),
            &Config {
                acl: SessionACL::All,
                ..Config::default()
            },
        );
        assert!(res.is_ok());
    }

    #[test]
    fn session_new_rejects_auto_unmount_without_shared_acl() {
        let mut config = Config::default();
        config.mount_options.push(MountOption::AutoUnmount);

        let err = match Session::new(DefaultFs, Path::new("."), &config) {
            Ok(_) => panic!("auto_unmount requires shared ACL"),
            Err(err) => err,
        };

        assert_eq!(err.kind(), io::ErrorKind::InvalidInput);
    }

    #[test]
    fn session_new_rejects_zero_thread_count() {
        let zero_threads = Config {
            n_threads: Some(0),
            ..Config::default()
        };
        assert!(Session::new(DefaultFs, Path::new("."), &zero_threads).is_err());
    }

    #[test]
    fn session_threading_allows_concurrent_dokan_callbacks_by_default() {
        let mut default_options = dokan::MountOptions::default();
        apply_session_threading(&Config::default(), &mut default_options);
        assert!(!default_options.single_thread);

        let mut explicit_options = dokan::MountOptions::default();
        apply_session_threading(
            &Config {
                n_threads: Some(1),
                ..Config::default()
            },
            &mut explicit_options,
        );
        assert!(explicit_options.single_thread);

        let mut multi_thread_options = dokan::MountOptions::default();
        apply_session_threading(
            &Config {
                n_threads: Some(2),
                ..Config::default()
            },
            &mut multi_thread_options,
        );
        assert!(!multi_thread_options.single_thread);
    }

    #[test]
    fn session_new_accepts_multi_thread_config() {
        let config = Config {
            n_threads: Some(2),
            ..Config::default()
        };

        assert!(Session::new(DefaultFs, Path::new("."), &config).is_ok());
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
            let session =
                Session::new(fs, Path::new("."), &Config::default()).expect("session new");
            drop(session);
        }
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn session_mountpoint_returns_constructor_path() {
        let session =
            Session::new(DefaultFs, Path::new("."), &Config::default()).expect("session new");
        assert_eq!(session.mountpoint(), Path::new("."));
    }

    fn background_session_from_guard(guard: JoinHandle<io::Result<()>>) -> BackgroundSession {
        BackgroundSession {
            mountpoint: PathBuf::from("."),
            guard,
            sender: ChannelSender,
            resolver: Arc::new(Mutex::new(PathResolver::default())),
            _mount: BackgroundMountGuard {
                mountpoint: U16CString::from_str(".").expect("mountpoint"),
                mount_state: Arc::new(Mutex::new(false)),
            },
        }
    }

    #[test]
    fn background_join_propagates_thread_result() {
        let session =
            background_session_from_guard(thread::spawn(|| Err(io::Error::other("mount failed"))));

        let err = session.join().expect_err("join should return inner error");
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert_eq!(err.to_string(), "mount failed");
    }

    #[test]
    fn background_join_maps_thread_panic_to_io_error() {
        let session = background_session_from_guard(thread::spawn(|| -> io::Result<()> {
            panic!("mount thread panic")
        }));

        let err = session.join().expect_err("join should map panic");
        assert_eq!(err.kind(), io::ErrorKind::Other);
        assert_eq!(err.to_string(), "background mount thread panicked");
    }
}
