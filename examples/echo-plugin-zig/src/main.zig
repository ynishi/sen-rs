//! Echo Plugin - A WASM plugin written in Zig
//!
//! Demonstrates that sen-rs plugins can be written in any language
//! that compiles to WASM. This plugin echoes back all arguments.

const std = @import("std");

// =============================================================================
// Memory Management (using page allocator for WASM)
// =============================================================================

const allocator = std.heap.page_allocator;

/// Allocate memory for host-guest communication
export fn plugin_alloc(size: i32) i32 {
    if (size <= 0) return 0;

    const slice = allocator.alloc(u8, @intCast(size)) catch return 0;
    return @intCast(@intFromPtr(slice.ptr));
}

/// Deallocate memory
export fn plugin_dealloc(ptr: i32, size: i32) void {
    if (ptr == 0 or size <= 0) return;

    const slice_ptr: [*]u8 = @ptrFromInt(@as(usize, @intCast(ptr)));
    const slice = slice_ptr[0..@intCast(size)];
    allocator.free(slice);
}

// =============================================================================
// MessagePack Writer (fixed buffer)
// =============================================================================

const Buffer = struct {
    data: [4096]u8 = undefined,
    len: usize = 0,

    fn append(self: *Buffer, byte: u8) void {
        if (self.len < self.data.len) {
            self.data[self.len] = byte;
            self.len += 1;
        }
    }

    fn appendSlice(self: *Buffer, bytes: []const u8) void {
        for (bytes) |b| {
            self.append(b);
        }
    }

    fn getSlice(self: *const Buffer) []const u8 {
        return self.data[0..self.len];
    }
};

fn writeU32(buf: *Buffer, value: u32) void {
    if (value <= 127) {
        buf.append(@intCast(value));
    } else if (value <= 255) {
        buf.append(0xcc);
        buf.append(@intCast(value));
    } else if (value <= 65535) {
        buf.append(0xcd);
        buf.append(@intCast(value >> 8));
        buf.append(@intCast(value & 0xff));
    } else {
        buf.append(0xce);
        buf.append(@intCast((value >> 24) & 0xff));
        buf.append(@intCast((value >> 16) & 0xff));
        buf.append(@intCast((value >> 8) & 0xff));
        buf.append(@intCast(value & 0xff));
    }
}

fn writeStr(buf: *Buffer, s: []const u8) void {
    if (s.len <= 31) {
        buf.append(@as(u8, 0xa0) | @as(u8, @intCast(s.len)));
    } else if (s.len <= 255) {
        buf.append(0xd9);
        buf.append(@intCast(s.len));
    } else {
        buf.append(0xda);
        buf.append(@intCast(s.len >> 8));
        buf.append(@intCast(s.len & 0xff));
    }
    buf.appendSlice(s);
}

fn writeNil(buf: *Buffer) void {
    buf.append(0xc0);
}

fn writeFalse(buf: *Buffer) void {
    buf.append(0xc2);
}

fn writeArrayHeader(buf: *Buffer, len: usize) void {
    if (len <= 15) {
        buf.append(@as(u8, 0x90) | @as(u8, @intCast(len)));
    } else if (len <= 65535) {
        buf.append(0xdc);
        buf.append(@intCast(len >> 8));
        buf.append(@intCast(len & 0xff));
    }
}

fn writeMapHeader(buf: *Buffer, len: usize) void {
    if (len <= 15) {
        buf.append(@as(u8, 0x80) | @as(u8, @intCast(len)));
    }
}

// =============================================================================
// MessagePack Reader (for string arrays)
// =============================================================================

const StringIter = struct {
    data: []const u8,
    pos: usize,
    count: usize,
    index: usize,

    fn init(data: []const u8) StringIter {
        var self = StringIter{
            .data = data,
            .pos = 0,
            .count = 0,
            .index = 0,
        };

        if (data.len == 0) return self;

        const header = data[0];
        self.pos = 1;

        if (header >= 0x90 and header <= 0x9f) {
            self.count = header - 0x90;
        } else if (header == 0xdc and data.len >= 3) {
            self.count = (@as(usize, data[1]) << 8) | data[2];
            self.pos = 3;
        }

        return self;
    }

    fn next(self: *StringIter) ?[]const u8 {
        if (self.index >= self.count or self.pos >= self.data.len) return null;

        const str_header = self.data[self.pos];
        self.pos += 1;

        var str_len: usize = 0;
        if (str_header >= 0xa0 and str_header <= 0xbf) {
            str_len = str_header - 0xa0;
        } else if (str_header == 0xd9 and self.pos < self.data.len) {
            str_len = self.data[self.pos];
            self.pos += 1;
        } else if (str_header == 0xda and self.pos + 1 < self.data.len) {
            str_len = (@as(usize, self.data[self.pos]) << 8) | self.data[self.pos + 1];
            self.pos += 2;
        } else {
            self.index += 1;
            return self.next();
        }

        if (self.pos + str_len > self.data.len) return null;

        const result = self.data[self.pos .. self.pos + str_len];
        self.pos += str_len;
        self.index += 1;

        return result;
    }
};

// =============================================================================
// Plugin Interface
// =============================================================================

/// Pack pointer and length into i64
fn packPtrLen(ptr: i32, len: i32) i64 {
    return (@as(i64, ptr) << 32) | (@as(i64, len) & 0xFFFFFFFF);
}

/// Copy data to allocated memory and return packed ptr/len
fn allocAndCopy(data: []const u8) i64 {
    const len: i32 = @intCast(data.len);
    const ptr = plugin_alloc(len);
    if (ptr == 0) return 0;

    const dest: [*]u8 = @ptrFromInt(@as(usize, @intCast(ptr)));
    @memcpy(dest[0..data.len], data);

    return packPtrLen(ptr, len);
}

/// Return plugin manifest
export fn plugin_manifest() i64 {
    var buf = Buffer{};

    // PluginManifest { api_version, command }
    writeMapHeader(&buf, 2);

    writeStr(&buf, "api_version");
    writeU32(&buf, 1);

    writeStr(&buf, "command");
    writeMapHeader(&buf, 6);

    writeStr(&buf, "name");
    writeStr(&buf, "echo");

    writeStr(&buf, "about");
    writeStr(&buf, "Echoes arguments back (written in Zig)");

    writeStr(&buf, "version");
    writeStr(&buf, "1.0.0");

    writeStr(&buf, "author");
    writeNil(&buf);

    writeStr(&buf, "args");
    writeArrayHeader(&buf, 1);
    // ArgSpec
    writeMapHeader(&buf, 8);
    writeStr(&buf, "name");
    writeStr(&buf, "message");
    writeStr(&buf, "long");
    writeNil(&buf);
    writeStr(&buf, "short");
    writeNil(&buf);
    writeStr(&buf, "required");
    writeFalse(&buf);
    writeStr(&buf, "help");
    writeStr(&buf, "Message to echo");
    writeStr(&buf, "value_name");
    writeNil(&buf);
    writeStr(&buf, "default_value");
    writeStr(&buf, "Hello from Zig!");
    writeStr(&buf, "possible_values");
    writeNil(&buf);

    writeStr(&buf, "subcommands");
    writeArrayHeader(&buf, 0);

    return allocAndCopy(buf.getSlice());
}

/// Execute the plugin - echo arguments back
export fn plugin_execute(args_ptr: i32, args_len: i32) i64 {
    if (args_ptr == 0 or args_len <= 0) {
        return writeSuccessResult("(no arguments)");
    }

    const slice: [*]const u8 = @ptrFromInt(@as(usize, @intCast(args_ptr)));
    const data = slice[0..@intCast(args_len)];

    // Parse arguments and build output
    var output = Buffer{};
    output.appendSlice("Echo: ");

    var iter = StringIter.init(data);
    var first = true;

    while (iter.next()) |s| {
        if (!first) output.append(' ');
        output.appendSlice(s);
        first = false;
    }

    if (first) {
        // No arguments parsed
        return writeSuccessResult("Hello from Zig!");
    }

    return writeSuccessResult(output.getSlice());
}

fn writeSuccessResult(message: []const u8) i64 {
    var buf = Buffer{};

    // ExecuteResult::Success("...")
    writeMapHeader(&buf, 1);
    writeStr(&buf, "Success");
    writeStr(&buf, message);

    return allocAndCopy(buf.getSlice());
}
