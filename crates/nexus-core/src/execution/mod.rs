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

pub mod authority;
pub mod bounds;
// Capability dispatch, compute and the protocol-neutral peer registry
// (v1.190 P3-T3). `capabilities` and `peer_tools` join the execution cohort;
// `compute` additionally requires the WASM edge, so it is gated separately.
pub mod capabilities;
pub mod commit_fs;
#[cfg(feature = "compute")]
pub mod compute;
pub mod executor;
pub mod handle_ops;
pub mod lifecycle;
pub mod peer_tools;
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
pub mod run_events;
pub mod schedules;
pub mod scope;
pub mod session;
pub mod session_commit;
// v1.193 P2-T2: the production ACP-backed `SoulNarrativeSynthesizer` moved
// here from the deleted daemon composition (technical contracts §4). It
// consumes the orchestration `CapabilityRegistry`, so it rides `execution`.
pub mod soul_narrative_synthesizer;
pub mod state_provider;
// Test-only crash/rendezvous seams. Compiled only for this package's tests or
// with the `test-hooks` feature, exactly as in the daemon before the move.
#[cfg(any(test, feature = "test-hooks"))]
pub mod test_hooks;
pub mod workflow;
pub mod workspace;

pub use handle_ops::run_event_page;
pub use lifecycle::{ExecutionBuildObserver, ExecutionHandle, ExecutionOpenError, RunnerDeps};
pub use schedules::{
    AutoChronologyConfig, CronSupervisorConfig, RefreshSchedulerConfig, StaleFindingsWatcherConfig,
};
pub use workflow::{
    drive_preset_run, resume_driven_sessions, CancelOutcome, DriveDisposition, PresetRunConfig,
    PresetRunOutcome, ResumeDecision, RunControlError, RunControlResult, RunSignal,
    WorkflowRunCoordinator,
};
pub use workspace::commit_workspace;
