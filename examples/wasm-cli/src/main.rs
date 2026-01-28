//! WASM Plugin CLI - Complete Integration Demo
//!
//! Demonstrates how to build a fully plugin-extensible CLI in Rust
//! using WASM plugins with hot reload support.
//!
//! Features:
//! - Hot reload: Plugins are automatically loaded/unloaded when files change
//! - Plugin discovery: Watches a plugins directory for .wasm files
//! - Unified command interface: Built-in + plugin commands in one CLI
//! - Interactive REPL mode for testing plugins
//!
//! # Quick Start
//!
//! ```bash
//! # Create plugins directory
//! mkdir -p plugins
//!
//! # Copy a plugin
//! cp ../hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm plugins/
//!
//! # Run in REPL mode
//! wasm-cli
//!
//! # Or run a single command
//! wasm-cli hello World
//! ```

use sen_plugin_api::ExecuteResult;
use sen_plugin_host::{HotReloadWatcher, PluginRegistry, WatcherConfig};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const VERSION: &str = "0.1.0";
const DEFAULT_PLUGIN_DIR: &str = "plugins";

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // Initialize plugin system
    let (registry, _watcher) = match init_plugin_system().await {
        Ok(r) => r,
        Err(e) => {
            eprintln!("Failed to initialize plugin system: {}", e);
            return ExitCode::FAILURE;
        }
    };

    if args.is_empty() {
        // REPL mode
        run_repl(registry).await
    } else {
        // Single command mode
        run_command(&registry, &args).await
    }
}

async fn init_plugin_system() -> Result<(PluginRegistry, Option<HotReloadWatcher>), String> {
    let registry = PluginRegistry::new().map_err(|e| e.to_string())?;

    let plugin_dir = PathBuf::from(DEFAULT_PLUGIN_DIR);

    // Create plugins directory if it doesn't exist
    if !plugin_dir.exists() {
        std::fs::create_dir_all(&plugin_dir)
            .map_err(|e| format!("Failed to create plugins directory: {}", e))?;
        println!("Created plugins directory: {}", plugin_dir.display());
    }

    // Start hot reload watcher
    let watcher = HotReloadWatcher::new(
        registry.clone(),
        vec![&plugin_dir],
        WatcherConfig {
            debounce: Duration::from_millis(300),
            load_existing: true,
        },
    )
    .await
    .map_err(|e| format!("Failed to start hot reload: {}", e))?;

    let plugin_count = registry.len().await;
    if plugin_count > 0 {
        println!(
            "Loaded {} plugin(s) from {}",
            plugin_count,
            plugin_dir.display()
        );
    }

    Ok((registry, Some(watcher)))
}

async fn run_repl(registry: PluginRegistry) -> ExitCode {
    println!("WASM Plugin CLI v{}", VERSION);
    println!("Type 'help' for available commands, 'quit' to exit.");
    println!("Plugins are hot-reloaded from ./plugins/");
    println!();

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        // Print prompt
        print!("> ");
        stdout.flush().ok();

        // Read line
        let mut line = String::new();
        match stdin.lock().read_line(&mut line) {
            Ok(0) => break, // EOF
            Ok(_) => {}
            Err(e) => {
                eprintln!("Read error: {}", e);
                continue;
            }
        }

        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Parse command and args
        let parts: Vec<&str> = line.split_whitespace().collect();
        let args: Vec<String> = parts.iter().map(|s| s.to_string()).collect();

        // Execute
        let exit = execute_command(&registry, &args).await;
        if exit {
            break;
        }
        println!();
    }

    println!("Goodbye!");
    ExitCode::SUCCESS
}

async fn run_command(registry: &PluginRegistry, args: &[String]) -> ExitCode {
    if args.is_empty() {
        return ExitCode::SUCCESS;
    }

    match args[0].as_str() {
        "help" | "--help" | "-h" => {
            print_help(registry).await;
            ExitCode::SUCCESS
        }
        "version" | "--version" | "-V" => {
            println!("wasm-cli v{}", VERSION);
            ExitCode::SUCCESS
        }
        "plugins" => {
            list_plugins(registry).await;
            ExitCode::SUCCESS
        }
        cmd => {
            // Try plugin command
            execute_plugin(registry, cmd, &args[1..]).await
        }
    }
}

async fn execute_command(registry: &PluginRegistry, args: &[String]) -> bool {
    if args.is_empty() {
        return false;
    }

    match args[0].as_str() {
        "quit" | "exit" | "q" => return true,
        "help" | "?" => print_help(registry).await,
        "version" => println!("wasm-cli v{}", VERSION),
        "plugins" | "list" => list_plugins(registry).await,
        "reload" => {
            println!("Plugins are automatically reloaded when files change.");
            println!("Current plugins:");
            list_plugins(registry).await;
        }
        cmd => {
            let _ = execute_plugin(registry, cmd, &args[1..]).await;
        }
    }

    false
}

async fn print_help(registry: &PluginRegistry) {
    println!("WASM Plugin CLI v{}", VERSION);
    println!();
    println!("BUILT-IN COMMANDS:");
    println!("  help, ?           Show this help message");
    println!("  plugins, list     List loaded plugins");
    println!("  version           Show version");
    println!("  quit, exit, q     Exit REPL mode");
    println!();

    let commands = registry.list_commands().await;
    if commands.is_empty() {
        println!("PLUGIN COMMANDS:");
        println!("  (no plugins loaded)");
        println!();
        println!("To add plugins, copy .wasm files to ./plugins/");
    } else {
        println!("PLUGIN COMMANDS:");
        for cmd in &commands {
            if let Some(manifest) = registry.get_manifest(cmd).await {
                println!("  {:<16}  {}", cmd, manifest.command.about);
            } else {
                println!("  {}", cmd);
            }
        }
    }
}

async fn list_plugins(registry: &PluginRegistry) {
    let commands = registry.list_commands().await;

    if commands.is_empty() {
        println!("No plugins loaded.");
        println!("Copy .wasm files to ./plugins/ to add plugins.");
        return;
    }

    println!("Loaded plugins ({}):", commands.len());
    println!();

    for cmd in &commands {
        if let Some(manifest) = registry.get_manifest(cmd).await {
            let version = manifest.command.version.as_deref().unwrap_or("-");
            println!("  {} (v{})", cmd, version);
            println!("    {}", manifest.command.about);

            if !manifest.command.args.is_empty() {
                let args: Vec<&str> = manifest
                    .command
                    .args
                    .iter()
                    .map(|a| a.name.as_str())
                    .collect();
                println!("    Args: {}", args.join(", "));
            }
            println!();
        }
    }
}

async fn execute_plugin(registry: &PluginRegistry, cmd: &str, args: &[String]) -> ExitCode {
    if !registry.has_command(cmd).await {
        eprintln!("Unknown command: '{}'", cmd);
        eprintln!("Type 'help' for available commands.");
        return ExitCode::FAILURE;
    }

    match registry.execute(cmd, args).await {
        Ok(result) => match result {
            ExecuteResult::Success(output) => {
                println!("{}", output);
                ExitCode::SUCCESS
            }
            ExecuteResult::Error(err) => {
                eprintln!("Error ({}): {}", err.code, err.message);
                ExitCode::from(err.code)
            }
        },
        Err(e) => {
            eprintln!("Execution failed: {}", e);
            ExitCode::FAILURE
        }
    }
}
