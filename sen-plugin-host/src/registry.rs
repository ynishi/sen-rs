//! Plugin registry with hot reload support
//!
//! Provides a thread-safe registry for managing loaded plugins with
//! support for dynamic addition, removal, and updates.
//!
//! # Permission System Integration
//!
//! The registry supports an optional permission system for capability-based
//! access control. When enabled, plugins are checked against their declared
//! capabilities before execution.
//!
//! ```rust,ignore
//! use sen_plugin_host::{PluginRegistry, PermissionPresets};
//!
//! // With permission checking
//! let config = PermissionPresets::interactive("myapp")?;
//! let registry = PluginRegistry::with_permissions(config)?;
//!
//! // Execution will check capabilities and prompt if needed
//! registry.execute("hello", &["World"]).await?;
//! ```

use crate::audit::{self, TrustLevel};
use crate::permission::{
    PermissionConfig, PermissionContext, PermissionDecision, StoredPermission, StoredTrustLevel,
};
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
    permission: Option<Arc<PermissionConfig>>,
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
            permission: None,
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
            permission: None,
        }
    }

    /// Create with permission configuration
    ///
    /// When permission config is set, the registry will check plugin
    /// capabilities before execution and prompt users as needed.
    pub fn with_permissions(config: PermissionConfig) -> Result<Self, LoaderError> {
        Ok(Self {
            inner: Arc::new(RwLock::new(RegistryInner {
                plugins: HashMap::new(),
                path_to_command: HashMap::new(),
            })),
            loader: Arc::new(PluginLoader::new()?),
            permission: Some(Arc::new(config)),
        })
    }

    /// Add permission configuration to an existing registry
    pub fn set_permissions(&mut self, config: PermissionConfig) {
        self.permission = Some(Arc::new(config));
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
    ///
    /// If permission configuration is set, this will:
    /// 1. Check stored permissions for the plugin
    /// 2. Apply the permission strategy to decide allow/deny/prompt
    /// 3. Prompt the user if needed (for interactive mode)
    /// 4. Record audit events
    /// 5. Execute the plugin if permitted
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

        // Check permissions if configured
        if let Some(ref perm_config) = self.permission {
            let capabilities = &entry.plugin.manifest.capabilities;

            // Record permission request audit event
            let _ = perm_config
                .audit
                .record(audit::permission_requested(command_name, capabilities));

            // Get stored permission
            let key = perm_config.store.make_key(
                command_name,
                None,
                perm_config.strategy.granularity(),
            );
            let stored = perm_config.store.get(&key).ok().flatten();

            // Build context for strategy
            let ctx = PermissionContext {
                plugin_name: command_name,
                command_path: &[],
                requested: capabilities,
                granted: stored.as_ref().map(|s| &s.capabilities),
                interactive: perm_config.prompt.is_interactive(),
            };

            // Check for escalation
            let decision = if let Some(ref stored_perm) = stored {
                if stored_perm.has_escalated(capabilities) {
                    // Record escalation audit event
                    let _ = perm_config.audit.record(audit::escalation_detected(
                        command_name,
                        &stored_perm.capabilities,
                        capabilities,
                    ));
                    perm_config.strategy.on_escalation(&ctx)
                } else {
                    perm_config.strategy.check(&ctx)
                }
            } else {
                perm_config.strategy.check(&ctx)
            };

            // Handle decision
            match decision {
                PermissionDecision::Allow => {
                    let _ = perm_config.audit.record(audit::permission_granted(
                        command_name,
                        capabilities,
                        TrustLevel::Permanent,
                    ));
                }
                PermissionDecision::Deny(reason) => {
                    let _ = perm_config.audit.record(audit::permission_denied(
                        command_name,
                        capabilities,
                        &reason,
                    ));
                    return Err(RegistryError::PermissionDenied {
                        plugin: command_name.to_string(),
                        reason,
                    });
                }
                PermissionDecision::Prompt => {
                    // Prompt user
                    let prompt_result = if let Some(ref stored_perm) = stored {
                        perm_config.prompt.prompt_escalation(
                            command_name,
                            &stored_perm.capabilities,
                            capabilities,
                        )
                    } else {
                        perm_config.prompt.prompt(command_name, capabilities)
                    };

                    match prompt_result {
                        Ok(result) if result.is_allowed() => {
                            // Store permission if should persist
                            if result.should_persist() {
                                let trust_level = result.to_trust_level().unwrap_or(StoredTrustLevel::Session);
                                let stored_perm = StoredPermission::new(capabilities.clone(), trust_level);
                                let _ = perm_config.store.set(&key, stored_perm);
                            }

                            let audit_trust = match result.to_trust_level() {
                                Some(StoredTrustLevel::Permanent) => TrustLevel::Permanent,
                                Some(StoredTrustLevel::Session) => TrustLevel::Session,
                                None => TrustLevel::Once,
                            };
                            let _ = perm_config.audit.record(audit::permission_granted(
                                command_name,
                                capabilities,
                                audit_trust,
                            ));
                        }
                        Ok(_) | Err(_) => {
                            let _ = perm_config.audit.record(audit::permission_denied(
                                command_name,
                                capabilities,
                                "User denied permission",
                            ));
                            return Err(RegistryError::PermissionDenied {
                                plugin: command_name.to_string(),
                                reason: "User denied permission".to_string(),
                            });
                        }
                    }
                }
                PermissionDecision::AllowPartial(_reduced) => {
                    // For now, treat partial as full allow
                    // Future: could pass reduced capabilities to plugin
                    let _ = perm_config.audit.record(audit::permission_granted(
                        command_name,
                        capabilities,
                        TrustLevel::Once,
                    ));
                }
            }
        }

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

/// Errors that can occur during registry operations
#[derive(Debug, thiserror::Error)]
pub enum RegistryError {
    #[error("Command not found: {0}")]
    CommandNotFound(String),

    #[error("Plugin execution failed: {0}")]
    Execution(#[source] LoaderError),

    #[error("Permission denied for plugin '{plugin}': {reason}")]
    PermissionDenied { plugin: String, reason: String },
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
