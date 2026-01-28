#!/bin/bash
# Build script for Zig WASM plugin
#
# Usage:
#   ./build.sh          # Build in release mode
#   ./build.sh debug    # Build in debug mode

set -e

cd "$(dirname "$0")"

MODE="${1:-release}"

if [ "$MODE" = "debug" ]; then
    echo "Building Zig WASM plugin (debug)..."
    zig build -Doptimize=Debug
else
    echo "Building Zig WASM plugin (release)..."
    zig build -Doptimize=ReleaseSmall
fi

# Show output location
WASM_FILE="zig-out/bin/echo_plugin.wasm"
if [ -f "$WASM_FILE" ]; then
    SIZE=$(ls -lh "$WASM_FILE" | awk '{print $5}')
    echo ""
    echo "Success! Output: $WASM_FILE ($SIZE)"
    echo ""
    echo "To test with wasm-cli:"
    echo "  cp $WASM_FILE ../wasm-cli/plugins/"
    echo "  cd ../wasm-cli && cargo run"
else
    echo "Error: WASM file not found at $WASM_FILE"
    exit 1
fi
