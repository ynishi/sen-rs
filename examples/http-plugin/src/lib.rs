//! HTTP Fetcher Plugin - Demonstrates Effect system for async I/O
//!
//! This plugin fetches data from URLs using the Effect system.
//! Instead of making HTTP requests directly (impossible in WASM sandbox),
//! it requests the host to perform the fetch and continues with the result.
//!
//! # Flow
//!
//! ```text
//! Plugin: execute(["https://api.example.com/data"])
//!       → Effect::HttpGet { id: 1, url: "..." }
//!
//! Host:  Performs HTTP GET asynchronously
//!       → EffectResult::Http { status: 200, body: "..." }
//!
//! Plugin: resume(1, result)
//!       → Success("Fetched: ...")
//! ```

use sen_plugin_sdk::prelude::*;

struct HttpPlugin;

impl Plugin for HttpPlugin {
    fn manifest() -> PluginManifest {
        PluginManifest::with_capabilities(
            CommandSpec::new("http-fetch", "Fetch data from a URL (Effect demo)")
                .version("1.0.0")
                .arg(ArgSpec::positional("url").help("URL to fetch").required())
                .arg(
                    ArgSpec::option("method", "method")
                        .short('m')
                        .help("HTTP method (GET or POST)")
                        .default("GET"),
                ),
            Capabilities::default()
                .with_net(vec![
                    // Allow any HTTPS host
                    NetPattern::https("*"),
                ])
                .with_stdio(StdioCapability::stdout_stderr()),
        )
    }

    fn execute(args: Vec<String>) -> ExecuteResult {
        let url = match args.first() {
            Some(u) => u,
            None => return ExecuteResult::user_error("Usage: http-fetch <url>"),
        };

        let method = args.get(1).map(|s| s.as_str()).unwrap_or("GET");

        // Validate URL (basic check)
        if !url.starts_with("http://") && !url.starts_with("https://") {
            return ExecuteResult::user_error("URL must start with http:// or https://");
        }

        // Request the host to perform HTTP fetch
        // The host will call plugin_resume with the result
        match method.to_uppercase().as_str() {
            "GET" => ExecuteResult::http_get(1, url),
            "POST" => {
                // For POST, we'd need a body - using empty for demo
                ExecuteResult::http_post(1, url, "")
            }
            _ => ExecuteResult::user_error(format!("Unsupported method: {}", method)),
        }
    }

    fn resume(_effect_id: u32, result: EffectResult) -> ExecuteResult {
        match result {
            EffectResult::Http(response) => {
                if response.is_success() {
                    // Truncate long responses for display
                    let body = if response.body.len() > 500 {
                        format!(
                            "{}... ({} bytes total)",
                            &response.body[..500],
                            response.body.len()
                        )
                    } else {
                        response.body
                    };

                    ExecuteResult::success(format!("=== HTTP {} ===\n{}", response.status, body))
                } else {
                    ExecuteResult::user_error(format!(
                        "HTTP error {}: {}",
                        response.status,
                        response.body.chars().take(200).collect::<String>()
                    ))
                }
            }
            EffectResult::Error(e) => ExecuteResult::user_error(format!("Request failed: {}", e)),
            _ => ExecuteResult::system_error("Unexpected effect result"),
        }
    }
}

export_plugin!(HttpPlugin);
