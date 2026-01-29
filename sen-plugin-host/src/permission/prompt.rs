//! Permission prompt handling for user interaction
//!
//! Provides trait-based prompt handling that framework users can customize
//! to match their application's UI requirements.

use sen_plugin_api::Capabilities;
use std::io::{self, BufRead, Write};
use thiserror::Error;

use super::store::StoredTrustLevel;

/// Error type for prompt operations
#[derive(Debug, Error)]
pub enum PromptError {
    #[error("Prompt cancelled by user")]
    Cancelled,

    #[error("Non-interactive environment")]
    NonInteractive,

    #[error("I/O error: {0}")]
    IoError(#[from] io::Error),

    #[error("Timeout waiting for user response")]
    Timeout,
}

/// Result of a permission prompt
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum PromptResult {
    /// Allow this time only
    AllowOnce,
    /// Allow for this session
    AllowSession,
    /// Allow permanently
    AllowAlways,
    /// Deny the permission
    #[default]
    Deny,
}

impl PromptResult {
    /// Convert to storage trust level (if applicable)
    pub fn to_trust_level(&self) -> Option<StoredTrustLevel> {
        match self {
            Self::AllowOnce => None,
            Self::AllowSession => Some(StoredTrustLevel::Session),
            Self::AllowAlways => Some(StoredTrustLevel::Permanent),
            Self::Deny => None,
        }
    }

    /// Check if permission was granted
    pub fn is_allowed(&self) -> bool {
        matches!(
            self,
            Self::AllowOnce | Self::AllowSession | Self::AllowAlways
        )
    }

    /// Check if permission should be persisted
    pub fn should_persist(&self) -> bool {
        matches!(self, Self::AllowSession | Self::AllowAlways)
    }
}

/// Trait for handling permission prompts
///
/// Framework users implement this trait to customize how permission
/// prompts are displayed and how user input is collected.
///
/// # Example
///
/// ```rust
/// use sen_plugin_host::permission::{PromptHandler, PromptResult, PromptError};
/// use sen_plugin_api::Capabilities;
///
/// struct GuiPromptHandler {
///     // GUI framework handle
/// }
///
/// impl PromptHandler for GuiPromptHandler {
///     fn prompt(
///         &self,
///         plugin: &str,
///         capabilities: &Capabilities,
///     ) -> Result<PromptResult, PromptError> {
///         // Show GUI dialog
///         // For now, just approve
///         Ok(PromptResult::AllowOnce)
///     }
///
///     fn is_interactive(&self) -> bool {
///         true
///     }
/// }
/// ```
pub trait PromptHandler: Send + Sync {
    /// Display a permission prompt and get user's decision
    fn prompt(
        &self,
        plugin: &str,
        capabilities: &Capabilities,
    ) -> Result<PromptResult, PromptError>;

    /// Check if this handler supports interactive prompts
    fn is_interactive(&self) -> bool;

    /// Display an escalation warning and get user's decision
    fn prompt_escalation(
        &self,
        plugin: &str,
        old_caps: &Capabilities,
        new_caps: &Capabilities,
    ) -> Result<PromptResult, PromptError> {
        // Default implementation: treat as new permission request
        let _ = old_caps;
        self.prompt(plugin, new_caps)
    }
}

// ============================================================================
// Terminal Prompt Handler
// ============================================================================

/// Terminal-based prompt handler
///
/// Displays permission prompts in the terminal and reads user input.
#[derive(Debug)]
pub struct TerminalPromptHandler {
    /// Whether to show detailed capability information
    verbose: bool,
}

impl TerminalPromptHandler {
    /// Create a new terminal prompt handler
    pub fn new() -> Self {
        Self { verbose: true }
    }

    /// Create a minimal prompt handler (less verbose)
    pub fn minimal() -> Self {
        Self { verbose: false }
    }

    /// Format capabilities for display
    fn format_capabilities(&self, caps: &Capabilities) -> String {
        let mut lines = Vec::new();

        if !caps.fs_read.is_empty() {
            for path in &caps.fs_read {
                let recursive = if path.recursive { " (recursive)" } else { "" };
                lines.push(format!("  - Read files in: {}{}", path.pattern, recursive));
            }
        }

        if !caps.fs_write.is_empty() {
            for path in &caps.fs_write {
                let recursive = if path.recursive { " (recursive)" } else { "" };
                lines.push(format!("  - Write files in: {}{}", path.pattern, recursive));
            }
        }

        if !caps.env_read.is_empty() {
            let vars = caps.env_read.join(", ");
            lines.push(format!("  - Read environment: {}", vars));
        }

        if !caps.net.is_empty() {
            for net in &caps.net {
                let port_str = net.port.map(|p| format!(":{}", p)).unwrap_or_default();
                lines.push(format!("  - Network access: {}{}", net.host, port_str));
            }
        }

        if caps.stdio.stdin {
            lines.push("  - Read from stdin".to_string());
        }
        if caps.stdio.stdout {
            lines.push("  - Write to stdout".to_string());
        }
        if caps.stdio.stderr {
            lines.push("  - Write to stderr".to_string());
        }

        lines.join("\n")
    }
}

impl Default for TerminalPromptHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptHandler for TerminalPromptHandler {
    fn prompt(
        &self,
        plugin: &str,
        capabilities: &Capabilities,
    ) -> Result<PromptResult, PromptError> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        // Check if we're in an interactive terminal
        if !atty_check() {
            return Err(PromptError::NonInteractive);
        }

        // Display the prompt
        writeln!(stdout)?;
        writeln!(
            stdout,
            "Plugin \"{}\" requests the following permissions:",
            plugin
        )?;
        writeln!(stdout)?;

        if self.verbose {
            writeln!(stdout, "{}", self.format_capabilities(capabilities))?;
            writeln!(stdout)?;
        }

        write!(stdout, "Allow? [y]es / [n]o / [a]lways / [s]ession: ")?;
        stdout.flush()?;

        // Read user input
        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;

        let input = input.trim().to_lowercase();

        match input.as_str() {
            "y" | "yes" => Ok(PromptResult::AllowOnce),
            "n" | "no" => Ok(PromptResult::Deny),
            "a" | "always" => Ok(PromptResult::AllowAlways),
            "s" | "session" => Ok(PromptResult::AllowSession),
            "" => Ok(PromptResult::Deny), // Default to deny
            _ => {
                writeln!(stdout, "Invalid input, defaulting to deny")?;
                Ok(PromptResult::Deny)
            }
        }
    }

    fn is_interactive(&self) -> bool {
        atty_check()
    }

    fn prompt_escalation(
        &self,
        plugin: &str,
        old_caps: &Capabilities,
        new_caps: &Capabilities,
    ) -> Result<PromptResult, PromptError> {
        let stdin = io::stdin();
        let mut stdout = io::stdout();

        if !atty_check() {
            return Err(PromptError::NonInteractive);
        }

        writeln!(stdout)?;
        writeln!(
            stdout,
            "WARNING: Plugin \"{}\" requests ADDITIONAL permissions!",
            plugin
        )?;
        writeln!(stdout)?;

        if self.verbose {
            writeln!(stdout, "Previously granted:")?;
            writeln!(stdout, "{}", self.format_capabilities(old_caps))?;
            writeln!(stdout)?;
            writeln!(stdout, "Now requesting:")?;
            writeln!(stdout, "{}", self.format_capabilities(new_caps))?;
            writeln!(stdout)?;
        }

        write!(stdout, "Allow escalation? [y]es / [n]o / [a]lways: ")?;
        stdout.flush()?;

        let mut input = String::new();
        stdin.lock().read_line(&mut input)?;

        let input = input.trim().to_lowercase();

        match input.as_str() {
            "y" | "yes" => Ok(PromptResult::AllowOnce),
            "n" | "no" => Ok(PromptResult::Deny),
            "a" | "always" => Ok(PromptResult::AllowAlways),
            _ => Ok(PromptResult::Deny),
        }
    }
}

// ============================================================================
// Auto-Approve Handler (for testing/CI with pre-approved permissions)
// ============================================================================

/// Handler that automatically approves/denies based on configuration
#[derive(Debug)]
pub struct AutoPromptHandler {
    /// Default response
    default_response: PromptResult,
}

impl AutoPromptHandler {
    /// Create handler that always approves
    pub fn always_allow() -> Self {
        Self {
            default_response: PromptResult::AllowAlways,
        }
    }

    /// Create handler that always denies
    pub fn always_deny() -> Self {
        Self {
            default_response: PromptResult::Deny,
        }
    }

    /// Create handler with custom default response
    pub fn with_response(response: PromptResult) -> Self {
        Self {
            default_response: response,
        }
    }
}

impl PromptHandler for AutoPromptHandler {
    fn prompt(
        &self,
        _plugin: &str,
        _capabilities: &Capabilities,
    ) -> Result<PromptResult, PromptError> {
        Ok(self.default_response.clone())
    }

    fn is_interactive(&self) -> bool {
        false
    }
}

// ============================================================================
// Recording Handler (for testing)
// ============================================================================

/// Handler that records prompts for testing
#[derive(Debug, Default)]
pub struct RecordingPromptHandler {
    /// Recorded prompts
    prompts: std::sync::Mutex<Vec<RecordedPrompt>>,
    /// Response to return
    response: PromptResult,
}

/// A recorded prompt
#[derive(Debug, Clone)]
pub struct RecordedPrompt {
    pub plugin: String,
    pub capabilities_hash: String,
    pub is_escalation: bool,
}

impl RecordingPromptHandler {
    /// Create a new recording handler
    pub fn new(response: PromptResult) -> Self {
        Self {
            prompts: std::sync::Mutex::new(Vec::new()),
            response,
        }
    }

    /// Get all recorded prompts
    pub fn prompts(&self) -> Vec<RecordedPrompt> {
        self.prompts.lock().unwrap().clone()
    }

    /// Get the number of prompts
    pub fn prompt_count(&self) -> usize {
        self.prompts.lock().unwrap().len()
    }

    /// Clear recorded prompts
    pub fn clear(&self) {
        self.prompts.lock().unwrap().clear();
    }
}

impl PromptHandler for RecordingPromptHandler {
    fn prompt(
        &self,
        plugin: &str,
        capabilities: &Capabilities,
    ) -> Result<PromptResult, PromptError> {
        self.prompts.lock().unwrap().push(RecordedPrompt {
            plugin: plugin.to_string(),
            capabilities_hash: capabilities.compute_hash(),
            is_escalation: false,
        });
        Ok(self.response.clone())
    }

    fn is_interactive(&self) -> bool {
        false
    }

    fn prompt_escalation(
        &self,
        plugin: &str,
        _old_caps: &Capabilities,
        new_caps: &Capabilities,
    ) -> Result<PromptResult, PromptError> {
        self.prompts.lock().unwrap().push(RecordedPrompt {
            plugin: plugin.to_string(),
            capabilities_hash: new_caps.compute_hash(),
            is_escalation: true,
        });
        Ok(self.response.clone())
    }
}

// ============================================================================
// Helper functions
// ============================================================================

/// Check if stdin/stdout are connected to a terminal
fn atty_check() -> bool {
    // Use platform-specific checks for reliable terminal detection
    #[cfg(unix)]
    {
        use std::os::unix::io::AsRawFd;
        // Check if stdout is a TTY using libc
        // SAFETY: isatty is safe to call with any file descriptor
        unsafe { libc::isatty(std::io::stdout().as_raw_fd()) != 0 }
    }

    #[cfg(windows)]
    {
        use std::os::windows::io::AsRawHandle;
        // On Windows, check console mode
        use windows_sys::Win32::System::Console::{GetConsoleMode, CONSOLE_MODE};
        let handle = std::io::stdout().as_raw_handle();
        let mut mode: CONSOLE_MODE = 0;
        // SAFETY: GetConsoleMode is safe with valid handle
        unsafe { GetConsoleMode(handle as _, &mut mode) != 0 }
    }

    #[cfg(not(any(unix, windows)))]
    {
        // Fallback for other platforms
        std::env::var("TERM").is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sen_plugin_api::PathPattern;

    #[test]
    fn test_prompt_result() {
        assert!(PromptResult::AllowOnce.is_allowed());
        assert!(PromptResult::AllowAlways.is_allowed());
        assert!(!PromptResult::Deny.is_allowed());

        assert!(!PromptResult::AllowOnce.should_persist());
        assert!(PromptResult::AllowAlways.should_persist());
        assert!(PromptResult::AllowSession.should_persist());
    }

    #[test]
    fn test_auto_handler() {
        let handler = AutoPromptHandler::always_allow();
        let caps = Capabilities::none();

        let result = handler.prompt("test", &caps).unwrap();
        assert_eq!(result, PromptResult::AllowAlways);

        let handler = AutoPromptHandler::always_deny();
        let result = handler.prompt("test", &caps).unwrap();
        assert_eq!(result, PromptResult::Deny);
    }

    #[test]
    fn test_recording_handler() {
        let handler = RecordingPromptHandler::new(PromptResult::AllowOnce);
        let caps = Capabilities::default().with_fs_read(vec![PathPattern::new("./data")]);

        handler.prompt("plugin1", &caps).unwrap();
        handler.prompt("plugin2", &caps).unwrap();

        assert_eq!(handler.prompt_count(), 2);
        let prompts = handler.prompts();
        assert_eq!(prompts[0].plugin, "plugin1");
        assert_eq!(prompts[1].plugin, "plugin2");
    }

    #[test]
    fn test_format_capabilities() {
        let handler = TerminalPromptHandler::new();
        let caps = Capabilities::default()
            .with_fs_read(vec![PathPattern::new("./data").recursive()])
            .with_fs_write(vec![PathPattern::new("./output")])
            .with_env_read(vec!["HOME".into(), "PATH".into()]);

        let formatted = handler.format_capabilities(&caps);
        assert!(formatted.contains("./data"));
        assert!(formatted.contains("recursive"));
        assert!(formatted.contains("./output"));
        assert!(formatted.contains("HOME"));
    }
}
