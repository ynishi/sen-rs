//! Integration tests for plugin loading and execution

use sen_plugin_api::{Capabilities, ExecuteResult, PathPattern, StdioCapability};
use sen_plugin_host::permission::{
    AutoPromptHandler, CiPermissionStrategy, DefaultPermissionStrategy, MemoryPermissionStore,
    PermissionConfig, PermissionPresets, PermissionStore, PromptResult, RecordingPromptHandler,
    TrustFlagConfig,
};
use sen_plugin_host::{audit, PluginLoader, PluginRegistry};

const HELLO_PLUGIN_WASM: &[u8] = include_bytes!(
    "../../examples/hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm"
);

const GREET_PLUGIN_WASM: &[u8] = include_bytes!(
    "../../examples/greet-plugin/target/wasm32-unknown-unknown/release/greet_plugin.wasm"
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

// Tests for SDK-based greet plugin
#[test]
fn test_load_greet_plugin_sdk() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let plugin = loader
        .load(GREET_PLUGIN_WASM)
        .expect("Failed to load plugin");

    assert_eq!(plugin.manifest.command.name, "greet");
    assert_eq!(
        plugin.manifest.command.about,
        "Greets a person with a custom message"
    );
    assert_eq!(plugin.manifest.command.args.len(), 2);
}

#[test]
fn test_execute_greet_plugin_sdk() {
    let loader = PluginLoader::new().expect("Failed to create loader");
    let mut plugin = loader
        .load(GREET_PLUGIN_WASM)
        .expect("Failed to load plugin");

    // Execute with custom name
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

// ============================================================================
// Permission Integration Tests
// ============================================================================

/// Test: Full permission workflow with registry
///
/// Verifies: load plugin -> check capabilities -> prompt -> store -> execute
#[tokio::test]
async fn test_permission_workflow_end_to_end() {
    // Setup: registry with recording prompt and memory store
    let store = std::sync::Arc::new(MemoryPermissionStore::new());
    let prompt = std::sync::Arc::new(RecordingPromptHandler::new(PromptResult::AllowAlways));
    let audit_sink = std::sync::Arc::new(audit::MemoryAuditSink::new());

    let config = PermissionConfig {
        strategy: std::sync::Arc::new(DefaultPermissionStrategy),
        store: store.clone(),
        prompt: prompt.clone(),
        audit: audit_sink.clone(),
        trust_flags: TrustFlagConfig::default(),
    };

    let registry = PluginRegistry::with_permissions(config).unwrap();

    // Load plugin with capabilities
    let loader = PluginLoader::new().unwrap();
    let mut plugin = loader.load(HELLO_PLUGIN_WASM).unwrap();
    plugin.manifest.capabilities = Capabilities::default()
        .with_stdio(StdioCapability::stdout_only())
        .with_fs_read(vec![PathPattern::new("./data")]);

    registry.register(plugin).await;

    // First execution: should prompt
    let result = registry
        .execute("hello", &["First".to_string()])
        .await
        .unwrap();

    assert!(matches!(result, ExecuteResult::Success(_)));
    assert_eq!(prompt.prompt_count(), 1, "Should have prompted once");

    // Permission should be stored
    assert!(
        store.get("hello").unwrap().is_some(),
        "Permission should be stored"
    );

    // Second execution: should NOT prompt (already granted)
    let result = registry
        .execute("hello", &["Second".to_string()])
        .await
        .unwrap();

    assert!(matches!(result, ExecuteResult::Success(_)));
    assert_eq!(prompt.prompt_count(), 1, "Should NOT prompt again");

    // Verify audit trail
    let events = audit_sink.events();
    assert!(events.len() >= 2, "Should have audit events");

    let granted_events = audit_sink.find_by_type(audit::AuditEventType::PermissionGranted);
    assert!(!granted_events.is_empty(), "Should have grant event");
}

/// Test: CI mode denies ungranted permissions
#[tokio::test]
async fn test_ci_mode_denies_ungranted() {
    let config = PermissionConfig {
        strategy: std::sync::Arc::new(CiPermissionStrategy),
        store: std::sync::Arc::new(MemoryPermissionStore::new()),
        prompt: std::sync::Arc::new(AutoPromptHandler::always_deny()),
        audit: std::sync::Arc::new(audit::NullAuditSink),
        trust_flags: TrustFlagConfig::disabled(),
    };

    let registry = PluginRegistry::with_permissions(config).unwrap();

    let loader = PluginLoader::new().unwrap();
    let mut plugin = loader.load(HELLO_PLUGIN_WASM).unwrap();
    plugin.manifest.capabilities =
        Capabilities::default().with_fs_read(vec![PathPattern::new("/sensitive")]);

    registry.register(plugin).await;

    // CI mode should deny
    let result = registry.execute("hello", &["Test".to_string()]).await;

    match result {
        Err(sen_plugin_host::RegistryError::PermissionDenied { plugin, reason }) => {
            assert_eq!(plugin, "hello");
            assert!(reason.contains("CI mode"), "Reason should mention CI mode");
        }
        Ok(_) => panic!("Should have denied in CI mode"),
        Err(e) => panic!("Unexpected error: {:?}", e),
    }
}

/// Test: Pre-granted permissions work in CI mode
#[tokio::test]
async fn test_ci_mode_allows_pregranted() {
    use sen_plugin_host::permission::{StoredPermission, StoredTrustLevel};

    let store = std::sync::Arc::new(MemoryPermissionStore::new());

    // Pre-grant permission
    let caps = Capabilities::default().with_stdio(StdioCapability::stdout_only());
    let perm = StoredPermission::new(caps.clone(), StoredTrustLevel::Permanent);
    store.set("hello", perm).unwrap();

    let config = PermissionConfig {
        strategy: std::sync::Arc::new(CiPermissionStrategy),
        store: store.clone(),
        prompt: std::sync::Arc::new(AutoPromptHandler::always_deny()),
        audit: std::sync::Arc::new(audit::NullAuditSink),
        trust_flags: TrustFlagConfig::disabled(),
    };

    let registry = PluginRegistry::with_permissions(config).unwrap();

    let loader = PluginLoader::new().unwrap();
    let mut plugin = loader.load(HELLO_PLUGIN_WASM).unwrap();
    plugin.manifest.capabilities = caps;

    registry.register(plugin).await;

    // Should be allowed (pre-granted)
    let result = registry
        .execute("hello", &["Test".to_string()])
        .await
        .unwrap();

    assert!(matches!(result, ExecuteResult::Success(_)));
}

/// Test: Capability escalation detection
#[tokio::test]
async fn test_capability_escalation_detection() {
    use sen_plugin_host::permission::{StoredPermission, StoredTrustLevel};

    let store = std::sync::Arc::new(MemoryPermissionStore::new());
    let prompt = std::sync::Arc::new(RecordingPromptHandler::new(PromptResult::AllowOnce));
    let audit_sink = std::sync::Arc::new(audit::MemoryAuditSink::new());

    // Pre-grant limited permission
    let old_caps = Capabilities::default().with_stdio(StdioCapability::stdout_only());
    let perm = StoredPermission::new(old_caps, StoredTrustLevel::Permanent);
    store.set("hello", perm).unwrap();

    let config = PermissionConfig {
        strategy: std::sync::Arc::new(DefaultPermissionStrategy),
        store: store.clone(),
        prompt: prompt.clone(),
        audit: audit_sink.clone(),
        trust_flags: TrustFlagConfig::default(),
    };

    let registry = PluginRegistry::with_permissions(config).unwrap();

    let loader = PluginLoader::new().unwrap();
    let mut plugin = loader.load(HELLO_PLUGIN_WASM).unwrap();

    // Request MORE capabilities than granted
    plugin.manifest.capabilities = Capabilities::default()
        .with_stdio(StdioCapability::stdout_only())
        .with_fs_write(vec![PathPattern::new("./output")]); // NEW!

    registry.register(plugin).await;

    // Should trigger escalation detection and prompt
    let _ = registry.execute("hello", &["Test".to_string()]).await;

    // Verify escalation was detected
    let escalation_events = audit_sink.find_by_type(audit::AuditEventType::EscalationDetected);
    assert!(
        !escalation_events.is_empty(),
        "Should detect capability escalation"
    );

    // Should have prompted due to escalation
    let prompts = prompt.prompts();
    assert!(!prompts.is_empty(), "Should prompt on escalation");
    assert!(
        prompts[0].is_escalation,
        "Prompt should be marked as escalation"
    );
}

/// Test: Testing preset allows everything automatically
#[tokio::test]
async fn test_testing_preset_auto_allows() {
    let config = PermissionPresets::testing();
    let registry = PluginRegistry::with_permissions(config).unwrap();

    let loader = PluginLoader::new().unwrap();
    let mut plugin = loader.load(HELLO_PLUGIN_WASM).unwrap();

    // Even with extensive capabilities
    plugin.manifest.capabilities = Capabilities::default()
        .with_fs_read(vec![PathPattern::new("/").recursive()])
        .with_fs_write(vec![PathPattern::new("/tmp")])
        .with_stdio(StdioCapability::all());

    registry.register(plugin).await;

    // Testing preset should auto-allow
    let result = registry
        .execute("hello", &["Test".to_string()])
        .await
        .unwrap();

    assert!(matches!(result, ExecuteResult::Success(_)));
}
