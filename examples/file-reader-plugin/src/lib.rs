//! File Reader Plugin - Demonstrates WASI filesystem capabilities
//!
//! This plugin reads files from the filesystem using WASI.
//! It declares `fs_read` capability for the current directory.

use sen_plugin_api::{
    ArgSpec, Capabilities, CommandSpec, ExecuteResult, PathPattern, PluginManifest, StdioCapability,
};
use sen_plugin_sdk::prelude::*;
use std::fs;

struct FileReaderPlugin;

impl Plugin for FileReaderPlugin {
    fn manifest() -> PluginManifest {
        // Declare capabilities: read files from ./data and access HOME env
        let capabilities = Capabilities::default()
            .with_fs_read(vec![PathPattern::new("./data").recursive()])
            .with_env_read(vec!["HOME".into(), "USER".into()])
            .with_stdio(StdioCapability::stdout_stderr());

        PluginManifest::with_capabilities(
            CommandSpec::new(
                "file-reader",
                "Read files from the data directory (WASI demo)",
            )
            .version("1.0.0")
            .arg(
                ArgSpec::positional("filename")
                    .help("File to read from ./data directory")
                    .required(),
            )
            .arg(
                ArgSpec::option("lines", "lines")
                    .short('n')
                    .help("Number of lines to show (default: all)"),
            ),
            capabilities,
        )
    }

    fn execute(args: Vec<String>) -> ExecuteResult {
        // Parse arguments
        let filename = match args.first() {
            Some(f) => f,
            None => return ExecuteResult::user_error("Usage: file-reader <filename>"),
        };

        let max_lines: Option<usize> = args.get(1).and_then(|s| s.parse().ok());

        // Build the path (WASI preopened directory maps ./data to /data)
        let path = format!("/data/{}", filename);

        // Try to read the file using WASI filesystem
        match fs::read_to_string(&path) {
            Ok(content) => {
                let output = if let Some(n) = max_lines {
                    content.lines().take(n).collect::<Vec<_>>().join("\n")
                } else {
                    content
                };

                // Also show environment info to demonstrate env capability
                let user = std::env::var("USER").unwrap_or_else(|_| "unknown".into());
                let home = std::env::var("HOME").unwrap_or_else(|_| "unknown".into());

                ExecuteResult::success(format!(
                    "=== File: {} ===\n{}\n\n=== Environment ===\nUSER: {}\nHOME: {}",
                    filename, output, user, home
                ))
            }
            Err(e) => ExecuteResult::user_error(format!("Failed to read '{}': {}", path, e)),
        }
    }
}

export_plugin!(FileReaderPlugin);
