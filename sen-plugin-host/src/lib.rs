//! sen-plugin-host: Wasm plugin host runtime for sen-rs
//!
//! This crate provides the runtime for loading and executing Wasm plugins.
//! It enables building plugin-extensible CLI applications with minimal effort.
//!
//! ## Features
//!
//! - **Plugin Loading**: Load and execute WASM plugins with sandboxed execution
//! - **Hot Reload**: Automatically reload plugins when files change
//! - **Plugin Discovery**: Scan directories for plugin files
//! - **CPU Limits**: Prevent infinite loops with fuel-based execution limits
//! - `sen-integration`: Enable integration with sen-rs Router (adds `bridge` module)
//!
//! ## Quick Start
//!
//! ### Building Plugins
//!
//! Plugins must be compiled for `wasm32-unknown-unknown` target:
//!
//! ```bash
//! # Add the WASM target (one-time setup)
//! rustup target add wasm32-unknown-unknown
//!
//! # Build plugin
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! ### Loading and Executing Plugins
//!
//! ```rust,ignore
//! use sen_plugin_host::{PluginLoader, PluginRegistry};
//!
//! // Load a single plugin
//! let loader = PluginLoader::new()?;
//! let wasm_bytes = std::fs::read("plugin.wasm")?;
//! let plugin = loader.load(&wasm_bytes)?;
//!
//! // Or use registry for multiple plugins
//! let registry = PluginRegistry::new()?;
//! registry.load_plugin("./plugins/hello.wasm").await?;
//!
//! // Execute plugin
//! let result = registry.execute("hello", &["World".to_string()]).await?;
//! ```
//!
//! ### Hot Reload with Directory Watching
//!
//! ```rust,ignore
//! use sen_plugin_host::{PluginRegistry, HotReloadWatcher, WatcherConfig};
//! use std::time::Duration;
//!
//! let registry = PluginRegistry::new()?;
//!
//! // Watch directory for plugin changes
//! let watcher = HotReloadWatcher::new(
//!     registry.clone(),
//!     vec!["./plugins"],
//!     WatcherConfig {
//!         debounce: Duration::from_millis(300),
//!         load_existing: true,  // Load existing plugins on start
//!     },
//! ).await?;
//!
//! // Plugins are now automatically loaded/reloaded/unloaded
//! // when files change in ./plugins/
//!
//! // List available commands
//! let commands = registry.list_commands().await;
//!
//! // Execute a plugin command
//! let result = registry.execute("hello", &["World".to_string()]).await?;
//! ```
//!
//! ## Example
//!
//! See `examples/wasm-cli/` for a complete example of a plugin-extensible CLI.

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
