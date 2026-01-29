//! WASI context builder from Capabilities
//!
//! This module provides the core logic for converting [`Capabilities`] declarations
//! into a configured [`wasmtime_wasi::WasiCtx`] for sandboxed plugin execution.
//!
//! # Overview
//!
//! The [`WasiConfigurer`] takes a plugin's declared capabilities and produces
//! a WASI context that grants exactly those permissions—no more, no less.
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         WasiConfigurer Flow                             │
//! ├─────────────────────────────────────────────────────────────────────────┤
//! │                                                                         │
//! │  Input: Capabilities                                                    │
//! │  ─────────────────────                                                  │
//! │  {                                                                      │
//! │    fs_read: ["./data", "~/.config/myapp"],                             │
//! │    fs_write: ["./output"],                                              │
//! │    env_read: ["HOME", "MYAPP_*"],                                       │
//! │    stdio: { stdin: false, stdout: true, stderr: true }                  │
//! │  }                                                                      │
//! │                                                                         │
//! │                           │                                             │
//! │                           ▼                                             │
//! │                                                                         │
//! │  WasiConfigurer                                                         │
//! │  ──────────────                                                         │
//! │  1. Validate & resolve paths (sandbox.rs)                               │
//! │  2. Expand env patterns (HOME, MYAPP_FOO, MYAPP_BAR)                    │
//! │  3. Build WasiCtxBuilder                                                │
//! │                                                                         │
//! │                           │                                             │
//! │                           ▼                                             │
//! │                                                                         │
//! │  Output: WasiCtx                                                        │
//! │  ─────────────                                                          │
//! │  - preopened_dir("/abs/path/data", "data", READ)                       │
//! │  - preopened_dir("/home/user/.config/myapp", ".config/myapp", READ)    │
//! │  - preopened_dir("/abs/path/output", "output", READ|WRITE)             │
//! │  - env("HOME", "/home/user")                                           │
//! │  - env("MYAPP_FOO", "value1")                                          │
//! │  - env("MYAPP_BAR", "value2")                                          │
//! │  - inherit_stdout()                                                     │
//! │  - inherit_stderr()                                                     │
//! │                                                                         │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Directory Permissions
//!
//! WASI uses preopened directories to grant filesystem access.
//! The permission level depends on whether the path is in `fs_read` or `fs_write`:
//!
//! | Capability | Dir Perms | File Perms |
//! |------------|-----------|------------|
//! | `fs_read` | `READ` | `READ` |
//! | `fs_write` | `READ \| MUTATE \| CREATE` | `READ \| WRITE` |
//!
//! Note: A path in `fs_write` also gets read permissions (write-only is rare).
//!
//! # Guest Path Mapping
//!
//! Preopened directories have both a host path and a guest path:
//!
//! ```text
//! Host Path (real filesystem):     /home/user/project/data
//! Guest Path (inside WASM):        /data
//!
//! Plugin code opens:               /data/file.txt
//! Host actually accesses:          /home/user/project/data/file.txt
//! ```
//!
//! The guest path is derived from the original pattern:
//! - `./data` → `/data`
//! - `~/config` → `/config`
//! - `/tmp/myapp` → `/tmp/myapp`
//!
//! # Example
//!
//! ```rust,ignore
//! use sen_plugin_host::wasi::{WasiConfigurer, WasiConfig};
//! use sen_plugin_api::{Capabilities, PathPattern, StdioCapability};
//!
//! let caps = Capabilities::default()
//!     .with_fs_read(vec![PathPattern::new("./data").recursive()])
//!     .with_env_read(vec!["HOME".into()])
//!     .with_stdio(StdioCapability::stdout_only());
//!
//! let wasi_ctx = WasiConfigurer::new()
//!     .with_capabilities(&caps)
//!     .with_working_directory("/home/user/project".into())
//!     .build()?;
//!
//! // wasi_ctx is now ready for use with wasmtime Store
//! ```
//!
//! # Security Considerations
//!
//! 1. **All paths are validated** through [`SandboxValidator`] before use
//! 2. **Environment variables are filtered** - only declared patterns are passed
//! 3. **Stdio is opt-in** - plugins cannot access stdin/stdout/stderr by default
//! 4. **No network access** - WASI Preview 1 doesn't support sockets
//! 5. **Clock/random allowed** - these are safe and commonly needed

use super::error::WasiError;
use super::sandbox::{expand_env_pattern, SandboxConfig, SandboxValidator};
use sen_plugin_api::Capabilities;
use std::path::PathBuf;

/// Configuration for WASI context building
#[derive(Debug, Clone)]
pub struct WasiConfig {
    /// Working directory for path resolution
    pub working_directory: Option<PathBuf>,

    /// Whether to follow symlinks during path validation
    pub follow_symlinks: bool,

    /// Whether paths must exist
    pub require_existence: bool,

    /// Arguments to pass to the plugin (argv)
    pub args: Vec<String>,

    /// Program name (argv[0])
    pub program_name: String,
}

impl Default for WasiConfig {
    fn default() -> Self {
        Self {
            working_directory: None,
            follow_symlinks: true,
            require_existence: true,
            args: Vec::new(),
            program_name: "plugin".to_string(),
        }
    }
}

/// Builder for WASI context from Capabilities
///
/// # Example
///
/// ```rust,ignore
/// let wasi_ctx = WasiConfigurer::new()
///     .with_capabilities(&caps)
///     .with_working_directory(std::env::current_dir()?)
///     .with_args(vec!["arg1".into(), "arg2".into()])
///     .build()?;
/// ```
#[derive(Debug)]
pub struct WasiConfigurer {
    config: WasiConfig,
    capabilities: Option<Capabilities>,
}

impl WasiConfigurer {
    /// Create a new WASI configurer
    pub fn new() -> Self {
        Self {
            config: WasiConfig::default(),
            capabilities: None,
        }
    }

    /// Set the capabilities to configure
    pub fn with_capabilities(mut self, capabilities: &Capabilities) -> Self {
        self.capabilities = Some(capabilities.clone());
        self
    }

    /// Set the working directory for path resolution
    pub fn with_working_directory(mut self, path: PathBuf) -> Self {
        self.config.working_directory = Some(path);
        self
    }

    /// Set command-line arguments
    pub fn with_args(mut self, args: Vec<String>) -> Self {
        self.config.args = args;
        self
    }

    /// Set the program name (argv[0])
    pub fn with_program_name(mut self, name: impl Into<String>) -> Self {
        self.config.program_name = name.into();
        self
    }

    /// Configure symlink following
    pub fn follow_symlinks(mut self, follow: bool) -> Self {
        self.config.follow_symlinks = follow;
        self
    }

    /// Configure path existence requirement
    pub fn require_existence(mut self, require: bool) -> Self {
        self.config.require_existence = require;
        self
    }

    /// Build the WASI configuration specification
    ///
    /// Returns a [`WasiSpec`] that contains all the validated and resolved
    /// configuration needed to create a [`wasmtime_wasi::WasiCtx`].
    ///
    /// # Errors
    ///
    /// Returns [`WasiError`] if:
    /// - Working directory is not set
    /// - Path validation fails
    /// - Environment pattern is invalid
    pub fn build(self) -> Result<WasiSpec, WasiError> {
        let working_dir = self
            .config
            .working_directory
            .ok_or(WasiError::WorkingDirectoryNotSet)?;

        let validator = SandboxValidator::new(SandboxConfig {
            working_directory: working_dir.clone(),
            follow_symlinks: self.config.follow_symlinks,
            require_existence: self.config.require_existence,
        });

        let caps = self.capabilities.unwrap_or_default();
        let mut spec = WasiSpec::new(self.config.program_name, self.config.args);

        // Process filesystem read paths
        for pattern in &caps.fs_read {
            let resolved = validator.validate_directory(&pattern.pattern)?;
            let guest_path = derive_guest_path(&pattern.pattern);
            spec.preopened_dirs.push(PreopenedDir {
                host_path: resolved,
                guest_path,
                writable: false,
            });
        }

        // Process filesystem write paths
        for pattern in &caps.fs_write {
            let resolved = validator.validate_directory(&pattern.pattern)?;
            let guest_path = derive_guest_path(&pattern.pattern);

            // Check if already added as read-only
            if let Some(existing) = spec
                .preopened_dirs
                .iter_mut()
                .find(|d| d.host_path == resolved)
            {
                // Upgrade to writable
                existing.writable = true;
            } else {
                spec.preopened_dirs.push(PreopenedDir {
                    host_path: resolved,
                    guest_path,
                    writable: true,
                });
            }
        }

        // Process environment variables
        for pattern in &caps.env_read {
            let vars = expand_env_pattern(pattern)?;
            for (key, value) in vars {
                // Avoid duplicates
                if !spec.env_vars.iter().any(|(k, _)| k == &key) {
                    spec.env_vars.push((key, value));
                }
            }
        }

        // Process stdio
        spec.inherit_stdin = caps.stdio.stdin;
        spec.inherit_stdout = caps.stdio.stdout;
        spec.inherit_stderr = caps.stdio.stderr;

        // Note: Network access is not supported in WASI Preview 1
        if !caps.net.is_empty() {
            tracing::warn!(
                "Network capabilities declared but not supported in WASI Preview 1. \
                 Network access will be denied."
            );
        }

        Ok(spec)
    }
}

impl Default for WasiConfigurer {
    fn default() -> Self {
        Self::new()
    }
}

/// Specification for WASI context
///
/// This is an intermediate representation that can be:
/// 1. Inspected for debugging/logging
/// 2. Serialized for configuration
/// 3. Converted to actual `WasiCtx` when wasmtime-wasi is available
#[derive(Debug, Clone)]
pub struct WasiSpec {
    /// Program name (argv[0])
    pub program_name: String,

    /// Command-line arguments
    pub args: Vec<String>,

    /// Preopened directories
    pub preopened_dirs: Vec<PreopenedDir>,

    /// Environment variables to pass
    pub env_vars: Vec<(String, String)>,

    /// Inherit stdin from host
    pub inherit_stdin: bool,

    /// Inherit stdout from host
    pub inherit_stdout: bool,

    /// Inherit stderr from host
    pub inherit_stderr: bool,
}

impl WasiSpec {
    /// Create a new empty spec
    pub fn new(program_name: String, args: Vec<String>) -> Self {
        Self {
            program_name,
            args,
            preopened_dirs: Vec::new(),
            env_vars: Vec::new(),
            inherit_stdin: false,
            inherit_stdout: false,
            inherit_stderr: false,
        }
    }

    /// Build a WASI Preview 1 context for module-based plugins
    ///
    /// This is the primary method for building WASI contexts for traditional
    /// WASM modules (not components). Most plugins use this.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let spec = WasiConfigurer::new()
    ///     .with_capabilities(&caps)
    ///     .with_working_directory(cwd)
    ///     .build()?;
    ///
    /// let wasi_ctx = spec.build_p1_ctx()?;
    /// let mut store = Store::new(&engine, wasi_ctx);
    /// ```
    ///
    /// # Errors
    ///
    /// Returns [`WasiError`] if:
    /// - A preopened directory cannot be opened
    /// - Directory permissions cannot be configured
    pub fn build_p1_ctx(self) -> Result<wasmtime_wasi::preview1::WasiP1Ctx, WasiError> {
        use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

        let mut builder = WasiCtxBuilder::new();

        // Set program arguments
        let mut all_args = vec![self.program_name];
        all_args.extend(self.args);
        builder.args(&all_args);

        // Set environment variables
        for (key, value) in &self.env_vars {
            builder.env(key, value);
        }

        // Configure stdio
        if self.inherit_stdin {
            builder.inherit_stdin();
        }
        if self.inherit_stdout {
            builder.inherit_stdout();
        }
        if self.inherit_stderr {
            builder.inherit_stderr();
        }

        // Configure preopened directories
        for dir in &self.preopened_dirs {
            let dir_perms = if dir.writable {
                DirPerms::all()
            } else {
                DirPerms::READ
            };

            let file_perms = if dir.writable {
                FilePerms::all()
            } else {
                FilePerms::READ
            };

            builder
                .preopened_dir(&dir.host_path, &dir.guest_path, dir_perms, file_perms)
                .map_err(|e| WasiError::PreopenFailed {
                    path: dir.host_path.clone(),
                    source: std::io::Error::other(e.to_string()),
                })?;
        }

        Ok(builder.build_p1())
    }

    /// Build the actual WASI context for wasmtime (Component Model)
    ///
    /// This is for use with the WebAssembly Component Model.
    /// For traditional WASM modules, use [`build_p1_ctx`] instead.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let spec = WasiConfigurer::new()
    ///     .with_capabilities(&caps)
    ///     .with_working_directory(cwd)
    ///     .build()?;
    ///
    /// let wasi_ctx = spec.build_ctx()?;
    /// let mut store = Store::new(&engine, wasi_ctx);
    /// ```
    pub fn build_ctx(self) -> Result<wasmtime_wasi::WasiCtx, WasiError> {
        use wasmtime_wasi::{DirPerms, FilePerms, WasiCtxBuilder};

        let mut builder = WasiCtxBuilder::new();

        // Set program arguments
        let mut all_args = vec![self.program_name.clone()];
        all_args.extend(self.args.clone());
        builder.args(&all_args);

        // Set environment variables
        for (key, value) in &self.env_vars {
            builder.env(key, value);
        }

        // Configure stdio
        if self.inherit_stdin {
            builder.inherit_stdin();
        }
        if self.inherit_stdout {
            builder.inherit_stdout();
        }
        if self.inherit_stderr {
            builder.inherit_stderr();
        }

        // Configure preopened directories
        for dir in &self.preopened_dirs {
            let dir_perms = if dir.writable {
                DirPerms::all()
            } else {
                DirPerms::READ
            };

            let file_perms = if dir.writable {
                FilePerms::all()
            } else {
                FilePerms::READ
            };

            builder
                .preopened_dir(&dir.host_path, &dir.guest_path, dir_perms, file_perms)
                .map_err(|e| WasiError::PreopenFailed {
                    path: dir.host_path.clone(),
                    source: std::io::Error::other(e.to_string()),
                })?;
        }

        Ok(builder.build())
    }

    /// Build WASI context and return it along with a ResourceTable
    ///
    /// This is the recommended method when you need both the context and
    /// the resource table for the wasmtime store (Component Model).
    ///
    /// For traditional WASM modules, use [`build_p1_ctx`] instead.
    pub fn build_ctx_with_table(
        self,
    ) -> Result<(wasmtime_wasi::WasiCtx, wasmtime_wasi::ResourceTable), WasiError> {
        let ctx = self.build_ctx()?;
        let table = wasmtime_wasi::ResourceTable::new();
        Ok((ctx, table))
    }

    /// Check if this spec grants any filesystem access
    pub fn has_fs_access(&self) -> bool {
        !self.preopened_dirs.is_empty()
    }

    /// Check if this spec grants any write access
    pub fn has_write_access(&self) -> bool {
        self.preopened_dirs.iter().any(|d| d.writable)
    }

    /// Get a summary of permissions for display
    pub fn permission_summary(&self) -> String {
        let mut parts = Vec::new();

        if !self.preopened_dirs.is_empty() {
            let read_paths: Vec<_> = self
                .preopened_dirs
                .iter()
                .filter(|d| !d.writable)
                .map(|d| d.guest_path.as_str())
                .collect();
            let write_paths: Vec<_> = self
                .preopened_dirs
                .iter()
                .filter(|d| d.writable)
                .map(|d| d.guest_path.as_str())
                .collect();

            if !read_paths.is_empty() {
                parts.push(format!("fs_read: [{}]", read_paths.join(", ")));
            }
            if !write_paths.is_empty() {
                parts.push(format!("fs_write: [{}]", write_paths.join(", ")));
            }
        }

        if !self.env_vars.is_empty() {
            let keys: Vec<_> = self.env_vars.iter().map(|(k, _)| k.as_str()).collect();
            parts.push(format!("env: [{}]", keys.join(", ")));
        }

        let mut stdio = Vec::new();
        if self.inherit_stdin {
            stdio.push("stdin");
        }
        if self.inherit_stdout {
            stdio.push("stdout");
        }
        if self.inherit_stderr {
            stdio.push("stderr");
        }
        if !stdio.is_empty() {
            parts.push(format!("stdio: [{}]", stdio.join(", ")));
        }

        if parts.is_empty() {
            "none".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// A preopened directory configuration
#[derive(Debug, Clone)]
pub struct PreopenedDir {
    /// Absolute path on host filesystem
    pub host_path: PathBuf,

    /// Path visible to guest (inside WASM)
    pub guest_path: String,

    /// Whether the guest can write to this directory
    pub writable: bool,
}

/// Derive guest path from the original pattern
///
/// ```text
/// ./data      → /data
/// ~/config    → /config
/// /tmp/myapp  → /tmp/myapp
/// ```
fn derive_guest_path(pattern: &str) -> String {
    if let Some(suffix) = pattern
        .strip_prefix("./")
        .or_else(|| pattern.strip_prefix("~/"))
    {
        format!("/{}", suffix)
    } else if pattern.starts_with('/') {
        pattern.to_string()
    } else {
        format!("/{}", pattern)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sen_plugin_api::{PathPattern, StdioCapability};

    #[test]
    fn test_derive_guest_path() {
        assert_eq!(derive_guest_path("./data"), "/data");
        assert_eq!(derive_guest_path("~/config"), "/config");
        assert_eq!(derive_guest_path("/tmp/myapp"), "/tmp/myapp");
        assert_eq!(derive_guest_path("data"), "/data");
    }

    #[test]
    fn test_wasi_spec_summary() {
        let mut spec = WasiSpec::new("test".into(), vec![]);
        spec.preopened_dirs.push(PreopenedDir {
            host_path: PathBuf::from("/data"),
            guest_path: "/data".into(),
            writable: false,
        });
        spec.env_vars.push(("HOME".into(), "/home/user".into()));
        spec.inherit_stdout = true;

        let summary = spec.permission_summary();
        assert!(summary.contains("fs_read"));
        assert!(summary.contains("/data"));
        assert!(summary.contains("env"));
        assert!(summary.contains("HOME"));
        assert!(summary.contains("stdout"));
    }

    #[test]
    fn test_configurer_requires_working_dir() {
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);

        let result = WasiConfigurer::new().with_capabilities(&caps).build();

        assert!(matches!(result, Err(WasiError::WorkingDirectoryNotSet)));
    }

    #[test]
    fn test_empty_capabilities() {
        let caps = Capabilities::none();

        let spec = WasiConfigurer::new()
            .with_capabilities(&caps)
            .with_working_directory(PathBuf::from("/tmp"))
            .require_existence(false)
            .build()
            .unwrap();

        assert!(!spec.has_fs_access());
        assert!(!spec.inherit_stdin);
        assert!(!spec.inherit_stdout);
        assert!(!spec.inherit_stderr);
    }

    #[test]
    fn test_stdio_configuration() {
        let caps = Capabilities::default().with_stdio(StdioCapability::stdout_stderr());

        let spec = WasiConfigurer::new()
            .with_capabilities(&caps)
            .with_working_directory(PathBuf::from("/tmp"))
            .require_existence(false)
            .build()
            .unwrap();

        assert!(!spec.inherit_stdin);
        assert!(spec.inherit_stdout);
        assert!(spec.inherit_stderr);
    }
}
