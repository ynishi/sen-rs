//! Tracing and logging support.
//!
//! This module provides structured logging capabilities using the `tracing` crate.
//! It offers easy initialization with sensible defaults and customization options.

#[cfg(feature = "tracing")]
pub use tracing::{self, debug, error, info, instrument, trace, warn};

#[cfg(feature = "tracing")]
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

/// Tracing output format.
#[cfg(feature = "tracing")]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracingFormat {
    /// Human-readable format with colors (default for development).
    Pretty,

    /// Compact format without colors.
    Compact,

    /// JSON format (recommended for production).
    Json,
}

/// Tracing configuration.
#[cfg(feature = "tracing")]
#[derive(Debug, Clone)]
pub struct TracingConfig {
    /// Log level filter.
    ///
    /// If None, uses RUST_LOG environment variable or defaults to "info".
    pub level: Option<tracing::Level>,

    /// Output format.
    pub format: TracingFormat,

    /// Include timestamps in output.
    pub timestamps: bool,

    /// Include target module names in output.
    pub target: bool,

    /// Include thread IDs in output.
    pub thread_ids: bool,
}

#[cfg(feature = "tracing")]
impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            level: None,
            format: TracingFormat::Pretty,
            timestamps: true,
            target: true,
            thread_ids: false,
        }
    }
}

/// Initialize tracing subscriber with default settings.
///
/// Uses RUST_LOG environment variable for level filtering.
/// Defaults to "info" level if RUST_LOG is not set.
///
/// # Example
///
/// ```ignore
/// use sen::tracing_support::init_subscriber;
///
/// fn main() {
///     init_subscriber();
///
///     // Your application code
/// }
/// ```
///
/// # Environment Variables
///
/// - `RUST_LOG=debug` - Enable debug logs
/// - `RUST_LOG=trace` - Enable trace logs
/// - `RUST_LOG=myapp=debug,sen=trace` - Per-module filtering
#[cfg(feature = "tracing")]
pub fn init_subscriber() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer())
        .init();
}

/// Initialize tracing subscriber with custom configuration.
///
/// # Example
///
/// ```ignore
/// use sen::tracing_support::{init_subscriber_with_config, TracingConfig, TracingFormat};
///
/// fn main() {
///     let config = TracingConfig {
///         format: TracingFormat::Json,
///         timestamps: true,
///         ..Default::default()
///     };
///
///     init_subscriber_with_config(config);
/// }
/// ```
#[cfg(feature = "tracing")]
pub fn init_subscriber_with_config(config: TracingConfig) {
    let filter = if let Some(level) = config.level {
        EnvFilter::new(level.to_string())
    } else {
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"))
    };

    match (config.format, config.timestamps) {
        (TracingFormat::Pretty, true) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .pretty()
                        .with_target(config.target)
                        .with_thread_ids(config.thread_ids),
                )
                .init();
        }
        (TracingFormat::Pretty, false) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .pretty()
                        .without_time()
                        .with_target(config.target)
                        .with_thread_ids(config.thread_ids),
                )
                .init();
        }
        (TracingFormat::Compact, true) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .compact()
                        .with_target(config.target)
                        .with_thread_ids(config.thread_ids),
                )
                .init();
        }
        (TracingFormat::Compact, false) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .compact()
                        .without_time()
                        .with_target(config.target)
                        .with_thread_ids(config.thread_ids),
                )
                .init();
        }
        (TracingFormat::Json, true) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .json()
                        .with_target(config.target)
                        .with_thread_ids(config.thread_ids),
                )
                .init();
        }
        (TracingFormat::Json, false) => {
            tracing_subscriber::registry()
                .with(filter)
                .with(
                    tracing_subscriber::fmt::layer()
                        .json()
                        .without_time()
                        .with_target(config.target)
                        .with_thread_ids(config.thread_ids),
                )
                .init();
        }
    }
}

// Fallback when tracing feature is disabled
#[cfg(not(feature = "tracing"))]
pub fn init_subscriber() {
    // No-op when tracing is disabled
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(feature = "tracing")]
    fn test_default_config() {
        let config = TracingConfig::default();
        assert_eq!(config.format, TracingFormat::Pretty);
        assert!(config.timestamps);
        assert!(config.target);
        assert!(!config.thread_ids);
    }
}
