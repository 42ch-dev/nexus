//! Optional execution authority (v1.190 P3-T1).
//!
//! This module owns the single engine/effect owner contract behind the
//! `execution` feature: engine construction, the run coordinator, prompt
//! execution and recovery ordering. Logging, HTTP binding, OS signals and the
//! SPA deliberately stay in the daemon — a domain-only [`crate::CoreService`]
//! open starts none of these tasks.
//!
//! Layers:
//!
//! - [`lifecycle`] — engine/storage construction from an owned
//!   [`crate::CoreService`], and the single-owner [`lifecycle::ExecutionHandle`].
//! - [`workflow`] — the bounded run driver, restart classification and the
//!   single-flight run coordinator the handle owns.
//!
//! The handle is the ONE owner of the task set, engine epoch and cleanup:
//! duplicate `start_execution` refuses or returns the same established owner
//! and never builds a second engine.

pub mod lifecycle;
pub mod workflow;

pub use lifecycle::{ExecutionBuildObserver, ExecutionHandle, ExecutionOpenError, RunnerDeps};
pub use workflow::{
    CancelOutcome, DriveDisposition, PresetRunConfig, PresetRunOutcome, ResumeDecision,
    RunControlError, RunControlResult, RunSignal, WorkflowRunCoordinator, drive_preset_run,
    resume_driven_sessions,
};
