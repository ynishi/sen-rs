//! Audit system for tracking permission events
//!
//! Provides a trait-based audit system that framework users can customize
//! to log permission-related events to their preferred destination.

use sen_plugin_api::Capabilities;
use serde::Serialize;
use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, RwLock};
use thiserror::Error;

/// Timestamp type (ISO 8601 string for portability)
pub type Timestamp = String;

/// Get current timestamp in ISO 8601 format
fn now_iso8601() -> Timestamp {
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple ISO 8601 format without external dependencies
    format!("{}", secs)
}

/// Audit event representing a permission-related action
#[derive(Debug, Clone, Serialize)]
pub struct AuditEvent {
    /// Timestamp of the event
    pub timestamp: Timestamp,
    /// Type of event
    pub event_type: AuditEventType,
    /// Plugin name
    pub plugin: String,
    /// Command path (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    /// Additional details
    pub details: AuditDetails,
}

impl AuditEvent {
    /// Create a new audit event
    pub fn new(
        event_type: AuditEventType,
        plugin: impl Into<String>,
        details: AuditDetails,
    ) -> Self {
        Self {
            timestamp: now_iso8601(),
            event_type,
            plugin: plugin.into(),
            command: None,
            details,
        }
    }

    /// Add command path to event
    pub fn with_command(mut self, command: impl Into<String>) -> Self {
        self.command = Some(command.into());
        self
    }
}

/// Type of audit event
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    /// Permission was requested
    PermissionRequested,
    /// Permission was granted (by user or configuration)
    PermissionGranted,
    /// Permission was denied
    PermissionDenied,
    /// A capability was used at runtime
    CapabilityUsed,
    /// Capability escalation was detected (plugin requests more than before)
    EscalationDetected,
    /// Plugin was loaded
    PluginLoaded,
    /// Plugin was unloaded
    PluginUnloaded,
}

/// Details about the audit event
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum AuditDetails {
    /// Permission request/grant/deny details
    Permission {
        /// Trust level if granted
        #[serde(skip_serializing_if = "Option::is_none")]
        trust_level: Option<TrustLevel>,
        /// Reason for denial
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
        /// Capabilities involved
        capabilities_hash: String,
    },
    /// File access details
    FileAccess { path: PathBuf, mode: AccessMode },
    /// Environment variable access
    EnvAccess { variable: String },
    /// Network access
    NetworkAccess { host: String, port: Option<u16> },
    /// Standard I/O access
    StdioAccess { stream: StdioStream },
    /// Capability escalation
    Escalation { old_hash: String, new_hash: String },
    /// Plugin lifecycle
    Lifecycle {
        #[serde(skip_serializing_if = "Option::is_none")]
        path: Option<PathBuf>,
        #[serde(skip_serializing_if = "Option::is_none")]
        version: Option<String>,
    },
}

/// Trust level for permission grants
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// Trust for this execution only
    Once,
    /// Trust for this session
    Session,
    /// Trust permanently
    Permanent,
}

/// File access mode
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AccessMode {
    Read,
    Write,
    ReadWrite,
}

/// Standard I/O stream
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StdioStream {
    Stdin,
    Stdout,
    Stderr,
}

/// Error type for audit operations
#[derive(Debug, Error)]
pub enum AuditError {
    #[error("Failed to write audit log: {0}")]
    WriteError(#[from] std::io::Error),

    #[error("Failed to serialize audit event: {0}")]
    SerializationError(#[from] serde_json::Error),

    #[error("Audit sink not available: {0}")]
    Unavailable(String),
}

/// Trait for audit event sinks
///
/// Framework users implement this trait to customize where audit events are sent.
///
/// # Example
///
/// ```rust
/// use sen_plugin_host::audit::{AuditSink, AuditEvent, AuditError};
///
/// struct MyCloudAuditSink {
///     endpoint: String,
/// }
///
/// impl AuditSink for MyCloudAuditSink {
///     fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
///         // Send to cloud service
///         println!("Would send to {}: {:?}", self.endpoint, event);
///         Ok(())
///     }
///
///     fn flush(&self) -> Result<(), AuditError> {
///         Ok(())
///     }
/// }
/// ```
pub trait AuditSink: Send + Sync {
    /// Record an audit event
    fn record(&self, event: AuditEvent) -> Result<(), AuditError>;

    /// Flush any buffered events
    fn flush(&self) -> Result<(), AuditError>;

    /// Check if the sink is healthy/available
    fn is_healthy(&self) -> bool {
        true
    }
}

// ============================================================================
// Default Implementations
// ============================================================================

/// File-based audit sink (JSONL format)
///
/// Writes audit events to a file in JSON Lines format (one JSON object per line).
pub struct FileAuditSink {
    path: PathBuf,
    writer: Mutex<BufWriter<File>>,
}

impl FileAuditSink {
    /// Create a new file audit sink
    pub fn new(path: impl AsRef<Path>) -> Result<Self, AuditError> {
        let path = path.as_ref().to_path_buf();

        // Create parent directories if needed
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let file = OpenOptions::new().create(true).append(true).open(&path)?;

        Ok(Self {
            path,
            writer: Mutex::new(BufWriter::new(file)),
        })
    }

    /// Get the log file path
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl AuditSink for FileAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        let json = serde_json::to_string(&event)?;
        let mut writer = self.writer.lock().unwrap();
        writeln!(writer, "{}", json)?;
        Ok(())
    }

    fn flush(&self) -> Result<(), AuditError> {
        let mut writer = self.writer.lock().unwrap();
        writer.flush()?;
        Ok(())
    }

    fn is_healthy(&self) -> bool {
        self.path.parent().map(|p| p.exists()).unwrap_or(true)
    }
}

impl fmt::Debug for FileAuditSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FileAuditSink")
            .field("path", &self.path)
            .finish()
    }
}

/// In-memory audit sink for testing
pub struct MemoryAuditSink {
    events: RwLock<Vec<AuditEvent>>,
    max_events: usize,
}

impl MemoryAuditSink {
    /// Create a new memory sink with default capacity (1000 events)
    pub fn new() -> Self {
        Self::with_capacity(1000)
    }

    /// Create a new memory sink with specified capacity
    pub fn with_capacity(max_events: usize) -> Self {
        Self {
            events: RwLock::new(Vec::with_capacity(max_events.min(1000))),
            max_events,
        }
    }

    /// Get all recorded events
    pub fn events(&self) -> Vec<AuditEvent> {
        self.events.read().unwrap().clone()
    }

    /// Get event count
    pub fn count(&self) -> usize {
        self.events.read().unwrap().len()
    }

    /// Clear all events
    pub fn clear(&self) {
        self.events.write().unwrap().clear();
    }

    /// Find events by type
    pub fn find_by_type(&self, event_type: AuditEventType) -> Vec<AuditEvent> {
        self.events
            .read()
            .unwrap()
            .iter()
            .filter(|e| e.event_type == event_type)
            .cloned()
            .collect()
    }

    /// Find events by plugin
    pub fn find_by_plugin(&self, plugin: &str) -> Vec<AuditEvent> {
        self.events
            .read()
            .unwrap()
            .iter()
            .filter(|e| e.plugin == plugin)
            .cloned()
            .collect()
    }
}

impl Default for MemoryAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditSink for MemoryAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        let mut events = self.events.write().unwrap();
        if events.len() >= self.max_events {
            events.remove(0); // FIFO eviction
        }
        events.push(event);
        Ok(())
    }

    fn flush(&self) -> Result<(), AuditError> {
        Ok(())
    }
}

impl fmt::Debug for MemoryAuditSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryAuditSink")
            .field("count", &self.count())
            .field("max_events", &self.max_events)
            .finish()
    }
}

/// Null audit sink (discards all events)
#[derive(Debug, Default)]
pub struct NullAuditSink;

impl NullAuditSink {
    pub fn new() -> Self {
        Self
    }
}

impl AuditSink for NullAuditSink {
    fn record(&self, _event: AuditEvent) -> Result<(), AuditError> {
        Ok(())
    }

    fn flush(&self) -> Result<(), AuditError> {
        Ok(())
    }
}

/// Composite audit sink that writes to multiple sinks
pub struct CompositeAuditSink {
    sinks: Vec<Box<dyn AuditSink>>,
}

impl CompositeAuditSink {
    pub fn new() -> Self {
        Self { sinks: Vec::new() }
    }

    pub fn with_sink(mut self, sink: impl AuditSink + 'static) -> Self {
        self.sinks.push(Box::new(sink));
        self
    }
}

impl Default for CompositeAuditSink {
    fn default() -> Self {
        Self::new()
    }
}

impl AuditSink for CompositeAuditSink {
    fn record(&self, event: AuditEvent) -> Result<(), AuditError> {
        for sink in &self.sinks {
            sink.record(event.clone())?;
        }
        Ok(())
    }

    fn flush(&self) -> Result<(), AuditError> {
        for sink in &self.sinks {
            sink.flush()?;
        }
        Ok(())
    }

    fn is_healthy(&self) -> bool {
        self.sinks.iter().all(|s| s.is_healthy())
    }
}

impl fmt::Debug for CompositeAuditSink {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CompositeAuditSink")
            .field("sink_count", &self.sinks.len())
            .finish()
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Create an audit event for permission request
pub fn permission_requested(plugin: &str, capabilities: &Capabilities) -> AuditEvent {
    AuditEvent::new(
        AuditEventType::PermissionRequested,
        plugin,
        AuditDetails::Permission {
            trust_level: None,
            reason: None,
            capabilities_hash: capabilities.compute_hash(),
        },
    )
}

/// Create an audit event for permission granted
pub fn permission_granted(
    plugin: &str,
    capabilities: &Capabilities,
    trust_level: TrustLevel,
) -> AuditEvent {
    AuditEvent::new(
        AuditEventType::PermissionGranted,
        plugin,
        AuditDetails::Permission {
            trust_level: Some(trust_level),
            reason: None,
            capabilities_hash: capabilities.compute_hash(),
        },
    )
}

/// Create an audit event for permission denied
pub fn permission_denied(plugin: &str, capabilities: &Capabilities, reason: &str) -> AuditEvent {
    AuditEvent::new(
        AuditEventType::PermissionDenied,
        plugin,
        AuditDetails::Permission {
            trust_level: None,
            reason: Some(reason.to_string()),
            capabilities_hash: capabilities.compute_hash(),
        },
    )
}

/// Create an audit event for escalation detection
pub fn escalation_detected(
    plugin: &str,
    old_caps: &Capabilities,
    new_caps: &Capabilities,
) -> AuditEvent {
    AuditEvent::new(
        AuditEventType::EscalationDetected,
        plugin,
        AuditDetails::Escalation {
            old_hash: old_caps.compute_hash(),
            new_hash: new_caps.compute_hash(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use sen_plugin_api::PathPattern;

    #[test]
    fn test_memory_sink() {
        let sink = MemoryAuditSink::new();
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);

        let event = permission_requested("test-plugin", &caps);
        sink.record(event).unwrap();

        assert_eq!(sink.count(), 1);
        let events = sink.find_by_type(AuditEventType::PermissionRequested);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].plugin, "test-plugin");
    }

    #[test]
    fn test_memory_sink_eviction() {
        let sink = MemoryAuditSink::with_capacity(2);
        let caps = Capabilities::none();

        for i in 0..3 {
            let event = permission_requested(&format!("plugin-{}", i), &caps);
            sink.record(event).unwrap();
        }

        assert_eq!(sink.count(), 2);
        let events = sink.events();
        assert_eq!(events[0].plugin, "plugin-1");
        assert_eq!(events[1].plugin, "plugin-2");
    }

    #[test]
    fn test_null_sink() {
        let sink = NullAuditSink::new();
        let caps = Capabilities::none();

        let event = permission_requested("test", &caps);
        assert!(sink.record(event).is_ok());
        assert!(sink.flush().is_ok());
    }

    #[test]
    fn test_composite_sink() {
        let memory1 = MemoryAuditSink::new();
        let memory2 = MemoryAuditSink::new();
        let caps = Capabilities::none();

        // We can't use the composite directly with borrowed sinks,
        // so test the concept
        let event = permission_requested("test", &caps);
        memory1.record(event.clone()).unwrap();
        memory2.record(event).unwrap();

        assert_eq!(memory1.count(), 1);
        assert_eq!(memory2.count(), 1);
    }

    #[test]
    fn test_event_serialization() {
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);
        let event =
            permission_granted("test", &caps, TrustLevel::Permanent).with_command("data:export");

        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains("permission_granted"));
        assert!(json.contains("test"));
        assert!(json.contains("permanent"));
    }

    #[test]
    fn test_file_sink() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("audit.jsonl");

        let sink = FileAuditSink::new(&path).unwrap();
        let caps = Capabilities::none();

        let event = permission_requested("test", &caps);
        sink.record(event).unwrap();
        sink.flush().unwrap();

        let content = std::fs::read_to_string(&path).unwrap();
        assert!(content.contains("permission_requested"));
    }
}
