//! sen-rs WASM Plugin SDK for Zig
//!
//! This SDK provides everything needed to create WASM plugins for sen-rs.
//!
//! ## Quick Start
//!
//! ```zig
//! const sdk = @import("sdk/plugin.zig");
//!
//! pub const plugin = sdk.Plugin{
//!     .name = "my-plugin",
//!     .about = "Does something useful",
//!     .version = "1.0.0",
//! };
//!
//! pub fn execute(ctx: *sdk.Context) sdk.Result {
//!     const msg = ctx.getArg("message") orelse "default";
//!     return ctx.success(msg);
//! }
//!
//! comptime {
//!     sdk.exportPlugin(@This());
//! }
//! ```

const std = @import("std");
pub const msgpack = @import("msgpack.zig");

// =============================================================================
// Memory Management
// =============================================================================

const allocator = std.heap.page_allocator;

/// Allocate memory for host-guest communication (exported)
pub export fn plugin_alloc(size: i32) i32 {
    if (size <= 0) return 0;
    const slice = allocator.alloc(u8, @intCast(size)) catch return 0;
    return @intCast(@intFromPtr(slice.ptr));
}

/// Deallocate memory (exported)
pub export fn plugin_dealloc(ptr: i32, size: i32) void {
    if (ptr == 0 or size <= 0) return;
    const slice_ptr: [*]u8 = @ptrFromInt(@as(usize, @intCast(ptr)));
    const slice = slice_ptr[0..@intCast(size)];
    allocator.free(slice);
}

// =============================================================================
// Capability Types (WASI)
// =============================================================================

/// Path pattern for filesystem access
pub const PathPattern = struct {
    /// Path pattern (e.g., "./data", "/tmp", "~/.config/app")
    pattern: []const u8,
    /// Allow recursive access to subdirectories
    recursive: bool = false,
};

/// Standard I/O capability
pub const StdioCapability = struct {
    stdin: bool = false,
    stdout: bool = false,
    stderr: bool = false,

    pub fn none() StdioCapability {
        return .{};
    }

    pub fn stdout_stderr() StdioCapability {
        return .{ .stdout = true, .stderr = true };
    }

    pub fn all() StdioCapability {
        return .{ .stdin = true, .stdout = true, .stderr = true };
    }

    pub fn is_none(self: StdioCapability) bool {
        return !self.stdin and !self.stdout and !self.stderr;
    }
};

/// Plugin capabilities (WASI permissions)
pub const Capabilities = struct {
    /// Filesystem read access paths
    fs_read: []const PathPattern = &.{},
    /// Filesystem write access paths
    fs_write: []const PathPattern = &.{},
    /// Environment variable access patterns (e.g., "HOME", "MY_*")
    env_read: []const []const u8 = &.{},
    /// Standard I/O access
    stdio: StdioCapability = .{},

    pub fn is_empty(self: Capabilities) bool {
        return self.fs_read.len == 0 and
            self.fs_write.len == 0 and
            self.env_read.len == 0 and
            self.stdio.is_none();
    }
};

// =============================================================================
// Plugin Definition Types
// =============================================================================

/// Argument specification for plugin commands
pub const ArgSpec = struct {
    /// Argument name (positional or named)
    name: []const u8,
    /// Long option name (e.g., "output" for --output)
    long: ?[]const u8 = null,
    /// Short option character (e.g., 'o' for -o)
    short: ?u8 = null,
    /// Whether the argument is required
    required: bool = false,
    /// Help text
    help: ?[]const u8 = null,
    /// Value placeholder in help (e.g., "FILE")
    value_name: ?[]const u8 = null,
    /// Default value if not provided
    default_value: ?[]const u8 = null,
    /// List of allowed values
    possible_values: ?[]const []const u8 = null,
};

/// Subcommand specification
pub const SubcommandSpec = struct {
    name: []const u8,
    about: ?[]const u8 = null,
    args: []const ArgSpec = &.{},
};

/// Plugin definition
pub const Plugin = struct {
    /// Command name
    name: []const u8,
    /// Description
    about: ?[]const u8 = null,
    /// Version string
    version: ?[]const u8 = null,
    /// Author
    author: ?[]const u8 = null,
    /// Arguments
    args: []const ArgSpec = &.{},
    /// Subcommands
    subcommands: []const SubcommandSpec = &.{},
    /// Capability requirements (WASI)
    capabilities: Capabilities = .{},
};

// =============================================================================
// Execution Context
// =============================================================================

/// Execution context with parsed arguments
pub const Context = struct {
    args_data: []const u8,
    output: msgpack.Buffer = .{},

    /// Get argument by index (for positional arguments)
    pub fn getArgAt(self: *Context, index: usize) ?[]const u8 {
        var iter = msgpack.StringIter.init(self.args_data);
        var i: usize = 0;
        while (iter.next()) |s| {
            if (i == index) return s;
            i += 1;
        }
        return null;
    }

    /// Get all arguments as an iterator
    pub fn argsIter(self: *Context) msgpack.StringIter {
        return msgpack.StringIter.init(self.args_data);
    }

    /// Build success result with message
    pub fn success(self: *Context, message: []const u8) Result {
        self.output.reset();
        msgpack.writeMapHeader(&self.output, 1);
        msgpack.writeStr(&self.output, "Success");
        msgpack.writeStr(&self.output, message);
        return .{ .data = self.output.slice() };
    }

    /// Build success result using format string
    pub fn successFmt(self: *Context, comptime fmt: []const u8, args: anytype) Result {
        self.output.reset();
        var temp: [2048]u8 = undefined;
        const message = std.fmt.bufPrint(&temp, fmt, args) catch fmt;
        return self.success(message);
    }

    /// Build error result
    pub fn err(self: *Context, message: []const u8) Result {
        self.output.reset();
        msgpack.writeMapHeader(&self.output, 1);
        msgpack.writeStr(&self.output, "Error");
        msgpack.writeStr(&self.output, message);
        return .{ .data = self.output.slice() };
    }
};

/// Execution result
pub const Result = struct {
    data: []const u8,
};

// =============================================================================
// Internal Helpers
// =============================================================================

/// Pack pointer and length into i64 for return
fn packPtrLen(ptr: i32, len: i32) i64 {
    return (@as(i64, ptr) << 32) | (@as(i64, len) & 0xFFFFFFFF);
}

/// Allocate memory and copy data, return packed ptr/len
fn allocAndCopy(data: []const u8) i64 {
    const len: i32 = @intCast(data.len);
    const ptr = plugin_alloc(len);
    if (ptr == 0) return 0;

    const dest: [*]u8 = @ptrFromInt(@as(usize, @intCast(ptr)));
    @memcpy(dest[0..data.len], data);

    return packPtrLen(ptr, len);
}

// =============================================================================
// Manifest Builder
// =============================================================================

fn writeArgSpec(buf: *msgpack.Buffer, arg: ArgSpec) void {
    msgpack.writeMapHeader(buf, 8);

    msgpack.writeStr(buf, "name");
    msgpack.writeStr(buf, arg.name);

    msgpack.writeStr(buf, "long");
    if (arg.long) |l| msgpack.writeStr(buf, l) else msgpack.writeNil(buf);

    msgpack.writeStr(buf, "short");
    if (arg.short) |s| {
        var char_buf: [1]u8 = .{s};
        msgpack.writeStr(buf, &char_buf);
    } else msgpack.writeNil(buf);

    msgpack.writeStr(buf, "required");
    msgpack.writeBool(buf, arg.required);

    msgpack.writeStr(buf, "help");
    if (arg.help) |h| msgpack.writeStr(buf, h) else msgpack.writeNil(buf);

    msgpack.writeStr(buf, "value_name");
    if (arg.value_name) |v| msgpack.writeStr(buf, v) else msgpack.writeNil(buf);

    msgpack.writeStr(buf, "default_value");
    if (arg.default_value) |d| msgpack.writeStr(buf, d) else msgpack.writeNil(buf);

    msgpack.writeStr(buf, "possible_values");
    if (arg.possible_values) |pv| {
        msgpack.writeArrayHeader(buf, pv.len);
        for (pv) |v| msgpack.writeStr(buf, v);
    } else msgpack.writeNil(buf);
}

fn writeSubcommand(buf: *msgpack.Buffer, sub: SubcommandSpec) void {
    msgpack.writeMapHeader(buf, 3);

    msgpack.writeStr(buf, "name");
    msgpack.writeStr(buf, sub.name);

    msgpack.writeStr(buf, "about");
    if (sub.about) |a| msgpack.writeStr(buf, a) else msgpack.writeNil(buf);

    msgpack.writeStr(buf, "args");
    msgpack.writeArrayHeader(buf, sub.args.len);
    for (sub.args) |arg| writeArgSpec(buf, arg);
}

fn writePathPattern(buf: *msgpack.Buffer, pattern: PathPattern) void {
    msgpack.writeMapHeader(buf, 2);
    msgpack.writeStr(buf, "pattern");
    msgpack.writeStr(buf, pattern.pattern);
    msgpack.writeStr(buf, "recursive");
    msgpack.writeBool(buf, pattern.recursive);
}

fn writeCapabilities(buf: *msgpack.Buffer, caps: Capabilities) void {
    // Count non-empty fields
    var field_count: usize = 0;
    if (caps.fs_read.len > 0) field_count += 1;
    if (caps.fs_write.len > 0) field_count += 1;
    if (caps.env_read.len > 0) field_count += 1;
    if (!caps.stdio.is_none()) field_count += 1;

    msgpack.writeMapHeader(buf, field_count);

    if (caps.fs_read.len > 0) {
        msgpack.writeStr(buf, "fs_read");
        msgpack.writeArrayHeader(buf, caps.fs_read.len);
        for (caps.fs_read) |p| writePathPattern(buf, p);
    }

    if (caps.fs_write.len > 0) {
        msgpack.writeStr(buf, "fs_write");
        msgpack.writeArrayHeader(buf, caps.fs_write.len);
        for (caps.fs_write) |p| writePathPattern(buf, p);
    }

    if (caps.env_read.len > 0) {
        msgpack.writeStr(buf, "env_read");
        msgpack.writeArrayHeader(buf, caps.env_read.len);
        for (caps.env_read) |e| msgpack.writeStr(buf, e);
    }

    if (!caps.stdio.is_none()) {
        msgpack.writeStr(buf, "stdio");
        msgpack.writeMapHeader(buf, 3);
        msgpack.writeStr(buf, "stdin");
        msgpack.writeBool(buf, caps.stdio.stdin);
        msgpack.writeStr(buf, "stdout");
        msgpack.writeBool(buf, caps.stdio.stdout);
        msgpack.writeStr(buf, "stderr");
        msgpack.writeBool(buf, caps.stdio.stderr);
    }
}

fn buildManifest(plugin: Plugin) []const u8 {
    const Static = struct {
        var buf: msgpack.Buffer = .{};
    };
    Static.buf.reset();

    // PluginManifest { api_version, command, capabilities? }
    const has_caps = !plugin.capabilities.is_empty();
    const map_size: usize = if (has_caps) 3 else 2;
    msgpack.writeMapHeader(&Static.buf, map_size);

    msgpack.writeStr(&Static.buf, "api_version");
    msgpack.writeUint(&Static.buf, 2); // API v2 for capabilities support

    msgpack.writeStr(&Static.buf, "command");
    msgpack.writeMapHeader(&Static.buf, 6);

    msgpack.writeStr(&Static.buf, "name");
    msgpack.writeStr(&Static.buf, plugin.name);

    msgpack.writeStr(&Static.buf, "about");
    if (plugin.about) |a| msgpack.writeStr(&Static.buf, a) else msgpack.writeNil(&Static.buf);

    msgpack.writeStr(&Static.buf, "version");
    if (plugin.version) |v| msgpack.writeStr(&Static.buf, v) else msgpack.writeNil(&Static.buf);

    msgpack.writeStr(&Static.buf, "author");
    if (plugin.author) |a| msgpack.writeStr(&Static.buf, a) else msgpack.writeNil(&Static.buf);

    msgpack.writeStr(&Static.buf, "args");
    msgpack.writeArrayHeader(&Static.buf, plugin.args.len);
    for (plugin.args) |arg| writeArgSpec(&Static.buf, arg);

    msgpack.writeStr(&Static.buf, "subcommands");
    msgpack.writeArrayHeader(&Static.buf, plugin.subcommands.len);
    for (plugin.subcommands) |sub| writeSubcommand(&Static.buf, sub);

    // Write capabilities if present
    if (has_caps) {
        msgpack.writeStr(&Static.buf, "capabilities");
        writeCapabilities(&Static.buf, plugin.capabilities);
    }

    return Static.buf.slice();
}

// =============================================================================
// Plugin Export Macro
// =============================================================================

/// Export plugin functions. Call this in comptime block:
///
/// ```zig
/// comptime {
///     sdk.exportPlugin(@This());
/// }
/// ```
pub fn exportPlugin(comptime T: type) void {
    const S = struct {
        export fn plugin_manifest() i64 {
            const data = buildManifest(T.plugin);
            return allocAndCopy(data);
        }

        export fn plugin_execute(args_ptr: i32, args_len: i32) i64 {
            var ctx = Context{
                .args_data = if (args_ptr != 0 and args_len > 0)
                    @as([*]const u8, @ptrFromInt(@as(usize, @intCast(args_ptr))))[0..@intCast(args_len)]
                else
                    &.{},
            };
            const result = T.execute(&ctx);
            return allocAndCopy(result.data);
        }
    };
    _ = S;
}
