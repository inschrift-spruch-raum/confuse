# fuser 到 Dokan/dokan-sys 转换层设计涉及工作

> 编写依据：本项目内仅使用 `docs/Standard/Rust fuser库 API架构分析.md` 与 `docs/Standard/Rust dokan(-sys)库 API架构分析.md`；另结合公开网络资料（fuser docs.rs、Dokany Doxygen、dokan-rust 文档）补充 API 语义。未阅读项目内其他信息。

## 1. 目标与边界

目标是在 Windows/Dokany 上承载面向 `fuser::Filesystem` 设计的文件系统逻辑，提供一层从 FUSE/fuser 语义到 Dokan/dokan-sys 语义的转换适配。该层不应把业务文件系统重写成 Dokan 原生实现，而应尽量保留 fuser 侧的核心抽象：inode、文件句柄、`FileAttr`、`Request`、`Reply*`、errno、TTL、lookup/forget 生命周期，以及 `Filesystem: Send + Sync + 'static` 的并发约束。

边界如下：

- **转换层负责**：挂载生命周期桥接、路径与 inode 解析、fuser 回调调度、reply 到 NTSTATUS/输出缓冲的转换、句柄与上下文管理、属性/时间/权限/错误码映射、目录枚举、并发与缓存策略。
- **业务文件系统负责**：实际数据源、目录树语义、权限策略、元数据更新、文件内容读写、锁/xattr/特殊操作的真实支持能力。
- **第一阶段不宜承诺完整 POSIX 语义**：Dokan 与 FUSE 的系统模型不同，尤其是 Windows 打开/共享/删除、ACL、大小写、备用数据流、内存映射 I/O、Cleanup/CloseFile 生命周期，无法机械等价。

## 2. 两端模型差异

| 维度 | fuser/FUSE | Dokan/dokan-sys | 转换层工作 |
|---|---|---|---|
| 对象定位 | inode 为主，目录项由 `(parent ino, name)` lookup 得到 | 路径字符串为主，`ZwCreateFile` 接收 `LPCWSTR FileName` | 维护 path ↔ inode ↔ attr 映射与负缓存失效策略 |
| 请求上下文 | `Request` 暴露 unique/uid/gid/pid | `DOKAN_FILE_INFO` 暴露 ProcessId、IsDirectory、DeletePending、PagingIo、Nocache、WriteToEndOfFile 等 | 构造 Windows 环境下的 `Request` 等价信息；无法提供真实 uid/gid 时定义策略 |
| 响应模型 | 每个回调拿一次性 `Reply*`，必须成功或 error 一次 | 回调返回 `NTSTATUS`，并通过 out 参数写入数据/长度 | 实现同步 reply 收集器，把 `Reply*` 成功载荷转为 Dokan 输出 |
| 打开生命周期 | `open/create -> read/write/flush/release`，`release` 每次 open 一次 | `ZwCreateFile -> Read/Write... -> Cleanup -> CloseFile`；Cleanup 后仍可能有 paging I/O | 建立 handle table；区分 flush/release 与 Cleanup/CloseFile，不在 Cleanup 过早释放 paging I/O 所需状态 |
| 错误码 | POSIX errno (`Errno::ENOENT` 等) | `NTSTATUS`，可由 Win32 转换 | 建立 errno ↔ NTSTATUS 映射表，并覆盖 Windows 特有状态 |
| 属性 | `FileAttr`：ino、size、blocks、time、kind、perm、uid/gid、nlink 等 | `BY_HANDLE_FILE_INFORMATION` / `WIN32_FIND_DATAW` / FILE_ATTRIBUTE flags | 映射时间、类型、大小、链接数、文件索引、权限降级策略 |
| 目录枚举 | `readdir` 按 offset 填 `ReplyDirectory` | `FindFiles`/`FindFilesWithPattern` 用填充回调输出 `WIN32_FIND_DATAW` | 维护稳定目录快照或 offset/token 映射；支持 Windows pattern 过滤 |
| 缓存 | `ReplyEntry`/`ReplyAttr` 明确 TTL；mount option 影响缓存 | Dokan/Windows 缓存由打开标志、文件属性、通知 API、系统缓存共同影响 | 设计 TTL 缓存、失效通知、`Nocache`/paging I/O 策略 |
| 并发 | `Filesystem: Send + Sync`，可多线程 session | Dokany 回调默认多线程，Read/Write 可并发 | 所有 inode/path/handle/dir snapshot 状态必须同步 |

## 3. 推荐架构

```text
Windows 应用 / Explorer
  -> Windows I/O manager
  -> dokan2.sys / dokan2.dll
  -> dokan-sys DOKAN_OPERATIONS 回调
  -> 转换层：Dokan 回调适配器
       - Mount/session bridge
       - PathResolver / InodeTable
       - HandleTable / DirectoryCursorTable
       - ReplyCollector
       - Attr/Error/Flag/Time mapper
       - Cache/notification manager
  -> 用户提供的 fuser::Filesystem 实现
```

建议优先直接面向 `dokan-sys` 设计底层适配核心，再视需要提供 `dokan` crate 风格的安全外壳。原因是转换层需要精细控制 `DOKAN_OPERATIONS`、`DOKAN_FILE_INFO.Context`、回调 out 参数、NTSTATUS、挂载 flags 和 unsafe 生命周期；这些正是 `dokan-sys` 暴露得最完整的部分。

## 4. 核心模块工作拆分

### 4.1 挂载与会话桥接

需要提供类似 fuser `mount2` / `spawn_mount2` 的入口：

- 接收 `FS: fuser::Filesystem`、挂载点、转换层 mount options。
- 生成 `DOKAN_OPTIONS`：版本、线程数、挂载点、超时、网络盘/可移动盘/只读/大小写敏感等 flags。
- 填充 `DOKAN_OPERATIONS` 函数指针表，并保证回调中的 handler 指针生命周期长于 Dokan 挂载。
- 支持阻塞挂载与后台挂载两种语义；后台句柄 drop 时应卸载，类似 `BackgroundSession`。
- 实现 `init` 与 `destroy` 调用顺序：Dokan 挂载成功前后调用 fuser `init`，卸载时调用 `destroy`。

### 4.2 路径、inode 与 lookup/forget

这是转换层最关键的语义层。

fuser 的 `lookup(parent, name, ReplyEntry)` 返回 inode 与属性；Dokan 多数回调只给路径。因此必须维护：

- `PathKey`：规范化 Windows 路径，处理 `\`、根目录、大小写策略、保留名、尾随分隔符。
- `InodeTable`：`ino -> attr/state`，至少保存 path、generation、lookup refcount、目录/文件类型；根 inode 应固定为 FUSE root 语义，inode 重用必须更换 generation 或避免重用。
- `DirEntryTable`：`(parent ino, name) -> ino`，用于把 path 分解为逐级 lookup。
- `resolve_path(path)`：从根 inode 开始逐级调用 fuser `lookup`，命中缓存时按 TTL 复用。
- `forget` 调度：当缓存项过期、路径被删除、卸载或显式失效时减少 lookup 引用。注意 fuser 文档说明卸载时不保证每个引用都有 forget，转换层可选择 best-effort。

需要明确的设计决策：Windows 按路径反复打开时是否每次都调用 lookup，还是由 TTL 缓存吸收。建议默认尊重 `ReplyEntry` TTL；TTL 为 0 时不缓存目录项。fuser 还支持 entry TTL 与 attr TTL 分离：entry TTL 控制 name -> inode 缓存，attr TTL 控制 metadata 缓存，转换层不应把二者无条件合并成一个超时。

### 4.3 Reply 收集器

fuser 方法不返回 `Result`，而通过一次性 `Reply*` 对象回包；Dokan 回调需要同步返回 `NTSTATUS` 并写 out 参数。因此需要实现内部 reply 收集机制：

- 每种 fuser reply 对应一个可被同步等待/读取的内部结果枚举：`Entry`、`Attr`、`Data`、`Directory`、`Open`、`Create`、`Write`、`Empty`、`Statfs`、`Xattr`、`Lock` 等。
- 成功载荷携带 fuser 的 TTL、attr、generation、fh、flags、bytes、written count 等。
- 错误载荷携带 errno。
- reply 必须 exactly once：重复回包视为转换层 bug；未回包应返回内部错误并记录诊断。
- 若未来允许异步 reply，需要把 Dokan 回调同步模型与请求挂起/超时扩展一起设计；第一阶段建议只支持同步回包。

### 4.4 错误码映射

建立集中映射表，避免每个回调手写：

| POSIX errno | 建议 NTSTATUS | 典型场景 |
|---|---|---|
| `ENOENT` | `STATUS_OBJECT_NAME_NOT_FOUND` / `STATUS_OBJECT_PATH_NOT_FOUND` | 文件或路径不存在 |
| `EEXIST` | `STATUS_OBJECT_NAME_COLLISION` | create 已存在 |
| `ENOTDIR` | `STATUS_NOT_A_DIRECTORY` | 期望目录但目标非目录 |
| `EISDIR` | `STATUS_FILE_IS_A_DIRECTORY` | 期望文件但目标为目录 |
| `ENOTEMPTY` | `STATUS_DIRECTORY_NOT_EMPTY` | 删除非空目录 |
| `EACCES` / `EPERM` | `STATUS_ACCESS_DENIED` | 权限拒绝 |
| `EBADF` | `STATUS_INVALID_HANDLE` | 句柄无效 |
| `EINVAL` | `STATUS_INVALID_PARAMETER` | 参数非法 |
| `ENOSYS` | `STATUS_NOT_IMPLEMENTED` | 未实现操作 |
| `ENOTSUP` / `EOPNOTSUPP` | `STATUS_NOT_SUPPORTED` | 不支持能力 |
| `ERANGE` | `STATUS_BUFFER_OVERFLOW` 或 `STATUS_BUFFER_TOO_SMALL` | xattr/security 缓冲不足 |
| `ENOSPC` | `STATUS_DISK_FULL` | 空间不足 |
| `EROFS` | `STATUS_MEDIA_WRITE_PROTECTED` | 只读卷写入 |

还需处理 Dokan 特殊成功语义：`OPEN_ALWAYS` / `CREATE_ALWAYS` 打开已有文件时应返回 `STATUS_OBJECT_NAME_COLLISION` 以告知 Dokan “打开了已有对象而非新建”。另外，FUSE/libfuse 对部分操作的 `ENOSYS` 有“永久不支持并启用默认降级”的含义，转换层需要把它记录为能力协商结果，而不是只当作一次普通错误。

### 4.5 属性、时间与文件类型映射

`fuser::FileAttr` 到 Windows 结构至少涉及：

- `FileType::Directory` -> `FILE_ATTRIBUTE_DIRECTORY`。
- 普通文件 -> `FILE_ATTRIBUTE_NORMAL`，只读权限可叠加 `FILE_ATTRIBUTE_READONLY`。
- symlink -> Windows reparse point；如果第一阶段不支持，应返回 `STATUS_NOT_SUPPORTED` 或把 readlink/symlink 标为不支持。
- `size` -> `nFileSizeHigh/Low`；`blocks` 可用于 allocation size，但 Windows 查询路径中可能要单独处理。
- `atime/mtime/ctime/crtime` -> `FILETIME`；注意 Unix epoch 到 Windows epoch 转换和纳秒精度损失。
- `ino` / `generation` -> `nFileIndexHigh/Low` 或内部 FileId；需要稳定，否则 Windows 缓存/硬链接识别会异常。
- `perm/uid/gid`：Windows ACL 与 POSIX mode 不等价。第一阶段可只映射只读位，并把访问控制交给业务 FS 或 Dokan security 回调。

### 4.6 打开、创建与句柄表

Dokan 的 `ZwCreateFile` 是打开、创建、截断、目录打开、权限检查的统一入口；fuser 将它拆成 `lookup`、`access`、`open`、`create`、`setattr` 等。

转换层需要解析 `CreateDisposition` / `CreateOptions` / `DesiredAccess`：

- 已存在且只打开：resolve path 后调用 `open` 或 `opendir`。
- 不存在且创建文件：对父目录 resolve，调用 fuser `create`；若未实现，可回退到 `mknod + open`。
- 创建目录：调用 `mkdir`，再按目录打开语义建立 context。
- 截断：打开成功后根据 `FILE_OVERWRITE*` 或 truncate 标志调用 `setattr(size=0)`。
- 目录标志：目标为目录时设置 `DOKAN_FILE_INFO.IsDirectory = TRUE`；类型不匹配返回对应 NTSTATUS。
- 返回的 fuser `fh` 进入 `HandleTable`，并绑定 Dokan `Context`。

句柄表至少保存：ino、fh、是否目录、访问模式、打开 flags、删除 pending、是否允许读写、目录枚举游标、是否仍允许 paging I/O。

### 4.7 Read/Write/Flush/Release 与 Cleanup/CloseFile

映射建议：

- `ReadFile` -> fuser `read(ino, fh, offset, size, flags, lock_owner, ReplyData)`，把返回 bytes 拷贝进 Dokan buffer，设置 `ReadLength`。
- `WriteFile` -> fuser `write(...)`，若 `DOKAN_FILE_INFO.WriteToEndOfFile` 为真，需要先以当前 size 作为 offset；paging I/O 不应扩展文件大小，需按 Windows 规则限制。
- `FlushFileBuffers` -> fuser `fsync(datasync=false)` 或 `flush` 的策略需区分。fuser 文档说明 `flush` 是每次 close 调用且不等于强制落盘；Windows `FlushFileBuffers` 更接近 `fsync`。
- `Cleanup`：处理 delete-on-close 的真实删除；可触发 fuser `flush`，但不能释放仍可能被 paging I/O 使用的核心状态。
- `CloseFile`：最终释放 Dokan Context；对应 fuser `release` / `releasedir` 的最佳位置。必须确保每次 `open/opendir/create` 最终只 release 一次。

这是高风险区域：Dokany 文档明确说明内存映射场景下 Read/Write 可能在 Cleanup 后发生，因此不能简单把 Cleanup 等同于 fuser `release`。

### 4.8 目录枚举

fuser `readdir` 使用 offset 分页，Dokan `FindFiles` 需要向填充回调持续输出目录项。转换层有两种实现方式：

1. **一次性收集**：调用 fuser `readdir`，递增 offset 直到 EOF，构造完整目录列表，再逐项转 `WIN32_FIND_DATAW`。实现简单，但大目录成本高。
2. **目录快照/游标**：在 `opendir` 或首次 `FindFiles` 时建立目录快照，后续按 pattern/filter 输出。更接近稳定枚举语义，但需要管理快照生命周期和失效。

建议第一阶段实现一次性收集，并为大目录保留游标接口。`FindFilesWithPattern` 可优先用 Dokan `DokanIsNameInExpression` 或自建 Windows wildcard 匹配；未实现时让 Dokan 回退到 `FindFiles` 过滤。

### 4.9 修改操作映射

| Dokan 回调 | fuser 目标方法 | 注意点 |
|---|---|---|
| `SetFileAttributes` | `setattr(flags/mode)` 的降级封装 | Windows attributes 与 POSIX mode 不等价 |
| `SetFileTime` | `setattr(atime/mtime/crtime)` | `ctime` 在 POSIX 中是 metadata change time，不等于 Windows creation time |
| `DeleteFile` | `unlink` 的预检查阶段 | Dokan 要求此处只检查，真实删除在 Cleanup |
| `DeleteDirectory` | `rmdir` 的预检查阶段 | 需能判断空目录；真实删除在 Cleanup |
| `MoveFile` | `rename` | `ReplaceIfExisting` 映射到 rename flags 或预删除策略 |
| `SetEndOfFile` | `setattr(size)` | 文件长度变化 |
| `SetAllocationSize` | `fallocate` 或 `setattr(size)` 降级 | 物理分配大小与逻辑 EOF 不同 |
| `LockFile` / `UnlockFile` | `setlk` / lock table | Windows byte range locks 与 POSIX lock owner 需要适配 |

删除语义必须单独设计：Dokan `DeleteFile/DeleteDirectory` 成功后，最后一个 handle 的 `Cleanup` 才会带 `DeletePending`，此时操作不能失败；因此预检查阶段必须确保 Cleanup 能完成。

### 4.10 statfs、卷信息与挂载选项

- `GetDiskFreeSpace` -> fuser `statfs`，映射 blocks/bfree/bavail/frsize 等。
- `GetVolumeInformation` -> 转换层 mount config，返回卷名、序列号、最大文件名长度、文件系统名。Dokany 文档建议文件系统名不超过约 10 字符，且部分 Windows 组件会按 `NTFS`/`FAT` 判断特性。
- fuser `MountOption::RO` -> `DOKAN_OPTION_WRITE_PROTECT` 或转换层写操作拒绝。
- fuser `DefaultPermissions` 在 Windows 上不能直接复用 Linux 内核权限检查；需要决定由转换层模拟 POSIX mode，还是交给 Dokan security 回调/业务层。
- fuser `n_threads` -> Dokan thread count。

### 4.11 xattr、安全描述符、备用数据流

fuser 有 xattr 回调；Dokan/Windows 有 security descriptor 与 alternate data streams：

- Linux xattr 与 Windows ADS/EA/安全描述符不是同一模型。
- 第一阶段建议把 xattr 标为可选能力：若业务 FS 依赖 xattr，需要定义命名空间映射（例如 `user.*` 到内部元数据，而非直接 ADS）。
- `GetFileSecurity` / `SetFileSecurity` 若未实现，Dokan 可构造默认安全描述符，但权限表现会与 POSIX 不同。
- `FindStreams` 只有启用 `DOKAN_OPTION_ALT_STREAM` 才调用；除非明确支持 ADS，不应默认开启。

### 4.12 通知与缓存失效

fuser 用 TTL 控制 entry/attr 缓存，且 `Session::notifier()` 可发通知；Dokan 提供 `DokanNotifyCreate/Delete/Rename/Update/XAttrUpdate`。转换层需要：

- 根据 `ReplyEntry` / `ReplyAttr` TTL 管理 path/inode/attr 缓存；entry TTL 与 attr TTL 分别管理，不应在需要精确语义时共用一个缓存项。
- 当业务侧修改目录或属性时更新缓存，并调用 Dokan notify API 让 Windows 侧观察到变化。
- TTL 为 0 的动态文件必须避免缓存 size/attr；标准文档中 fuser 示例说明动态 size 若缓存会过期。
- 设计负缓存（不存在项）的 TTL，避免 Windows Explorer 重复探测造成过多 lookup。

## 5. 操作映射矩阵

| fuser 方法 | Dokan 入口 | 支持级别建议 | 说明 |
|---|---|---|---|
| `init` / `destroy` | `Mounted`/`Unmounted` + mount wrapper | 必需 | 保证生命周期顺序 |
| `lookup` / `forget` | 所有路径入口前置解析 | 必需 | 转换层核心 |
| `getattr` | `GetFileInformation`, `FindFiles` | 必需 | 属性查询与目录项输出 |
| `open` / `opendir` | `ZwCreateFile` | 必需 | 建立 context/handle |
| `read` | `ReadFile` | 必需 | 只读 FS 最小能力之一 |
| `write` | `WriteFile` | 写支持必需 | 只读卷可返回拒绝 |
| `readdir` | `FindFiles` / `FindFilesWithPattern` | 必需 | Explorer 依赖 |
| `release` / `releasedir` | `CloseFile` | 必需 | 每次 open/opendir 一次 |
| `flush` / `fsync` | `Cleanup`, `FlushFileBuffers` | 必需 | 语义需区分 |
| `create` / `mknod` / `mkdir` | `ZwCreateFile` | 写支持必需 | create disposition 映射复杂 |
| `unlink` / `rmdir` | `DeleteFile/DeleteDirectory` + `Cleanup` | 写支持必需 | 预检查与真实删除分离 |
| `rename` | `MoveFile` | 写支持必需 | replace 语义需处理 |
| `setattr` | `SetFileAttributes/Time/EndOfFile/AllocationSize` | 写支持必需 | 拆分成多个 Windows 回调 |
| `statfs` | `GetDiskFreeSpace` | 推荐 | 卷容量显示 |
| `access` | `ZwCreateFile` 访问检查 / security | 推荐 | `DefaultPermissions` 需重解释 |
| `readlink` / `symlink` | reparse point 相关 | 可选 | Windows symlink 需要权限和 reparse tag |
| `link` | hard link 支持 | 可选 | Windows hard link 可支持但语义需确认 |
| xattr 系列 | xattr update / ADS / 内部元数据 | 可选 | 不直接等价 |
| `getlk` / `setlk` | `LockFile` / `UnlockFile` | 可选 | 锁 owner/范围语义不同 |
| `ioctl` / `poll` / `bmap` / `lseek` / `copy_file_range` / `fallocate` | 无直接等价或部分等价 | 延后 | 需要按业务场景逐项设计 |

## 6. 分阶段实施计划

### 阶段 A：最小只读闭环

交付目标：能挂载、浏览目录、读取文件、正确卸载。

工作项：

1. dokan-sys mount wrapper 与 `DOKAN_OPERATIONS` 静态回调表。
2. handler 指针/Arc 生命周期管理，保证 unsafe FFI 边界可审计。
3. `PathResolver`、`InodeTable`、TTL 缓存。
4. `ReplyEntry`、`ReplyAttr`、`ReplyData`、`ReplyDirectory`、`ReplyOpen`、`ReplyEmpty` 收集器。
5. `ZwCreateFile` 只读打开、目录打开、类型检查。
6. `GetFileInformation`、`FindFiles`、`ReadFile`、`Cleanup`、`CloseFile`。
7. errno 到 NTSTATUS 基础映射。

验收：Windows Explorer/命令行可列目录、查看属性、读取文件；重复打开关闭无句柄泄漏；不存在路径返回正确错误。

### 阶段 B：基础写入与元数据

交付目标：支持创建、写入、截断、删除、重命名、mtime/size 更新。

工作项：

1. `CreateDisposition` 完整映射到 `create/mknod/open/setattr`。
2. `WriteFile`、`SetEndOfFile`、`SetAllocationSize`。
3. `DeleteFile/DeleteDirectory` 预检查 + `Cleanup` 删除提交。
4. `MoveFile` 到 `rename`，处理 replace-if-existing。
5. `SetFileTime`、`SetFileAttributes` 的可支持子集。
6. 目录/属性缓存失效与 Dokan notify。

验收：可用 PowerShell/cmd 创建、追加、覆盖、重命名、删除文件和目录；Explorer 刷新后属性一致。

### 阶段 C：权限、缓存与并发稳固

交付目标：在多线程、大目录、并发读写、内存映射读写下行为稳定。

工作项：

1. 线程安全审计：inode/path/handle/dir snapshot 锁粒度。
2. Cleanup 后 paging I/O 场景测试与状态延迟释放。
3. `DefaultPermissions`/POSIX mode/Windows ACL 的策略实现或文档化降级。
4. 大目录枚举游标或快照优化。
5. 负缓存、TTL、notify 的一致性测试。
6. 超时处理：长操作调用 `DokanResetTimeout`。

验收：并发 copy/read/write、Explorer 缩略图/索引访问、内存映射读取不崩溃；缓存失效可观察。

### 阶段 D：可选高级能力

按业务需要选择：symlink/reparse point、hard link、xattr、ADS、security descriptor、byte-range lock、statfs 精细映射、fallocate/copy_file_range 降级、异步 reply 支持。

## 7. 风险与需要提前定案的问题

1. **inode 与 path 的双模型一致性**：Windows 侧路径驱动，fuser 侧 inode 驱动；rename/delete/hardlink 会让映射失效复杂化。
2. **Cleanup/CloseFile 不等于 flush/release**：过早释放 context 会破坏内存映射 I/O。
3. **权限模型不等价**：POSIX uid/gid/mode 与 Windows token/ACL 不能自动转换。
4. **删除语义不等价**：Dokan 删除检查与删除提交分离，fuser `unlink/rmdir` 通常是直接操作。
5. **目录 offset 不等价**：fuser readdir offset 需要稳定，Dokan FindFiles 偏向一次性枚举。
6. **缓存来源不同**：fuser TTL 与 Windows 文件缓存/Dokan notify 需要组合策略。
7. **错误码细节影响用户体验**：同样是“不存在”，Windows 区分 object name/path not found；Explorer 行为依赖精确状态码。
8. **版本耦合**：`dokan-sys`、import library、运行时 DLL、驱动版本必须匹配。
9. **dokan-sys 结构体版本差异**：公开 `dokan-sys` 文档可能对应较旧 Dokany 版本，字段如 `ThreadCount`、`DeleteOnClose` 与 Dokany 2.3.x 的 `SingleThread`、`DeletePending` 口径可能不同；实现前必须以锁定版本的头文件和 bindgen 结果为准。
10. **unsafe FFI 面**：回调函数指针、wide string、out buffer、Context 指针释放必须集中封装和测试。

## 8. 建议的验收场景

- 挂载/卸载：前台阻塞、后台句柄 drop、异常卸载。
- 路径解析：根目录、深层路径、不存在父目录、不存在文件、大小写差异。
- 只读 I/O：`dir`/Explorer 列表、`type`/PowerShell `Get-Content`、随机 offset 读取。
- 写 I/O：新建、覆盖、追加、截断、写到 EOF、只读卷写入拒绝。
- 元数据：size、mtime、ctime/crtime、目录标志、只读属性。
- 生命周期：重复 open/close、dup-like 多 handle、Cleanup 后 read/write、CloseFile 后释放。
- 删除/重命名：打开中文件删除、非空目录删除、replace rename。
- 并发：多线程读写同一文件、大目录枚举、Explorer 与命令行同时访问。
- 错误：权限拒绝、无效句柄、buffer 太小、未实现能力、磁盘满。

## 9. 公开资料来源

- `docs/Standard/Rust fuser库 API架构分析.md`
- `docs/Standard/Rust dokan(-sys)库 API架构分析.md`
- fuser docs.rs: https://docs.rs/fuser/latest/fuser/
- fuser `Filesystem` trait: https://docs.rs/fuser/latest/fuser/trait.Filesystem.html
- fuser reply source: https://docs.rs/fuser/latest/src/fuser/reply.rs.html
- fuser examples: https://github.com/cberner/fuser/tree/master/examples
- libfuse low-level operations: https://libfuse.github.io/doxygen/structfuse__lowlevel__ops.html
- Linux FUSE ABI: https://man7.org/linux/man-pages/man4/fuse.4.html
- Dokany `DOKAN_OPERATIONS`: https://dokan-dev.github.io/dokany-doc/html/struct_d_o_k_a_n___o_p_e_r_a_t_i_o_n_s.html
- Dokany `DOKAN_FILE_INFO`: https://dokan-dev.github.io/dokany-doc/html/struct_d_o_k_a_n___f_i_l_e___i_n_f_o.html
- Dokany `dokan.h` API: https://dokan-dev.github.io/dokany-doc/html/dokan_8h.html
- dokan-rust README: https://github.com/dokan-dev/dokan-rust
- dokan-rust Rustdoc: https://dokan-dev.github.io/dokan-rust-doc/html/dokan/
