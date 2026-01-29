//! Permission system for plugin capabilities
//!
//! This module provides a flexible, customizable permission system for controlling
//! what capabilities plugins can access. It's designed as a framework that
//! application developers can customize to fit their needs.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         PermissionConfig                                 │
//! │  ┌─────────────┐ ┌─────────────┐ ┌─────────────┐ ┌─────────────────────┐│
//! │  │  Strategy   │ │    Store    │ │   Prompt    │ │       Audit         ││
//! │  │             │ │             │ │             │ │                     ││
//! │  │ - Default   │ │ - File      │ │ - Terminal  │ │ - File (JSONL)      ││
//! │  │ - Strict    │ │ - Memory    │ │ - Auto      │ │ - Memory            ││
//! │  │ - Permissive│ │ - ReadOnly  │ │ - Recording │ │ - Null              ││
//! │  │ - CI        │ │             │ │             │ │ - Composite         ││
//! │  │ - TrustAll  │ │             │ │             │ │                     ││
//! │  └─────────────┘ └─────────────┘ └─────────────┘ └─────────────────────┘│
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Quick Start
//!
//! ## Using Presets
//!
//! ```rust,ignore
//! use sen_plugin_host::permission::PermissionPresets;
//!
//! // Interactive development
//! let config = PermissionPresets::interactive("myapp")?;
//!
//! // CI/CD pipeline
//! let config = PermissionPresets::ci("myapp", None)?;
//!
//! // Testing
//! let config = PermissionPresets::testing();
//! ```
//!
//! ## Custom Configuration
//!
//! ```rust,ignore
//! use sen_plugin_host::permission::{
//!     PermissionConfigBuilder,
//!     DefaultPermissionStrategy,
//!     MemoryPermissionStore,
//!     TerminalPromptHandler,
//!     TrustFlagConfig,
//! };
//! use sen_plugin_host::audit::NullAuditSink;
//!
//! let config = PermissionConfigBuilder::new()
//!     .app_name("myapp")
//!     .strategy(DefaultPermissionStrategy)
//!     .store(MemoryPermissionStore::new())
//!     .prompt(TerminalPromptHandler::new())
//!     .audit(NullAuditSink)
//!     .trust_flags(TrustFlagConfig::default())
//!     .build()?;
//! ```
//!
//! # Components
//!
//! ## Strategy
//!
//! Controls how permission decisions are made:
//!
//! | Strategy | Granularity | Prompts | Best For |
//! |----------|-------------|---------|----------|
//! | Default | Plugin | When needed | General use |
//! | Strict | Command | Always (interactive) | Security |
//! | Permissive | Plugin | Network only | Development |
//! | CI | Plugin | Never | CI/CD |
//! | TrustAll | Plugin | Never | Testing only |
//!
//! ## Store
//!
//! Persists granted permissions:
//!
//! - `FilePermissionStore`: JSON file in config directory
//! - `MemoryPermissionStore`: In-memory (session only)
//! - `ReadOnlyPermissionStore`: Wrapper that prevents writes
//!
//! ## Prompt
//!
//! Handles user interaction:
//!
//! - `TerminalPromptHandler`: Interactive terminal prompts
//! - `AutoPromptHandler`: Automatic approve/deny
//! - `RecordingPromptHandler`: Records prompts (testing)
//!
//! ## Trust Flags
//!
//! Configurable CLI flags for explicit trust:
//!
//! ```bash
//! # Default format
//! myapp --trust-plugin=hello run
//!
//! # Allow-style (configurable)
//! myapp --allow-plugin=hello run
//!
//! # Custom aliases
//! myapp --yolo run  # Trust all (if configured)
//! ```

pub mod presets;
pub mod prompt;
pub mod store;
pub mod strategy;
pub mod trust;

// Re-exports for convenience
pub use presets::{PermissionConfig, PermissionConfigBuilder, PermissionPresets, PresetError};
pub use prompt::{AutoPromptHandler, RecordingPromptHandler, TerminalPromptHandler};
pub use prompt::{PromptError, PromptHandler, PromptResult};
pub use store::{FilePermissionStore, MemoryPermissionStore, ReadOnlyPermissionStore};
pub use store::{PermissionStore, StoreError, StoredPermission, StoredTrustLevel};
pub use strategy::{
    CiPermissionStrategy, DefaultPermissionStrategy, PermissivePermissionStrategy,
    StrictPermissionStrategy, TrustAllStrategy,
};
pub use strategy::{
    PermissionContext, PermissionDecision, PermissionGranularity, PermissionStrategy,
};
pub use trust::{
    TrustDirectives, TrustEffect, TrustFlagAlias, TrustFlagConfig, TrustFlagPresets, TrustTarget,
};
