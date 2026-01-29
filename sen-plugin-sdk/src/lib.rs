//! sen-plugin-sdk: SDK for creating WASM plugins
//!
//! This SDK provides utilities and helpers for creating WASM plugins
//! with minimal boilerplate. Using this SDK, you can create a fully functional
//! plugin in under 30 lines of code.
//!
//! # Table of Contents
//!
//! - [Project Setup](#project-setup)
//! - [Quick Start](#quick-start)
//! - [Arguments](#arguments)
//! - [Error Handling](#error-handling)
//! - [Advanced Usage](#advanced-usage)
//! - [Manual Implementation](#manual-implementation)
//! - [Best Practices](#best-practices)
//! - [Troubleshooting](#troubleshooting)
//!
//! # Project Setup
//!
//! ## 1. Create a New Plugin Project
//!
//! ```bash
//! cargo new --lib my-plugin
//! cd my-plugin
//! ```
//!
//! ## 2. Configure Cargo.toml
//!
//! Your complete `Cargo.toml` should look like:
//!
//! ```toml
//! [package]
//! name = "my-plugin"
//! version = "0.1.0"
//! edition = "2021"
//!
//! [lib]
//! crate-type = ["cdylib"]  # Required for WASM output
//!
//! [dependencies]
//! sen-plugin-sdk = { version = "0.7" }
//!
//! # Optimize for size (optional but recommended)
//! [profile.release]
//! opt-level = "s"
//! lto = true
//! strip = true
//! ```
//!
//! ## 3. Install WASM Target (One-Time)
//!
//! ```bash
//! rustup target add wasm32-unknown-unknown
//! ```
//!
//! ## 4. Build Your Plugin
//!
//! ```bash
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! The output file will be at:
//! `target/wasm32-unknown-unknown/release/my_plugin.wasm`
//!
//! # Quick Start
//!
//! A minimal plugin requires:
//! 1. A struct implementing the [`Plugin`] trait
//! 2. The [`export_plugin!`] macro to generate WASM exports
//!
//! ```rust,ignore
//! use sen_plugin_sdk::prelude::*;
//!
//! struct HelloPlugin;
//!
//! impl Plugin for HelloPlugin {
//!     fn manifest() -> PluginManifest {
//!         PluginManifest::new(
//!             CommandSpec::new("hello", "Says hello to the world")
//!                 .version("1.0.0")
//!                 .arg(ArgSpec::positional("name").help("Name to greet"))
//!         )
//!     }
//!
//!     fn execute(args: Vec<String>) -> ExecuteResult {
//!         let name = args.first().map(|s| s.as_str()).unwrap_or("World");
//!         ExecuteResult::success(format!("Hello, {}!", name))
//!     }
//! }
//!
//! export_plugin!(HelloPlugin);
//! ```
//!
//! # Arguments
//!
//! ## Positional Arguments
//!
//! Positional arguments are passed in order:
//!
//! ```rust,ignore
//! CommandSpec::new("copy", "Copy files")
//!     .arg(ArgSpec::positional("source").required().help("Source file"))
//!     .arg(ArgSpec::positional("dest").required().help("Destination file"))
//! ```
//!
//! Usage: `copy src.txt dst.txt`
//!
//! In `execute()`, args are: `["src.txt", "dst.txt"]`
//!
//! ## Options (Flags with Values)
//!
//! Named options with long and short forms:
//!
//! ```rust,ignore
//! CommandSpec::new("greet", "Greet someone")
//!     .arg(ArgSpec::positional("name").default("World"))
//!     .arg(
//!         ArgSpec::option("greeting", "greeting")
//!             .short('g')
//!             .help("Custom greeting message")
//!             .default("Hello")
//!     )
//!     .arg(
//!         ArgSpec::option("count", "count")
//!             .short('n')
//!             .help("Number of times to greet")
//!             .default("1")
//!     )
//! ```
//!
//! Usage: `greet Alice -g "Good morning" --count 3`
//!
//! ## Required Arguments
//!
//! Mark arguments as required:
//!
//! ```rust,ignore
//! ArgSpec::positional("file")
//!     .required()
//!     .help("Input file (required)")
//! ```
//!
//! ## Default Values
//!
//! Provide fallback values:
//!
//! ```rust,ignore
//! ArgSpec::option("format", "format")
//!     .short('f')
//!     .default("json")
//!     .help("Output format [default: json]")
//! ```
//!
//! ## Argument Parsing in execute()
//!
//! Arguments are passed as a `Vec<String>` in the order they appear.
//! The host handles option parsing; your plugin receives resolved values:
//!
//! ```rust,ignore
//! fn execute(args: Vec<String>) -> ExecuteResult {
//!     // For: greet Alice -g "Hi"
//!     // args = ["Alice", "Hi"]
//!
//!     let name = args.get(0).map(|s| s.as_str()).unwrap_or("World");
//!     let greeting = args.get(1).map(|s| s.as_str()).unwrap_or("Hello");
//!
//!     ExecuteResult::success(format!("{}, {}!", greeting, name))
//! }
//! ```
//!
//! # Error Handling
//!
//! Plugins return [`ExecuteResult`] which can be:
//!
//! ## Success
//!
//! ```rust,ignore
//! ExecuteResult::success("Operation completed successfully")
//! ```
//!
//! ## User Error (Exit Code 1)
//!
//! For expected errors like invalid input:
//!
//! ```rust,ignore
//! fn execute(args: Vec<String>) -> ExecuteResult {
//!     let file = match args.first() {
//!         Some(f) => f,
//!         None => return ExecuteResult::user_error("Missing required argument: file"),
//!     };
//!
//!     if !is_valid_format(file) {
//!         return ExecuteResult::user_error(format!(
//!             "Invalid file format: {}. Expected .json or .yaml",
//!             file
//!         ));
//!     }
//!
//!     ExecuteResult::success("File processed")
//! }
//! ```
//!
//! ## System Error (Exit Code 101)
//!
//! For unexpected internal errors:
//!
//! ```rust,ignore
//! fn execute(args: Vec<String>) -> ExecuteResult {
//!     match process_data(&args) {
//!         Ok(result) => ExecuteResult::success(result),
//!         Err(e) => ExecuteResult::system_error(format!("Internal error: {}", e)),
//!     }
//! }
//! ```
//!
//! # Advanced Usage
//!
//! ## Subcommands
//!
//! Create nested command structures:
//!
//! ```rust,ignore
//! CommandSpec::new("db", "Database operations")
//!     .subcommand(
//!         CommandSpec::new("create", "Create a new database")
//!             .arg(ArgSpec::positional("name").required())
//!     )
//!     .subcommand(
//!         CommandSpec::new("drop", "Drop a database")
//!             .arg(ArgSpec::positional("name").required())
//!     )
//!     .subcommand(
//!         CommandSpec::new("list", "List all databases")
//!     )
//! ```
//!
//! ## Plugin Metadata
//!
//! Add author and version information:
//!
//! ```rust,ignore
//! CommandSpec::new("mytool", "My awesome tool")
//!     .version("2.1.0")
//!     // Note: author is set on CommandSpec, not PluginManifest
//! ```
//!
//! # Manual Implementation
//!
//! If you need more control, you can implement the WASM exports manually
//! instead of using the SDK. This is what the `export_plugin!` macro generates:
//!
//! ```rust,ignore
//! use sen_plugin_api::{ArgSpec, CommandSpec, ExecuteResult, PluginManifest, API_VERSION};
//! use std::alloc::{alloc, dealloc, Layout};
//!
//! // 1. Memory allocator for host-guest communication
//! #[no_mangle]
//! pub extern "C" fn plugin_alloc(size: i32) -> i32 {
//!     if size <= 0 { return 0; }
//!     let layout = Layout::from_size_align(size as usize, 1).unwrap();
//!     unsafe { alloc(layout) as i32 }
//! }
//!
//! // 2. Memory deallocator
//! #[no_mangle]
//! pub extern "C" fn plugin_dealloc(ptr: i32, size: i32) {
//!     if ptr == 0 || size <= 0 { return; }
//!     let layout = Layout::from_size_align(size as usize, 1).unwrap();
//!     unsafe { dealloc(ptr as *mut u8, layout) }
//! }
//!
//! // 3. Return plugin manifest (command specification)
//! #[no_mangle]
//! pub extern "C" fn plugin_manifest() -> i64 {
//!     let manifest = PluginManifest {
//!         api_version: API_VERSION,
//!         command: CommandSpec::new("hello", "Says hello")
//!             .arg(ArgSpec::positional("name").default("World")),
//!     };
//!     serialize_to_memory(&manifest)
//! }
//!
//! // 4. Execute the command
//! #[no_mangle]
//! pub extern "C" fn plugin_execute(args_ptr: i32, args_len: i32) -> i64 {
//!     let args: Vec<String> = unsafe {
//!         let slice = std::slice::from_raw_parts(args_ptr as *const u8, args_len as usize);
//!         rmp_serde::from_slice(slice).unwrap_or_default()
//!     };
//!
//!     let name = args.first().map(|s| s.as_str()).unwrap_or("World");
//!     let result = ExecuteResult::success(format!("Hello, {}!", name));
//!     serialize_to_memory(&result)
//! }
//!
//! // Helper: Pack pointer and length into i64
//! fn pack_ptr_len(ptr: i32, len: i32) -> i64 {
//!     ((ptr as i64) << 32) | (len as i64 & 0xFFFFFFFF)
//! }
//!
//! // Helper: Serialize value to guest memory
//! fn serialize_to_memory<T: serde::Serialize>(value: &T) -> i64 {
//!     let bytes = rmp_serde::to_vec(value).expect("Serialization failed");
//!     let len = bytes.len() as i32;
//!     let ptr = plugin_alloc(len);
//!     if ptr == 0 { return 0; }
//!     unsafe {
//!         std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
//!     }
//!     pack_ptr_len(ptr, len)
//! }
//! ```
//!
//! # Best Practices
//!
//! ## Do
//!
//! - **Keep plugins focused**: One plugin, one responsibility
//! - **Validate inputs early**: Check arguments at the start of `execute()`
//! - **Return meaningful errors**: Include context in error messages
//! - **Use default values**: Make common cases convenient
//! - **Document your commands**: Use `.help()` on all arguments
//!
//! ## Don't
//!
//! - **Don't panic**: Always return `ExecuteResult::user_error` or `system_error`
//! - **Don't use unwrap()**: Prefer `unwrap_or`, `unwrap_or_default`, or match
//! - **Don't allocate excessively**: WASM has limited memory
//! - **Don't block forever**: The host has CPU limits (fuel)
//!
//! ## Example: Robust Argument Handling
//!
//! ```rust,ignore
//! fn execute(args: Vec<String>) -> ExecuteResult {
//!     // Validate required arguments
//!     let file = match args.get(0) {
//!         Some(f) if !f.is_empty() => f,
//!         _ => return ExecuteResult::user_error("Missing required argument: file"),
//!     };
//!
//!     // Parse optional numeric argument with default
//!     let count: usize = args.get(1)
//!         .and_then(|s| s.parse().ok())
//!         .unwrap_or(1);
//!
//!     // Validate value range
//!     if count == 0 || count > 100 {
//!         return ExecuteResult::user_error(
//!             "Count must be between 1 and 100"
//!         );
//!     }
//!
//!     ExecuteResult::success(format!("Processing {} {} time(s)", file, count))
//! }
//! ```
//!
//! # Troubleshooting
//!
//! ## Build Errors
//!
//! **Error: `can't find crate for std`**
//!
//! Make sure you're building for the correct target:
//! ```bash
//! cargo build --release --target wasm32-unknown-unknown
//! ```
//!
//! **Error: `crate-type must be cdylib`**
//!
//! Add to your `Cargo.toml`:
//! ```toml
//! [lib]
//! crate-type = ["cdylib"]
//! ```
//!
//! ## Runtime Errors
//!
//! **Error: `API version mismatch`**
//!
//! Your plugin was built with a different API version. Rebuild with the
//! matching `sen-plugin-sdk` version.
//!
//! **Error: `Function not found: plugin_manifest`**
//!
//! Make sure you have `export_plugin!(YourPlugin);` at the end of your lib.rs.
//!
//! **Error: `Fuel exhausted`**
//!
//! Your plugin is taking too long (possible infinite loop). The host limits
//! CPU usage to prevent runaway plugins.
//!
//! ## Debugging Tips
//!
//! 1. **Test locally first**: Write unit tests for your `execute()` logic
//! 2. **Check WASM size**: Large plugins may have unnecessary dependencies
//! 3. **Simplify arguments**: Start with positional args, add options later
//!
//! # Examples
//!
//! See the `examples/` directory for complete working plugins:
//!
//! - `examples/hello-plugin/`: Manual implementation (no SDK)
//! - `examples/greet-plugin/`: SDK-based with options

use std::alloc::{alloc, dealloc, Layout};

// Re-export everything from sen-plugin-api
pub use sen_plugin_api::*;

/// Prelude module for convenient imports
pub mod prelude {
    pub use crate::{export_plugin, memory, Plugin};
    pub use sen_plugin_api::{
        ArgSpec, CommandSpec, ExecuteError, ExecuteResult, PluginManifest, API_VERSION,
    };
}

/// Trait that plugins must implement
pub trait Plugin {
    /// Returns the plugin manifest describing the command
    fn manifest() -> PluginManifest;

    /// Executes the plugin with the given arguments
    fn execute(args: Vec<String>) -> ExecuteResult;
}

/// Memory utilities for Wasm plugin development
///
/// # Platform
/// These functions are designed for **WASM32 targets only**.
/// Pointer values are represented as `i32`, which is correct for WASM32's
/// 32-bit linear memory address space. Do not use on 64-bit native targets.
pub mod memory {
    use super::*;

    /// Allocate memory in the Wasm linear memory
    ///
    /// # Platform
    /// WASM32 only. Pointer is returned as `i32` (32-bit address).
    ///
    /// # Returns
    /// - Pointer to allocated memory as `i32`
    /// - `0` (null pointer) on allocation failure or invalid size
    ///
    /// # Safety
    /// This function is safe to call from the host.
    #[inline]
    pub fn plugin_alloc(size: i32) -> i32 {
        if size <= 0 {
            return 0;
        }
        // Safe: size > 0 is checked above, and positive i32 always fits in usize
        let size_usize = size as usize;
        let layout = match Layout::from_size_align(size_usize, 1) {
            Ok(l) => l,
            Err(_) => return 0, // Invalid layout, return null pointer
        };
        // SAFETY:
        // 1. Layout is valid (checked above with from_size_align)
        // 2. Layout has non-zero size (size > 0 checked above)
        // 3. The returned pointer will be properly aligned (alignment = 1)
        unsafe { alloc(layout) as i32 }
    }

    /// Deallocate memory in the Wasm linear memory
    ///
    /// # Safety
    /// The ptr must have been allocated by `plugin_alloc` with the same size.
    #[inline]
    pub fn plugin_dealloc(ptr: i32, size: i32) {
        if ptr == 0 || size <= 0 {
            return;
        }
        // Safe: size > 0 is checked above
        let size_usize = size as usize;
        let layout = match Layout::from_size_align(size_usize, 1) {
            Ok(l) => l,
            Err(_) => return, // Invalid layout, skip deallocation
        };
        // SAFETY:
        // 1. ptr was allocated by plugin_alloc with the same layout (caller's responsibility)
        // 2. ptr is non-null (checked above: ptr == 0 returns early)
        // 3. Layout matches the allocation (same size, alignment = 1)
        // 4. The memory block has not been deallocated yet (caller's responsibility)
        unsafe { dealloc(ptr as *mut u8, layout) }
    }

    /// Pack a pointer and length into a single i64 value
    ///
    /// This is the standard way to return two values from a Wasm function
    /// since wasm32-unknown-unknown doesn't support multi-value returns.
    #[inline]
    pub fn pack_ptr_len(ptr: i32, len: i32) -> i64 {
        ((ptr as i64) << 32) | (len as i64 & 0xFFFFFFFF)
    }

    /// Serialize data and return it as an allocated buffer
    ///
    /// Returns a packed i64 containing the pointer and length.
    /// Returns (0, 0) on serialization failure or if data exceeds i32::MAX bytes.
    ///
    /// Uses named serialization for compatibility with `skip_serializing_if` attributes.
    pub fn serialize_and_return<T: serde::Serialize>(data: &T) -> i64 {
        // Use to_vec_named for proper handling of optional/skipped fields
        let bytes = match rmp_serde::to_vec_named(data) {
            Ok(b) => b,
            Err(_) => return pack_ptr_len(0, 0),
        };

        // Check for integer overflow before casting
        let len: i32 = match bytes.len().try_into() {
            Ok(l) => l,
            Err(_) => return pack_ptr_len(0, 0), // Data too large for i32
        };

        let ptr = plugin_alloc(len);

        if ptr != 0 && len > 0 {
            // SAFETY:
            // 1. src (bytes.as_ptr()) is valid for reads of len bytes
            // 2. dst (ptr) is valid for writes of len bytes (allocated by plugin_alloc)
            // 3. Both pointers are properly aligned (alignment = 1 for u8)
            // 4. Memory regions do not overlap (src is stack/heap, dst is Wasm linear memory)
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, len as usize);
            }
        }

        pack_ptr_len(ptr, len)
    }

    /// Error type for deserialization failures
    #[derive(Debug)]
    pub enum DeserializeError {
        /// Null pointer or invalid length provided
        InvalidPointer { ptr: i32, len: i32 },
        /// MessagePack deserialization failed
        DeserializeFailed(rmp_serde::decode::Error),
    }

    impl std::fmt::Display for DeserializeError {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            match self {
                Self::InvalidPointer { ptr, len } => {
                    write!(f, "invalid pointer/length: ptr={}, len={}", ptr, len)
                }
                Self::DeserializeFailed(e) => write!(f, "deserialization failed: {}", e),
            }
        }
    }

    impl std::error::Error for DeserializeError {
        fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
            match self {
                Self::DeserializeFailed(e) => Some(e),
                _ => None,
            }
        }
    }

    /// Deserialize data from a raw pointer and length
    ///
    /// # Platform
    /// WASM32 only. Expects pointer as `i32` (32-bit address).
    ///
    /// # Errors
    /// - `InvalidPointer` if ptr is 0 or len <= 0
    /// - `DeserializeFailed` if MessagePack deserialization fails
    ///
    /// # Safety
    /// Caller must ensure:
    /// 1. `ptr` points to a valid memory region in Wasm linear memory
    /// 2. The memory region is at least `len` bytes
    /// 3. The memory contains valid MessagePack data
    /// 4. The memory will not be modified during deserialization
    pub unsafe fn deserialize_from_ptr<T: serde::de::DeserializeOwned>(
        ptr: i32,
        len: i32,
    ) -> Result<T, DeserializeError> {
        if ptr == 0 || len <= 0 {
            return Err(DeserializeError::InvalidPointer { ptr, len });
        }
        // SAFETY: Caller guarantees ptr is valid for len bytes (see function docs)
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        rmp_serde::from_slice(slice).map_err(DeserializeError::DeserializeFailed)
    }
}

/// Macro to export all required plugin functions
///
/// This macro generates the `plugin_manifest`, `plugin_execute`, `plugin_alloc`,
/// and `plugin_dealloc` functions required by the host.
///
/// # Example
///
/// ```rust,ignore
/// struct MyPlugin;
///
/// impl Plugin for MyPlugin {
///     fn manifest() -> PluginManifest { /* ... */ }
///     fn execute(args: Vec<String>) -> ExecuteResult { /* ... */ }
/// }
///
/// export_plugin!(MyPlugin);
/// ```
#[macro_export]
macro_rules! export_plugin {
    ($plugin:ty) => {
        #[no_mangle]
        pub extern "C" fn plugin_manifest() -> i64 {
            let manifest = <$plugin as $crate::Plugin>::manifest();
            $crate::memory::serialize_and_return(&manifest)
        }

        #[no_mangle]
        pub extern "C" fn plugin_execute(args_ptr: i32, args_len: i32) -> i64 {
            let args: Vec<String> = unsafe {
                match $crate::memory::deserialize_from_ptr(args_ptr, args_len) {
                    Ok(v) => v,
                    Err(_e) => {
                        // Return error result for invalid/corrupted arguments
                        let result =
                            $crate::ExecuteResult::system_error("Failed to deserialize arguments");
                        return $crate::memory::serialize_and_return(&result);
                    }
                }
            };
            let result = <$plugin as $crate::Plugin>::execute(args);
            $crate::memory::serialize_and_return(&result)
        }

        #[no_mangle]
        pub extern "C" fn plugin_alloc(size: i32) -> i32 {
            $crate::memory::plugin_alloc(size)
        }

        #[no_mangle]
        pub extern "C" fn plugin_dealloc(ptr: i32, size: i32) {
            $crate::memory::plugin_dealloc(ptr, size)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pack_ptr_len() {
        let ptr = 0x12345678_i32;
        let len = 0x00000100_i32;
        let packed = memory::pack_ptr_len(ptr, len);

        // Verify the packed value
        let unpacked_ptr = (packed >> 32) as i32;
        let unpacked_len = (packed & 0xFFFFFFFF) as i32;

        assert_eq!(unpacked_ptr, ptr);
        assert_eq!(unpacked_len, len);
    }

    #[test]
    fn test_alloc_edge_cases() {
        // Test zero/negative edge cases - these should return 0
        assert_eq!(memory::plugin_alloc(0), 0);
        assert_eq!(memory::plugin_alloc(-1), 0);
    }

    // Note: Full allocation tests run via integration tests with actual Wasm plugins.
    // The memory functions are designed for Wasm linear memory and may behave
    // differently in native test environments.
}
