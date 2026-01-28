//! Plugin discovery and directory scanning
//!
//! Automatically discovers and loads plugins from filesystem directories.

use crate::{LoadedPlugin, LoaderError, PluginLoader};
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during plugin discovery
#[derive(Debug, Error)]
pub enum DiscoveryError {
    #[error("Directory not found: {0}")]
    DirectoryNotFound(PathBuf),

    #[error("Failed to read directory: {0}")]
    ReadDirectory(#[source] std::io::Error),

    #[error("Failed to load plugin {path}: {source}")]
    LoadPlugin {
        path: PathBuf,
        #[source]
        source: LoaderError,
    },
}

/// Result of plugin discovery
pub struct DiscoveryResult {
    /// Successfully loaded plugins
    pub plugins: Vec<LoadedPlugin>,

    /// Plugins that failed to load (with errors)
    pub failures: Vec<(PathBuf, DiscoveryError)>,
}

impl DiscoveryResult {
    /// Returns true if all plugins loaded successfully
    pub fn is_success(&self) -> bool {
        self.failures.is_empty()
    }

    /// Total number of plugin files found
    pub fn total_found(&self) -> usize {
        self.plugins.len() + self.failures.len()
    }
}

/// Plugin directory scanner
pub struct PluginScanner {
    loader: PluginLoader,
}

impl PluginScanner {
    /// Create a new plugin scanner
    pub fn new() -> Result<Self, LoaderError> {
        Ok(Self {
            loader: PluginLoader::new()?,
        })
    }

    /// Create with an existing loader
    pub fn with_loader(loader: PluginLoader) -> Self {
        Self { loader }
    }

    /// Scan a directory for .wasm plugin files
    pub fn scan_directory(&self, dir: impl AsRef<Path>) -> Result<DiscoveryResult, DiscoveryError> {
        let dir = dir.as_ref();

        if !dir.exists() {
            return Err(DiscoveryError::DirectoryNotFound(dir.to_path_buf()));
        }

        if !dir.is_dir() {
            return Err(DiscoveryError::DirectoryNotFound(dir.to_path_buf()));
        }

        let entries = std::fs::read_dir(dir).map_err(DiscoveryError::ReadDirectory)?;

        let mut plugins = Vec::new();
        let mut failures = Vec::new();

        for entry in entries {
            let entry = match entry {
                Ok(e) => e,
                Err(e) => {
                    failures.push((dir.to_path_buf(), DiscoveryError::ReadDirectory(e)));
                    continue;
                }
            };

            let path = entry.path();

            // Only process .wasm files
            if path.extension().map(|e| e == "wasm").unwrap_or(false) {
                match self.load_plugin(&path) {
                    Ok(plugin) => plugins.push(plugin),
                    Err(e) => failures.push((path, e)),
                }
            }
        }

        Ok(DiscoveryResult { plugins, failures })
    }

    /// Scan multiple directories
    pub fn scan_directories(
        &self,
        dirs: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> DiscoveryResult {
        let mut all_plugins = Vec::new();
        let mut all_failures = Vec::new();

        for dir in dirs {
            match self.scan_directory(dir) {
                Ok(result) => {
                    all_plugins.extend(result.plugins);
                    all_failures.extend(result.failures);
                }
                Err(e) => {
                    // Directory-level errors
                    if let DiscoveryError::DirectoryNotFound(path) = &e {
                        all_failures.push((path.clone(), e));
                    }
                }
            }
        }

        DiscoveryResult {
            plugins: all_plugins,
            failures: all_failures,
        }
    }

    /// Load a single plugin file
    fn load_plugin(&self, path: &Path) -> Result<LoadedPlugin, DiscoveryError> {
        let wasm_bytes = std::fs::read(path).map_err(|e| DiscoveryError::LoadPlugin {
            path: path.to_path_buf(),
            source: LoaderError::MemoryAccess(format!("Failed to read file: {}", e)),
        })?;

        self.loader
            .load(&wasm_bytes)
            .map_err(|e| DiscoveryError::LoadPlugin {
                path: path.to_path_buf(),
                source: e,
            })
    }
}

/// Get default plugin directories for the current platform
pub fn default_plugin_dirs(app_name: &str) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // User-local plugins
    if let Some(data_dir) = dirs::data_local_dir() {
        dirs.push(data_dir.join(app_name).join("plugins"));
    }

    // Current directory plugins
    dirs.push(PathBuf::from("plugins"));

    dirs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_scan_empty_directory() {
        let temp = TempDir::new().unwrap();
        let scanner = PluginScanner::new().unwrap();

        let result = scanner.scan_directory(temp.path()).unwrap();
        assert!(result.plugins.is_empty());
        assert!(result.failures.is_empty());
        assert!(result.is_success());
    }

    #[test]
    fn test_scan_nonexistent_directory() {
        let scanner = PluginScanner::new().unwrap();
        let result = scanner.scan_directory("/nonexistent/path/to/plugins");

        assert!(result.is_err());
        match result {
            Err(DiscoveryError::DirectoryNotFound(_)) => {}
            _ => panic!("Expected DirectoryNotFound error"),
        }
    }

    #[test]
    fn test_scan_with_wasm_file() {
        let temp = TempDir::new().unwrap();

        // Copy hello plugin to temp directory
        let wasm_bytes = include_bytes!(
            "../../examples/hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm"
        );
        let plugin_path = temp.path().join("hello.wasm");
        fs::write(&plugin_path, wasm_bytes).unwrap();

        let scanner = PluginScanner::new().unwrap();
        let result = scanner.scan_directory(temp.path()).unwrap();

        assert_eq!(result.plugins.len(), 1);
        assert!(result.failures.is_empty());
        assert_eq!(result.plugins[0].manifest.command.name, "hello");
    }

    #[test]
    fn test_scan_ignores_non_wasm_files() {
        let temp = TempDir::new().unwrap();

        // Create non-wasm files
        fs::write(temp.path().join("readme.txt"), "Hello").unwrap();
        fs::write(temp.path().join("config.json"), "{}").unwrap();

        let scanner = PluginScanner::new().unwrap();
        let result = scanner.scan_directory(temp.path()).unwrap();

        assert!(result.plugins.is_empty());
        assert!(result.failures.is_empty());
    }

    #[test]
    fn test_default_plugin_dirs() {
        let dirs = default_plugin_dirs("myapp");
        assert!(!dirs.is_empty());
        assert!(dirs.iter().any(|d| d.ends_with("plugins")));
    }
}
