# `src/fuser_facade` 对齐方案

`src/fuser_facade` 提供 fuser 0.17 兼容的下游开发表面。运行时能力由 `src/dokan_impl` 按 Dokan 官方语义调用这些 fuser 风格入口。

## 1. 基本原则

1. **API 对齐 fuser**：`src/fuser_facade` 对齐 fuser 0.17 的公开 API，包括公开导出、函数、trait 方法、类型、参数顺序、参数名、返回形状和默认行为。Dokan callback 形状属于 `src/dokan_impl`。
2. **fd 平台类型**：上游 fuser API 使用 `std::os::fd::*` 的位置，Windows facade 使用 `rustix::fd::*` 命名对应 fd 类型/trait。该规则只覆盖 Windows 标准库 fd 模块差异。
3. **运行时归属**：facade 提供下游实现入口；调用时机、错误到 NTSTATUS 的转换、能力协商结果归 `dokan_impl`。
4. **`ENOSYS` 能力协商**：除 `batch_forget` 的委托型实现外，fuser 默认方法返回或回复 `Errno::ENOSYS`。`init` 的默认 `io::Error` 携带 raw errno `ENOSYS`。`destroy` / `forget` 保持 no-op。`EPERM` / `EACCES` 表示下游基于权限或策略拒绝。
5. **能力声明表面**：`Filesystem` trait 保持 fuser 形状；能力协商来自实际调用结果。`ENOSYS` 记录协商结果；成功和业务 errno 由下游负责语义。

## 2. 能力分组

fuser 能力按 facade 入口和 Dokan 调用语义分组：

| 分组 | 定义 | facade 行为 | `dokan_impl` 解释 |
|---|---|---|---|
| fuser none | fuser trait/API 没有相关入口，例如 Windows security descriptor 完整结构、Dokan handle context、path/inode cache。 | 保持 fuser API 表面。 | `dokan_impl` 模拟、派生或内部保存。 |
| fuser core | Dokan 浏览、读取、打开生命周期使用的 fuser 入口。 | 保留 fuser 方法和 reply 形状；默认 `ENOSYS`。 | `ENOSYS` 转换为 Dokan 错误。 |
| fuser mutation | RW 挂载中的 create/write/unlink/rmdir/rename/setattr 等入口。 | 保留 fuser 方法和 reply 形状；默认 `ENOSYS`。 | 真实写请求中的 `ENOSYS` 转换为 Dokan 错误。 |
| fuser negotiated | xattr/ADS、locks、link-family、fuser-only 高级能力等按业务启用入口。 | 保留入口和默认 `ENOSYS`。 | `auto_probe` 与挂载选项记录能力协商结果。 |

“core / mutation / negotiated” 是 Dokan 调用语义分组，不改变 Rust trait 的默认方法形状。

## 3. `MountOption` 对能力分组的影响

`src/dokan_impl/mount_options.rs` 接收和解释挂载选项，并派生 Dokan 调用语义。

| 挂载条件 | fuser 入口 | 说明 |
|---|---|---|
| 所有挂载 | `lookup`、`getattr`、`readdir`、`read`、`open`、`opendir`、`release`、`releasedir`、`init`。 | 最小浏览/读取路径。`destroy` / `forget` no-op，`batch_forget` 委托逐项 forget，其他默认 `ENOSYS`。 |
| `MountOption::RO` | 写入、删除、rename、setattr 入口保持 fuser 形状。 | 写相关 Dokan 请求由 `dokan_impl` 按只读卷官方语义拒绝。 |
| RW | `create`、`mkdir`、`write`、`unlink`、`rmdir`、`rename`、`setattr`。 | 真实写请求直接调用下游入口，`ENOSYS` 转换为 Dokan 错误。 |
| `DefaultPermissions` | `access`。 | Windows ACL 与 POSIX mode 的调用策略由 `dokan_impl` 文档定义。 |
| `CUSTOM("auto_probe")` | xattr/ADS、locks、link-family、fuser-only 高级能力。 | `dokan_impl` 探测并记录能力协商结果。 |

## 4. 探测与错误语义

能力协商通过调用 fuser 方法并观察 reply：

1. `ENOSYS`：记录能力协商结果。
2. `EPERM` / `EACCES`：记录下游权限或策略拒绝。
3. 成功：记录该对象路径上的入口语义由下游承载。
4. 其他 errno：按业务错误转换为 NTSTATUS。

`CUSTOM("auto_probe")` 驱动能力探测。探测发生在首次真实请求或安全验证点；有副作用的方法使用真实请求结果完成协商。

## 5. facade 入口形状

| fuser 入口组 | 对齐要求 | 默认行为 |
|---|---|---|
| lookup / attr / dir / read | 保持 fuser 0.17 参数和 reply 类型。 | `lookup`、`getattr`、`readdir`、`read` 默认 `ENOSYS`。 |
| open / opendir / release / releasedir | 保持 fuser 0.17 参数和 reply 类型。 | 默认 `ENOSYS`；下游实现表示打开和关闭成功。 |
| write / create / mutation | 保持 fuser 入口，不增加 Dokan 参数。 | 默认 `ENOSYS`。Dokan 的 `WriteToEndOfFile`、`PagingIo`、delete two-phase 由 `dokan_impl` 适配。 |
| xattr | 保持 fuser xattr 入口。 | 默认 `ENOSYS`。`user.*` 由 `dokan_impl` 映射为 ADS。 |
| lock | 保持 `getlk` / `setlk`。 | 默认 `ENOSYS`。`dokan_impl` 通过能力协商决定用户态锁语义。 |
| link-family | `readlink` / `symlink` / `link` 保持 fuser 形状。 | 默认统一 `ENOSYS`；`EPERM` 表示权限拒绝。 |
| fuser-only 高级能力 | `ioctl`、`poll`、`bmap`、`lseek`、`copy_file_range`、`fallocate` 等继续保留。 | 默认 `ENOSYS`；Dokan 控制面由 `dokan_impl` 设计。 |
| lifecycle / statfs | `init` 保持 `io::Result<()>`，`statfs` 保持 `ReplyStatfs`。 | `init` 默认返回 `ENOSYS` errno；`statfs` 默认回复 `ENOSYS`。 |

## 6. resolver、TTL 与 notifier 归属

`src/fuser_facade` 暴露 fuser reply TTL 和 `Notifier` 表面；`src/dokan_impl` 持有 TTL-aware resolver、path/inode/attr cache、负缓存和 Dokan 原生通知桥接。

- `ReplyEntry` TTL 驱动 name -> inode entry cache；`ReplyAttr` TTL 驱动 attr cache。
- TTL 为 0 的 entry 走一次性解析；adapter 在消费者拿到 inode 后 best-effort `forget` resolver 持有的 lookup ref。
- `negative_ttl=0` / `negative_ttl=off` 关闭 ENOENT 负缓存，`negative_ttl_ms=N` 调整负缓存 TTL。
- `Notifier::inval_entry` 同步清 resolver 中对应正缓存和负缓存；Dokan 原生 notify API 由 `dokan-sys` 后端承载。
