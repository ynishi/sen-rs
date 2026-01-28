//! sen-plugin-sdk: SDK for creating Wasm plugins
//!
//! This SDK provides utilities and helpers for creating Wasm plugins
//! with minimal boilerplate.
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
//! // Generate all required exports
//! export_plugin!(MyPlugin);
//! ```

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
pub mod memory {
    use super::*;

    /// Allocate memory in the Wasm linear memory
    ///
    /// # Safety
    /// This function is safe to call from the host.
    #[inline]
    pub fn plugin_alloc(size: i32) -> i32 {
        if size <= 0 {
            return 0;
        }
        let layout = Layout::from_size_align(size as usize, 1).unwrap();
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
        let layout = Layout::from_size_align(size as usize, 1).unwrap();
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
    pub fn serialize_and_return<T: serde::Serialize>(data: &T) -> i64 {
        let bytes = rmp_serde::to_vec(data).unwrap_or_default();
        let len = bytes.len() as i32;
        let ptr = plugin_alloc(len);

        if ptr != 0 && len > 0 {
            unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, len as usize);
            }
        }

        pack_ptr_len(ptr, len)
    }

    /// Deserialize data from a raw pointer and length
    ///
    /// # Safety
    /// The pointer must be valid and point to `len` bytes of valid MessagePack data.
    pub unsafe fn deserialize_from_ptr<T: serde::de::DeserializeOwned>(
        ptr: i32,
        len: i32,
    ) -> Option<T> {
        if ptr == 0 || len <= 0 {
            return None;
        }
        let slice = std::slice::from_raw_parts(ptr as *const u8, len as usize);
        rmp_serde::from_slice(slice).ok()
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
                $crate::memory::deserialize_from_ptr(args_ptr, args_len).unwrap_or_default()
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
