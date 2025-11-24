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
//! // Implement handlers as plain functions
//! mod handlers {
//!     use super::*;
//!
//!     pub fn status(state: State<AppState>) -> CliResult<String> {
//!         Ok("Status: OK".to_string())
//!     }
//!
//!     pub fn build(state: State<AppState>, args: BuildArgs) -> CliResult<()> {
//!         // Build logic here
//!         Ok(())
//!     }
//! }
//!
//! fn main() {
//!     let state = State::new(AppState { config: Config::load() });
//!     let cmd = Commands::parse();
//!     let response = cmd.execute(state);
//!
//!     if !response.output.is_empty() {
//!         println!("{}", response.output);
//!     }
//!     std::process::exit(response.exit_code);
//! }
//! ```

use std::sync::Arc;

// Re-export the derive macro
pub use sen_rs_macros::SenRouter;

// Optional modules
pub mod build_info;
pub mod tracing_support;

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

// ============================================================================
// Core Types
// ============================================================================

/// Shared application state wrapper.
///
/// Wraps your application state in an `Arc` for cheap cloning across handlers.
/// Handlers receive this by value, but cloning is cheap (just incrementing a ref count).
///
/// # Example
///
/// ```
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
/// // Access inner state
/// assert_eq!(state.get().config, "production");
/// ```
#[derive(Clone)]
pub struct State<T>(Arc<T>);

impl<T> State<T> {
    /// Create a new state wrapper.
    pub fn new(inner: T) -> Self {
        Self(Arc::new(inner))
    }

    /// Get a reference to the inner state.
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
#[derive(Debug)]
pub enum CliError {
    /// User-fixable errors (exit code 1).
    ///
    /// These should include actionable hints for users.
    User(UserError),

    /// System-level failures (exit code 101).
    ///
    /// These indicate bugs or environmental issues that users can't fix.
    System(SystemError),
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
#[derive(Debug)]
pub enum UserError {
    /// Generic user error with a message.
    Generic(String),

    /// Invalid argument provided.
    InvalidArgument { arg: String, reason: String },

    /// Missing required dependency.
    MissingDependency { tool: String, install_hint: String },

    /// Validation failed.
    ValidationFailed { details: Vec<String> },

    /// Prerequisite not met.
    PrerequisiteNotMet { check: String, fix_hint: String },
}

impl std::fmt::Display for UserError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UserError::Generic(msg) => write!(f, "Error: {}", msg),
            UserError::InvalidArgument { arg, reason } => {
                write!(f, "Error: Invalid argument '{}'\n\n{}", arg, reason)
            }
            UserError::MissingDependency { tool, install_hint } => {
                write!(
                    f,
                    "Error: Missing dependency '{}'\n\nHint: {}",
                    tool, install_hint
                )
            }
            UserError::ValidationFailed { details } => {
                write!(f, "Error: Validation failed\n\n{}", details.join("\n"))
            }
            UserError::PrerequisiteNotMet { check, fix_hint } => {
                write!(
                    f,
                    "Error: Prerequisite not met: {}\n\nHint: {}",
                    check, fix_hint
                )
            }
        }
    }
}

/// System-level failures (exit code 101).
#[derive(Debug)]
pub enum SystemError {
    /// Generic internal error.
    Internal(String),

    /// I/O error.
    Io(std::io::Error),

    /// Configuration parsing error.
    ConfigParse(String),
}

impl std::fmt::Display for SystemError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SystemError::Internal(msg) => {
                write!(f, "Internal Error: {}\n\nThis is likely a bug.", msg)
            }
            SystemError::Io(e) => {
                write!(
                    f,
                    "Internal Error: I/O operation failed\n\n{:?}\n\nThis is likely a bug.",
                    e
                )
            }
            SystemError::ConfigParse(e) => {
                write!(
                    f,
                    "Internal Error: Config parse failed\n\n{}\n\nThis is likely a bug.",
                    e
                )
            }
        }
    }
}

impl From<std::io::Error> for CliError {
    fn from(e: std::io::Error) -> Self {
        CliError::System(SystemError::Io(e))
    }
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
}

impl Response {
    /// Create a successful response with text output.
    pub fn text(content: impl Into<String>) -> Self {
        Self {
            exit_code: 0,
            output: Output::Text(content.into()),
        }
    }

    /// Create a successful silent response.
    pub fn silent() -> Self {
        Self {
            exit_code: 0,
            output: Output::Silent,
        }
    }

    /// Create an error response.
    pub fn error(exit_code: i32, message: impl Into<String>) -> Self {
        Self {
            exit_code,
            output: Output::Text(message.into()),
        }
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
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_state_creation_and_access() {
        #[derive(Clone)]
        struct TestState {
            value: i32,
        }

        let state = State::new(TestState { value: 42 });
        assert_eq!(state.get().value, 42);

        let cloned = state.clone();
        assert_eq!(cloned.get().value, 42);
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
}
