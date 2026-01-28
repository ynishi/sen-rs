//! Plugin loader using wasmtime
//!
//! Loads Wasm plugins and provides safe execution with sandboxing.

use sen_plugin_api::{ExecuteResult, PluginManifest, API_VERSION};
use thiserror::Error;
use wasmtime::*;

/// Errors that can occur during plugin loading
#[derive(Debug, Error)]
pub enum LoaderError {
    #[error("Engine creation failed: {0}")]
    EngineCreation(#[source] anyhow::Error),

    #[error("Module compilation failed: {0}")]
    ModuleCompilation(#[source] anyhow::Error),

    #[error("Instantiation failed: {0}")]
    Instantiation(#[source] anyhow::Error),

    #[error("Function not found: {0}")]
    FunctionNotFound(String),

    #[error("Function call failed: {function} - {source}")]
    FunctionCall {
        function: &'static str,
        #[source]
        source: anyhow::Error,
    },

    #[error("API version mismatch: expected {expected}, got {actual}")]
    ApiVersionMismatch { expected: u32, actual: u32 },

    #[error("Deserialization failed: {0}")]
    Deserialization(#[source] rmp_serde::decode::Error),

    #[error("Memory access error: {0}")]
    MemoryAccess(String),

    #[error("Fuel exhausted (CPU limit exceeded)")]
    FuelExhausted,
}

/// Plugin loader with wasmtime engine
pub struct PluginLoader {
    engine: Engine,
}

/// A loaded plugin ready for execution
pub struct LoadedPlugin {
    /// Plugin manifest with command specification
    pub manifest: PluginManifest,

    /// Plugin instance for execution
    pub instance: PluginInstance,
}

/// Plugin instance that can execute commands
pub struct PluginInstance {
    store: Store<()>,
    instance: Instance,
    memory: Memory,
    alloc_fn: TypedFunc<i32, i32>,
    dealloc_fn: TypedFunc<(i32, i32), ()>,
}

/// Unpack ptr and len from a packed i64
#[inline]
fn unpack_ptr_len(packed: i64) -> (i32, i32) {
    let ptr = (packed >> 32) as i32;
    let len = (packed & 0xFFFFFFFF) as i32;
    (ptr, len)
}

impl PluginLoader {
    /// Create a new plugin loader
    pub fn new() -> Result<Self, LoaderError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.wasm_memory64(false);

        let engine = Engine::new(&config).map_err(LoaderError::EngineCreation)?;

        Ok(Self { engine })
    }

    /// Load a plugin from Wasm bytes
    pub fn load(&self, wasm_bytes: &[u8]) -> Result<LoadedPlugin, LoaderError> {
        // 1. Compile module
        let module =
            Module::new(&self.engine, wasm_bytes).map_err(LoaderError::ModuleCompilation)?;

        // 2. Create store with fuel limit (no WASI for MVP)
        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(10_000_000)
            .map_err(LoaderError::EngineCreation)?;

        // 3. Create linker (empty for now, no WASI imports)
        let linker = Linker::new(&self.engine);

        // 4. Instantiate
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(LoaderError::Instantiation)?;

        // 5. Get memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| LoaderError::FunctionNotFound("memory".to_string()))?;

        // 6. Get allocator functions
        let alloc_fn = instance
            .get_typed_func::<i32, i32>(&mut store, "plugin_alloc")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_alloc".to_string()))?;

        let dealloc_fn = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "plugin_dealloc")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_dealloc".to_string()))?;

        // 7. Call manifest function (returns packed i64)
        let manifest_fn = instance
            .get_typed_func::<(), i64>(&mut store, "plugin_manifest")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_manifest".to_string()))?;

        let packed = manifest_fn.call(&mut store, ()).map_err(|e| {
            if e.to_string().contains("fuel") {
                LoaderError::FuelExhausted
            } else {
                LoaderError::FunctionCall {
                    function: "plugin_manifest",
                    source: e,
                }
            }
        })?;

        let (ptr, len) = unpack_ptr_len(packed);

        // 8. Read manifest from memory
        let manifest_bytes = Self::read_memory(&store, &memory, ptr as usize, len as usize)?;
        let manifest: PluginManifest =
            rmp_serde::from_slice(&manifest_bytes).map_err(LoaderError::Deserialization)?;

        // 9. Validate API version
        if manifest.api_version != API_VERSION {
            return Err(LoaderError::ApiVersionMismatch {
                expected: API_VERSION,
                actual: manifest.api_version,
            });
        }

        // 10. Deallocate manifest memory
        dealloc_fn
            .call(&mut store, (ptr, len))
            .map_err(|e| LoaderError::FunctionCall {
                function: "plugin_dealloc",
                source: e,
            })?;

        Ok(LoadedPlugin {
            manifest,
            instance: PluginInstance {
                store,
                instance,
                memory,
                alloc_fn,
                dealloc_fn,
            },
        })
    }

    fn read_memory(
        store: &Store<()>,
        memory: &Memory,
        ptr: usize,
        len: usize,
    ) -> Result<Vec<u8>, LoaderError> {
        let data = memory.data(store);
        if ptr + len > data.len() {
            return Err(LoaderError::MemoryAccess(format!(
                "Out of bounds: ptr={}, len={}, memory_size={}",
                ptr,
                len,
                data.len()
            )));
        }
        Ok(data[ptr..ptr + len].to_vec())
    }
}

impl Default for PluginLoader {
    fn default() -> Self {
        Self::new().expect("Failed to create PluginLoader")
    }
}

impl PluginInstance {
    /// Execute the plugin with given arguments
    pub fn execute(&mut self, args: &[String]) -> Result<ExecuteResult, LoaderError> {
        // 1. Serialize arguments
        let args_bytes = rmp_serde::to_vec(args)
            .map_err(|e| LoaderError::MemoryAccess(format!("Failed to serialize args: {}", e)))?;

        // 2. Allocate memory in guest
        let args_len = args_bytes.len() as i32;
        let args_ptr = self.alloc_fn.call(&mut self.store, args_len).map_err(|e| {
            LoaderError::FunctionCall {
                function: "plugin_alloc",
                source: e,
            }
        })?;

        // 3. Write args to guest memory
        self.memory
            .write(&mut self.store, args_ptr as usize, &args_bytes)
            .map_err(|e| LoaderError::MemoryAccess(format!("Failed to write args: {}", e)))?;

        // 4. Call execute function (returns packed i64)
        let execute_fn = self
            .instance
            .get_typed_func::<(i32, i32), i64>(&mut self.store, "plugin_execute")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_execute".to_string()))?;

        // Reset fuel for execution
        self.store
            .set_fuel(10_000_000)
            .map_err(LoaderError::EngineCreation)?;

        let packed = execute_fn
            .call(&mut self.store, (args_ptr, args_len))
            .map_err(|e| {
                if e.to_string().contains("fuel") {
                    LoaderError::FuelExhausted
                } else {
                    LoaderError::FunctionCall {
                        function: "plugin_execute",
                        source: e,
                    }
                }
            })?;

        let (result_ptr, result_len) = unpack_ptr_len(packed);

        // 5. Read result from memory
        let result_bytes = PluginLoader::read_memory(
            &self.store,
            &self.memory,
            result_ptr as usize,
            result_len as usize,
        )?;

        let result: ExecuteResult =
            rmp_serde::from_slice(&result_bytes).map_err(LoaderError::Deserialization)?;

        // 6. Deallocate args and result memory
        self.dealloc_fn
            .call(&mut self.store, (args_ptr, args_len))
            .ok();
        self.dealloc_fn
            .call(&mut self.store, (result_ptr, result_len))
            .ok();

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_loader_creation() {
        let loader = PluginLoader::new();
        assert!(loader.is_ok());
    }

    #[test]
    fn test_pack_unpack() {
        let ptr = 0x12345678_i32;
        let len = 0x00000100_i32;
        let packed = ((ptr as i64) << 32) | (len as i64 & 0xFFFFFFFF);
        let (up, ul) = unpack_ptr_len(packed);
        assert_eq!(up, ptr);
        assert_eq!(ul, len);
    }
}
