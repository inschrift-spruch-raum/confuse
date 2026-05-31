use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::fuser_facade::FsCell;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::request::RequestIds;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct AdapterContext {
    pub(crate) fh: u64,
    pub(crate) flags: u32,
    pub(crate) ino: u64,
    pub(crate) is_dir: bool,
    pub(crate) lock_owner: u64,
    pub(crate) request_ids: RequestIds,
}

pub(crate) struct DokanAdapter<FS: Filesystem> {
    pub(crate) fs: Arc<FsCell<FS>>,
    pub(crate) handles: Arc<Mutex<HashMap<String, AdapterContext>>>,
    pub(crate) dir_offsets: Arc<Mutex<HashMap<String, i64>>>,
    pub(crate) volume_name: String,
    pub(crate) fs_name: String,
    pub(crate) destroyed: Arc<AtomicBool>,
}
