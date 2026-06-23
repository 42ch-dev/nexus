//! Error types for the Nexus WASM compute host.

use thiserror::Error;

/// Errors returned by the WASM compute host.
#[derive(Debug, Error)]
pub enum ComputeError {
    /// The provided bytes are not a valid WebAssembly module.
    #[error("invalid wasm module: {0}")]
    InvalidModule(String),

    /// A required module export (e.g. `compute`, `alloc`, `memory`) is missing.
    #[error("module export missing: {0}")]
    MissingExport(String),

    /// The module's `compute` export returned a negative status code.
    #[error("module compute failed (status {0})")]
    ModuleComputeFailed(i64),

    /// The output buffer provided to the module was too small.
    #[error("module output buffer too small (needed at least {0} bytes)")]
    OutputBufferTooSmall(usize),

    /// The module exhausted its fuel budget before completing.
    #[error("module exhausted its fuel budget (out of fuel)")]
    OutOfFuel,

    /// The module exceeded the configured wall-time deadline.
    #[error("module exceeded the wall-time deadline")]
    WallTimeExceeded,

    /// The module exceeded its memory cap.
    #[error("module exceeded its memory cap")]
    MemoryCapExceeded,

    /// The module trapped for any other reason (out of bounds, divide by zero, …).
    #[error("module trapped: {0}")]
    Trap(String),

    /// The bytes returned by the module were not valid UTF-8 or valid JSON.
    #[error("module produced invalid output: {0}")]
    InvalidOutput(String),

    /// The host could not read/write the instance's linear memory.
    #[error("memory access error: {0}")]
    MemoryAccess(#[from] wasmtime::MemoryAccessError),

    /// The deserialized output did not match the `ComputeOutput` envelope.
    #[error("output envelope mismatch: {0}")]
    OutputSchemaMismatch(String),

    /// An internal wasmtime error (engine/store/instantiation failure).
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),

    /// A JSON (de)serialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// An I/O error from the embedded-module guard or similar.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
}

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, ComputeError>;
