# Environment Reader Plugin (Zig + WASI)

A WASM plugin written in Zig that demonstrates WASI capabilities for environment variable access.

## Features

- Reads environment variables using WASI
- Declares `env_read` capability for security
- Shows Zig SDK capabilities support

## Building

```bash
zig build wasm
```

The output will be at `zig-out/bin/env-reader-plugin.wasm`.

## Capabilities Declared

```zig
.capabilities = .{
    .env_read = &.{ "USER", "HOME", "PATH", "SHELL" },
    .stdio = sdk.StdioCapability.stdout_stderr(),
},
```

## Usage

```bash
# Read USER environment variable (default)
sen plugin run env-reader

# Read specific variable
sen plugin run env-reader HOME
```

## WASI Integration

This plugin uses WASI to access:
- Environment variables via `std.process.getenv()`
- stdout/stderr for output

The host must grant the declared capabilities for the plugin to function.
