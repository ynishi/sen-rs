//! Environment Reader Plugin - Demonstrates WASI capabilities in Zig
//!
//! This plugin reads environment variables using WASI.
//! It declares `env_read` capability for accessing env vars.

const std = @import("std");
const sdk = @import("sdk");

// =============================================================================
// Plugin Definition
// =============================================================================

/// Plugin metadata with WASI capabilities
pub const plugin = sdk.Plugin{
    .name = "env-reader",
    .about = "Read environment variables (WASI demo in Zig)",
    .version = "1.0.0",
    .args = &.{
        .{
            .name = "var",
            .help = "Environment variable name to read",
            .default_value = "USER",
        },
    },
    // Declare WASI capabilities
    .capabilities = .{
        .env_read = &.{ "USER", "HOME", "PATH", "SHELL" },
        .stdio = sdk.StdioCapability.stdout_stderr(),
    },
};

// =============================================================================
// WASI Environment Access
// =============================================================================

/// Get environment variable value using WASI
fn getEnvVar(name: []const u8) ?[]const u8 {
    // Use std.posix.environ for WASI
    const env_map = std.process.getEnvMap(std.heap.page_allocator) catch return null;
    defer env_map.deinit();
    return env_map.get(name);
}

// =============================================================================
// Plugin Implementation
// =============================================================================

/// Execute the plugin - read and display environment variable
pub fn execute(ctx: *sdk.Context) sdk.Result {
    const var_name = ctx.getArgAt(0) orelse "USER";

    // Try to get all available env vars to show
    var env_map = std.process.getEnvMap(std.heap.page_allocator) catch {
        return ctx.err("Failed to access environment");
    };
    defer env_map.deinit();

    if (env_map.get(var_name)) |value| {
        return ctx.successFmt("=== {s} ===\n{s}", .{ var_name, value });
    } else {
        // Show available variables as hint
        var available_buf: [512]u8 = undefined;
        var available_len: usize = 0;

        var iter = env_map.iterator();
        var count: usize = 0;
        while (iter.next()) |entry| {
            if (count >= 5) break; // Show max 5
            if (available_len > 0 and available_len < available_buf.len - 2) {
                available_buf[available_len] = ',';
                available_buf[available_len + 1] = ' ';
                available_len += 2;
            }
            const name_len = @min(entry.key_ptr.len, available_buf.len - available_len);
            @memcpy(available_buf[available_len..][0..name_len], entry.key_ptr.*[0..name_len]);
            available_len += name_len;
            count += 1;
        }

        if (available_len > 0) {
            return ctx.successFmt("'{s}' not set. Available: {s}", .{ var_name, available_buf[0..available_len] });
        } else {
            return ctx.successFmt("'{s}' not set (no env vars available)", .{var_name});
        }
    }
}

// =============================================================================
// Export Plugin (Required)
// =============================================================================

comptime {
    sdk.exportPlugin(@This());
}
