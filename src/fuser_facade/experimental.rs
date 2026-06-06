use std::ffi::OsStr;
use std::sync::Arc;
use std::time::Duration;

use super::Errno;
use super::FileAttr;
use super::FileHandle;
use super::FileType;
use super::Filesystem;
use super::Generation;
use super::INodeNo;
use super::LockOwner;
use super::OpenFlags;
use super::ReplyAttr;
use super::ReplyData;
use super::ReplyDirectory;
use super::ReplyEntry;
use super::Request;
use super::RequestId;

pub type Result<T> = std::result::Result<T, Errno>;

pub struct RequestContext {
    uid: u32,
    gid: u32,
    pid: u32,
    request_id: RequestId,
}

impl RequestContext {
    fn new(uid: u32, gid: u32, pid: u32, request_id: RequestId) -> Self {
        Self {
            uid,
            gid,
            pid,
            request_id,
        }
    }

    pub fn user_id(&self) -> u32 {
        self.uid
    }

    pub fn group_id(&self) -> u32 {
        self.gid
    }

    pub fn process_id(&self) -> u32 {
        self.pid
    }

    pub fn request_id(&self) -> RequestId {
        self.request_id
    }
}

impl From<&Request> for RequestContext {
    fn from(req: &Request) -> Self {
        Self::new(req.uid(), req.gid(), req.pid(), req.unique())
    }
}

pub struct DirEntListBuilder<'a> {
    entries: &'a mut ReplyDirectory,
}

impl DirEntListBuilder<'_> {
    #[must_use]
    pub fn add<T: AsRef<OsStr>>(
        &mut self, ino: INodeNo, offset: u64, kind: FileType, name: T,
    ) -> bool {
        self.entries.add(ino, offset, kind, name)
    }
}

#[derive(Debug)]
pub struct LookupResponse {
    ttl: Duration,
    attr: FileAttr,
    generation: Generation,
}

impl LookupResponse {
    pub fn new(ttl: Duration, attr: FileAttr, generation: Generation) -> Self {
        Self {
            ttl,
            attr,
            generation,
        }
    }
}

#[derive(Debug)]
pub struct GetAttrResponse {
    ttl: Duration,
    attr: FileAttr,
}

impl GetAttrResponse {
    pub fn new(ttl: Duration, attr: FileAttr) -> Self {
        Self { ttl, attr }
    }
}

#[derive(Debug)]
pub struct TokioAdapter<T: AsyncFilesystem> {
    inner: Arc<T>,
    runtime: tokio::runtime::Runtime,
}

impl<T: AsyncFilesystem> TokioAdapter<T> {
    pub fn new(inner: T) -> Self {
        Self {
            inner: Arc::new(inner),
            runtime: tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .build()
                .expect("tokio runtime"),
        }
    }
}

impl<T: AsyncFilesystem + Send + Sync + 'static> Filesystem for TokioAdapter<T> {
    fn lookup(&self, req: &Request, parent: INodeNo, name: &OsStr, reply: ReplyEntry) {
        let context = RequestContext::from(req);
        let name = name.to_os_string();
        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            match inner.lookup(&context, parent, &name).await {
                Ok(LookupResponse {
                    ttl,
                    attr,
                    generation,
                }) => reply.entry(&ttl, &attr, generation),
                Err(e) => reply.error(e),
            }
        });
    }

    fn getattr(&self, req: &Request, ino: INodeNo, fh: Option<FileHandle>, reply: ReplyAttr) {
        let context = RequestContext::from(req);
        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            match inner.getattr(&context, ino, fh).await {
                Ok(GetAttrResponse { ttl, attr }) => reply.attr(&ttl, &attr),
                Err(e) => reply.error(e),
            }
        });
    }

    fn read(
        &self, req: &Request, ino: INodeNo, fh: FileHandle, offset: u64, size: u32,
        flags: OpenFlags, lock_owner: Option<LockOwner>, reply: ReplyData,
    ) {
        let context = RequestContext::from(req);
        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            let mut buf = Vec::new();
            match inner
                .read(&context, ino, fh, offset, size, flags, lock_owner, &mut buf)
                .await
            {
                Ok(()) => reply.data(&buf),
                Err(e) => reply.error(e),
            }
        });
    }

    fn readdir(
        &self, req: &Request, ino: INodeNo, fh: FileHandle, offset: u64, mut reply: ReplyDirectory,
    ) {
        let context = RequestContext::from(req);
        let inner = self.inner.clone();
        self.runtime.spawn(async move {
            let builder = DirEntListBuilder {
                entries: &mut reply,
            };
            match inner.readdir(&context, ino, fh, offset, builder).await {
                Ok(()) => reply.ok(),
                Err(e) => reply.error(e),
            }
        });
    }
}

#[async_trait::async_trait]
pub trait AsyncFilesystem: Send + Sync + 'static {
    async fn lookup(
        &self, context: &RequestContext, parent: INodeNo, name: &OsStr,
    ) -> Result<LookupResponse>;

    async fn getattr(
        &self, context: &RequestContext, ino: INodeNo, file_handle: Option<FileHandle>,
    ) -> Result<GetAttrResponse>;

    #[allow(clippy::too_many_arguments)]
    async fn read(
        &self, context: &RequestContext, ino: INodeNo, file_handle: FileHandle, offset: u64,
        size: u32, flags: OpenFlags, lock_owner: Option<LockOwner>, data: &mut Vec<u8>,
    ) -> Result<()>;

    async fn readdir(
        &self, context: &RequestContext, ino: INodeNo, file_handle: FileHandle, offset: u64,
        entries: DirEntListBuilder<'_>,
    ) -> Result<()>;
}
