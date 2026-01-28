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
//! use sen_plugin_host::{PluginLoader, PluginScanner, RouterPluginExt};
//! use sen::Router;
//!
//! // Scan directory for plugins
//! let scanner = PluginScanner::new()?;
//! let result = scanner.scan_directory("./plugins")?;
//!
//! // Register all discovered plugins
//! let mut router = Router::new();
//! for plugin in result.plugins {
//!     router = router.plugin(plugin);
//! }
//! let router = router.with_state(state);
//! ```

pub mod discovery;
pub mod loader;

#[cfg(feature = "sen-integration")]
pub mod bridge;

pub use discovery::{default_plugin_dirs, DiscoveryError, DiscoveryResult, PluginScanner};
pub use loader::{LoadedPlugin, LoaderError, PluginInstance, PluginLoader};
pub use sen_plugin_api::{ArgSpec, CommandSpec, ExecuteError, ExecuteResult, PluginManifest};

#[cfg(feature = "sen-integration")]
pub use bridge::{generate_plugin_help, register_plugins_from_spec, RouterPluginExt, WasmHandler};
