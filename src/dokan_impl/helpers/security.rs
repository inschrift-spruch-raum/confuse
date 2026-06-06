use winapi::shared::ntstatus::STATUS_NOT_IMPLEMENTED;
use winapi::um::winnt::{
    ACCESS_ALLOWED_ACE_TYPE, ACL_REVISION, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ,
    FILE_GENERIC_WRITE, PSECURITY_DESCRIPTOR, SE_DACL_PRESENT, SE_SELF_RELATIVE,
};

use crate::fuser_facade::filesystem::Filesystem;
use crate::fuser_facade::reply::ReplyAttr;
use crate::fuser_facade::request::Request;
use crate::fuser_facade::types::{FileAttr, FileHandle, INodeNo};

use super::{checked_dokan_len, errno_to_ntstatus, missing_reply_status};

fn push_u16_le(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn push_u32_le(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn unix_sid(kind: u32, id: u32) -> Vec<u8> {
    let mut sid = vec![1, 2, 0, 0, 0, 0, 0, 22];
    push_u32_le(&mut sid, kind);
    push_u32_le(&mut sid, id);
    sid
}

fn world_sid() -> Vec<u8> {
    let mut sid = vec![1, 1, 0, 0, 0, 0, 0, 1];
    push_u32_le(&mut sid, 0);
    sid
}

fn perm_to_file_access(perm: u16, shift: u8) -> u32 {
    let bits = (perm >> shift) & 0o7;
    let mut access = 0;
    if bits & 0o4 != 0 {
        access |= FILE_GENERIC_READ;
    }
    if bits & 0o2 != 0 {
        access |= FILE_GENERIC_WRITE;
    }
    if bits & 0o1 != 0 {
        access |= FILE_GENERIC_EXECUTE;
    }
    access
}

fn push_allow_ace(out: &mut Vec<u8>, access: u32, sid: &[u8]) -> bool {
    if access == 0 {
        return false;
    }
    out.push(ACCESS_ALLOWED_ACE_TYPE);
    out.push(0);
    push_u16_le(out, (8 + sid.len()) as u16);
    push_u32_le(out, access);
    out.extend_from_slice(sid);
    true
}

pub(crate) fn synthesized_security_descriptor(attr: FileAttr) -> Vec<u8> {
    let owner_sid = unix_sid(1, attr.uid);
    let group_sid = unix_sid(2, attr.gid);
    let world_sid = world_sid();

    let mut aces = Vec::new();
    let ace_count = [
        push_allow_ace(&mut aces, perm_to_file_access(attr.perm, 6), &owner_sid),
        push_allow_ace(&mut aces, perm_to_file_access(attr.perm, 3), &group_sid),
        push_allow_ace(&mut aces, perm_to_file_access(attr.perm, 0), &world_sid),
    ]
    .into_iter()
    .filter(|added| *added)
    .count();

    let descriptor_len = 20;
    let owner_offset = descriptor_len;
    let group_offset = owner_offset + owner_sid.len();
    let dacl_offset = group_offset + group_sid.len();
    let acl_len = 8 + aces.len();

    let mut descriptor = Vec::with_capacity(dacl_offset + acl_len);
    descriptor.push(1);
    descriptor.push(0);
    push_u16_le(&mut descriptor, SE_SELF_RELATIVE | SE_DACL_PRESENT);
    push_u32_le(&mut descriptor, owner_offset as u32);
    push_u32_le(&mut descriptor, group_offset as u32);
    push_u32_le(&mut descriptor, 0);
    push_u32_le(&mut descriptor, dacl_offset as u32);
    descriptor.extend_from_slice(&owner_sid);
    descriptor.extend_from_slice(&group_sid);
    descriptor.push(ACL_REVISION);
    descriptor.push(0);
    push_u16_le(&mut descriptor, acl_len as u16);
    push_u16_le(&mut descriptor, ace_count as u16);
    push_u16_le(&mut descriptor, 0);
    descriptor.extend_from_slice(&aces);
    descriptor
}

pub(crate) fn synthesized_security_descriptor_from_fs<FS: Filesystem>(
    fs: &FS, req: &Request, ino: INodeNo, fh: FileHandle,
) -> Result<Vec<u8>, i32> {
    let attr = ReplyAttr::capture();
    fs.getattr(req, ino, Some(fh), attr.duplicate());
    match *attr.status.lock().map_err(|_| STATUS_NOT_IMPLEMENTED)? {
        Some(Ok(attr)) => Ok(synthesized_security_descriptor(attr)),
        Some(Err(err)) => Err(errno_to_ntstatus(err)),
        None => Err(missing_reply_status()),
    }
}

pub(crate) fn copy_security_descriptor(
    data: &[u8], security_descriptor: PSECURITY_DESCRIPTOR, buffer_length: u32,
) -> Result<u32, i32> {
    let needed = checked_dokan_len(data.len())?;
    if buffer_length < needed {
        return Ok(needed);
    }
    if !security_descriptor.is_null() {
        unsafe {
            std::ptr::copy_nonoverlapping(
                data.as_ptr(),
                security_descriptor.cast::<u8>(),
                data.len(),
            );
        }
    }
    Ok(needed)
}

pub(crate) fn xattr_reported_len(data_len: usize, size_hint: Option<u32>) -> Result<u32, i32> {
    size_hint.map_or_else(|| checked_dokan_len(data_len), Ok)
}

pub(crate) fn xattr_needs_data_fetch(data: &[u8], size_hint: Option<u32>) -> Option<u32> {
    if data.is_empty() {
        size_hint.filter(|size| *size > 0)
    } else {
        None
    }
}

pub(crate) fn stream_name_from_xattr(name: &[u8]) -> Option<String> {
    let name = std::str::from_utf8(name).ok()?;
    let stream = name.strip_prefix("user.")?;
    if stream.is_empty() || stream == "dokan.security_descriptor" {
        return None;
    }
    Some(format!(":{stream}:$DATA"))
}

pub(crate) const SECURITY_DESCRIPTOR_XATTR: &str = "user.dokan.security_descriptor";
