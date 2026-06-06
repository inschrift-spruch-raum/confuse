use std::collections::BTreeMap;
use std::ffi::OsString;
use std::sync::Mutex;
use std::time::SystemTime;

use confuse::{Errno, FileAttr};

use super::inode::{INode, alloc_ino};

#[derive(Clone, Copy)]
pub(crate) enum NewNodeKind {
    File,
    Directory,
}

pub(crate) fn insert_node(
    inodes: &Mutex<BTreeMap<u64, INode>>, children: &Mutex<BTreeMap<(u64, OsString), u64>>,
    parent: u64, name: OsString, mode: u32, kind: NewNodeKind,
) -> Result<FileAttr, Errno> {
    if children
        .lock()
        .unwrap()
        .contains_key(&(parent, name.clone()))
    {
        return Err(Errno::EEXIST);
    }

    let ino = alloc_ino();
    let node = match kind {
        NewNodeKind::File => INode::new_file(ino, parent, name.clone(), mode as u16),
        NewNodeKind::Directory => INode::new_dir(ino, parent, name.clone(), mode as u16),
    };
    let attr = node.to_attr();

    inodes.lock().unwrap().insert(ino, node);
    children.lock().unwrap().insert((parent, name), ino);
    touch_parent(inodes, parent, kind);
    Ok(attr)
}

fn touch_parent(inodes: &Mutex<BTreeMap<u64, INode>>, parent: u64, kind: NewNodeKind) {
    let now = SystemTime::now();
    if let Some(parent_node) = inodes.lock().unwrap().get_mut(&parent) {
        if matches!(kind, NewNodeKind::Directory) {
            parent_node.nlink += 1;
        }
        parent_node.mtime = now;
        parent_node.ctime = now;
    }
}
