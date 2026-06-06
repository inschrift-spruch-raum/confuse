use super::*;

#[test]
fn stream_name_mapping_skips_security_descriptor_xattr() {
    assert_eq!(
        stream_name_from_xattr(b"user.zone"),
        Some(":zone:$DATA".to_string())
    );
    assert_eq!(
        stream_name_from_xattr(b"user.dokan.security_descriptor"),
        None
    );
    assert_eq!(stream_name_from_xattr(b"trusted.zone"), None);
}

#[derive(Default)]
struct SecurityAndStreamFs {
    lookup_called: AtomicUsize,
    setxattr_called: AtomicUsize,
    listxattr_called: AtomicUsize,
    security: Vec<u8>,
    security_missing: bool,
    perm: Option<u16>,
    stream_names: Vec<u8>,
    stream_size: u32,
}

impl Filesystem for SecurityAndStreamFs {
    fn lookup(&self, _req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        self.lookup_called.fetch_add(1, Ordering::SeqCst);
        match (parent, name.to_string_lossy().as_ref()) {
            (INodeNo::ROOT, "file") => {
                reply.entry(&Duration::from_secs(60), &test_file_attr(2), Generation(0));
            }
            _ => reply.error(Errno::ENOENT),
        }
    }

    fn getattr(&self, _req: &Request, ino: INodeNo, _fh: Option<FileHandle>, reply: ReplyAttr) {
        let mut attr = test_file_attr(ino.0);
        attr.size = 123;
        attr.perm = self.perm.unwrap_or(attr.perm);
        reply.attr(&Duration::from_secs(1), &attr);
    }

    fn getxattr(&self, _req: &Request, _ino: INodeNo, name: &OsStr, size: u32, reply: ReplyXattr) {
        if name == OsStr::new(SECURITY_DESCRIPTOR_XATTR) {
            if self.security_missing {
                reply.error(Errno::NO_XATTR);
            } else if size == 0 {
                reply.size(self.security.len() as u32);
            } else if size < self.security.len() as u32 {
                reply.error(Errno::ERANGE);
            } else {
                reply.data(&self.security);
            }
        } else if size == 0 {
            reply.size(self.stream_size);
        } else {
            reply.data(&vec![0; self.stream_size as usize]);
        }
    }

    fn listxattr(&self, _req: &Request, _ino: INodeNo, size: u32, reply: ReplyXattr) {
        self.listxattr_called.fetch_add(1, Ordering::SeqCst);
        if size == 0 {
            reply.size(self.stream_names.len() as u32);
        } else {
            reply.data(&self.stream_names);
        }
    }

    fn setxattr(
        &self, _req: &Request, _ino: INodeNo, _name: &OsStr, _value: &[u8], _flags: i32,
        _position: u32, reply: ReplyEmpty,
    ) {
        self.setxattr_called.fetch_add(1, Ordering::SeqCst);
        reply.ok();
    }
}

#[test]
fn file_security_missing_descriptor_xattr_synthesizes_descriptor_from_attr() {
    let adapter = test_adapter(SecurityAndStreamFs {
        security_missing: true,
        perm: Some(0o444),
        ..Default::default()
    });
    let path = U16CString::from_str("\\file").expect("path");
    let ctx = AdapterContext {
        ino: INodeNo(2),
        fh: FileHandle(3),
        ..Default::default()
    };
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);

    let required = dokan::FileSystemHandler::get_file_security(
        &adapter,
        path.as_ucstr(),
        0,
        std::ptr::null_mut(),
        0,
        &info,
        &ctx,
    )
    .expect("missing security descriptor xattr falls back to synthesized descriptor length");
    let mut out = vec![0_u8; required as usize];

    let copied = dokan::FileSystemHandler::get_file_security(
        &adapter,
        path.as_ucstr(),
        0,
        out.as_mut_ptr().cast(),
        out.len() as u32,
        &info,
        &ctx,
    )
    .expect("missing security descriptor xattr copies synthesized descriptor");

    assert_eq!(copied, required);
    assert_eq!(out.len(), required as usize);
    assert_eq!(out[0], 1);
    assert_eq!(
        u16::from_le_bytes([out[2], out[3]]) & SE_SELF_RELATIVE,
        SE_SELF_RELATIVE
    );
    assert_eq!(
        u16::from_le_bytes([out[2], out[3]]) & SE_DACL_PRESENT,
        SE_DACL_PRESENT
    );
    let dacl_offset = u32::from_le_bytes([out[16], out[17], out[18], out[19]]) as usize;
    let ace_offset = dacl_offset + 8;
    let mask = u32::from_le_bytes([
        out[ace_offset + 4],
        out[ace_offset + 5],
        out[ace_offset + 6],
        out[ace_offset + 7],
    ]);
    assert_ne!(mask & FILE_GENERIC_READ, 0);
    assert_eq!(mask & FILE_WRITE_DATA, 0);
}

#[test]
fn file_security_reports_size_hint_and_copies_descriptor() {
    let adapter = test_adapter(SecurityAndStreamFs {
        security: vec![1, 2, 3, 4],
        ..Default::default()
    });
    let path = U16CString::from_str("\\file").expect("path");
    let ctx = AdapterContext {
        ino: INodeNo(2),
        fh: FileHandle(3),
        ..Default::default()
    };
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);

    let required = dokan::FileSystemHandler::get_file_security(
        &adapter,
        path.as_ucstr(),
        0,
        std::ptr::null_mut(),
        0,
        &info,
        &ctx,
    )
    .expect("size probe reports descriptor length");
    assert_eq!(required, 4);

    let mut out = [0_u8; 4];
    let copied = dokan::FileSystemHandler::get_file_security(
        &adapter,
        path.as_ucstr(),
        0,
        out.as_mut_ptr().cast(),
        out.len() as u32,
        &info,
        &ctx,
    )
    .expect("descriptor copy succeeds");
    assert_eq!(copied, 4);
    assert_eq!(out, [1, 2, 3, 4]);
}

#[test]
fn contextless_file_security_resolves_through_path_resolver() {
    let adapter = test_adapter(SecurityAndStreamFs {
        security: vec![1, 2, 3, 4],
        ..Default::default()
    });
    let path = U16CString::from_str("\\file").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();

    let required = dokan::FileSystemHandler::get_file_security(
        &adapter,
        path.as_ucstr(),
        0,
        std::ptr::null_mut(),
        0,
        &info,
        &ctx,
    )
    .expect("contextless security lookup resolves by path");

    assert_eq!(required, 4);
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
}

#[test]
fn contextless_set_file_security_resolves_through_path_resolver() {
    let adapter = test_adapter(SecurityAndStreamFs::default());
    let path = U16CString::from_str("\\file").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();
    let mut descriptor = [1_u8, 2, 3, 4];

    dokan::FileSystemHandler::set_file_security(
        &adapter,
        path.as_ucstr(),
        0,
        descriptor.as_mut_ptr().cast(),
        descriptor.len() as u32,
        &info,
        &ctx,
    )
    .expect("contextless set security resolves by path");

    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.setxattr_called.load(Ordering::SeqCst), 1);
}

#[test]
fn file_security_short_buffer_reports_required_length_without_copy() {
    let adapter = test_adapter(SecurityAndStreamFs {
        security: vec![1, 2, 3, 4],
        ..Default::default()
    });
    let path = U16CString::from_str("\\file").expect("path");
    let ctx = AdapterContext {
        ino: INodeNo(2),
        fh: FileHandle(3),
        ..Default::default()
    };
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let mut out = [9_u8; 2];

    let required = dokan::FileSystemHandler::get_file_security(
        &adapter,
        path.as_ucstr(),
        0,
        out.as_mut_ptr().cast(),
        out.len() as u32,
        &info,
        &ctx,
    )
    .expect("short buffer reports descriptor length");

    assert_eq!(required, 4);
    assert_eq!(out, [9, 9]);
}

#[test]
fn find_streams_fetches_xattr_names_after_size_probe() {
    let adapter = test_adapter(SecurityAndStreamFs {
        stream_names: b"user.zone\0user.dokan.security_descriptor\0".to_vec(),
        stream_size: 7,
        ..Default::default()
    });
    let path = U16CString::from_str("\\file").expect("path");
    let ctx = AdapterContext {
        ino: INodeNo(2),
        fh: FileHandle(3),
        ..Default::default()
    };
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let mut streams = Vec::new();

    dokan::FileSystemHandler::find_streams(
        &adapter,
        path.as_ucstr(),
        |stream| {
            streams.push((stream.name.to_string_lossy(), stream.size));
            Ok(())
        },
        &info,
        &ctx,
    )
    .expect("stream listing succeeds");

    assert_eq!(
        streams,
        vec![("::$DATA".to_string(), 123), (":zone:$DATA".to_string(), 7)]
    );
}

#[test]
fn contextless_find_streams_resolves_through_path_resolver() {
    let adapter = test_adapter(SecurityAndStreamFs {
        stream_names: b"user.zone\0".to_vec(),
        stream_size: 7,
        ..Default::default()
    });
    let path = U16CString::from_str("\\file").expect("path");
    let mut raw_info: dokan_sys::DOKAN_FILE_INFO = unsafe { std::mem::zeroed() };
    let info = dokan::OperationInfo::new(&mut raw_info);
    let ctx = AdapterContext::default();
    let mut streams = Vec::new();

    dokan::FileSystemHandler::find_streams(
        &adapter,
        path.as_ucstr(),
        |stream| {
            streams.push((stream.name.to_string_lossy(), stream.size));
            Ok(())
        },
        &info,
        &ctx,
    )
    .expect("contextless stream listing resolves by path");

    assert_eq!(
        streams,
        vec![("::$DATA".to_string(), 123), (":zone:$DATA".to_string(), 7)]
    );
    let fs = adapter.fs.lock().expect("fs lock");
    assert_eq!(fs.lookup_called.load(Ordering::SeqCst), 1);
    assert_eq!(fs.listxattr_called.load(Ordering::SeqCst), 2);
}
