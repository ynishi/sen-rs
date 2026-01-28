//! Hello Plugin - A minimal sen-rs plugin example
//!
//! This plugin demonstrates the basic plugin interface:
//! - plugin_manifest: Returns command specification
//! - plugin_execute: Executes the command
//! - plugin_alloc/plugin_dealloc: Memory management

use sen_plugin_api::{ArgSpec, CommandSpec, ExecuteResult, PluginManifest, API_VERSION};
use std::alloc::{alloc, dealloc, Layout};

/// Allocate memory for host-guest communication
#[no_mangle]
pub extern "C" fn plugin_alloc(size: i32) -> i32 {
    if size <= 0 {
        return 0;
    }
    let layout = Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { alloc(layout) as i32 }
}

/// Deallocate memory
#[no_mangle]
pub extern "C" fn plugin_dealloc(ptr: i32, size: i32) {
    if ptr == 0 || size <= 0 {
        return;
    }
    let layout = Layout::from_size_align(size as usize, 1).unwrap();
    unsafe { dealloc(ptr as *mut u8, layout) }
}

/// Return plugin manifest (command specification)
/// Returns packed i64: (ptr << 32) | len
#[no_mangle]
pub extern "C" fn plugin_manifest() -> i64 {
    let manifest = PluginManifest {
        api_version: API_VERSION,
        command: CommandSpec::new("hello", "Says hello to the world")
            .version("1.0.0")
            .arg(
                ArgSpec::positional("name")
                    .help("Name to greet")
                    .default("World"),
            ),
    };

    serialize_to_memory(&manifest)
}

/// Execute the plugin command
/// Returns packed i64: (ptr << 32) | len
#[no_mangle]
pub extern "C" fn plugin_execute(args_ptr: i32, args_len: i32) -> i64 {
    // Deserialize arguments from host
    let args: Vec<String> = unsafe {
        let slice = std::slice::from_raw_parts(args_ptr as *const u8, args_len as usize);
        match rmp_serde::from_slice(slice) {
            Ok(a) => a,
            Err(e) => {
                return serialize_to_memory(&ExecuteResult::system_error(format!(
                    "Failed to deserialize args: {}",
                    e
                )));
            }
        }
    };

    // Get name from args (first positional argument)
    let name = args.first().map(|s| s.as_str()).unwrap_or("World");

    // Return greeting
    let result = ExecuteResult::success(format!("Hello, {}!", name));
    serialize_to_memory(&result)
}

/// Helper: Serialize a value to guest memory and return packed (ptr, len)
fn serialize_to_memory<T: serde::Serialize>(value: &T) -> i64 {
    let bytes = rmp_serde::to_vec(value).expect("Serialization failed");
    let len = bytes.len() as i32;
    let ptr = plugin_alloc(len);

    if ptr == 0 {
        return 0;
    }

    unsafe {
        std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr as *mut u8, bytes.len());
    }

    pack_ptr_len(ptr, len)
}

/// Pack ptr and len into a single i64
#[inline]
fn pack_ptr_len(ptr: i32, len: i32) -> i64 {
    ((ptr as i64) << 32) | (len as i64 & 0xFFFFFFFF)
}
