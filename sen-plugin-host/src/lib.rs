//! sen-plugin-host: Wasm plugin host runtime for sen-rs
//!
//! This crate provides the runtime for loading and executing Wasm plugins.

pub mod loader;

pub use loader::{LoadedPlugin, LoaderError, PluginInstance, PluginLoader};
pub use sen_plugin_api::{ArgSpec, CommandSpec, ExecuteError, ExecuteResult, PluginManifest};
