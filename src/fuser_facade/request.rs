use std::sync::atomic::{AtomicU64, Ordering};

use std::os::windows::io::AsRawHandle;

use winapi::shared::minwindef::DWORD;
use winapi::um::securitybaseapi::{GetLengthSid, GetTokenInformation};
use winapi::um::winnt::{TOKEN_PRIMARY_GROUP, TOKEN_USER, TokenPrimaryGroup, TokenUser};

use super::types::RequestId;

pub(crate) const INVALID_UID_GID: u32 = u32::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RequestIds {
    pub(crate) uid: u32,
    pub(crate) gid: u32,
    pub(crate) pid: u32,
}

impl RequestIds {
    pub(crate) fn unavailable(pid: u32) -> Self {
        Self {
            uid: INVALID_UID_GID,
            gid: INVALID_UID_GID,
            pid,
        }
    }
}

impl Default for RequestIds {
    fn default() -> Self {
        Self::unavailable(0)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Request {
    unique: u64,
    uid: u32,
    gid: u32,
    pid: u32,
}

impl Request {
    pub(crate) fn from_ids(unique: u64, uid: u32, gid: u32, pid: u32) -> Self {
        Self {
            unique,
            uid,
            gid,
            pid,
        }
    }

    /// Returns the request identifier.
    pub fn unique(&self) -> RequestId {
        RequestId(self.unique)
    }
    pub fn uid(&self) -> u32 {
        self.uid
    }
    pub fn gid(&self) -> u32 {
        self.gid
    }
    pub fn pid(&self) -> u32 {
        self.pid
    }
}

pub(crate) static REQUEST_UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(1);

pub(crate) fn next_request_unique() -> u64 {
    REQUEST_UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed)
}

pub(crate) fn request_from_info<'c, 'h: 'c, FSH: dokan::FileSystemHandler<'c, 'h> + 'h>(
    info: &dokan::OperationInfo<'c, 'h, FSH>,
) -> Request {
    request_from_ids(RequestIds::unavailable(info.pid()))
}

pub(crate) fn request_from_ids(ids: RequestIds) -> Request {
    Request::from_ids(next_request_unique(), ids.uid, ids.gid, ids.pid)
}

pub(crate) fn request_ids_from_create_info<
    'c,
    'h: 'c,
    FSH: dokan::FileSystemHandler<'c, 'h> + 'h,
>(
    info: &dokan::OperationInfo<'c, 'h, FSH>,
) -> Option<RequestIds> {
    if info.file_info().DokanOptions.is_null() {
        return None;
    }
    let token = info.requester_token()?;
    token_request_ids(token.as_raw_handle().cast(), info.pid())
}

fn token_sid_id<T, F>(
    token: winapi::um::winnt::HANDLE, class: winapi::um::winnt::TOKEN_INFORMATION_CLASS, sid: F,
) -> Option<u32>
where
    F: FnOnce(&T) -> winapi::um::winnt::PSID,
{
    let mut len: DWORD = 0;
    unsafe {
        GetTokenInformation(token, class, std::ptr::null_mut(), 0, &mut len);
    }
    if len == 0 {
        return None;
    }

    let mut buffer = vec![0_u8; len as usize];
    let ok =
        unsafe { GetTokenInformation(token, class, buffer.as_mut_ptr().cast(), len, &mut len) };
    if ok == 0 || buffer.len() < std::mem::size_of::<T>() {
        return None;
    }

    let value = unsafe { std::ptr::read_unaligned(buffer.as_ptr().cast::<T>()) };
    sid_to_id(sid(&value))
}

fn token_request_ids(token: winapi::um::winnt::HANDLE, pid: u32) -> Option<RequestIds> {
    Some(RequestIds {
        uid: token_sid_id(token, TokenUser, |value: &TOKEN_USER| value.User.Sid)?,
        gid: token_sid_id(token, TokenPrimaryGroup, |value: &TOKEN_PRIMARY_GROUP| {
            value.PrimaryGroup
        })?,
        pid,
    })
}

fn sid_to_id(sid: winapi::um::winnt::PSID) -> Option<u32> {
    if sid.is_null() {
        return None;
    }
    let len = unsafe { GetLengthSid(sid) };
    if len == 0 {
        return None;
    }
    let sid_bytes = unsafe { std::slice::from_raw_parts(sid.cast::<u8>(), len as usize) };
    Some(stable_u32(sid_bytes.iter().copied()))
}

fn stable_u32(value: impl IntoIterator<Item = u8>) -> u32 {
    let mut hash = 0x811c_9dc5_u32;
    for byte in value {
        hash ^= u32::from(byte);
        hash = hash.wrapping_mul(0x0100_0193);
    }
    hash
}

#[cfg(test)]
pub(crate) fn request_kernel() -> Request {
    Request::from_ids(0, 0, 0, 0)
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn request_unique_is_exposed_and_stable() {
        let req = Request::from_ids(42, 1000, 1001, 9999);
        assert_eq!(req.unique(), RequestId(42));
        assert_eq!(req.uid(), 1000);
        assert_eq!(req.gid(), 1001);
        assert_eq!(req.pid(), 9999);
    }

    #[test]
    fn request_unique_counter_progresses() {
        let a = next_request_unique();
        let b = next_request_unique();
        assert!(b > a);
    }

    #[test]
    fn request_unique_counter_is_unique_across_threads() {
        let seen = collect_unique_ids_across_threads(8, 64);

        assert_eq!(seen.len(), 8 * 64);
    }

    fn collect_unique_ids_across_threads(
        thread_count: usize, ids_per_thread: usize,
    ) -> HashSet<u64> {
        let threads: Vec<_> = (0..thread_count)
            .map(|_| std::thread::spawn(move || collect_unique_ids(ids_per_thread)))
            .collect();

        threads
            .into_iter()
            .flat_map(|thread| thread.join().expect("thread should complete"))
            .collect()
    }

    fn collect_unique_ids(count: usize) -> Vec<u64> {
        (0..count).map(|_| next_request_unique()).collect()
    }

    #[test]
    fn request_accessors_preserve_values() {
        let req = Request::from_ids(42, 1001, 1002, 12345);
        assert_eq!(req.unique(), RequestId(42));
        assert_eq!(req.uid(), 1001);
        assert_eq!(req.gid(), 1002);
        assert_eq!(req.pid(), 12345);
    }

    #[test]
    fn unavailable_request_ids_use_fuse_invalid_uid_gid() {
        let ids = RequestIds::unavailable(1234);
        let req = request_from_ids(ids);
        assert_eq!(req.uid(), INVALID_UID_GID);
        assert_eq!(req.gid(), INVALID_UID_GID);
        assert_eq!(req.pid(), 1234);
    }
}
