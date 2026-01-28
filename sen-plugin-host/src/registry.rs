//! Plugin registry with hot reload support
//!
//! Provides a thread-safe registry for managing loaded plugins with
//! support for dynamic addition, removal, and updates.

use crate::{LoadedPlugin, LoaderError, PluginLoader};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

/// A thread-safe registry for managing loaded plugins
#[derive(Clone)]
pub struct PluginRegistry {
    inner: Arc<RwLock<RegistryInner>>,
    loader: Arc<PluginLoader>,
}

struct RegistryInner {
    /// Plugins indexed by command name
    plugins: HashMap<String, PluginEntry>,
    /// Map from file path to command name for reload tracking
    path_to_command: HashMap<PathBuf, String>,
}

struct PluginEntry {
    plugin: LoadedPlugin,
    source_path: Option<PathBuf>,
}

impl PluginRegistry {
    /// Create a new empty plugin registry
    pub fn new() -> Result<Self, LoaderError> {
        Ok(Self {
            inner: Arc::new(RwLock::new(RegistryInner {
                plugins: HashMap::new(),
                path_to_command: HashMap::new(),
            })),
            loader: Arc::new(PluginLoader::new()?),
        })
    }

    /// Create with an existing loader
    pub fn with_loader(loader: PluginLoader) -> Self {
        Self {
            inner: Arc::new(RwLock::new(RegistryInner {
                plugins: HashMap::new(),
                path_to_command: HashMap::new(),
            })),
            loader: Arc::new(loader),
        }
    }

    /// Load and register a plugin from a file path
    pub async fn load_plugin(&self, path: impl AsRef<Path>) -> Result<String, LoaderError> {
        let path = path.as_ref();
        let wasm_bytes = tokio::fs::read(path).await.map_err(|e| {
            LoaderError::MemoryAccess(format!("Failed to read file {}: {}", path.display(), e))
        })?;

        let plugin = self.loader.load(&wasm_bytes)?;
        let command_name = plugin.manifest.command.name.clone();

        let mut inner = self.inner.write().await;

        // Remove old mapping if exists
        if let Some(old_cmd) = inner.path_to_command.remove(path) {
            inner.plugins.remove(&old_cmd);
        }

        // Add new mappings
        inner
            .path_to_command
            .insert(path.to_path_buf(), command_name.clone());
        inner.plugins.insert(
            command_name.clone(),
            PluginEntry {
                plugin,
                source_path: Some(path.to_path_buf()),
            },
        );

        tracing::info!(command = %command_name, path = %path.display(), "Plugin loaded");
        Ok(command_name)
    }

    /// Register a pre-loaded plugin (without file path tracking)
    pub async fn register(&self, plugin: LoadedPlugin) -> String {
        let command_name = plugin.manifest.command.name.clone();

        let mut inner = self.inner.write().await;
        inner.plugins.insert(
            command_name.clone(),
            PluginEntry {
                plugin,
                source_path: None,
            },
        );

        tracing::info!(command = %command_name, "Plugin registered");
        command_name
    }

    /// Unload a plugin by file path
    pub async fn unload_by_path(&self, path: impl AsRef<Path>) -> Option<String> {
        let path = path.as_ref();
        let mut inner = self.inner.write().await;

        if let Some(command_name) = inner.path_to_command.remove(path) {
            inner.plugins.remove(&command_name);
            tracing::info!(command = %command_name, path = %path.display(), "Plugin unloaded");
            Some(command_name)
        } else {
            None
        }
    }

    /// Unload a plugin by command name
    pub async fn unload(&self, command_name: &str) -> bool {
        let mut inner = self.inner.write().await;

        if let Some(entry) = inner.plugins.remove(command_name) {
            if let Some(path) = entry.source_path {
                inner.path_to_command.remove(&path);
            }
            tracing::info!(command = %command_name, "Plugin unloaded");
            true
        } else {
            false
        }
    }

    /// Reload a plugin from its source path
    pub async fn reload_by_path(&self, path: impl AsRef<Path>) -> Result<String, LoaderError> {
        self.load_plugin(path).await
    }

    /// Get a list of all registered command names
    pub async fn list_commands(&self) -> Vec<String> {
        let inner = self.inner.read().await;
        inner.plugins.keys().cloned().collect()
    }

    /// Check if a command exists
    pub async fn has_command(&self, command_name: &str) -> bool {
        let inner = self.inner.read().await;
        inner.plugins.contains_key(command_name)
    }

    /// Execute a plugin command
    pub async fn execute(
        &self,
        command_name: &str,
        args: &[String],
    ) -> Result<sen_plugin_api::ExecuteResult, RegistryError> {
        let mut inner = self.inner.write().await;

        let entry = inner
            .plugins
            .get_mut(command_name)
            .ok_or_else(|| RegistryError::CommandNotFound(command_name.to_string()))?;

        entry
            .plugin
            .instance
            .execute(args)
            .map_err(RegistryError::Execution)
    }

    /// Get plugin manifest for a command
    pub async fn get_manifest(&self, command_name: &str) -> Option<sen_plugin_api::PluginManifest> {
        let inner = self.inner.read().await;
        inner
            .plugins
            .get(command_name)
            .map(|e| e.plugin.manifest.clone())
    }

    /// Get all plugin manifests
    pub async fn get_all_manifests(&self) -> Vec<sen_plugin_api::PluginManifest> {
        let inner = self.inner.read().await;
        inner
            .plugins
            .values()
            .map(|e| e.plugin.manifest.clone())
            .collect()
    }

    /// Get the number of loaded plugins
    pub async fn len(&self) -> usize {
        let inner = self.inner.read().await;
        inner.plugins.len()
    }

    /// Check if the registry is empty
    pub async fn is_empty(&self) -> bool {
        let inner = self.inner.read().await;
        inner.plugins.is_empty()
    }
}

impl Default for PluginRegistry {
    /// Creates a new empty PluginRegistry with default settings.
    ///
    /// # Panics
    /// Panics if the underlying PluginLoader fails to initialize.
    /// Use `PluginRegistry::new()` for fallible construction.
    fn default() -> Self {
        Self::new().expect("Failed to create PluginRegistry: loader initialization failed")
    }
}

/// Errors that can occur during registry operations
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("Plugin execution failed: {0}")]
    Execution(#[source] LoaderError),
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELLO_PLUGIN_WASM: &[u8] = include_bytes!(
        "../../examples/hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm"
    );

    #[tokio::test]
    async fn test_registry_register_and_execute() {
        let registry = PluginRegistry::new().unwrap();
        let loader = PluginLoader::new().unwrap();
        let plugin = loader.load(HELLO_PLUGIN_WASM).unwrap();

        let cmd = registry.register(plugin).await;
        assert_eq!(cmd, "hello");
        assert!(registry.has_command("hello").await);

        let result = registry
            .execute("hello", &["World".to_string()])
            .await
            .unwrap();
        match result {
            sen_plugin_api::ExecuteResult::Success(output) => {
                assert_eq!(output, "Hello, World!");
            }
            _ => panic!("Expected success"),
        }
    }

    #[tokio::test]
    async fn test_registry_unload() {
        let registry = PluginRegistry::new().unwrap();
        let loader = PluginLoader::new().unwrap();
        let plugin = loader.load(HELLO_PLUGIN_WASM).unwrap();

        registry.register(plugin).await;
        assert!(registry.has_command("hello").await);

        registry.unload("hello").await;
        assert!(!registry.has_command("hello").await);
    }

    #[tokio::test]
    async fn test_registry_list_commands() {
        let registry = PluginRegistry::new().unwrap();
        let loader = PluginLoader::new().unwrap();
        let plugin = loader.load(HELLO_PLUGIN_WASM).unwrap();

        registry.register(plugin).await;

        let commands = registry.list_commands().await;
        assert_eq!(commands, vec!["hello"]);
    }
}
