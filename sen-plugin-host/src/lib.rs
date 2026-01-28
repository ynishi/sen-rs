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
//! use sen_plugin_host::{PluginLoader, RouterPluginExt};
//! use sen::Router;
//!
//! let loader = PluginLoader::new()?;
//! let plugin = loader.load(&wasm_bytes)?;
//!
//! let router = Router::new()
//!     .plugin(plugin)  // Register plugin as route
//!     .with_state(state);
//! ```

pub mod loader;

#[cfg(feature = "sen-integration")]
pub mod bridge;

pub use loader::{LoadedPlugin, LoaderError, PluginInstance, PluginLoader};
pub use sen_plugin_api::{ArgSpec, CommandSpec, ExecuteError, ExecuteResult, PluginManifest};

#[cfg(feature = "sen-integration")]
pub use bridge::{generate_plugin_help, register_plugins_from_spec, RouterPluginExt, WasmHandler};
