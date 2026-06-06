use std::ffi::OsStr;
use widestring::U16CString;
use winapi::shared::ntstatus::*;
use winapi::um::winnt::*;

use super::{AdapterContext, DokanAdapter};
use crate::dokan_impl::*;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::*;
use crate::fuser_facade::request::{
    RequestIds, request_from_ids, request_from_info, request_ids_from_create_info,
};
use crate::fuser_facade::types::{Errno, INodeNo, TimeOrNow};

// ---------------------------------------------------------------------------
// FileSystemHandler implementation — translates Dokan callbacks to fuser calls
// ---------------------------------------------------------------------------

#[macro_use]
#[path = "handler_methods/dispatch.rs"]
mod handler_dispatch;
#[macro_use]
#[path = "handler_methods/metadata_io.rs"]
mod handler_metadata_io;
#[macro_use]
#[path = "handler_methods/security_locks_streams.rs"]
mod handler_security_locks_streams;
use handler_security_locks_streams::SecurityDescriptorCopy;

impl<'c, 'h: 'c, FS: Filesystem + 'h> dokan::FileSystemHandler<'c, 'h> for DokanAdapter<FS> {
    type Context = AdapterContext;

    handler_delete_move!();
    handler_directory_find!();
    handler_directory_find_pattern!();
    handler_directory_cleanup!();
    handler_lifecycle_create!();
    handler_metadata!();
    handler_io_info!();
    handler_security!();
    handler_locks!();
    handler_streams!();
}
