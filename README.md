# SEN: CLI Engine

[![Rust](https://img.shields.io/badge/rust-1.70%2B-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](LICENSE)

A type-safe, macro-powered CLI framework.

## 🎯 Philosophy

SEN transforms CLI development from ad-hoc scripts into systematic applications with:

- **Compile-time safety**: Enum-based routing with exhaustiveness checking
- **Zero boilerplate**: Derive macros generate all wiring code
- **Type-driven DI**: Handler parameters injected based on type signature
- **Fixed workflows**: Predictable behavior for humans and AI agents
- **Strict separation**: Prevents the "1000-line main.rs" problem

## 🚀 Quick Start

### Installation

Add to your `Cargo.toml`:

```toml
[dependencies]
sen = { version = "0.1", features = ["clap"] }
clap = { version = "4", features = ["derive"] }
tokio = { version = "1", features = ["full"] }
```

Or use `cargo add`:

```bash
cargo add sen --features clap
cargo add clap --features derive
cargo add tokio --features full
```

### Example (Router API with Clap - Recommended)

```rust
use sen::{Args, CliResult, Router, State};
use clap::Parser;

// 1. Define application state
#[derive(Clone)]
pub struct AppState {
    pub config: String,
}

// 2. Define arguments with Clap derive macro
//    These types are automatically parsed by SEN when clap feature is enabled
#[derive(Parser, Debug)]
struct BuildArgs {
    /// Build in release mode
    #[arg(long)]
    release: bool,

    /// Number of parallel jobs
    #[arg(short, long, default_value = "4")]
    jobs: usize,
}

// Add descriptions to handlers with #[sen::handler] macro (Router API)
// Or use #[sen(desc = "...")] for Enum API (see below)

#[derive(Parser, Debug)]
struct DeployArgs {
    /// Target environment (positional argument)
    environment: String,

    /// Docker image tag
    #[arg(long, default_value = "latest")]
    tag: String,
}

// 3. Implement handlers as async functions
//    Handlers can accept State, Args, or both in any order
//    Use #[sen::handler(desc = "...")] to add descriptions for help
mod handlers {
    use super::*;

    // Handler with State only (no arguments)
    // You can also use: #[sen::handler(desc = "Show application status")]
    pub async fn status(state: State<AppState>) -> CliResult<String> {
        let app = state.read().await;
        Ok(format!("Status: OK (config: {})", app.config))
    }

    // Handler with State + Args
    // Args(args): Args<BuildArgs> - Clap automatically parses CLI arguments here!
    #[sen::handler(desc = "Build the project")]
    pub async fn build(
        state: State<AppState>,
        Args(args): Args<BuildArgs>,  // 👈 Automatic parsing via Clap!
    ) -> CliResult<String> {
        let app = state.read().await;
        let mode = if args.release { "release" } else { "debug" };
        Ok(format!("Building in {} mode with {} jobs (config: {})",
                   mode, args.jobs, app.config))
    }

    // Order doesn't matter! Args can come before State
    #[sen::handler(desc = "Deploy to environment")]
    pub async fn deploy(
        Args(args): Args<DeployArgs>,  // 👈 Clap parses from CLI automatically!
        state: State<AppState>,
    ) -> CliResult<String> {
        let app = state.read().await;
        Ok(format!("Deploying to {} with tag {} (config: {})",
                   args.environment, args.tag, app.config))
    }
}

// 4. Wire it up with Router (< 30 lines of main.rs)
#[tokio::main]
async fn main() {
    // Create application state (shared across all handlers)
    let state = AppState {
        config: "production".to_string(),
    };

    // Build the router with command → handler mappings
    // Use #[sen::sen()] macro to set CLI metadata
    let router = build_router(state);

    // Execute the command from CLI arguments
    let response = router.execute().await;

    // Print output and exit with proper code
    if !response.output.is_empty() {
        println!("{}", response.output);
    }
    std::process::exit(response.exit_code);
}

// Set CLI metadata with #[sen::sen()] macro
#[sen::sen(
    name = "myapp",
    version = "1.0.0",
    about = "My awesome CLI application"
)]
fn build_router(state: AppState) -> Router<()> {
    Router::new()
        .route("status", handlers::status)   // myapp status
        .route("build", handlers::build)     // myapp build [--release] [--jobs N]
        .route("deploy", handlers::deploy)   // myapp deploy <env> [--tag TAG]
        .with_state(state)                   // Inject state into all handlers
}
```

**Usage:**
```bash
myapp status                              # No arguments
myapp build --release --jobs 8            # With arguments
myapp deploy production --tag v1.2.3      # Positional + flags
myapp --help                              # Hierarchical help
myapp build --help                        # Clap auto-generates detailed help
```

**Hierarchical `--help` output** (automatically generated):
```
myapp 1.0.0
My awesome CLI application

Usage: myapp [OPTIONS] <COMMAND>

Other Commands:
  build   Build the project
  deploy  Deploy to environment
  status

Options:
  -h, --help            Print help
      --help --json     Show CLI schema (JSON format)
  -V, --version         Print version
```

**Per-command `--help`** (via Clap):
```
$ myapp build --help
Usage: cmd [OPTIONS]

Options:
      --release          Build in release mode
  -j, --jobs <JOBS>      Number of parallel jobs [default: 4]
  -h, --help             Print help
```

### Example (Enum API with Clap - Type-safe alternative)

```rust
use sen::{Args, CliResult, State, SenRouter};
use clap::Parser;

// 1. Define application state
#[derive(Clone)]
pub struct AppState {
    pub config: String,
}

// 2. Define arguments with Clap derive macro
//    Same as Router API - just derive Parser on your argument types
#[derive(Parser, Debug)]
struct BuildArgs {
    /// Build in release mode
    #[arg(long)]
    release: bool,

    /// Number of parallel jobs
    #[arg(short, long, default_value = "4")]
    jobs: usize,
}

#[derive(Parser, Debug)]
struct DeployArgs {
    /// Target environment (positional argument)
    environment: String,

    /// Docker image tag
    #[arg(long, default_value = "latest")]
    tag: String,
}

// 3. Define commands with SenRouter derive macro
//    This generates the execute() method and routing logic at compile-time
#[derive(SenRouter)]
#[sen(state = AppState)]  // Tell macro what State type to use
enum Commands {
    #[sen(handler = handlers::status, desc = "Show application status")]
    Status,  // No arguments

    #[sen(handler = handlers::build, desc = "Build the project")]
    Build(BuildArgs),  // With Clap-parsed arguments

    #[sen(handler = handlers::deploy, desc = "Deploy to environment")]
    Deploy(DeployArgs),  // Compiler checks ALL variants have handlers!
}

// The macro also generates Commands::help() for displaying all commands
// Example: println!("{}", Commands::help());

// 4. Implement handlers as async functions
//    Same signature style as Router API
mod handlers {
    use super::*;

    // Handler with State only
    pub async fn status(state: State<AppState>) -> CliResult<String> {
        let app = state.read().await;
        Ok(format!("Status: OK (config: {})", app.config))
    }

    // Handler with State + Args
    pub async fn build(
        state: State<AppState>,
        Args(args): Args<BuildArgs>,  // 👈 Clap automatically parses here!
    ) -> CliResult<String> {
        let app = state.read().await;
        let mode = if args.release { "release" } else { "debug" };
        Ok(format!("Building in {} mode with {} jobs (config: {})",
                   mode, args.jobs, app.config))
    }

    // Order doesn't matter! Args can come before State
    pub async fn deploy(
        Args(args): Args<DeployArgs>,  // 👈 Automatic parsing via Clap!
        state: State<AppState>,
    ) -> CliResult<String> {
        let app = state.read().await;
        Ok(format!("Deploying to {} with tag {} (config: {})",
                   args.environment, args.tag, app.config))
    }
}

// 5. Wire it up (< 30 lines of main.rs)
#[tokio::main]
async fn main() {
    // Create application state (shared across all handlers)
    let state = State::new(AppState {
        config: "production".to_string(),
    });

    // Parse command from CLI arguments (your parsing logic)
    let cmd = Commands::parse();

    // Execute! The macro-generated execute() method handles routing
    let response = cmd.execute(state).await;

    // Print output and exit with proper code
    if !response.output.is_empty() {
        println!("{}", response.output);
    }
    std::process::exit(response.exit_code);
}
```

**Key Features of Enum API:**
- **Compile-time safety**: `#[derive(SenRouter)]` macro generates the `execute()` method
- **Exhaustive matching**: Compiler ensures all commands have handlers
- **Clap integration**: Just add `#[derive(Parser)]` to argument types
- **Type-driven DI**: Automatically injects `State<T>` and `Args<T>` based on handler signatures

## 📁 Project Structure

SEN enforces clean file separation from day one:

```
my-cli/
├── src/
│   ├── main.rs              # Entry point only (< 50 lines)
│   ├── handlers/            # One file per command
│   │   ├── mod.rs
│   │   ├── status.rs
│   │   ├── build.rs
│   │   └── test.rs
│   ├── workflows/           # Multi-task operations
│   │   └── preflight.rs     # fmt → lint → test
│   ├── tasks/               # Atomic operations
│   │   ├── fmt.rs
│   │   └── lint.rs
│   └── lib.rs               # Re-exports
```

**Why?**
- Each command is independently testable
- No `println!` debugging (handlers return structured data)
- Impossible to create "1000-line main.rs"
- AI agents can understand and modify specific commands easily

## 🎨 Key Features

### 1. Flexible Routing - Choose Your Style

**Router API (Axum-style)** - Dynamic and flexible:
```rust
// Register handlers dynamically
let router = Router::new()
    .route("status", handlers::status)
    .route("build", handlers::build)
    .with_state(app_state);

// Easy to integrate with existing CLIs
let response = router.execute(&args).await;
```

**Enum API** - Compile-time safety:
```rust
#[derive(SenRouter)]
#[sen(state = AppState)]
enum Commands {
    #[sen(handler = handlers::status)]  // Typo? Compile error!
    Status,
}
```

Both approaches are supported - choose based on your needs:
- **Router API**: Better for gradual migration, dynamic routes, existing CLIs
- **Enum API**: Better for new projects, compile-time exhaustiveness checking

### 2. Axum-Style Handler Signatures

```rust
// Order doesn't matter!
pub async fn handler1(state: State<App>, args: Args) -> CliResult<String>
pub async fn handler2(args: Args, state: State<App>) -> CliResult<String>

// State optional
pub async fn handler3(args: Args) -> CliResult<()>
```

### 3. Smart Error Handling

```rust
pub enum CliError {
    User(UserError),      // Exit code 1: user can fix
    System(SystemError),  // Exit code 101: bug/system failure
}
```

Errors automatically format with helpful hints:

```
Error: Invalid argument '--foo'

The value 'bar' is not supported.

Hint: Use one of: baz, qux
```

### 4. Professional Help Generation

**Automatic hierarchical grouping** - Commands are organized by prefix:

```
$ myctl --help

Configuration Commands:
  edit      Edit configuration in editor
  init      Initialize configuration file
  show      Show current configuration

Database Commands:
  create    Create a new database
  delete    Delete a database
  list      List all databases

Server Commands:
  start     Start server instances
  stop      Stop server instances
```

**Clap integration** - Per-command help with full argument details:

```
$ myctl db create --help
Usage: cmd [OPTIONS] <NAME>

Arguments:
  <NAME>  Database name

Options:
      --size <SIZE>      Storage size [default: 10GB]
      --engine <ENGINE>  Database engine [default: postgres]
  -h, --help             Print help
```

**JSON schema export** for AI agents and IDEs:

```bash
$ myctl --help --json
{
  "commands": {
    "db:create": {
      "description": "Create a new database",
      "arguments": [...],
      "options": [...]
    }
  }
}
```

**How grouping works:**
- Commands with `:` prefix are automatically grouped (e.g., `db:create` → "Database Commands")
- Commands are displayed with just the suffix (e.g., `create` instead of `db:create`)
- Groups are sorted alphabetically, with "Other Commands" last
- Use `#[sen::handler(desc = "...")]` to add descriptions

### 5. No Println! in Handlers

Handlers return structured data, framework handles output:

```rust
// ❌ Bad: Can't test, can't redirect
pub async fn status() -> CliResult<()> {
    println!("Status: OK");
    Ok(())
}

// ✅ Good: Testable, flexible
pub async fn status() -> CliResult<StatusReport> {
    Ok(StatusReport { status: "OK" })
}
```

## 🤖 Agent Mode (Machine-Readable Output)

SEN provides **automatic** AI agent integration through built-in `--agent-mode` flag support.

### Automatic Agent Mode (Recommended)

Simply call `.with_agent_mode()` and the framework handles everything:

```rust
use sen::Router;

#[tokio::main]
async fn main() {
    let router = Router::new()
        .route("build", handlers::build)
        .with_agent_mode()  // Enable automatic --agent-mode support
        .with_state(state);

    let response = router.execute().await;

    // Automatically outputs JSON if --agent-mode was passed
    if response.agent_mode {
        println!("{}", response.to_agent_json());
    } else {
        if !response.output.is_empty() {
            println!("{}", response.output);
        }
    }

    std::process::exit(response.exit_code);
}
```

**User runs:**
```bash
myapp build              # Normal text output
myapp --agent-mode build # JSON output
```

### How It Works

1. **Router detects** `--agent-mode` flag automatically
2. **Strips the flag** before passing args to handlers
3. **Sets `response.agent_mode = true`** for your output logic
4. **Zero boilerplate** - no manual arg parsing needed

### Example Output

```json
{
  "result": "success",
  "exit_code": 0,
  "output": "Build completed successfully",
  "tier": "safe",
  "tags": ["build", "production"],
  "sensors": {
    "timestamp": "2024-01-15T10:30:00Z",
    "os": "macos",
    "cwd": "/Users/user/project"
  }
}
```

### Advanced: Manual Agent Mode

For complex scenarios with global options, you can still manually implement agent mode (see `examples/practical-cli`).

### Features

- **Automatic `--agent-mode` detection**: Framework handles flag parsing
- **`to_agent_json()`**: Converts `Response` to structured JSON
- **Environment Sensors**: Automatic collection of system metadata (requires `sensors` feature)
- **Tier & Tags**: Safety tier and command categorization metadata
- **Structured Errors**: Exit codes and error messages in machine-readable format

## 💡 Argument Parsing: Clap Integration (Recommended)

SEN has **built-in Clap integration** - the de-facto standard for Rust CLI argument parsing.

### 🚀 Use Clap (Recommended for 99% of CLIs)

Simply derive `clap::Parser` on your argument types:

**Step 1**: Enable the `clap` feature:

```toml
[dependencies]
sen = { version = "0.1", features = ["clap"] }
clap = { version = "4", features = ["derive"] }
```

**Step 2**: Define arguments with `#[derive(Parser)]`:

```rust
use sen::{Args, CliResult};
use clap::Parser;

#[derive(Parser, Debug)]
struct BuildArgs {
    /// Build in release mode
    #[arg(long)]
    release: bool,

    /// Number of parallel jobs
    #[arg(short, long, default_value = "4")]
    jobs: usize,
}

async fn build(Args(args): Args<BuildArgs>) -> CliResult<String> {
    let mode = if args.release { "release" } else { "debug" };
    Ok(format!("Building in {} mode with {} jobs", mode, args.jobs))
}
```

**Step 3**: Register the handler - that's it!

```rust
let router = Router::new()
    .route("build", build)
    .with_state(state);
```

**How it works**: When the `clap` feature is enabled, SEN automatically implements `FromArgs` for any type implementing `clap::Parser`. Zero boilerplate required.

**Benefits**:
- ✅ Automatic help generation (`--help`)
- ✅ Type-safe with compile-time validation
- ✅ Supports complex options (enums, lists, subcommands)
- ✅ Battle-tested (used by cargo, ripgrep, etc.)
- ✅ **Recommended for all production CLIs**

**Example `--help` output** (auto-generated from your `#[arg]` attributes):
```bash
$ myapp build --help
Usage: myapp build [OPTIONS]

Options:
      --release          Build in release mode
  -j, --jobs <JOBS>      Number of parallel jobs [default: 4]
  -h, --help             Print help
```

All the documentation comments (`///`) in your struct become help text automatically!

### Global Options (For CLI-wide Flags)

For applications with global flags that apply to all commands:

```rust
use sen::FromGlobalArgs;

#[derive(Clone)]
pub struct GlobalOpts {
    pub verbose: bool,
    pub config_path: String,
}

impl FromGlobalArgs for GlobalOpts {
    fn from_global_args(args: &[String]) -> Result<(Self, Vec<String>), CliError> {
        let mut verbose = false;
        let mut config_path = "~/.config/myapp".to_string();
        let mut remaining_args = Vec::new();

        for arg in args {
            match arg.as_str() {
                "--verbose" | "-v" => verbose = true,
                "--config" => { /* handle next arg */ }
                _ => remaining_args.push(arg.clone()),
            }
        }

        Ok((GlobalOpts { verbose, config_path }, remaining_args))
    }
}
```

**Use Global Options when:**
- ✅ You need flags that apply to **all** commands (`--verbose`, `--config`)
- ✅ You want integration with `clap` or other parsers
- ✅ You have complex validation or conflicting flag logic
- ✅ Building a production CLI (like `practical-cli` example)

### Why practical-cli Uses Global Options

The `practical-cli` example intentionally uses `FromGlobalArgs` instead of `FromArgs`:

1. **Global flags**: `--verbose` and `--config` apply to all commands
2. **`clap` integration**: Uses `clap::Command` for help generation
3. **Flexibility**: Manual parsing allows complex validation
4. **Real-world pattern**: Mirrors production CLI tools like `kubectl`, `docker`, etc.

**Key Insight:** For complex CLIs with global flags, use `FromGlobalArgs` to parse them once, then use Clap's `#[derive(Parser)]` for per-command arguments.

See `examples/practical-cli` for a complete production-ready example showing:
- Global flags with `FromGlobalArgs`
- Per-command arguments with Clap's `#[derive(Parser)]`
- Nested routers for organizing commands by resource

### Advanced: Manual `FromArgs` Implementation (Rarely Needed)

If you need custom parsing logic and **cannot** use Clap, you can manually implement `FromArgs`:

```rust
use sen::{Args, FromArgs, CliError, CliResult};

#[derive(Debug)]
struct CustomArgs {
    flag: bool,
}

impl FromArgs for CustomArgs {
    fn from_args(args: &[String]) -> Result<Self, CliError> {
        // Your custom parsing logic
        Ok(CustomArgs {
            flag: args.contains(&"--flag".to_string()),
        })
    }
}

async fn handler(Args(args): Args<CustomArgs>) -> CliResult<String> {
    Ok(format!("Flag: {}", args.flag))
}
```

**Only use manual `FromArgs` when:**
- ❌ Clap doesn't support your use case (very rare)
- ❌ You need parsing logic that's impossible to express in Clap
- ❌ You're integrating with a non-Clap parser

**For 99% of use cases, use Clap's `#[derive(Parser)]` instead.**

## 🔌 Plugin System (WASM)

SEN provides a secure, cross-platform plugin system powered by WebAssembly.

### Why WASM Plugins?

- **Write Once, Run Anywhere**: Single `.wasm` binary works on all platforms
- **Language Agnostic**: Write plugins in Rust, Zig, or any WASM-compatible language
- **Secure by Default**: Sandboxed execution with CPU/memory limits
- **Hot Reload**: Plugins reload automatically when files change

### Architecture

```
┌─────────────────────────────────────────┐
│  sen-plugin-api                         │
│  Shared protocol types (MessagePack)    │
└─────────────────────────────────────────┘
         ↑                    ↑
         │                    │
┌────────┴────────┐  ┌────────┴────────┐
│ sen-plugin-sdk  │  │ sen-plugin-host │
│ Rust SDK for    │  │ Wasmtime-based  │
│ plugin authors  │  │ plugin runtime  │
└─────────────────┘  └─────────────────┘
```

### Quick Start: Rust Plugin

```rust
use sen_plugin_sdk::prelude::*;

struct GreetPlugin;

impl Plugin for GreetPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest::new(CommandSpec {
            name: "greet".into(),
            description: "Greet someone".into(),
            version: "1.0.0".into(),
            args: vec![ArgSpec::positional("name", "Name to greet")],
            subcommands: vec![],
        })
    }

    fn execute(args: Vec<String>) -> ExecuteResult {
        let name = args.first().map(|s| s.as_str()).unwrap_or("World");
        ExecuteResult::success(format!("Hello, {}!", name))
    }
}

export_plugin!(GreetPlugin);
```

Build with:
```bash
cargo build --release --target wasm32-unknown-unknown
```

### Quick Start: Zig Plugin

```zig
const sdk = @import("sdk/plugin.zig");

pub const plugin = sdk.Plugin{
    .name = "echo",
    .about = "Echoes arguments back",
    .version = "1.0.0",
    .args = &.{
        .{ .name = "message", .description = "Message to echo" },
    },
};

pub fn execute(ctx: *sdk.Context) sdk.Result {
    var args = ctx.args();
    const message = args.next() orelse "No message";
    return sdk.Result.success(message);
}

comptime { sdk.exportPlugin(@This()); }
```

Build with:
```bash
zig build wasm
```

### Router Integration

```rust
use sen::Router;
use sen_plugin_host::{PluginRegistry, RouterPluginExt};

#[tokio::main]
async fn main() {
    let registry = PluginRegistry::new().unwrap();
    registry.load_plugin("./plugins/greet.wasm").await.unwrap();

    let router = Router::new()
        .route("status", handlers::status)
        .plugin(registry.get("greet").unwrap())  // Add plugin as route
        .with_state(state);

    router.execute().await;
}
```

### Hot Reload

```rust
use sen_plugin_host::{PluginRegistry, HotReloadWatcher, WatcherConfig};

let registry = PluginRegistry::new()?;
let _watcher = HotReloadWatcher::new(
    registry.clone(),
    vec!["./plugins"],
    WatcherConfig::default(),
).await?;

// Plugins automatically reload when .wasm files change
```

### Security Model

| Protection | Implementation |
|------------|----------------|
| **CPU Limit** | 10M fuel per execution |
| **Stack Limit** | 1MB WASM stack |
| **Memory Isolation** | Per-plugin linear memory |
| **API Versioning** | Rejects incompatible plugins |
| **Capabilities** | Fine-grained permission system |

### Permission System

Plugins declare required capabilities, and the host controls access:

```rust
use sen_plugin_host::permission::{PermissionPresets, PermissionConfig};

// Choose a preset based on your environment
let config = PermissionPresets::interactive("myapp")?;  // Development
let config = PermissionPresets::ci("myapp", None)?;     // CI/CD
let config = PermissionPresets::strict("myapp")?;       // Production

// Or customize with builder
let config = PermissionConfigBuilder::new()
    .app_name("myapp")
    .strategy(DefaultPermissionStrategy)
    .store(FilePermissionStore::default_for_app("myapp")?)
    .prompt(TerminalPromptHandler::new())
    .build()?;
```

**Trust Flags** for CLI integration:
```bash
myapp --trust-plugin=hello run    # Trust specific plugin
myapp --trust-command=db:migrate  # Trust specific command
```

**Available Strategies**:

| Strategy | Behavior |
|----------|----------|
| Default | Prompts for ungranted permissions |
| Strict | Denies in non-interactive mode |
| Permissive | Allows non-network without prompt |
| CI | Never prompts, requires pre-granted |
| TrustAll | Bypasses all checks (dev only) |

### Plugin Examples

See the examples directory:
- `examples/hello-plugin/` - Manual WASM implementation (Rust)
- `examples/greet-plugin/` - SDK-based plugin (Rust)
- `examples/echo-plugin-zig/` - Zig SDK example

## 🏗️ Architecture

SEN follows a three-layer design:

```
┌─────────────────────────────────────────┐
│  Router Layer (Compile-time)            │
│  - Enum-based command tree              │
│  - Handler binding via proc macros      │
│  - Type-safe routing                    │
└─────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│  Handler Layer (Runtime)                │
│  - Dependency injection (State, Args)   │
│  - Business logic execution             │
│  - Result<T, E> return type             │
└─────────────────────────────────────────┘
                   │
                   ▼
┌─────────────────────────────────────────┐
│  Response Layer (Exit)                  │
│  - Exit code mapping (0, 1, 101)        │
│  - Structured output (JSON/Human)       │
│  - Logging & telemetry                  │
└─────────────────────────────────────────┘
```

See [DESIGN.md](./docs/DESIGN.md) for full architecture details.

## 📚 Examples

Check out the [examples/simple-cli](./examples/simple-cli) directory for a working CLI with:
- Status command (no args)
- Build command (with `--release` flag)
- Test command (with optional filter)
- Proper error handling

Run it:

```bash
cd examples/simple-cli
cargo build
./target/debug/admin status
./target/debug/admin build --release
./target/debug/admin test my_test
```

## 🧪 Testing

```bash
# Run all tests
cargo test

# Test specific crate
cargo test -p sen
cargo test -p sen-rs-macros
```

## 📖 Documentation

- [DESIGN.md](./docs/DESIGN.md) - Complete design document

## 🛣️ Roadmap

- [x] Phase 1: Core framework (State, CliResult, IntoResponse)
- [x] Phase 2: Macro system (#[derive(SenRouter)])
- [x] Phase 3: WASM Plugin System
  - [x] Plugin loading with wasmtime
  - [x] Rust SDK (`sen-plugin-sdk`)
  - [x] Zig SDK
  - [x] Hot reload
  - [x] Router integration
  - [x] Capabilities & Permission system
  - [x] Audit logging
  - [ ] WASI integration (planned)
- [ ] Phase 4: Advanced features (ReloadableConfig, tracing)
- [ ] Phase 5: Developer experience (CLI generator, templates)

## 🤝 Contributing

Contributions welcome! Please read [DESIGN.md](./docs/DESIGN.md) to understand the architecture first.

## 📜 License

Licensed under either of:

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
- MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

## 🙏 Inspiration

SEN is inspired by:
- [Axum](https://github.com/tokio-rs/axum) - Type-safe handler functions
- [Clap](https://github.com/clap-rs/clap) - CLI argument parsing
- The philosophy of Anti-Fragility and Fixed Workflows

---

**SEN** (線/先): The Line to Success, Leading the Future of CLI Development
