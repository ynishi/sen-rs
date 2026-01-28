//! MessagePack Encoder/Decoder for sen-rs WASM plugins
//!
//! Minimal MessagePack implementation supporting the types needed
//! for plugin communication: strings, integers, arrays, maps, nil, bool.

const std = @import("std");

// =============================================================================
// Buffer - Fixed-size write buffer
// =============================================================================

/// Fixed-size buffer for building MessagePack data
pub const Buffer = struct {
    data: [4096]u8 = undefined,
    len: usize = 0,

    /// Append a single byte
    pub fn append(self: *Buffer, byte: u8) void {
        if (self.len < self.data.len) {
            self.data[self.len] = byte;
            self.len += 1;
        }
    }

    /// Append multiple bytes
    pub fn appendSlice(self: *Buffer, bytes: []const u8) void {
        for (bytes) |b| {
            self.append(b);
        }
    }

    /// Get the written data as a slice
    pub fn slice(self: *const Buffer) []const u8 {
        return self.data[0..self.len];
    }

    /// Reset the buffer
    pub fn reset(self: *Buffer) void {
        self.len = 0;
    }
};

// =============================================================================
// MessagePack Writers
// =============================================================================

/// Write an unsigned integer (auto-selects encoding)
pub fn writeUint(buf: *Buffer, value: u32) void {
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

/// Write a string
pub fn writeStr(buf: *Buffer, s: []const u8) void {
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

/// Write nil
pub fn writeNil(buf: *Buffer) void {
    buf.append(0xc0);
}

/// Write boolean true
pub fn writeTrue(buf: *Buffer) void {
    buf.append(0xc3);
}

/// Write boolean false
pub fn writeFalse(buf: *Buffer) void {
    buf.append(0xc2);
}

/// Write boolean
pub fn writeBool(buf: *Buffer, value: bool) void {
    if (value) writeTrue(buf) else writeFalse(buf);
}

/// Write array header (elements follow)
pub fn writeArrayHeader(buf: *Buffer, len: usize) void {
    if (len <= 15) {
        buf.append(@as(u8, 0x90) | @as(u8, @intCast(len)));
    } else if (len <= 65535) {
        buf.append(0xdc);
        buf.append(@intCast(len >> 8));
        buf.append(@intCast(len & 0xff));
    } else {
        buf.append(0xdd);
        buf.append(@intCast((len >> 24) & 0xff));
        buf.append(@intCast((len >> 16) & 0xff));
        buf.append(@intCast((len >> 8) & 0xff));
        buf.append(@intCast(len & 0xff));
    }
}

/// Write map header (key-value pairs follow)
pub fn writeMapHeader(buf: *Buffer, len: usize) void {
    if (len <= 15) {
        buf.append(@as(u8, 0x80) | @as(u8, @intCast(len)));
    } else if (len <= 65535) {
        buf.append(0xde);
        buf.append(@intCast(len >> 8));
        buf.append(@intCast(len & 0xff));
    } else {
        buf.append(0xdf);
        buf.append(@intCast((len >> 24) & 0xff));
        buf.append(@intCast((len >> 16) & 0xff));
        buf.append(@intCast((len >> 8) & 0xff));
        buf.append(@intCast(len & 0xff));
    }
}

// =============================================================================
// MessagePack Reader - String Array Iterator
// =============================================================================

/// Iterator for reading string arrays from MessagePack
pub const StringIter = struct {
    data: []const u8,
    pos: usize,
    count: usize,
    index: usize,

    /// Initialize from MessagePack array data
    pub fn init(data: []const u8) StringIter {
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
            // fixarray
            self.count = header - 0x90;
        } else if (header == 0xdc and data.len >= 3) {
            // array 16
            self.count = (@as(usize, data[1]) << 8) | data[2];
            self.pos = 3;
        } else if (header == 0xdd and data.len >= 5) {
            // array 32
            self.count = (@as(usize, data[1]) << 24) |
                (@as(usize, data[2]) << 16) |
                (@as(usize, data[3]) << 8) |
                data[4];
            self.pos = 5;
        }

        return self;
    }

    /// Get the next string, or null if exhausted
    pub fn next(self: *StringIter) ?[]const u8 {
        if (self.index >= self.count or self.pos >= self.data.len) return null;

        const str_header = self.data[self.pos];
        self.pos += 1;

        var str_len: usize = 0;
        if (str_header >= 0xa0 and str_header <= 0xbf) {
            // fixstr
            str_len = str_header - 0xa0;
        } else if (str_header == 0xd9 and self.pos < self.data.len) {
            // str 8
            str_len = self.data[self.pos];
            self.pos += 1;
        } else if (str_header == 0xda and self.pos + 1 < self.data.len) {
            // str 16
            str_len = (@as(usize, self.data[self.pos]) << 8) | self.data[self.pos + 1];
            self.pos += 2;
        } else if (str_header == 0xdb and self.pos + 3 < self.data.len) {
            // str 32
            str_len = (@as(usize, self.data[self.pos]) << 24) |
                (@as(usize, self.data[self.pos + 1]) << 16) |
                (@as(usize, self.data[self.pos + 2]) << 8) |
                self.data[self.pos + 3];
            self.pos += 4;
        } else {
            // Skip unknown type
            self.index += 1;
            return self.next();
        }

        if (self.pos + str_len > self.data.len) return null;

        const result = self.data[self.pos .. self.pos + str_len];
        self.pos += str_len;
        self.index += 1;

        return result;
    }

    /// Get remaining count
    pub fn remaining(self: *const StringIter) usize {
        return self.count - self.index;
    }

    /// Reset to beginning
    pub fn reset(self: *StringIter) void {
        self.* = init(self.data);
    }
};
