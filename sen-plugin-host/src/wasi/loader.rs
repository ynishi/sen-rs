//! WASI-enabled plugin loader
//!
//! This module provides a plugin loader that grants WASI capabilities to plugins,
//! allowing them to access filesystem, environment variables, and stdio based on
//! their declared [`Capabilities`].
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────────┐
//! │                       WasiPluginLoader Flow                                 │
//! ├─────────────────────────────────────────────────────────────────────────────┤
//! │                                                                             │
//! │  1. Load Plugin                                                             │
//! │     ├─ Compile WASM module                                                  │
//! │     ├─ Call plugin_manifest() to get capabilities                           │
//! │     └─ Validate API version                                                 │
//! │                                                                             │
//! │  2. Execute (with WASI)                                                     │
//! │     ├─ Build WasiSpec from capabilities                                     │
//! │     ├─ Create WasiCtx with configured permissions                           │
//! │     ├─ Create Store with WasiCtx                                            │
//! │     ├─ Link WASI imports                                                    │
//! │     ├─ Call plugin_execute()                                                │
//! │     └─ Plugin can now access granted resources                              │
//! │                                                                             │
//! └─────────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Differences from Regular Loader
//!
//! | Feature | `PluginLoader` | `WasiPluginLoader` |
//! |---------|----------------|-------------------|
//! | Filesystem access | Denied | Based on capabilities |
//! | Environment vars | Denied | Based on capabilities |
//! | Stdio | Denied | Based on capabilities |
//! | Store state | `()` | `WasiState` |
//! | WASI imports | None | Full WASI Preview 1 |
//!
//! # Example
//!
//! ```rust,ignore
//! use sen_plugin_host::wasi::{WasiPluginLoader, WasiLoaderConfig};
//!
//! let loader = WasiPluginLoader::new(WasiLoaderConfig {
//!     working_directory: std::env::current_dir()?,
//!     ..Default::default()
//! })?;
//!
//! let wasm_bytes = std::fs::read("plugin.wasm")?;
//! let mut plugin = loader.load(&wasm_bytes)?;
//!
//! // Plugin can now access declared capabilities
//! let result = plugin.instance.execute(&["arg1".into()])?;
//! ```
//!
//! # Security
//!
//! Even with WASI enabled, plugins are still sandboxed:
//!
//! - **Filesystem**: Only declared paths are accessible
//! - **Environment**: Only declared variables are visible
//! - **Network**: Not available (WASI Preview 1 limitation)
//! - **CPU**: Still limited by fuel (10M instructions)
//! - **Stack**: Still limited (1MB)

use super::context::{WasiConfigurer, WasiSpec};
use super::error::WasiError;
use crate::loader::LoaderError;
use sen_plugin_api::{Capabilities, ExecuteResult, PluginManifest, API_VERSION};
use std::path::PathBuf;
use wasmtime::*;
use wasmtime_wasi::preview1::WasiP1Ctx;
use wasmtime_wasi::WasiCtxBuilder;

/// Configuration for WASI plugin loader
#[derive(Debug, Clone)]
pub struct WasiLoaderConfig {
    /// Working directory for path resolution
    pub working_directory: PathBuf,

    /// Whether to follow symlinks during path validation
    pub follow_symlinks: bool,

    /// Whether paths must exist before execution
    pub require_existence: bool,

    /// Fuel limit per execution (CPU limit)
    pub fuel_limit: u64,

    /// Maximum WASM stack size in bytes
    pub max_stack_size: usize,
}

impl Default for WasiLoaderConfig {
    fn default() -> Self {
        Self {
            working_directory: std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")),
            follow_symlinks: true,
            require_existence: true,
            fuel_limit: 10_000_000,
            max_stack_size: 1024 * 1024, // 1MB
        }
    }
}

/// State held by the WASI store
///
/// This struct contains the WASI Preview 1 context needed for plugin execution.
pub struct WasiState {
    /// WASI Preview 1 context with configured capabilities
    pub wasi: WasiP1Ctx,
}

impl WasiState {
    /// Create a new WASI state from a specification
    pub fn from_spec(spec: WasiSpec) -> Result<Self, WasiError> {
        let wasi = spec.build_p1_ctx()?;
        Ok(Self { wasi })
    }

    /// Create an empty WASI state (no capabilities)
    pub fn empty() -> Self {
        let wasi = WasiCtxBuilder::new().build_p1();
        Self { wasi }
    }
}

/// WASI-enabled plugin loader
///
/// This loader creates plugins that can access system resources
/// based on their declared capabilities.
pub struct WasiPluginLoader {
    engine: Engine,
    config: WasiLoaderConfig,
}

/// A loaded plugin with WASI capabilities
pub struct WasiLoadedPlugin {
    /// Plugin manifest with command specification and capabilities
    pub manifest: PluginManifest,

    /// Plugin instance for execution
    pub instance: WasiPluginInstance,
}

/// Plugin instance that executes with WASI capabilities
pub struct WasiPluginInstance {
    engine: Engine,
    module: Module,
    config: WasiLoaderConfig,
    capabilities: Capabilities,
}

/// Unpack ptr and len from a packed i64
#[inline]
fn unpack_ptr_len(packed: i64) -> (i32, i32) {
    let ptr = (packed >> 32) as i32;
    let len = (packed & 0xFFFFFFFF) as i32;
    (ptr, len)
}

impl WasiPluginLoader {
    /// Create a new WASI plugin loader
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let loader = WasiPluginLoader::new(WasiLoaderConfig {
    ///     working_directory: std::env::current_dir()?,
    ///     ..Default::default()
    /// })?;
    /// ```
    pub fn new(config: WasiLoaderConfig) -> Result<Self, LoaderError> {
        let mut engine_config = Config::new();

        // Security: Enable fuel for CPU limiting
        engine_config.consume_fuel(true);

        // Security: Limit WASM stack size
        engine_config.max_wasm_stack(config.max_stack_size);

        // Disable memory64 for wasm32 compatibility
        engine_config.wasm_memory64(false);

        let engine = Engine::new(&engine_config).map_err(LoaderError::EngineCreation)?;

        Ok(Self { engine, config })
    }

    /// Create a loader with default configuration
    pub fn with_working_directory(working_directory: PathBuf) -> Result<Self, LoaderError> {
        Self::new(WasiLoaderConfig {
            working_directory,
            ..Default::default()
        })
    }

    /// Load a plugin from WASM bytes
    ///
    /// This compiles the module and extracts the manifest, but does not
    /// yet create a WASI context. The context is created at execution time
    /// based on the plugin's declared capabilities.
    pub fn load(&self, wasm_bytes: &[u8]) -> Result<WasiLoadedPlugin, LoaderError> {
        // 1. Compile module
        let module =
            Module::new(&self.engine, wasm_bytes).map_err(LoaderError::ModuleCompilation)?;

        // 2. Create temporary store to read manifest (no WASI needed for this)
        let mut store = Store::new(&self.engine, WasiState::empty());
        store
            .set_fuel(self.config.fuel_limit)
            .map_err(|e| LoaderError::StoreConfig(format!("Failed to set fuel: {}", e)))?;

        // 3. Create linker with WASI Preview 1 imports
        let mut linker: Linker<WasiState> = Linker::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |state| &mut state.wasi).map_err(
            |e| LoaderError::Instantiation(anyhow::anyhow!("Failed to add WASI to linker: {}", e)),
        )?;

        // 4. Instantiate to read manifest
        let instance = linker
            .instantiate(&mut store, &module)
            .map_err(LoaderError::Instantiation)?;

        // 5. Get memory
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| LoaderError::FunctionNotFound("memory".to_string()))?;

        // 6. Get allocator functions
        let dealloc_fn = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "plugin_dealloc")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_dealloc".to_string()))?;

        // 7. Call manifest function
        let manifest_fn = instance
            .get_typed_func::<(), i64>(&mut store, "plugin_manifest")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_manifest".to_string()))?;

        let packed = manifest_fn.call(&mut store, ()).map_err(|e| {
            if e.downcast_ref::<Trap>()
                .is_some_and(|t| *t == Trap::OutOfFuel)
            {
                LoaderError::FuelExhausted
            } else {
                LoaderError::FunctionCall {
                    function: "plugin_manifest",
                    source: e,
                }
            }
        })?;

        let (ptr, len) = unpack_ptr_len(packed);

        if ptr < 0 || len < 0 {
            return Err(LoaderError::MemoryAccess(format!(
                "Invalid manifest pointer/length: ptr={}, len={}",
                ptr, len
            )));
        }

        // 8. Read manifest from memory
        let manifest_bytes = read_memory(&store, &memory, ptr as usize, len as usize)?;
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
        if let Err(e) = dealloc_fn.call(&mut store, (ptr, len)) {
            tracing::warn!(error = %e, "Failed to deallocate manifest memory");
        }

        let capabilities = manifest.capabilities.clone();

        Ok(WasiLoadedPlugin {
            manifest,
            instance: WasiPluginInstance {
                engine: self.engine.clone(),
                module,
                config: self.config.clone(),
                capabilities,
            },
        })
    }
}

impl WasiPluginInstance {
    /// Execute the plugin with given arguments
    ///
    /// This creates a fresh WASI context for each execution, configured
    /// with the plugin's declared capabilities.
    pub fn execute(&self, args: &[String]) -> Result<ExecuteResult, LoaderError> {
        // 1. Build WASI spec from capabilities
        let spec = WasiConfigurer::new()
            .with_capabilities(&self.capabilities)
            .with_working_directory(self.config.working_directory.clone())
            .with_args(args.to_vec())
            .follow_symlinks(self.config.follow_symlinks)
            .require_existence(self.config.require_existence)
            .build()
            .map_err(|e| LoaderError::StoreConfig(format!("WASI configuration failed: {}", e)))?;

        tracing::debug!(
            capabilities = %spec.permission_summary(),
            "Executing plugin with WASI capabilities"
        );

        // 2. Create WASI state
        let wasi_state = WasiState::from_spec(spec).map_err(|e| {
            LoaderError::StoreConfig(format!("WASI context creation failed: {}", e))
        })?;

        // 3. Create store with WASI state
        let mut store = Store::new(&self.engine, wasi_state);
        store
            .set_fuel(self.config.fuel_limit)
            .map_err(|e| LoaderError::StoreConfig(format!("Failed to set fuel: {}", e)))?;

        // 4. Create linker with WASI Preview 1 imports
        let mut linker: Linker<WasiState> = Linker::new(&self.engine);
        wasmtime_wasi::preview1::add_to_linker_sync(&mut linker, |state| &mut state.wasi).map_err(
            |e| LoaderError::Instantiation(anyhow::anyhow!("Failed to add WASI to linker: {}", e)),
        )?;

        // 5. Instantiate module
        let instance = linker
            .instantiate(&mut store, &self.module)
            .map_err(LoaderError::Instantiation)?;

        // 6. Get required functions
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| LoaderError::FunctionNotFound("memory".to_string()))?;

        let alloc_fn = instance
            .get_typed_func::<i32, i32>(&mut store, "plugin_alloc")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_alloc".to_string()))?;

        let dealloc_fn = instance
            .get_typed_func::<(i32, i32), ()>(&mut store, "plugin_dealloc")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_dealloc".to_string()))?;

        let execute_fn = instance
            .get_typed_func::<(i32, i32), i64>(&mut store, "plugin_execute")
            .map_err(|_| LoaderError::FunctionNotFound("plugin_execute".to_string()))?;

        // 7. Serialize arguments
        let args_bytes = rmp_serde::to_vec(args)
            .map_err(|e| LoaderError::MemoryAccess(format!("Failed to serialize args: {}", e)))?;

        let args_len: i32 = args_bytes.len().try_into().map_err(|_| {
            LoaderError::MemoryAccess(format!(
                "Arguments too large: {} bytes exceeds i32::MAX",
                args_bytes.len()
            ))
        })?;

        // 8. Allocate and write args
        let args_ptr =
            alloc_fn
                .call(&mut store, args_len)
                .map_err(|e| LoaderError::FunctionCall {
                    function: "plugin_alloc",
                    source: e,
                })?;

        memory
            .write(&mut store, args_ptr as usize, &args_bytes)
            .map_err(|e| LoaderError::MemoryAccess(format!("Failed to write args: {}", e)))?;

        // 9. Execute
        let packed = execute_fn
            .call(&mut store, (args_ptr, args_len))
            .map_err(|e| {
                if e.downcast_ref::<Trap>()
                    .is_some_and(|t| *t == Trap::OutOfFuel)
                {
                    LoaderError::FuelExhausted
                } else {
                    LoaderError::FunctionCall {
                        function: "plugin_execute",
                        source: e,
                    }
                }
            })?;

        let (result_ptr, result_len) = unpack_ptr_len(packed);

        if result_ptr < 0 || result_len < 0 {
            return Err(LoaderError::MemoryAccess(format!(
                "Invalid result pointer/length: ptr={}, len={}",
                result_ptr, result_len
            )));
        }

        // 10. Read result
        let result_bytes = read_memory(&store, &memory, result_ptr as usize, result_len as usize)?;
        let result: ExecuteResult =
            rmp_serde::from_slice(&result_bytes).map_err(LoaderError::Deserialization)?;

        // 11. Cleanup
        if let Err(e) = dealloc_fn.call(&mut store, (args_ptr, args_len)) {
            tracing::warn!(error = %e, "Failed to deallocate args memory");
        }
        if let Err(e) = dealloc_fn.call(&mut store, (result_ptr, result_len)) {
            tracing::warn!(error = %e, "Failed to deallocate result memory");
        }

        Ok(result)
    }

    /// Get the capabilities declared by this plugin
    pub fn capabilities(&self) -> &Capabilities {
        &self.capabilities
    }
}

fn read_memory(
    store: &Store<WasiState>,
    memory: &Memory,
    ptr: usize,
    len: usize,
) -> Result<Vec<u8>, LoaderError> {
    let data = memory.data(store);
    let end = ptr.checked_add(len).ok_or_else(|| {
        LoaderError::MemoryAccess(format!("Integer overflow: ptr={}, len={}", ptr, len))
    })?;
    if end > data.len() {
        return Err(LoaderError::MemoryAccess(format!(
            "Out of bounds: ptr={}, len={}, memory_size={}",
            ptr,
            len,
            data.len()
        )));
    }
    Ok(data[ptr..end].to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wasi_loader_creation() {
        let loader = WasiPluginLoader::new(WasiLoaderConfig::default());
        assert!(loader.is_ok());
    }

    #[test]
    fn test_wasi_state_empty() {
        let state = WasiState::empty();
        // Just verify it doesn't panic
        assert!(true);
    }
}
