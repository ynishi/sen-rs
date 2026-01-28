//! Bridge between Wasm plugins and sen-rs Router
//!
//! Provides integration to register plugin commands as native routes.

use crate::{LoadedPlugin, PluginInstance};
use sen::{Handler, HandlerMetadata, Response, State};
use sen_plugin_api::{CommandSpec, ExecuteResult};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::Mutex;

/// A handler that wraps a Wasm plugin instance
///
/// # Memory Note
///
/// The `command_about` field uses `Box::leak` to satisfy `HandlerMetadata`'s `'static`
/// lifetime requirement. This is a deliberate trade-off:
/// - PRO: Simple, zero-cost access to the description string
/// - CON: Memory is not reclaimed when the handler is dropped
///
/// For typical CLI applications where plugins are loaded once and used throughout
/// the program lifetime, this is acceptable. For applications that frequently
/// load/unload plugins, consider using a different integration approach.
#[derive(Clone)]
pub struct WasmHandler {
    instance: Arc<Mutex<PluginInstance>>,
    command_name: String,
    /// Leaked at construction time to satisfy HandlerMetadata's 'static requirement.
    /// See struct-level documentation for trade-offs.
    command_about: &'static str,
}

impl WasmHandler {
    /// Create a new WasmHandler from a plugin instance
    ///
    /// Note: `command_about` is leaked once to satisfy HandlerMetadata's 'static requirement.
    /// This is acceptable for long-lived plugin registrations.
    pub fn new(
        instance: PluginInstance,
        command_name: impl Into<String>,
        command_about: impl Into<String>,
    ) -> Self {
        // Leak the description once at construction time instead of on every metadata() call
        let about: String = command_about.into();
        let leaked_about: &'static str = Box::leak(about.into_boxed_str());
        Self {
            instance: Arc::new(Mutex::new(instance)),
            command_name: command_name.into(),
            command_about: leaked_about,
        }
    }

    /// Create from a loaded plugin
    pub fn from_loaded(plugin: LoadedPlugin) -> Self {
        let command_name = plugin.manifest.command.name.clone();
        let command_about = plugin.manifest.command.about.clone();
        Self::new(plugin.instance, command_name, command_about)
    }

    /// Get the command name
    pub fn command_name(&self) -> &str {
        &self.command_name
    }

    /// Get the command description
    pub fn command_about(&self) -> &'static str {
        self.command_about
    }
}

impl<S> Handler<(), S> for WasmHandler
where
    S: Clone + Send + Sync + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send + 'static>>;

    fn call(self, _state: State<S>, args: Vec<String>) -> Self::Future {
        Box::pin(async move {
            let mut instance = self.instance.lock().await;

            match instance.execute(&args) {
                Ok(ExecuteResult::Success(output)) => Response::text(output),
                Ok(ExecuteResult::Error(err)) => Response::error(err.code as i32, err.message),
                Err(e) => Response::error(101, format!("Plugin execution error: {}", e)),
            }
        })
    }

    fn metadata(&self) -> Option<HandlerMetadata> {
        // command_about is already leaked at construction time
        Some(HandlerMetadata {
            desc: Some(self.command_about),
            tier: None,
            tags: None,
        })
    }
}

/// Extension trait for Router to integrate plugins
pub trait RouterPluginExt<S>
where
    S: Clone + Send + Sync + 'static,
{
    /// Register a loaded plugin's command
    fn plugin(self, plugin: LoadedPlugin) -> Self;

    /// Register a plugin with a custom route prefix
    fn plugin_with_prefix(self, prefix: &str, plugin: LoadedPlugin) -> Self;
}

impl<S> RouterPluginExt<S> for sen::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn plugin(self, plugin: LoadedPlugin) -> Self {
        let route_name = plugin.manifest.command.name.clone();
        let handler = WasmHandler::from_loaded(plugin);
        self.route(route_name, handler)
    }

    fn plugin_with_prefix(self, prefix: &str, plugin: LoadedPlugin) -> Self {
        let route_name = if prefix.is_empty() {
            plugin.manifest.command.name.clone()
        } else {
            format!("{}:{}", prefix, plugin.manifest.command.name)
        };
        let handler = WasmHandler::from_loaded(plugin);
        self.route(route_name, handler)
    }
}

/// Register multiple plugins from a directory
pub fn register_plugins_from_spec<S>(
    mut router: sen::Router<S>,
    plugins: Vec<LoadedPlugin>,
) -> sen::Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    for plugin in plugins {
        router = router.plugin(plugin);
    }
    router
}

/// Generate help text for a plugin command
pub fn generate_plugin_help(spec: &CommandSpec) -> String {
    let mut help = format!("{}\n\n", spec.about);

    if !spec.args.is_empty() {
        help.push_str("ARGUMENTS:\n");
        for arg in &spec.args {
            let required = if arg.required { " (required)" } else { "" };
            help.push_str(&format!("  {}{}\n", arg.name, required));
            if !arg.help.is_empty() {
                help.push_str(&format!("      {}\n", arg.help));
            }
            if let Some(default) = &arg.default_value {
                help.push_str(&format!("      [default: {}]\n", default));
            }
        }
    }

    if !spec.subcommands.is_empty() {
        help.push_str("\nSUBCOMMANDS:\n");
        for sub in &spec.subcommands {
            help.push_str(&format!("  {}    {}\n", sub.name, sub.about));
        }
    }

    help
}

#[cfg(test)]
mod tests {
    use super::*;
    use sen_plugin_api::ArgSpec;

    #[test]
    fn test_generate_plugin_help() {
        let spec = CommandSpec::new("hello", "Says hello to the world")
            .version("1.0.0")
            .arg(
                ArgSpec::positional("name")
                    .help("Name to greet")
                    .default("World"),
            );

        let help = generate_plugin_help(&spec);
        assert!(help.contains("Says hello"));
        assert!(help.contains("name"));
        assert!(help.contains("Name to greet"));
        assert!(help.contains("[default: World]"));
    }
}
