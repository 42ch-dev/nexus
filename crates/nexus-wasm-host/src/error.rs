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

    /// Manifest-declared JSON-Schema validation failed (V1.62).
    /// `path` identifies the failing field (e.g. `key_blocks[1].body.attributes.base_atk`),
    /// `detail` describes the violation.
    #[error("manifest validation failed at {path}: {detail}")]
    ManifestValidationFailed { path: String, detail: String },

    /// Manifest-declared JSON-Schema validation failed for one or more
    /// `key_blocks` input entries (V1.147 P3 F2).
    ///
    /// All-or-nothing: ANY invalid entry fails the whole invocation — invalid
    /// entries are never silently skipped. Each failure carries the entry id
    /// (or a positional `key_blocks[i]` fallback label) plus the reason, so
    /// the caller can surface an honest, actionable per-entry error (HTTP 422
    /// `invalid_input` with `invalid_entries` detail) instead of a 500.
    #[error("input validation failed for {} entry(ies)", .0.len())]
    InputValidationFailed(Vec<EntryValidationFailure>),

    /// An internal wasmtime error (engine/store/instantiation failure).
    #[error("wasmtime error: {0}")]
    Wasmtime(#[from] wasmtime::Error),

    /// A JSON (de)serialization error.
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    /// An I/O error from the embedded-module guard or similar.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    /// Aggregated failures while warming the module cache at boot
    /// (R-V161P3-PERF-002). Individual module compile/parse errors are joined
    /// so a single bad module surfaces all problems at once without aborting
    /// the rest of the warmup.
    #[error("module cache warmup error: {0}")]
    CacheWarmup(String),
}

/// Result alias used across the crate.
pub type Result<T> = std::result::Result<T, ComputeError>;

/// One input `key_blocks` entry that failed manifest-schema validation
/// (V1.147 P3 F2 — per-entry failure detail).
///
/// `entry_id` is the spoke `KnowledgeEntry` id carried on the input entry;
/// entries without an id fall back to the positional `key_blocks[i]` label
/// (see `compute::kb_entry_id`).
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct EntryValidationFailure {
    pub entry_id: String,
    pub reason: String,
}
