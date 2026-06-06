use std::io::IoSlice;

use super::super::types::INodeNo;

#[derive(Debug)]
pub struct BackingId(BackingIdToken);

#[derive(Debug)]
struct BackingIdToken;

impl BackingId {
    #[cfg(test)]
    pub(crate) fn unsupported_for_windows() -> Self {
        Self(BackingIdToken)
    }
}

/// One public batched forget entry.
///
/// Matches upstream fuser 0.17.0: callers receive `ForgetOne` values through
/// `Filesystem::batch_forget` and inspect them with accessors.
#[allow(non_camel_case_types)]
#[derive(Debug)]
#[repr(C)]
struct fuse_forget_one {
    nodeid: u64,
    nlookup: u64,
}

#[derive(Debug)]
#[repr(transparent)]
pub struct ForgetOne {
    forget_one: fuse_forget_one,
}

impl ForgetOne {
    pub fn nodeid(&self) -> INodeNo {
        INodeNo(self.forget_one.nodeid)
    }

    pub fn nlookup(&self) -> u64 {
        self.forget_one.nlookup
    }

    #[cfg(test)]
    pub(crate) fn new(nodeid: u64, nlookup: u64) -> Self {
        Self {
            forget_one: fuse_forget_one { nodeid, nlookup },
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ChannelSender;

impl ChannelSender {
    pub(crate) fn send(&self, _data: &[IoSlice<'_>]) -> std::io::Result<()> {
        Ok(())
    }
}
