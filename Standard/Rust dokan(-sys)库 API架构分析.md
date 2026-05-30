# Rust dokan(-sys) 库 API 架构分析

> 资料范围：本文只基于公开网络资料整理，包括 dokan-rust GitHub README、dokan / dokan-sys Rustdoc、crates/docs.rs 页面、Dokany GitHub README 与 Dokany Doxygen API 文档。未使用任何本地私有代码或未公开实现。

## 1. 结论摘要

Rust 生态中的 `dokan` / `dokan-sys` 是对 Windows Dokany 用户态文件系统能力的两层封装：

- `dokan-sys` 是底层 FFI crate，暴露 Dokany C API 的结构体、常量、函数指针表和 unsafe 函数，目标是“像使用原生 Dokan 一样使用”。
- `dokan` 构建在 `dokan-sys` 之上，提供 Rust 风格的 `Drive` 构建器、`FileSystemHandler` trait、`OperationInfo` 操作上下文、`OperationError` 错误转换和若干通知/工具函数。
- 上游 Dokany 的核心模型是：Windows I/O 请求进入内核驱动 `dokan2.sys`，驱动通过用户态 DLL `dokan2.dll` 调用应用注册的 `DOKAN_OPERATIONS` 回调，应用返回 `NTSTATUS` 结果。
- 因此，`dokan` 的 API 架构本质是把 C 侧 `DOKAN_OPTIONS + DOKAN_OPERATIONS + DokanMain` 模型，重塑为 Rust 侧 `Drive::mount(handler)` + `FileSystemHandler` trait 回调模型。

## 2. 背景：Dokany 的运行架构

Dokany 是 Windows 上类似 FUSE 的用户态文件系统库。公开 README 描述其组成包含用户态 DLL `dokan2.dll` 与内核模式文件系统驱动 `dokan2.sys`：

```text
应用程序 / Windows Explorer
        |
        v
Windows I/O 子系统
        |
        v
Dokan 内核驱动 dokan2.sys
        |
        v
Dokan 用户态库 dokan2.dll
        |
        v
文件系统应用注册的回调函数
```

上游 Doxygen 文档说明，文件系统应用需要填充：

- `DOKAN_OPTIONS`：描述挂载点、线程数、超时、设备行为标志等。
- `DOKAN_OPERATIONS`：一组文件系统操作回调，例如 `ZwCreateFile`、`ReadFile`、`WriteFile`、`FindFiles`、`Cleanup`、`CloseFile` 等。
- 调用 `DokanMain(options, operations)` 完成挂载；`DokanMain` 会阻塞直到卸载。Dokany 2.x 还提供 `DokanCreateFileSystem`、`DokanWaitForFileSystemClosed`、`DokanCloseHandle` 等异步挂载/生命周期 API。

关键约束：Dokany 文档明确指出 `DOKAN_OPERATIONS` 回调会被多个线程调用，除非启用单线程选项，因此文件系统实现必须线程安全。

## 3. crate 分层

### 3.1 `dokan-sys`：原始 FFI 层

公开 Rustdoc 将 `dokan_sys` 描述为 “Raw FFI bindings for Dokan”。其主要 API 面如下：

| 类别 | 代表 API | 作用 |
|---|---|---|
| C 结构体镜像 | `DOKAN_OPTIONS`, `DOKAN_OPERATIONS`, `DOKAN_FILE_INFO`, `DOKAN_IO_SECURITY_CONTEXT`, `DOKAN_ACCESS_STATE`, `DOKAN_CONTROL` | 与 Dokany C 头文件对齐的数据结构 |
| 指针别名 | `PDOKAN_OPTIONS`, `PDOKAN_OPERATIONS`, `PDOKAN_FILE_INFO`, `PFillFindData`, `PFillFindStreamData` | 匹配 C ABI 的指针类型 |
| 挂载/卸载 | `DokanMain`, `DokanUnmount`, `DokanRemoveMountPoint` | 启动和移除文件系统挂载 |
| 信息查询 | `DokanVersion`, `DokanDriverVersion`, `DokanGetMountPointList`, `DokanReleaseMountPointList` | 查询库/驱动版本和挂载点 |
| 操作辅助 | `DokanMapKernelToUserCreateFileFlags`, `DokanNtStatusFromWin32`, `DokanIsNameInExpression`, `DokanResetTimeout`, `DokanOpenRequestorToken` | 标志转换、错误转换、通配符匹配、超时延长、请求者 token |
| 通知 API | `DokanNotifyCreate`, `DokanNotifyDelete`, `DokanNotifyRename`, `DokanNotifyUpdate`, `DokanNotifyXAttrUpdate` | 通知 Dokan 文件变化 |
| 挂载标志常量 | `DOKAN_OPTION_NETWORK`, `DOKAN_OPTION_REMOVABLE`, `DOKAN_OPTION_MOUNT_MANAGER`, `DOKAN_OPTION_WRITE_PROTECT`, `DOKAN_OPTION_ALT_STREAM`, `DOKAN_OPTION_CASE_SENSITIVE` 等 | 控制挂载卷行为 |

架构定位：`dokan-sys` 不隐藏 unsafe，也不抽象文件系统语义。它适合需要完全控制 C ABI、对齐原生 Dokany 文档、或高层 crate 未覆盖某些新 API 时使用。

### 3.2 `dokan`：Rust 友好封装层

公开 README 和 Rustdoc 均说明 `dokan` 构建在 `dokan-sys` 之上，并推荐一般用户优先使用 `dokan`。它的核心抽象是：

```text
Drive builder
    -> 生成 DOKAN_OPTIONS
    -> 将 FileSystemHandler trait 适配为 DOKAN_OPERATIONS 函数指针表
    -> 调用 dokan-sys::DokanMain
    -> Dokany 回调进入 Rust trait 方法
```

主要公开类型：

| 类型 | 架构角色 |
|---|---|
| `Drive` | 挂载配置构建器；设置线程数、挂载标志、挂载点、UNC 名称、超时、扇区大小、分配单元大小，并通过 `mount` 阻塞挂载 |
| `FileSystemHandler` | 文件系统能力接口；实现者处理打开、读写、枚举、元数据、安全描述符、挂载/卸载等回调 |
| `OperationInfo` | 单次操作上下文视图；读取进程 ID、是否目录、是否 delete-on-close、paging I/O、同步 I/O、no-cache、write-to-EOF、挂载配置，并可 reset timeout / 获取 requester token |
| `CreateFileInfo<T>` | `create_file` 的返回信息，承载打开对象上下文等 |
| `FileInfo`, `FindData`, `FindStreamData`, `DiskSpaceInfo`, `VolumeInfo` | Rust 侧结果结构，分别对应文件元数据、目录项、数据流、磁盘空间、卷信息 |
| `MountFlags` | 对 Dokany 挂载标志的 Rust 封装 |
| `OperationError` | 回调错误类型，支持 `NtStatus(NTSTATUS)` 与 `Win32(DWORD)`，最终转换为 Dokany 接受的 `NTSTATUS` |
| `MountError` | `Drive::mount` 的挂载错误类型 |

## 4. 核心 API 调用链

### 4.1 挂载路径

典型使用路径：

1. 用户定义一个实现 `FileSystemHandler` 的类型。
2. 通过 `Drive::new()` 创建挂载构建器。
3. 配置 `thread_count`、`flags`、`mount_point`、`unc_name`、`timeout`、`allocation_unit_size`、`sector_size` 等。
4. 调用 `Drive::mount(&handler)`。
5. 高层封装把 Rust handler 适配为 `DOKAN_OPERATIONS` 函数指针表，并通过底层 `DokanMain` 进入 Dokany 挂载生命周期。

公开 Rustdoc 指出 `Drive::mount` 会阻塞当前线程，直到卷被卸载。这与 Dokany C API 中 `DokanMain` 的阻塞语义一致。

### 4.2 I/O 请求路径

一次读请求可抽象为：

```text
Windows ReadFile
  -> Windows I/O subsystem
  -> dokan2.sys
  -> dokan2.dll
  -> DOKAN_OPERATIONS::ReadFile
  -> dokan crate 适配层
  -> FileSystemHandler::read_file(file_name, offset, buffer, info, context)
  -> Result<u32, OperationError>
  -> NTSTATUS + 已读字节数
```

写请求、目录枚举、元数据查询、设置属性等均遵循类似路径。

### 4.3 对象生命周期

Dokany C 文档强调每个用户态文件句柄关联一个 `DOKAN_FILE_INFO`，其中 `Context` 可保存应用侧数据，并应在 `Cleanup` / `CloseFile` 阶段释放。

`dokan` 将这个模式 Rust 化：

- `FileSystemHandler` 有关联类型 `Context: Sync`，表示打开文件对象的上下文。
- `create_file` 返回 `CreateFileInfo<Self::Context>`。
- 后续 `read_file`、`write_file`、`get_file_information` 等方法接收 `&Self::Context`。
- `cleanup` 用于最后一个文件句柄关闭前的清理逻辑，例如处理 `delete_on_close`。
- `close_file` 是对象生命周期末尾，用于安全释放资源；公开文档说明 context 会在此方法返回后 drop。

这使 C 侧裸 `ULONG64 Context` / 指针生命周期转化为 Rust 侧有类型的上下文生命周期，但用户仍需保证上下文内部并发访问安全。

## 5. `FileSystemHandler` 回调族

公开 Rustdoc 中 `FileSystemHandler` 的方法与 Dokany `DOKAN_OPERATIONS` 基本一一对应。可以分为几组：

### 5.1 打开与生命周期

- `create_file`：对应 `ZwCreateFile`；处理打开/创建文件或目录。必要回调，缺失会导致文件系统不可用。
- `cleanup`：最后一个 handle 关闭前调用；若 `OperationInfo::delete_on_close()` 为真，应在此执行删除。
- `close_file`：文件对象生命周期末尾；释放资源。

### 5.2 数据 I/O

- `read_file`：按 offset 读取数据，返回实际读取字节数。
- `write_file`：按 offset 写入数据；若 `OperationInfo::write_to_eof()` 为真，应忽略 offset 并写到文件末尾。
- `flush_file_buffers`：刷新缓冲。

### 5.3 元数据与目录

- `get_file_information`：返回文件元数据。
- `find_files`：枚举目录项。
- `find_files_with_pattern`：带 Windows/MS-DOS 风格通配模式枚举；若返回未实现，可退回 `find_files` 并由 Dokan 过滤。
- `find_streams`：枚举备用数据流，通常与 `DOKAN_OPTION_ALT_STREAM` 相关。

### 5.4 修改操作

- `set_file_attributes`
- `set_file_time`
- `delete_file`
- `delete_directory`
- `move_file`
- `set_end_of_file`
- `set_allocation_size`
- `lock_file` / `unlock_file`

Dokany C 文档提醒：`DeleteFile` / `DeleteDirectory` 主要用于检查是否可删除，真正删除应在后续 `Cleanup` 且 delete pending 为真时完成。

### 5.5 卷与安全

- `get_disk_free_space`
- `get_volume_information`
- `mounted`
- `unmounted`
- `get_file_security`
- `set_file_security`

上游 Doxygen 指出 `GetVolumeInformation` 和 `GetDiskFreeSpace` 不保存 `DOKAN_FILE_INFO.Context`，且调用前未必发生 `ZwCreateFile`；这解释了 Rust trait 中这两个方法不接收 `context` 参数。

## 6. 错误与状态码设计

Dokany C API 以 `NTSTATUS` 作为回调错误语义。`dokan` 的 `OperationError` 将错误表示成：

- `OperationError::NtStatus(NTSTATUS)`：直接返回内核语义状态码。
- `OperationError::Win32(DWORD)`：接收 Win32 错误码，并自动转换为 `NTSTATUS`。

公开 Rustdoc 特别说明，虽然 Windows 中 `STATUS_SUCCESS` 和 `ERROR_SUCCESS` 表示成功，但它们不应出现在 `OperationError` 中；成功应使用 `Ok(...)`，错误枚举中的 success 值会被转换为 `STATUS_INTERNAL_ERROR`。

架构意义：`dokan` 将 C 回调中的“返回状态码”改成 Rust `Result<T, OperationError>`，降低误用概率，同时保留 NTSTATUS 级别的表达能力。

## 7. 并发与线程安全

Dokany Doxygen 明确指出 `DOKAN_OPERATIONS` 回调会由多个线程调用。`dokan` 在 trait 签名中体现了这一点：

- `FileSystemHandler<'a, 'b>: Sync + Sized`
- `type Context: Sync + 'a`

这表示 handler 和打开对象上下文都必须能被多线程共享引用安全访问。实际文件系统实现如果需要可变状态，通常应在 `Context` 或 handler 内部使用互斥、读写锁、原子类型或其他并发安全结构。

注意：`OperationInfo` / `Drive` 的 Rustdoc 自动 trait 显示为 `!Send` / `!Sync`，因此它们更像当前回调/构建过程的局部视图，不应被跨线程长期保存。

## 8. 构建、链接与版本耦合

dokan-rust README 描述：

- `dokan-sys` 需要原生 Dokan 库的 import library 才能链接。
- 如果存在 `DokanLibrary2_LibraryPath_{ARCH}` 环境变量，构建脚本会从该目录查找 import library；这些变量可由 Dokan 安装器设置。
- 否则，`dokan-sys` 会从捆绑的 Dokan 源码构建 import library；README 还提到 DLL 也会被构建，并可通过 `DOKAN_DLL_OUTPUT_PATH` 复制到指定目录。
- `dokan-sys` crate 版本、链接的 import library 版本、运行时加载的 Dokan library 版本应一致，否则可能出问题。

公开 docs.rs 页面显示 `dokan 1.3.1` 已 yanked；GitHub README 当前说明使用 `DokanLibrary2_LibraryPath_{ARCH}`，而较早 docs.rs 1.3.1 页面出现 `DokanLibrary1_LibraryPath_{ARCH}`，这反映了 Dokany 主版本 / Rust wrapper 版本之间存在强绑定。选型时应以目标 Dokany 主版本对应的 crate 文档和 GitHub 分支/tag 为准。

## 9. `dokan` 与 `dokan-sys` 的选型建议

| 场景 | 建议 |
|---|---|
| 实现常规用户态文件系统 | 优先 `dokan`，用 `FileSystemHandler` 避免直接管理 C 函数指针与裸指针 |
| 需要完全复刻 C 示例或使用原生 API 细节 | 使用 `dokan-sys`，但调用者负责 unsafe、ABI、生命周期和线程安全 |
| 需要访问高层 crate 尚未封装的新 Dokany API | 可在 `dokan` 为主的项目中局部使用 `dokan-sys` |
| 对版本兼容非常敏感 | 固定 `dokan` / `dokan-sys` / Dokany 安装包版本，并在部署时校验 DLL 和驱动版本 |

## 10. 主要风险点

1. **版本不匹配**：README 明确警告 crate、import library、运行时 DLL 版本不一致会带来问题。
2. **线程安全不足**：上游回调多线程执行；Rust trait 要求 `Sync`，但内部可变状态仍需正确同步。
3. **生命周期误判**：`Cleanup` 后可能仍有内存映射相关 I/O，`CloseFile` 才是最终释放点。
4. **删除语义错误**：`delete_file` / `delete_directory` 应检查可删除性，实际删除通常在 `cleanup` 且 delete pending 时完成。
5. **错误码语义混淆**：回调成功应返回 `Ok`，不能把 success code 塞进 `OperationError`。
6. **Windows 专属性**：Dokany 依赖 Windows 驱动和 DLL；该架构不具备跨平台文件系统后端属性。
7. **维护状态需要核查**：GitHub 页面显示 dokan-rust 公开仓库 star / fork 规模较小，docs.rs 上部分版本构建失败或 yanked；生产使用前应确认目标版本、issue、release/tag 和 Dokany 版本匹配。

## 11. 架构图

```text
Rust 应用
  |
  | implements
  v
FileSystemHandler trait
  |
  | mounted by
  v
Drive::mount(&handler)
  |
  | wraps / adapts
  v
dokan crate callback adapter
  |
  | calls unsafe FFI
  v
dokan-sys
  |
  | C ABI
  v
dokan2.dll / Dokany user-mode library
  |
  | driver communication
  v
dokan2.sys / kernel driver
  |
  | Windows I/O requests
  v
Windows applications
```

## 12. 公开资料来源

- dokan-rust GitHub README: https://github.com/dokan-dev/dokan-rust
- `dokan` Rustdoc: https://dokan-dev.github.io/dokan-rust-doc/html/dokan/
- `FileSystemHandler` Rustdoc: https://dokan-dev.github.io/dokan-rust-doc/html/dokan/trait.FileSystemHandler.html
- `Drive` Rustdoc: https://dokan-dev.github.io/dokan-rust-doc/html/dokan/struct.Drive.html
- `OperationInfo` Rustdoc: https://dokan-dev.github.io/dokan-rust-doc/html/dokan/struct.OperationInfo.html
- `OperationError` Rustdoc: https://dokan-dev.github.io/dokan-rust-doc/html/dokan/enum.OperationError.html
- `dokan-sys` Rustdoc: https://dokan-dev.github.io/dokan-rust-doc/html/dokan_sys/
- docs.rs crate page for `dokan 1.3.1`: https://docs.rs/crate/dokan/1.3.1
- Dokany GitHub README: https://github.com/dokan-dev/dokany
- Dokany Doxygen main docs: https://dokan-dev.github.io/dokany-doc/html/
- Dokany `DOKAN_OPERATIONS` Doxygen: https://dokan-dev.github.io/dokany-doc/html/struct_d_o_k_a_n___o_p_e_r_a_t_i_o_n_s.html
- Dokany library function group: https://dokan-dev.github.io/dokany-doc/html/group___dokan.html
