//! Example CLI demonstrating WASM plugin loading and execution
//!
//! This example shows how to:
//! - Load WASM plugins from files
//! - Execute plugin commands
//! - List available plugins
//!
//! # Usage
//!
//! ```bash
//! # List available commands
//! wasm-cli help
//!
//! # Load and execute a plugin
//! wasm-cli load ../hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm
//! wasm-cli run hello World
//!
//! # List loaded plugins
//! wasm-cli plugins
//! ```

use sen_plugin_api::ExecuteResult;
use sen_plugin_host::{PluginLoader, PluginRegistry};
use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = env::args().skip(1).collect();

    if args.is_empty() {
        print_help();
        return ExitCode::SUCCESS;
    }

    match args[0].as_str() {
        "help" | "--help" | "-h" => {
            print_help();
            ExitCode::SUCCESS
        }
        "version" | "--version" | "-V" => {
            println!("wasm-cli 0.1.0");
            ExitCode::SUCCESS
        }
        "run" => run_plugin(&args[1..]).await,
        "info" => show_plugin_info(&args[1..]).await,
        _ => {
            eprintln!("Unknown command: {}", args[0]);
            eprintln!("Run 'wasm-cli help' for usage information.");
            ExitCode::FAILURE
        }
    }
}

fn print_help() {
    println!(
        r#"wasm-cli - WASM Plugin CLI Example

USAGE:
    wasm-cli <COMMAND> [OPTIONS]

COMMANDS:
    run <plugin.wasm> [args...]    Load and execute a WASM plugin
    info <plugin.wasm>             Show plugin manifest information
    help                           Show this help message
    version                        Show version information

EXAMPLES:
    # Run hello plugin
    wasm-cli run ../hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm World

    # Show plugin info
    wasm-cli info ../hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm
"#
    );
}

async fn run_plugin(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: Plugin path required");
        eprintln!("Usage: wasm-cli run <plugin.wasm> [args...]");
        return ExitCode::FAILURE;
    }

    let plugin_path = PathBuf::from(&args[0]);
    let plugin_args: Vec<String> = args[1..].to_vec();

    // Validate plugin file exists
    if !plugin_path.exists() {
        eprintln!("Error: Plugin file not found: {}", plugin_path.display());
        return ExitCode::FAILURE;
    }

    // Create registry and load plugin
    let registry = match PluginRegistry::new() {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Error: Failed to create plugin registry: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let command_name = match registry.load_plugin(&plugin_path).await {
        Ok(name) => name,
        Err(e) => {
            eprintln!("Error: Failed to load plugin: {}", e);
            return ExitCode::FAILURE;
        }
    };

    println!("Loaded plugin: {}", command_name);

    // Execute the plugin
    match registry.execute(&command_name, &plugin_args).await {
        Ok(result) => match result {
            ExecuteResult::Success(output) => {
                println!("{}", output);
                ExitCode::SUCCESS
            }
            ExecuteResult::Error(err) => {
                eprintln!("Plugin error ({}): {}", err.code, err.message);
                ExitCode::from(err.code)
            }
        },
        Err(e) => {
            eprintln!("Execution error: {}", e);
            ExitCode::FAILURE
        }
    }
}

async fn show_plugin_info(args: &[String]) -> ExitCode {
    if args.is_empty() {
        eprintln!("Error: Plugin path required");
        eprintln!("Usage: wasm-cli info <plugin.wasm>");
        return ExitCode::FAILURE;
    }

    let plugin_path = PathBuf::from(&args[0]);

    // Validate plugin file exists
    if !plugin_path.exists() {
        eprintln!("Error: Plugin file not found: {}", plugin_path.display());
        return ExitCode::FAILURE;
    }

    // Load plugin to get manifest
    let loader = match PluginLoader::new() {
        Ok(l) => l,
        Err(e) => {
            eprintln!("Error: Failed to create plugin loader: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let wasm_bytes = match tokio::fs::read(&plugin_path).await {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!("Error: Failed to read plugin file: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let plugin = match loader.load(&wasm_bytes) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Error: Failed to load plugin: {}", e);
            return ExitCode::FAILURE;
        }
    };

    let manifest = &plugin.manifest;
    let cmd = &manifest.command;

    println!("Plugin Information");
    println!("==================");
    println!("API Version: {}", manifest.api_version);
    println!();
    println!("Command: {}", cmd.name);
    if let Some(version) = &cmd.version {
        println!("Version: {}", version);
    }
    println!("About:   {}", cmd.about);

    if !cmd.args.is_empty() {
        println!();
        println!("Arguments:");
        for arg in &cmd.args {
            let required = if arg.required { " (required)" } else { "" };
            println!("  {}{}", arg.name, required);
            if !arg.help.is_empty() {
                println!("      {}", arg.help);
            }
            if let Some(default) = &arg.default_value {
                println!("      [default: {}]", default);
            }
        }
    }

    if !cmd.subcommands.is_empty() {
        println!();
        println!("Subcommands:");
        for sub in &cmd.subcommands {
            println!("  {}    {}", sub.name, sub.about);
        }
    }

    ExitCode::SUCCESS
}
