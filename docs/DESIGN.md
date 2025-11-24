# SEN: Script to System CLI Engine - Design Document

## 1. Problem Statement

### Current State of CLI Development
- CLI tools are typically collections of ad-hoc scripts with inconsistent patterns
- Manual command chaining (`cmd1 && cmd2 && cmd3`) is error-prone and not reproducible
- State management (config, database) is scattered across handlers
- Error handling is inconsistent (exit codes, messages, logging)
- AI agents struggle with unpredictable CLI behavior

### Desired State
A CLI framework that provides:
1. **Compile-time guarantees** for command routing and state injection
2. **Fixed workflows** instead of dynamic command composition
3. **Predictable behavior** for both humans and AI agents
4. **Clear separation** between framework mechanics and business logic

## 2. Core Architecture

### 2.1 Three-Layer Design

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

### 2.2 Responsibility Boundaries

| Component | Responsibilities | NOT Responsible For |
|-----------|------------------|---------------------|
| **Macro System** | Type checking, code generation, wiring | Business logic, error semantics |
| **Framework** | Routing, injection, response formatting | Workflow semantics, task ordering |
| **Application** | Workflow definition, task implementation | Boilerplate, state threading |

## 3. Type System Design

### 3.1 Handler Signature

```rust
// Core types
pub struct State<T>(Arc<T>);
pub type CliResult<T> = Result<T, CliError>;

// Error hierarchy
pub enum CliError {
    User(UserError),      // Exit code 1: user-fixable errors
    System(SystemError),  // Exit code 101: system failures
}

// Handler function signature
fn handler(
    state: State<AppState>,  // Injected by framework
    args: CommandArgs,       // Parsed from CLI
) -> CliResult<impl IntoResponse>
```

### 3.2 Response Types

```rust
pub trait IntoResponse {
    fn into_response(self) -> Response;
}

pub struct Response {
    exit_code: i32,
    output: Output,
    metadata: Metadata,
}

pub enum Output {
    Silent,
    Text(String),
    Json(Value),
    Stream(Box<dyn AsyncRead>),
}
```

## 4. Workflow Design

### 4.1 Task vs Workflow

**Atomic Task**: Single-responsibility unit with clear success/failure semantics
- Location: `src/tasks/{task_name}.rs`
- Interface: `pub fn run(ctx: &TaskContext) -> TaskResult<()>`
- Examples: `fmt::run()`, `lint::run()`, `test::run()`

**Workflow**: Ordered composition of tasks with defined semantics
- Location: `src/workflows/{workflow_name}.rs`
- Interface: `pub fn execute(ctx: &WorkflowContext) -> CliResult<Report>`
- Examples: `preflight::execute()`, `ship::execute()`

### 4.2 Workflow Contract

```rust
// workflows/preflight.rs
pub struct PreflightReport {
    fmt_result: TaskResult<FmtStats>,
    lint_result: TaskResult<LintStats>,
    test_result: TaskResult<TestStats>,
}

pub fn execute(ctx: &WorkflowContext) -> CliResult<PreflightReport> {
    let fmt = tasks::fmt::run(&ctx.task_context())?;
    let lint = tasks::lint::run(&ctx.task_context())?;
    let test = tasks::test::run(&ctx.task_context())?;

    Ok(PreflightReport { fmt, lint, test })
}
```

### 4.3 Anti-Pattern: Dynamic Composition

```rust
// ❌ BAD: User-defined task chains
sen run --chain "fmt,lint,test"

// ✅ GOOD: Named workflows
sen preflight  // Internally: fmt → lint → test (fixed order)
```

## 5. State Management

### 5.1 Configuration Strategy

**Principle**: Immutable-by-default, mutable-where-necessary

```rust
pub trait ConfigProvider: Send + Sync {
    fn get(&self) -> Arc<Config>;
}

// Default: Static config (99% of commands)
pub struct StaticConfig {
    config: Arc<Config>,
}

impl ConfigProvider for StaticConfig {
    fn get(&self) -> Arc<Config> {
        Arc::clone(&self.config)
    }
}

// Long-running commands: Reloadable config
pub struct ReloadableConfig {
    config: ArcSwap<Config>,
    watcher: Option<FileWatcher>,
}

impl ConfigProvider for ReloadableConfig {
    fn get(&self) -> Arc<Config> {
        self.config.load_full()
    }
}
```

### 5.2 State Lifecycle

```
CLI Start
    │
    ├─→ Load Config (StaticConfig)
    │   │
    │   ├─→ Short-lived commands (preflight, build, etc.)
    │   │   └─→ Use initial config throughout execution
    │   │
    │   └─→ Exit
    │
    └─→ Load Config (ReloadableConfig)
        │
        ├─→ Long-running commands (serve, watch)
        │   │
        │   ├─→ Detect config file change
        │   ├─→ Reload config (atomic swap)
        │   └─→ Continue with new config
        │
        └─→ Exit on signal
```

## 6. Error Handling Strategy

### 6.1 Error Classification

```rust
pub enum CliError {
    // User-fixable errors (exit code 1)
    User(UserError),
}

pub enum UserError {
    InvalidArgument { arg: String, reason: String },
    MissingDependency { tool: String, install_hint: String },
    ValidationFailed { details: Vec<String> },
    PrerequisiteNotMet { check: String, fix_hint: String },
}

pub enum SystemError {
    // System-level failures (exit code 101)
    IoError(io::Error),
    ConfigParseError(toml::Error),
    InternalError(String),
}
```

### 6.2 Error Display Contract

```
User Errors (Exit 1):
┌────────────────────────────────────────┐
│ Error: Invalid argument '--foo'       │
│                                        │
│ The value 'bar' is not supported.     │
│                                        │
│ Hint: Use one of: baz, qux            │
│                                        │
│ See: sen help <command>                │
└────────────────────────────────────────┘

System Errors (Exit 101):
┌────────────────────────────────────────┐
│ Internal Error: Config parse failed    │
│                                        │
│ at: src/config.rs:42                   │
│                                        │
│ This is likely a bug. Please report:  │
│ https://github.com/.../issues          │
└────────────────────────────────────────┘
```

## 7. Macro System Design

### 7.1 Design Philosophy: Axum-Inspired Ergonomics

The macro system is inspired by Axum's handler function design:
- **Zero boilerplate**: Handlers are plain functions, no trait implementations
- **Type-driven injection**: Parameters are injected based on their type signature
- **Compile-time validation**: Invalid handler signatures are caught at compile time
- **Single derive**: `#[derive(SenRouter)]` generates all routing code

### 7.2 Router Macro (`#[derive(SenRouter)]`)

#### Usage

```rust
#[derive(Subcommand, SenRouter)]
#[sen(state = AppState)]
enum Commands {
    #[sen(handler = handlers::preflight)]
    Preflight,

    #[sen(handler = handlers::serve)]
    Serve(ServeArgs),

    #[sen(handler = handlers::build)]
    Build(BuildArgs),
}
```

#### Generated Code

The macro generates an `execute()` method that:
1. Matches on the enum variant
2. Injects `State<T>` and args (if present)
3. Calls the handler function
4. Converts the result into a `Response`

```rust
// Generated by #[derive(SenRouter)]
impl Commands {
    pub fn execute(self, state: sen::State<AppState>) -> sen::Response {
        use sen::IntoResponse;

        match self {
            Commands::Preflight => {
                handlers::preflight(state).into_response()
            }
            Commands::Serve(args) => {
                handlers::serve(state, args).into_response()
            }
            Commands::Build(args) => {
                handlers::build(state, args).into_response()
            }
        }
    }
}
```

**Key Points:**
- Handler paths are validated at compile time (no typos)
- State type is extracted from `#[sen(state = T)]`
- Unit variants (`Preflight`) inject only state
- Tuple variants (`Serve(Args)`) inject both state and args

### 7.3 Handler Function Patterns

Handlers are plain functions with flexible signatures:

```rust
// Pattern 1: State only (no arguments)
pub fn preflight(state: State<AppState>) -> CliResult<String> {
    // Access state
    let config = &state.get().config;
    Ok("Preflight checks passed".to_string())
}

// Pattern 2: State + Args (Axum-style, order doesn't matter)
pub fn serve(state: State<AppState>, args: ServeArgs) -> CliResult<String> {
    // Both state and args available
    let port = args.port;
    Ok(format!("Server started on port {}", port))
}

// Pattern 3: Args-first (same as Pattern 2, different order)
pub fn build(args: BuildArgs, state: State<AppState>) -> CliResult<()> {
    // Order is flexible
    Ok(())
}
```

**Return Type Flexibility:**

```rust
// Return String (printed to stdout)
fn status(state: State<AppState>) -> CliResult<String>

// Return () for silent success
fn cleanup(state: State<AppState>) -> CliResult<()>

// Return custom type implementing IntoResponse
fn report(state: State<AppState>) -> CliResult<Report>
```

### 7.4 Injection Rules

The framework automatically injects parameters based on type:

| Parameter Type | Source | Cardinality | Notes |
|----------------|--------|-------------|-------|
| `State<T>` | Application state | 0..1 | Cloning is cheap (Arc-based) |
| `{Command}Args` | CLI parser | 0..1 | Owned value from enum |
| `&Context` | Request context | 0..1 | Future: for tracing, telemetry |

**Constraints:**
- Each parameter type can appear at most once
- Parameter order is flexible (Axum-style)
- Handler signature is validated at compile time

```rust
// ✅ Valid signatures
fn handler1(state: State<App>, args: Args) -> CliResult<String>
fn handler2(args: Args, state: State<App>) -> CliResult<String>
fn handler3(state: State<App>) -> CliResult<String>
fn handler4(args: Args) -> CliResult<String>  // No state needed

// ❌ Invalid signatures (compile error)
fn handler5(state1: State<App>, state2: State<App>) -> CliResult<String>
fn handler6(args1: Args, args2: Args) -> CliResult<String>
```

### 7.5 Macro Implementation Overview

The `SenRouter` derive macro performs:

1. **Extract State Type**: Parse `#[sen(state = T)]` attribute
2. **Iterate Variants**: For each enum variant:
   - Extract handler path from `#[sen(handler = path)]`
   - Determine if variant has args (Unit vs Tuple)
3. **Generate Match Arms**: Create appropriate handler call
4. **Wrap in IntoResponse**: Convert handler result to Response

```rust
// Simplified macro logic (pseudo-code)
#[proc_macro_derive(SenRouter, attributes(sen))]
pub fn derive_sen_router(input: TokenStream) -> TokenStream {
    let enum_name = parse_enum_name(input);
    let state_type = extract_state_type(input);

    let match_arms = input.variants.map(|variant| {
        let variant_name = variant.name;
        let handler_path = extract_handler_path(variant);

        if variant.has_args() {
            quote! {
                #enum_name::#variant_name(args) => {
                    #handler_path(state, args).into_response()
                }
            }
        } else {
            quote! {
                #enum_name::#variant_name => {
                    #handler_path(state).into_response()
                }
            }
        }
    });

    quote! {
        impl #enum_name {
            pub fn execute(self, state: State<#state_type>) -> Response {
                match self {
                    #(#match_arms)*
                }
            }
        }
    }
}
```

## 8. Code Organization & File Structure

### 8.1 The 1000-Line Problem

**Problem**: `main.rs` tends to grow into a 1000+ line monolith containing:
- Command parsing logic
- 10-20 handler implementations
- Ad-hoc `println!` scattered everywhere
- Inconsistent error handling
- Tangled state management

**Solution**: Enforce strict file separation from day one.

### 8.2 Recommended File Structure

```
my-cli/
├── src/
│   ├── main.rs              # Entry point only (< 50 lines)
│   ├── handlers/            # Command handlers
│   │   ├── mod.rs           # Handler exports
│   │   ├── status.rs        # Status command
│   │   ├── build.rs         # Build command
│   │   ├── deploy.rs        # Deploy command
│   │   └── ...              # One file per command
│   ├── workflows/           # Complex multi-task operations
│   │   ├── mod.rs
│   │   ├── preflight.rs     # fmt → lint → test
│   │   └── ship.rs          # build → upload → notify
│   ├── tasks/               # Atomic operations
│   │   ├── mod.rs
│   │   ├── fmt.rs           # Format code
│   │   ├── lint.rs          # Run linter
│   │   └── test.rs          # Run tests
│   ├── state.rs             # AppState definition
│   └── lib.rs               # Re-exports for testability
```

### 8.3 File Responsibility Matrix

| File | Responsibilities | Size Limit | Anti-Patterns |
|------|------------------|------------|---------------|
| **main.rs** | Parse CLI args, initialize state, call `execute()` | < 50 lines | ❌ Business logic<br>❌ Direct println!<br>❌ Handler impl |
| **handlers/*.rs** | Single command logic, call workflows/tasks | < 200 lines | ❌ println! (use Response)<br>❌ Nested match hell<br>❌ Direct I/O |
| **workflows/*.rs** | Coordinate multiple tasks, aggregate results | < 150 lines | ❌ Low-level I/O<br>❌ Direct sys calls |
| **tasks/*.rs** | Atomic operations (fmt, lint, test) | < 100 lines | ❌ Task interdependencies |

### 8.4 main.rs: The Minimal Entry Point

**Good Example (< 50 lines):**

```rust
// src/main.rs
use my_cli::{AppState, Commands, State};

fn main() {
    // 1. Initialize tracing/logging
    tracing_subscriber::init();

    // 2. Load application state
    let app_state = match AppState::load() {
        Ok(state) => state,
        Err(e) => {
            eprintln!("{}", format_error(&e));
            std::process::exit(e.exit_code());
        }
    };

    // 3. Parse CLI arguments
    let cli = Commands::parse();

    // 4. Execute command (macro-generated)
    let response = cli.execute(State::new(app_state));

    // 5. Handle response
    match response.output {
        Output::Text(msg) => println!("{}", msg),
        Output::Json(json) => println!("{}", serde_json::to_string_pretty(&json).unwrap()),
        Output::Silent => {}
    }

    std::process::exit(response.exit_code);
}

fn format_error(e: &CliError) -> String {
    // Delegate to error formatter
    my_cli::error::format(e)
}
```

**Bad Example (Anti-Pattern):**

```rust
// ❌ DON'T DO THIS - main.rs becoming a dumping ground
fn main() {
    let cli = Commands::parse();

    match cli {
        Commands::Build(args) => {
            // ❌ 100+ lines of build logic in main.rs
            println!("Building...");
            if args.release {
                // ... lots of logic
            }
            // ... more logic
        }
        Commands::Test(args) => {
            // ❌ Another 100+ lines
            println!("Testing...");
            // ...
        }
        // ... 10 more commands inline
    }
}
```

### 8.5 Handlers: One File Per Command

**Principle**: Each command gets its own file, making it easy to:
- Find and modify specific command logic
- Write focused tests
- Review changes in isolation
- Avoid merge conflicts

**Example: handlers/deploy.rs**

```rust
// src/handlers/deploy.rs
use crate::{State, AppState, CliResult, workflows};

pub struct DeployArgs {
    pub env: String,
    pub confirm: bool,
}

pub fn deploy(state: State<AppState>, args: DeployArgs) -> CliResult<String> {
    // 1. Validation (business rules)
    if args.env == "production" && !args.confirm {
        return Err(CliError::User(
            "Production deploys require --confirm flag".into()
        ));
    }

    // 2. Delegate to workflow
    let report = workflows::deploy::execute(&state, &args.env)?;

    // 3. Format response (no direct println!)
    Ok(format!(
        "✓ Deployed to {} in {:.2}s\n{}",
        args.env,
        report.duration.as_secs_f64(),
        report.summary()
    ))
}
```

**Key Points:**
- No `println!` - return structured data
- No complex logic - delegate to workflows
- Simple validation and coordination only

### 8.6 Output Strategy: No Println in Handlers

**Problem**: Scattered `println!` statements make it impossible to:
- Test handlers (output goes to stdout)
- Support JSON output mode
- Capture output for logging/telemetry

**Solution**: Return structured data, let framework handle output.

```rust
// ❌ Bad: Direct output
pub fn status(state: State<AppState>) -> CliResult<()> {
    println!("Status: OK");  // Can't test, can't redirect
    println!("Version: 1.0");
    Ok(())
}

// ✅ Good: Return structured data
pub fn status(state: State<AppState>) -> CliResult<StatusReport> {
    Ok(StatusReport {
        status: "OK".into(),
        version: "1.0".into(),
    })
}

impl IntoResponse for StatusReport {
    fn into_response(self) -> Response {
        Response::text(format!("Status: {}\nVersion: {}", self.status, self.version))
    }
}
```

### 8.7 Testing Strategy

With proper file separation, testing becomes straightforward:

```rust
// tests/handlers/deploy_test.rs
#[test]
fn test_deploy_requires_confirm_for_production() {
    let state = test_helpers::mock_state();
    let args = DeployArgs {
        env: "production".into(),
        confirm: false,
    };

    let result = handlers::deploy(state, args);

    assert!(matches!(result, Err(CliError::User(_))));
}
```

**Benefits:**
- Handlers are pure functions (State + Args → Result)
- No mocking of I/O (delegated to tasks)
- Fast unit tests (no subprocess spawning)

### 8.8 Migration Path for Existing CLIs

For existing monolithic CLIs:

1. **Step 1**: Move command parsing to enum (keep logic inline)
2. **Step 2**: Extract handlers to separate functions (still in main.rs)
3. **Step 3**: Move handlers to handlers/ directory
4. **Step 4**: Add `#[derive(SenRouter)]` to eliminate match boilerplate
5. **Step 5**: Extract workflows and tasks as needed

**Incremental adoption** - no need to rewrite everything at once.

## 9. Implementation Roadmap

### Phase 1: Core Framework (MVP)
- [ ] Define core traits (`IntoResponse`, `ConfigProvider`)
- [ ] Implement basic router without macros (manual matching)
- [ ] Implement error types and exit code mapping
- [ ] Add State<T> wrapper and injection logic

### Phase 2: Macro System
- [ ] Implement `#[derive(SenRouter)]` proc macro
- [ ] Add handler binding validation at compile time
- [ ] Generate injection code based on handler signature
- [ ] Add helpful compile errors for misuse

### Phase 3: Advanced Features
- [ ] Add ReloadableConfig for long-running commands
- [ ] Implement structured logging with tracing
- [ ] Add telemetry hooks (optional)
- [ ] Support middleware (rate limiting, auth, etc.)

### Phase 4: Developer Experience
- [ ] CLI generator (`sen new my-cli`)
- [ ] Workflow templates
- [ ] Integration testing utilities
- [ ] Documentation site

## 10. Design Decisions & Trade-offs

### 10.1 Why Enum-based routing?

**Alternative**: String-based dispatch (like web routers)
```rust
// Rejected approach
router.add("/serve", handlers::serve);
```

**Rationale**:
- Enums provide exhaustiveness checking at compile time
- IDEs can autocomplete command variants
- Refactoring is safer (rename propagates automatically)
- No runtime routing overhead

**Trade-off**: Less dynamic, but that's by design (Anti-Fragile principle)

### 10.2 Why fixed workflows?

**Alternative**: Allow users to compose tasks dynamically
```rust
sen run --tasks "fmt,lint,test"
```

**Rationale**:
- Task ordering often has semantic meaning (fmt before lint)
- Dependencies between tasks are implicit and error-prone
- AI agents can't infer correct ordering
- Reproducibility requires fixed recipes

**Trade-off**: Less flexibility, but eliminates entire class of errors

### 10.3 Why separate User vs System errors?

**Alternative**: Single error type with exit code 1 for all errors

**Rationale**:
- User errors need actionable hints, system errors need stack traces
- Monitoring systems can alert on exit code 101 (unexpected failures)
- Users shouldn't file bugs for exit code 1 errors
- Clear signal: "did I do something wrong?" vs "is the tool broken?"

**Trade-off**: Developers must categorize errors correctly

### 10.4 Why strict file separation?

**Alternative**: Keep everything in main.rs or split only when it gets too big

**Rationale**:
- Prevents the "1000-line main.rs" problem before it happens
- Each command is independently testable and reviewable
- Eliminates println! debugging by forcing structured output
- Makes it impossible to accidentally tangle state management
- AI agents can more easily understand and modify specific commands

**Trade-off**: More files for simple CLIs, but the structure pays off at 5+ commands

## 11. Success Metrics

### For Developers
- Time to add new command: < 5 minutes
- Lines of boilerplate per handler: 0
- Compile-time error detection rate: > 95%
- main.rs size: < 50 lines (enforced by design)

### For End Users
- Command execution predictability: 100% (no side effects)
- Error message actionability: User can fix without reading docs
- AI agent success rate: > 90% on first attempt

### For System
- Binary size overhead: < 500KB vs handwritten equivalent
- Runtime overhead: < 1ms for routing and injection
- Memory footprint: O(1) regardless of command count
