# `src/dokan_impl` 调用方案

`src/dokan_impl` 按 Dokan/Dokany 官方语义运行，并把回调翻译成 `src/fuser_facade` 暴露的 fuser 风格入口。Dokan 官方文档语义定义调用边界与错误处理规则。

## 1. 总规则

1. **Dokan-first**：先判断 Dokan 官方回调需求，再决定 fuser 入口表达方式。
2. **fuser none 时由 adapter 模拟**：路径/inode 缓存、handle context、Windows security descriptor、Dokan mount flags、请求者 token、volume info 等由 adapter 内部保存。
3. **fuser 有入口但返回 `ENOSYS` 时按能力协商处理**：required 路径返回 Dokan 错误；按业务启用的路径记录协商结果。
4. **能力探测**：`MountOption::CUSTOM("auto_probe")` 驱动能力探测；挂载选项定义所需 fuser 入口。
5. **双层后端**：`dokan` safe trait 承载常规回调；原生 mount flag、通知、request-layer 与 callback 细节归入 `dokan-sys` 后端。

## 2. Dokan 回调调用矩阵

| Dokan 官方需求 | fuser 状态 | `dokan_impl` 调用方案 | 下游 `ENOSYS` 时 | 后端层级 |
|---|---|---|---|---|
| `Mounted` / `Unmounted` | `init` / `destroy` | mount 成功后调用 `init`，卸载时调用一次 `destroy`。 | `init` 返回 `ENOSYS` 时挂载失败；`destroy` 按 no-op 处理。 | `dokan` safe trait |
| `CreateFile` 打开已有对象 | `lookup` + `open/opendir` | 按路径解析 parent/name，查 inode，按目录/文件调用 open。 | `lookup` 返回 `ENOSYS/ENOENT` 等按 NTSTATUS 返回。 | `dokan` safe trait |
| `CreateFile` 创建文件/目录 | `create` / `mkdir`，`mknod + open` 协商路径 | RW 按 disposition 调用；RO 按只读卷拒绝。 | required 写路径返回 Dokan 错误。 | `dokan` safe trait |
| `CreateFile` share/security/options | fuser none；`access` 协助 | `share_access`、security context、type mismatch、truncate/supersede 由 adapter 按 Dokan 官方语义处理。 | adapter 返回确定错误。 | `dokan-sys`/内部状态 |
| `ReadFile` | `read` | 用 context 中 ino/fh/flags 调用 fuser `read`，拷贝 bytes 到 Dokan buffer。 | 返回 errno-derived NTSTATUS。 | `dokan` safe trait |
| `WriteFile` | `write` + `getattr`/attr cache | `WriteToEndOfFile` 时先取文件 size 作为 offset；`PagingIo` 按官方规则处理越界写：截断写入、返回拒绝。 | RW 下 `write` 返回 `ENOSYS` 即报错；RO 下直接拒绝。 | `dokan` safe trait |
| `FlushFileBuffers` | `fsync` / `fsyncdir` | `FlushFileBuffers` 映射 `fsync(datasync=false)`；`flush` 归入 release/close 策略。 | `fsync` 返回 `ENOSYS` 时走能力协商错误。 | `dokan` safe trait |
| `Cleanup` / `CloseFile` | `unlink/rmdir`、`release/releasedir` | DeletePending 时在 Cleanup 提交删除；CloseFile 释放 context 并调用 release/releasedir。Cleanup 后的 paging I/O 继续使用核心状态。 | delete precheck 提前保证 Cleanup 完成。 | `dokan` safe trait |
| `DeleteFile` / `DeleteDirectory` | 预检查借 `lookup/getattr/access/readdir`，提交用 `unlink/rmdir` | 按官方语义执行删除预检查；目录删除检查空目录；删除提交在 Cleanup。 | 预检查路径返回错误。 | `dokan` safe trait |
| `MoveFile` | `rename` + `lookup` | 处理 `replace_if_existing`：no-replace 且目标存在时返回 collision；replace 路径调用 rename。更新 path/inode cache。 | `rename` 返回 `ENOSYS` 时返回 not implemented。 | `dokan` safe trait |
| `SetEndOfFile` | `setattr(size)` | 修改逻辑 EOF。 | `setattr` `ENOSYS` 报错。 | `dokan` safe trait |
| `SetAllocationSize` | `fallocate` 能力；adapter 策略 | Dokan 区分 allocation size 与 EOF；adapter 对 `ENOSYS` 后端采用明确降级与能力协商错误策略。 | 降级策略失败时报错。 | `dokan` safe trait + fuser capability |
| `SetFileAttributes` / `SetFileTime` | `setattr` | attributes/time 映射到 fuser 表达字段；其余属性按策略忽略、拒绝，并文档化。 | `setattr` `ENOSYS` 报错。 | `dokan` safe trait |
| `GetFileInformation` | `getattr` | `FileAttr` 映射为 Dokan `FileInfo`；ino/nlink/time/size/type 保持稳定。 | `getattr` `ENOSYS` 报错。 | `dokan` safe trait |
| `FindFiles` | `readdir` + attr cache/`getattr` | 输出完整 Dokan find data；目录分页用 fuser offset 与 adapter snapshot 维护。 | `readdir` `ENOSYS` 报错。 | `dokan` safe trait |
| `FindFilesWithPattern` | `readdirplus` 能力；`readdir` 路径 | `readdirplus` 返回 `ENOSYS` 时使用 `readdir` + Dokan pattern 过滤。 | `readdir` 错误按 NTSTATUS 返回。 | `dokan` safe trait |
| `GetDiskFreeSpace` | `statfs` | `statfs` 提供卷容量；`ENOSYS` 转换为 Dokan 错误与明确降级策略。 | `ENOSYS` 返回 Dokan 错误。 | `dokan` safe trait |
| `GetVolumeInformation` | fuser none | 从 mount config/adapter state 派生卷名、文件系统名、serial、max component length。 | adapter 模拟。 | `dokan` safe trait |
| `GetFileSecurity` / `SetFileSecurity` | fuser none；借 `FileAttr` + xattr | 目标是 POSIX 转完整 Windows ACL：`user.dokan.security_descriptor` 提供 Windows security descriptor；uid/gid/perm 提供合成 descriptor；`SetFileSecurity` 写回该 xattr。 | xattr 返回 `ENOSYS` 时降级为合成只读 descriptor 与能力协商错误，按模式记录。 | `dokan-sys`/Win32 SD helper |
| `LockFile` / `UnlockFile` | `getlk` / `setlk` 能力 | `auto_probe` 与特定挂载选项触发时探测并启用；owner/range 按 Dokan 官方语义映射。 | `ENOSYS` 走能力协商错误。 | `dokan` safe trait |
| `FindStreams` / ADS | xattr 能力 | 挂载启用 Dokan `ALT_STREAM`；运行时把 `user.*` xattr 映射为 ADS，枚举时包含 unnamed `::$DATA`。 | xattr `ENOSYS` 时具体 stream 走能力协商错误与空枚举策略。 | mount flag 由挂载后端设置；枚举由 safe trait 路由承载 |
| symlink / hardlink / readlink | `readlink` / `symlink` / `link` 能力 | link-family 回调经请求层路由到内部实现；完整 reparse/hardlink 支持归入 `dokan-sys` 后端。 | 请求层 `ENOSYS` 走能力协商错误。 | `dokan-sys`/请求层 |
| notify / cache invalidation | fuser `Notifier` surface + Dokan notify API | TTL cache 变化、create/delete/rename/update/xattr update 时同步更新 resolver/cache；Dokan 原生通知由 `dokan-sys` 后端承载。 | notify 失败按调用面错误语义处理；resolver/cache 失效保持 adapter 内部一致。 | resolver 失效走 safe trait；Dokan 原生通知走 `dokan-sys` |
| ioctl/poll/bmap/lseek/copy_file_range | fuser-only 能力 | Dokan 对这些 fuser-only 入口走独立控制面设计。 | `ENOSYS`。 | control-plane design |

## 3. 路径、inode、TTL 与 forget

路径解析使用 TTL-aware resolver：

1. `PathResolver` 规范化 Windows 路径，维护 path -> `(parent, name, ino, generation)`，path-only 路径入口从 root 逐级解析并复用未过期 entry TTL。
2. resolver 内部维护 ino -> attr/state、lookup refcount、attr TTL；entry TTL 与 attr TTL 分开管理，lookup entry 只维护 entry 缓存。
3. TTL 为 0 的 entry 走一次性解析；路径解析消费者拿到 inode 后，adapter 立即 best-effort 调 `forget` 释放 resolver 持有的 lookup ref。
4. rename/delete/create、entry/subtree 失效、过期回收会按影响范围清路径缓存，并对正缓存释放产生的 lookup ref 调 `forget`；setattr/write 主要失效受影响 inode/parent 的 attr TTL，必要时清对应路径缓存。
5. 负缓存用于 ENOENT 路径，TTL 为 1 秒；`MountOption::CUSTOM("negative_ttl=0")` / `negative_ttl=off` 关闭负缓存，`negative_ttl_ms=N` 调整 TTL。`Notifier::inval_entry` 会同步清 resolver 中对应的负缓存。

handle-bound 请求使用 `AdapterContext` 中保存的 ino/fh/flags；contextless 路径入口经过 resolver。

## 4. `auto_probe` 策略

`MountOption::CUSTOM("auto_probe")` 显式开启能力探测。挂载选项定义所需 fuser 入口。

| 能力 | 挂载规则 | `auto_probe` 行为 |
|---|---|---|
| ADS / xattr | Dokan `ALT_STREAM` 由挂载配置启用；具体 stream 请求在 xattr 返回 `ENOSYS` 时走能力协商错误。 | 缓存 xattr capability，`user.*` 映射 ADS。 |
| locks | lock 请求按能力协商错误响应。 | 首次 lock 路径探测 `getlk/setlk`，成功后缓存支持。 |
| readdirplus | 目录枚举使用 `readdir`。 | 探测 `readdirplus`，`ENOSYS` 结果缓存为 `readdir` 路径。 |
| fallocate/allocation | allocation-size 使用 adapter 降级策略。 | 探测 `fallocate` 支持后用于 allocation-size 降级。 |

探测结果只缓存“capability-negotiated = `ENOSYS`”。`EPERM/EACCES` 作为真实拒绝处理。
