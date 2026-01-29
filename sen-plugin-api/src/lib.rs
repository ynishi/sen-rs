//! sen-plugin-api: Shared types for sen-rs plugin system
//!
//! This crate defines the protocol between host and guest (wasm plugin).
//! Communication uses MessagePack serialization.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// API version for compatibility checking
/// - v1: Initial version (command + args only)
/// - v2: Added capabilities support
pub const API_VERSION: u32 = 2;

// ============================================================================
// Capabilities Types
// ============================================================================

/// Plugin capability declarations
///
/// Plugins declare what system resources they need access to.
/// The host will prompt users to grant these permissions before execution.
///
/// # Example
///
/// ```rust
/// use sen_plugin_api::{Capabilities, PathPattern, StdioCapability};
///
/// let caps = Capabilities::default()
///     .with_fs_read(vec![PathPattern::new("./data").recursive()])
///     .with_fs_write(vec![PathPattern::new("./output")])
///     .with_stdio(StdioCapability::stdout_stderr());
/// ```
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    /// Filesystem read access paths
    /// Relative paths resolved from CWD, supports ~ expansion
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fs_read: Vec<PathPattern>,

    /// Filesystem write access paths
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fs_write: Vec<PathPattern>,

    /// Environment variable access patterns
    /// Supports glob: "MY_*", exact: "HOME"
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub env_read: Vec<String>,

    /// Network access patterns (WASI Preview 2)
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub net: Vec<NetPattern>,

    /// Standard I/O access
    #[serde(default, skip_serializing_if = "StdioCapability::is_none")]
    pub stdio: StdioCapability,
}

impl Capabilities {
    /// Create empty capabilities (no permissions)
    pub fn none() -> Self {
        Self::default()
    }

    /// Check if no capabilities are requested
    pub fn is_empty(&self) -> bool {
        self.fs_read.is_empty()
            && self.fs_write.is_empty()
            && self.env_read.is_empty()
            && self.net.is_empty()
            && self.stdio.is_none()
    }

    /// Add filesystem read paths
    pub fn with_fs_read(mut self, paths: Vec<PathPattern>) -> Self {
        self.fs_read = paths;
        self
    }

    /// Add filesystem write paths
    pub fn with_fs_write(mut self, paths: Vec<PathPattern>) -> Self {
        self.fs_write = paths;
        self
    }

    /// Add environment variable patterns
    pub fn with_env_read(mut self, patterns: Vec<String>) -> Self {
        self.env_read = patterns;
        self
    }

    /// Add network patterns
    pub fn with_net(mut self, patterns: Vec<NetPattern>) -> Self {
        self.net = patterns;
        self
    }

    /// Set stdio capabilities
    pub fn with_stdio(mut self, stdio: StdioCapability) -> Self {
        self.stdio = stdio;
        self
    }

    /// Check if `self` is a subset of `other` (all requested capabilities are granted)
    pub fn is_subset_of(&self, other: &Capabilities) -> bool {
        // Check fs_read
        for path in &self.fs_read {
            if !other.fs_read.iter().any(|p| p.contains(path)) {
                return false;
            }
        }

        // Check fs_write
        for path in &self.fs_write {
            if !other.fs_write.iter().any(|p| p.contains(path)) {
                return false;
            }
        }

        // Check env_read (simple string match for now)
        for env in &self.env_read {
            if !other.env_read.contains(env) {
                return false;
            }
        }

        // Check net
        for net in &self.net {
            if !other.net.iter().any(|n| n.contains(net)) {
                return false;
            }
        }

        // Check stdio
        if self.stdio.stdin && !other.stdio.stdin {
            return false;
        }
        if self.stdio.stdout && !other.stdio.stdout {
            return false;
        }
        if self.stdio.stderr && !other.stdio.stderr {
            return false;
        }

        true
    }

    /// Compute hash for change detection
    pub fn compute_hash(&self) -> String {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        // Hash serialized form for stability
        if let Ok(bytes) = rmp_serde::to_vec(self) {
            bytes.hash(&mut hasher);
        }
        format!("{:016x}", hasher.finish())
    }
}

/// Filesystem path pattern
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathPattern {
    /// Path pattern (e.g., "./data", "/tmp", "~/.config/app")
    pub pattern: String,

    /// Allow recursive access to subdirectories
    #[serde(default)]
    pub recursive: bool,
}

impl PathPattern {
    /// Create a new path pattern
    pub fn new(pattern: impl Into<String>) -> Self {
        Self {
            pattern: pattern.into(),
            recursive: false,
        }
    }

    /// Enable recursive access
    pub fn recursive(mut self) -> Self {
        self.recursive = true;
        self
    }

    /// Check if this pattern contains/covers another pattern
    pub fn contains(&self, other: &PathPattern) -> bool {
        if self.pattern == other.pattern {
            // Same path: recursive covers non-recursive
            return self.recursive || !other.recursive;
        }

        // If self is recursive, check if other is under self's path
        if self.recursive {
            let self_path = PathBuf::from(&self.pattern);
            let other_path = PathBuf::from(&other.pattern);

            // Normalize for comparison (handle ./foo vs foo)
            let self_normalized = self_path.components().collect::<Vec<_>>();
            let other_normalized = other_path.components().collect::<Vec<_>>();

            if other_normalized.len() >= self_normalized.len() {
                return other_normalized
                    .iter()
                    .take(self_normalized.len())
                    .eq(self_normalized.iter());
            }
        }

        false
    }
}

/// Network access pattern
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NetPattern {
    /// Host pattern (e.g., "api.example.com", "*.github.com")
    pub host: String,

    /// Port (None = any port)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,

    /// Protocol
    #[serde(default)]
    pub protocol: NetProtocol,
}

impl NetPattern {
    /// Create HTTPS pattern
    pub fn https(host: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port: None,
            protocol: NetProtocol::Https,
        }
    }

    /// Create HTTPS pattern with specific port
    pub fn https_port(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port: Some(port),
            protocol: NetProtocol::Https,
        }
    }

    /// Create TCP pattern
    pub fn tcp(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port: Some(port),
            protocol: NetProtocol::Tcp,
        }
    }

    /// Check if this pattern contains/covers another pattern
    pub fn contains(&self, other: &NetPattern) -> bool {
        // Protocol must match
        if self.protocol != other.protocol {
            return false;
        }

        // Host matching (simple wildcard support)
        let host_matches = if self.host.starts_with("*.") {
            let suffix = &self.host[1..]; // ".github.com"
            other.host.ends_with(suffix) || other.host == self.host[2..]
        } else {
            self.host == other.host
        };

        if !host_matches {
            return false;
        }

        // Port matching (None means any)
        match (self.port, other.port) {
            (None, _) => true,
            (Some(sp), Some(op)) => sp == op,
            (Some(_), None) => false,
        }
    }
}

/// Network protocol
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum NetProtocol {
    #[default]
    Https = 0,
    Http = 1,
    Tcp = 2,
}

/// Standard I/O capability flags
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct StdioCapability {
    /// Access to stdin
    #[serde(default)]
    pub stdin: bool,

    /// Access to stdout
    #[serde(default)]
    pub stdout: bool,

    /// Access to stderr
    #[serde(default)]
    pub stderr: bool,
}

impl StdioCapability {
    /// No stdio access
    pub fn none() -> Self {
        Self::default()
    }

    /// Check if no stdio is requested
    pub fn is_none(&self) -> bool {
        !self.stdin && !self.stdout && !self.stderr
    }

    /// Full stdio access
    pub fn all() -> Self {
        Self {
            stdin: true,
            stdout: true,
            stderr: true,
        }
    }

    /// stdout + stderr only (common for plugins that produce output)
    pub fn stdout_stderr() -> Self {
        Self {
            stdin: false,
            stdout: true,
            stderr: true,
        }
    }

    /// stdout only
    pub fn stdout_only() -> Self {
        Self {
            stdin: false,
            stdout: true,
            stderr: false,
        }
    }
}

// ============================================================================
// Command Specification Types
// ============================================================================

/// Command specification returned by plugin's `manifest()` function
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommandSpec {
    /// Command name (used for routing, e.g., "hello" or "db:create")
    pub name: String,

    /// Short description for help text
    pub about: String,

    /// Plugin version (semver)
    #[serde(default)]
    pub version: Option<String>,

    /// Plugin author
    #[serde(default)]
    pub author: Option<String>,

    /// Argument specifications
    #[serde(default)]
    pub args: Vec<ArgSpec>,

    /// Nested subcommands
    #[serde(default)]
    pub subcommands: Vec<CommandSpec>,
}

/// Argument specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArgSpec {
    /// Argument name (positional) or option name
    pub name: String,

    /// Long option name (e.g., "--output")
    #[serde(default)]
    pub long: Option<String>,

    /// Short option name (e.g., "-o")
    #[serde(default)]
    pub short: Option<char>,

    /// Whether this argument is required
    #[serde(default)]
    pub required: bool,

    /// Help text for this argument
    #[serde(default)]
    pub help: String,

    /// Value placeholder name (e.g., "FILE")
    #[serde(default)]
    pub value_name: Option<String>,

    /// Default value if not provided
    #[serde(default)]
    pub default_value: Option<String>,

    /// List of allowed values
    #[serde(default)]
    pub possible_values: Option<Vec<String>>,
}

/// Result of plugin execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ExecuteResult {
    /// Successful execution with output
    Success(String),

    /// Execution failed
    Error(ExecuteError),
}

/// Error details from plugin execution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecuteError {
    /// Exit code (1 = user error, 101 = system error)
    pub code: u8,

    /// Error message
    pub message: String,
}

/// Plugin manifest with API version
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// API version for compatibility
    pub api_version: u32,

    /// Command specification
    pub command: CommandSpec,

    /// Capability requirements (v2+)
    #[serde(default, skip_serializing_if = "Capabilities::is_empty")]
    pub capabilities: Capabilities,
}

impl PluginManifest {
    /// Create a new plugin manifest with current API version
    pub fn new(command: CommandSpec) -> Self {
        Self {
            api_version: API_VERSION,
            command,
            capabilities: Capabilities::default(),
        }
    }

    /// Create a new plugin manifest with capabilities
    pub fn with_capabilities(command: CommandSpec, capabilities: Capabilities) -> Self {
        Self {
            api_version: API_VERSION,
            command,
            capabilities,
        }
    }

    /// Add capabilities to an existing manifest
    pub fn capabilities(mut self, caps: Capabilities) -> Self {
        self.capabilities = caps;
        self
    }
}

impl ExecuteResult {
    /// Create a success result
    pub fn success(output: impl Into<String>) -> Self {
        Self::Success(output.into())
    }

    /// Create a user error (exit code 1)
    pub fn user_error(message: impl Into<String>) -> Self {
        Self::Error(ExecuteError {
            code: 1,
            message: message.into(),
        })
    }

    /// Create a system error (exit code 101)
    pub fn system_error(message: impl Into<String>) -> Self {
        Self::Error(ExecuteError {
            code: 101,
            message: message.into(),
        })
    }
}

impl CommandSpec {
    /// Create a new command spec
    pub fn new(name: impl Into<String>, about: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            about: about.into(),
            version: None,
            author: None,
            args: Vec::new(),
            subcommands: Vec::new(),
        }
    }

    /// Add version
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// Add an argument
    pub fn arg(mut self, arg: ArgSpec) -> Self {
        self.args.push(arg);
        self
    }

    /// Add a subcommand
    pub fn subcommand(mut self, cmd: CommandSpec) -> Self {
        self.subcommands.push(cmd);
        self
    }
}

impl ArgSpec {
    /// Create a positional argument
    pub fn positional(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            long: None,
            short: None,
            required: false,
            help: String::new(),
            value_name: None,
            default_value: None,
            possible_values: None,
        }
    }

    /// Create an option with long name
    pub fn option(name: impl Into<String>, long: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            long: Some(long.into()),
            short: None,
            required: false,
            help: String::new(),
            value_name: None,
            default_value: None,
            possible_values: None,
        }
    }

    /// Set as required
    pub fn required(mut self) -> Self {
        self.required = true;
        self
    }

    /// Set help text
    pub fn help(mut self, help: impl Into<String>) -> Self {
        self.help = help.into();
        self
    }

    /// Set short option
    pub fn short(mut self, short: char) -> Self {
        self.short = Some(short);
        self
    }

    /// Set default value
    pub fn default(mut self, value: impl Into<String>) -> Self {
        self.default_value = Some(value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_command_spec_serialization() {
        let spec = CommandSpec::new("hello", "Says hello")
            .version("1.0.0")
            .arg(
                ArgSpec::positional("name")
                    .help("Name to greet")
                    .default("World"),
            );

        let bytes = rmp_serde::to_vec(&spec).unwrap();
        let decoded: CommandSpec = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(decoded.name, "hello");
        assert_eq!(decoded.about, "Says hello");
        assert_eq!(decoded.args.len(), 1);
    }

    #[test]
    fn test_execute_result_serialization() {
        let result = ExecuteResult::success("Hello, World!");
        let bytes = rmp_serde::to_vec(&result).unwrap();
        let decoded: ExecuteResult = rmp_serde::from_slice(&bytes).unwrap();

        match decoded {
            ExecuteResult::Success(s) => assert_eq!(s, "Hello, World!"),
            _ => panic!("Expected success"),
        }
    }

    // ========================================================================
    // Capabilities Tests
    // ========================================================================

    #[test]
    fn test_capabilities_empty() {
        let caps = Capabilities::none();
        assert!(caps.is_empty());

        let caps_with_fs = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        assert!(!caps_with_fs.is_empty());
    }

    #[test]
    fn test_capabilities_serialization() {
        let caps = Capabilities::default()
            .with_fs_read(vec![PathPattern::new("./data").recursive()])
            .with_fs_write(vec![PathPattern::new("./output")])
            .with_env_read(vec!["HOME".into(), "PATH".into()])
            .with_net(vec![NetPattern::https("api.example.com")])
            .with_stdio(StdioCapability::stdout_stderr());

        // Use named serialization for struct fields (consistent with existing codebase)
        let bytes = rmp_serde::to_vec_named(&caps).unwrap();
        let decoded: Capabilities = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(decoded.fs_read.len(), 1);
        assert!(decoded.fs_read[0].recursive);
        assert_eq!(decoded.fs_write.len(), 1);
        assert_eq!(decoded.env_read.len(), 2);
        assert_eq!(decoded.net.len(), 1);
        assert!(decoded.stdio.stdout);
        assert!(!decoded.stdio.stdin);
    }

    #[test]
    fn test_capabilities_subset() {
        let requested = Capabilities::default()
            .with_fs_read(vec![PathPattern::new("./data")])
            .with_stdio(StdioCapability::stdout_only());

        let granted = Capabilities::default()
            .with_fs_read(vec![PathPattern::new("./data").recursive()])
            .with_fs_write(vec![PathPattern::new("./output")])
            .with_stdio(StdioCapability::stdout_stderr());

        assert!(requested.is_subset_of(&granted));

        // Request more than granted
        let over_requested =
            Capabilities::default().with_fs_read(vec![PathPattern::new("./secret")]);

        assert!(!over_requested.is_subset_of(&granted));
    }

    #[test]
    fn test_path_pattern_contains() {
        let parent = PathPattern::new("./data").recursive();
        let child = PathPattern::new("./data/subdir");

        assert!(parent.contains(&child));
        assert!(!child.contains(&parent));

        let same = PathPattern::new("./data");
        assert!(parent.contains(&same));
        assert!(!same.contains(&parent)); // non-recursive doesn't cover recursive request
    }

    #[test]
    fn test_net_pattern_contains() {
        let wildcard = NetPattern::https("*.github.com");
        let specific = NetPattern::https("api.github.com");

        assert!(wildcard.contains(&specific));
        assert!(!specific.contains(&wildcard));

        let with_port = NetPattern::https_port("api.example.com", 443);
        let any_port = NetPattern::https("api.example.com");

        assert!(any_port.contains(&with_port));
        assert!(!with_port.contains(&any_port));
    }

    #[test]
    fn test_manifest_with_capabilities() {
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);

        let manifest =
            PluginManifest::with_capabilities(CommandSpec::new("data-export", "Export data"), caps);

        assert_eq!(manifest.api_version, API_VERSION);
        assert_eq!(manifest.capabilities.fs_read.len(), 1);

        // Serialization roundtrip
        let bytes = rmp_serde::to_vec(&manifest).unwrap();
        let decoded: PluginManifest = rmp_serde::from_slice(&bytes).unwrap();

        assert_eq!(decoded.capabilities.fs_read.len(), 1);
    }

    #[test]
    fn test_capabilities_hash() {
        let caps1 = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let caps2 = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let caps3 = Capabilities::default().with_fs_read(vec![PathPattern::new("./other")]);

        assert_eq!(caps1.compute_hash(), caps2.compute_hash());
        assert_ne!(caps1.compute_hash(), caps3.compute_hash());
    }
}
