# confuse 架构文档

> **核心契约**：confuse 完全采用 fuser 的构造、函数、trait。任何与 fuser 的偏离都是 bug。垫片层只涉及下游开发者的 API。
>
> **版本口径**：API 表面对齐 `fuser 0.17.0`（docs.rs/crates.io latest）。

## 1. 项目概述

confuse 是一个跨平台 FUSE 类文件系统 shim 库，为 Rust 下游文件系统提供统一的 fuser 兼容 API：

- **Linux/非 Windows**：`src/lib.rs` 直接 `pub use fuser::*`，不引入额外垫片逻辑。
- **Windows**：`src/lib.rs` 直接声明 `src/fuser_facade/` 与 `src/dokan_impl/`；`src/fuser_facade/` 暴露 fuser 面向公开 API，`src/dokan_impl/` 承担 Dokan/Dokan-sys 后端适配。

```text
┌─────────────────────────────────────────────────────────────┐
│                      下游文件系统实现                        │
│                impl confuse::Filesystem                     │
├─────────────────────────────────────────────────────────────┤
│                      confuse API                            │
│  Filesystem / Reply* / Request / Session / Config / ...     │
├──────────────┬──────────────────────────────────────────────┤
│ not(windows) │                    windows                   │
│   pub use    │    src/fuser_facade API + src/dokan_impl     │
│   fuser::*   │        Dokan 回调 → fuser 风格调用           │
├──────────────┼──────────────────────────────────────────────┤
│    fuser     │              dokan / dokan-sys               │
│  内核 FUSE   │              用户态文件系统                   │
└──────────────┴──────────────────────────────────────────────┘
```

### 1.1 契约检查点

| 检查项 | 状态 | 说明 |
|---|---|---|
| fuser 根模块公开导出/类型 | ✅ | 非 Windows 直接 `pub use fuser::*`；Windows 符号表面对齐 fuser 0.17 垫片契约 |
| `Filesystem` trait 签名/默认实现 | ✅ | `Send + Sync + 'static`；`init` 返回 `io::Result<()>`；强类型参数（`INodeNo`、`FileHandle`、`OpenFlags` 等） |
| mount/session 函数表 | ✅ | `mount2` / `spawn_mount2` 接受 `&Config`；`mount` / `spawn_mount` deprecated 委托到 `*2` 变体 |
| Dokan 回调映射 | ✅ | `src/dokan_impl/adapter` 通过 `DokanAdapter` 连接到相同的 fuser 面向公开 API |

当前无已接受的公开**签名/表面** API 偏差；任何新增偏差都必须先视为 bug。

## 2. 当前目录结构

```text
src/
├── lib.rs                         # 平台分发：非 Windows 直接导出外部 fuser；Windows 导出 fuser_facade 模块
├── fuser_facade/
│   ├── mod.rs                     # fuser 兼容公开表面 re-export
│   ├── fuse_abi.rs                # fuser 兼容常量命名空间
│   ├── types.rs                   # FileType, FileAttr, KernelConfig, Config, MountOption, TimeOrNow, INodeNo,
│   │                              # FileHandle, LockOwner, Generation, RequestId, Version, SessionACL 等类型别名/新类型
│   ├── request.rs                 # Request 类型（无生命周期）、请求唯一 ID 计数、Dokan requester token/缓存身份 → Request
│   ├── session.rs                 # FsCell, Session, BackgroundSession, SessionUnmounter 和 mount 函数族
│   ├── notifier.rs                # Notifier 兼容表面（无条件导出，无 ABI feature gate）
│   ├── filesystem/
│   │   ├── mod.rs                 # 只声明子模块并 re-export
│   │   ├── api.rs                 # Filesystem trait（fuser 0.17 风格签名与默认实现）
│   │   └── tests.rs               # trait 默认行为契约测试
│   └── reply/
│       ├── mod.rs                 # 只声明子模块并 re-export
│       ├── api.rs                 # Reply trait 与 Reply* 类型的同步捕获实现、ChannelSender、fuse_forget_one
│       └── tests.rs               # Reply 载荷、目录容量、xattr 等契约测试
└── dokan_impl/
    ├── mod.rs                     # Dokan 后端内部模块根
    ├── mountpoint.rs              # mountpoint 路径转换工具
    ├── mount_options.rs           # fuser 风格挂载参数解析与 Dokan MountOptions 映射
    ├── helpers.rs                 # errno/属性转换、路径/inode 解析、生命周期与句柄辅助函数、default_kernel_config()
    ├── tests.rs                   # Dokan 辅助函数与挂载选项契约测试
    └── adapter/
        ├── mod.rs                 # 只声明子模块并 re-export
        ├── state.rs               # AdapterContext 与 DokanAdapter 状态容器
        └── handlers.rs            # Dokan FileSystemHandler 实现：回调翻译到 Filesystem 方法

examples/
└── memfs/
    └── main.rs                    # 跨平台内存文件系统示例

tests/
└── api_surface.rs                 # 非 Windows fuser re-export 与 Windows fuser 风格公开表面检查
```

## 3. 分层职责

### 3.1 `src/lib.rs`

平台边界只在根模块发生：

```rust
#[cfg(not(windows))]
pub use fuser::*;

#[cfg(windows)]
pub use fuser_facade::*;

#[cfg(windows)]
pub(crate) mod dokan_impl;
#[cfg(windows)]
mod fuser_facade;
```

因此 Linux/非 Windows 的行为由 `fuser` 本身决定；Windows 的兼容工作拆分在 `src/fuser_facade/` 与 `src/dokan_impl/` 内。

### 3.2 `fuser_facade/` 与 `dokan_impl/`

Windows 侧不再通过 `windows` 包装模块聚合。`src/fuser_facade/` 承载下游开发者看到的 fuser 兼容公开表面；`src/dokan_impl/` 承载 Dokan 后端命名空间、挂载点转换、挂载选项映射、辅助转换函数和 Dokan callback adapter。

### 3.3 `filesystem/`

`Filesystem` trait 是下游最核心的开发表面。Windows 侧以 fuser 0.17 风格签名暴露元数据、创建删除、I/O、目录、xattr、lock、ioctl、poll、fallocate、lseek、copy_file_range 等方法，并为未实现方法提供确定的默认回复。

关键 0.17 变更：
- `Filesystem: Send + Sync + 'static`（trait 级约束）
- `init` 返回 `io::Result<()>`（不再是 `Result<(), c_int>`）
- `getattr` 接受 `_fh: Option<FileHandle>` 参数
- 除 `lseek` 外的 `offset` 参数为 `u64`（`lseek` 保留 `i64`，与 fuser 0.17 一致）
- 使用强类型参数：`INodeNo`、`FileHandle`、`LockOwner`、`OpenFlags`、`WriteFlags`、`RenameFlags`、`AccessFlags`、`IoctlFlags`、`PollFlags`、`PollEvents`、`CopyFileRangeFlags`、`BsdFileFlags`

### 3.4 `reply/`

Dokan 回调是同步返回模型，而 fuser 的下游实现通过 `Reply*` 对象写回结果。Windows 侧 `Reply*` 使用 `Arc<Mutex<Option<Result<...>>>>` 捕获下游写回内容，`DokanAdapter` 在 Filesystem 方法返回后读取该结果并转换成 Dokan 返回值。

目录类 reply 还跟踪 `max_size`、`used_size` 与 `full`，用于模拟 fuser 目录缓冲区容量语义。

`ReplyPoll` 无条件导出（不再受 `abi-7-11` feature gate）。

### 3.5 `fuser_facade/session.rs` 与 `dokan_impl/helpers.rs`

fuser 会话状态直接定义在 `session.rs` 内（无独立 `facade/` 子目录）；Dokan 与 fuser 语义差异的共享辅助层位于 `dokan_impl/helpers.rs` 与 `dokan_impl/mount_options.rs`：

- `FsCell<FS>` 用 `Mutex<FS>` 包装下游文件系统，并显式标记 `Send + Sync`，供 Dokan 回调共享访问。
- `Session<FS>` 的结构定义和生命周期实现均位于 `fuser_facade/session.rs`。
- `dokan_impl/helpers.rs` 提供 errno→NTSTATUS、`FileType`→Windows 属性、Windows 路径拆分、路径→inode 查找、句柄上下文解析、锁 owner、目录 offset、rename 覆盖策略、rename 后代路径重映射、卷名派生、`default_kernel_config()` 等辅助逻辑。
- `dokan_impl/mount_options.rs` 解析 fuser 风格挂载参数，并将 Dokan 可表达的选项映射到 `dokan::MountOptions`。

### 3.6 `dokan_impl/adapter/`

`DokanAdapter<FS>` 是 Windows 运行时适配器，持有：

```rust
pub(crate) struct DokanAdapter<FS: Filesystem> {
    pub(crate) fs: Arc<FsCell<FS>>,
    pub(crate) handles: Arc<Mutex<HashMap<String, AdapterContext>>>,
    pub(crate) dir_offsets: Arc<Mutex<HashMap<String, i64>>>,
    pub(crate) volume_name: String,
    pub(crate) fs_name: String,
    pub(crate) destroyed: Arc<AtomicBool>,
}
```

每个 handle 的 `AdapterContext` 保存 fuser 侧需要复用的运行时状态：

```rust
pub(crate) struct AdapterContext {
    pub(crate) fh: u64,
    pub(crate) flags: u32,
    pub(crate) ino: u64,
    pub(crate) is_dir: bool,
    pub(crate) lock_owner: u64,
    pub(crate) request_ids: RequestIds,
}
```

`handlers.rs` 实现 Dokan `FileSystemHandler`，把基于路径和 Windows 句柄上下文的 Dokan 回调翻译成基于 inode、file handle、`Request` 与 `Reply*` 的 fuser 风格调用。

## 4. 路径到 inode 的翻译

Dokan 回调以 Windows 路径为核心输入，例如 `\dir\file.txt`；fuser 回调以 inode 为核心输入。Windows 侧通过两类信息得到 inode：

1. 打开/创建路径时保存的 `AdapterContext.ino`。
2. 无可用上下文时，通过 `path_to_ino()` 从 `FUSE_ROOT_ID` 开始逐组件调用下游 `lookup()`。

关键辅助函数：

- `split_parent_and_name()`：把 Windows 路径拆分为父路径与叶子名。
- `path_to_ino()`：逐级 `lookup()`，把完整路径解析为 inode。
- `resolve_parent_ino()`：解析父目录 inode，失败时回退 `FUSE_ROOT_ID`。
- `resolve_ctx()`：优先使用 `AdapterContext`，并为根路径 `\` 提供 `FUSE_ROOT_ID` 回退。

## 5. Dokan 回调到 fuser 方法的映射

| Dokan 回调 | fuser 风格方法 | 说明 |
|---|---|---|
| `mounted` / `unmounted` | `init` / `destroy` | 生命周期桥接；`init` 返回 `io::Result<()>`；`destroyed` 标志用于协调 `unmounted` 与 `Session::drop` 的销毁状态 |
| `create_file` | `mkdir` / `create` / `lookup` / `open` / `opendir` | 根据 create disposition、目录标志与已有对象状态选择调用 |
| `read_file` | `read` | 使用 `AdapterContext` 中的 ino/fh/flags |
| `write_file` | `write` | 使用 `AdapterContext` 中的 ino/fh/flags |
| `get_file_information` | `getattr` | 将 `FileAttr` 转换为 Dokan `FileInfo` |
| `find_files` | `readdir` | 使用 `dir_offsets` 保存目录分页 offset（`i64`），并按需 `getattr` 补足属性 |
| `find_files_with_pattern` | `readdirplus` | 使用 Dokan pattern 匹配后返回条目 |
| `flush_file_buffers` | `flush` / `fsyncdir` | 文件与目录按 `is_dir` 分发 |
| `close_file` | `release` / `releasedir` | 关闭时清理 `handles` 与 `dir_offsets` |
| `delete_file` / `delete_directory` | 无直接删除 | 接受 Dokan 预删除回调；实际删除在 `cleanup` 中发生 |
| `cleanup` | `unlink` / `rmdir` | `delete_on_close` 时执行删除；目录删除先递归清理子项 |
| `move_file` | `rename` | 尊重 Dokan `replace_if_existing` 覆盖策略；调用 `rename` 后重映射句柄表与目录 offset 表中的后代路径 |
| `set_file_time` | `setattr` | Windows file time 映射到 `TimeOrNow` 或 `SystemTime` 字段 |
| `set_end_of_file` / `set_allocation_size` | `setattr` | 写入 size 字段 |
| `set_file_attributes` | `setattr` | 只读属性映射到 `0o444`，否则映射到 `0o644` |
| `get_disk_free_space` | `statfs` | blocks×bsize 转换为字节容量 |
| `get_volume_information` | 挂载选项派生 | `FSName` 作为卷名，`Subtype` 作为文件系统名 |
| `get_file_security` | `access` | 以最小权限检查形式桥接 |
| `lock_file` / `unlock_file` | `getlk` / `setlk` | 加锁先查冲突，再设置写锁；解锁设置 `UNLCK` |
| `find_streams` | `listxattr` | 以 xattr 列表对齐 NTFS stream 查询入口 |

覆盖说明：create/open、read、write、flush、getattr、目录读取、rename/unlink/rmdir、security、lock、stream/xattr 等活动路径都有对应桥接入口；`symlink`、`link`、`xattr`、`ioctl`、`poll`、`fallocate`、`lseek`、`copy_file_range` 等 fuser 面向 trait/API 表面存在，但不代表每一项都有独立 Dokan 回调。

## 6. Request 身份与 Dokan token 边界

fuser 0.17 的 `Request` 暴露 `unique()` → `RequestId`、`uid()`、`gid()`、`pid()`。`Request` 无生命周期参数（`Request<'a>` → `Request`）。Windows 侧保留这些访问器，但 Dokan 的 requestor token 不是所有回调阶段都能安全查询，所以 identity 构造分成两条路径：

1. `create_file` 路径调用 `request_ids_from_create_info()`，通过 Dokan `OperationInfo::requester_token()` 读取请求者 token，再把 user SID 与 primary group SID 映射为稳定的 synthetic `u32` uid/gid，并连同 `pid()` 写入 `AdapterContext.request_ids`。
2. handle-bound 回调使用 `request_from_ids(context.request_ids)`，复用打开/创建该句柄时缓存的身份。
3. 没有句柄身份的回调使用 `request_from_info()`，只保留 `pid()`，uid/gid 使用 `u32::MAX` 作为 unavailable sentinel。

这个边界是安全约束：非 create/context-free 回调不调用 `requester_token()`，避免触发 Dokan requestor token 查询在错误回调阶段崩溃。`u32::MAX` 用来表示未知 uid/gid，而不是 `0`，因为 fuser/Linux 语义中 `0` 表示 root。

## 7. Rename 覆盖策略

Dokan `move_file` 带有 `replace_if_existing`，而 fuser `rename` 只接收 fuser 风格参数与 `RenameFlags`。Windows DokanAdapter 因此在进入下游 `rename` 前执行 Dokan 覆盖策略：

1. 当 `replace_if_existing == false` 时，先对目标父目录和目标名执行 `lookup()`。
2. 如果目标已存在，直接返回 `STATUS_OBJECT_NAME_COLLISION`，不调用下游 `rename()`。
3. 如果目标不存在，或下游不支持用于预检查的 `lookup()`，再调用下游 `rename(..., flags = 0, ...)`。
4. 当 `replace_if_existing == true` 时，直接调用下游 `rename()`，允许按文件系统自身语义覆盖。

这使 Windows Explorer 的移动同名文件流程保持正确：第一次尝试不允许覆盖时得到 collision，Explorer 才会弹出覆盖确认；用户确认后，后续允许覆盖的 move 再进入实际 rename。

## 8. 挂载与会话生命周期

Windows 侧公开 fuser 0.17 风格函数族：

- `mount2(filesystem, mountpoint, &Config)` — 接受 `Config` 结构体（包含 `mount_options`、`acl`、`n_threads`、`clone_fd`）
- `spawn_mount2(filesystem, mountpoint, &Config)` — 返回 `BackgroundSession`
- `mount(filesystem, mountpoint, &[&OsStr])`（deprecated，解析 `-o` 风格参数后委托 `mount2`）
- `spawn_mount(filesystem, mountpoint, &[&OsStr])`（deprecated，解析后委托 `spawn_mount2`）

`Session::new()` 接受 `&Config`，保存下游文件系统、挂载点和选项，并预校验 Dokan 挂载选项。`Session::run()` 创建 `DokanAdapter`、派生卷名/文件系统名、初始化 Dokan、挂载文件系统，并在文件系统关闭后 shutdown Dokan。`BackgroundSession` 在线程中运行 `Session::run()`，并通过 `BackgroundMountGuard` 在 drop 时触发卸载。

## 9. 挂载选项策略

`MountOption` 保留 fuser 面向的常见选项。Windows 侧处理策略：

| 选项 | Dokan 处理 |
|---|---|
| `RO` | 设置 `MountFlags::WRITE_PROTECT` |
| `RW` | 默认读写，无额外 flag |
| `FSName` | 由 `derive_volume_names()` 消费为卷名 |
| `Subtype` | 由 `derive_volume_names()` 消费为文件系统名 |
| `CUSTOM("single_thread")` | 设置 `MountOptions::single_thread = true` |
| `CUSTOM("debug")` | 设置 `MountFlags::DEBUG` |
| `AllowOther` / `AllowRoot` / `AutoUnmount` / `DefaultPermissions` / `Dev` / `NoDev` / `Suid` / `NoSuid` / `Exec` / `NoExec` / `Atime` / `NoAtime` / `DirSync` / `Sync` / `Async` | 为 fuser API 兼容性接受；Dokan 无等价表达时作为 no-op |
| 其他 `CUSTOM` | 为 fuser 面向兼容性接受 |

Dokan 无法表达的挂载选项属于显式例外：依赖 Linux FUSE 内核挂载语义的行为，在 Windows 上通过 Dokan 兼容的垫片行为表示。当前策略是在 API 边界接受并记录为 no-op；后续新增无法表达的选项时，必须在拒绝或记录为 no-op 之间明确选择，并补充对应测试。

## 10. 错误与属性转换

`errno_to_ntstatus()` 将下游 Filesystem 返回的 POSIX errno 转换成 Dokan/Windows NTSTATUS：

| POSIX errno | Windows NTSTATUS |
|---|---|
| `ENOSYS` | `STATUS_NOT_IMPLEMENTED` |
| `ENOENT` | `STATUS_OBJECT_NAME_NOT_FOUND` |
| `EEXIST` | `STATUS_OBJECT_NAME_COLLISION` |
| `ENOSPC` | `STATUS_DISK_FULL` |
| `EACCES` / `EPERM` | `STATUS_ACCESS_DENIED` |
| `EINVAL` | `STATUS_INVALID_PARAMETER` |
| `EBUSY` | `STATUS_ALREADY_COMMITTED` |
| 其他 | `STATUS_UNSUCCESSFUL` |

`filetype_to_windows_attr()` 将目录映射为 `FILE_ATTRIBUTE_DIRECTORY`，其他类型映射为 `FILE_ATTRIBUTE_NORMAL`；当权限中没有写位时附加 `FILE_ATTRIBUTE_READONLY`。

`rename_with_replace_policy()` 也会直接产生 `STATUS_OBJECT_NAME_COLLISION`，用于表达 Dokan `replace_if_existing == false` 且目标已存在的移动冲突；这不是 POSIX errno 转换，而是 Windows move/Explorer 覆盖确认路径的一部分。

## 11. 类型系统

### 11.1 0.17 新类型

Windows 侧定义以下 fuser 0.17 对齐的类型别名与新类型：

| 类型 | 底层类型 | 用途 |
|---|---|---|
| `INodeNo` | `u64` | inode 编号 |
| `FileHandle` | `u64` | 文件句柄 |
| `LockOwner` | `u64` | 锁 owner |
| `Generation` | `u64` | inode generation |
| `InitFlags` | `u32` | `init` 阶段 capability flags |
| `OpenFlags` | `i32` | open/create flags |
| `FopenFlags` | `u32` | fopen flags |
| `WriteFlags` | `u32` | write flags |
| `RenameFlags` | `u32` | rename flags |
| `AccessFlags` | `i32` | access 检查 mask |
| `IoctlFlags` | `u32` | ioctl flags |
| `PollFlags` | `u32` | poll flags |
| `PollEvents` | `u32` | poll events |
| `CopyFileRangeFlags` | `u32` | copy_file_range flags |
| `BsdFileFlags` | `u32` | BSD file flags |
| `RequestId` | `u64`（newtype） | 请求唯一标识符 |
| `Version` | struct `{ major: u32, minor: u32 }` | FUSE 协议版本 |
| `SessionACL` | enum | 会话访问控制（`All` / `RootAndOwner` / `Owner`） |
| `Config` | struct | 挂载配置（`mount_options`、`acl`、`n_threads`、`clone_fd`） |
| `KernelConfig` | struct | 内核协商配置（含 `max_background`、`congestion_threshold`、`time_gran`、`max_stack_depth`、`kernel_abi` 等字段） |

### 11.2 Reply 类型系统

Windows 侧公开的主要 Reply 类型包括：

| 类型 | 内部载荷 | 主要写回方法 |
|---|---|---|
| `ReplyEmpty` | `Result<(), c_int>` | `ok()`, `error(err)` |
| `ReplyData` | `Result<Vec<u8>, c_int>` | `data(&[u8])`, `error(err)` |
| `ReplyEntry` | `Result<FileAttr, c_int>` | `entry(ttl, attr, generation)`, `error(err)` |
| `ReplyAttr` | `Result<FileAttr, c_int>` | `attr(ttl, attr)`, `error(err)` |
| `ReplyOpen` | `Result<(u64, u32), c_int>` | `opened(fh, flags)`, `error(err)` |
| `ReplyWrite` | `Result<u32, c_int>` | `written(size)`, `error(err)` |
| `ReplyCreate` | `Result<(FileAttr, u64, u32), c_int>` | `created(ttl, attr, generation, fh, flags)`, `error(err)` |
| `ReplyStatfs` | `Result<(u64,u64,u64,u64,u64,u32,u32,u32), c_int>` | `statfs(...)`, `error(err)` |
| `ReplyDirectory` | 目录条目列表 + 容量状态 | `add(...)`, `ok()`, `error(err)` |
| `ReplyDirectoryPlus` | 目录增强条目列表 + 容量状态 | `add(...)`, `ok()`, `error(err)` |
| `ReplyLock` | `Result<(u64, u64, i32, u32), c_int>` | `locked(start, end, typ, pid)`, `error(err)` |
| `ReplyXattr` | `Result<Vec<u8>, c_int>` + size hint | `size(u32)`, `data(&[u8])`, `error(err)` |
| `ReplyBmap` | `Result<u64, c_int>` | `bmap(block)`, `error(err)` |
| `ReplyIoctl` | `Result<(i32, Vec<u8>), c_int>` | `ioctl(result, data)`, `error(err)` |
| `ReplyLseek` | `Result<i64, c_int>` | `offset(i64)`, `error(err)` |
| `ReplyPoll` | `Result<(), c_int>` | `poll(events)`, `error(err)` |

`ReplyPoll` 无条件导出（不受 ABI feature gate）。

## 12. 测试与契约守卫

当前仓库的架构守卫主要分布在：

| 测试类别 | 状态 | 说明 |
|---|---|---|
| API 表面/导出编译守卫 | ✅ | `tests/api_surface.rs` 检查非 Windows 直接 fuser re-export，以及 Windows 公开符号、挂载函数（`mount2`/`spawn_mount2` 接受 `&Config`）、Reply、Request（无生命周期）、Session、Notifier 等 fuser 风格表面 |
| 选项解析/映射单元测试 | ✅ | `src/dokan_impl/tests.rs` 覆盖挂载选项解析/映射 |
| Reply 载荷单元测试 | ✅ | `src/fuser_facade/reply/tests.rs` 覆盖 Reply 捕获、目录容量、xattr |
| Filesystem 默认实现契约 | ✅ | `src/fuser_facade/filesystem/tests.rs` 覆盖 trait 默认行为 |
| Request/类型语义测试 | ✅ | `src/fuser_facade/request.rs` 与 `src/fuser_facade/types.rs` 内部测试覆盖请求 ID（`RequestId`）、请求身份不可用回退、KernelConfig、TimeOrNow、Config |
| Notifier 契约测试 | ✅ | `src/fuser_facade/notifier.rs` 内部测试覆盖 Notifier 公开方法形状 |

仓库范围闭环条件：`tests/api_surface.rs` 必须通过，模块内语义契约场景必须通过；当前文档不声明真实挂载端到端测试。文档更新属于文档表面变更，不改变生产代码行为；验证重点是文档内容与当前源码布局、公开 API 分层和 Windows DokanAdapter 职责保持一致。

## 13. 设计约束

1. **公开 API 以 fuser 0.17 为准**：Linux 直接导出 fuser；Windows 只允许在实现层借助 Dokan，不允许改变下游面向的 fuser 风格构造、函数和 trait。
2. **公开 API 名称/签名优先匹配**：后端行为必须连接在相同公开 API 之后，不能用 Dokan 形状替换下游开发者看到的 fuser 形状。
3. **垫片边界只在 Windows 后端**：所有平台分歧收敛到 `src/lib.rs`、`src/fuser_facade/` 与 `src/dokan_impl/`，不再存在额外 `windows` 包装模块。
4. **Dokan 路径模型必须转换为 fuser inode 模型**：打开句柄优先使用 `AdapterContext`，缺失时通过逐级 `lookup()` 解析路径。
5. **同步 Reply 捕获**：下游仍按 fuser 风格写 reply；Windows DokanAdapter 同步读取 reply 并转换为 Dokan 结果。
6. **无法由 Dokan 表达的 fuser 挂载选项必须显式处理**：当前策略是在 API 边界接受并作为记录化 no-op。
7. **生命周期必须避免重复 destroy**：Dokan unmounted 与 `Session::drop` 共享 `destroyed` 标志。
8. **Dokan requestor token 只在 create 路径查询**：其他回调使用缓存身份或 unavailable sentinel，不能为了填充 uid/gid 在非安全阶段重新打开 token。
9. **Dokan 覆盖语义先于 fuser rename 调用处理**：`replace_if_existing == false` 的目标冲突必须返回 `STATUS_OBJECT_NAME_COLLISION`，以保留 Explorer 覆盖确认行为。
10. **契约通过测试持续检查**：API surface 编译守卫和模块语义测试是每次结构调整后的最低闭环。
11. **`Filesystem` trait 满足 `Send + Sync + 'static`**：允许跨线程共享和静态生命周期约束，与 fuser 0.17 对齐。
12. **`init` 返回 `io::Result<()>`**：失败通过 `Err(io::Error)` 传播，不再是 `Result<(), c_int>`。
13. **Feature flags `abi-7-*` 范围有限**：facade 层公开 API 类型（Notifier、ReplyPoll 等）无条件导出；`KernelConfig` 方法（`set_max_background`、`set_congestion_threshold`、`set_time_granularity`）始终可用，不受 feature gate；feature flags 仅控制 `Filesystem::poll`（`abi-7-11`）、`Filesystem::batch_forget`（`abi-7-16`）的方法级条件编译，以及对应测试的条件编译。
