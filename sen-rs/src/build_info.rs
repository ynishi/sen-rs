//! Build information module.
//!
//! This module provides access to compile-time build information such as:
//! - Package name and version
//! - Git commit hash
//! - Build timestamp
//! - Rust compiler version
//! - Target architecture

#[cfg(feature = "build-info")]
pub mod built_info {
    include!(concat!(env!("OUT_DIR"), "/built.rs"));
}

/// Get formatted version information.
///
/// Returns a multi-line string containing:
/// - Package name and version
/// - Target architecture
/// - Build timestamp
/// - Git commit hash (if available)
/// - Rust compiler version
///
/// # Example
///
/// ```ignore
/// println!("{}", sen::build_info::version_info());
/// ```
///
/// Output:
/// ```text
/// sen 0.1.0 (x86_64-apple-darwin)
/// Built: 2024-11-24 12:34:56 UTC
/// Commit: a1b2c3d
/// Rustc: 1.75.0
/// ```
#[cfg(feature = "build-info")]
pub fn version_info() -> String {
    format!(
        "{} {} ({})\nBuilt: {}\nCommit: {}\nRustc: {}",
        built_info::PKG_NAME,
        built_info::PKG_VERSION,
        built_info::TARGET,
        built_info::BUILT_TIME_UTC,
        built_info::GIT_COMMIT_HASH.unwrap_or("unknown"),
        built_info::RUSTC_VERSION
    )
}

/// Get short version string (package version only).
///
/// # Example
///
/// ```ignore
/// println!("Version: {}", sen::build_info::version_short());
/// ```
#[cfg(feature = "build-info")]
pub fn version_short() -> &'static str {
    built_info::PKG_VERSION
}

/// Get package name.
#[cfg(feature = "build-info")]
pub fn package_name() -> &'static str {
    built_info::PKG_NAME
}

/// Get git commit hash (if available).
#[cfg(feature = "build-info")]
pub fn git_commit() -> Option<&'static str> {
    built_info::GIT_COMMIT_HASH
}

/// Get build timestamp.
#[cfg(feature = "build-info")]
pub fn build_time() -> &'static str {
    built_info::BUILT_TIME_UTC
}

/// Get target triple.
#[cfg(feature = "build-info")]
pub fn target() -> &'static str {
    built_info::TARGET
}

// Fallback implementations when build-info feature is disabled
#[cfg(not(feature = "build-info"))]
pub fn version_info() -> String {
    format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
}

#[cfg(not(feature = "build-info"))]
pub fn version_short() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

#[cfg(not(feature = "build-info"))]
pub fn package_name() -> &'static str {
    env!("CARGO_PKG_NAME")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_info_not_empty() {
        let info = version_info();
        assert!(!info.is_empty());
    }

    #[test]
    fn test_version_short() {
        let version = version_short();
        assert!(!version.is_empty());
    }

    #[test]
    fn test_package_name() {
        let name = package_name();
        assert_eq!(name, "sen");
    }
}
