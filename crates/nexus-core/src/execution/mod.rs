//! Optional execution authority (v1.190 P3).
//!
//! This module owns the single engine/effect owner contract behind the
//! `execution` feature: engine construction, the run coordinator, prompt
//! execution, the durable workspace commit authority and the background
//! scheduler ownership. Logging, HTTP binding, OS signals and the SPA
//! deliberately stay in the daemon — a domain-only [`crate::CoreService`]
//! open starts none of these tasks.
//!
//! Layers:
//!
//! - [`lifecycle`] — engine/storage construction from an owned
//!   [`crate::CoreService`], and the single-owner [`lifecycle::ExecutionHandle`].
//! - [`workflow`] — the bounded run driver, restart classification and the
//!   single-flight run coordinator the handle owns.
//! - [`workspace`] — the durable workspace commit authority (sessions, OCC,
//!   intents, recovery) moved out of the daemon in P3-T2, with its
//!   filesystem/OCC primitives ([`commit_fs`], [`bounds`], [`authority`],
//!   [`scope`]) and its two orchestration adapters ([`executor`],
//!   [`state_provider`]).
//! - [`schedules`] — the scheduler/background-effect ownership: cron
//!   staggering, the master-decision timeout watcher, the reference refresh
//!   sweep and per-Work auto-chronology.
//! - [`run_events`] — the bounded per-run event rings the SSE transport
//!   replays from; the ring is payload-neutral so the daemon keeps the
//!   `HostEvent` vocabulary.
//! - `prompt_executor` and [`soul_narrative_synthesizer`] — the production
//!   prompt executor over the Host facade and the ACP-backed soul-narrative
//!   synthesizer, moved here from the deleted daemon composition (v1.193
//!   P2-T2, technical contracts §4). Each stays the single owner of its
//!   adapter.
//! - `production` — the core-owned hosted workspace composition (v1.195
//!   P0-T1): it resolves the selected creative root from the core's own
//!   metadata and builds the workspace port bundle the hosted production
//!   factory consumes. It needs the Host edge only because that factory does,
//!   so it rides `execution + provider-host`.
//!
//! The handle is the ONE owner of the task set, engine epoch and cleanup:
//! duplicate `start_execution` refuses or returns the same established owner
//! and never builds a second engine.

pub mod applied_documents;
// v1.245 extraction: the filesystem/OCC primitives live in
// `nexus-execution`; the re-export keeps the in-crate paths stable while the
// owner module still forwards them.
pub use nexus_execution::{authority, commit_fs};
// v1.245 extraction: the session layer and its path bounds live in
// `nexus-execution` too.
pub use nexus_execution::{bounds, scope, session, session_commit};
// Capability dispatch and compute (v1.190 P3-T3). The protocol-neutral peer
// registry it consults is `nexus_connect::peer_tools` (v1.244 extraction);
// `compute` additionally requires the WASM edge, so it is gated separately.
pub mod capabilities;
#[cfg(feature = "compute")]
pub mod compute;
pub mod executor;
pub mod handle_ops;
pub mod lifecycle;
// v1.207 P3-T1: the stable durable operation id (`op_<hex32>`) derivation and
// shape validation every receipt consumer resolves through (spec §B.1). It
// lives at the crate root (feature `operation-id`) because the Connect host
// resolves ids too and does not link `execution`; this re-export keeps the
// original `nexus_core::execution::operation_id` path valid.
#[cfg(feature = "operation-id")]
pub use crate::operation_id;
// v1.195 P0-T1: the hosted production composition. It builds the selected-root
// workspace port bundle for the hosted factory that the NEXT task adds, so it
// needs both the execution layer and that factory's Host edge.
#[cfg(all(feature = "execution", feature = "provider-host"))]
pub mod production;
// v1.193 P2-T2: the production `PromptExecutor` over the existing Host plane
// moved here from the deleted daemon composition (technical contracts §4).
// It needs the Host facade edge, so it compiles only with `provider-host`.
#[cfg(feature = "provider-host")]
pub mod prompt_executor;
// v1.245 extraction: the run event rings and the scheduler ownership live in
// `nexus-execution`.
pub use nexus_execution::{run_events, schedules};
// v1.193 P2-T2: the production ACP-backed `SoulNarrativeSynthesizer` moved
// here from the deleted daemon composition (technical contracts §4). It
// consumes the orchestration `CapabilityRegistry`, so it rides `execution`.
pub mod soul_narrative_synthesizer;
pub mod state_provider;
// Test-only crash/rendezvous seams, owned by `nexus-execution` and compiled
// only for a consumer's tests or its `test-hooks` feature.
#[cfg(any(test, feature = "test-hooks"))]
pub use nexus_execution::test_hooks;
// v1.234 P0+P1 (ADR #485 §4.4): the single byte-write port for Work artifacts;
// `fs/write_text_file` commits every `Works/<work_ref>/…` path through it.
pub mod work_artifact;
// v1.245 extraction: the run driver lives in `nexus-execution`.
pub use nexus_execution::workflow;
pub mod workspace;

pub use handle_ops::run_event_page;
pub use lifecycle::{
    ExecutionBuildObserver, ExecutionHandle, ExecutionOpenError, ExecutionSubscriptionObserver,
    RunnerDeps,
};
pub use schedules::{AutoChronologyConfig, CronSupervisorConfig, StaleFindingsWatcherConfig};
pub use workflow::{
    drive_preset_run, resume_driven_sessions, CancelOutcome, DriveDisposition, PresetRunConfig,
    PresetRunOutcome, ResumeDecision, RunControlError, RunControlResult, RunSignal,
    WorkflowRunCoordinator,
};
pub use workspace::commit_workspace;
