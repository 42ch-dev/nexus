//! The execution owner (v1.190 P3; renamed by the v1.245 extraction).
//!
//! This module owns what only the crate defining [`crate::CoreService`] can
//! own: engine construction and the single-owner [`lifecycle::ExecutionHandle`],
//! the `nexus.*` capability/compute/ops surface on that handle, the hosted
//! composition, and the workspace executor and commit-authority adapters that
//! read the owner's own state. Logging, HTTP binding, OS signals and the SPA
//! stay in the daemon — a domain-only [`crate::CoreService`] open starts none
//! of these tasks.
//!
//! The durable run engine lives in `nexus-execution`: the bounded run driver
//! and its coordinator (`workflow`), the scheduler ownership (`schedules`), the
//! per-run event rings (`run_events`), and the workspace session/commit
//! primitives (`session`, `session_commit`, `scope`, `bounds`, `commit_fs`,
//! `authority`). Those modules are driven by ports, never by the owner's state,
//! so the dependency direction is `nexus-core → nexus-execution`, never back.
//!
//! The handle is the ONE owner of the task set, engine epoch and cleanup:
//! duplicate `start_execution` refuses or returns the same established owner
//! and never builds a second engine.
// why: these are the pieces only the crate defining `CoreService` can own, so a domain-only open
// starts none of the daemon's tasks.

pub mod applied_documents;
// why: Capability dispatch and compute (v1.190 P3-T3). The protocol-neutral peer
// registry it consults is `nexus_connect::peer_tools` (v1.244 extraction);
// `compute` additionally requires the WASM edge, so it is gated separately.
pub mod capabilities;
#[cfg(feature = "compute")]
pub mod compute;
pub mod executor;
pub mod handle_ops;
pub mod lifecycle;
// why: v1.195 P0-T1: the hosted production composition. It builds the selected-root
// workspace port bundle for the hosted factory, so it needs both the execution
// layer and that factory's Host edge.
#[cfg(all(feature = "execution", feature = "provider-host"))]
pub mod production;
// v1.234 P0+P1 (ADR #485 §4.4): the single byte-write port for Work artifacts;
// `fs/write_text_file` commits every `Works/<work_ref>/…` path through it.
pub mod work_artifact;
pub mod workspace;

pub use handle_ops::run_event_page;
pub use lifecycle::{
    ExecutionBuildObserver, ExecutionHandle, ExecutionOpenError, ExecutionSubscriptionObserver,
    RunnerDeps,
};
pub use workspace::commit_workspace;
