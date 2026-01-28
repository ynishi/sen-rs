# Echo Plugin (Zig)

A WASM plugin written in Zig, demonstrating that sen-rs plugins can be created
in any language that compiles to WebAssembly.

## Prerequisites

- [Zig](https://ziglang.org/) 0.11.0 or later

```bash
# macOS
brew install zig

# Or download from https://ziglang.org/download/
```

## Building

```bash
# Release build (optimized for size)
./build.sh

# Debug build
./build.sh debug

# Or use zig directly
zig build -Doptimize=ReleaseSmall
```

Output: `zig-out/bin/echo_plugin.wasm`

## Testing

```bash
# Copy to wasm-cli plugins directory
cp zig-out/bin/echo_plugin.wasm ../wasm-cli/plugins/

# Run wasm-cli
cd ../wasm-cli
cargo run

# In the REPL:
> echo Hello World
Echo: Hello World

> plugins
Loaded plugins (1):
  echo (v1.0.0)
    Echoes arguments back (written in Zig)
```

## How It Works

The plugin implements the sen-plugin-api protocol:

1. **`plugin_alloc(size) -> ptr`**: Allocate memory for host-guest communication
2. **`plugin_dealloc(ptr, size)`**: Free allocated memory
3. **`plugin_manifest() -> packed_ptr_len`**: Return plugin metadata (MessagePack)
4. **`plugin_execute(args_ptr, args_len) -> packed_ptr_len`**: Execute command

### MessagePack Encoding

The plugin manually encodes/decodes MessagePack since Zig doesn't have a
standard serde equivalent. The implementation in `main.zig` shows how to:

- Encode maps, arrays, strings, integers, nil
- Decode string arrays (for arguments)
- Pack pointer/length into i64 return values

## File Structure

```
echo-plugin-zig/
├── build.zig       # Zig build configuration
├── build.sh        # Convenience build script
├── README.md       # This file
└── src/
    └── main.zig    # Plugin implementation
```

## Why Zig?

- **No runtime**: Pure WASM without garbage collection overhead
- **Small binaries**: ~15KB for this simple plugin
- **C interop**: Easy to call C libraries if needed
- **Memory safety**: Comptime checks and explicit allocators
