//! MCP (Model Context Protocol) support for sen-rs
//!
//! This module provides MCP server capabilities, allowing CLI commands to be
//! exposed as MCP tools for AI agent integration.
//!
//! ## Features
//!
//! - `--mcp-server`: Start in MCP server mode (JSON-RPC over stdio)
//! - `--mcp-init <client>`: Generate MCP configuration for specified client
//! - Automatic tool schema generation from RouteMetadata
//! - Streaming support (stdout → MCP notifications)

use crate::{Output, Response, RouteMetadata};
use jsonrpc_core::{IoHandler, Params, Value};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::io::{BufRead, BufReader, Write};

/// MCP Tool definition according to Model Context Protocol specification
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpTool {
    /// Tool name (command name)
    pub name: String,
    /// Human-readable description
    pub description: String,
    /// JSON Schema for tool input parameters
    #[serde(rename = "inputSchema")]
    pub input_schema: serde_json::Value,
}

impl McpTool {
    /// Convert RouteMetadata to MCP tool schema
    ///
    /// # Arguments
    ///
    /// * `name` - Command name
    /// * `metadata` - Route metadata containing description and args schema
    ///
    /// # Returns
    ///
    /// An MCP tool with proper JSON Schema for input parameters
    pub fn from_route_metadata(name: String, metadata: &RouteMetadata) -> Self {
        // Get description from metadata (prefer route description over handler description)
        let description = metadata
            .get_description()
            .unwrap_or("No description available")
            .to_string();

        // Get input schema from metadata, or use default empty object schema
        let input_schema = metadata.get_args_schema().cloned().unwrap_or_else(|| {
            json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        });

        McpTool {
            name,
            description,
            input_schema,
        }
    }
}

/// MCP server handler
pub struct McpServer {
    io: IoHandler,
}

impl McpServer {
    /// Create a new MCP server
    pub fn new() -> Self {
        let mut io = IoHandler::new();

        // Register MCP protocol methods
        io.add_method("initialize", |_params: Params| async {
            Ok(Value::String("MCP Server initialized".to_string()))
        });

        Self { io }
    }

    /// Add a tool to the MCP server
    pub fn add_tool(&mut self, name: &str, description: &str) {
        let name = name.to_string();
        let desc = description.to_string();

        self.io.add_method(&name.clone(), move |_params: Params| {
            let result = json!({
                "tool": name,
                "description": desc,
                "executed": true
            });
            async move { Ok(result) }
        });
    }
}

impl Default for McpServer {
    fn default() -> Self {
        Self::new()
    }
}

/// Send an MCP notification to stdout
///
/// # Arguments
///
/// * `method` - Notification method (e.g., "notifications/message")
/// * `params` - Notification parameters as JSON value
fn send_notification(method: &str, params: Value) {
    let notification = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });

    if let Ok(notification_str) = serde_json::to_string(&notification) {
        println!("{}", notification_str);
        std::io::stdout().flush().ok();
    }
}

/// Convert MCP arguments (JSON object) to CLI arguments (Vec<String>)
///
/// # Arguments
///
/// * `arguments` - MCP arguments as JSON object
///
/// # Returns
///
/// Vector of CLI arguments
fn convert_mcp_arguments_to_cli_args(arguments: &Value) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(obj) = arguments.as_object() {
        for (key, value) in obj {
            if key.starts_with("--") {
                // Option argument (e.g., "--size", "50GB")
                args.push(key.clone());
                match value {
                    Value::Bool(true) => {
                        // Boolean flag, no value needed
                    }
                    Value::Bool(false) => {
                        // Skip false boolean flags
                        args.pop(); // Remove the flag we just added
                    }
                    Value::String(s) => {
                        args.push(s.clone());
                    }
                    Value::Number(n) => {
                        args.push(n.to_string());
                    }
                    _ => {
                        args.push(value.to_string());
                    }
                }
            } else {
                // Positional argument
                match value {
                    Value::String(s) => {
                        args.push(s.clone());
                    }
                    Value::Number(n) => {
                        args.push(n.to_string());
                    }
                    _ => {
                        args.push(value.to_string());
                    }
                }
            }
        }
    }

    args
}

/// Run MCP server mode (JSON-RPC over stdio)
///
/// This function starts an MCP server that listens on stdin for JSON-RPC requests
/// and writes responses to stdout. It registers all routes as MCP tools.
///
/// # Arguments
///
/// * `tools` - Vector of MCP tools with proper schemas
/// * `execute_tool` - Callback function to execute a tool with given name and arguments
///
/// # Returns
///
/// A Response indicating the server has shut down (exit code 0 for clean exit)
pub fn run_mcp_server<F>(tools: Vec<McpTool>, execute_tool: F) -> Response
where
    F: Fn(&str, Vec<String>) -> Response,
{
    let mut server = McpServer::new();

    // Register all tools
    for tool in &tools {
        server.add_tool(&tool.name, &tool.description);
    }

    // Store tools for tools/list response
    let tools_json = serde_json::to_value(&tools).unwrap_or_else(|_| json!([]));

    // Print server ready message to stderr (stdout is for JSON-RPC)
    eprintln!("MCP server started. Listening on stdin...");

    // Read from stdin, process JSON-RPC requests
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());

    for line in reader.lines() {
        match line {
            Ok(request_str) => {
                // Parse and handle JSON-RPC request
                if let Ok(request) = serde_json::from_str::<Value>(&request_str) {
                    let method = request.get("method").and_then(|m| m.as_str());

                    // Handle MCP protocol methods
                    let result = match method {
                        Some("initialize") => {
                            json!({
                                "protocolVersion": "2024-11-05",
                                "capabilities": {
                                    "tools": {},
                                    "logging": {}
                                },
                                "serverInfo": {
                                    "name": "sen-rs MCP server",
                                    "version": "0.5.0"
                                }
                            })
                        }
                        Some("tools/list") => {
                            json!({
                                "tools": tools_json
                            })
                        }
                        Some("tools/call") => {
                            // Extract tool name and arguments from params
                            let params = request.get("params");
                            let tool_name = params
                                .and_then(|p| p.get("name"))
                                .and_then(|n| n.as_str())
                                .unwrap_or("");

                            let arguments = params
                                .and_then(|p| p.get("arguments"))
                                .cloned()
                                .unwrap_or_else(|| json!({}));

                            // Send notification: execution started
                            send_notification(
                                "notifications/message",
                                json!({
                                    "level": "info",
                                    "logger": "sen-rs.mcp",
                                    "data": format!("Executing tool: {}", tool_name)
                                }),
                            );

                            // Convert MCP arguments to CLI args
                            let cli_args = convert_mcp_arguments_to_cli_args(&arguments);

                            // Execute the tool
                            let tool_response = execute_tool(tool_name, cli_args);

                            // Send notification: execution completed
                            let notification_level = if tool_response.exit_code == 0 {
                                "info"
                            } else {
                                "error"
                            };
                            send_notification(
                                "notifications/message",
                                json!({
                                    "level": notification_level,
                                    "logger": "sen-rs.mcp",
                                    "data": format!(
                                        "Tool {} completed with exit code {}",
                                        tool_name,
                                        tool_response.exit_code
                                    )
                                }),
                            );

                            // Convert Response to MCP result
                            let mcp_result = match tool_response.output {
                                Output::Text(text) => {
                                    json!({
                                        "content": [{
                                            "type": "text",
                                            "text": text
                                        }],
                                        "isError": tool_response.exit_code != 0
                                    })
                                }
                                Output::Json(json_str) => {
                                    json!({
                                        "content": [{
                                            "type": "text",
                                            "text": json_str
                                        }],
                                        "isError": tool_response.exit_code != 0
                                    })
                                }
                                Output::Silent => {
                                    json!({
                                        "content": [{
                                            "type": "text",
                                            "text": ""
                                        }],
                                        "isError": false
                                    })
                                }
                            };

                            mcp_result
                        }
                        _ => {
                            json!({
                                "status": "received",
                                "method": method
                            })
                        }
                    };

                    let response = json!({
                        "jsonrpc": "2.0",
                        "id": request.get("id"),
                        "result": result
                    });

                    // Write response to stdout
                    if let Ok(response_str) = serde_json::to_string(&response) {
                        println!("{}", response_str);
                        std::io::stdout().flush().ok();
                    }
                } else {
                    eprintln!("Failed to parse JSON-RPC request: {}", request_str);
                }
            }
            Err(e) => {
                eprintln!("Error reading from stdin: {}", e);
                break;
            }
        }
    }

    Response {
        exit_code: 0,
        output: Output::Silent,
        agent_mode: false,
        #[cfg(feature = "sensors")]
        metadata: None,
    }
}

/// Generate MCP configuration JSON for a given client
///
/// This function outputs the MCP server configuration JSON to stdout,
/// which users can copy into their client's configuration file.
///
/// # Arguments
///
/// * `client` - Client name (e.g., "claude", "cline")
/// * `command_path` - Path to the CLI executable
/// * `tools` - List of available MCP tools
///
/// # Returns
///
/// A Response with the configuration JSON in the output
pub fn generate_mcp_config(client: &str, command_path: String, tools: Vec<McpTool>) -> Response {
    // Get executable name from path
    let exe_name = std::path::Path::new(&command_path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("myctl");

    // Generate the mcpServers configuration
    let config = json!({
        "mcpServers": {
            exe_name: {
                "command": command_path,
                "args": ["--mcp-server"],
                "metadata": {
                    "description": format!("{} MCP server", exe_name),
                    "toolCount": tools.len()
                }
            }
        }
    });

    // Output instructions to stderr
    eprintln!("\n=== MCP Configuration for {} ===\n", client);
    eprintln!(
        "Copy the JSON below and merge it into your {} configuration file:",
        client
    );

    match client {
        "claude" => {
            eprintln!("  macOS: ~/Library/Application Support/Claude/claude_desktop_config.json");
            eprintln!("  Windows: %APPDATA%/Claude/claude_desktop_config.json");
        }
        "cline" => {
            eprintln!("  VS Code: Settings → Extensions → Cline → MCP Settings");
        }
        _ => {
            eprintln!(
                "  (Please refer to your client's documentation for the config file location)"
            );
        }
    }

    eprintln!("\nAvailable tools: {}", tools.len());
    eprintln!(
        "Tool names: {}\n",
        tools
            .iter()
            .map(|t| t.name.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    eprintln!("--- Configuration JSON ---\n");

    // Output the JSON to stdout (so users can pipe it to a file if needed)
    let config_str = serde_json::to_string_pretty(&config).unwrap_or_else(|_| "{}".to_string());

    Response {
        exit_code: 0,
        output: Output::Text(config_str),
        agent_mode: false,
        #[cfg(feature = "sensors")]
        metadata: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mcp_server_creation() {
        let _server = McpServer::new();
        // If we got here without panicking, the server was created successfully
    }

    #[test]
    fn test_add_tool() {
        let mut server = McpServer::new();
        server.add_tool("test_tool", "A test tool");
        // If we got here without panicking, the tool was added successfully
    }

    #[test]
    fn test_default_trait() {
        let _server = McpServer::default();
        // Verify Default trait works
    }

    #[test]
    fn test_mcp_tool_from_route_metadata() {
        use serde_json::json;

        // Create a mock RouteMetadata
        let metadata = RouteMetadata {
            handler_meta: None,
            description: Some("Test command description".to_string()),
            args_schema: Some(json!({
                "type": "object",
                "properties": {
                    "name": {
                        "type": "string",
                        "description": "Name parameter"
                    }
                },
                "required": ["name"]
            })),
        };

        let tool = McpTool::from_route_metadata("test-command".to_string(), &metadata);

        assert_eq!(tool.name, "test-command");
        assert_eq!(tool.description, "Test command description");
        assert_eq!(tool.input_schema["type"], "object");
        assert_eq!(tool.input_schema["properties"]["name"]["type"], "string");
    }

    #[test]
    fn test_mcp_tool_with_no_description() {
        let metadata = RouteMetadata {
            handler_meta: None,
            description: None,
            args_schema: None,
        };

        let tool = McpTool::from_route_metadata("test".to_string(), &metadata);

        assert_eq!(tool.description, "No description available");
        assert_eq!(tool.input_schema["type"], "object");
        assert_eq!(tool.input_schema["properties"], json!({}));
    }

    #[test]
    fn test_generate_mcp_config() {
        let tools = vec![McpTool {
            name: "test-tool".to_string(),
            description: "Test tool description".to_string(),
            input_schema: json!({"type": "object"}),
        }];

        let response = generate_mcp_config("claude", "/usr/bin/myctl".to_string(), tools);

        assert_eq!(response.exit_code, 0);
        assert!(!response.agent_mode);

        // Check that output contains valid JSON
        if let Output::Text(output) = response.output {
            let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
            assert!(parsed.get("mcpServers").is_some());
            assert!(parsed["mcpServers"].get("myctl").is_some());
            assert_eq!(parsed["mcpServers"]["myctl"]["command"], "/usr/bin/myctl");
            assert_eq!(
                parsed["mcpServers"]["myctl"]["args"],
                json!(["--mcp-server"])
            );
        } else {
            panic!("Expected text output");
        }
    }

    #[test]
    fn test_generate_mcp_config_extracts_exe_name() {
        let tools = vec![];
        let response = generate_mcp_config("cline", "/path/to/my-awesome-cli".to_string(), tools);

        if let Output::Text(output) = response.output {
            let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
            assert!(parsed["mcpServers"].get("my-awesome-cli").is_some());
        } else {
            panic!("Expected text output");
        }
    }

    #[test]
    fn test_convert_mcp_arguments_to_cli_args_with_string_options() {
        let arguments = json!({
            "name": "mydb",
            "--size": "100GB",
            "--engine": "postgres"
        });

        let args = super::convert_mcp_arguments_to_cli_args(&arguments);

        assert!(args.contains(&"mydb".to_string()));
        assert!(args.contains(&"--size".to_string()));
        assert!(args.contains(&"100GB".to_string()));
        assert!(args.contains(&"--engine".to_string()));
        assert!(args.contains(&"postgres".to_string()));
    }

    #[test]
    fn test_convert_mcp_arguments_to_cli_args_with_boolean_flags() {
        let arguments = json!({
            "--backup": true,
            "--force": false,
            "--verbose": true
        });

        let args = super::convert_mcp_arguments_to_cli_args(&arguments);

        // true flags should be present
        assert!(args.contains(&"--backup".to_string()));
        assert!(args.contains(&"--verbose".to_string()));

        // false flags should not be present
        assert!(!args.contains(&"--force".to_string()));
    }

    #[test]
    fn test_convert_mcp_arguments_to_cli_args_with_numbers() {
        let arguments = json!({
            "count": 42,
            "--port": 8080
        });

        let args = super::convert_mcp_arguments_to_cli_args(&arguments);

        assert!(args.contains(&"42".to_string()));
        assert!(args.contains(&"--port".to_string()));
        assert!(args.contains(&"8080".to_string()));
    }
}
