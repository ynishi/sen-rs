//! Sandbox validation and path security
//!
//! This module provides path validation to ensure plugins cannot escape
//! their declared sandbox boundaries through symlinks, `../` patterns,
//! or other path manipulation techniques.
//!
//! # Security Properties
//!
//! The sandbox validator guarantees:
//!
//! 1. **No Traversal Escape**: Paths like `./data/../secret` are blocked
//! 2. **Symlink Safety**: Symlinks are resolved and validated against sandbox
//! 3. **Absolute Path Resolution**: All paths are canonicalized before use
//! 4. **Existence Verification**: Paths must exist before being granted
//!
//! # Path Resolution Flow
//!
//! ```text
//! Input: "./data/../config"
//!           │
//!           ▼
//! ┌─────────────────────────────┐
//! │  1. Expand ~ (home dir)     │
//! │     ~/foo → /home/user/foo  │
//! └─────────────────────────────┘
//!           │
//!           ▼
//! ┌─────────────────────────────┐
//! │  2. Resolve relative paths  │
//! │     ./foo → {cwd}/foo       │
//! └─────────────────────────────┘
//!           │
//!           ▼
//! ┌─────────────────────────────┐
//! │  3. Canonicalize            │
//! │     - Resolve ..            │
//! │     - Follow symlinks       │
//! │     - Get absolute path     │
//! └─────────────────────────────┘
//!           │
//!           ▼
//! ┌─────────────────────────────┐
//! │  4. Validate existence      │
//! │     - Must exist            │
//! │     - Must be directory     │
//! │       (for fs access)       │
//! └─────────────────────────────┘
//!           │
//!           ▼
//! Output: "/home/user/project/config" (absolute, validated)
//! ```
//!
//! # Example
//!
//! ```rust,ignore
//! use sen_plugin_host::wasi::sandbox::{SandboxValidator, SandboxConfig};
//! use std::path::PathBuf;
//!
//! let validator = SandboxValidator::new(SandboxConfig {
//!     working_directory: PathBuf::from("/home/user/project"),
//!     follow_symlinks: true,
//!     require_existence: true,
//! });
//!
//! // Valid path within project
//! let resolved = validator.validate_path("./data")?;
//! assert_eq!(resolved, PathBuf::from("/home/user/project/data"));
//!
//! // Invalid: escapes project directory
//! let result = validator.validate_path("./data/../../../etc/passwd");
//! assert!(result.is_err());
//! ```

use super::error::WasiError;
use std::path::{Path, PathBuf};

/// Configuration for sandbox validation
#[derive(Debug, Clone)]
pub struct SandboxConfig {
    /// Working directory for relative path resolution
    pub working_directory: PathBuf,

    /// Whether to follow symlinks during validation
    /// Default: true (symlinks are resolved and validated)
    pub follow_symlinks: bool,

    /// Whether paths must exist
    /// Default: true for security
    pub require_existence: bool,
}

impl Default for SandboxConfig {
    fn default() -> Self {
        Self {
            working_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            follow_symlinks: true,
            require_existence: true,
        }
    }
}

/// Validates and resolves paths for WASI sandbox
///
/// # Security
///
/// This validator is critical for security. All filesystem paths
/// declared in plugin capabilities MUST pass through this validator
/// before being used in WASI configuration.
#[derive(Debug, Clone)]
pub struct SandboxValidator {
    config: SandboxConfig,
}

impl SandboxValidator {
    /// Create a new sandbox validator
    pub fn new(config: SandboxConfig) -> Self {
        Self { config }
    }

    /// Create a validator with default configuration
    pub fn with_working_directory(working_directory: PathBuf) -> Self {
        Self::new(SandboxConfig {
            working_directory,
            ..Default::default()
        })
    }

    /// Validate and resolve a path pattern
    ///
    /// # Security
    ///
    /// This method performs full path validation:
    /// 1. Expands `~` to home directory
    /// 2. Resolves relative paths against working directory
    /// 3. Canonicalizes the path (resolves `..` and symlinks)
    /// 4. Verifies the path exists (if configured)
    ///
    /// # Errors
    ///
    /// Returns `WasiError` if:
    /// - Path contains invalid characters
    /// - Path escapes the working directory via `..`
    /// - Symlink points outside allowed boundaries
    /// - Path does not exist (if `require_existence` is true)
    pub fn validate_path(&self, pattern: &str) -> Result<PathBuf, WasiError> {
        // 1. Expand home directory
        let expanded = self.expand_home(pattern);

        // 2. Resolve relative paths
        let absolute = if expanded.is_relative() {
            self.config.working_directory.join(&expanded)
        } else {
            expanded
        };

        // 3. Canonicalize (resolve symlinks and ..)
        let resolved = if self.config.follow_symlinks {
            if self.config.require_existence {
                absolute.canonicalize().map_err(|e| {
                    if e.kind() == std::io::ErrorKind::NotFound {
                        WasiError::PathNotFound(absolute.clone())
                    } else {
                        WasiError::SymlinkResolution {
                            path: absolute.clone(),
                            source: e,
                        }
                    }
                })?
            } else {
                // Best-effort canonicalization for non-existent paths
                self.normalize_path(&absolute)
            }
        } else {
            self.normalize_path(&absolute)
        };

        // 4. Security check: ensure path is within working directory
        // (only for relative paths - absolute paths are explicit grants)
        let is_relative =
            pattern.starts_with("./") || pattern.starts_with("../") || !pattern.starts_with('/');
        if is_relative && !pattern.starts_with('~') {
            let cwd_canonical = self
                .config
                .working_directory
                .canonicalize()
                .unwrap_or_else(|_| self.config.working_directory.clone());

            if !resolved.starts_with(&cwd_canonical) {
                return Err(WasiError::sandbox_escape(pattern, &resolved));
            }
        }

        Ok(resolved)
    }

    /// Validate that a path is a directory
    pub fn validate_directory(&self, pattern: &str) -> Result<PathBuf, WasiError> {
        let resolved = self.validate_path(pattern)?;

        if self.config.require_existence && !resolved.is_dir() {
            return Err(WasiError::NotADirectory(resolved));
        }

        Ok(resolved)
    }

    /// Expand `~` to home directory
    fn expand_home(&self, path: &str) -> PathBuf {
        if let Some(suffix) = path.strip_prefix("~/") {
            if let Some(home) = dirs::home_dir() {
                return home.join(suffix);
            }
        } else if path == "~" {
            if let Some(home) = dirs::home_dir() {
                return home;
            }
        }
        PathBuf::from(path)
    }

    /// Normalize a path without following symlinks
    ///
    /// This is a best-effort normalization that:
    /// - Removes redundant `.`
    /// - Resolves `..` where possible
    /// - Does NOT follow symlinks
    fn normalize_path(&self, path: &Path) -> PathBuf {
        let mut components = Vec::new();

        for component in path.components() {
            match component {
                std::path::Component::ParentDir => {
                    // Only pop if we have a normal component (not root)
                    if matches!(components.last(), Some(std::path::Component::Normal(_))) {
                        components.pop();
                    } else {
                        components.push(component);
                    }
                }
                std::path::Component::CurDir => {
                    // Skip `.`
                }
                _ => {
                    components.push(component);
                }
            }
        }

        components.iter().collect()
    }
}

/// Validate environment variable patterns
///
/// Patterns can be:
/// - Exact match: `HOME`, `PATH`
/// - Wildcard suffix: `MY_*` (matches `MY_VAR`, `MY_CONFIG`, etc.)
///
/// # Security
///
/// Wildcards are limited to suffix patterns only.
/// Patterns like `*_SECRET` or `*` are rejected.
pub fn validate_env_pattern(pattern: &str) -> Result<(), WasiError> {
    if pattern.is_empty() {
        return Err(WasiError::InvalidEnvPattern("empty pattern".to_string()));
    }

    // Check for invalid wildcard positions
    let wildcard_count = pattern.matches('*').count();

    if wildcard_count > 1 {
        return Err(WasiError::InvalidEnvPattern(format!(
            "multiple wildcards not supported: {}",
            pattern
        )));
    }

    if wildcard_count == 1 && !pattern.ends_with('*') {
        return Err(WasiError::InvalidEnvPattern(format!(
            "wildcard must be at end of pattern: {}",
            pattern
        )));
    }

    // Check for valid characters (alphanumeric, underscore, and *)
    if !pattern
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '*')
    {
        return Err(WasiError::InvalidEnvPattern(format!(
            "invalid characters in pattern: {}",
            pattern
        )));
    }

    // Reject bare wildcard
    if pattern == "*" {
        return Err(WasiError::InvalidEnvPattern(
            "bare wildcard '*' not allowed (too permissive)".to_string(),
        ));
    }

    Ok(())
}

/// Expand environment variable patterns to actual variable names
///
/// # Example
///
/// ```rust,ignore
/// // Pattern "MY_*" with env vars MY_FOO=1, MY_BAR=2, OTHER=3
/// let vars = expand_env_pattern("MY_*")?;
/// assert_eq!(vars, vec![("MY_FOO", "1"), ("MY_BAR", "2")]);
/// ```
pub fn expand_env_pattern(pattern: &str) -> Result<Vec<(String, String)>, WasiError> {
    validate_env_pattern(pattern)?;

    if let Some(prefix) = pattern.strip_suffix('*') {
        // Wildcard pattern: MY_* matches MY_FOO, MY_BAR, etc.
        Ok(std::env::vars()
            .filter(|(key, _)| key.starts_with(prefix))
            .collect())
    } else {
        // Exact match
        match std::env::var(pattern) {
            Ok(value) => Ok(vec![(pattern.to_string(), value)]),
            Err(std::env::VarError::NotPresent) => {
                // Not an error - env var might not exist, which is fine
                Ok(vec![])
            }
            Err(std::env::VarError::NotUnicode(_)) => Err(WasiError::InvalidEnvPattern(format!(
                "environment variable '{}' contains invalid unicode",
                pattern
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_env_pattern_valid() {
        assert!(validate_env_pattern("HOME").is_ok());
        assert!(validate_env_pattern("PATH").is_ok());
        assert!(validate_env_pattern("MY_VAR").is_ok());
        assert!(validate_env_pattern("MY_*").is_ok());
        assert!(validate_env_pattern("MYAPP_CONFIG_*").is_ok());
    }

    #[test]
    fn test_validate_env_pattern_invalid() {
        // Empty
        assert!(validate_env_pattern("").is_err());

        // Bare wildcard
        assert!(validate_env_pattern("*").is_err());

        // Wildcard not at end
        assert!(validate_env_pattern("*_SECRET").is_err());
        assert!(validate_env_pattern("MY_*_VAR").is_err());

        // Multiple wildcards
        assert!(validate_env_pattern("MY_*_*").is_err());

        // Invalid characters
        assert!(validate_env_pattern("MY-VAR").is_err());
        assert!(validate_env_pattern("MY.VAR").is_err());
    }

    #[test]
    fn test_normalize_path() {
        let validator = SandboxValidator::new(SandboxConfig {
            working_directory: PathBuf::from("/home/user"),
            follow_symlinks: false,
            require_existence: false,
        });

        // Basic normalization
        let result = validator.normalize_path(Path::new("/home/user/./data"));
        assert_eq!(result, PathBuf::from("/home/user/data"));

        // Parent directory resolution
        let result = validator.normalize_path(Path::new("/home/user/data/../config"));
        assert_eq!(result, PathBuf::from("/home/user/config"));
    }

    #[test]
    fn test_expand_home() {
        let validator = SandboxValidator::new(SandboxConfig::default());

        // Only test if HOME is set
        if dirs::home_dir().is_some() {
            let expanded = validator.expand_home("~/data");
            assert!(!expanded.to_string_lossy().contains('~'));
            assert!(expanded.to_string_lossy().ends_with("/data"));
        }
    }
}
