//! WASI Integration for Plugin Sandboxing
//!
//! This module provides WebAssembly System Interface (WASI) integration,
//! enabling plugins to access system resources (filesystem, environment, stdio)
//! in a controlled, capability-based manner.
//!
//! # Architecture Overview
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         WASI Integration Layer                          │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                         │
//! │  ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐  │
//! │  │   Capabilities  │ ──► │  WasiConfigurer │ ──► │    WasiCtx      │  │
//! │  │  (from manifest)│     │  (this module)  │     │ (wasmtime-wasi) │  │
//! │  └─────────────────┘     └─────────────────┘     └─────────────────┘  │
//! │                                                                         │
//! │  Capabilities:           WasiConfigurer:         WasiCtx:              │
//! │  - fs_read: [./data]     - Validates paths       - preopened_dir()    │
//! │  - fs_write: [./out]     - Resolves symlinks     - env()              │
//! │  - env_read: [HOME]      - Applies sandbox       - inherit_stdio()    │
//! │  - stdio: stdout         - Builds WasiCtx        - (configured)       │
//! │                                                                         │
//! └─────────────────────────────────────────────────────────────────────────┘
//!                                    │
//!                                    ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Plugin Execution                                │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                         │
//! │  1. Permission Check (permission module)                                │
//! │     └─► Strategy.check(capabilities) → Allow/Deny/Prompt               │
//! │                                                                         │
//! │  2. WASI Context Build (this module)                                    │
//! │     └─► WasiConfigurer::build(capabilities) → WasiCtx                  │
//! │                                                                         │
//! │  3. Store Creation with WASI                                            │
//! │     └─► Store::new(engine, WasiCtx)                                    │
//! │                                                                         │
//! │  4. Plugin Execution                                                    │
//! │     └─► Plugin can access ONLY granted resources                       │
//! │                                                                         │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Security Model
//!
//! WASI integration follows the **Principle of Least Privilege**:
//!
//! | Resource | Default | With Capability |
//! |----------|---------|-----------------|
//! | Filesystem Read | Denied | Allowed for declared paths only |
//! | Filesystem Write | Denied | Allowed for declared paths only |
//! | Environment Vars | Denied | Allowed for declared patterns |
//! | Stdin | Denied | Allowed if `stdio.stdin = true` |
//! | Stdout | Denied | Allowed if `stdio.stdout = true` |
//! | Stderr | Denied | Allowed if `stdio.stderr = true` |
//! | Network | Denied | Future: WASI Preview 2 |
//! | Clock/Random | Allowed | Always available (safe) |
//!
//! ## Path Security
//!
//! All filesystem paths undergo strict validation:
//!
//! 1. **Canonicalization**: Paths are resolved to absolute form
//! 2. **Symlink Resolution**: Symlinks are followed and validated
//! 3. **Traversal Prevention**: `../` patterns outside allowed paths are blocked
//! 4. **Sandbox Enforcement**: Access is restricted to declared directories
//!
//! ```text
//! Requested: ./data/../secret
//!                    │
//!                    ▼
//! Canonicalized: /home/user/project/secret
//!                    │
//!                    ▼
//! Allowed paths: [/home/user/project/data]
//!                    │
//!                    ▼
//! Result: DENIED (path escapes sandbox)
//! ```
//!
//! # Capability to WASI Mapping
//!
//! ```text
//! sen-plugin-api::Capabilities    ──►    wasmtime-wasi::WasiCtxBuilder
//! ─────────────────────────────────────────────────────────────────────
//!
//! fs_read: ["./data"]             ──►    preopened_dir("./data", DirPerms::READ)
//!
//! fs_write: ["./out"]             ──►    preopened_dir("./out", DirPerms::all())
//!
//! env_read: ["HOME", "MY_*"]      ──►    env("HOME", value)
//!                                        env("MY_VAR1", value)  // expanded
//!                                        env("MY_VAR2", value)
//!
//! stdio.stdin: true               ──►    inherit_stdin()
//! stdio.stdout: true              ──►    inherit_stdout()
//! stdio.stderr: true              ──►    inherit_stderr()
//!
//! net: ["api.example.com"]        ──►    (Not yet implemented - Preview 2)
//! ```
//!
//! # Usage
//!
//! ## Basic Usage (with Permission System)
//!
//! ```rust,ignore
//! use sen_plugin_host::{PluginRegistry, PermissionPresets};
//! use sen_plugin_host::wasi::WasiConfigurer;
//!
//! // 1. Create registry with permissions
//! let registry = PluginRegistry::with_permissions(
//!     PermissionPresets::interactive("myapp")?
//! )?;
//!
//! // 2. Load plugin (capabilities are in manifest)
//! registry.load_plugin("./plugins/data-export.wasm").await?;
//!
//! // 3. Execute (WASI context is built automatically based on capabilities)
//! // - If not yet permitted: user is prompted
//! // - If permitted: WasiCtx is configured with declared capabilities
//! let result = registry.execute("data-export", &[]).await?;
//! ```
//!
//! ## Manual WASI Configuration
//!
//! ```rust,ignore
//! use sen_plugin_host::wasi::{WasiConfigurer, WasiConfig};
//! use sen_plugin_api::Capabilities;
//!
//! // Define capabilities
//! let caps = Capabilities::default()
//!     .with_fs_read(vec![PathPattern::new("./data").recursive()])
//!     .with_fs_write(vec![PathPattern::new("./output")])
//!     .with_env_read(vec!["HOME".into(), "PATH".into()])
//!     .with_stdio(StdioCapability::stdout_stderr());
//!
//! // Build WASI context
//! let wasi_ctx = WasiConfigurer::new()
//!     .with_capabilities(&caps)
//!     .with_working_directory(std::env::current_dir()?)
//!     .build()?;
//!
//! // Use with wasmtime Store
//! let mut store = Store::new(&engine, wasi_ctx);
//! ```
//!
//! # Network Access (Future)
//!
//! Network access via WASI is planned for a future release using WASI Preview 2:
//!
//! ```text
//! Option A: WASI Preview 2 Sockets (complex, full network stack)
//! ├── wasi:sockets/tcp
//! ├── wasi:sockets/udp
//! └── wasi:sockets/dns
//!
//! Option B: Host-Proxy HTTP (simpler, HTTP-only)
//! ├── Plugin calls: host_http_request(url, method, body)
//! ├── Host validates: net capability contains url's host
//! └── Host executes: actual HTTP request
//!
//! Current recommendation: Option B for MVP (simpler, sufficient for most CLIs)
//! ```
//!
//! # Module Structure
//!
//! - [`context`]: WASI context builder from Capabilities
//! - [`sandbox`]: Path validation and sandbox enforcement
//! - [`error`]: WASI-specific error types
//!
//! # Feature Flags
//!
//! This module requires the `wasi` feature:
//!
//! ```toml
//! [dependencies]
//! sen-plugin-host = { version = "0.8", features = ["wasi"] }
//! ```

pub mod context;
pub mod error;
pub mod sandbox;

pub use context::{WasiConfig, WasiConfigurer};
pub use error::WasiError;
pub use sandbox::{SandboxConfig, SandboxValidator};
