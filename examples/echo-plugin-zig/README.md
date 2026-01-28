# Echo Plugin (Zig)

A WASM plugin written in Zig, demonstrating that sen-rs plugins can be created
in any language that compiles to WebAssembly.

## Prerequisites

- [Zig](https://ziglang.org/) 0.15.x or later

```bash
# macOS
brew install zig

# Or download from https://ziglang.org/download/
```

## Building

```bash
# Release build (optimized for size, ~4.6KB)
zig build -Doptimize=ReleaseSmall

# Debug build
zig build
```

Output: `zig-out/bin/echo_plugin.wasm`

## Testing

```bash
# Copy to wasm-cli plugins directory
cp zig-out/bin/echo_plugin.wasm ../wasm-cli/plugins/

# Run wasm-cli
cd ../wasm-cli && cargo run

# In the REPL:
> echo Hello World
Echo: Hello World

> plugins
Loaded plugins (3):
  echo (v1.0.0) - Echoes arguments back (written in Zig)
```

## SDK Structure

```
echo-plugin-zig/
├── sdk/
│   ├── msgpack.zig   # MessagePack encoder/decoder
│   └── plugin.zig    # Plugin protocol & helpers
├── src/
│   └── main.zig      # Your plugin implementation
├── build.zig         # Zig build configuration
└── README.md
```

## Creating Your Own Plugin

1. **Define plugin metadata:**

```zig
const sdk = @import("sdk");

pub const plugin = sdk.Plugin{
    .name = "my-plugin",
    .about = "Description of my plugin",
    .version = "1.0.0",
    .args = &.{
        .{
            .name = "input",
            .help = "Input argument",
            .required = true,
        },
    },
};
```

2. **Implement the execute function:**

```zig
pub fn execute(ctx: *sdk.Context) sdk.Result {
    var iter = ctx.argsIter();
    if (iter.next()) |arg| {
        return ctx.successFmt("Got: {s}", .{arg});
    }
    return ctx.err("No input provided");
}
```

3. **Export the plugin (required):**

```zig
comptime {
    sdk.exportPlugin(@This());
}
```

## SDK API Reference

### Plugin Definition

| Field | Type | Description |
|-------|------|-------------|
| `name` | `[]const u8` | Command name |
| `about` | `?[]const u8` | Description |
| `version` | `?[]const u8` | Version string |
| `author` | `?[]const u8` | Author name |
| `args` | `[]const ArgSpec` | Arguments |
| `subcommands` | `[]const SubcommandSpec` | Subcommands |

### ArgSpec

| Field | Type | Description |
|-------|------|-------------|
| `name` | `[]const u8` | Argument name |
| `long` | `?[]const u8` | Long option (--name) |
| `short` | `?u8` | Short option (-n) |
| `required` | `bool` | Is required? |
| `help` | `?[]const u8` | Help text |
| `default_value` | `?[]const u8` | Default value |
| `possible_values` | `?[]const []const u8` | Allowed values |

### Context Methods

| Method | Description |
|--------|-------------|
| `argsIter()` | Get iterator over all arguments |
| `getArgAt(index)` | Get argument by index |
| `success(msg)` | Return success result |
| `successFmt(fmt, args)` | Return formatted success |
| `err(msg)` | Return error result |

## Protocol Details

The SDK implements the sen-plugin-api protocol:

1. **`plugin_alloc(size) -> ptr`**: Allocate memory for host-guest communication
2. **`plugin_dealloc(ptr, size)`**: Free allocated memory
3. **`plugin_manifest() -> packed_ptr_len`**: Return plugin metadata (MessagePack)
4. **`plugin_execute(args_ptr, args_len) -> packed_ptr_len`**: Execute command

All data exchange uses MessagePack encoding, handled by `sdk/msgpack.zig`.

## Why Zig?

- **No runtime**: Pure WASM without garbage collection overhead
- **Small binaries**: ~4.6KB for this plugin (ReleaseSmall)
- **C interop**: Easy to call C libraries if needed
- **Memory safety**: Comptime checks and explicit allocators
