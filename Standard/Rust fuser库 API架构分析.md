# Rust fuser 库 API 架构分析

> 范围说明：本文只基于外部公开信息整理，包括 docs.rs、crates.io、GitHub README 与公开示例源码；未使用任何本地私有代码或非公开资料。版本口径以 docs.rs/crates.io 当前 latest `fuser 0.17.0` 为主，README 中仍出现的旧依赖示例版本仅作为历史/文档现状提示。

## 1. 项目定位

`fuser` 是 Rust 的 FUSE（Filesystem in Userspace）用户态库，描述为 “Filesystem in Userspace (FUSE) for Rust”。它的核心目标不是简单绑定 libfuse，而是以 Rust 方式重写 FUSE low-level userspace library：内核驱动仍由 FUSE 项目提供，开发者实现用户态文件系统逻辑，`fuser` 位于二者之间，负责建立会话、收发内核请求、分派到 Rust trait 方法，并把 reply 写回内核。

官方 README 对运行结构的描述可抽象为三层：

1. **FUSE kernel driver**：注册文件系统并把 VFS 操作转发到用户态进程。
2. **userspace library 层**：传统上由 libfuse 承担；`fuser` 试图用 Rust 替代这层的大部分能力。
3. **用户实现层**：开发者实现具体文件系统行为。

README 还说明：除了挂载/卸载阶段可能依赖 libfuse 外，其余逻辑在 Rust 中运行；在 Linux 上这些 libfuse 调用可通过不启用 `libfuse` feature 移除。crates.io latest `0.17.0` 的默认 feature 为空，`libfuse2`/`libfuse3` 通过 `libfuse` feature 组合提供。

## 2. 总体架构

从公开 API 看，`fuser` 的架构可以按职责分为五层：

```text
应用代码
  └─ 实现 fuser::Filesystem trait
      └─ fuser 分派层：Request + Filesystem 方法 + Reply* 类型
          └─ Session / BackgroundSession / mount2 / spawn_mount2
              └─ FUSE 设备 fd、mount/unmount、内核协议读写
                  └─ FUSE kernel driver / OS VFS
```

### 2.1 应用入口层

开发者通常定义一个文件系统结构体，例如公开 `examples/hello.rs` 中的 `HelloFS`，然后实现 `fuser::Filesystem`。README 的用法也明确写道：创建新文件系统需要实现 `fuser::Filesystem`，并参考 docs.rs 或 `examples`。

### 2.2 trait 分派层

`Filesystem` trait 是 API 核心。docs.rs 显示该 trait 约束为：

```rust
pub trait Filesystem: Send + Sync + 'static { ... }
```

它暴露 41 个默认方法，文档说明这些方法对应 libfuse 的 `fuse_lowlevel_ops`。默认实现可以得到“可挂载但基本不做事”的文件系统，因此使用者可以只覆盖需要支持的操作。

### 2.3 请求上下文层

多数 `Filesystem` 方法第一个参数是 `&Request`。`Request` 是 FUSE request parameters，公开方法包括：

- `unique()`：请求唯一 ID；
- `uid()`：发起请求的用户 ID；
- `gid()`：发起请求的组 ID；
- `pid()`：发起请求的进程 ID。

因此 `Request` 承担的是调用者身份与请求追踪上下文，而不是业务数据载体。业务数据通过 inode、文件句柄、offset、flags、name、path、data slice 等参数传入。

### 2.4 reply 响应层

`fuser` 使用一组一次性 reply 对象表达不同 FUSE 响应类型，例如：

- `ReplyEntry`：目录项查询结果，常用于 `lookup`；
- `ReplyAttr`：属性结果，常用于 `getattr`/`setattr`；
- `ReplyData`：字节数据，常用于 `read`/`readlink`；
- `ReplyDirectory` / `ReplyDirectoryPlus`：目录枚举；
- `ReplyOpen` / `ReplyCreate`：打开/创建文件；
- `ReplyWrite`：写入字节数；
- `ReplyEmpty`：无数据成功或错误；
- `ReplyStatfs`、`ReplyXattr`、`ReplyLock`、`ReplyIoctl`、`ReplyPoll`、`ReplyBmap`、`ReplyLseek` 等。

这种设计把“操作处理”和“响应发送”显式绑定：每个回调拿到对应的 reply 类型，然后调用成功方法或 `error(...)`。例如 `ReplyEntry::entry(ttl, attr, generation)` 返回目录项，`ReplyAttr::attr(ttl, attr)` 返回属性，`ReplyDirectory::add(...)` 逐项填充目录缓冲区并用 `ok()` 完成，`ReplyOpen::opened(fh, flags)` 返回文件句柄和打开标志。

## 3. 核心数据模型

### 3.1 inode 与文件句柄

`fuser` 使用 newtype 表示核心标识：

- `INodeNo`：inode number；
- `FileHandle`：文件句柄；
- `Generation`：generation number；
- `RequestId`：内核请求唯一 ID；
- `LockOwner`：锁拥有者。

这比裸 `u64` 更贴近 Rust 类型接口，也能减少参数位置误用。公开 `hello.rs` 示例用 `INodeNo::ROOT` 表示根目录，用 `INodeNo(2)` 表示 `hello.txt`。

### 3.2 FileAttr

`FileAttr` 是文件属性结构，字段包括：

- 标识与大小：`ino`、`size`、`blocks`；
- 时间：`atime`、`mtime`、`ctime`、`crtime`；
- 类型和权限：`kind: FileType`、`perm`、`nlink`；
- 所有者：`uid`、`gid`；
- 设备/块信息：`rdev`、`blksize`；
- 平台 flags：`flags`，文档标注 macOS only。

`FileType` 表达目录、普通文件等类型。公开示例中，目录属性使用 `FileType::Directory`、权限 `0o755`、链接数 2；普通文件使用 `FileType::RegularFile`、权限 `0o644`。

### 3.3 错误模型

`Errno` 表示返回给调用方的错误码。公开示例中，未找到 inode 或 name 时调用 `reply.error(Errno::ENOENT)`。这说明 API 的错误表达贴近 POSIX/FUSE：回调本身一般不返回 `Result`，而是通过 reply 对象发送成功或 errno。

## 4. Filesystem trait 的操作分组

docs.rs 列出的 41 个方法可按文件系统语义分组：

### 4.1 生命周期与协商

- `init(&mut self, req, config) -> Result<()>`：第一个被调用的方法，可通过 `KernelConfig` 配置内核连接；
- `destroy(&mut self)`：文件系统退出时清理。

### 4.2 名称解析与 inode 生命周期

- `lookup(parent, name, ReplyEntry)`：按父目录 inode 和名称查找目录项；
- `forget(ino, nlookup)`：内核忘记 inode 引用；
- `batch_forget(nodes)`：批量 forget，默认回退到逐个 `forget`。

文档建议：如果文件系统实现 inode 生命周期，每次 lookup 获取一个引用，forget 时减少 `nlookup` 个引用；卸载时不保证所有引用都会收到 forget。

### 4.3 属性与元数据

- `getattr(ino, fh, ReplyAttr)`；
- `setattr(...)`；
- `statfs(ino, ReplyStatfs)`；
- `access(ino, mask, ReplyEmpty)`。

`access` 文档指出，如果使用 `default_permissions` 挂载选项，则该方法不会被调用，因为权限检查交由内核执行。

### 4.4 目录项与链接修改

- `mknod`、`mkdir`；
- `unlink`、`rmdir`；
- `symlink`、`readlink`；
- `rename`、`link`。

这些方法围绕父 inode、名称、目标路径、flags 和 `ReplyEntry`/`ReplyEmpty` 建模。

### 4.5 文件 I/O

- `open(ino, flags, ReplyOpen)`；
- `read(ino, fh, offset, size, flags, lock_owner, ReplyData)`；
- `write(ino, fh, offset, data, write_flags, flags, lock_owner, ReplyWrite)`；
- `flush`、`release`、`fsync`；
- `create(...)`：创建并打开文件；
- `fallocate`、`lseek`、`copy_file_range`。

`open` 文档说明文件系统可以把任意文件句柄（指针、索引等）保存在 `fh` 中，后续 `read/write/flush/release/fsync` 使用。也可以实现无状态 I/O，不保存句柄。

`read` 文档强调：除 EOF 或错误外，应返回请求的精确字节数，否则剩余数据可能被零填充；`direct_io` 模式例外。`write` 类似：除错误外应返回请求写入字节数。

### 4.6 目录 I/O

- `opendir`；
- `readdir`；
- `readdirplus`；
- `releasedir`；
- `fsyncdir`。

`ReplyDirectory::add(ino, offset, kind, name)` 返回 `bool` 表示缓冲区是否已满；调用者需要在满时停止追加，并最终 `ok()`。公开 `hello.rs` 示例使用 `offset` 跳过已读条目，并把 `i + 1` 作为下一次读取的 offset。

### 4.7 扩展属性、锁与特殊操作

- xattr：`setxattr`、`getxattr`、`listxattr`、`removexattr`；
- 锁：`getlk`、`setlk`；
- 其他：`bmap`、`ioctl`、`poll`。

这些 API 说明 `fuser` 不只覆盖最小文件读写，还试图覆盖 libfuse low-level interface 的大量系统级能力。

## 5. 会话与挂载 API

### 5.1 Config 与 MountOption

`Config` 是 session 配置结构，标记为 `#[non_exhaustive]`，字段包括：

- `mount_options: Vec<MountOption>`：挂载选项；
- `acl: SessionACL`：谁能访问文件系统；
- `n_threads: Option<usize>`：事件循环线程数，未指定时为单线程；
- `clone_fd: bool`：使用 `FUSE_DEV_IOC_CLONE` 给每个 worker thread 独立 fd，Linux 4.5+ 可提升多线程请求处理效率。

`MountOption` 包括 `FSName`、`Subtype`、`CUSTOM`、`AutoUnmount`、`DefaultPermissions`、`RO/RW`、`Exec/NoExec`、`Atime/NoAtime`、`Sync/Async` 等。`CUSTOM(String)` 为未枚举的 mount option 留出口。

### 5.2 前台挂载：mount2

`mount2(filesystem, mountpoint, options) -> Result<()>` 挂载文件系统并阻塞，直到文件系统卸载。文档说明如果选项错误、FUSE device 无法挂载，或 session 结束时有最终错误，会返回 error。

### 5.3 后台挂载：spawn_mount2

`spawn_mount2(filesystem, mountpoint, options) -> Result<BackgroundSession>` 挂载后启动后台线程处理文件系统操作，并立即返回。返回的 `BackgroundSession` 必须被保存；如果被 drop，文件系统会卸载。

### 5.4 Session 低层控制

`Session<FS>` 是会话数据结构，提供：

- `Session::new(filesystem, mountpoint, options)`：创建并挂载；
- `Session::from_fd(filesystem, fd, acl, config)`：包装已有 `/dev/fuse` fd，本身不执行挂载；
- `spawn()`：后台运行 session loop；
- `unmount()`：卸载；
- `unmount_callable()`：返回线程安全卸载对象；
- `notifier()`：返回可向 kernel 发送通知的对象。

因此 API 同时提供简化入口（`mount2`/`spawn_mount2`）和较底层的 session 控制能力。

## 6. 典型调用链

基于 docs.rs 与公开 `hello.rs` 示例，可以把一次 `cat hello.txt` 的路径概括为：

1. 应用构造文件系统结构体并调用 mount API。
2. 内核收到路径访问后向 FUSE 发送 lookup 请求。
3. `fuser` 解析请求，调用 `Filesystem::lookup(&self, &Request, parent, name, ReplyEntry)`。
4. 实现方根据 `(parent, name)` 查表，调用 `reply.entry(&ttl, &FileAttr, Generation(...))` 或 `reply.error(Errno::ENOENT)`。
5. 内核随后请求属性或打开文件，触发 `getattr` / `open`。
6. 读取时触发 `read(ino, fh, offset, size, ..., ReplyData)`。
7. 实现方按 offset 切片并调用 `reply.data(bytes)`，错误时调用 `reply.error(errno)`。

目录列表则是 `readdir`：实现方用 `ReplyDirectory::add` 填充 `.`、`..` 与子项；如果 buffer 满则停止，并调用 `ok()`。

## 7. 并发与状态管理含义

`Filesystem: Send + Sync + 'static` 暗示 `fuser` 可以在线程间共享文件系统对象。`Config::n_threads` 与 `clone_fd` 进一步说明 session loop 支持多线程处理。公开 `hello.rs` 示例为了统计每线程 read 次数，使用 `AtomicU64`、`AtomicUsize` 和 thread-local `THREAD_INDEX`，也从用法侧印证：实现者需要自行保证内部状态的并发安全。

因此架构上，`fuser` 管分派和协议，业务对象的缓存、inode 表、句柄表、锁、目录快照一致性等由实现方负责。常见实现会需要：

- inode -> 元数据/内容的索引；
- path/name -> inode 的目录项索引；
- open file handle -> 后端资源或游标；
- TTL 策略，控制内核缓存属性和目录项的时间；
- 并发控制，通常用锁、原子变量或内部并发数据结构。

公开 `examples/simple.rs` 展示了比 hello 更完整的状态管理形态：维护 inode 分配、目录/文件元数据、文件句柄、目录句柄、访问检查、扩展属性和创建/修改操作。这类示例说明，`fuser` 并不替应用持有完整 VFS 状态；它提供协议分派与类型化回包，状态模型仍由具体文件系统设计。

## 8. 平台与构建约束

README 说明：

- Linux：通常安装 `fuse` 或 `fuse3`，该 crate 兼容两者；构建需要 FUSE headers/libraries 与 `pkg-config`。
- macOS：README 标注 macOS 支持为 “untested”，依赖 macFUSE；Apple Silicon 需要启用第三方 kernel extensions。
- FreeBSD：安装 `fusefs-libs` 与 `pkgconf`。
- README 的兼容性部分称项目在 Linux 开发和测试，并在 Linux 与 FreeBSD 上用 stable Rust 测试。

crates.io `0.17.0` 元数据显示：license 为 MIT，edition 为 2024，Rust version 为 1.85，latest/max stable version 为 0.17.0。

## 9. Feature flags 与演进面

`0.17.0` 公开 feature flags 主要包括：

- ABI 相关：`abi-7-20` 到 `abi-7-40`；
- `libfuse`、`libfuse2`、`libfuse3`；
- `macfuse-4-compat`、`macos-no-mount`；
- `serializable` -> `serde`；
- `experimental` -> `async-trait` + `tokio`；
- `default` 不启用额外 feature。

README 提到项目源自 `fuse` crate fork，目标之一是继续开发并增加 ABI 7.19 之后的特性；当前 feature flags 也体现了它围绕 FUSE ABI 版本演进的设计。

## 10. API 设计特点

### 10.1 低层 FUSE 映射，而非高级虚拟文件系统框架

`Filesystem` 方法与 `fuse_lowlevel_ops` 对齐，参数直接暴露 inode、fh、offset、flags、errno、xattr、lock 等概念。这给实现者较高控制力，但也要求理解 FUSE/VFS 语义。

### 10.2 类型化 reply 约束响应形态

每个操作拿到专用 reply 类型，成功路径方法与该操作语义匹配，错误统一通过 errno。这样避免了用一个泛型返回值承载所有操作响应，也更接近内核协议的单次请求/单次响应模型。

### 10.3 默认方法降低最小实现成本

所有主要操作都有默认实现；最小文件系统只需覆盖必要方法。公开 hello 示例只覆盖 `lookup`、`getattr`、`read`、`readdir` 即可提供只读目录与文件。

### 10.4 缓存由 TTL 和 mount option 共同影响

`ReplyEntry::entry`、`ReplyAttr::attr` 都接收 TTL；示例中动态文件 `stats-per-thread` 使用 `Duration::ZERO`，注释说明否则旧 size 会被缓存。这表明实现者必须显式设计缓存语义。

### 10.5 session API 分层清晰

简单使用者调用 `mount2` 或 `spawn_mount2`；高级使用者可直接操作 `Session`、已有 fd、notifier、unmounter。这种分层兼顾易用入口与系统级控制。

## 11. 实现一个 fuser 文件系统的最小结构

公开资料可归纳出典型 skeleton：

```rust
struct MyFs {
    // inode table, file data, handle table, etc.
}

impl fuser::Filesystem for MyFs {
    fn lookup(&self, req: &fuser::Request, parent: fuser::INodeNo, name: &std::ffi::OsStr, reply: fuser::ReplyEntry) {
        // find child under parent, then reply.entry(...) or reply.error(...)
    }

    fn getattr(&self, req: &fuser::Request, ino: fuser::INodeNo, fh: Option<fuser::FileHandle>, reply: fuser::ReplyAttr) {
        // return FileAttr or errno
    }

    fn readdir(&self, req: &fuser::Request, ino: fuser::INodeNo, fh: fuser::FileHandle, offset: u64, reply: fuser::ReplyDirectory) {
        // fill directory entries and call ok()
    }

    fn read(&self, req: &fuser::Request, ino: fuser::INodeNo, fh: fuser::FileHandle, offset: u64, size: u32, flags: fuser::OpenFlags, lock_owner: Option<fuser::LockOwner>, reply: fuser::ReplyData) {
        // return bytes or errno
    }
}
```

挂载侧则使用 `Config` 与 mount option：

```rust
let mut cfg = fuser::Config::default();
cfg.mount_options.push(fuser::MountOption::RO);
cfg.mount_options.push(fuser::MountOption::FSName("myfs".to_string()));
fuser::mount2(MyFs { /* ... */ }, mountpoint, &cfg)?;
```

实际项目需要根据是否阻塞主线程选择 `mount2` 或 `spawn_mount2`，并根据并发模型设置 `n_threads` 与内部同步结构。

## 12. 适用场景与注意点

适合：

- 用 Rust 快速实现用户态文件系统；
- 原型化只读/虚拟/聚合文件系统；
- 对接对象存储、数据库、远程 API 或内存数据结构为文件系统视图；
- 需要较低层 FUSE 控制能力的系统工具。

注意：

- API 贴近 low-level FUSE，必须正确处理 inode 生命周期、offset、TTL、errno、权限和并发；
- 默认实现“可挂载但不做事”，不等于提供 POSIX 完整语义；
- `DefaultPermissions` 会改变权限检查责任边界；
- 多线程 session 下，`Filesystem` 内部状态必须线程安全；
- 动态属性如果 TTL 过长，会被内核缓存导致 size/metadata 过期；
- macOS 支持在 README 中标注为 untested，平台差异需要单独验证。

## 13. 公开来源

- docs.rs crate page: https://docs.rs/fuser/latest/fuser/
- docs.rs `Filesystem` trait: https://docs.rs/fuser/latest/fuser/trait.Filesystem.html
- docs.rs `FileAttr`: https://docs.rs/fuser/latest/fuser/struct.FileAttr.html
- docs.rs `Request`: https://docs.rs/fuser/latest/fuser/struct.Request.html
- docs.rs `Config`: https://docs.rs/fuser/latest/fuser/struct.Config.html
- docs.rs `MountOption`: https://docs.rs/fuser/latest/fuser/enum.MountOption.html
- docs.rs `mount2`: https://docs.rs/fuser/latest/fuser/fn.mount2.html
- docs.rs `spawn_mount2`: https://docs.rs/fuser/latest/fuser/fn.spawn_mount2.html
- docs.rs `Session`: https://docs.rs/fuser/latest/fuser/struct.Session.html
- docs.rs feature flags: https://docs.rs/crate/fuser/latest/features
- crates.io API metadata: https://crates.io/api/v1/crates/fuser
- GitHub README: https://github.com/cberner/fuser/blob/master/README.md
- GitHub hello example: https://raw.githubusercontent.com/cberner/fuser/master/examples/hello.rs
- GitHub simple example: https://raw.githubusercontent.com/cberner/fuser/master/examples/simple.rs
