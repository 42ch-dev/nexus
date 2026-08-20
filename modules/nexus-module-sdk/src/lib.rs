//! Official SDK for Nexus compute modules (V1.170 P0, AR-1..AR-6).
//!
//! A compute module is a **stateless pure function**: it receives a
//! [`ComputeInput`] envelope from the host and returns a 4-part
//! [`ComputeOutput`] envelope. The SDK owns every ABI-facing symbol — module
//! authors write **zero** `#[no_mangle]` code:
//!
//! ```ignore
//! use nexus_module_sdk::{nexus_entry, ComputeInput, ComputeOutput, ModuleError};
//!
//! fn my_compute(input: ComputeInput) -> Result<ComputeOutput, ModuleError> {
//!     // ... module logic ...
//!     # let _ = input;
//!     # unimplemented!()
//! }
//!
//! nexus_entry!(my_compute);
//! ```
//!
//! [`nexus_entry!`] expands to the three ABI exports (`alloc`, `init`,
//! `compute`); the global allocator is wired automatically on
//! `wasm32-unknown-unknown` (dlmalloc). The typed envelope skeleton
//! ([`types`]), key-block accessors ([`key_blocks`]), host-import wrappers
//! ([`host`]), error sentinel mapping ([`error`]), and the manifest helper
//! ([`manifest`]) cover the full V1 ABI surface.
//!
//! The SDK compiles for host targets (its own `cargo test` runs in CI) and
//! for `wasm32-unknown-unknown` (consumed by module cdylibs). Only final
//! module crates declare `crate-type = ["cdylib"]`.

pub mod error;
pub mod host;
pub mod key_blocks;
pub mod manifest;
pub mod shim;
pub mod types;

pub use error::{write_output, ModuleError};
pub use host::HostError;
pub use manifest::{
    HostFunction, ModuleManifest, ModuleSchemas, DEFAULT_FUEL, DEFAULT_MEMORY_MIB,
    DEFAULT_WALL_TIME_MS,
};
pub use types::{ComputeInput, ComputeOutput, DeltaOp, StateDeltaOp, WorldRef};

// Global allocator for wasm32-unknown-unknown (std provides none on this
// target). dlmalloc grows linear memory on demand; the host's memory cap
// bounds it. Host-target builds keep std's allocator so SDK unit tests run
// on the host.
#[cfg(target_arch = "wasm32")]
#[global_allocator]
static A: dlmalloc::GlobalDlmalloc = dlmalloc::GlobalDlmalloc;

/// User-facing entry point for a compute module — a trait, not a hardcoded
/// signature.
///
/// V1's only entry form is a plain function
/// `fn(ComputeInput) -> Result<ComputeOutput, ModuleError>` (see the blanket
/// impl below). The trait exists so ABI V2 can add optional methods / a
/// context argument **additively** (ABI §9.1, DR-49 posture) without breaking
/// V1 modules.
pub trait NexusModule {
    /// Resolve one compute invocation.
    fn compute(&self, input: ComputeInput) -> Result<ComputeOutput, ModuleError>;

    /// One-shot initialization. No-op by default; called once after
    /// instantiation when the manifest declares `init_export`.
    fn init(&mut self) {}
}

/// Blanket impl: any `fn(ComputeInput) -> Result<ComputeOutput, ModuleError>`
/// is a [`NexusModule`] (V1's only entry form).
impl<F> NexusModule for F
where
    F: Fn(ComputeInput) -> Result<ComputeOutput, ModuleError>,
{
    fn compute(&self, input: ComputeInput) -> Result<ComputeOutput, ModuleError> {
        self(input)
    }
}

/// Generate the three ABI exports for a module (AR-2).
///
/// Invocation:
///
/// ```ignore
/// nexus_module_sdk::nexus_entry!(my_compute); // my_compute: fn(ComputeInput) -> Result<ComputeOutput, ModuleError>
/// ```
///
/// expands to:
///
/// ```ignore
/// #[no_mangle] pub extern "C" fn alloc(len: u32) -> u32 { nexus_module_sdk::shim::alloc(len) }
/// #[no_mangle] pub extern "C" fn init() { nexus_module_sdk::shim::init() }
/// #[no_mangle] pub extern "C" fn compute(in_ptr: u32, in_len: u32, out_ptr: u32, out_cap: u32) -> i64 {
///     nexus_module_sdk::shim::compute(in_ptr, in_len, out_ptr, out_cap, my_compute)
/// }
/// ```
///
/// `shim::alloc` keeps the intentional-leak semantics (fresh per-invocation
/// instance; the `Vec` is leaked, never freed).
#[macro_export]
macro_rules! nexus_entry {
    ($compute:ident) => {
        #[no_mangle]
        pub extern "C" fn alloc(len: u32) -> u32 {
            $crate::shim::alloc(len)
        }

        #[no_mangle]
        pub extern "C" fn init() {
            $crate::shim::init()
        }

        #[no_mangle]
        pub extern "C" fn compute(in_ptr: u32, in_len: u32, out_ptr: u32, out_cap: u32) -> i64 {
            $crate::shim::compute(in_ptr, in_len, out_ptr, out_cap, $compute)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// A minimal module entry used by the macro + trait tests.
    fn echo_compute(input: ComputeInput) -> Result<ComputeOutput, ModuleError> {
        Ok(ComputeOutput {
            schema_version: input.schema_version,
            state_delta: vec![],
            timeline_events: vec![],
            new_key_blocks: vec![],
            battle_report: json!({ "kind": "echo", "world": input.world_ref.world_id }),
        })
    }

    /// The macro must expand to the three ABI exports wired to the shims
    /// (AR-2). The full round-trip through linear memory runs on wasm32 (real
    /// host integration tests); on the host we verify the expansion compiles
    /// and the generated `compute` reaches the shim's sentinel path without
    /// touching memory (in_len == 0 is rejected before any dereference).
    #[test]
    fn nexus_entry_macro_expands_to_working_exports() {
        nexus_entry!(echo_compute);

        // `init` is a no-op export.
        init();

        // `alloc` returns a pointer into a leaked buffer (no crash on host).
        let _ptr = alloc(16);

        // `compute` with an empty input must return the InputMalformed
        // sentinel (-1) without dereferencing the (null) pointers.
        assert_eq!(compute(0, 0, 0, 0), -1);
    }

    /// The blanket impl must accept a plain fn item directly (V1's only entry
    /// form), not just through the macro (AR-2).
    #[test]
    fn blanket_impl_accepts_fn_item() {
        fn double(input: ComputeInput) -> Result<ComputeOutput, ModuleError> {
            let _ = input;
            Ok(ComputeOutput {
                schema_version: 1,
                state_delta: vec![],
                timeline_events: vec![],
                new_key_blocks: vec![],
                battle_report: json!({ "kind": "double" }),
            })
        }

        let input = ComputeInput {
            schema_version: 1,
            world_ref: WorldRef {
                world_id: "w".to_string(),
                branch_id: "root".to_string(),
                timeline_head_event_id: "evt_0".to_string(),
            },
            key_blocks: vec![],
            narrative_state: json!({}),
            invocation: json!({}),
        };

        let output = NexusModule::compute(&double, input).expect("compute succeeds");
        assert_eq!(output.battle_report["kind"], "double");
    }
}
