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

### Example

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

// 3. Implement handlers as plain functions
mod handlers {
    use super::*;

    pub fn status(state: State<AppState>) -> CliResult<String> {
        Ok(format!("Config: {}", state.get().config))
    }

    pub fn build(state: State<AppState>, args: BuildArgs) -> CliResult<()> {
        let mode = if args.release { "release" } else { "debug" };
        println!("Building in {} mode", mode);
        Ok(())
    }
}

// 4. Wire it up (< 50 lines of main.rs)
fn main() {
    let state = State::new(AppState {
        config: "production".to_string(),
    });

    let cmd = Commands::parse(); // Your arg parsing logic
    let response = cmd.execute(state); // Macro-generated!

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

### 1. Type-Safe Routing

Commands are enums, not strings:

```rust
// ✅ Compile-time checked
#[derive(SenRouter)]
#[sen(state = AppState)]
enum Commands {
    #[sen(handler = handlers::status)]  // Typo? Compile error!
    Status,
}

// ❌ Runtime dispatch (other frameworks)
router.add("/status", handlers::status);  // Typo? Runtime panic!
```

### 2. Flexible Handler Signatures (Axum-style)

```rust
// Order doesn't matter!
pub fn handler1(state: State<App>, args: Args) -> CliResult<String>
pub fn handler2(args: Args, state: State<App>) -> CliResult<String>

// State optional
pub fn handler3(args: Args) -> CliResult<()>
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
pub fn status() -> CliResult<()> {
    println!("Status: OK");
    Ok(())
}

// ✅ Good: Testable, flexible
pub fn status() -> CliResult<StatusReport> {
    Ok(StatusReport { status: "OK" })
}
```

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
