//! Sensor system for environment context injection.
//!
//! Provides automatic context awareness for AI agents by collecting
//! environmental data (timestamps, CWD, Git status, etc.) without
//! requiring explicit prompting.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

// ============================================================================
// Sensor Data Structures
// ============================================================================

/// Complete sensor data collected from the environment.
///
/// This provides the "Heads-Up Display" (HUD) for AI agents,
/// eliminating the need for questions like "Where am I?" or "What changed?"
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensorData {
    /// Current timestamp in RFC3339 format
    pub timestamp: String,

    /// Current working directory
    pub cwd: PathBuf,

    /// Operating system and architecture (e.g., "darwin-aarch64")
    pub os_arch: String,

    /// Git repository information (if in a git repo)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git: Option<GitSensor>,

    /// Docker environment information (future)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub docker: Option<DockerSensor>,
}

/// Git repository sensor data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitSensor {
    /// Current branch name
    pub branch: String,

    /// Working tree status ("clean" or "dirty")
    pub status: String,

    /// Whether the working tree has uncommitted changes
    pub dirty: bool,
}

/// Docker environment sensor data (placeholder for future implementation).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerSensor {
    /// Whether running inside a Docker container
    pub in_container: bool,

    /// Container ID (if available)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_id: Option<String>,
}

// ============================================================================
// Sensor Collection
// ============================================================================

impl SensorData {
    /// Collect all available sensor data from the environment.
    ///
    /// This is called automatically when handlers request `Sensors` extractor.
    pub fn collect() -> Self {
        Self {
            timestamp: Self::collect_timestamp(),
            cwd: Self::collect_cwd(),
            os_arch: Self::collect_os_arch(),
            git: Self::collect_git(),
            docker: Self::collect_docker(),
        }
    }

    /// Collect current timestamp.
    fn collect_timestamp() -> String {
        chrono::Utc::now().to_rfc3339()
    }

    /// Collect current working directory.
    fn collect_cwd() -> PathBuf {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }

    /// Collect OS and architecture information.
    fn collect_os_arch() -> String {
        format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
    }

    /// Collect Git repository information.
    ///
    /// Returns `None` if not in a Git repository or if Git operations fail.
    fn collect_git() -> Option<GitSensor> {
        // Try to get current branch
        let branch = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .output()
            .ok()
            .and_then(|output| {
                if output.status.success() {
                    String::from_utf8(output.stdout).ok().map(|s| s.trim().to_string())
                } else {
                    None
                }
            })?;

        // Check if working tree is dirty
        let status_output = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .output()
            .ok()?;

        let dirty = !status_output.stdout.is_empty();
        let status = if dirty { "dirty" } else { "clean" };

        Some(GitSensor {
            branch,
            status: status.to_string(),
            dirty,
        })
    }

    /// Collect Docker environment information.
    ///
    /// Returns `None` if not running in Docker.
    fn collect_docker() -> Option<DockerSensor> {
        // Check if running in a Docker container by looking for .dockerenv
        let in_container = std::path::Path::new("/.dockerenv").exists();

        if !in_container {
            return None;
        }

        // Try to read container ID from cgroup
        let container_id = std::fs::read_to_string("/proc/self/cgroup")
            .ok()
            .and_then(|content| {
                content
                    .lines()
                    .find(|line| line.contains("docker"))
                    .and_then(|line| line.split('/').last())
                    .map(|id| id.to_string())
            });

        Some(DockerSensor {
            in_container,
            container_id,
        })
    }
}

// ============================================================================
// Sensors Extractor (for use in handlers)
// ============================================================================

/// Sensor data extractor for handler functions.
///
/// Similar to `Args<T>` and `State<S>`, this can be used in handler signatures
/// to automatically inject environment context.
///
/// # Example
///
/// ```ignore
/// use sen::{Sensors, CliResult, State};
///
/// async fn build(
///     state: State<AppState>,
///     sensors: Sensors,
///     args: Args<BuildArgs>
/// ) -> CliResult<String> {
///     println!("Building on branch: {}", sensors.git.as_ref()?.branch);
///     Ok("Build started".to_string())
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Sensors(pub SensorData);

impl Sensors {
    /// Get reference to inner sensor data.
    pub fn data(&self) -> &SensorData {
        &self.0
    }

    /// Get mutable reference to inner sensor data.
    pub fn data_mut(&mut self) -> &mut SensorData {
        &mut self.0
    }
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sensor_data_collect() {
        let data = SensorData::collect();

        // Timestamp should be valid RFC3339
        assert!(!data.timestamp.is_empty());

        // CWD should be a valid path
        assert!(data.cwd.exists() || data.cwd == PathBuf::from("."));

        // OS/arch should follow pattern "os-arch"
        assert!(data.os_arch.contains('-'));
    }

    #[test]
    fn test_os_arch_format() {
        let os_arch = SensorData::collect_os_arch();
        assert!(os_arch.contains('-'));

        // Should be non-empty and follow "os-arch" format
        let parts: Vec<&str> = os_arch.split('-').collect();
        assert!(parts.len() >= 2, "OS-arch should be in format 'os-arch'");
    }

    #[test]
    fn test_timestamp_format() {
        let timestamp = SensorData::collect_timestamp();

        // Should parse as RFC3339
        assert!(chrono::DateTime::parse_from_rfc3339(&timestamp).is_ok());
    }

    #[test]
    fn test_cwd_exists() {
        let cwd = SensorData::collect_cwd();
        assert!(cwd.exists() || cwd == PathBuf::from("."));
    }
}
