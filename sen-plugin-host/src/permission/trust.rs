//! Trust flag configuration for CLI integration
//!
//! Provides template-based trust flag generation without reserving specific flag names.
//! Framework users can customize the flag format to match their CLI conventions.

/// Trust flag configuration
///
/// Allows framework users to define how trust flags are generated
/// without the framework reserving specific flag names.
///
/// # Example
///
/// ```rust
/// use sen_plugin_host::permission::{TrustFlagConfig, TrustEffect};
///
/// // Default: --trust-plugin=name, --trust-command=name
/// let config = TrustFlagConfig::default();
///
/// // Custom: --allow-plugin=name
/// let config = TrustFlagConfig::new()
///     .with_flag_template("--allow-{target}")
///     .with_alias("--yolo", TrustEffect::TrustAll);
/// ```
#[derive(Debug, Clone)]
pub struct TrustFlagConfig {
    /// Enable trust flags feature
    pub enabled: bool,

    /// Flag template: {target} is replaced with "plugin" or "command"
    /// Default: "--trust-{target}"
    pub flag_template: String,

    /// Value template: {name} is replaced with plugin/command name
    /// Default: "{name}"
    pub value_template: String,

    /// Custom aliases for common trust patterns
    pub aliases: Vec<TrustFlagAlias>,

    /// Whether to show trust flags in help
    pub show_in_help: bool,

    /// Help text template for generated flags
    pub help_template: String,
}

impl Default for TrustFlagConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            flag_template: "--trust-{target}".into(),
            value_template: "{name}".into(),
            aliases: vec![],
            show_in_help: true,
            help_template: "Trust {target} '{name}' for this execution".into(),
        }
    }
}

impl TrustFlagConfig {
    /// Create a new trust flag configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Disable trust flags entirely
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            ..Self::default()
        }
    }

    /// Set the flag template
    ///
    /// Variables:
    /// - `{target}`: "plugin" or "command"
    pub fn with_flag_template(mut self, template: impl Into<String>) -> Self {
        self.flag_template = template.into();
        self
    }

    /// Set the value template
    ///
    /// Variables:
    /// - `{name}`: plugin or command name
    pub fn with_value_template(mut self, template: impl Into<String>) -> Self {
        self.value_template = template.into();
        self
    }

    /// Add a custom alias
    pub fn with_alias(mut self, flag: impl Into<String>, effect: TrustEffect) -> Self {
        self.aliases.push(TrustFlagAlias {
            flag: flag.into(),
            description: effect.default_description(),
            effect,
        });
        self
    }

    /// Add a custom alias with description
    pub fn with_alias_desc(
        mut self,
        flag: impl Into<String>,
        description: impl Into<String>,
        effect: TrustEffect,
    ) -> Self {
        self.aliases.push(TrustFlagAlias {
            flag: flag.into(),
            description: description.into(),
            effect,
        });
        self
    }

    /// Hide trust flags from help output
    pub fn hidden(mut self) -> Self {
        self.show_in_help = false;
        self
    }

    /// Generate the flag name for a target type
    pub fn generate_flag(&self, target: TrustTarget) -> String {
        self.flag_template.replace("{target}", target.as_str())
    }

    /// Generate the value format for a name
    pub fn generate_value(&self, name: &str) -> String {
        self.value_template.replace("{name}", name)
    }

    /// Generate help text for a flag
    pub fn generate_help(&self, target: TrustTarget, name: &str) -> String {
        self.help_template
            .replace("{target}", target.as_str())
            .replace("{name}", name)
    }

    /// Parse command line arguments and extract trust directives
    pub fn parse_args(&self, args: &[String]) -> TrustDirectives {
        if !self.enabled {
            return TrustDirectives::default();
        }

        let mut directives = TrustDirectives::default();

        for arg in args {
            // Check aliases first
            for alias in &self.aliases {
                if arg == &alias.flag {
                    match &alias.effect {
                        TrustEffect::TrustAll => directives.trust_all = true,
                        TrustEffect::TrustSession => directives.trust_session = true,
                        TrustEffect::TrustNamed { target, name } => match target {
                            TrustTarget::Plugin => {
                                directives.trusted_plugins.push(name.clone());
                            }
                            TrustTarget::Command => {
                                directives.trusted_commands.push(name.clone());
                            }
                        },
                    }
                }
            }

            // Check template-based flags
            let plugin_flag = self.generate_flag(TrustTarget::Plugin);
            let command_flag = self.generate_flag(TrustTarget::Command);

            if let Some(value) = arg.strip_prefix(&format!("{}=", plugin_flag)) {
                directives.trusted_plugins.push(value.to_string());
            } else if let Some(value) = arg.strip_prefix(&format!("{}=", command_flag)) {
                directives.trusted_commands.push(value.to_string());
            }
        }

        directives
    }
}

/// Target type for trust flags
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustTarget {
    Plugin,
    Command,
}

impl TrustTarget {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Plugin => "plugin",
            Self::Command => "command",
        }
    }
}

/// Custom trust flag alias
#[derive(Debug, Clone)]
pub struct TrustFlagAlias {
    /// The flag (e.g., "--yolo")
    pub flag: String,
    /// Description for help text
    pub description: String,
    /// What this alias does
    pub effect: TrustEffect,
}

/// Effect of a trust flag
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustEffect {
    /// Trust a specific plugin or command
    TrustNamed { target: TrustTarget, name: String },
    /// Trust all plugins for this execution (dangerous)
    TrustAll,
    /// Trust for this session only (not persisted)
    TrustSession,
}

impl TrustEffect {
    /// Get default description for this effect
    fn default_description(&self) -> String {
        match self {
            Self::TrustNamed { target, name } => {
                format!("Trust {} '{}'", target.as_str(), name)
            }
            Self::TrustAll => "Trust all plugins (dangerous)".into(),
            Self::TrustSession => "Trust permissions for this session only".into(),
        }
    }
}

/// Parsed trust directives from command line
#[derive(Debug, Clone, Default)]
pub struct TrustDirectives {
    /// Explicitly trusted plugin names
    pub trusted_plugins: Vec<String>,
    /// Explicitly trusted command names
    pub trusted_commands: Vec<String>,
    /// Trust all plugins (--trust-all or equivalent)
    pub trust_all: bool,
    /// Trust for session only (not persisted)
    pub trust_session: bool,
}

impl TrustDirectives {
    /// Check if a plugin is trusted
    pub fn is_plugin_trusted(&self, name: &str) -> bool {
        self.trust_all || self.trusted_plugins.iter().any(|p| p == name)
    }

    /// Check if a command is trusted
    pub fn is_command_trusted(&self, name: &str) -> bool {
        self.trust_all || self.trusted_commands.iter().any(|c| c == name)
    }

    /// Check if any trust directive is active
    pub fn has_any(&self) -> bool {
        self.trust_all
            || self.trust_session
            || !self.trusted_plugins.is_empty()
            || !self.trusted_commands.is_empty()
    }
}

/// Builder for common trust flag configurations
pub struct TrustFlagPresets;

impl TrustFlagPresets {
    /// Standard configuration with --trust-plugin and --trust-command
    pub fn standard() -> TrustFlagConfig {
        TrustFlagConfig::default()
    }

    /// Allow-style flags (--allow-plugin, --allow-command)
    pub fn allow_style() -> TrustFlagConfig {
        TrustFlagConfig::new().with_flag_template("--allow-{target}")
    }

    /// Short flags (-tp, -tc)
    pub fn short_style() -> TrustFlagConfig {
        TrustFlagConfig::new()
            .with_flag_template("-t{target}")
            .with_alias("-ta", TrustEffect::TrustAll)
    }

    /// Kubernetes-style (--trust plugin=name)
    pub fn k8s_style() -> TrustFlagConfig {
        TrustFlagConfig::new()
            .with_flag_template("--trust")
            .with_value_template("{target}={name}")
    }

    /// Disabled (no trust flags)
    pub fn disabled() -> TrustFlagConfig {
        TrustFlagConfig::disabled()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = TrustFlagConfig::default();
        assert!(config.enabled);
        assert_eq!(config.generate_flag(TrustTarget::Plugin), "--trust-plugin");
        assert_eq!(
            config.generate_flag(TrustTarget::Command),
            "--trust-command"
        );
    }

    #[test]
    fn test_custom_template() {
        let config = TrustFlagConfig::new().with_flag_template("--allow-{target}");

        assert_eq!(config.generate_flag(TrustTarget::Plugin), "--allow-plugin");
    }

    #[test]
    fn test_parse_args() {
        let config = TrustFlagConfig::default();
        let args = vec![
            "--trust-plugin=hello".into(),
            "--trust-command=db:migrate".into(),
            "other-arg".into(),
        ];

        let directives = config.parse_args(&args);
        assert!(directives.is_plugin_trusted("hello"));
        assert!(directives.is_command_trusted("db:migrate"));
        assert!(!directives.is_plugin_trusted("other"));
    }

    #[test]
    fn test_alias() {
        let config = TrustFlagConfig::new().with_alias("--yolo", TrustEffect::TrustAll);

        let args = vec!["--yolo".into()];
        let directives = config.parse_args(&args);

        assert!(directives.trust_all);
        assert!(directives.is_plugin_trusted("any-plugin"));
    }

    #[test]
    fn test_disabled_config() {
        let config = TrustFlagConfig::disabled();
        let args = vec!["--trust-plugin=hello".into()];
        let directives = config.parse_args(&args);

        assert!(!directives.is_plugin_trusted("hello"));
    }

    #[test]
    fn test_presets() {
        let allow = TrustFlagPresets::allow_style();
        assert_eq!(allow.generate_flag(TrustTarget::Plugin), "--allow-plugin");

        let short = TrustFlagPresets::short_style();
        assert_eq!(short.generate_flag(TrustTarget::Plugin), "-tplugin");
    }
}
