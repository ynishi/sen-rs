//! Pre-configured permission setups for common use cases
//!
//! Provides ready-to-use configurations that framework users can use
//! directly or as starting points for customization.

use std::path::PathBuf;
use std::sync::Arc;

use super::prompt::{AutoPromptHandler, PromptHandler, TerminalPromptHandler};
use super::store::{
    FilePermissionStore, MemoryPermissionStore, PermissionStore, ReadOnlyPermissionStore,
};
use super::strategy::{
    CiPermissionStrategy, DefaultPermissionStrategy, PermissionStrategy,
    PermissivePermissionStrategy, StrictPermissionStrategy, TrustAllStrategy,
};
use super::trust::{TrustFlagConfig, TrustFlagPresets};
use crate::audit::{AuditSink, FileAuditSink, MemoryAuditSink, NullAuditSink};

/// Complete permission configuration bundle
pub struct PermissionConfig {
    /// Permission checking strategy
    pub strategy: Arc<dyn PermissionStrategy>,
    /// Permission storage
    pub store: Arc<dyn PermissionStore>,
    /// User prompt handler
    pub prompt: Arc<dyn PromptHandler>,
    /// Audit sink
    pub audit: Arc<dyn AuditSink>,
    /// Trust flag configuration
    pub trust_flags: TrustFlagConfig,
}

impl std::fmt::Debug for PermissionConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PermissionConfig")
            .field("trust_flags", &self.trust_flags)
            .finish_non_exhaustive()
    }
}

impl PermissionConfig {
    /// Create a new configuration with custom components
    pub fn new(
        strategy: impl PermissionStrategy + 'static,
        store: impl PermissionStore + 'static,
        prompt: impl PromptHandler + 'static,
        audit: impl AuditSink + 'static,
        trust_flags: TrustFlagConfig,
    ) -> Self {
        Self {
            strategy: Arc::new(strategy),
            store: Arc::new(store),
            prompt: Arc::new(prompt),
            audit: Arc::new(audit),
            trust_flags,
        }
    }
}

/// Builder for permission configurations
pub struct PermissionConfigBuilder {
    strategy: Option<Arc<dyn PermissionStrategy>>,
    store: Option<Arc<dyn PermissionStore>>,
    prompt: Option<Arc<dyn PromptHandler>>,
    audit: Option<Arc<dyn AuditSink>>,
    trust_flags: TrustFlagConfig,
    app_name: Option<String>,
}

impl PermissionConfigBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self {
            strategy: None,
            store: None,
            prompt: None,
            audit: None,
            trust_flags: TrustFlagConfig::default(),
            app_name: None,
        }
    }

    /// Set the application name (used for default paths)
    pub fn app_name(mut self, name: impl Into<String>) -> Self {
        self.app_name = Some(name.into());
        self
    }

    /// Set the permission strategy
    pub fn strategy(mut self, strategy: impl PermissionStrategy + 'static) -> Self {
        self.strategy = Some(Arc::new(strategy));
        self
    }

    /// Set the permission store
    pub fn store(mut self, store: impl PermissionStore + 'static) -> Self {
        self.store = Some(Arc::new(store));
        self
    }

    /// Set the prompt handler
    pub fn prompt(mut self, prompt: impl PromptHandler + 'static) -> Self {
        self.prompt = Some(Arc::new(prompt));
        self
    }

    /// Set the audit sink
    pub fn audit(mut self, audit: impl AuditSink + 'static) -> Self {
        self.audit = Some(Arc::new(audit));
        self
    }

    /// Set trust flag configuration
    pub fn trust_flags(mut self, config: TrustFlagConfig) -> Self {
        self.trust_flags = config;
        self
    }

    /// Build the configuration
    pub fn build(self) -> Result<PermissionConfig, PresetError> {
        let app_name = self.app_name.as_deref().unwrap_or("plugin-host");

        let store: Arc<dyn PermissionStore> = match self.store {
            Some(s) => s,
            None => {
                let store = FilePermissionStore::default_for_app(app_name)
                    .map_err(|e| PresetError::StoreInit(e.to_string()))?;
                Arc::new(store)
            }
        };

        Ok(PermissionConfig {
            strategy: self
                .strategy
                .unwrap_or_else(|| Arc::new(DefaultPermissionStrategy)),
            store,
            prompt: self
                .prompt
                .unwrap_or_else(|| Arc::new(TerminalPromptHandler::new())),
            audit: self.audit.unwrap_or_else(|| Arc::new(NullAuditSink)),
            trust_flags: self.trust_flags,
        })
    }
}

impl Default for PermissionConfigBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Error type for preset initialization
#[derive(Debug, thiserror::Error)]
pub enum PresetError {
    #[error("Failed to initialize store: {0}")]
    StoreInit(String),

    #[error("Failed to initialize audit: {0}")]
    AuditInit(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}

// ============================================================================
// Preset Configurations
// ============================================================================

/// Preset configurations for common use cases
pub struct PermissionPresets;

impl PermissionPresets {
    /// Interactive development mode
    ///
    /// - Default permission strategy (prompts for new permissions)
    /// - File-based permission storage
    /// - Terminal prompts
    /// - File-based audit log
    /// - Standard trust flags
    pub fn interactive(app_name: &str) -> Result<PermissionConfig, PresetError> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join(app_name);

        let store = FilePermissionStore::new(config_dir.join("permissions.json"))
            .map_err(|e| PresetError::StoreInit(e.to_string()))?;

        let audit = FileAuditSink::new(config_dir.join("audit.jsonl"))
            .map_err(|e| PresetError::AuditInit(e.to_string()))?;

        Ok(PermissionConfig {
            strategy: Arc::new(DefaultPermissionStrategy),
            store: Arc::new(store),
            prompt: Arc::new(TerminalPromptHandler::new()),
            audit: Arc::new(audit),
            trust_flags: TrustFlagPresets::standard(),
        })
    }

    /// Strict mode for security-conscious environments
    ///
    /// - Strict permission strategy (denies in non-interactive)
    /// - File-based storage
    /// - Verbose terminal prompts
    /// - File-based audit log
    /// - Standard trust flags
    pub fn strict(app_name: &str) -> Result<PermissionConfig, PresetError> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join(app_name);

        let store = FilePermissionStore::new(config_dir.join("permissions.json"))
            .map_err(|e| PresetError::StoreInit(e.to_string()))?;

        let audit = FileAuditSink::new(config_dir.join("audit.jsonl"))
            .map_err(|e| PresetError::AuditInit(e.to_string()))?;

        Ok(PermissionConfig {
            strategy: Arc::new(StrictPermissionStrategy),
            store: Arc::new(store),
            prompt: Arc::new(TerminalPromptHandler::new()),
            audit: Arc::new(audit),
            trust_flags: TrustFlagPresets::standard(),
        })
    }

    /// CI/CD pipeline mode
    ///
    /// - CI strategy (no prompts, requires pre-granted permissions)
    /// - Read-only file storage
    /// - Auto-deny prompt handler
    /// - File-based audit log
    /// - Trust flags disabled
    pub fn ci(
        app_name: &str,
        permissions_file: Option<PathBuf>,
    ) -> Result<PermissionConfig, PresetError> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join(app_name);

        let store_path = permissions_file.unwrap_or_else(|| config_dir.join("permissions.json"));

        let inner_store = FilePermissionStore::new(&store_path)
            .map_err(|e| PresetError::StoreInit(e.to_string()))?;

        let store = ReadOnlyPermissionStore::new(inner_store);

        let audit = FileAuditSink::new(config_dir.join("audit.jsonl"))
            .map_err(|e| PresetError::AuditInit(e.to_string()))?;

        Ok(PermissionConfig {
            strategy: Arc::new(CiPermissionStrategy),
            store: Arc::new(store),
            prompt: Arc::new(AutoPromptHandler::always_deny()),
            audit: Arc::new(audit),
            trust_flags: TrustFlagPresets::disabled(),
        })
    }

    /// Permissive development mode
    ///
    /// - Permissive strategy (allows non-network without prompt)
    /// - File-based storage
    /// - Terminal prompts for network
    /// - Memory-based audit (not persisted)
    /// - Allow-style trust flags
    pub fn permissive(app_name: &str) -> Result<PermissionConfig, PresetError> {
        let config_dir = dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join(app_name);

        let store = FilePermissionStore::new(config_dir.join("permissions.json"))
            .map_err(|e| PresetError::StoreInit(e.to_string()))?;

        Ok(PermissionConfig {
            strategy: Arc::new(PermissivePermissionStrategy),
            store: Arc::new(store),
            prompt: Arc::new(TerminalPromptHandler::minimal()),
            audit: Arc::new(MemoryAuditSink::new()),
            trust_flags: TrustFlagPresets::allow_style(),
        })
    }

    /// Testing mode (in-memory, no persistence)
    ///
    /// - Default strategy
    /// - In-memory storage
    /// - Auto-approve prompt handler
    /// - Memory-based audit
    /// - Trust flags enabled
    pub fn testing() -> PermissionConfig {
        PermissionConfig {
            strategy: Arc::new(DefaultPermissionStrategy),
            store: Arc::new(MemoryPermissionStore::new()),
            prompt: Arc::new(AutoPromptHandler::always_allow()),
            audit: Arc::new(MemoryAuditSink::new()),
            trust_flags: TrustFlagPresets::standard(),
        }
    }

    /// Dangerous: Trust all plugins (DEVELOPMENT ONLY)
    ///
    /// - Trust-all strategy (bypasses all checks)
    /// - In-memory storage
    /// - Auto-approve
    /// - Null audit
    /// - Trust flags disabled (not needed)
    ///
    /// # Safety
    ///
    /// This configuration bypasses all security checks.
    /// Only use for development/testing in controlled environments.
    pub fn trust_all_dangerous() -> PermissionConfig {
        PermissionConfig {
            strategy: Arc::new(TrustAllStrategy::new_dangerous()),
            store: Arc::new(MemoryPermissionStore::new()),
            prompt: Arc::new(AutoPromptHandler::always_allow()),
            audit: Arc::new(NullAuditSink),
            trust_flags: TrustFlagPresets::disabled(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builder() {
        let config = PermissionConfigBuilder::new()
            .app_name("test-app")
            .strategy(DefaultPermissionStrategy)
            .store(MemoryPermissionStore::new())
            .prompt(AutoPromptHandler::always_deny())
            .audit(NullAuditSink)
            .build()
            .unwrap();

        assert!(!config.prompt.is_interactive());
    }

    #[test]
    fn test_testing_preset() {
        let config = PermissionPresets::testing();
        assert!(!config.prompt.is_interactive());
    }

    #[test]
    fn test_trust_all_dangerous() {
        let config = PermissionPresets::trust_all_dangerous();
        assert!(!config.trust_flags.enabled);
    }
}
