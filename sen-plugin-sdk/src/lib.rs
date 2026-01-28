//! sen-plugin-sdk: SDK for creating Wasm plugins
//!
//! This SDK provides utilities and helpers for creating Wasm plugins
//! with minimal boilerplate.
//!
//! ## Setup
//!
//! Add to your `Cargo.toml`:
//!
//! ```toml
//! [lib]
//! crate-type = ["cdylib"]
//!
//! [dependencies]
//! sen-plugin-sdk = { path = "path/to/sen-plugin-sdk" }
//! ```
//!
//! ## Building
//!
//! Plugins must be compiled for `wasm32-unknown-unknown`:
//!
//! ```bash
//! # Add target (one-time)
//! rustup target add wasm32-unknown-unknown
//!
//! # Build plugin
//! cargo build --release --target wasm32-unknown-unknown
//!
//! # Output: target/wasm32-unknown-unknown/release/your_plugin.wasm
//! ```
//!
//! ## Quick Start
//!
//! ```rust,ignore
//! use sen_plugin_sdk::prelude::*;
//!
//! struct MyPlugin;
//!
//! impl Plugin for MyPlugin {
//!     fn manifest() -> PluginManifest {
//!         PluginManifest::new(
//!             CommandSpec::new("greet", "Greets a person")
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
//! // Generate all required WASM exports
//! export_plugin!(MyPlugin);
//! ```
//!
//! ## Error Handling
//!
//! ```rust,ignore
//! fn execute(args: Vec<String>) -> ExecuteResult {
//!     if args.is_empty() {
//!         return ExecuteResult::user_error("Missing required argument");
//!     }
//!     // ... process args
//!     ExecuteResult::success("Done!")
//! }
//! ```
//!
//! ## Examples
//!
//! See `examples/hello-plugin/` and `examples/greet-plugin/` for complete examples.

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
    pub fn serialize_and_return<T: serde::Serialize>(data: &T) -> i64 {
        let bytes = match rmp_serde::to_vec(data) {
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
