//! Greet Plugin - Using sen-plugin-sdk
//!
//! This example shows how to create a plugin with minimal boilerplate
//! using the sen-plugin-sdk.

use sen_plugin_sdk::prelude::*;

struct GreetPlugin;

impl Plugin for GreetPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest::new(
            CommandSpec::new("greet", "Greets a person with a custom message")
                .version("1.0.0")
                .arg(
                    ArgSpec::positional("name")
                        .help("Name to greet")
                        .default("World"),
                )
                .arg(
                    ArgSpec::option("greeting", "greeting")
                        .short('g')
                        .help("Custom greeting")
                        .default("Hello"),
                ),
        )
    }

    fn execute(args: Vec<String>) -> ExecuteResult {
        // Parse args: [name, greeting] or just [name]
        let name = args.first().map(|s| s.as_str()).unwrap_or("World");
        let greeting = args.get(1).map(|s| s.as_str()).unwrap_or("Hello");

        ExecuteResult::success(format!("{}, {}!", greeting, name))
    }
}

// This single macro generates all 4 required exports:
// - plugin_manifest
// - plugin_execute
// - plugin_alloc
// - plugin_dealloc
export_plugin!(GreetPlugin);
