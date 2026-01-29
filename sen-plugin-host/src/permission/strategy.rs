//! Permission strategy trait and default implementations
//!
//! Framework users can customize permission behavior by implementing
//! the `PermissionStrategy` trait or using provided defaults.

use sen_plugin_api::Capabilities;

/// Permission granularity level
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PermissionGranularity {
    /// One permission per plugin (default)
    #[default]
    Plugin,
    /// Separate permissions per command/subcommand path
    Command,
    /// Require permission for every execution
    Execution,
}

/// Context provided to permission strategy for decision making
#[derive(Debug)]
pub struct PermissionContext<'a> {
    /// Plugin name
    pub plugin_name: &'a str,
    /// Command path (e.g., ["db", "migrate"] for "db:migrate")
    pub command_path: &'a [String],
    /// Capabilities requested by the plugin
    pub requested: &'a Capabilities,
    /// Previously granted capabilities (if any)
    pub granted: Option<&'a Capabilities>,
    /// Whether running in interactive mode
    pub interactive: bool,
}

/// Decision returned by permission strategy
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Allow execution with requested capabilities
    Allow,
    /// Deny execution with reason
    Deny(String),
    /// Prompt user for permission
    Prompt,
    /// Allow but with reduced capabilities
    AllowPartial(Capabilities),
}

/// Strategy trait for permission resolution
///
/// Framework users implement this trait to customize permission behavior.
///
/// # Example
///
/// ```rust
/// use sen_plugin_host::permission::{
///     PermissionStrategy, PermissionGranularity, PermissionContext, PermissionDecision
/// };
///
/// struct MyStrategy {
///     trusted_plugins: Vec<String>,
/// }
///
/// impl PermissionStrategy for MyStrategy {
///     fn granularity(&self) -> PermissionGranularity {
///         PermissionGranularity::Plugin
///     }
///
///     fn inherit_capabilities(&self) -> bool {
///         false
///     }
///
///     fn check(&self, ctx: &PermissionContext) -> PermissionDecision {
///         if self.trusted_plugins.contains(&ctx.plugin_name.to_string()) {
///             PermissionDecision::Allow
///         } else {
///             PermissionDecision::Prompt
///         }
///     }
/// }
/// ```
pub trait PermissionStrategy: Send + Sync {
    /// Get the granularity level for permission checks
    fn granularity(&self) -> PermissionGranularity;

    /// Whether subcommands inherit parent command's capabilities
    fn inherit_capabilities(&self) -> bool;

    /// Check permission and return decision
    fn check(&self, ctx: &PermissionContext) -> PermissionDecision;

    /// Called when capabilities escalation is detected (plugin updated with more permissions)
    fn on_escalation(&self, ctx: &PermissionContext) -> PermissionDecision {
        // Default: always prompt on escalation
        let _ = ctx;
        PermissionDecision::Prompt
    }
}

// ============================================================================
// Default Implementations
// ============================================================================

/// Default permission strategy
///
/// - Plugin-level granularity
/// - No capability inheritance
/// - Prompts for ungranted permissions
/// - Allows if already granted
pub struct DefaultPermissionStrategy;

impl PermissionStrategy for DefaultPermissionStrategy {
    fn granularity(&self) -> PermissionGranularity {
        PermissionGranularity::Plugin
    }

    fn inherit_capabilities(&self) -> bool {
        false
    }

    fn check(&self, ctx: &PermissionContext) -> PermissionDecision {
        match ctx.granted {
            Some(granted) if ctx.requested.is_subset_of(granted) => PermissionDecision::Allow,
            Some(_) => PermissionDecision::Prompt, // Escalation
            None if ctx.requested.is_empty() => PermissionDecision::Allow,
            None => PermissionDecision::Prompt,
        }
    }
}

/// Strict permission strategy
///
/// - Command-level granularity
/// - No inheritance
/// - Denies in non-interactive mode
pub struct StrictPermissionStrategy;

impl PermissionStrategy for StrictPermissionStrategy {
    fn granularity(&self) -> PermissionGranularity {
        PermissionGranularity::Command
    }

    fn inherit_capabilities(&self) -> bool {
        false
    }

    fn check(&self, ctx: &PermissionContext) -> PermissionDecision {
        match ctx.granted {
            Some(granted) if ctx.requested.is_subset_of(granted) => PermissionDecision::Allow,
            _ if !ctx.interactive => PermissionDecision::Deny(
                "Non-interactive mode requires pre-granted permissions".into(),
            ),
            _ => PermissionDecision::Prompt,
        }
    }
}

/// Permissive strategy for trusted environments
///
/// - Plugin-level granularity
/// - Allows all non-network capabilities without prompt
/// - Still prompts for network access
pub struct PermissivePermissionStrategy;

impl PermissionStrategy for PermissivePermissionStrategy {
    fn granularity(&self) -> PermissionGranularity {
        PermissionGranularity::Plugin
    }

    fn inherit_capabilities(&self) -> bool {
        true
    }

    fn check(&self, ctx: &PermissionContext) -> PermissionDecision {
        // Allow everything except network
        if ctx.requested.net.is_empty() {
            PermissionDecision::Allow
        } else {
            match ctx.granted {
                Some(granted) if ctx.requested.is_subset_of(granted) => PermissionDecision::Allow,
                _ => PermissionDecision::Prompt,
            }
        }
    }
}

/// CI/Batch mode strategy
///
/// - Never prompts (non-interactive)
/// - Allows only pre-granted permissions
/// - Denies everything else
pub struct CiPermissionStrategy;

impl PermissionStrategy for CiPermissionStrategy {
    fn granularity(&self) -> PermissionGranularity {
        PermissionGranularity::Plugin
    }

    fn inherit_capabilities(&self) -> bool {
        false
    }

    fn check(&self, ctx: &PermissionContext) -> PermissionDecision {
        match ctx.granted {
            Some(granted) if ctx.requested.is_subset_of(granted) => PermissionDecision::Allow,
            None if ctx.requested.is_empty() => PermissionDecision::Allow,
            _ => PermissionDecision::Deny("CI mode: all permissions must be pre-granted".into()),
        }
    }

    fn on_escalation(&self, _ctx: &PermissionContext) -> PermissionDecision {
        PermissionDecision::Deny("CI mode: capability escalation not allowed".into())
    }
}

/// Trust-all strategy (DANGEROUS - for development only)
///
/// - Allows all capabilities without prompt
/// - Should only be used in development/testing
#[derive(Debug)]
pub struct TrustAllStrategy {
    _private: (),
}

impl TrustAllStrategy {
    /// Create a new trust-all strategy
    ///
    /// # Safety
    ///
    /// This strategy bypasses all permission checks. Only use in controlled environments.
    pub fn new_dangerous() -> Self {
        Self { _private: () }
    }
}

impl PermissionStrategy for TrustAllStrategy {
    fn granularity(&self) -> PermissionGranularity {
        PermissionGranularity::Plugin
    }

    fn inherit_capabilities(&self) -> bool {
        true
    }

    fn check(&self, _ctx: &PermissionContext) -> PermissionDecision {
        PermissionDecision::Allow
    }

    fn on_escalation(&self, _ctx: &PermissionContext) -> PermissionDecision {
        PermissionDecision::Allow
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sen_plugin_api::{PathPattern, StdioCapability};

    fn make_context<'a>(
        plugin: &'a str,
        requested: &'a Capabilities,
        granted: Option<&'a Capabilities>,
        interactive: bool,
    ) -> PermissionContext<'a> {
        PermissionContext {
            plugin_name: plugin,
            command_path: &[],
            requested,
            granted,
            interactive,
        }
    }

    #[test]
    fn test_default_strategy_empty_caps() {
        let strategy = DefaultPermissionStrategy;
        let caps = Capabilities::none();
        let ctx = make_context("test", &caps, None, true);

        assert_eq!(strategy.check(&ctx), PermissionDecision::Allow);
    }

    #[test]
    fn test_default_strategy_ungranted() {
        let strategy = DefaultPermissionStrategy;
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let ctx = make_context("test", &caps, None, true);

        assert_eq!(strategy.check(&ctx), PermissionDecision::Prompt);
    }

    #[test]
    fn test_default_strategy_granted() {
        let strategy = DefaultPermissionStrategy;
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let granted =
            Capabilities::default().with_fs_read(vec![PathPattern::new("./data").recursive()]);
        let ctx = make_context("test", &caps, Some(&granted), true);

        assert_eq!(strategy.check(&ctx), PermissionDecision::Allow);
    }

    #[test]
    fn test_strict_strategy_non_interactive() {
        let strategy = StrictPermissionStrategy;
        let caps = Capabilities::default().with_stdio(StdioCapability::stdout_only());
        let ctx = make_context("test", &caps, None, false);

        match strategy.check(&ctx) {
            PermissionDecision::Deny(_) => {}
            other => panic!("Expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn test_ci_strategy_denies_ungranted() {
        let strategy = CiPermissionStrategy;
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let ctx = make_context("test", &caps, None, false);

        match strategy.check(&ctx) {
            PermissionDecision::Deny(msg) => {
                assert!(msg.contains("CI mode"));
            }
            other => panic!("Expected Deny, got {:?}", other),
        }
    }

    #[test]
    fn test_permissive_allows_non_network() {
        let strategy = PermissivePermissionStrategy;
        let caps = Capabilities::default()
            .with_fs_read(vec![PathPattern::new("./data")])
            .with_fs_write(vec![PathPattern::new("./output")])
            .with_stdio(StdioCapability::all());
        let ctx = make_context("test", &caps, None, true);

        assert_eq!(strategy.check(&ctx), PermissionDecision::Allow);
    }

    #[test]
    fn test_trust_all_allows_everything() {
        let strategy = TrustAllStrategy::new_dangerous();
        let caps = Capabilities::default()
            .with_fs_read(vec![PathPattern::new("/")])
            .with_net(vec![sen_plugin_api::NetPattern::https("*")]);
        let ctx = make_context("test", &caps, None, false);

        assert_eq!(strategy.check(&ctx), PermissionDecision::Allow);
    }
}
