use std::ffi::OsStr;
use std::io;
use std::io::IoSlice;

use super::reply::ChannelSender;

#[derive(Debug)]
pub struct Notifier(ChannelSender);

impl Notifier {
    pub(crate) fn new(cs: ChannelSender) -> Self {
        Self(cs)
    }

    pub fn poll(&self, _kh: u64) -> io::Result<()> {
        let notif = Notification::new(_kh.to_le_bytes().to_vec());
        self.send(NotifyCode::Poll, &notif)
    }

    pub(crate) fn send_inval(
        &self, code: NotifyCode, notification: &Notification,
    ) -> io::Result<()> {
        match self.send(code, notification) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            x => x,
        }
    }

    pub(crate) fn send(&self, code: NotifyCode, notification: &Notification) -> io::Result<()> {
        notification
            .with_iovec(code, |iov| self.0.send(iov))
            .map_err(Self::too_big_err)?
    }

    pub(crate) fn too_big_err(tfie: std::num::TryFromIntError) -> io::Error {
        io::Error::other(format!("Data too large: {}", tfie))
    }

    pub fn inval_inode(&self, _ino: u64, _off: i64, _len: i64) -> io::Result<()> {
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&_ino.to_le_bytes());
        payload.extend_from_slice(&_off.to_le_bytes());
        payload.extend_from_slice(&_len.to_le_bytes());
        let notif = Notification::new(payload);
        self.send_inval(NotifyCode::InvalInode, &notif)
    }

    pub fn inval_entry(&self, _parent: u64, _name: &OsStr) -> io::Result<()> {
        let name = _name.to_string_lossy();
        let mut payload = Vec::with_capacity(16 + name.len() + 1);
        payload.extend_from_slice(&_parent.to_le_bytes());
        let namelen = u32::try_from(name.len()).map_err(Self::too_big_err)?;
        payload.extend_from_slice(&namelen.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(name.as_bytes());
        payload.push(0);
        let notif = Notification::new(payload);
        self.send_inval(NotifyCode::InvalEntry, &notif)
    }

    pub fn store(&self, _ino: u64, _offset: u64, _data: &[u8]) -> io::Result<()> {
        let mut payload = Vec::with_capacity(16 + _data.len());
        payload.extend_from_slice(&_ino.to_le_bytes());
        payload.extend_from_slice(&_offset.to_le_bytes());
        payload.extend_from_slice(_data);
        let notif = Notification::new(payload);
        self.send_inval(NotifyCode::Store, &notif)
    }

    pub fn delete(&self, _parent: u64, _child: u64, _name: &OsStr) -> io::Result<()> {
        let name = _name.to_string_lossy();
        let mut payload = Vec::with_capacity(24 + name.len() + 1);
        payload.extend_from_slice(&_parent.to_le_bytes());
        payload.extend_from_slice(&_child.to_le_bytes());
        let namelen = u32::try_from(name.len()).map_err(Self::too_big_err)?;
        payload.extend_from_slice(&namelen.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(name.as_bytes());
        payload.push(0);
        let notif = Notification::new(payload);
        self.send_inval(NotifyCode::Delete, &notif)
    }
}

#[derive(Clone, Copy)]
pub(crate) enum NotifyCode {
    Poll,
    InvalEntry,
    InvalInode,
    Store,
    Delete,
}

pub(crate) mod fuse_notify_code {
    pub const FUSE_POLL: u32 = 1;
    pub const FUSE_NOTIFY_INVAL_ENTRY: u32 = 2;
    pub const FUSE_NOTIFY_INVAL_INODE: u32 = 3;
    pub const FUSE_NOTIFY_STORE: u32 = 4;
    pub const FUSE_NOTIFY_DELETE: u32 = 5;
}

pub(crate) struct Notification {
    payload: Vec<u8>,
}

impl Notification {
    fn new(payload: Vec<u8>) -> Self {
        Self { payload }
    }

    fn with_iovec<F>(
        &self, code: NotifyCode, mut f: F,
    ) -> Result<io::Result<()>, std::num::TryFromIntError>
    where
        F: FnMut(&[IoSlice<'_>]) -> io::Result<()>,
    {
        let code_num: u32 = match code {
            NotifyCode::Poll => fuse_notify_code::FUSE_POLL,
            NotifyCode::InvalEntry => fuse_notify_code::FUSE_NOTIFY_INVAL_ENTRY,
            NotifyCode::InvalInode => fuse_notify_code::FUSE_NOTIFY_INVAL_INODE,
            NotifyCode::Store => fuse_notify_code::FUSE_NOTIFY_STORE,
            NotifyCode::Delete => fuse_notify_code::FUSE_NOTIFY_DELETE,
        };
        let payload_len = u32::try_from(self.payload.len())?;
        let total_len = payload_len + 16;
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&total_len.to_le_bytes());
        header[8..12].copy_from_slice(&(code_num as i32).to_le_bytes());
        let iov = [IoSlice::new(&header), IoSlice::new(self.payload.as_slice())];
        Ok(f(&iov))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;

    #[test]
    fn notifier_contract_matches_fuser_shape() {
        let n = Notifier::new(ChannelSender);
        assert!(n.poll(1).is_ok());
        assert!(n.inval_inode(1, 0, 0).is_ok());
        assert!(n.inval_entry(1, OsStr::new("x")).is_ok());
        assert!(n.store(1, 0, &[]).is_ok());
        assert!(n.delete(1, 2, OsStr::new("x")).is_ok());
    }
}
