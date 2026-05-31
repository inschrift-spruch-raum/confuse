#![deny(unsafe_op_in_unsafe_fn)]

#[cfg(not(windows))]
pub use fuser::*;

#[cfg(windows)]
pub use self::fuser_facade::*;

#[cfg(windows)]
pub(crate) mod dokan_impl;
#[cfg(windows)]
mod fuser_facade;
