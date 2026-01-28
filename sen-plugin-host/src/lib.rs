//! sen-plugin-host: Wasm plugin host runtime for sen-rs
//!
//! This crate provides the runtime for loading and executing Wasm plugins.
//!
//! ## Features
//!
//! - `sen-integration`: Enable integration with sen-rs Router (adds `bridge` module)
//!
//! ## Example
//!
//! ```rust,ignore
//! use sen_plugin_host::{PluginRegistry, HotReloadWatcher, WatcherConfig};
//!
//! // Create registry and start hot reload watcher
//! let registry = PluginRegistry::new()?;
//! let watcher = HotReloadWatcher::new(
//!     registry.clone(),
//!     vec!["./plugins"],
//!     WatcherConfig::default(),
//! ).await?;
//!
//! // Execute a plugin command
//! let result = registry.execute("hello", &["World".to_string()]).await?;
//! ```

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
