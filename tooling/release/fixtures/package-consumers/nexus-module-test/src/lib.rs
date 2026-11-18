//! Module side of the mini-host fixture.
//!
//! This is the dependency shape a real compute-module crate has: `nexus-module-sdk` is the
//! dependency the module is built against, and `nexus-module-test` is a host-target
//! **dev-dependency**. Only a test target sees a dev-dependency, so the ABI-conformance assertions
//! live in `tests/abi.rs` and the checker exercises this fixture with `cargo test`.

use nexus_module_sdk::{ComputeInput, ComputeOutput, ModuleError};

/// The module's compute entry: pass the invocation parameters back as the report.
///
/// # Errors
///
/// Never fails in this fixture; the signature is the ABI's.
pub fn echo_compute(input: ComputeInput) -> Result<ComputeOutput, ModuleError> {
    Ok(ComputeOutput {
        schema_version: input.schema_version,
        state_delta: Vec::new(),
        timeline_events: Vec::new(),
        new_key_blocks: Vec::new(),
        battle_report: input.invocation,
    })
}
