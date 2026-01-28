//! Echo Plugin - A WASM plugin written in Zig
//!
//! Demonstrates how to create sen-rs plugins using the Zig SDK.
//! This plugin echoes back all arguments provided.

const std = @import("std");
const sdk = @import("sdk");

// =============================================================================
// Plugin Definition
// =============================================================================

/// Plugin metadata and command specification
pub const plugin = sdk.Plugin{
    .name = "echo",
    .about = "Echoes arguments back (written in Zig)",
    .version = "1.0.0",
    .args = &.{
        .{
            .name = "message",
            .help = "Message to echo",
            .default_value = "Hello from Zig!",
        },
    },
};

// =============================================================================
// Plugin Implementation
// =============================================================================

/// Execute the plugin - echo arguments back
pub fn execute(ctx: *sdk.Context) sdk.Result {
    var iter = ctx.argsIter();

    // Collect all arguments
    var output: [2048]u8 = undefined;
    var len: usize = 0;

    // Add prefix
    const prefix = "Echo: ";
    @memcpy(output[0..prefix.len], prefix);
    len = prefix.len;

    var first = true;
    while (iter.next()) |arg| {
        if (!first and len < output.len - 1) {
            output[len] = ' ';
            len += 1;
        }
        const copy_len = @min(arg.len, output.len - len);
        @memcpy(output[len..][0..copy_len], arg[0..copy_len]);
        len += copy_len;
        first = false;
    }

    if (first) {
        // No arguments provided
        return ctx.success("Hello from Zig!");
    }

    return ctx.success(output[0..len]);
}

// =============================================================================
// Export Plugin (Required)
// =============================================================================

comptime {
    sdk.exportPlugin(@This());
}
