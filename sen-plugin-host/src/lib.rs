//! sen-plugin-host: WASM plugin host runtime for sen-rs
//!
//! This crate provides the runtime for loading and executing WASM plugins.
//! It enables building plugin-extensible CLI applications with minimal effort.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      Your Application                       │
//! ├─────────────────────────────────────────────────────────────┤
//! │  PluginRegistry                                             │
//! │  ├─ load_plugin("./plugins/hello.wasm")                     │
//! │  ├─ execute("hello", &["World"])                            │
//! │  └─ list_commands()                                         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  HotReloadWatcher (optional)                                │
//! │  └─ Watches directories, auto-loads/unloads plugins         │
//! ├─────────────────────────────────────────────────────────────┤
//! │  PluginLoader                                               │
//! │  ├─ Compiles WASM modules                                   │
//! │  ├─ Validates API version                                   │
//! │  └─ Creates sandboxed instances                             │
//! ├─────────────────────────────────────────────────────────────┤
//! │  wasmtime (sandboxed execution)                             │
//! │  ├─ CPU limits (fuel)                                       │
//! │  ├─ Stack limits (1MB)                                      │
//! │  └─ Memory isolation                                        │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Features
//!
//! - **Plugin Loading**: Load and execute WASM plugins with sandboxed execution
//! - **Hot Reload**: Automatically reload plugins when files change
//! - **Plugin Discovery**: Scan directories for plugin files
//! - **CPU Limits**: Prevent infinite loops with fuel-based execution limits
//! - **Stack Limits**: Prevent stack overflow attacks (1MB limit)
//! - `sen-integration`: Enable integration with sen-rs Router (adds `bridge` module)
//!
//! # Security
//!
//! Plugins run in a sandboxed WASM environment with multiple protections:
//!
//! | Protection | Description |
//! |------------|-------------|
//! | CPU Limit | 10M instructions per execution (fuel) |
//! | Stack Limit | 1MB maximum WASM stack |
//! | Memory Isolation | Each plugin has isolated linear memory |
//! | No System Access | No filesystem, network, or OS access |
//! | API Versioning | Rejects plugins with incompatible API versions |
//!
//! # Quick Start
//!
//! ## Building Plugins
//!
//! See the `sen-plugin-sdk` crate for detailed plugin development guide.
//!
//! ```bash
//! # Add the WASM target (one-time setup)
//! rustup target add wasm32-unknown-unknown
//!
//! # Build plugin
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! ## Loading and Executing Plugins
//!
//! ### Direct Loading
//!
//! ```rust,ignore
//! use sen_plugin_host::PluginLoader;
//!
//! let loader = PluginLoader::new()?;
//! let wasm_bytes = std::fs::read("plugin.wasm")?;
//! let mut plugin = loader.load(&wasm_bytes)?;
//!
//! // Check plugin metadata
//! println!("Command: {}", plugin.manifest.command.name);
//! println!("Description: {}", plugin.manifest.command.about);
//!
//! // Execute
//! let result = plugin.instance.execute(&["World".to_string()])?;
//! match result {
//!     ExecuteResult::Success(output) => println!("{}", output),
//!     ExecuteResult::Error(err) => eprintln!("Error: {}", err.message),
//! }
//! ```
//!
//! ### Using Registry (Recommended)
//!
//! ```rust,ignore
//! use sen_plugin_host::PluginRegistry;
//!
//! let registry = PluginRegistry::new()?;
//!
//! // Load plugins
//! registry.load_plugin("./plugins/hello.wasm").await?;
//! registry.load_plugin("./plugins/greet.wasm").await?;
//!
//! // List available commands
//! for cmd in registry.list_commands().await {
//!     println!("  {}", cmd);
//! }
//!
//! // Execute by command name
//! let result = registry.execute("hello", &["World".to_string()]).await?;
//! ```
//!
//! ## Hot Reload
//!
//! Automatically load/reload/unload plugins when files change:
//!
//! ```rust,ignore
//! use sen_plugin_host::{PluginRegistry, HotReloadWatcher, WatcherConfig};
//! use std::time::Duration;
//!
//! let registry = PluginRegistry::new()?;
//!
//! // Watch directory for plugin changes
//! let _watcher = HotReloadWatcher::new(
//!     registry.clone(),
//!     vec!["./plugins"],
//!     WatcherConfig {
//!         debounce: Duration::from_millis(300),
//!         load_existing: true,  // Load existing plugins on start
//!     },
//! ).await?;
//!
//! // Plugins are now automatically managed:
//! // - New .wasm files are loaded
//! // - Modified .wasm files are reloaded
//! // - Deleted .wasm files are unloaded
//!
//! // Your application continues running...
//! loop {
//!     let cmd = read_user_input();
//!     if let Ok(result) = registry.execute(&cmd, &args).await {
//!         // Handle result
//!     }
//! }
//! ```
//!
//! ## Plugin Discovery
//!
//! Scan directories for plugins without hot reload:
//!
//! ```rust,ignore
//! use sen_plugin_host::PluginScanner;
//!
//! let scanner = PluginScanner::new()?;
//! let result = scanner.scan_directory("./plugins")?;
//!
//! println!("Found {} plugins", result.plugins.len());
//! for plugin in &result.plugins {
//!     println!("  - {} ({})", plugin.manifest.command.name, plugin.manifest.command.about);
//! }
//!
//! if !result.failures.is_empty() {
//!     eprintln!("Failed to load {} plugins:", result.failures.len());
//!     for (path, error) in &result.failures {
//!         eprintln!("  - {}: {}", path.display(), error);
//!     }
//! }
//! ```
//!
//! # sen-rs Router Integration
//!
//! Enable the `sen-integration` feature to integrate plugins with sen-rs Router:
//!
//! ```toml
//! [dependencies]
//! sen-plugin-host = { version = "0.7", features = ["sen-integration"] }
//! ```
//!
//! ```rust,ignore
//! use sen::Router;
//! use sen_plugin_host::{PluginLoader, RouterPluginExt};
//!
//! let loader = PluginLoader::new()?;
//! let plugin = loader.load(&wasm_bytes)?;
//!
//! // Register plugin as a router command
//! let router = Router::new()
//!     .plugin(plugin)
//!     .with_state(MyState);
//!
//! // Plugin command is now available via the router
//! let response = router.execute_with(&["app", "hello", "World"]).await;
//! ```
//!
//! # Examples
//!
//! See `examples/wasm-cli/` for a complete example of a plugin-extensible CLI
//! with hot reload support.

pub mod discovery;
pub mod loader;
pub mod registry;
pub mod watcher;

#[cfg(feature = "sen-integration")]
pub mod bridge;

pub use discovery::{default_plugin_dirs, DiscoveryError, DiscoveryResult, PluginScanner};
pub use loader::{LoadedPlugin, LoaderError, PluginInstance, PluginLoader};
pub use registry::{PluginRegistry, RegistryError};
pub use sen_plugin_api::{ArgSpec, CommandSpec, ExecuteError, ExecuteResult, PluginManifest};
pub use watcher::{HotReloadWatcher, WatcherConfig, WatcherError};

#[cfg(feature = "sen-integration")]
pub use bridge::{generate_plugin_help, register_plugins_from_spec, RouterPluginExt, WasmHandler};
