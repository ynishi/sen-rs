//! # SEN: Script to System CLI Engine
//!
//! A type-safe, macro-powered CLI framework inspired by Axum's ergonomics.
//!
//! ## Core Principles
//!
//! - **Compile-time safety**: Enum-based routing with exhaustiveness checking
//! - **Zero boilerplate**: Derive macros generate all wiring code
//! - **Type-driven DI**: Handler parameters are injected based on type signature
//! - **Fixed workflows**: Predictable behavior for humans and AI agents
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use sen::{CliResult, State, SenRouter};
//!
//! // Define application state
//! pub struct AppState {
//!     pub config: Config,
//! }
//!
//! // Define commands with derive macro
//! #[derive(SenRouter)]
//! #[sen(state = AppState)]
//! enum Commands {
//!     #[sen(handler = handlers::status)]
//!     Status,
//!
//!     #[sen(handler = handlers::build)]
//!     Build(BuildArgs),
//! }
//!
//! // Implement handlers as async functions
//! mod handlers {
//!     use super::*;
//!
//!     pub async fn status(state: State<AppState>) -> CliResult<String> {
//!         let app = state.read().await;
//!         Ok("Status: OK".to_string())
//!     }
//!
//!     pub async fn build(state: State<AppState>, args: BuildArgs) -> CliResult<()> {
//!         // Build logic here (can use async DB, API calls, etc.)
//!         Ok(())
//!     }
//! }
//!
//! #[tokio::main]
//! async fn main() {
//!     let state = State::new(AppState { config: Config::load() });
//!     let cmd = Commands::parse();
//!     let response = cmd.execute(state).await;
//!
//!     if !response.output.is_empty() {
//!         println!("{}", response.output);
//!     }
//!     std::process::exit(response.exit_code);
//! }
//! ```

use std::sync::Arc;

// Re-export macros
pub use sen_rs_macros::handler;
pub use sen_rs_macros::sen;
pub use sen_rs_macros::SenRouter;

// Optional modules
pub mod build_info;
pub mod tracing_support;

#[cfg(feature = "sensors")]
pub mod sensors;

#[cfg(feature = "mcp")]
pub mod mcp;

// Re-export tracing itself (required for #[instrument] macro)
#[cfg(feature = "tracing")]
pub use tracing_support::tracing;

// Re-export commonly used items
#[cfg(feature = "tracing")]
pub use tracing_support::{
    debug, error, info, init_subscriber, init_subscriber_with_config, instrument, trace, warn,
    TracingConfig, TracingFormat,
};

#[cfg(feature = "build-info")]
pub use build_info::{version_info, version_short};

#[cfg(feature = "sensors")]
pub use sensors::{GitSensor, SensorData, Sensors};

// Re-export clap for convenience when using clap integration
#[cfg(feature = "clap")]
pub use clap;

// ============================================================================
// Core Types
// ============================================================================

/// Shared application state wrapper with async-safe interior mutability.
///
/// Wraps your application state in `Arc<RwLock<T>>` for safe concurrent access.
/// Handlers receive this by value, but cloning is cheap (just incrementing a ref count).
///
/// # Example
///
/// ```ignore
/// use sen::State;
///
/// struct AppState {
///     config: String,
/// }
///
/// let state = State::new(AppState {
///     config: "production".to_string(),
/// });
///
/// // Access inner state (read-only)
/// let app = state.read().await;
/// assert_eq!(app.config, "production");
///
/// // Mutate inner state
/// let mut app = state.write().await;
/// app.config = "development".to_string();
/// ```
pub struct State<T>(Arc<tokio::sync::RwLock<T>>);

// Manual Clone implementation that doesn't require T: Clone
impl<T> Clone for State<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> State<T> {
    /// Create a new state wrapper.
    pub fn new(inner: T) -> Self {
        Self(Arc::new(tokio::sync::RwLock::new(inner)))
    }

    /// Get a read lock to the inner state.
    ///
    /// Multiple readers can hold read locks simultaneously.
    pub async fn read(&self) -> tokio::sync::RwLockReadGuard<'_, T> {
        self.0.read().await
    }

    /// Get a write lock to the inner state.
    ///
    /// Only one writer can hold a write lock at a time.
    pub async fn write(&self) -> tokio::sync::RwLockWriteGuard<'_, T> {
        self.0.write().await
    }
}

/// Global options wrapper for CLI-wide flags.
///
/// Similar to State, but immutable (read-only). Used for global flags like
/// `--verbose`, `--config`, etc. that apply to all commands.
///
/// # Usage Pattern
///
/// ```ignore
/// use sen::{GlobalOptions, FromGlobalArgs, State};
///
/// // 1. Define global options structure
/// #[derive(Clone)]
/// struct GlobalOpts {
///     verbose: bool,
///     config: Option<String>,
/// }
///
/// // 2. Implement FromGlobalArgs (or use clap::Parser derive)
/// impl FromGlobalArgs for GlobalOpts {
///     fn from_global_args(args: &[String]) -> Result<(Self, Vec<String>), CliError> {
///         // Parse global flags and return remaining args
///         // ...
///     }
/// }
///
/// // 3. In main(), parse global options first
/// #[tokio::main]
/// async fn main() {
///     let args: Vec<String> = std::env::args().skip(1).collect();
///
///     // Parse global options
///     let (global_opts, remaining_args) = GlobalOpts::from_global_args(&args).unwrap();
///
///     // Include in application state
///     let state = State::new(AppState {
///         global: global_opts,
///         // ... other state fields
///     });
///
///     let router = Router::new()
///         .route("command", handler)
///         .with_state(state);
///
///     let response = router.execute(&remaining_args).await;
///     std::process::exit(response.exit_code);
/// }
/// ```
///
/// # Alternative: Direct Usage
///
/// You can also wrap global options in `GlobalOptions` and pass them directly:
///
/// ```ignore
/// let global = GlobalOptions::new(GlobalOpts {
///     verbose: true,
///     config: Some("~/.myapp/config.toml".to_string()),
/// });
///
/// // Access inner options (cheap clone)
/// let opts = global.get();
/// assert_eq!(opts.verbose, true);
/// ```
pub struct GlobalOptions<T>(Arc<T>);

// Manual Clone implementation that doesn't require T: Clone
impl<T> Clone for GlobalOptions<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> GlobalOptions<T> {
    /// Create a new global options wrapper.
    pub fn new(inner: T) -> Self {
        Self(Arc::new(inner))
    }

    /// Get a reference to the inner options.
    ///
    /// This is cheap - just returns a reference to the Arc'd data.
    pub fn get(&self) -> &T {
        &self.0
    }
}

/// CLI result type.
///
/// All handler functions should return `CliResult<T>` where `T` implements `IntoResponse`.
pub type CliResult<T> = Result<T, CliError>;

// ============================================================================
// Error Types
// ============================================================================

/// Top-level error type for CLI operations.
///
/// Distinguishes between user-fixable errors (exit code 1) and system failures (exit code 101).
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    /// User-fixable errors (exit code 1).
    ///
    /// These should include actionable hints for users.
    #[error(transparent)]
    User(#[from] UserError),

    /// System-level failures (exit code 101).
    ///
    /// These indicate bugs or environmental issues that users can't fix.
    #[error(transparent)]
    System(#[from] SystemError),
}

impl CliError {
    /// Get the appropriate exit code for this error.
    pub fn exit_code(&self) -> i32 {
        match self {
            CliError::User(_) => 1,
            CliError::System(_) => 101,
        }
    }

    /// Convenience constructor for user errors.
    pub fn user(message: impl Into<String>) -> Self {
        CliError::User(UserError::Generic(message.into()))
    }

    /// Convenience constructor for system errors.
    pub fn system(message: impl Into<String>) -> Self {
        CliError::System(SystemError::Internal(message.into()))
    }
}

/// User-fixable errors (exit code 1).
#[derive(Debug, thiserror::Error)]
pub enum UserError {
    /// Generic user error with a message.
    #[error("Error: {0}")]
    Generic(String),

    /// Invalid argument provided.
    #[error("Error: Invalid argument '{arg}'\n\n{reason}")]
    InvalidArgument { arg: String, reason: String },

    /// Missing required dependency.
    #[error("Error: Missing dependency '{tool}'\n\nHint: {install_hint}")]
    MissingDependency { tool: String, install_hint: String },

    /// Validation failed.
    #[error("Error: Validation failed\n\n{}", .details.join("\n"))]
    ValidationFailed { details: Vec<String> },

    /// Prerequisite not met.
    #[error("Error: Prerequisite not met: {check}\n\nHint: {fix_hint}")]
    PrerequisiteNotMet { check: String, fix_hint: String },
}

/// System-level failures (exit code 101).
#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    /// Generic internal error.
    #[error("Internal Error: {0}\n\nThis is likely a bug.")]
    Internal(String),

    /// I/O error.
    #[error("Internal Error: I/O operation failed\n\n{0:?}\n\nThis is likely a bug.")]
    Io(#[from] std::io::Error),

    /// Configuration parsing error.
    #[error("Internal Error: Config parse failed\n\n{0}\n\nThis is likely a bug.")]
    ConfigParse(String),
}

// ============================================================================
// Response Types
// ============================================================================

/// Response returned by handlers after execution.
///
/// Contains exit code and output to be displayed to the user.
pub struct Response {
    /// Exit code (0 = success, 1 = user error, 101 = system error).
    pub exit_code: i32,

    /// Output to display (text, JSON, or silent).
    pub output: Output,

    /// Whether this response should be output in agent mode (JSON).
    pub agent_mode: bool,

    /// Optional metadata for agent mode (tier, tags, sensors).
    #[cfg(feature = "sensors")]
    pub metadata: Option<ResponseMetadata>,
}

/// Metadata attached to Response for AI agents.
#[cfg(feature = "sensors")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct ResponseMetadata {
    /// Safety tier of the executed command
    pub tier: Option<&'static str>,

    /// Tags of the executed command
    pub tags: Option<Vec<&'static str>>,

    /// Environment sensor data
    pub sensors: Option<crate::sensors::SensorData>,
}

impl Response {
    /// Create a successful response with text output.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            exit_code: 0,
            output: Output::Text(content.into()),
            agent_mode: false,
            #[cfg(feature = "sensors")]
            metadata: None,
        }
    }

    /// Create a successful silent response.
    pub fn silent() -> Self {
        Self {
            exit_code: 0,
            output: Output::Silent,
            agent_mode: false,
            #[cfg(feature = "sensors")]
            metadata: None,
        }
    }

    /// Create an error response.
    pub fn error(exit_code: i32, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            output: Output::Text(message.into()),
            agent_mode: false,
            #[cfg(feature = "sensors")]
            metadata: None,
        }
    }

    /// Attach metadata to this response (for agent mode).
    #[cfg(feature = "sensors")]
    pub fn with_metadata(mut self, metadata: ResponseMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Convert response to agent-friendly JSON format.
    ///
    /// Returns a JSON object with:
    /// - `result`: "success" or "error"
    /// - `exit_code`: numeric exit code
    /// - `output`: command output
    /// - `tier`: safety tier (if available)
    /// - `tags`: command tags (if available)
    /// - `sensors`: environment data (if available)
    #[cfg(feature = "sensors")]
    pub fn to_agent_json(&self) -> String {
        let result = if self.exit_code == 0 {
            "success"
        } else {
            "error"
        };

        let output = match &self.output {
            Output::Silent => String::new(),
            Output::Text(s) => s.clone(),
            Output::Json(s) => s.clone(),
        };

        let mut json = serde_json::json!({
            "result": result,
            "exit_code": self.exit_code,
            "output": output,
        });

        if let Some(ref metadata) = self.metadata {
            if let Some(tier) = metadata.tier {
                json["tier"] = serde_json::json!(tier);
            }
            if let Some(ref tags) = metadata.tags {
                json["tags"] = serde_json::json!(tags);
            }
            if let Some(ref sensors) = metadata.sensors {
                json["sensors"] = serde_json::to_value(sensors).unwrap_or(serde_json::json!(null));
            }
        }

        serde_json::to_string_pretty(&json).unwrap_or_else(|_| "{}".to_string())
    }
}

/// Output type for responses.
#[derive(Debug)]
pub enum Output {
    /// No output.
    Silent,

    /// Text output (printed to stdout).
    Text(String),

    /// JSON output (for machine-readable responses).
    Json(String),
}

impl Output {
    /// Check if output is empty.
    pub fn is_empty(&self) -> bool {
        matches!(self, Output::Silent)
    }
}

impl std::fmt::Display for Output {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Output::Silent => Ok(()),
            Output::Text(s) | Output::Json(s) => write!(f, "{}", s),
        }
    }
}

// ============================================================================
// Response Conversion Trait
// ============================================================================

/// Trait for converting handler return values into responses.
///
/// Implemented for common return types like `String`, `()`, and `Result<T, E>`.
pub trait IntoResponse {
    /// Convert into a response.
    fn into_response(self) -> Response;
}

impl IntoResponse for String {
    fn into_response(self) -> Response {
        Response::text(self)
    }
}

impl IntoResponse for () {
    fn into_response(self) -> Response {
        Response::silent()
    }
}

impl<T: IntoResponse> IntoResponse for CliResult<T> {
    fn into_response(self) -> Response {
        match self {
            Ok(value) => value.into_response(),
            Err(e) => {
                let exit_code = e.exit_code();
                let message = match e {
                    CliError::User(user_err) => format!("{}", user_err),
                    CliError::System(sys_err) => format!("{}", sys_err),
                };
                Response::error(exit_code, message)
            }
        }
    }
}

// ============================================================================
// Router & Handler System (Axum-style)
// ============================================================================

use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;

/// Boxed future for type erasure
type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Safety tier for CLI commands.
///
/// Determines the risk level of a command and whether it requires
/// human approval when executed by AI agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(clap::ValueEnum))]
pub enum Tier {
    /// Safe operations (read-only, information gathering).
    /// Examples: status, list, version, help
    /// Agent permission: Always allow
    Safe,

    /// Standard operations (idempotent or reversible).
    /// Examples: fmt, build, test, lint
    /// Agent permission: Auto-approve
    Standard,

    /// Critical operations (destructive, deployment, authentication).
    /// Examples: deploy, publish, delete, drop-database
    /// Agent permission: Require human confirmation
    Critical,
}

impl Tier {
    /// Parse tier from string (case-insensitive).
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "safe" => Some(Tier::Safe),
            "standard" => Some(Tier::Standard),
            "critical" => Some(Tier::Critical),
            _ => None,
        }
    }

    /// Convert tier to static string.
    pub fn as_str(&self) -> &'static str {
        match self {
            Tier::Safe => "safe",
            Tier::Standard => "standard",
            Tier::Critical => "critical",
        }
    }

    /// Check if this tier requires human approval for AI agents.
    pub fn requires_approval(&self) -> bool {
        matches!(self, Tier::Critical)
    }
}

impl std::fmt::Display for Tier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[cfg(feature = "clap")]
impl std::str::FromStr for Tier {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| {
            format!(
                "Invalid tier: '{}'. Valid options: safe, standard, critical",
                s
            )
        })
    }
}

/// Metadata for CLI application and commands.
///
/// This is used by the `#[sen(...)]` attribute macro to provide
/// help generation and CLI documentation.
#[derive(Debug, Clone)]
pub struct RouterMetadata {
    /// Application name
    pub name: &'static str,
    /// Version string (optional)
    pub version: Option<&'static str>,
    /// Short description
    pub about: Option<&'static str>,
}

/// Metadata for individual command handlers.
///
/// This is used by the `#[sen::handler(...)]` attribute macro.
#[derive(Debug, Clone)]
pub struct HandlerMetadata {
    /// Short description of what this handler does
    pub desc: Option<&'static str>,
    /// Safety tier for this command
    pub tier: Option<Tier>,
    /// Tags for command categorization and discovery
    pub tags: Option<Vec<&'static str>>,
}

/// Metadata for a specific route in the router.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RouteMetadata {
    /// Handler-level metadata (from #[sen::handler])
    handler_meta: Option<HandlerMetadata>,
    /// Route description (can be set via .describe())
    description: Option<String>,
    /// CLI argument schema (from Clap, if available)
    args_schema: Option<serde_json::Value>,
}

impl RouteMetadata {
    /// Get the description for this route
    ///
    /// Prefers route-level description over handler-level description
    pub fn get_description(&self) -> Option<&str> {
        self.description
            .as_deref()
            .or_else(|| self.handler_meta.as_ref()?.desc)
    }

    /// Get the CLI argument schema for this route
    pub fn get_args_schema(&self) -> Option<&serde_json::Value> {
        self.args_schema.as_ref()
    }
}

/// Handler trait - allows functions with various signatures to be used as handlers.
///
/// This trait is automatically implemented for async functions with compatible signatures.
/// Inspired by Axum's handler system.
pub trait Handler<T, S>: Clone + Send + Sync + Sized + 'static {
    /// Future type returned by the handler
    type Future: Future<Output = Response> + Send + 'static;

    /// Call the handler with state and arguments
    fn call(self, state: State<S>, args: Vec<String>) -> Self::Future;

    /// Get handler metadata (optional)
    fn metadata(&self) -> Option<HandlerMetadata> {
        None
    }

    /// Get CLI argument schema (optional)
    fn args_schema(&self) -> Option<serde_json::Value> {
        None
    }
}

/// Wrapper that attaches metadata to a handler.
///
/// This is typically created by the `#[sen::handler]` macro.
pub struct HandlerWithMeta<H, T, S> {
    pub handler: H,
    pub metadata: HandlerMetadata,
    _marker: PhantomData<fn() -> (T, S)>,
}

impl<H, T, S> Clone for HandlerWithMeta<H, T, S>
where
    H: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            metadata: self.metadata.clone(),
            _marker: PhantomData,
        }
    }
}

impl<H, T, S> HandlerWithMeta<H, T, S>
where
    H: Handler<T, S>,
{
    pub fn new(handler: H, metadata: HandlerMetadata) -> Self {
        Self {
            handler,
            metadata,
            _marker: PhantomData,
        }
    }
}

impl<H, T, S> Handler<T, S> for HandlerWithMeta<H, T, S>
where
    H: Handler<T, S>,
    T: 'static,
    S: Send + Sync + Clone + 'static,
{
    type Future = H::Future;

    fn call(self, state: State<S>, args: Vec<String>) -> Self::Future {
        self.handler.call(state, args)
    }

    fn metadata(&self) -> Option<HandlerMetadata> {
        Some(self.metadata.clone())
    }

    fn args_schema(&self) -> Option<serde_json::Value> {
        self.handler.args_schema()
    }
}

/// Type-erased handler for storage in Router
trait ErasedHandler<S>: Send + Sync {
    fn call_boxed<'a>(&'a self, state: State<S>, args: Vec<String>) -> BoxFuture<'a, Response>;

    fn clone_box(&self) -> Box<dyn ErasedHandler<S>>;

    #[allow(dead_code)]
    fn metadata(&self) -> Option<HandlerMetadata>;

    #[allow(dead_code)]
    fn args_schema(&self) -> Option<serde_json::Value>;
}

impl<S> Clone for Box<dyn ErasedHandler<S>> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Wrapper that implements ErasedHandler for any Handler
struct HandlerService<H, T, S> {
    handler: H,
    _marker: PhantomData<fn() -> (T, S)>,
}

impl<H, T, S> HandlerService<H, T, S> {
    fn new(handler: H) -> Self {
        Self {
            handler,
            _marker: PhantomData,
        }
    }
}

impl<H, T, S> Clone for HandlerService<H, T, S>
where
    H: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            _marker: PhantomData,
        }
    }
}

impl<H, T, S> ErasedHandler<S> for HandlerService<H, T, S>
where
    H: Handler<T, S>,
    S: Send + Sync + 'static,
    T: 'static,
{
    fn call_boxed<'a>(&'a self, state: State<S>, args: Vec<String>) -> BoxFuture<'a, Response> {
        let handler = self.handler.clone();
        Box::pin(async move { handler.call(state, args).await })
    }

    fn clone_box(&self) -> Box<dyn ErasedHandler<S>> {
        Box::new(self.clone())
    }

    fn metadata(&self) -> Option<HandlerMetadata> {
        self.handler.metadata()
    }

    fn args_schema(&self) -> Option<serde_json::Value> {
        self.handler.args_schema()
    }
}

/// Router for CLI commands.
///
/// Similar to Axum's Router, this allows dynamic registration of command handlers.
/// The generic parameter `S` represents the "missing state type" - handlers need
/// `State<S>` to execute.
///
/// # Example
///
/// ```ignore
/// use sen::{Router, State, CliResult};
///
/// async fn status(state: State<AppState>) -> CliResult<String> {
///     Ok("Status: OK".to_string())
/// }
///
/// let router = Router::new()
///     .route("status", status)
///     .with_state(app_state);
///
/// let response = router.execute(&["status"]).await;
/// ```
pub struct Router<S = ()> {
    routes: HashMap<String, Box<dyn ErasedHandler<S>>>,
    route_metadata: HashMap<String, RouteMetadata>,
    metadata: Option<RouterMetadata>,
    agent_mode_enabled: bool,
    #[cfg(feature = "mcp")]
    mcp_enabled: bool,
    _marker: PhantomData<S>,
}

impl<S> Default for Router<S>
where
    S: Send + Sync + Clone + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<S> Router<S>
where
    S: Send + Sync + Clone + 'static,
{
    /// Create a new empty router.
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            route_metadata: HashMap::new(),
            metadata: None,
            agent_mode_enabled: false,
            #[cfg(feature = "mcp")]
            mcp_enabled: false,
            _marker: PhantomData,
        }
    }

    /// Register a handler for a command.
    ///
    /// # Example
    ///
    /// ```ignore
    /// router.route("build", handlers::build)
    /// ```
    pub fn route<H, T: 'static>(mut self, command: impl Into<String>, handler: H) -> Self
    where
        H: Handler<T, S>,
    {
        let command_name = command.into();
        if self.routes.contains_key(&command_name) {
            panic!("Duplicate route: {}", command_name);
        }

        // Collect handler metadata and schema information
        let handler_meta = handler.metadata();
        let args_schema = handler.args_schema();

        // Store metadata
        self.route_metadata.insert(
            command_name.clone(),
            RouteMetadata {
                handler_meta,
                description: None,
                args_schema,
            },
        );

        self.routes
            .insert(command_name, Box::new(HandlerService::new(handler)));
        self
    }

    /// Nest a router under a prefix.
    ///
    /// This allows organizing commands into hierarchies (subcommands).
    ///
    /// # Example
    ///
    /// ```ignore
    /// let db_router = Router::new()
    ///     .route("create", handlers::db::create)
    ///     .route("list", handlers::db::list)
    ///     .route("delete", handlers::db::delete);
    ///
    /// let app = Router::new()
    ///     .nest("db", db_router)
    ///     .nest("server", server_router)
    ///     .with_state(state);
    ///
    /// // Routes: "db:create", "db:list", "db:delete", "server:start", ...
    /// ```
    pub fn nest(mut self, prefix: impl Into<String>, router: Router<S>) -> Self {
        let prefix = prefix.into();

        // Add all routes from the nested router with the prefix
        for (path, handler) in router.routes {
            let nested_path = if path.is_empty() {
                prefix.clone()
            } else {
                format!("{}:{}", prefix, path)
            };

            if self.routes.contains_key(&nested_path) {
                panic!("Duplicate route: {}", nested_path);
            }

            self.routes.insert(nested_path.clone(), handler);

            // Transfer route metadata if exists
            if let Some(meta) = router.route_metadata.get(&path) {
                self.route_metadata.insert(nested_path, meta.clone());
            }
        }

        self
    }

    /// Attach metadata to the router.
    ///
    /// This is typically used by the `#[sen(...)]` attribute macro to provide
    /// CLI metadata for help generation.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let router = Router::new()
    ///     .route("status", handlers::status)
    ///     .with_metadata(RouterMetadata {
    ///         name: "myctl",
    ///         version: Some("1.0.0"),
    ///         about: Some("My CLI tool"),
    ///     })
    ///     .with_state(state);
    /// ```
    pub fn with_metadata(mut self, metadata: RouterMetadata) -> Self {
        self.metadata = Some(metadata);
        self
    }

    /// Enable automatic agent mode support.
    ///
    /// When enabled, the router will:
    /// - Automatically detect `--agent-mode` flag in arguments
    /// - Strip the flag before routing to handlers
    /// - Set `agent_mode` in the Response for automatic JSON output
    ///
    /// # Example
    ///
    /// ```ignore
    /// let router = Router::new()
    ///     .route("build", handlers::build)
    ///     .with_agent_mode()
    ///     .with_state(state);
    ///
    /// // User runs: myapp --agent-mode build
    /// // Router automatically handles the flag and outputs JSON
    /// ```
    pub fn with_agent_mode(mut self) -> Self {
        self.agent_mode_enabled = true;
        self
    }

    /// Enable MCP (Model Context Protocol) support.
    ///
    /// When enabled, the router will recognize and handle MCP-specific flags:
    /// - `--mcp-server`: Start in MCP server mode (JSON-RPC over stdio)
    /// - `--mcp-init <client>`: Generate MCP configuration for the specified client
    ///
    /// # Example
    ///
    /// ```ignore
    /// let router = Router::new()
    ///     .route("build", handlers::build)
    ///     .with_mcp()
    ///     .with_state(state);
    ///
    /// // Usage:
    /// // $ mycli --mcp-server              # Start MCP server
    /// // $ mycli --mcp-init claude         # Generate claude_desktop_config.json
    /// ```
    #[cfg(feature = "mcp")]
    pub fn with_mcp(mut self) -> Self {
        self.mcp_enabled = true;
        self
    }

    /// Provide the application state, converting `Router<S>` to `Router<()>`.
    ///
    /// This follows Axum's pattern where the type system ensures all required
    /// state is provided before the router can execute requests.
    pub fn with_state(self, state: S) -> Router<()> {
        let routes: HashMap<String, Box<dyn ErasedHandler<()>>> = self
            .routes
            .into_iter()
            .map(|(cmd, handler)| {
                let state = state.clone();
                let boxed: Box<dyn ErasedHandler<()>> =
                    Box::new(StatefulHandler { handler, state });
                (cmd, boxed)
            })
            .collect();

        Router {
            routes,
            route_metadata: self.route_metadata,
            metadata: self.metadata,
            agent_mode_enabled: self.agent_mode_enabled,
            #[cfg(feature = "mcp")]
            mcp_enabled: self.mcp_enabled,
            _marker: PhantomData,
        }
    }
}

/// Handler that has been bound to a state
struct StatefulHandler<S> {
    handler: Box<dyn ErasedHandler<S>>,
    state: S,
}

impl<S> Clone for StatefulHandler<S>
where
    S: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handler: self.handler.clone(),
            state: self.state.clone(),
        }
    }
}

impl<S> ErasedHandler<()> for StatefulHandler<S>
where
    S: Clone + Send + Sync + 'static,
{
    fn call_boxed<'a>(&'a self, _state: State<()>, args: Vec<String>) -> BoxFuture<'a, Response> {
        let handler = self.handler.clone();
        let state = State::new(self.state.clone());
        Box::pin(async move { handler.call_boxed(state, args).await })
    }

    fn clone_box(&self) -> Box<dyn ErasedHandler<()>> {
        Box::new(self.clone())
    }

    fn metadata(&self) -> Option<HandlerMetadata> {
        self.handler.metadata()
    }

    fn args_schema(&self) -> Option<serde_json::Value> {
        self.handler.args_schema()
    }
}

impl Router<()> {
    /// Execute a command using environment arguments.
    ///
    /// This is the most common usage - automatically reads from `std::env::args()`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// #[tokio::main]
    /// async fn main() {
    ///     let router = Router::new()
    ///         .route("build", build_handler)
    ///         .route("test", test_handler);
    ///
    ///     let response = router.execute().await;
    ///     std::process::exit(response.exit_code);
    /// }
    /// ```
    pub async fn execute(&self) -> Response {
        let args: Vec<String> = std::env::args().collect();
        self.execute_with(&args).await
    }

    /// Execute a command with custom arguments.
    ///
    /// Useful for testing or when you need to provide arguments programmatically.
    /// The first element (`args[0]`) should be the program name (like `std::env::args()`).
    ///
    /// Supports both flat and nested command structures:
    /// - `["myapp", "build"]` → matches route "build"
    /// - `["myapp", "db", "create"]` → matches route "db:create"
    /// - `["myapp", "db", "backup", "create"]` → matches route "db:backup:create"
    ///
    /// Special handling:
    /// - `--help` or `-h` → displays help message
    /// - `version` → displays version (if metadata.version is set)
    ///
    /// # Example
    ///
    /// ```ignore
    /// // Testing
    /// let response = router.execute_with(&["myapp", "build", "--release"]).await;
    /// assert_eq!(response.exit_code, 0);
    /// ```
    ///
    /// Returns a Response with exit code and output.
    pub async fn execute_with(&self, args: &[String]) -> Response {
        // Skip program name (args[0])
        let command_args = if args.is_empty() { &[] } else { &args[1..] };

        // Detect and strip --agent-mode flag if agent_mode is enabled
        let (agent_mode_active, command_args) = if self.agent_mode_enabled {
            let agent_mode = command_args.contains(&"--agent-mode".to_string());
            let filtered: Vec<String> = command_args
                .iter()
                .filter(|arg| *arg != "--agent-mode")
                .cloned()
                .collect();
            (agent_mode, filtered)
        } else {
            (false, command_args.to_vec())
        };

        let command_args_slice: &[String] = &command_args;

        // Handle MCP flags if MCP is enabled
        #[cfg(feature = "mcp")]
        if self.mcp_enabled {
            // Handle --mcp-server flag
            if command_args_slice.contains(&"--mcp-server".to_string()) {
                // Convert route_metadata to MCP tool schemas
                let tools: Vec<crate::mcp::McpTool> = self
                    .route_metadata
                    .iter()
                    .map(|(name, metadata)| {
                        crate::mcp::McpTool::from_route_metadata(name.clone(), metadata)
                    })
                    .collect();

                return crate::mcp::run_mcp_server(tools);
            }

            // Handle --mcp-init flag (will be implemented next)
            if command_args_slice.iter().any(|arg| arg == "--mcp-init") {
                // TODO: Implement --mcp-init
                return Response::error(1, "--mcp-init not yet implemented");
            }
        }

        // Handle --help flag
        if command_args_slice.contains(&"--help".to_string())
            || command_args_slice.contains(&"-h".to_string())
        {
            // Check if JSON format is requested
            let json_output = command_args_slice.contains(&"--json".to_string())
                || command_args_slice
                    .iter()
                    .any(|a| a.starts_with("--format=json"));
            let mut response = self.generate_help(command_args_slice, json_output);
            response.agent_mode = agent_mode_active;
            return response;
        }

        if command_args_slice.is_empty() {
            let mut response = self.generate_help(&[], false);
            response.agent_mode = agent_mode_active;
            return response;
        }

        // Handle built-in version command
        if command_args_slice.len() == 1
            && (command_args_slice[0] == "version"
                || command_args_slice[0] == "--version"
                || command_args_slice[0] == "-V")
        {
            let mut response = self.handle_version();
            response.agent_mode = agent_mode_active;
            return response;
        }

        // Try to match nested commands first (longest match wins)
        // e.g., ["db", "create", "--flag"] tries:
        //   1. "db:create" (found!)
        //   2. "db" (fallback)
        let (matched_handler, remaining_args) = self.find_route(command_args_slice);

        let mut response = match matched_handler {
            Some(handler) => {
                let state = State::new(());
                handler.call_boxed(state, remaining_args).await
            }
            None => {
                let command = command_args_slice.join(" ");
                let err: CliResult<()> =
                    Err(CliError::user(format!("Unknown command: {}", command)));
                err.into_response()
            }
        };

        // Set agent_mode flag if it was detected
        response.agent_mode = agent_mode_active;
        response
    }

    /// Generate help message based on router metadata and available commands.
    fn generate_help(&self, _args: &[String], json_output: bool) -> Response {
        if json_output {
            self.generate_cli_schema_json()
        } else {
            self.generate_help_text()
        }
    }

    /// Generate text-based help message.
    fn generate_help_text(&self) -> Response {
        let mut help = String::new();

        if let Some(meta) = &self.metadata {
            help.push_str(meta.name);
            if let Some(version) = meta.version {
                help.push_str(&format!(" {}", version));
            }
            help.push('\n');

            if let Some(about) = meta.about {
                help.push_str(&format!("{}\n", about));
            }
            help.push('\n');
        }

        help.push_str("Usage: ");
        if let Some(meta) = &self.metadata {
            help.push_str(meta.name);
        } else {
            help.push_str("<command>");
        }
        help.push_str(" [OPTIONS] <COMMAND>\n\n");

        help.push_str("Commands:\n");
        let mut commands: Vec<_> = self.routes.keys().collect();
        commands.sort();

        for cmd in commands {
            help.push_str(&format!("  {}\n", cmd));
        }

        help.push_str("\nOptions:\n");
        help.push_str("  -h, --help            Show help\n");
        help.push_str(
            "  -h, --help --json     Show CLI schema (all commands with arguments/options)\n",
        );
        if self.metadata.as_ref().and_then(|m| m.version).is_some() {
            help.push_str("  -V, --version         Show version\n");
        }

        Response::text(help)
    }

    /// Generate CLI schema JSON specification.
    ///
    /// Outputs a CLI-friendly JSON format that includes all commands with their
    /// arguments, options, and metadata in a single dump.
    fn generate_cli_schema_json(&self) -> Response {
        use serde_json::json;

        let name = self.metadata.as_ref().map(|m| m.name).unwrap_or("cli");
        let version = self
            .metadata
            .as_ref()
            .and_then(|m| m.version)
            .unwrap_or("unknown");
        let description = self.metadata.as_ref().and_then(|m| m.about);

        let mut commands = serde_json::Map::new();

        // Collect all routes and their metadata
        let mut command_names: Vec<_> = self.routes.keys().collect();
        command_names.sort();

        for cmd in command_names {
            // Get handler metadata
            let handler_meta = self
                .route_metadata
                .get(cmd)
                .and_then(|meta| meta.handler_meta.as_ref());

            // Get handler description
            let desc = handler_meta
                .and_then(|h| h.desc)
                .unwrap_or("No description available");

            // Get tier information
            let tier = handler_meta.and_then(|h| h.tier).map(|t| t.as_str());

            // Get tags
            let tags = handler_meta.and_then(|h| h.tags.as_ref());

            // Build usage string
            let usage = format!("{} {}", name, cmd.replace(':', " "));

            let mut command_schema = json!({
                "description": desc,
                "usage": usage,
            });

            // Add tier if available
            if let Some(tier_str) = tier {
                command_schema["tier"] = json!(tier_str);
                command_schema["requires_approval"] = json!(Tier::parse(tier_str)
                    .map(|t| t.requires_approval())
                    .unwrap_or(false));
            }

            // Add tags if available
            if let Some(tag_list) = tags {
                command_schema["tags"] = json!(tag_list);
            }

            // Add argument schema if available
            if let Some(meta) = self.route_metadata.get(cmd) {
                if let Some(args_schema) = &meta.args_schema {
                    command_schema["arguments"] = args_schema["arguments"].clone();
                    command_schema["options"] = args_schema["options"].clone();
                }
            }

            commands.insert(cmd.to_string(), command_schema);
        }

        let spec = json!({
            "name": name,
            "version": version,
            "description": description.unwrap_or(""),
            "commands": commands,
        });

        match serde_json::to_string_pretty(&spec) {
            Ok(json) => Response::text(json),
            Err(e) => Response::error(1, format!("Failed to generate JSON: {}", e)),
        }
    }

    /// Handle version command.
    fn handle_version(&self) -> Response {
        if let Some(meta) = &self.metadata {
            if let Some(version) = meta.version {
                return Response::text(format!("{} {}", meta.name, version));
            }
        }

        #[cfg(feature = "build-info")]
        {
            Response::text(crate::version_info())
        }

        #[cfg(not(feature = "build-info"))]
        Response::text("version information not available")
    }

    /// Find the longest matching route for the given arguments.
    ///
    /// Returns the matched handler and remaining arguments.
    fn find_route(&self, args: &[String]) -> (Option<&dyn ErasedHandler<()>>, Vec<String>) {
        // Try matching from longest to shortest
        for depth in (1..=args.len()).rev() {
            let route_parts = &args[..depth];
            let route_key = route_parts.join(":");

            if let Some(handler) = self.routes.get(&route_key) {
                let remaining = args[depth..].to_vec();
                return (Some(handler.as_ref()), remaining);
            }
        }

        (None, args.to_vec())
    }
}

// ============================================================================
// Args Extractor (Axum-style)
// ============================================================================

/// Extractor for command-line arguments.
///
/// Similar to Axum's `Path` or `Query`, this allows handlers to receive
/// parsed arguments.
///
/// # Example
///
/// ```ignore
/// #[derive(Debug)]
/// struct BuildArgs {
///     release: bool,
/// }
///
/// impl FromArgs for BuildArgs {
///     fn from_args(args: &[String]) -> Result<Self, CliError> {
///         Ok(BuildArgs {
///             release: args.get(0).map(|s| s == "--release").unwrap_or(false),
///         })
///     }
/// }
///
/// async fn build(State(app): State<AppState>, Args(args): Args<BuildArgs>) -> CliResult<String> {
///     if args.release {
///         Ok("Release build".to_string())
///     } else {
///         Ok("Debug build".to_string())
///     }
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Args<T>(pub T);

/// Trait for parsing command-line arguments into a type.
///
/// This is similar to Axum's `FromRequest` trait and provides a lightweight
/// way to parse per-command arguments.
///
/// # When to use `FromArgs`
///
/// Use `FromArgs` when:
/// - ✅ You have simple per-command arguments (e.g., `--release`, `--output file.txt`)
/// - ✅ You don't need global flags that apply to all commands
/// - ✅ You want the framework to handle argument injection automatically
///
/// **Don't use `FromArgs` when:**
/// - ❌ You need global flags (e.g., `--verbose`, `--config`) → use `FromGlobalArgs`
/// - ❌ You need complex validation or conflicting flag logic → use `clap` directly
/// - ❌ You're building a production CLI with many commands → see `examples/practical-cli`
///
/// # Example
///
/// ```rust
/// use sen::{Args, FromArgs, CliError, CliResult};
///
/// #[derive(Debug)]
/// struct BuildArgs {
///     release: bool,
///     output: Option<String>,
/// }
///
/// impl FromArgs for BuildArgs {
///     fn from_args(args: &[String]) -> Result<Self, CliError> {
///         let mut release = false;
///         let mut output = None;
///
///         let mut iter = args.iter();
///         while let Some(arg) = iter.next() {
///             match arg.as_str() {
///                 "--release" => release = true,
///                 "--output" => output = iter.next().map(|s| s.clone()),
///                 _ => {}
///             }
///         }
///
///         Ok(BuildArgs { release, output })
///     }
/// }
///
/// // Use in handler
/// async fn build(Args(args): Args<BuildArgs>) -> CliResult<String> {
///     let mode = if args.release { "release" } else { "debug" };
///     Ok(format!("Building in {} mode", mode))
/// }
/// ```
///
/// # Comparison with `FromGlobalArgs`
///
/// | Feature | `FromArgs` | `FromGlobalArgs` |
/// |---------|-----------|------------------|
/// | Scope | Per-command | All commands |
/// | Injection | Via `Args<T>` extractor | Via `State<AppState>` |
/// | Use case | Simple flags | Global configuration |
/// | Example | `--release`, `--output` | `--verbose`, `--config` |
///
/// See README.md § "Argument Parsing: FromArgs vs Global Options" for detailed guide.
pub trait FromArgs: Sized {
    /// Parse arguments into Self, or return an error.
    fn from_args(args: &[String]) -> Result<Self, CliError>;

    /// Get CLI schema information for this argument type (optional).
    ///
    /// Returns JSON representation of the command structure, including
    /// arguments, options, and descriptions.
    fn cli_schema() -> Option<serde_json::Value> {
        None
    }
}

/// Trait for parsing global options from command-line arguments.
///
/// Global options are flags that apply to **all commands** in your CLI,
/// such as `--verbose`, `--config`, or `--output-format`.
///
/// # When to use `FromGlobalArgs`
///
/// Use `FromGlobalArgs` when:
/// - ✅ You have flags that apply to **all** commands (e.g., `--verbose`, `--config`)
/// - ✅ You want to avoid repeating the same flags in every handler
/// - ✅ You're building a production CLI with multiple commands
/// - ✅ You need integration with `clap` or other complex parsers
///
/// # Comparison with `FromArgs`
///
/// | Feature | `FromArgs` | `FromGlobalArgs` |
/// |---------|-----------|------------------|
/// | Scope | Per-command | All commands |
/// | Injection | Via `Args<T>` extractor | Via `State<AppState>` |
/// | Parsing time | During handler call | Before routing |
/// | Use case | Command-specific flags | CLI-wide configuration |
///
/// # Example: Production CLI Pattern
///
/// ```ignore
/// use sen::FromGlobalArgs;
///
/// #[derive(Clone)]
/// struct GlobalOpts {
///     verbose: bool,
///     config: Option<String>,
/// }
///
/// impl FromGlobalArgs for GlobalOpts {
///     fn from_global_args(args: &[String]) -> Result<(Self, Vec<String>), CliError> {
///         let mut verbose = false;
///         let mut config = None;
///         let mut remaining = Vec::new();
///
///         for arg in args {
///             match arg.as_str() {
///                 "--verbose" | "-v" => verbose = true,
///                 s if s.starts_with("--config=") => {
///                     config = Some(s.strip_prefix("--config=").unwrap().to_string());
///                 }
///                 _ => remaining.push(arg.clone()),
///             }
///         }
///
///         Ok((GlobalOpts { verbose, config }, remaining))
///     }
/// }
///
/// #[derive(Clone)]
/// struct AppState {
///     global: GlobalOpts,
///     // ... other state fields
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let args: Vec<String> = std::env::args().skip(1).collect();
///     let (global_opts, remaining) = GlobalOpts::from_global_args(&args).unwrap();
///
///     let state = AppState { global: global_opts };
///     let router = Router::new()
///         .route("build", handlers::build)
///         .with_state(state);
///
///     // Execute with remaining args (global flags already parsed)
///     let program_name = std::env::args().next().unwrap_or_default();
///     let mut execute_args = vec![program_name];
///     execute_args.extend(remaining);
///     let response = router.execute_with(&execute_args).await;
/// }
/// ```
///
/// # Real-World Pattern
///
/// This pattern mirrors production CLIs like `kubectl`, `docker`, `aws`:
///
/// ```bash
/// kubectl --context=prod get pods          # global: --context, command: get
/// docker --debug run nginx                 # global: --debug, command: run
/// myctl --verbose --config=prod db create  # global: --verbose --config, command: db create
/// ```
///
/// See `examples/practical-cli` for a complete implementation with nested commands.
///
/// See README.md § "Argument Parsing: FromArgs vs Global Options" for detailed guide.
pub trait FromGlobalArgs: Sized + Clone {
    /// Parse global options from command-line arguments.
    ///
    /// This is called before routing, so it receives all arguments.
    /// It should extract global flags and return the remaining non-global args.
    ///
    /// Returns `(parsed_options, remaining_args)` where `remaining_args` should be
    /// passed to the router for command routing.
    fn from_global_args(args: &[String]) -> Result<(Self, Vec<String>), CliError>;
}

// ============================================================================
// Clap Integration (when clap feature is enabled)
// ============================================================================

#[cfg(feature = "clap")]
/// Blanket implementation: any type implementing `clap::Parser` can be used with `Args<T>`.
///
/// This allows seamless integration with clap's derive macros:
///
/// ```ignore
/// use clap::Parser;
/// use sen::{Args, CliResult, State};
///
/// #[derive(Parser)]
/// struct BuildArgs {
///     /// Database name
///     name: String,
///
///     /// Build in release mode
///     #[arg(long)]
///     release: bool,
///
///     /// Target architecture (can also be set via env var)
///     #[arg(long, env = "BUILD_TARGET")]
///     target: Option<String>,
/// }
///
/// async fn build(
///     state: State<AppState>,
///     Args(args): Args<BuildArgs>  // Clap automatically parses!
/// ) -> CliResult<String> {
///     if args.release {
///         Ok("Building in release mode".to_string())
///     } else {
///         Ok("Building in debug mode".to_string())
///     }
/// }
/// ```
impl<T> FromArgs for T
where
    T: clap::Parser,
{
    fn from_args(args: &[String]) -> Result<Self, CliError> {
        // Clap expects the command name as the first argument
        // Since we're parsing subcommand args, we need to prepend a dummy command name
        let args_with_cmd = std::iter::once("cmd".to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>();

        T::try_parse_from(args_with_cmd).map_err(|e| CliError::user(e.to_string()))
    }

    fn cli_schema() -> Option<serde_json::Value> {
        let cmd = T::command();
        Some(clap_command_to_json(&cmd))
    }
}

#[cfg(feature = "clap")]
/// Convert a clap::Command to a JSON representation.
fn clap_command_to_json(cmd: &clap::Command) -> serde_json::Value {
    use serde_json::json;

    // Extract positional arguments
    let mut positionals = Vec::new();
    let mut options = Vec::new();

    for arg in cmd.get_arguments() {
        if arg.is_positional() {
            positionals.push(json!({
                "name": arg.get_id().as_str(),
                "type": format!("{:?}", arg.get_value_parser().type_id()),
                "required": arg.is_required_set(),
                "description": arg.get_help().map(|h| h.to_string()).unwrap_or_default(),
            }));
        } else {
            let mut option = json!({
                "name": format!("--{}", arg.get_id().as_str()),
                "type": format!("{:?}", arg.get_value_parser().type_id()),
                "required": arg.is_required_set(),
                "description": arg.get_help().map(|h| h.to_string()).unwrap_or_default(),
            });

            // Add short flag if available
            if let Some(short) = arg.get_short() {
                option["short"] = json!(format!("-{}", short));
            }

            // Add default value if available
            let defaults = arg.get_default_values();
            if !defaults.is_empty() {
                option["default"] = json!(defaults[0].to_string_lossy().to_string());
            }

            // Add env var if available
            if let Some(env) = arg.get_env() {
                option["env"] = json!(env.to_string_lossy().to_string());
            }

            options.push(option);
        }
    }

    json!({
        "arguments": positionals,
        "options": options,
    })
}

#[cfg(feature = "clap")]
/// Blanket implementation for global options using clap::Parser.
///
/// This allows using clap's derive macros for global flags:
///
/// ```ignore
/// use clap::Parser;
///
/// #[derive(Parser, Clone)]
/// struct GlobalOpts {
///     /// Enable verbose logging
///     #[arg(long, short, global = true)]
///     verbose: bool,
///
///     /// Configuration file path
///     #[arg(long, global = true)]
///     config: Option<String>,
/// }
/// ```
impl<T> FromGlobalArgs for T
where
    T: clap::Parser + Clone,
{
    fn from_global_args(args: &[String]) -> Result<(Self, Vec<String>), CliError> {
        // Try to parse global options using clap
        // We need to use clap's API to extract global flags and return remaining args

        // For now, parse all args and let clap handle it
        // In a real implementation, we'd need to separate global from command-specific args
        let args_with_cmd = std::iter::once("cmd".to_string())
            .chain(args.iter().cloned())
            .collect::<Vec<_>>();

        match T::try_parse_from(&args_with_cmd) {
            Ok(global) => {
                // For simplicity, return empty remaining args
                // In practice, clap would need to be configured to allow unknown args
                Ok((global, vec![]))
            }
            Err(e) => Err(CliError::user(e.to_string())),
        }
    }
}

// ============================================================================
// Manual FromArgs implementations (when clap feature is NOT enabled)
// ============================================================================

#[cfg(not(feature = "clap"))]
// Implement FromArgs for () (no args needed)
impl FromArgs for () {
    fn from_args(_args: &[String]) -> Result<Self, CliError> {
        Ok(())
    }
}

#[cfg(not(feature = "clap"))]
// Implement FromArgs for Vec<String> (raw args)
impl FromArgs for Vec<String> {
    fn from_args(args: &[String]) -> Result<Self, CliError> {
        Ok(args.to_vec())
    }
}

// ============================================================================
// Handler Implementations for Common Function Signatures
// ============================================================================

// Handler for: async fn(State<S>) -> impl IntoResponse
impl<F, Fut, S, Res> Handler<(State<S>,), S> for F
where
    F: Fn(State<S>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Res> + Send + 'static,
    Res: IntoResponse + 'static,
    S: Send + Sync + Clone + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, state: State<S>, _args: Vec<String>) -> Self::Future {
        Box::pin(async move {
            let result = self(state).await;
            result.into_response()
        })
    }
}

// Handler for: async fn(State<S>, Args<T>) -> impl IntoResponse
impl<F, Fut, S, T, Res> Handler<(State<S>, Args<T>), S> for F
where
    F: Fn(State<S>, Args<T>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Res> + Send + 'static,
    Res: IntoResponse + 'static,
    T: FromArgs + Send + 'static,
    S: Send + Sync + Clone + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, state: State<S>, args: Vec<String>) -> Self::Future {
        Box::pin(async move {
            // Parse args
            let parsed_args = match T::from_args(&args) {
                Ok(args) => args,
                Err(e) => {
                    let result: CliResult<()> = Err(e);
                    return result.into_response();
                }
            };

            let result = self(state, Args(parsed_args)).await;
            result.into_response()
        })
    }

    fn args_schema(&self) -> Option<serde_json::Value> {
        T::cli_schema()
    }
}

// Handler for: async fn(Args<T>) -> impl IntoResponse (no state)
impl<F, Fut, T, Res> Handler<(Args<T>,), ()> for F
where
    F: Fn(Args<T>) -> Fut + Clone + Send + Sync + 'static,
    Fut: Future<Output = Res> + Send + 'static,
    Res: IntoResponse + 'static,
    T: FromArgs + Send + 'static,
{
    type Future = Pin<Box<dyn Future<Output = Response> + Send>>;

    fn call(self, _state: State<()>, args: Vec<String>) -> Self::Future {
        Box::pin(async move {
            // Parse args
            let parsed_args = match T::from_args(&args) {
                Ok(args) => args,
                Err(e) => {
                    let result: CliResult<()> = Err(e);
                    return result.into_response();
                }
            };

            let result = self(Args(parsed_args)).await;
            result.into_response()
        })
    }

    fn args_schema(&self) -> Option<serde_json::Value> {
        T::cli_schema()
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================
    // Tier Tests
    // ========================================

    #[test]
    fn test_tier_parse() {
        assert_eq!(Tier::parse("safe"), Some(Tier::Safe));
        assert_eq!(Tier::parse("SAFE"), Some(Tier::Safe));
        assert_eq!(Tier::parse("standard"), Some(Tier::Standard));
        assert_eq!(Tier::parse("critical"), Some(Tier::Critical));
        assert_eq!(Tier::parse("invalid"), None);
    }

    #[test]
    fn test_tier_as_str() {
        assert_eq!(Tier::Safe.as_str(), "safe");
        assert_eq!(Tier::Standard.as_str(), "standard");
        assert_eq!(Tier::Critical.as_str(), "critical");
    }

    #[test]
    fn test_tier_requires_approval() {
        assert!(!Tier::Safe.requires_approval());
        assert!(!Tier::Standard.requires_approval());
        assert!(Tier::Critical.requires_approval());
    }

    #[test]
    fn test_tier_display() {
        assert_eq!(format!("{}", Tier::Safe), "safe");
        assert_eq!(format!("{}", Tier::Standard), "standard");
        assert_eq!(format!("{}", Tier::Critical), "critical");
    }

    // ========================================
    // State Tests
    // ========================================

    #[tokio::test]
    async fn test_state_creation_and_access() {
        struct TestState {
            value: i32,
        }

        let state = State::new(TestState { value: 42 });
        assert_eq!(state.read().await.value, 42);

        let cloned = state.clone();
        assert_eq!(cloned.read().await.value, 42);
    }

    #[tokio::test]
    async fn test_state_write() {
        struct TestState {
            value: i32,
        }

        let state = State::new(TestState { value: 42 });

        // Mutate through write lock
        {
            let mut app = state.write().await;
            app.value = 100;
        }

        // Verify mutation
        assert_eq!(state.read().await.value, 100);
    }

    #[test]
    fn test_user_error_exit_code() {
        let err = CliError::user("test error");
        assert_eq!(err.exit_code(), 1);
    }

    #[test]
    fn test_system_error_exit_code() {
        let err = CliError::system("test error");
        assert_eq!(err.exit_code(), 101);
    }

    #[test]
    fn test_string_into_response() {
        let response = "hello".to_string().into_response();
        assert_eq!(response.exit_code, 0);
        assert!(matches!(response.output, Output::Text(_)));
    }

    #[test]
    fn test_unit_into_response() {
        let response = ().into_response();
        assert_eq!(response.exit_code, 0);
        assert!(matches!(response.output, Output::Silent));
    }

    #[test]
    fn test_result_ok_into_response() {
        let result: CliResult<String> = Ok("success".to_string());
        let response = result.into_response();
        assert_eq!(response.exit_code, 0);
    }

    #[test]
    fn test_result_err_into_response() {
        let result: CliResult<String> = Err(CliError::user("failure"));
        let response = result.into_response();
        assert_eq!(response.exit_code, 1);
    }

    // ========================================
    // Router Tests
    // ========================================

    #[tokio::test]
    async fn test_router_basic() {
        #[derive(Clone)]
        struct AppState {
            value: i32,
        }

        async fn get_value(state: State<AppState>) -> CliResult<String> {
            let app = state.read().await;
            Ok(format!("Value: {}", app.value))
        }

        let state = AppState { value: 42 };
        let router = Router::new().route("status", get_value).with_state(state);

        let response = router
            .execute_with(&["test".to_string(), "status".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);
        assert!(matches!(response.output, Output::Text(_)));
    }

    #[tokio::test]
    async fn test_router_unknown_command() {
        let router: Router<()> = Router::new().with_state(());
        let response = router
            .execute_with(&["test".to_string(), "unknown".to_string()])
            .await;
        assert_eq!(response.exit_code, 1);
    }

    #[tokio::test]
    async fn test_router_no_command() {
        let router: Router<()> = Router::new().with_state(());
        let response = router.execute_with(&["test".to_string()]).await;
        // Empty args now show help with exit code 0 (changed from error)
        assert_eq!(response.exit_code, 0);
    }

    #[tokio::test]
    async fn test_router_multiple_routes() {
        #[derive(Clone)]
        struct AppState {
            #[allow(dead_code)]
            count: i32,
        }

        async fn status(_state: State<AppState>) -> CliResult<String> {
            Ok("OK".to_string())
        }

        async fn version(_state: State<AppState>) -> CliResult<String> {
            Ok("v1.0.0".to_string())
        }

        let state = AppState { count: 0 };
        let router = Router::new()
            .route("status", status)
            .route("version", version)
            .with_state(state);

        let response1 = router
            .execute_with(&["test".to_string(), "status".to_string()])
            .await;
        assert_eq!(response1.exit_code, 0);

        let response2 = router
            .execute_with(&["test".to_string(), "version".to_string()])
            .await;
        assert_eq!(response2.exit_code, 0);
    }

    // ========================================
    // Args Extractor Tests
    // ========================================

    #[tokio::test]
    async fn test_router_with_args() {
        #[derive(Clone)]
        struct AppState {
            #[allow(dead_code)]
            base_cmd: String,
        }

        #[derive(Debug)]
        struct BuildArgs {
            release: bool,
        }

        impl FromArgs for BuildArgs {
            fn from_args(args: &[String]) -> Result<Self, CliError> {
                Ok(BuildArgs {
                    release: args.first().map(|s| s == "--release").unwrap_or(false),
                })
            }
        }

        async fn build(_state: State<AppState>, Args(args): Args<BuildArgs>) -> CliResult<String> {
            if args.release {
                Ok("release".to_string())
            } else {
                Ok("debug".to_string())
            }
        }

        let state = AppState {
            base_cmd: "cargo build".to_string(),
        };
        let router = Router::new().route("build", build).with_state(state);

        // Test with --release flag
        let response1 = router
            .execute_with(&[
                "test".to_string(),
                "build".to_string(),
                "--release".to_string(),
            ])
            .await;
        assert_eq!(response1.exit_code, 0);
        if let Output::Text(output) = response1.output {
            assert_eq!(output, "release");
        }

        // Test without flag
        let response2 = router
            .execute_with(&["test".to_string(), "build".to_string()])
            .await;
        assert_eq!(response2.exit_code, 0);
        if let Output::Text(output) = response2.output {
            assert_eq!(output, "debug");
        }
    }

    #[tokio::test]
    async fn test_router_args_no_state() {
        #[derive(Debug)]
        struct EchoArgs {
            message: String,
        }

        impl FromArgs for EchoArgs {
            fn from_args(args: &[String]) -> Result<Self, CliError> {
                let message = args.first().cloned().unwrap_or_else(|| "".to_string());
                Ok(EchoArgs { message })
            }
        }

        async fn echo(Args(args): Args<EchoArgs>) -> CliResult<String> {
            Ok(args.message)
        }

        let router = Router::new().route("echo", echo).with_state(());

        let response = router
            .execute_with(&["test".to_string(), "echo".to_string(), "Hello!".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);
        if let Output::Text(output) = response.output {
            assert_eq!(output, "Hello!");
        }
    }

    #[tokio::test]
    async fn test_router_args_parse_error() {
        #[derive(Debug)]
        struct StrictArgs;

        impl FromArgs for StrictArgs {
            fn from_args(args: &[String]) -> Result<Self, CliError> {
                if args.is_empty() {
                    Err(CliError::user("Arguments required"))
                } else {
                    Ok(StrictArgs)
                }
            }
        }

        async fn strict(Args(_args): Args<StrictArgs>) -> CliResult<String> {
            Ok("success".to_string())
        }

        let router = Router::new().route("strict", strict).with_state(());

        // Should fail with no args
        let response = router
            .execute_with(&["test".to_string(), "strict".to_string()])
            .await;
        assert_eq!(response.exit_code, 1);
    }

    // ========================================
    // Router::nest() Tests
    // ========================================

    #[tokio::test]
    async fn test_router_nest_basic() {
        #[derive(Clone)]
        struct AppState;

        async fn db_create(_state: State<AppState>) -> CliResult<String> {
            Ok("DB created".to_string())
        }

        async fn db_list(_state: State<AppState>) -> CliResult<String> {
            Ok("DB list".to_string())
        }

        let db_router = Router::new()
            .route("create", db_create)
            .route("list", db_list);

        let router = Router::new().nest("db", db_router).with_state(AppState);

        // Test nested command: "db create"
        let response = router
            .execute_with(&["test".to_string(), "db".to_string(), "create".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);
        if let Output::Text(output) = response.output {
            assert_eq!(output, "DB created");
        }

        // Test nested command: "db list"
        let response = router
            .execute_with(&["test".to_string(), "db".to_string(), "list".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);
        if let Output::Text(output) = response.output {
            assert_eq!(output, "DB list");
        }
    }

    #[tokio::test]
    async fn test_router_nest_with_args() {
        #[derive(Clone)]
        struct AppState;

        #[derive(Debug)]
        struct CreateArgs {
            name: String,
        }

        impl FromArgs for CreateArgs {
            fn from_args(args: &[String]) -> Result<Self, CliError> {
                let name = args
                    .first()
                    .cloned()
                    .unwrap_or_else(|| "default".to_string());
                Ok(CreateArgs { name })
            }
        }

        async fn db_create(
            _state: State<AppState>,
            Args(args): Args<CreateArgs>,
        ) -> CliResult<String> {
            Ok(format!("Created DB: {}", args.name))
        }

        let db_router = Router::new().route("create", db_create);

        let router = Router::new().nest("db", db_router).with_state(AppState);

        // Test: "db create mydb"
        let response = router
            .execute_with(&[
                "test".to_string(),
                "db".to_string(),
                "create".to_string(),
                "mydb".to_string(),
            ])
            .await;

        assert_eq!(response.exit_code, 0);
        if let Output::Text(output) = response.output {
            assert_eq!(output, "Created DB: mydb");
        }
    }

    #[tokio::test]
    async fn test_router_nest_multiple_levels() {
        #[derive(Clone)]
        struct AppState;

        async fn status(_state: State<AppState>) -> CliResult<String> {
            Ok("OK".to_string())
        }

        async fn db_create(_state: State<AppState>) -> CliResult<String> {
            Ok("DB created".to_string())
        }

        async fn server_start(_state: State<AppState>) -> CliResult<String> {
            Ok("Server started".to_string())
        }

        let db_router = Router::new().route("create", db_create);
        let server_router = Router::new().route("start", server_start);

        let router = Router::new()
            .route("status", status) // Top-level command
            .nest("db", db_router) // Nested commands
            .nest("server", server_router)
            .with_state(AppState);

        // Top-level command
        let response = router
            .execute_with(&["test".to_string(), "status".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);

        // Nested command: db create
        let response = router
            .execute_with(&["test".to_string(), "db".to_string(), "create".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);

        // Nested command: server start
        let response = router
            .execute_with(&[
                "test".to_string(),
                "server".to_string(),
                "start".to_string(),
            ])
            .await;
        assert_eq!(response.exit_code, 0);
    }

    #[tokio::test]
    async fn test_router_nest_unknown_subcommand() {
        #[derive(Clone)]
        struct AppState;

        async fn db_create(_state: State<AppState>) -> CliResult<String> {
            Ok("DB created".to_string())
        }

        let db_router = Router::new().route("create", db_create);
        let router = Router::new().nest("db", db_router).with_state(AppState);

        // Unknown subcommand
        let response = router
            .execute_with(&["test".to_string(), "db".to_string(), "delete".to_string()])
            .await;
        assert_eq!(response.exit_code, 1);
    }

    // ========================================
    // Clap Integration Tests
    // ========================================
    // TODO: These tests need Handler trait implementation for module-scoped async functions
    // Currently blocked - async functions defined in test scope don't satisfy Handler trait bounds
    // Workaround: Move handlers to top-level module or use #[sen::handler] macro

    #[cfg(feature = "unstable-clap-tests")]
    #[tokio::test]
    async fn test_clap_integration_basic() {
        mod test_scope {
            use super::*;
            use clap::Parser;

            #[derive(Clone)]
            pub struct AppState;

            #[derive(Parser, Debug)]
            pub struct BuildArgs {
                /// Database name
                pub name: String,

                /// Build in release mode
                #[arg(long)]
                pub release: bool,
            }

            pub async fn build(
                _state: State<AppState>,
                Args(args): Args<BuildArgs>,
            ) -> CliResult<String> {
                if args.release {
                    Ok(format!("Release build: {}", args.name))
                } else {
                    Ok(format!("Debug build: {}", args.name))
                }
            }
        }

        let router = Router::new()
            .route("build", test_scope::build)
            .with_state(test_scope::AppState);

        // Test with --release
        let response = router
            .execute(&[
                "build".to_string(),
                "myapp".to_string(),
                "--release".to_string(),
            ])
            .await;
        assert_eq!(response.exit_code, 0);
        if let Output::Text(output) = response.output {
            assert_eq!(output, "Release build: myapp");
        }

        // Test without --release
        let response = router
            .execute(&["build".to_string(), "myapp".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);
        if let Output::Text(output) = response.output {
            assert_eq!(output, "Debug build: myapp");
        }
    }

    #[cfg(feature = "unstable-clap-tests")]
    #[tokio::test]
    async fn test_clap_integration_with_env() {
        mod test_scope {
            use super::*;
            use clap::Parser;

            #[derive(Clone)]
            pub struct AppState;

            #[derive(Parser, Debug)]
            pub struct DeployArgs {
                /// App name
                pub app: String,

                /// Target environment (from env var or flag)
                #[arg(long, env = "DEPLOY_ENV", default_value = "production")]
                pub env: String,
            }

            pub async fn deploy(
                _state: State<AppState>,
                Args(args): Args<DeployArgs>,
            ) -> CliResult<String> {
                Ok(format!("Deploying {} to {}", args.app, args.env))
            }
        }

        let router = Router::new()
            .route("deploy", test_scope::deploy)
            .with_state(test_scope::AppState);

        // Test with explicit --env flag
        let response = router
            .execute(&[
                "deploy".to_string(),
                "myapp".to_string(),
                "--env".to_string(),
                "staging".to_string(),
            ])
            .await;
        assert_eq!(response.exit_code, 0);
        if let Output::Text(output) = response.output {
            assert_eq!(output, "Deploying myapp to staging");
        }

        // Test with default value
        let response = router
            .execute(&["deploy".to_string(), "myapp".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);
        if let Output::Text(output) = response.output {
            assert_eq!(output, "Deploying myapp to production");
        }
    }

    #[cfg(feature = "unstable-clap-tests")]
    #[tokio::test]
    async fn test_clap_integration_validation() {
        mod test_scope {
            use super::*;
            use clap::Parser;

            #[derive(Clone)]
            pub struct AppState;

            #[derive(Parser, Debug)]
            pub struct CreateArgs {
                /// Name (required)
                pub name: String,

                /// Port number
                #[arg(long, value_parser = clap::value_parser!(u16).range(1..=65535))]
                pub port: Option<u16>,
            }

            pub async fn create(
                _state: State<AppState>,
                Args(args): Args<CreateArgs>,
            ) -> CliResult<String> {
                Ok(format!("Created {} on port {:?}", args.name, args.port))
            }
        }

        let router = Router::new()
            .route("create", test_scope::create)
            .with_state(test_scope::AppState);

        // Valid port
        let response = router
            .execute(&[
                "create".to_string(),
                "mydb".to_string(),
                "--port".to_string(),
                "3000".to_string(),
            ])
            .await;
        assert_eq!(response.exit_code, 0);

        // Invalid port (out of range) - clap will return error
        let response = router
            .execute(&[
                "create".to_string(),
                "mydb".to_string(),
                "--port".to_string(),
                "99999".to_string(),
            ])
            .await;
        assert_eq!(response.exit_code, 1);
    }

    // ========================================
    // Agent Mode Tests
    // ========================================

    #[tokio::test]
    async fn test_router_with_agent_mode_enabled() {
        async fn status(_state: State<()>) -> CliResult<String> {
            Ok("Status: OK".to_string())
        }

        let router = Router::new()
            .route("status", status)
            .with_agent_mode()
            .with_state(());

        // Without --agent-mode flag
        let response = router
            .execute_with(&["test".to_string(), "status".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);
        assert!(!response.agent_mode);

        // With --agent-mode flag
        let response = router
            .execute_with(&[
                "test".to_string(),
                "--agent-mode".to_string(),
                "status".to_string(),
            ])
            .await;
        assert_eq!(response.exit_code, 0);
        assert!(response.agent_mode);
    }

    #[tokio::test]
    async fn test_router_without_agent_mode_enabled() {
        async fn status(_state: State<()>) -> CliResult<String> {
            Ok("Status: OK".to_string())
        }

        let router = Router::new().route("status", status).with_state(());

        // With --agent-mode flag but agent_mode not enabled
        // The flag is treated as a command, resulting in "Unknown command" error
        let response = router
            .execute_with(&[
                "test".to_string(),
                "--agent-mode".to_string(),
                "status".to_string(),
            ])
            .await;
        assert_eq!(response.exit_code, 1); // Error: unknown command
        assert!(!response.agent_mode);

        // Without --agent-mode flag works fine
        let response = router
            .execute_with(&["test".to_string(), "status".to_string()])
            .await;
        assert_eq!(response.exit_code, 0);
        assert!(!response.agent_mode);
    }

    #[tokio::test]
    async fn test_agent_mode_flag_stripped_from_args() {
        #[derive(Debug)]
        struct TestArgs {
            message: String,
        }

        impl FromArgs for TestArgs {
            fn from_args(args: &[String]) -> Result<Self, CliError> {
                // --agent-mode should NOT appear in args
                for arg in args {
                    if arg == "--agent-mode" {
                        return Err(CliError::user(
                            "Unexpected --agent-mode flag in handler args",
                        ));
                    }
                }

                let message = args.first().cloned().unwrap_or_else(|| "empty".to_string());
                Ok(TestArgs { message })
            }
        }

        async fn echo(Args(args): Args<TestArgs>) -> CliResult<String> {
            Ok(args.message)
        }

        let router = Router::new()
            .route("echo", echo)
            .with_agent_mode()
            .with_state(());

        // --agent-mode should be stripped before passing to handler
        let response = router
            .execute_with(&[
                "test".to_string(),
                "--agent-mode".to_string(),
                "echo".to_string(),
                "hello".to_string(),
            ])
            .await;

        assert_eq!(response.exit_code, 0);
        assert!(response.agent_mode);
        // Should receive "hello", not "--agent-mode" or error
        if let Output::Text(output) = response.output {
            assert_eq!(output, "hello");
        } else {
            panic!("Expected text output");
        }
    }
}
