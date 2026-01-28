//! sen-plugin-api: Shared types for sen-rs plugin system
//!
//! This crate defines the protocol between host and guest (wasm plugin).
//! Communication uses MessagePack serialization.

use serde::{Deserialize, Serialize};

/// API version for compatibility checking
pub const API_VERSION: u32 = 1;

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
}
