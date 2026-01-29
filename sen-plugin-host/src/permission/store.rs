//! Permission storage for persisting granted permissions
//!
//! Provides trait-based permission storage that framework users can customize.

use sen_plugin_api::Capabilities;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use thiserror::Error;

use super::strategy::PermissionGranularity;

/// Error type for permission store operations
#[derive(Debug, Error)]
pub enum StoreError {
    #[error("Failed to read permission store: {0}")]
    ReadError(#[from] std::io::Error),

    #[error("Failed to parse permission store: {0}")]
    ParseError(#[from] serde_json::Error),

    #[error("Permission not found for: {0}")]
    NotFound(String),

    #[error("Store is read-only")]
    ReadOnly,
}

/// Trust level for stored permissions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoredTrustLevel {
    /// Trust for this session only (cleared on restart)
    Session,
    /// Trust permanently
    Permanent,
}

/// Stored permission entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredPermission {
    /// When permission was granted (Unix timestamp)
    pub granted_at: u64,
    /// Hash of capabilities at grant time
    pub capabilities_hash: String,
    /// Granted capabilities
    pub capabilities: Capabilities,
    /// Trust level
    pub trust_level: StoredTrustLevel,
}

impl StoredPermission {
    /// Create a new stored permission
    pub fn new(capabilities: Capabilities, trust_level: StoredTrustLevel) -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let granted_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            granted_at,
            capabilities_hash: capabilities.compute_hash(),
            capabilities,
            trust_level,
        }
    }

    /// Check if this permission has escalated (capabilities changed)
    pub fn has_escalated(&self, new_caps: &Capabilities) -> bool {
        self.capabilities_hash != new_caps.compute_hash()
    }
}

/// Trait for permission storage
///
/// Framework users implement this trait to customize permission persistence.
pub trait PermissionStore: Send + Sync {
    /// Get stored permission for a plugin
    fn get(&self, key: &str) -> Result<Option<StoredPermission>, StoreError>;

    /// Store permission for a plugin
    fn set(&self, key: &str, permission: StoredPermission) -> Result<(), StoreError>;

    /// Remove permission for a plugin
    fn remove(&self, key: &str) -> Result<(), StoreError>;

    /// List all stored permissions
    fn list(&self) -> Result<Vec<(String, StoredPermission)>, StoreError>;

    /// Clear all permissions
    fn clear(&self) -> Result<(), StoreError>;

    /// Generate storage key from plugin/command info
    fn make_key(
        &self,
        plugin: &str,
        command: Option<&str>,
        granularity: PermissionGranularity,
    ) -> String {
        match granularity {
            PermissionGranularity::Plugin => plugin.to_string(),
            PermissionGranularity::Command => match command {
                Some(cmd) => format!("{}:{}", plugin, cmd),
                None => plugin.to_string(),
            },
            PermissionGranularity::Execution => {
                // Execution-level permissions are not stored
                format!("{}:execution", plugin)
            }
        }
    }
}

// ============================================================================
// File-based Permission Store
// ============================================================================

/// Persistent file data structure
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct PermissionFileData {
    version: u32,
    plugins: HashMap<String, StoredPermission>,
}

impl PermissionFileData {
    fn new() -> Self {
        Self {
            version: 1,
            plugins: HashMap::new(),
        }
    }
}

/// File-based permission store
///
/// Stores permissions in a JSON file at a configurable location.
/// Default: `~/.config/<app>/permissions.json`
pub struct FilePermissionStore {
    path: PathBuf,
    data: RwLock<PermissionFileData>,
}

impl FilePermissionStore {
    /// Create a new file-based store at the specified path
    pub fn new(path: impl AsRef<Path>) -> Result<Self, StoreError> {
        let path = path.as_ref().to_path_buf();

        let data = if path.exists() {
            let file = File::open(&path)?;
            let reader = BufReader::new(file);
            serde_json::from_reader(reader)?
        } else {
            PermissionFileData::new()
        };

        Ok(Self {
            path,
            data: RwLock::new(data),
        })
    }

    /// Create a store in the default location for an application
    pub fn default_for_app(app_name: &str) -> Result<Self, StoreError> {
        let config_dir = dirs::config_dir().unwrap_or_else(|| PathBuf::from(".config"));
        let path = config_dir.join(app_name).join("permissions.json");
        Self::new(path)
    }

    /// Get the store file path
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Save data to file
    fn save(&self) -> Result<(), StoreError> {
        // Create parent directory if needed
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }

        let data = self.data.read().unwrap();
        let file = File::create(&self.path)?;
        let writer = BufWriter::new(file);
        serde_json::to_writer_pretty(writer, &*data)?;
        Ok(())
    }
}

impl PermissionStore for FilePermissionStore {
    fn get(&self, key: &str) -> Result<Option<StoredPermission>, StoreError> {
        let data = self.data.read().unwrap();
        Ok(data.plugins.get(key).cloned())
    }

    fn set(&self, key: &str, permission: StoredPermission) -> Result<(), StoreError> {
        {
            let mut data = self.data.write().unwrap();
            data.plugins.insert(key.to_string(), permission);
        }
        self.save()
    }

    fn remove(&self, key: &str) -> Result<(), StoreError> {
        {
            let mut data = self.data.write().unwrap();
            data.plugins.remove(key);
        }
        self.save()
    }

    fn list(&self) -> Result<Vec<(String, StoredPermission)>, StoreError> {
        let data = self.data.read().unwrap();
        Ok(data
            .plugins
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect())
    }

    fn clear(&self) -> Result<(), StoreError> {
        {
            let mut data = self.data.write().unwrap();
            data.plugins.clear();
        }
        self.save()
    }
}

impl std::fmt::Debug for FilePermissionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FilePermissionStore")
            .field("path", &self.path)
            .finish()
    }
}

// ============================================================================
// In-Memory Permission Store
// ============================================================================

/// In-memory permission store for testing or session-only permissions
pub struct MemoryPermissionStore {
    data: RwLock<HashMap<String, StoredPermission>>,
}

impl MemoryPermissionStore {
    /// Create a new in-memory store
    pub fn new() -> Self {
        Self {
            data: RwLock::new(HashMap::new()),
        }
    }

    /// Get the number of stored permissions
    pub fn len(&self) -> usize {
        self.data.read().unwrap().len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.data.read().unwrap().is_empty()
    }
}

impl Default for MemoryPermissionStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PermissionStore for MemoryPermissionStore {
    fn get(&self, key: &str) -> Result<Option<StoredPermission>, StoreError> {
        let data = self.data.read().unwrap();
        Ok(data.get(key).cloned())
    }

    fn set(&self, key: &str, permission: StoredPermission) -> Result<(), StoreError> {
        let mut data = self.data.write().unwrap();
        data.insert(key.to_string(), permission);
        Ok(())
    }

    fn remove(&self, key: &str) -> Result<(), StoreError> {
        let mut data = self.data.write().unwrap();
        data.remove(key);
        Ok(())
    }

    fn list(&self) -> Result<Vec<(String, StoredPermission)>, StoreError> {
        let data = self.data.read().unwrap();
        Ok(data.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
    }

    fn clear(&self) -> Result<(), StoreError> {
        let mut data = self.data.write().unwrap();
        data.clear();
        Ok(())
    }
}

impl std::fmt::Debug for MemoryPermissionStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryPermissionStore")
            .field("count", &self.len())
            .finish()
    }
}

// ============================================================================
// Read-Only Permission Store
// ============================================================================

/// Read-only wrapper for any permission store
///
/// Useful for CI environments where permissions should be pre-defined
/// but not modified at runtime.
pub struct ReadOnlyPermissionStore<S: PermissionStore> {
    inner: S,
}

impl<S: PermissionStore> ReadOnlyPermissionStore<S> {
    /// Create a read-only wrapper
    pub fn new(inner: S) -> Self {
        Self { inner }
    }
}

impl<S: PermissionStore> PermissionStore for ReadOnlyPermissionStore<S> {
    fn get(&self, key: &str) -> Result<Option<StoredPermission>, StoreError> {
        self.inner.get(key)
    }

    fn set(&self, _key: &str, _permission: StoredPermission) -> Result<(), StoreError> {
        Err(StoreError::ReadOnly)
    }

    fn remove(&self, _key: &str) -> Result<(), StoreError> {
        Err(StoreError::ReadOnly)
    }

    fn list(&self) -> Result<Vec<(String, StoredPermission)>, StoreError> {
        self.inner.list()
    }

    fn clear(&self) -> Result<(), StoreError> {
        Err(StoreError::ReadOnly)
    }
}

impl<S: PermissionStore + std::fmt::Debug> std::fmt::Debug for ReadOnlyPermissionStore<S> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ReadOnlyPermissionStore")
            .field("inner", &self.inner)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sen_plugin_api::PathPattern;

    #[test]
    fn test_memory_store() {
        let store = MemoryPermissionStore::new();
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let perm = StoredPermission::new(caps, StoredTrustLevel::Permanent);

        store.set("test-plugin", perm.clone()).unwrap();

        let retrieved = store.get("test-plugin").unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().capabilities_hash, perm.capabilities_hash);

        store.remove("test-plugin").unwrap();
        assert!(store.get("test-plugin").unwrap().is_none());
    }

    #[test]
    fn test_file_store() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("permissions.json");

        let store = FilePermissionStore::new(&path).unwrap();
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let perm = StoredPermission::new(caps, StoredTrustLevel::Permanent);

        store.set("test-plugin", perm).unwrap();

        // Verify file was created
        assert!(path.exists());

        // Create new store from same file
        let store2 = FilePermissionStore::new(&path).unwrap();
        let retrieved = store2.get("test-plugin").unwrap();
        assert!(retrieved.is_some());
    }

    #[test]
    fn test_read_only_store() {
        let inner = MemoryPermissionStore::new();
        let caps = Capabilities::none();
        let perm = StoredPermission::new(caps, StoredTrustLevel::Session);
        inner.set("pre-existing", perm).unwrap();

        let store = ReadOnlyPermissionStore::new(inner);

        // Can read
        assert!(store.get("pre-existing").unwrap().is_some());

        // Cannot write
        let new_perm = StoredPermission::new(Capabilities::none(), StoredTrustLevel::Session);
        assert!(matches!(
            store.set("new", new_perm),
            Err(StoreError::ReadOnly)
        ));

        // Cannot remove
        assert!(matches!(
            store.remove("pre-existing"),
            Err(StoreError::ReadOnly)
        ));
    }

    #[test]
    fn test_escalation_detection() {
        let caps1 = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let perm = StoredPermission::new(caps1, StoredTrustLevel::Permanent);

        // Same capabilities
        let caps2 = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        assert!(!perm.has_escalated(&caps2));

        // Different capabilities (escalation)
        let caps3 = Capabilities::default()
            .with_fs_read(vec![PathPattern::new("./data")])
            .with_fs_write(vec![PathPattern::new("./output")]);
        assert!(perm.has_escalated(&caps3));
    }

    #[test]
    fn test_make_key() {
        let store = MemoryPermissionStore::new();

        let key = store.make_key("hello", None, PermissionGranularity::Plugin);
        assert_eq!(key, "hello");

        let key = store.make_key("hello", Some("greet"), PermissionGranularity::Command);
        assert_eq!(key, "hello:greet");

        let key = store.make_key("hello", None, PermissionGranularity::Command);
        assert_eq!(key, "hello");
    }
}
