# WASM Plugin CLI Example

A complete demonstration of building a plugin-extensible CLI in Rust using WASM plugins with hot reload support.

## Features

- **Hot Reload**: Plugins automatically load/unload when files change
- **Plugin Discovery**: Watches `./plugins/` directory for `.wasm` files
- **Unified Interface**: Built-in commands + plugin commands in one CLI
- **REPL Mode**: Interactive shell for testing plugins
- **Sandboxed Execution**: Plugins run in WASM sandbox with CPU limits

## Quick Start

### 1. Build the Example Plugins

```bash
# Build hello-plugin
cd ../hello-plugin
cargo build --release --target wasm32-unknown-unknown

# Build greet-plugin
cd ../greet-plugin
cargo build --release --target wasm32-unknown-unknown
```

### 2. Copy Plugins to plugins/ Directory

```bash
cd ../wasm-cli
mkdir -p plugins

cp ../hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm plugins/
cp ../greet-plugin/target/wasm32-unknown-unknown/release/greet_plugin.wasm plugins/
```

### 3. Run the CLI

```bash
# Build and run
cargo run

# Or build first
cargo build
./target/debug/wasm-cli
```

## Usage

### REPL Mode (Interactive)

```bash
$ cargo run
WASM Plugin CLI v0.1.0
Type 'help' for available commands, 'quit' to exit.
Plugins are hot-reloaded from ./plugins/

> help
BUILT-IN COMMANDS:
  help, ?           Show this help message
  plugins, list     List loaded plugins
  version           Show version
  quit, exit, q     Exit REPL mode

PLUGIN COMMANDS:
  greet             Greets a person with a custom message
  hello             Says hello to the world

> hello Rust
Hello, Rust!

> greet Alice "Good morning"
Good morning, Alice!

> quit
Goodbye!
```

### Single Command Mode

```bash
# Run plugin directly
$ cargo run -- hello World
Hello, World!

# List plugins
$ cargo run -- plugins
Loaded plugins (2):

  greet (v1.0.0)
    Greets a person with a custom message
    Args: name, greeting

  hello (v1.0.0)
    Says hello to the world
    Args: name
```

## Hot Reload

While the CLI is running, you can:

1. **Add plugins**: Copy a `.wasm` file to `./plugins/` → automatically loaded
2. **Update plugins**: Rebuild and overwrite `.wasm` file → automatically reloaded
3. **Remove plugins**: Delete `.wasm` file → automatically unloaded

## Creating Your Own Plugin

See `../hello-plugin/` and `../greet-plugin/` for examples.

### Minimal Plugin Template

```rust
use sen_plugin_sdk::prelude::*;

struct MyPlugin;

impl Plugin for MyPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest::new(
            CommandSpec::new("mycommand", "Description of my command")
                .version("1.0.0")
                .arg(ArgSpec::positional("input").help("Input value"))
        )
    }

    fn execute(args: Vec<String>) -> ExecuteResult {
        let input = args.first().map(|s| s.as_str()).unwrap_or("default");
        ExecuteResult::success(format!("Result: {}", input))
    }
}

export_plugin!(MyPlugin);
```

### Build Your Plugin

```bash
cargo build --release --target wasm32-unknown-unknown
cp target/wasm32-unknown-unknown/release/my_plugin.wasm /path/to/wasm-cli/plugins/
```

## Architecture

```
wasm-cli/
├── src/main.rs      # CLI application
├── plugins/         # Plugin directory (watched for changes)
│   ├── hello_plugin.wasm
│   └── greet_plugin.wasm
└── Cargo.toml
```

## Dependencies

- `sen-plugin-host`: WASM plugin runtime with hot reload
- `sen-plugin-api`: Plugin manifest and result types
- `tokio`: Async runtime
