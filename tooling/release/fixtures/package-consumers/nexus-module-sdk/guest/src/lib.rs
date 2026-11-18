//! wasm32-unknown-unknown guest for the packed `nexus-module-sdk` archive.
//!
//! Only `wasm32-unknown-unknown` is a valid target for this crate: the official
//! [`nexus_entry!`] macro supplies the three ABI exports, and the SDK is the
//! crate's only dependency, so no host-side code can ride along into the
//! artifact.

use nexus_module_sdk::{nexus_entry, ComputeInput, ComputeOutput, ModuleError};

/// Pass the module-declared invocation parameters back as the report: a guest
/// that needs nothing beyond the SDK.
fn echo_compute(input: ComputeInput) -> Result<ComputeOutput, ModuleError> {
    Ok(ComputeOutput {
        schema_version: input.schema_version,
        state_delta: Vec::new(),
        timeline_events: Vec::new(),
        new_key_blocks: Vec::new(),
        battle_report: input.invocation,
    })
}

nexus_entry!(echo_compute);
