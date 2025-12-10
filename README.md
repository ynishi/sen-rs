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
sen = "0.1"
```

Or use `cargo add`:

```bash
cargo add sen
```

### Example (Router API - Recommended)

```rust
use sen::{CliResult, State, Router};

// 1. Define application state
#[derive(Clone)]
pub struct AppState {
    pub config: String,
}

// 2. Implement handlers as async functions
mod handlers {
    use super::*;

    pub async fn status(state: State<AppState>) -> CliResult<String> {
        let app = state.read().await;
        Ok(format!("Config: {}", app.config))
    }

    pub async fn build(state: State<AppState>) -> CliResult<()> {
        println!("Building...");
        Ok(())
    }
}

// 3. Wire it up with Router (< 20 lines of main.rs)
#[tokio::main]
async fn main() {
    let state = AppState {
        config: "production".to_string(),
    };

    let router = Router::new()
        .route("status", handlers::status)
        .route("build", handlers::build)
        .with_state(state);

    let response = router.execute().await;

    if !response.output.is_empty() {
        println!("{}", response.output);
    }
    std::process::exit(response.exit_code);
}
```

### Example (Enum API - Type-safe alternative)

```rust
use sen::{CliResult, State, SenRouter};

// 1. Define application state
pub struct AppState {
    pub config: String,
}

// 2. Define commands with derive macro
#[derive(SenRouter)]
#[sen(state = AppState)]
enum Commands {
    #[sen(handler = handlers::status)]
    Status,

    #[sen(handler = handlers::build)]
    Build(BuildArgs),
}

pub struct BuildArgs {
    pub release: bool,
}

// 3. Implement handlers as async functions
mod handlers {
    use super::*;

    pub async fn status(state: State<AppState>) -> CliResult<String> {
        let app = state.read().await;
        Ok(format!("Config: {}", app.config))
    }

    pub async fn build(state: State<AppState>, args: BuildArgs) -> CliResult<()> {
        let mode = if args.release { "release" } else { "debug" };
        println!("Building in {} mode", mode);
        Ok(())
    }
}

// 4. Wire it up (< 50 lines of main.rs)
#[tokio::main]
async fn main() {
    let state = State::new(AppState {
        config: "production".to_string(),
    });

    let cmd = Commands::parse(); // Your arg parsing logic
    let response = cmd.execute(state).await; // Macro-generated async execute!

    if !response.output.is_empty() {
        println!("{}", response.output);
    }
    std::process::exit(response.exit_code);
}
```

That's it! The `#[derive(SenRouter)]` macro generates the `execute()` method that:
- Routes commands to handlers
- Injects `State<T>` and args automatically
- Converts results into responses with proper exit codes

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

### 4. No Println! in Handlers

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

## 💡 Argument Parsing: `FromArgs` vs Global Options

SEN provides multiple approaches for argument parsing, each suited for different use cases.

### Simple Cases: Use `FromArgs`

For per-command arguments without global flags:

```rust
use sen::{Args, FromArgs, CliError};

#[derive(Debug)]
struct BuildArgs {
    release: bool,
}

impl FromArgs for BuildArgs {
    fn from_args(args: &[String]) -> Result<Self, CliError> {
        Ok(BuildArgs {
            release: args.contains(&"--release".to_string()),
        })
    }
}

async fn build(Args(args): Args<BuildArgs>) -> CliResult<String> {
    let mode = if args.release { "release" } else { "debug" };
    Ok(format!("Building in {} mode", mode))
}
```

**Use `FromArgs` when:**
- ✅ You have simple per-command arguments
- ✅ No global flags needed (`--verbose`, `--config`, etc.)
- ✅ You want the framework to handle everything

### Complex Cases: Use Global Options + Manual Parsing

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

**Key Insight:** `FromArgs` is a convenience feature, not required. For complex CLIs, manual parsing gives you full control.

See `examples/practical-cli` for a complete implementation.

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
- [ ] Phase 3: Advanced features (ReloadableConfig, tracing)
- [ ] Phase 4: Developer experience (CLI generator, templates)

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
