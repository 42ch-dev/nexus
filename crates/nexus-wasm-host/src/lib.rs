//! `nexus-wasm-host` — sandboxed WASM compute host for Nexus (V1.61).
//!
//! Hosts **WASM compute modules** in a sandboxed [`wasmtime`] runtime. Compute
//! modules are stateless pure functions: they receive a [`ComputeInput`]
//! envelope, run inside a per-invocation sandboxed instance, and return a
//! 4-part [`ComputeOutput`] envelope (`state_delta`, `timeline_events`,
//! `new_key_blocks`, `battle_report`).
//!
//! See [`AGENTS.md`](../AGENTS.md) for the full design (compass grill decisions
//! Q1/Q6/Q8, module ABI, host-function whitelist).
//!
//! # Example
//!
//! ```no_run
//! use nexus_wasm_host::{
//!     embedded_module_bytes, embedded_module_manifest, ComputeInput,
//!     ModuleManifest, WasmEngine,
//! };
//!
//! # fn main() -> nexus_wasm_host::Result<()> {
//! let engine = WasmEngine::new()?;
//! let wasm = embedded_module_bytes("basic-combat").expect("embedded");
//! let module = engine.load_module(wasm)?;
//! let manifest: ModuleManifest =
//!     serde_json::from_str(embedded_module_manifest("basic-combat").unwrap())?;
//! let input: ComputeInput = /* ... */ serde_json::from_str("{}")?;
//! let output = engine.compute(&module, &manifest, &input)?;
//! println!("{}", serde_json::to_string_pretty(&output)?);
//! # Ok(())
//! # }
//! ```

mod compute;
mod embedded;
mod engine;
mod error;
mod host;
mod manifest;
mod sandbox;

pub use embedded::{embedded_module_bytes, embedded_module_ids, embedded_module_manifest};
pub use engine::{WasmEngine, WasmModule};
pub use error::{ComputeError, Result};
pub use host::HostContext;
pub use manifest::{HostFunction, ModuleManifest};
pub use sandbox::SandboxConfig;

// Re-export the compute ABI types from nexus-contracts so consumers depend on a
// single crate. These are the generated wire types for `schemas/compute/`.
pub use nexus_contracts::generated::compute_input::ComputeInput;
pub use nexus_contracts::generated::compute_output::{ComputeOutput, ComputeOutputStateDelta};
