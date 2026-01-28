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

use sen_plugin_api::{ExecuteResult, PluginManifest};
use sen_plugin_host::{HotReloadWatcher, PluginRegistry, PluginScanner, WatcherConfig};
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const VERSION: &str = "0.1.0";
const API_VERSION: u32 = 1;
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
        "doctor" => {
            run_doctor().await;
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
        "doctor" => run_doctor().await,
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
    println!("  doctor            Diagnose plugin files");
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

    // Handle --help for plugin commands
    if args
        .first()
        .map(|s| s == "--help" || s == "-h")
        .unwrap_or(false)
    {
        if let Some(manifest) = registry.get_manifest(cmd).await {
            print_plugin_help(&manifest);
            return ExitCode::SUCCESS;
        }
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

/// Print help for a plugin command (clap-style)
fn print_plugin_help(manifest: &PluginManifest) {
    let cmd = &manifest.command;

    // Header: name version - about
    print!("{}", cmd.name);
    if let Some(version) = &cmd.version {
        print!(" {}", version);
    }
    println!();
    println!("{}", cmd.about);
    println!();

    // Usage
    print!("Usage: {}", cmd.name);
    if !cmd.args.is_empty() {
        print!(" [OPTIONS]");
        for arg in &cmd.args {
            if arg.long.is_none() && arg.short.is_none() {
                // Positional argument
                if arg.required {
                    print!(" <{}>", arg.name.to_uppercase());
                } else {
                    print!(" [{}]", arg.name.to_uppercase());
                }
            }
        }
    }
    println!();
    println!();

    // Arguments section (positional)
    let positional: Vec<_> = cmd
        .args
        .iter()
        .filter(|a| a.long.is_none() && a.short.is_none())
        .collect();

    if !positional.is_empty() {
        println!("Arguments:");
        for arg in &positional {
            let mut line = format!("  {:<16}", arg.name);
            if !arg.help.is_empty() {
                line.push_str(&format!("  {}", arg.help));
            }
            if let Some(default) = &arg.default_value {
                line.push_str(&format!(" [default: {}]", default));
            }
            println!("{}", line);
        }
        println!();
    }

    // Options section
    let options: Vec<_> = cmd
        .args
        .iter()
        .filter(|a| a.long.is_some() || a.short.is_some())
        .collect();

    println!("Options:");
    for arg in &options {
        let mut opt_str = String::new();
        if let Some(short) = arg.short {
            opt_str.push_str(&format!("-{}", short));
            if arg.long.is_some() {
                opt_str.push_str(", ");
            }
        } else {
            opt_str.push_str("    ");
        }
        if let Some(long) = &arg.long {
            opt_str.push_str(&format!("--{}", long));
        }
        if let Some(value_name) = &arg.value_name {
            opt_str.push_str(&format!(" <{}>", value_name));
        }

        let mut line = format!("  {:<20}", opt_str);
        if !arg.help.is_empty() {
            line.push_str(&format!("  {}", arg.help));
        }
        if let Some(default) = &arg.default_value {
            line.push_str(&format!(" [default: {}]", default));
        }
        println!("{}", line);
    }
    println!("  {:<20}  Print help", "-h, --help");
}

/// Run plugin diagnostics
async fn run_doctor() {
    let plugin_dir = PathBuf::from(DEFAULT_PLUGIN_DIR);

    println!("Plugin Doctor");
    println!("=============");
    println!();
    println!("Checking plugins in {}/ ...", plugin_dir.display());
    println!();

    if !plugin_dir.exists() {
        println!("  (directory does not exist)");
        println!();
        println!("Create the plugins directory and add .wasm files.");
        return;
    }

    // Get list of .wasm files
    let wasm_files: Vec<_> = match std::fs::read_dir(&plugin_dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "wasm")
                    .unwrap_or(false)
            })
            .collect(),
        Err(e) => {
            eprintln!("  Error reading directory: {}", e);
            return;
        }
    };

    if wasm_files.is_empty() {
        println!("  (no .wasm files found)");
        println!();
        println!(
            "Copy .wasm plugin files to {}/ to use them.",
            plugin_dir.display()
        );
        return;
    }

    let scanner = match PluginScanner::new() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("  Failed to create scanner: {}", e);
            return;
        }
    };

    let mut ok_count = 0;
    let mut fail_count = 0;

    for entry in wasm_files {
        let path = entry.path();
        let filename = path.file_name().unwrap_or_default().to_string_lossy();

        // Check file size
        let metadata = match std::fs::metadata(&path) {
            Ok(m) => m,
            Err(e) => {
                println!("\u{2717} {}", filename);
                println!("    Error: Cannot read file: {}", e);
                println!();
                fail_count += 1;
                continue;
            }
        };
        let size_kb = metadata.len() / 1024;

        // Check WASM magic bytes
        let bytes = match std::fs::read(&path) {
            Ok(b) => b,
            Err(e) => {
                println!("\u{2717} {}", filename);
                println!("    Error: Cannot read file: {}", e);
                println!();
                fail_count += 1;
                continue;
            }
        };

        if bytes.len() < 8 || &bytes[0..4] != b"\0asm" {
            println!("\u{2717} {}", filename);
            println!("    Error: Not a valid WASM file (bad magic bytes)");
            println!("    Suggestion: Rebuild with correct WASM target");
            println!();
            fail_count += 1;
            continue;
        }

        // Try to load as plugin
        let result = scanner.scan_directory(&plugin_dir);
        let plugin_result = match &result {
            Ok(r) => r
                .plugins
                .iter()
                .find(|p| {
                    path.file_stem()
                        .map(|s| s.to_string_lossy().contains(&p.manifest.command.name))
                        .unwrap_or(false)
                })
                .map(|p| Ok(p.manifest.clone()))
                .or_else(|| {
                    r.failures
                        .iter()
                        .find(|(p, _)| p == &path)
                        .map(|(_, e)| Err(e.to_string()))
                }),
            Err(e) => Some(Err(e.to_string())),
        };

        match plugin_result {
            Some(Ok(manifest)) => {
                println!("\u{2713} {} ({}KB)", filename, size_kb);
                println!("    Command: {}", manifest.command.name);
                println!(
                    "    API: v{} {}",
                    manifest.api_version,
                    if manifest.api_version == API_VERSION {
                        "(compatible)"
                    } else {
                        "(INCOMPATIBLE!)"
                    }
                );
                if let Some(version) = &manifest.command.version {
                    println!("    Version: {}", version);
                }
                if !manifest.command.args.is_empty() {
                    let args: Vec<_> = manifest.command.args.iter().map(|a| &a.name).collect();
                    println!(
                        "    Args: [{}]",
                        args.into_iter()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                println!();
                ok_count += 1;
            }
            Some(Err(e)) => {
                println!("\u{2717} {} ({}KB)", filename, size_kb);
                println!("    Error: {}", e);

                // Provide suggestions based on error
                if e.contains("not found") {
                    println!(
                        "    Suggestion: Ensure plugin exports plugin_manifest and plugin_execute"
                    );
                } else if e.contains("version") {
                    println!(
                        "    Suggestion: Rebuild with sen-plugin-sdk {}.x",
                        API_VERSION
                    );
                } else if e.contains("Deserialization") {
                    println!("    Suggestion: Check manifest format matches sen-plugin-api");
                }
                println!();
                fail_count += 1;
            }
            None => {
                // File exists but wasn't processed - try direct load
                println!("? {} ({}KB)", filename, size_kb);
                println!("    Status: Unknown (scan did not process this file)");
                println!();
            }
        }
    }

    println!("Summary: {} OK, {} failed", ok_count, fail_count);
}
