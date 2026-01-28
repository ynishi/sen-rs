//! Integration tests for plugin loading and execution

use sen_plugin_api::ExecuteResult;
use sen_plugin_host::PluginLoader;

const HELLO_PLUGIN_WASM: &[u8] = include_bytes!(
    "../../examples/hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm"
);

#[test]
fn test_load_hello_plugin() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let plugin = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");

    assert_eq!(plugin.manifest.command.name, "hello");
    assert_eq!(plugin.manifest.command.about, "Says hello to the world");
    assert_eq!(plugin.manifest.command.version, Some("1.0.0".to_string()));
    assert_eq!(plugin.manifest.command.args.len(), 1);
    assert_eq!(plugin.manifest.command.args[0].name, "name");
}

#[test]
fn test_execute_hello_plugin_default() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let mut plugin = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");

    // Execute with no args (should use default "World")
    let result = plugin.instance.execute(&[]).expect("Execution failed");

    match result {
        ExecuteResult::Success(output) => {
            assert_eq!(output, "Hello, World!");
        }
        ExecuteResult::Error(e) => {
            panic!("Unexpected error: {:?}", e);
        }
    }
}

#[test]
fn test_execute_hello_plugin_with_name() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let mut plugin = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");

    // Execute with a name
    let result = plugin
        .instance
        .execute(&["Rust".to_string()])
        .expect("Execution failed");

    match result {
        ExecuteResult::Success(output) => {
            assert_eq!(output, "Hello, Rust!");
        }
        ExecuteResult::Error(e) => {
            panic!("Unexpected error: {:?}", e);
        }
    }
}

#[test]
fn test_multiple_executions() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let mut plugin = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");

    // Execute multiple times
    for name in ["Alice", "Bob", "Charlie"] {
        let result = plugin
            .instance
            .execute(&[name.to_string()])
            .expect("Execution failed");

        match result {
            ExecuteResult::Success(output) => {
                assert_eq!(output, format!("Hello, {}!", name));
            }
            ExecuteResult::Error(e) => {
                panic!("Unexpected error: {:?}", e);
            }
        }
    }
}
