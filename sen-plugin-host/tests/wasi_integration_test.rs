//! Integration tests for WASI plugin loader
//!
//! These tests verify that plugins can access system resources
//! through WASI capabilities.

#![cfg(feature = "wasi")]

use sen_plugin_api::ExecuteResult;
use sen_plugin_host::wasi::{WasiLoaderConfig, WasiPluginLoader};
use std::path::PathBuf;

// Include the WASI-enabled plugins
const FILE_READER_PLUGIN_WASM: &[u8] = include_bytes!(
    "../../examples/file-reader-plugin/target/wasm32-wasip1/release/file_reader_plugin.wasm"
);

// Zig WASI plugin (env-reader)
const ENV_READER_ZIG_WASM: &[u8] =
    include_bytes!("../../examples/env-reader-plugin-zig/zig-out/bin/env_reader_plugin.wasm");

fn get_example_data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("examples")
        .join("file-reader-plugin")
        .join("data")
        .canonicalize()
        .expect("data directory should exist")
}

#[test]
fn test_load_wasi_plugin() {
    let loader = WasiPluginLoader::new(WasiLoaderConfig {
        working_directory: get_example_data_dir().parent().unwrap().to_path_buf(),
        require_existence: false,
        ..Default::default()
    })
    .expect("loader creation should succeed");

    let plugin = loader
        .load(FILE_READER_PLUGIN_WASM)
        .expect("plugin load should succeed");

    assert_eq!(plugin.manifest.command.name, "file-reader");
    assert!(!plugin.manifest.capabilities.fs_read.is_empty());
    assert!(!plugin.manifest.capabilities.env_read.is_empty());
}

#[test]
fn test_wasi_plugin_capabilities() {
    let loader = WasiPluginLoader::new(WasiLoaderConfig {
        working_directory: get_example_data_dir().parent().unwrap().to_path_buf(),
        require_existence: false,
        ..Default::default()
    })
    .expect("loader creation should succeed");

    let plugin = loader
        .load(FILE_READER_PLUGIN_WASM)
        .expect("plugin load should succeed");

    let caps = &plugin.manifest.capabilities;

    // Check fs_read capability
    assert_eq!(caps.fs_read.len(), 1);
    assert_eq!(caps.fs_read[0].pattern, "./data");
    assert!(caps.fs_read[0].recursive);

    // Check env_read capability
    assert!(caps.env_read.contains(&"HOME".to_string()));
    assert!(caps.env_read.contains(&"USER".to_string()));

    // Check stdio capability
    assert!(!caps.stdio.stdin);
    assert!(caps.stdio.stdout);
    assert!(caps.stdio.stderr);
}

#[test]
fn test_wasi_plugin_read_file() {
    let working_dir = get_example_data_dir().parent().unwrap().to_path_buf();

    let loader = WasiPluginLoader::new(WasiLoaderConfig {
        working_directory: working_dir,
        require_existence: true,
        ..Default::default()
    })
    .expect("loader creation should succeed");

    let plugin = loader
        .load(FILE_READER_PLUGIN_WASM)
        .expect("plugin load should succeed");

    // Execute plugin to read a file
    let result = plugin
        .instance
        .execute(&["sample.txt".to_string()])
        .expect("execution should succeed");

    match result {
        ExecuteResult::Success(output) => {
            assert!(output.contains("Hello from WASI!"));
            assert!(output.contains("USER:"));
            assert!(output.contains("HOME:"));
        }
        ExecuteResult::Error(e) => {
            panic!("Plugin execution failed: {}", e.message);
        }
        ExecuteResult::Effect(e) => {
            panic!("Unexpected effect: {:?}", e);
        }
    }
}

#[test]
fn test_wasi_plugin_file_not_found() {
    let working_dir = get_example_data_dir().parent().unwrap().to_path_buf();

    let loader = WasiPluginLoader::new(WasiLoaderConfig {
        working_directory: working_dir,
        require_existence: true,
        ..Default::default()
    })
    .expect("loader creation should succeed");

    let plugin = loader
        .load(FILE_READER_PLUGIN_WASM)
        .expect("plugin load should succeed");

    // Try to read a non-existent file
    let result = plugin
        .instance
        .execute(&["nonexistent.txt".to_string()])
        .expect("execution should succeed");

    match result {
        ExecuteResult::Error(e) => {
            assert!(e.message.contains("Failed to read"));
        }
        ExecuteResult::Success(_) => {
            panic!("Expected error for non-existent file");
        }
        ExecuteResult::Effect(e) => {
            panic!("Unexpected effect: {:?}", e);
        }
    }
}

// =============================================================================
// Zig WASI Plugin Tests
// =============================================================================

#[test]
fn test_load_zig_wasi_plugin() {
    let loader = WasiPluginLoader::new(WasiLoaderConfig {
        working_directory: std::env::current_dir().unwrap(),
        require_existence: false,
        ..Default::default()
    })
    .expect("loader creation should succeed");

    let plugin = loader
        .load(ENV_READER_ZIG_WASM)
        .expect("Zig plugin load should succeed");

    assert_eq!(plugin.manifest.command.name, "env-reader");
    assert!(!plugin.manifest.capabilities.env_read.is_empty());
}

#[test]
fn test_zig_wasi_plugin_capabilities() {
    let loader = WasiPluginLoader::new(WasiLoaderConfig {
        working_directory: std::env::current_dir().unwrap(),
        require_existence: false,
        ..Default::default()
    })
    .expect("loader creation should succeed");

    let plugin = loader
        .load(ENV_READER_ZIG_WASM)
        .expect("Zig plugin load should succeed");

    let caps = &plugin.manifest.capabilities;

    // Check env_read capability (USER, HOME, PATH, SHELL declared)
    assert!(caps.env_read.contains(&"USER".to_string()));
    assert!(caps.env_read.contains(&"HOME".to_string()));
    assert!(caps.env_read.contains(&"PATH".to_string()));
    assert!(caps.env_read.contains(&"SHELL".to_string()));

    // Check stdio capability
    assert!(!caps.stdio.stdin);
    assert!(caps.stdio.stdout);
    assert!(caps.stdio.stderr);
}

#[test]
fn test_zig_wasi_plugin_read_env() {
    let loader = WasiPluginLoader::new(WasiLoaderConfig {
        working_directory: std::env::current_dir().unwrap(),
        require_existence: false,
        ..Default::default()
    })
    .expect("loader creation should succeed");

    let plugin = loader
        .load(ENV_READER_ZIG_WASM)
        .expect("Zig plugin load should succeed");

    // Execute plugin to read USER env var
    let result = plugin
        .instance
        .execute(&["USER".to_string()])
        .expect("execution should succeed");

    match result {
        ExecuteResult::Success(output) => {
            // Should contain the USER variable output
            assert!(output.contains("USER"), "Output should mention USER");
        }
        ExecuteResult::Error(e) => {
            panic!("Zig plugin execution failed: {}", e.message);
        }
        ExecuteResult::Effect(e) => {
            panic!("Unexpected effect: {:?}", e);
        }
    }
}
