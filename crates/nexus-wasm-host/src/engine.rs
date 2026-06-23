//! [`WasmEngine`] — owns the wasmtime [`Engine`] and compiles modules.
//!
//! The engine enables the sandbox *features* (fuel consumption + epoch
//! interruption) once at construction. The *amounts* (fuel budget, memory cap,
//! wall-time deadline) are applied per-invocation in [`compute`](crate::compute)
//! against a fresh [`Store`](wasmtime::Store), in line with the per-invocation
//! sandbox decision (compass Q6).

use wasmtime::{Config, Engine, Module};

use crate::error::{ComputeError, Result};
use crate::sandbox::SandboxConfig;

/// A WebAssembly module compiled by [`WasmEngine`].
///
/// Cheap to clone (wasmtime `Module` is internally `Arc`-shared). A compiled
/// module may be reused across many `compute()` calls; each call still gets a
/// fresh isolated instance.
#[derive(Clone, Debug)]
pub struct WasmModule {
    pub(crate) module: Module,
}

impl WasmModule {
    /// Returns the underlying wasmtime module (for advanced embedding).
    #[must_use]
    pub const fn as_wasmtime(&self) -> &Module {
        &self.module
    }
}

/// Owns the wasmtime engine and compiles compute modules.
///
/// Construct once (e.g. at daemon startup in P-last) and reuse for every
/// `compute()` call. The engine's [`Config`] enables fuel + epoch interruption;
/// actual limits are applied per call.
pub struct WasmEngine {
    pub(crate) engine: Engine,
    pub(crate) default_sandbox: SandboxConfig,
}

impl Default for WasmEngine {
    fn default() -> Self {
        Self::new().expect("default WasmEngine config must build")
    }
}

impl WasmEngine {
    /// Create an engine with the default sandbox configuration.
    ///
    /// # Errors
    /// Returns [`ComputeError::Wasmtime`] if wasmtime cannot initialize the
    /// engine (e.g. the JIT is unavailable on the host).
    pub fn new() -> Result<Self> {
        Self::with_config(SandboxConfig::default())
    }

    /// Create an engine with a custom default sandbox configuration. The
    /// `fuel` / `wall_time` / `max_memory_bytes` are defaults; per-call overrides
    /// via the module manifest take precedence in [`compute`](crate::compute).
    ///
    /// # Errors
    /// Returns [`ComputeError::Wasmtime`] on engine init failure.
    pub fn with_config(default_sandbox: SandboxConfig) -> Result<Self> {
        let mut config = Config::new();
        // Fuel: per-instruction budget, set per-invocation via Store::set_fuel.
        config.consume_fuel(true);
        // Epoch interruption: enables the wall-time watchdog to trap runaway
        // modules via Engine::increment_epoch().
        config.epoch_interruption(true);
        // Debug-info off for release-grade modules; speed over introspection.
        config.debug_info(false);

        let engine = Engine::new(&config)?;
        Ok(Self {
            engine,
            default_sandbox,
        })
    }

    /// The default sandbox limits applied when a manifest does not override.
    #[must_use]
    pub const fn default_sandbox(&self) -> SandboxConfig {
        self.default_sandbox
    }

    /// Borrow the underlying wasmtime [`Engine`].
    #[must_use]
    pub const fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Validate and compile a WebAssembly module from raw bytes.
    ///
    /// Compilation results are cached by wasmtime on disk (when enabled), so
    /// repeated loads of the same bytes are fast.
    ///
    /// # Errors
    /// Returns [`ComputeError::InvalidModule`] if the bytes are not valid wasm.
    pub fn load_module(&self, bytes: &[u8]) -> Result<WasmModule> {
        let module = Module::from_binary(&self.engine, bytes)
            .map_err(|e| ComputeError::InvalidModule(e.to_string()))?;
        Ok(WasmModule { module })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_valid_module() {
        let engine = WasmEngine::new().unwrap();
        // A minimal valid wasm module (empty, no exports).
        let empty = wat::parse_str("(module)").unwrap_or_default();
        engine.load_module(&empty).expect("empty module loads");
    }

    #[test]
    fn rejects_invalid_bytes() {
        let engine = WasmEngine::new().unwrap();
        let err = engine.load_module(b"definitely not wasm").unwrap_err();
        assert!(matches!(err, ComputeError::InvalidModule(_)));
    }
}
