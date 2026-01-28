//! Integration tests for plugin-router bridge
//!
//! These tests require the `sen-integration` feature to be enabled.

#![cfg(feature = "sen-integration")]

use sen::Router;
use sen_plugin_host::{PluginLoader, RouterPluginExt};

const HELLO_PLUGIN_WASM: &[u8] = include_bytes!(
    "../../examples/hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm"
);

#[derive(Clone)]
struct TestState;

fn args(strs: &[&str]) -> Vec<String> {
    strs.iter().map(|s| s.to_string()).collect()
}

#[tokio::test]
async fn test_router_plugin_integration() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let plugin = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");

    // Verify manifest
    assert_eq!(plugin.manifest.command.name, "hello");

    let state = TestState;

    // Register plugin with router
    let router = Router::new().plugin(plugin).with_state(state);

    // Execute the plugin command
    let response = router.execute_with(&args(&["test", "hello", "Rust"])).await;

    assert_eq!(response.exit_code, 0);
    match &response.output {
        sen::Output::Text(s) => assert_eq!(s, "Hello, Rust!"),
        _ => panic!("Expected text output"),
    }
}

#[tokio::test]
async fn test_router_plugin_with_prefix() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let plugin = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");

    let state = TestState;

    // Register plugin with prefix
    let router = Router::new()
        .plugin_with_prefix("greet", plugin)
        .with_state(state);

    // Execute with prefixed route
    let response = router
        .execute_with(&args(&["test", "greet:hello", "World"]))
        .await;

    assert_eq!(response.exit_code, 0);
    match &response.output {
        sen::Output::Text(s) => assert_eq!(s, "Hello, World!"),
        _ => panic!("Expected text output"),
    }
}

#[tokio::test]
async fn test_router_multiple_plugins() {
    let loader = PluginLoader::new().expect("Failed to create loader");

    // Load same plugin twice with different prefixes
    let plugin1 = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");
    let plugin2 = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");

    let state = TestState;

    let router = Router::new()
        .plugin_with_prefix("en", plugin1)
        .plugin_with_prefix("greeting", plugin2)
        .with_state(state);

    // Test first plugin
    let response1 = router
        .execute_with(&args(&["test", "en:hello", "Alice"]))
        .await;
    assert_eq!(response1.exit_code, 0);

    // Test second plugin
    let response2 = router
        .execute_with(&args(&["test", "greeting:hello", "Bob"]))
        .await;
    assert_eq!(response2.exit_code, 0);
}

#[tokio::test]
async fn test_plugin_help_integration() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let plugin = loader
        .load(HELLO_PLUGIN_WASM)
        .expect("Failed to load plugin");

    // Verify the plugin has description
    assert_eq!(plugin.manifest.command.about, "Says hello to the world");

    let state = TestState;

    // Register plugin with router
    let router = Router::new().plugin(plugin).with_state(state);

    // Request help
    let response = router.execute_with(&args(&["test", "--help"])).await;

    // Help should show plugin command with its description
    assert_eq!(response.exit_code, 0);
    match &response.output {
        sen::Output::Text(help_text) => {
            // Verify plugin command appears in help
            assert!(
                help_text.contains("hello"),
                "Help should contain 'hello' command"
            );
            // Verify plugin description appears in help
            assert!(
                help_text.contains("Says hello to the world"),
                "Help should contain plugin description"
            );
        }
        _ => panic!("Expected text output for help"),
    }
}
