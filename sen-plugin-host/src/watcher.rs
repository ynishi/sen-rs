//! Hot reload file watcher for plugins
//!
//! Watches plugin directories for changes and automatically
//! loads, reloads, or unloads plugins.

use crate::{LoaderError, PluginRegistry};
use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::sync::mpsc;

/// Configuration for the hot reload watcher
#[derive(Debug, Clone)]
pub struct WatcherConfig {
    /// Debounce duration for file events
    pub debounce: Duration,
    /// Whether to load existing plugins on start
    pub load_existing: bool,
}

impl Default for WatcherConfig {
    fn default() -> Self {
        Self {
            debounce: Duration::from_millis(500),
            load_existing: true,
        }
    }
}

/// Hot reload watcher for plugin directories
pub struct HotReloadWatcher {
    registry: PluginRegistry,
    _watcher: RecommendedWatcher,
    shutdown_tx: mpsc::Sender<()>,
}

impl HotReloadWatcher {
    /// Create a new hot reload watcher for the given directories
    pub async fn new(
        registry: PluginRegistry,
        directories: impl IntoIterator<Item = impl AsRef<Path>>,
        config: WatcherConfig,
    ) -> Result<Self, WatcherError> {
        let directories: Vec<PathBuf> = directories
            .into_iter()
            .map(|p| p.as_ref().to_path_buf())
            .collect();

        // Load existing plugins if configured
        if config.load_existing {
            for dir in &directories {
                if dir.exists() && dir.is_dir() {
                    Self::load_directory(&registry, dir).await?;
                }
            }
        }

        // Create async channel for file events
        let (event_tx, mut event_rx) = mpsc::channel::<WatchEvent>(100);
        let (shutdown_tx, mut shutdown_rx) = mpsc::channel::<()>(1);

        // Create the file watcher
        let tx_clone = event_tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            if let Ok(event) = res {
                let _ = tx_clone.blocking_send(WatchEvent::FileEvent(event));
            }
        })
        .map_err(WatcherError::WatcherInit)?;

        // Watch directories
        for dir in &directories {
            if dir.exists() {
                watcher
                    .watch(dir, RecursiveMode::NonRecursive)
                    .map_err(WatcherError::WatcherInit)?;
                tracing::info!(dir = %dir.display(), "Watching directory for plugins");
            } else {
                tracing::warn!(dir = %dir.display(), "Directory does not exist, skipping");
            }
        }

        // Spawn event processing task
        let registry_clone = registry.clone();
        let debounce = config.debounce;
        tokio::spawn(async move {
            let mut pending_events: Vec<PathBuf> = Vec::new();
            let mut debounce_timer: Option<tokio::time::Instant> = None;

            loop {
                tokio::select! {
                    // Check for shutdown
                    _ = shutdown_rx.recv() => {
                        tracing::info!("Hot reload watcher shutting down");
                        break;
                    }

                    // Process file events
                    Some(WatchEvent::FileEvent(event)) = event_rx.recv() => {
                        for path in event.paths {
                            if Self::is_wasm_file(&path) {
                                if !pending_events.contains(&path) {
                                    pending_events.push(path);
                                }
                                debounce_timer = Some(tokio::time::Instant::now() + debounce);
                            }
                        }
                    }

                    // Check debounce timer
                    _ = async {
                        if let Some(deadline) = debounce_timer {
                            tokio::time::sleep_until(deadline).await;
                        } else {
                            std::future::pending::<()>().await;
                        }
                    } => {
                        // Process pending events
                        for path in pending_events.drain(..) {
                            Self::handle_file_change(&registry_clone, &path).await;
                        }
                        debounce_timer = None;
                    }
                }
            }
        });

        Ok(Self {
            registry,
            _watcher: watcher,
            shutdown_tx,
        })
    }

    /// Load all plugins from a directory
    async fn load_directory(registry: &PluginRegistry, dir: &Path) -> Result<(), WatcherError> {
        let entries = std::fs::read_dir(dir).map_err(|e| {
            WatcherError::Io(format!("Failed to read directory {}: {}", dir.display(), e))
        })?;

        for entry in entries.flatten() {
            let path = entry.path();
            if Self::is_wasm_file(&path) {
                match registry.load_plugin(&path).await {
                    Ok(cmd) => {
                        tracing::info!(command = %cmd, path = %path.display(), "Loaded plugin");
                    }
                    Err(e) => {
                        tracing::warn!(path = %path.display(), error = %e, "Failed to load plugin");
                    }
                }
            }
        }

        Ok(())
    }

    /// Handle a file change event
    async fn handle_file_change(registry: &PluginRegistry, path: &Path) {
        if path.exists() {
            // File created or modified - load/reload
            match registry.reload_by_path(path).await {
                Ok(cmd) => {
                    tracing::info!(command = %cmd, path = %path.display(), "Plugin reloaded");
                }
                Err(e) => {
                    tracing::warn!(path = %path.display(), error = %e, "Failed to reload plugin");
                }
            }
        } else {
            // File deleted - unload
            if let Some(cmd) = registry.unload_by_path(path).await {
                tracing::info!(command = %cmd, path = %path.display(), "Plugin unloaded (file deleted)");
            }
        }
    }

    /// Check if a path is a wasm file
    fn is_wasm_file(path: &Path) -> bool {
        path.extension().map(|ext| ext == "wasm").unwrap_or(false)
    }

    /// Get a reference to the plugin registry
    pub fn registry(&self) -> &PluginRegistry {
        &self.registry
    }

    /// Shutdown the watcher
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(()).await;
    }
}

enum WatchEvent {
    FileEvent(Event),
}

/// Errors that can occur during watching
#[derive(Debug, thiserror::Error)]
pub enum WatcherError {
    #[error("Failed to initialize watcher: {0}")]
    WatcherInit(#[source] notify::Error),

    #[error("IO error: {0}")]
    Io(String),

    #[error("Loader error: {0}")]
    Loader(#[from] LoaderError),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    const HELLO_PLUGIN_WASM: &[u8] = include_bytes!(
        "../../examples/hello-plugin/target/wasm32-unknown-unknown/release/hello_plugin.wasm"
    );

    #[tokio::test]
    async fn test_watcher_loads_existing() {
        let temp = TempDir::new().unwrap();
        let plugin_path = temp.path().join("hello.wasm");
        fs::write(&plugin_path, HELLO_PLUGIN_WASM).unwrap();

        let registry = PluginRegistry::new().unwrap();
        let _watcher = HotReloadWatcher::new(
            registry.clone(),
            vec![temp.path()],
            WatcherConfig::default(),
        )
        .await
        .unwrap();

        // Should have loaded the existing plugin
        assert!(registry.has_command("hello").await);
    }

    #[tokio::test]
    async fn test_watcher_hot_reload() {
        let temp = TempDir::new().unwrap();

        let registry = PluginRegistry::new().unwrap();
        let _watcher = HotReloadWatcher::new(
            registry.clone(),
            vec![temp.path()],
            WatcherConfig {
                debounce: Duration::from_millis(100),
                load_existing: true,
            },
        )
        .await
        .unwrap();

        // Initially empty
        assert!(!registry.has_command("hello").await);

        // Add a plugin file
        let plugin_path = temp.path().join("hello.wasm");
        fs::write(&plugin_path, HELLO_PLUGIN_WASM).unwrap();

        // Wait for debounce + processing
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Should have loaded the plugin
        assert!(registry.has_command("hello").await);

        // Delete the plugin file
        fs::remove_file(&plugin_path).unwrap();

        // Wait for debounce + processing
        tokio::time::sleep(Duration::from_millis(300)).await;

        // Should have unloaded the plugin
        assert!(!registry.has_command("hello").await);
    }
}
