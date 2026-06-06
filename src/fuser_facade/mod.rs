//! Windows Dokan facade for the fuser 0.17.0 public API.
//!
//! All fuser 0.17.0 signatures that can be named on Windows are mirrored here.
//! The only intentional signature deviations are fd-backed APIs that upstream
//! declares with `std::os::fd::{OwnedFd, AsFd}`: `Session::from_fd`,
//! `Session: rustix::fd::AsFd`, `ReplyOpen::open_backing`, and
//! `ReplyCreate::open_backing`. The `std::os::fd` module is not available for
//! the Windows target, so the facade names `rustix::fd` directly; unsupported
//! runtime fd surfaces return `io::ErrorKind::Unsupported`.

#[cfg(feature = "experimental")]
pub mod experimental;
pub(crate) mod filesystem;
pub(crate) mod fuse_abi;
pub(crate) mod reply;
pub(crate) mod request;
mod session;
pub(crate) mod types;

pub use filesystem::*;
pub use fuse_abi::consts;
pub use reply::*;
pub use request::*;
pub(crate) use session::FsCell;
pub use session::*;
pub use types::*;

pub(crate) mod notifier;
pub use notifier::*;
