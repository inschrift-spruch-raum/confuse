# confuse

`confuse` is a Rust filesystem compatibility layer. On non-Windows platforms it re-exports [`fuser`](https://crates.io/crates/fuser). On Windows it provides a fuser 0.17-style API facade backed by [Dokan](https://dokan-dev.github.io/) for runtime mounting.

The goal is to let downstream filesystem implementations target one fuser-compatible API while still compiling, testing, and running on Linux, macOS, and Windows.

## Features

- Non-Windows: `pub use fuser::*`, preserving upstream fuser behavior.
- Windows: exposes fuser 0.17-compatible public types, replies, the `Filesystem` trait, `mount2`, `spawn_mount2`, `Session`, and notifier APIs.
- Windows backend: adapts fuser-style callbacks to Dokan requests, mount options, path resolution, TTL caches, and notification bridges.
- Example: `examples/memfs` contains a cross-platform in-memory filesystem.
- Compatibility tests: `tests/api_surface` records and verifies the fuser 0.17 API alignment scope.

## Platform behavior

| Platform | Behavior |
|---|---|
| Linux / macOS / other non-Windows targets | Uses the `fuser` crate API and runtime directly. |
| Windows | Uses this repository's `fuser_facade` API and mounts through `dokan` / `dokan-sys`. |

The Windows facade has a few intentional differences. Upstream fuser APIs that use `std::os::fd::*` cannot name those types on Windows, so the facade names the corresponding `rustix::fd::*` types directly. Unsupported `/dev/fuse` fd runtime surfaces return `io::ErrorKind::Unsupported`.

## Installation and prerequisites

### Rust

This repository uses Rust edition 2024. `Cargo.toml` declares a minimum supported Rust version of `1.96`.

### Windows

Runtime mounting requires the Dokan runtime. Windows development and testing use these dependencies:

- `dokan`
- `dokan-sys`
- `rustix`
- `widestring`
- `winapi`

### Non-Windows

Non-Windows targets depend on `fuser`. Real FUSE mounts usually also require system FUSE/libfuse support.

## Quick start

After adding the dependency, implement `Filesystem` as you would with fuser:

```rust
use confuse::{Config, Filesystem, mount2};

struct MyFs;

impl Filesystem for MyFs {}

fn main() -> std::io::Result<()> {
    let mountpoint = std::env::args_os()
        .nth(1)
        .expect("missing mountpoint");

    mount2(MyFs, mountpoint, &Config::default())
}
```

Run the bundled in-memory filesystem example:

```sh
cargo run --example memfs -- <mountpoint>
```

On Windows, `<mountpoint>` can be a drive letter such as `M:\` or an empty directory. On Linux/macOS, it is usually an existing mount directory.

## Common commands

```sh
cargo check
cargo test
cargo run --example memfs -- <mountpoint>
```

Enable specific compatibility features when needed:

```sh
cargo test --features macos-api
cargo test --features serializable
```

## Cargo features

This crate forwards or provides these main features:

- `abi-7-20` through `abi-7-40`: forwards fuser ABI features.
- `experimental`: enables experimental APIs and turns on `async-trait`, `tokio`, and fuser experimental support.
- `libfuse`, `libfuse2`, `libfuse3`: forwards libfuse-related features.
- `macfuse-4-compat`, `macos-no-mount`: forwards macOS/fuser compatibility features.
- `macos-api`: exposes fuser's macOS-only API surface on the Windows facade so cross-platform implementations can compile and be tested.
- `serializable`: enables `serde` derives and forwards fuser serializable support.

## Repository layout

```text
src/lib.rs                  crate entry point; selects fuser re-export or Windows facade by platform
src/fuser_facade/           fuser 0.17-compatible public API for Windows
src/dokan_impl/             Dokan runtime adapter, mount options, path resolution, and handler routes
examples/memfs/             cross-platform in-memory filesystem example
tests/api_surface/          fuser API surface alignment tests
docs/Design/                facade and Dokan adapter design documents
docs/Standard/              fuser / dokan API architecture analysis
```

## Design documents

- `docs/Design/fuser_facade对齐方案.md`: API alignment principles for the Windows facade and fuser 0.17.
- `docs/Design/dokan_impl调用方案.md`: Dokan adapter call design.
- `docs/Design/fuser到dokan-sys转换层涉及工作.md`: fuser-to-Dokan conversion layer work breakdown.
- `docs/Standard/Rust fuser库 API架构分析.md`: fuser API architecture analysis.
- `docs/Standard/Rust dokan(-sys)库 API架构分析.md`: Dokan / dokan-sys API architecture analysis.

## License

This project is licensed under the MIT License.
