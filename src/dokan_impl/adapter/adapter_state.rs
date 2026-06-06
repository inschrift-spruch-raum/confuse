use std::collections::HashMap;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};

use crate::fuser_facade::FsCell;
use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::request::RequestIds;
use crate::fuser_facade::types::{FileHandle, FopenFlags, INodeNo, LockOwner};

use super::PathResolver;

#[derive(Clone, Copy, Debug)]
pub(crate) struct AdapterContext {
    pub(crate) fh: FileHandle,
    pub(crate) flags: FopenFlags,
    pub(crate) ino: INodeNo,
    pub(crate) is_dir: bool,
    pub(crate) lock_owner: Option<LockOwner>,
    pub(crate) request_ids: RequestIds,
}

impl Default for AdapterContext {
    fn default() -> Self {
        Self {
            fh: FileHandle(0),
            flags: FopenFlags::empty(),
            ino: INodeNo(0),
            is_dir: false,
            lock_owner: None,
            request_ids: RequestIds::default(),
        }
    }
}

pub(crate) struct DokanAdapter<FS: Filesystem> {
    pub(crate) fs: Arc<FsCell<FS>>,
    pub(crate) handles: Arc<Mutex<HashMap<String, AdapterContext>>>,
    pub(crate) resolver: Arc<Mutex<PathResolver>>,
    pub(crate) dir_offsets: Arc<Mutex<HashMap<String, i64>>>,
    pub(crate) volume_name: String,
    pub(crate) fs_name: String,
    pub(crate) destroyed: Arc<AtomicBool>,
}
