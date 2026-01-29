//! WASI-specific error types
//!
//! Provides detailed error information for WASI configuration and sandbox violations.

use std::path::PathBuf;
use thiserror::Error;

/// Errors that can occur during WASI configuration
#[derive(Debug, Error)]
pub enum WasiError {
    /// Path validation failed
    #[error("Invalid path '{path}': {reason}")]
    InvalidPath { path: PathBuf, reason: String },

    /// Path escapes sandbox boundaries
    #[error("Path '{path}' escapes sandbox (resolved to '{resolved}')")]
    SandboxEscape { path: PathBuf, resolved: PathBuf },

    /// Path does not exist
    #[error("Path does not exist: {0}")]
    PathNotFound(PathBuf),

    /// Path is not a directory (for fs access)
    #[error("Path is not a directory: {0}")]
    NotADirectory(PathBuf),

    /// Environment variable not found
    #[error("Environment variable not found: {0}")]
    EnvNotFound(String),

    /// Environment variable pattern invalid
    #[error("Invalid environment variable pattern: {0}")]
    InvalidEnvPattern(String),

    /// WASI context creation failed
    #[error("Failed to create WASI context: {0}")]
    ContextCreation(String),

    /// Preopened directory configuration failed
    #[error("Failed to preopen directory '{path}': {source}")]
    PreopenFailed {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Symlink resolution failed
    #[error("Failed to resolve symlink '{path}': {source}")]
    SymlinkResolution {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    /// Working directory not set
    #[error("Working directory must be set for relative path resolution")]
    WorkingDirectoryNotSet,

    /// Capability not supported
    #[error("Capability not supported: {0}")]
    UnsupportedCapability(String),
}

impl WasiError {
    /// Create an invalid path error
    pub fn invalid_path(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Self::InvalidPath {
            path: path.into(),
            reason: reason.into(),
        }
    }

    /// Create a sandbox escape error
    pub fn sandbox_escape(path: impl Into<PathBuf>, resolved: impl Into<PathBuf>) -> Self {
        Self::SandboxEscape {
            path: path.into(),
            resolved: resolved.into(),
        }
    }

    /// Check if this is a security-related error
    pub fn is_security_violation(&self) -> bool {
        matches!(self, Self::SandboxEscape { .. } | Self::InvalidPath { .. })
    }
}
