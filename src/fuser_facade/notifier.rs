use std::ffi::OsStr;
use std::io;
use std::io::IoSlice;
use std::sync::{Arc, Mutex};

use crate::dokan_impl::adapter::PathResolver;

use super::reply::ChannelSender;
use super::types::INodeNo;

/// A handle to a pending `poll()` request.
#[derive(Copy, Clone, Debug)]
pub struct PollHandle(pub u64);

/// A poll handle coupled with a notifier reference.
#[derive(Clone)]
pub struct PollNotifier {
    handle: PollHandle,
    notifier: Notifier,
}

impl PollNotifier {
    #[cfg(test)]
    pub(crate) fn new(cs: ChannelSender, kh: PollHandle) -> Self {
        Self {
            handle: kh,
            notifier: Notifier::new(cs),
        }
    }

    /// Handle associated with this poll notifier.
    pub fn handle(&self) -> PollHandle {
        self.handle
    }

    /// Notify the kernel that the associated file handle is ready to be polled.
    pub fn notify(self) -> io::Result<()> {
        self.notifier.poll(self.handle)
    }
}

impl std::fmt::Debug for PollNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("PollHandle").field(&self.handle).finish()
    }
}

#[derive(Clone, Debug)]
pub struct Notifier {
    sender: ChannelSender,
    resolver: Option<Arc<Mutex<PathResolver>>>,
}

impl Notifier {
    #[cfg(test)]
    pub(crate) fn new(cs: ChannelSender) -> Self {
        Self {
            sender: cs,
            resolver: None,
        }
    }

    pub(crate) fn with_resolver(cs: ChannelSender, resolver: Arc<Mutex<PathResolver>>) -> Self {
        Self {
            sender: cs,
            resolver: Some(resolver),
        }
    }

    pub fn poll(&self, kh: PollHandle) -> io::Result<()> {
        let notif = Notification::new(kh.0.to_le_bytes().to_vec());
        self.send(NotifyCode::Poll, &notif)
    }

    fn send_inval(&self, code: NotifyCode, notification: &Notification) -> io::Result<()> {
        match self.send(code, notification) {
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
            x => x,
        }
    }

    fn send(&self, code: NotifyCode, notification: &Notification) -> io::Result<()> {
        notification
            .with_iovec(code, |iov| self.sender.send(iov))
            .map_err(Self::too_big_err)?
    }

    fn too_big_err(tfie: std::num::TryFromIntError) -> io::Error {
        io::Error::other(format!("Data too large: {}", tfie))
    }

    pub fn inval_inode(&self, _ino: INodeNo, _off: i64, _len: i64) -> io::Result<()> {
        if let Some(resolver) = &self.resolver
            && let Ok(mut resolver) = resolver.lock()
        {
            resolver.invalidate_inode_attr(_ino);
        }
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&_ino.0.to_le_bytes());
        payload.extend_from_slice(&_off.to_le_bytes());
        payload.extend_from_slice(&_len.to_le_bytes());
        let notif = Notification::new(payload);
        self.send_inval(NotifyCode::InvalInode, &notif)
    }

    pub fn inval_entry(&self, _parent: INodeNo, _name: &OsStr) -> io::Result<()> {
        if let Some(resolver) = &self.resolver
            && let Ok(mut resolver) = resolver.lock()
        {
            resolver.invalidate_entry_from_notify(_parent, _name);
        }
        let name = _name.to_string_lossy();
        let mut payload = Vec::with_capacity(16 + name.len() + 1);
        payload.extend_from_slice(&_parent.0.to_le_bytes());
        let namelen = u32::try_from(name.len()).map_err(Self::too_big_err)?;
        payload.extend_from_slice(&namelen.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(name.as_bytes());
        payload.push(0);
        let notif = Notification::new(payload);
        self.send_inval(NotifyCode::InvalEntry, &notif)
    }

    pub fn store(&self, _ino: INodeNo, _offset: u64, _data: &[u8]) -> io::Result<()> {
        let size = u32::try_from(_data.len()).map_err(Self::too_big_err)?;
        let mut payload = Vec::with_capacity(24 + _data.len());
        payload.extend_from_slice(&_ino.0.to_le_bytes());
        payload.extend_from_slice(&_offset.to_le_bytes());
        payload.extend_from_slice(&size.to_le_bytes());
        payload.extend_from_slice(&0u32.to_le_bytes());
        payload.extend_from_slice(_data);
        let notif = Notification::new(payload);
        self.send_inval(NotifyCode::Store, &notif)
    }

    pub fn delete(&self, _parent: INodeNo, _child: INodeNo, _name: &OsStr) -> io::Result<()> {
        if let Some(resolver) = &self.resolver
            && let Ok(mut resolver) = resolver.lock()
        {
            resolver.invalidate_entry_from_notify(_parent, _name);
            resolver.invalidate_inode_attr(_child);
        }
        let name = _name.to_string_lossy();
        let mut payload = Vec::with_capacity(24 + name.len() + 1);
        payload.extend_from_slice(&_parent.0.to_le_bytes());
        payload.extend_from_slice(&_child.0.to_le_bytes());
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
enum NotifyCode {
    Poll,
    InvalEntry,
    InvalInode,
    Store,
    Delete,
}

mod fuse_notify_code {
    pub const FUSE_POLL: u32 = 1;
    pub const FUSE_NOTIFY_INVAL_INODE: u32 = 2;
    pub const FUSE_NOTIFY_INVAL_ENTRY: u32 = 3;
    pub const FUSE_NOTIFY_STORE: u32 = 4;
    pub const FUSE_NOTIFY_DELETE: u32 = 6;
}

struct Notification {
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
        header[4..8].copy_from_slice(&(code_num as i32).to_le_bytes());
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
    use crate::dokan_impl::adapter::PathResolver;
    use crate::fuser_facade::types::Generation;
    use std::ffi::OsStr;
    use std::sync::{Arc, Mutex};

    fn notification_bytes(code: NotifyCode, notification: &Notification) -> Vec<u8> {
        let mut payload = Vec::new();
        notification
            .with_iovec(code, |iov| {
                for slice in iov {
                    payload.extend_from_slice(slice);
                }
                Ok(())
            })
            .expect("notification length")
            .expect("notification bytes");
        payload
    }

    #[test]
    fn notifier_methods_return_ok_with_stub_channel_sender() {
        let n = Notifier::new(ChannelSender);
        assert!(n.poll(PollHandle(1)).is_ok());
        assert!(n.inval_inode(INodeNo(1), 0, 0).is_ok());
        assert!(n.inval_entry(INodeNo(1), OsStr::new("x")).is_ok());
        assert!(n.store(INodeNo(1), 0, &[]).is_ok());
        assert!(n.delete(INodeNo(1), INodeNo(2), OsStr::new("x")).is_ok());
    }

    #[test]
    fn notifier_inval_entry_clears_resolver_negative_cache() {
        let resolver = Arc::new(Mutex::new(PathResolver::default()));
        resolver.lock().expect("resolver lock").remember_negative(
            OsStr::new("\\missing"),
            INodeNo::ROOT,
            Generation(0),
            OsStr::new("missing"),
        );

        let notifier = Notifier::with_resolver(ChannelSender, Arc::clone(&resolver));
        notifier
            .inval_entry(INodeNo::ROOT, OsStr::new("missing"))
            .expect("notify invalidation succeeds");

        let cached = resolver.lock().expect("resolver lock").cached(
            OsStr::new("\\missing"),
            INodeNo::ROOT,
            Generation(0),
        );
        assert!(cached.is_none());
    }

    #[test]
    fn poll_notifier_exposes_handle_and_notifies_with_stub_channel_sender() {
        let notifier = PollNotifier::new(ChannelSender, PollHandle(9));
        assert_eq!(notifier.handle().0, 9);
        assert_eq!(format!("{:?}", notifier), "PollHandle(PollHandle(9))");
        assert!(notifier.notify().is_ok());
    }

    #[test]
    fn notifier_poll_matches_fuser_017_wire_shape() {
        let notif = Notification::new(0x4321_u64.to_le_bytes().to_vec());

        assert_eq!(
            notification_bytes(NotifyCode::Poll, &notif),
            vec![
                0x18, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x21, 0x43, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ]
        );
    }

    #[test]
    fn notifier_inval_inode_matches_fuser_017_wire_shape() {
        let mut payload = Vec::with_capacity(24);
        payload.extend_from_slice(&0x42_u64.to_le_bytes());
        payload.extend_from_slice(&100_i64.to_le_bytes());
        payload.extend_from_slice(&200_i64.to_le_bytes());
        let notif = Notification::new(payload);

        assert_eq!(
            notification_bytes(NotifyCode::InvalInode, &notif),
            vec![
                0x28, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x64, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0xc8, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
            ]
        );
    }

    #[test]
    fn notifier_inval_entry_matches_fuser_017_wire_shape() {
        let mut payload = Vec::with_capacity(20);
        payload.extend_from_slice(&0x42_u64.to_le_bytes());
        payload.extend_from_slice(&3_u32.to_le_bytes());
        payload.extend_from_slice(&0_u32.to_le_bytes());
        payload.extend_from_slice(b"abc");
        payload.push(0);
        let notif = Notification::new(payload);

        assert_eq!(
            notification_bytes(NotifyCode::InvalEntry, &notif),
            vec![
                0x24, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x03, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x61, 0x62, 0x63, 0x00,
            ]
        );
    }

    #[test]
    fn notifier_store_matches_fuser_017_wire_shape() {
        let mut payload = Vec::with_capacity(28);
        payload.extend_from_slice(&0x42_u64.to_le_bytes());
        payload.extend_from_slice(&50_u64.to_le_bytes());
        payload.extend_from_slice(&4_u32.to_le_bytes());
        payload.extend_from_slice(&0_u32.to_le_bytes());
        payload.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
        let notif = Notification::new(payload);

        assert_eq!(
            notification_bytes(NotifyCode::Store, &notif),
            vec![
                0x2c, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x42, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x32, 0x00, 0x00, 0x00,
                0x00, 0x00, 0x00, 0x00, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0xde, 0xad,
                0xbe, 0xef,
            ]
        );
    }

    #[test]
    fn notifier_delete_uses_fuser_017_notify_code() {
        let notif = Notification::new(Vec::new());

        assert_eq!(
            notification_bytes(NotifyCode::Delete, &notif)[4..8],
            [0x06, 0x00, 0x00, 0x00]
        );
    }
}
