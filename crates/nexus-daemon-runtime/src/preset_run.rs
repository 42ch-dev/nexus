//! Minimal daemon-local preset-run driver (V1.180 P2 T1 — DR-06 ops E2E).
//!
//! Before this module there was NO production code that advanced an outer
//! orchestration session: the HTTP surface offers create/list/get/signal
//! (`signal` flips in-memory status without stepping), and the schedule
//! supervisor admits schedules but never executes them. A waiting join
//! (merge/converge) only re-checks its `timeout_ms` deadline when **stepped**
//! — so without a driver the DR-06 bounded-join timeout is unreachable in a
//! real daemon session (V1.179 QA carried note qc3 S-7).
//!
//! This driver is the missing seam: a bounded, cancellable step-loop that
//! drives a session via [`OrchestrationEngine::run_step`] until terminal,
//! error, or an external-input stop. Task 2 (BL-04 checkpoint resume)
//! re-drives recovered sessions through this same function.
//!
//! ## Pause / signal semantics (as already implemented in the engine)
//!
//! - Every successful non-terminal step reports `StepOutcome::Paused` — the
//!   graph-flow boundary pause between tasks — and `run_step_internal` then
//!   records the in-memory status as `SessionStatus::Paused`. A user
//!   `EngineSignal::Pause` flips the SAME in-memory status, so a status
//!   probe cannot distinguish an inter-task boundary from a user pause
//!   (the existing child-session driver, `InnerGraphTask`, therefore
//!   continues stepping through `Paused` outcomes). This driver does the
//!   same: `Paused` steps continue.
//! - A `Cancel` signal flips the in-memory status to `Failed`, which the
//!   driver observes pre-step and reports as [`PresetRunOutcome::Cancelled`].
//! - `EngineSignal::Advance`/`Resume` only flip status back to `Running`;
//!   stepping a session whose status is `Running`, `Paused`, or
//!   `WaitingForInput` (with [`PresetRunConfig::resume_waiting`]) is safe
//!   because `run_step` itself never consults the tracker status.
//!
//! ## Failure record
//!
//! A terminal engine error is returned as [`PresetRunOutcome::Failed`] with
//! the full engine error text — the typed `converge_timeout:` discriminator
//! from `GraphError::TaskExecutionFailed` surfaces verbatim in that text.
//! When a [`graph_flow::SessionStorage`] handle is supplied, the driver
//! also lands the failure in the persisted session context (`_run_status` =
//! `"failed"`, `_run_error` = error text) and flips the daemon-visible
//! tracker status to `Failed` via a `Cancel` signal — the only failure
//! status-flip the engine signal surface exposes.

use std::sync::Arc;
use std::time::Duration;

use graph_flow::SessionStorage;
use nexus_orchestration::engine::{
    EngineSignal, OrchestrationEngine, SessionId, SessionStatus, SessionSummary, StepOutcome,
};
use nexus_orchestration::resume_rules;
use tokio_util::sync::CancellationToken;

/// Bounds and posture for one [`drive_preset_run`] call.
#[derive(Debug, Clone)]
pub struct PresetRunConfig {
    /// Hard upper bound on `run_step` invocations per drive call.
    pub max_steps: u32,
    /// Delay between consecutive steps (yields between checkpoint saves).
    pub step_delay: Duration,
    /// When `true` the driver MAY step a session whose last outcome was
    /// `WaitingForInput` (re-drive posture — how a parked join re-checks its
    /// `timeout_ms` deadline). Default `false`: a waiting session is an
    /// external-input stop and the caller decides when to resume — re-stepping
    /// a `manual`/`llm_judge` wait would auto-advance it.
    pub resume_waiting: bool,
}

impl Default for PresetRunConfig {
    fn default() -> Self {
        Self {
            max_steps: 1000,
            step_delay: Duration::from_millis(10),
            resume_waiting: false,
        }
    }
}

/// Terminal stopping point of one [`drive_preset_run`] call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetRunOutcome {
    /// The session reached `Completed`.
    Completed { steps: u32 },
    /// The session is parked waiting for external input; the driver stopped
    /// (either the pre-step status was `WaitingForInput` without
    /// [`PresetRunConfig::resume_waiting`], or the step itself produced
    /// `WaitForInput`). Re-drive with `resume_waiting: true` once the input
    /// or deadline condition is met.
    WaitingForInput { steps: u32 },
    /// Cancellation observed: the `cancel` token fired, or the in-memory
    /// status was flipped to `Failed` by a `Cancel` signal / prior failure.
    Cancelled { steps: u32 },
    /// The engine reported a terminal error (e.g. the typed
    /// `converge_timeout:` `GraphError::TaskExecutionFailed`). `error` is
    /// the daemon-observable failure record; when storage was supplied it
    /// is also persisted into the session context (`_run_status`,
    /// `_run_error`).
    Failed { steps: u32, error: String },
    /// `max_steps` exhausted without reaching a terminal state.
    MaxStepsExceeded { steps: u32 },
}

/// Drive one session to a stopping point.
///
/// # Semantics
///
/// - Pre-step status `Completed` short-circuits (already done); status
///   `Failed` stops as [`PresetRunOutcome::Cancelled`]; status
///   `WaitingForInput` stops unless [`PresetRunConfig::resume_waiting`].
/// - `StepOutcome::Paused` is the normal inter-task boundary — the loop
///   keeps stepping.
/// - On a terminal engine error the tracker status is flipped to `Failed`
///   via a `Cancel` signal (see module docs) and, when `storage` is `Some`,
///   the failure is written into the persisted session context
///   (`_run_status` / `_run_error`). Persistence is best-effort; storage
///   failures are logged, not returned (the step-loop already failed).
/// - The `cancel` token is checked before every step, including before the
///   first.
///
/// `steps` counts `run_step` invocations performed by THIS call (a re-drive
/// starts at 0).
pub async fn drive_preset_run(
    engine: &dyn OrchestrationEngine,
    storage: Option<&Arc<dyn SessionStorage>>,
    session_id: &SessionId,
    config: &PresetRunConfig,
    cancel: Option<&CancellationToken>,
) -> PresetRunOutcome {
    let mut steps: u32 = 0;

    loop {
        if cancel.is_some_and(CancellationToken::is_cancelled) {
            return PresetRunOutcome::Cancelled { steps };
        }
        if steps > 0 && !config.step_delay.is_zero() {
            tokio::time::sleep(config.step_delay).await;
        }
        if steps >= config.max_steps {
            return PresetRunOutcome::MaxStepsExceeded { steps };
        }

        // Re-read the daemon-visible status before each step (signal
        // semantics: Failed = cancelled, WaitingForInput = external-input
        // stop unless resume_waiting; Paused is the normal boundary).
        match engine.get_status(session_id).await {
            Ok(SessionStatus::Completed) => return PresetRunOutcome::Completed { steps },
            Ok(SessionStatus::Failed) => return PresetRunOutcome::Cancelled { steps },
            Ok(SessionStatus::WaitingForInput) if !config.resume_waiting => {
                return PresetRunOutcome::WaitingForInput { steps };
            }
            Ok(SessionStatus::WaitingForInput | SessionStatus::Running | SessionStatus::Paused) => {
            }
            Err(e) => {
                let error = format!("get_status failed: {e}");
                persist_failure(storage, session_id, &error).await;
                return PresetRunOutcome::Failed { steps, error };
            }
        }

        match engine.run_step(session_id).await {
            Ok(StepOutcome::Completed { .. }) => {
                return PresetRunOutcome::Completed {
                    steps: steps.saturating_add(1),
                };
            }
            Ok(StepOutcome::Paused { .. }) => {
                // Inter-task boundary — keep stepping.
                steps = steps.saturating_add(1);
            }
            Ok(StepOutcome::WaitingForInput { .. }) => {
                return PresetRunOutcome::WaitingForInput {
                    steps: steps.saturating_add(1),
                };
            }
            Ok(StepOutcome::Error(msg)) => {
                mark_failed(engine, session_id).await;
                persist_failure(storage, session_id, &msg).await;
                return PresetRunOutcome::Failed {
                    steps: steps.saturating_add(1),
                    error: msg,
                };
            }
            Err(e) => {
                let error = e.to_string();
                mark_failed(engine, session_id).await;
                persist_failure(storage, session_id, &error).await;
                return PresetRunOutcome::Failed {
                    steps: steps.saturating_add(1),
                    error,
                };
            }
        }
    }
}

/// Flip the daemon-visible tracker status to `Failed` (the only failure
/// status-flip the engine signal surface exposes). Best-effort: the session
/// may already be gone.
async fn mark_failed(engine: &dyn OrchestrationEngine, session_id: &SessionId) {
    if let Err(e) = engine.signal(session_id, EngineSignal::Cancel).await {
        tracing::debug!(
            session_id = %session_id.0,
            error = %e,
            "preset-run driver: could not mark session failed in tracker"
        );
    }
}

/// Persist the failure record into the session context (best-effort).
async fn persist_failure(
    storage: Option<&Arc<dyn SessionStorage>>,
    session_id: &SessionId,
    error: &str,
) {
    let Some(storage) = storage else {
        return;
    };
    let Ok(Some(session)) = storage.get(&session_id.0).await else {
        return;
    };
    session.context.set("_run_status", "failed").await;
    session.context.set("_run_error", error.to_string()).await;
    if let Err(e) = storage.save(session).await {
        tracing::warn!(
            session_id = %session_id.0,
            error = %e,
            "preset-run driver: failed to persist failure record"
        );
    }
}

// ---------------------------------------------------------------------------
// Restart resume (BL-04 slice, V1.180 P2 T2)
// ---------------------------------------------------------------------------

/// The resume re-drive's decision for one recovered session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResumeDecision {
    /// Re-driven from the persisted position; `outcome` is the drive result.
    ReDriven {
        session_id: SessionId,
        outcome: PresetRunOutcome,
    },
    /// Skipped: the persisted context carries a typed failure record
    /// (`_run_status` / `_run_error`). `SqliteSessionStorage::save` never
    /// updates the DB `status` column on conflict, so a typed-failed join
    /// still reads `running` from storage — re-driving it would re-tick the
    /// failed join. The context keys are the only reliable failure record.
    SkippedTypedFailed { session_id: SessionId },
    /// Skipped: not of the converge/merge chain class — the persisted
    /// context carries no LIVE join-tracking key (all `_converge_arrivals_*`,
    /// `_merge_*`, `_join_wait_start_*` keys are absent or `Value::Null`).
    /// Such sessions behave byte-identically to pre-T2 boot
    /// (tracked-but-not-driven).
    SkippedNotConvergeMergeClass { session_id: SessionId },
    /// Skipped: no `FlowRunner` exists for the session (reconstruction
    /// failed — e.g. a user preset that is not embedded). It stays
    /// tracked-but-not-driven; driving it would fail with `NoGraphLoaded`
    /// and the driver would mark it failed.
    SkippedNoRunner { session_id: SessionId },
    /// Skipped: the persisted session could not be loaded from storage.
    SkippedUnreadable {
        session_id: SessionId,
        error: String,
    },
}

/// Resume re-drive for recovered non-terminal sessions (BL-04 slice, T2).
///
/// After a daemon restart, boot recovery reconstructs `FlowRunner`s for
/// recovered sessions (`recover_sessions` → `reconstruct_runner`). This
/// function re-drives the converge/merge chain class of those sessions
/// from their persisted position via [`drive_preset_run`] — completed
/// edges are not re-executed because the persisted `current_task_id` +
/// `context_json` have advanced past them.
///
/// # Scoped rule (declared)
///
/// A recovered session is re-driven only when ALL of:
/// 1. It is non-terminal (the caller passes `list_non_terminal_sessions`
///    output — boot already filters).
/// 2. Its persisted context carries NO typed-failure record
///    (`_run_status` / `_run_error`).
/// 3. Its persisted context carries a LIVE join-tracking key
///    (`_converge_arrivals_*`, `_merge_*`, or `_join_wait_start_*`) with a
///    non-Null value — written ONLY by merge/converge gate states. The
///    gates clear their keys by writing `Value::Null` (deadline exceeded /
///    success-leave), so a session whose join keys are all Null has LEFT
///    the chain and is not re-driven. This is the converge/merge chain
///    class: a `manual`/`llm_judge` wait (no live join keys) is never
///    auto-advanced, and sessions without the class behave
///    byte-identically to pre-T2 boot.
/// 4. A `FlowRunner` exists for the session (`engine.has_runner`) — the
///    caller must have reconstructed it (boot does via `recover_sessions`).
///
/// # `_join_wait_start_*` across downtime (pinned)
///
/// The wait-start timestamp is a wall-clock value persisted in
/// `context_json`; `join_timeout_tick` compares it against the wall clock
/// on every re-step. Elapsed therefore INCLUDES downtime — a join whose
/// deadline passed while the daemon was down fires on the first re-step
/// (no re-baseline). Pinned by the restart-resume test.
///
/// # Caveat (declared in this slice's Done)
///
/// Runner reconstruction covers embedded presets only
/// (`load_embedded_preset` in `recover_sessions`). A user-preset session
/// that fails reconstruction stays tracked-but-not-driven (warn) — the
/// resume re-drive skips it ([`ResumeDecision::SkippedNoRunner`]) and never
/// marks it failed. Operator-visible resume for such sessions is the
/// BL-04 remainder (out of scope).
///
/// The caller passes `config`; the resume posture requires
/// `resume_waiting: true` so a parked join re-checks its `timeout_ms`
/// deadline on the first re-step (the DR-06 recovery seam).
pub async fn resume_driven_sessions(
    engine: &dyn OrchestrationEngine,
    storage: &Arc<dyn SessionStorage>,
    summaries: &[SessionSummary],
    config: &PresetRunConfig,
    cancel: Option<&CancellationToken>,
) -> Vec<ResumeDecision> {
    let mut decisions = Vec::with_capacity(summaries.len());
    for summary in summaries {
        let session_id = summary.session_id.clone();

        // Load the persisted session (position + context).
        let session = match storage.get(&session_id.0).await {
            Ok(Some(session)) => session,
            Ok(None) => {
                decisions.push(ResumeDecision::SkippedUnreadable {
                    session_id,
                    error: "session not found in storage".to_string(),
                });
                continue;
            }
            Err(e) => {
                decisions.push(ResumeDecision::SkippedUnreadable {
                    session_id,
                    error: e.to_string(),
                });
                continue;
            }
        };

        // 1. Typed-failure filter (T1 review constraint): never re-tick a
        //    typed-failed join. The DB `status` column stays `running`
        //    after a typed failure (save ON CONFLICT does not update it),
        //    so the context keys are the only reliable failure record.
        //    Projected via the shared `resume_rules` module — the same
        //    predicates `nexus42 ops inspect` uses, over the same persisted
        //    data (single source of truth, qc1 W1; string-typed values only,
        //    mirroring `Context::get`).
        let context_value = serde_json::to_value(&session.context).ok();
        let data = context_value.as_ref().and_then(resume_rules::context_data);
        if data.is_some_and(resume_rules::typed_failure_keys_present) {
            decisions.push(ResumeDecision::SkippedTypedFailed { session_id });
            continue;
        }

        // 2. Converge/merge chain class filter (no-checkpoint default
        //    equivalence): only sessions provably inside a converge/merge
        //    chain are re-driven.
        if !data.is_some_and(resume_rules::is_converge_merge_chain) {
            decisions.push(ResumeDecision::SkippedNotConvergeMergeClass { session_id });
            continue;
        }

        // 3. Runner-existence filter: a session whose runner failed
        //    reconstruction stays tracked-but-not-driven.
        if !engine.has_runner(&session_id).await {
            decisions.push(ResumeDecision::SkippedNoRunner { session_id });
            continue;
        }

        // 4. Re-drive from the persisted position.
        let outcome = drive_preset_run(engine, Some(storage), &session_id, config, cancel).await;
        decisions.push(ResumeDecision::ReDriven {
            session_id,
            outcome,
        });
    }
    decisions
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::VecDeque;

    use nexus_orchestration::engine::{
        ChildSessionParams, Context, EngineError, SessionFilter, SessionKey, SessionSummary,
    };

    /// Scripted engine: `run_step` pops from a queue; `get_status` reads a
    /// settable status; `signal` records signals (Cancel flips status to
    /// `Failed`, mirroring the real engines).
    struct ScriptedEngine {
        script: parking_lot::Mutex<VecDeque<Result<StepOutcome, EngineError>>>,
        status: parking_lot::Mutex<SessionStatus>,
        signals: parking_lot::Mutex<Vec<EngineSignal>>,
        /// `has_runner` answer (default `true`; set `false` to exercise the
        /// resume no-runner skip).
        runner_present: parking_lot::Mutex<bool>,
    }

    impl ScriptedEngine {
        fn with_script(script: Vec<Result<StepOutcome, EngineError>>) -> Self {
            Self {
                script: parking_lot::Mutex::new(VecDeque::from(script)),
                status: parking_lot::Mutex::new(SessionStatus::Running),
                signals: parking_lot::Mutex::new(Vec::new()),
                runner_present: parking_lot::Mutex::new(true),
            }
        }
    }

    #[async_trait]
    impl OrchestrationEngine for ScriptedEngine {
        async fn run_step(&self, _session_id: &SessionId) -> Result<StepOutcome, EngineError> {
            self.script.lock().pop_front().unwrap_or_else(|| {
                Err(EngineError::SessionNotFound("script exhausted".to_string()))
            })
        }

        async fn new_session(
            &self,
            _key: SessionKey,
            _ctx: Context,
        ) -> Result<SessionId, EngineError> {
            unreachable!("not used by driver unit tests")
        }

        async fn start_session_with_graph(
            &self,
            _id_prefix: &str,
            _graph: Arc<graph_flow::Graph>,
        ) -> Result<SessionId, EngineError> {
            unreachable!("not used by driver unit tests")
        }

        async fn get_status(&self, _session_id: &SessionId) -> Result<SessionStatus, EngineError> {
            Ok(self.status.lock().clone())
        }

        async fn has_runner(&self, _session_id: &SessionId) -> bool {
            *self.runner_present.lock()
        }

        async fn signal(
            &self,
            _session_id: &SessionId,
            signal: EngineSignal,
        ) -> Result<(), EngineError> {
            self.signals.lock().push(signal.clone());
            if matches!(signal, EngineSignal::Cancel) {
                *self.status.lock() = SessionStatus::Failed;
            }
            Ok(())
        }

        async fn list_active(
            &self,
            _filter: SessionFilter,
        ) -> Result<Vec<SessionSummary>, EngineError> {
            unreachable!("not used by driver unit tests")
        }

        async fn spawn_child_session(
            &self,
            _params: ChildSessionParams,
        ) -> Result<SessionId, EngineError> {
            unreachable!("not used by driver unit tests")
        }

        async fn get_context(
            &self,
            _session_id: &SessionId,
        ) -> Result<graph_flow::Context, EngineError> {
            unreachable!("not used by driver unit tests")
        }

        async fn start_session_with_preset(
            &self,
            _loaded: &nexus_orchestration::preset::LoadedPreset,
        ) -> Result<SessionId, EngineError> {
            unreachable!("not used by driver unit tests")
        }

        async fn start_session_with_preset_for_creator(
            &self,
            _loaded: &nexus_orchestration::preset::LoadedPreset,
            _creator_id: &str,
        ) -> Result<SessionId, EngineError> {
            unreachable!("not used by driver unit tests")
        }
    }

    fn sid() -> SessionId {
        SessionId("test:session".to_string())
    }

    fn paused() -> StepOutcome {
        StepOutcome::Paused {
            next_task_id: "next".to_string(),
            reason: "Task completed, continuing to next task".to_string(),
        }
    }

    fn waiting() -> StepOutcome {
        StepOutcome::WaitingForInput { response: None }
    }

    #[tokio::test]
    async fn steps_through_paused_boundaries_to_completed() {
        let engine = ScriptedEngine::with_script(vec![
            Ok(paused()),
            Ok(paused()),
            Ok(StepOutcome::Completed { response: None }),
        ]);
        let outcome =
            drive_preset_run(&engine, None, &sid(), &PresetRunConfig::default(), None).await;
        assert_eq!(
            outcome,
            PresetRunOutcome::Completed { steps: 3 },
            "inter-task Paused outcomes step through to completion"
        );
    }

    #[tokio::test]
    async fn stops_at_waiting_for_input_without_resume_flag() {
        let engine = ScriptedEngine::with_script(vec![Ok(waiting())]);
        let outcome =
            drive_preset_run(&engine, None, &sid(), &PresetRunConfig::default(), None).await;
        assert_eq!(outcome, PresetRunOutcome::WaitingForInput { steps: 1 });
    }

    #[tokio::test]
    async fn does_not_step_a_parked_session_without_resume_flag() {
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        *engine.status.lock() = SessionStatus::WaitingForInput;
        let outcome =
            drive_preset_run(&engine, None, &sid(), &PresetRunConfig::default(), None).await;
        assert_eq!(
            outcome,
            PresetRunOutcome::WaitingForInput { steps: 0 },
            "a parked session is an external-input stop, not auto-stepped"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step must be consumed for a parked session"
        );
    }

    #[tokio::test]
    async fn resumes_parked_session_with_resume_waiting() {
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        *engine.status.lock() = SessionStatus::WaitingForInput;
        let config = PresetRunConfig {
            resume_waiting: true,
            ..PresetRunConfig::default()
        };
        let outcome = drive_preset_run(&engine, None, &sid(), &config, None).await;
        assert_eq!(
            outcome,
            PresetRunOutcome::Completed { steps: 1 },
            "resume_waiting lets the driver re-step a parked session (join deadline re-check)"
        );
    }

    #[tokio::test]
    async fn cancel_token_stops_before_any_step() {
        let engine = ScriptedEngine::with_script(vec![Ok(paused())]);
        let token = CancellationToken::new();
        token.cancel();
        let outcome = drive_preset_run(
            &engine,
            None,
            &sid(),
            &PresetRunConfig::default(),
            Some(&token),
        )
        .await;
        assert_eq!(
            outcome,
            PresetRunOutcome::Cancelled { steps: 0 },
            "a pre-cancelled token stops before the first step"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed after cancellation"
        );
    }

    #[tokio::test]
    async fn step_error_returns_typed_failure_and_marks_session_failed() {
        let engine = ScriptedEngine::with_script(vec![Err(EngineError::GraphFlow(
            graph_flow::GraphError::TaskExecutionFailed(
                "converge_timeout: gate=converge, state_id=join, arrived=1, expected=2, \
                 elapsed_ms=123"
                    .to_string(),
            ),
        ))]);
        let outcome =
            drive_preset_run(&engine, None, &sid(), &PresetRunConfig::default(), None).await;
        match outcome {
            PresetRunOutcome::Failed { steps, error } => {
                assert_eq!(steps, 1);
                assert!(
                    error.contains("converge_timeout: gate=converge, state_id=join, arrived=1"),
                    "typed discriminator must surface verbatim: {error}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(*engine.status.lock(), SessionStatus::Failed);
        assert!(
            engine
                .signals
                .lock()
                .iter()
                .any(|s| matches!(s, EngineSignal::Cancel)),
            "driver must flip the tracker status to Failed via Cancel"
        );
    }

    #[tokio::test]
    async fn failure_record_is_persisted_into_session_context() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        let session = graph_flow::Session::new_from_task("test:session".to_string(), "join");
        storage.save(session).await.unwrap();

        let engine = ScriptedEngine::with_script(vec![Err(EngineError::GraphFlow(
            graph_flow::GraphError::TaskExecutionFailed(
                "converge_timeout: gate=merge, state_id=join, arrived=0, expected=2, elapsed_ms=9"
                    .to_string(),
            ),
        ))]);
        let outcome = drive_preset_run(
            &engine,
            Some(&storage),
            &sid(),
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert!(matches!(outcome, PresetRunOutcome::Failed { .. }));

        let persisted = storage.get("test:session").await.unwrap().unwrap();
        let status: String = persisted.context.get("_run_status").await.unwrap();
        let error: String = persisted.context.get("_run_error").await.unwrap();
        assert_eq!(status, "failed");
        assert!(error.contains("converge_timeout: gate=merge"));
    }

    #[tokio::test]
    async fn max_steps_bound_is_enforced() {
        let engine = ScriptedEngine::with_script(vec![Ok(paused()), Ok(paused())]);
        let config = PresetRunConfig {
            max_steps: 2,
            ..PresetRunConfig::default()
        };
        let outcome = drive_preset_run(&engine, None, &sid(), &config, None).await;
        assert_eq!(
            outcome,
            PresetRunOutcome::MaxStepsExceeded { steps: 2 },
            "a stalled non-terminal loop must hit the bound instead of spinning"
        );
    }

    #[tokio::test]
    async fn stop_short_when_session_status_is_failed() {
        let engine = ScriptedEngine::with_script(vec![Ok(paused())]);
        *engine.status.lock() = SessionStatus::Failed;
        let outcome =
            drive_preset_run(&engine, None, &sid(), &PresetRunConfig::default(), None).await;
        assert_eq!(
            outcome,
            PresetRunOutcome::Cancelled { steps: 0 },
            "a session already flipped to Failed (cancel signal / prior failure) is not stepped"
        );
    }

    // -----------------------------------------------------------------------
    // Resume re-drive (BL-04 slice, T2)
    // -----------------------------------------------------------------------

    /// Seed a persisted session with the given context keys.
    async fn seed_session(
        storage: &Arc<dyn SessionStorage>,
        id: &str,
        current_task: &str,
        keys: &[(&str, serde_json::Value)],
    ) {
        let session = graph_flow::Session::new_from_task(id.to_string(), current_task);
        for (k, v) in keys {
            session.context.set(*k, v.clone()).await;
        }
        storage.save(session).await.unwrap();
    }

    fn summary(id: &str) -> SessionSummary {
        SessionSummary {
            session_id: SessionId(id.to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::WaitingForInput,
            current_task_id: Some("join".to_string()),
        }
    }

    #[tokio::test]
    async fn resume_skips_typed_failed_session() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        // A typed-failed join: DB status would read `running` (save ON
        // CONFLICT never updates status), but the context carries the
        // failure record — the resume must NOT re-tick it.
        seed_session(
            &storage,
            "test:typed-failed",
            "join",
            &[
                ("_converge_arrivals_join", serde_json::json!(["branch_a"])),
                ("_join_wait_start_join", serde_json::json!(1000u64)),
                ("_run_status", serde_json::json!("failed")),
                (
                    "_run_error",
                    serde_json::json!("converge_timeout: gate=converge"),
                ),
            ],
        )
        .await;
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            &[summary("test:typed-failed")],
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::SkippedTypedFailed {
                session_id: SessionId("test:typed-failed".to_string())
            }],
            "a typed-failed session must never be re-driven"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for a typed-failed session"
        );
    }

    #[tokio::test]
    async fn resume_skips_non_converge_merge_class_session() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        // No join-tracking keys: not of the converge/merge chain class —
        // byte-identical to pre-T2 boot (tracked-but-not-driven).
        seed_session(&storage, "test:linear", "mid", &[]).await;
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            &[summary("test:linear")],
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::SkippedNotConvergeMergeClass {
                session_id: SessionId("test:linear".to_string())
            }],
            "a session without join-tracking keys must not be re-driven"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for a non-class session"
        );
    }

    #[tokio::test]
    async fn resume_skips_session_without_runner() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        seed_session(
            &storage,
            "test:no-runner",
            "join",
            &[("_join_wait_start_join", serde_json::json!(1000u64))],
        )
        .await;
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        *engine.runner_present.lock() = false;
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            &[summary("test:no-runner")],
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::SkippedNoRunner {
                session_id: SessionId("test:no-runner".to_string())
            }],
            "a session whose runner failed reconstruction stays tracked-but-not-driven"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for a runner-less session"
        );
    }

    #[tokio::test]
    async fn resume_skips_unreadable_session() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            &[summary("test:missing")],
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(decisions.len(), 1);
        match &decisions[0] {
            ResumeDecision::SkippedUnreadable { session_id, .. } => {
                assert_eq!(session_id, &SessionId("test:missing".to_string()));
            }
            other => panic!("expected SkippedUnreadable, got {other:?}"),
        }
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for an unreadable session"
        );
    }

    #[tokio::test]
    async fn resume_re_drives_converge_merge_chain_session() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        // Parked at the join with the arrival set + wait-start persisted:
        // the resume re-drives from THIS position (completed edges are not
        // re-executed because current_task_id has advanced past them).
        seed_session(
            &storage,
            "test:parked-join",
            "join",
            &[
                ("_converge_arrivals_join", serde_json::json!(["branch_a"])),
                ("_join_wait_start_join", serde_json::json!(1000u64)),
            ],
        )
        .await;
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let config = PresetRunConfig {
            resume_waiting: true,
            ..PresetRunConfig::default()
        };
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            &[summary("test:parked-join")],
            &config,
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::ReDriven {
                session_id: SessionId("test:parked-join".to_string()),
                outcome: PresetRunOutcome::Completed { steps: 1 },
            }],
            "a parked converge/merge chain session is re-driven from its persisted position"
        );
    }

    // QC fix wave 1 (qc2 F-001 + qc3 F-002): key PRESENCE misclassifies
    // completed / post-join sessions as live. `Context::set` never removes
    // keys; the join gates clear them by writing `Value::Null` (deadline
    // exceeded in `join_timeout_tick`; success-leave in the join task). A
    // session whose join keys are all Null has LEFT the chain and must be
    // class-negative — otherwise the resume re-drive re-executes completed
    // sessions and auto-advances post-join `llm_judge`/`manual` waits.
    #[tokio::test]
    async fn resume_skips_completed_session_with_null_join_keys() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        // The join completed and LEFT the chain: the gates cleared their
        // tracking keys by writing `Value::Null` (success-leave). Key
        // presence alone would misclassify this session as live and
        // re-drive it.
        seed_session(
            &storage,
            "test:completed",
            "post_join",
            &[
                ("_converge_arrivals_join", serde_json::Value::Null),
                ("_join_wait_start_join", serde_json::Value::Null),
            ],
        )
        .await;
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let config = PresetRunConfig {
            resume_waiting: true,
            ..PresetRunConfig::default()
        };
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            &[summary("test:completed")],
            &config,
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::SkippedNotConvergeMergeClass {
                session_id: SessionId("test:completed".to_string())
            }],
            "a session whose join keys are all Null has left the chain and must not be re-driven"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for a completed session"
        );
    }

    #[tokio::test]
    async fn resume_does_not_auto_advance_post_join_llm_judge_wait() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        // The session passed the join (keys cleared to Null) and now waits
        // at an llm_judge/manual state. It must NOT be auto-advanced by the
        // resume re-drive — only a LIVE join key makes a session
        // class-positive.
        seed_session(
            &storage,
            "test:post-join-wait",
            "llm_judge",
            &[("_join_wait_start_join", serde_json::Value::Null)],
        )
        .await;
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let config = PresetRunConfig {
            resume_waiting: true,
            ..PresetRunConfig::default()
        };
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            &[summary("test:post-join-wait")],
            &config,
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::SkippedNotConvergeMergeClass {
                session_id: SessionId("test:post-join-wait".to_string())
            }],
            "a post-join llm_judge/manual wait (Null join key) must not be auto-advanced"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for a post-join wait"
        );
    }

    // QC fix wave 1 (qc2 F-002 + qc3 F-001): the boot resume spawn must be
    // cancellable — `drive_preset_run` checks the token before every step,
    // so cancelling mid-drive stops the re-drive without exhausting the
    // step budget.
    #[tokio::test]
    async fn resume_stops_stepping_after_cancel_fires() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        seed_session(
            &storage,
            "test:cancel",
            "join",
            &[("_join_wait_start_join", serde_json::json!(1000u64))],
        )
        .await;
        // A long Paused script: without cancellation the drive would consume
        // every step; with the token it must stop at the first check after
        // cancel fires. Shared via `Arc` so the spawned task and the
        // assertion below can both reach the script queue.
        let engine = Arc::new(ScriptedEngine::with_script(
            (0..1000).map(|_| Ok(paused())).collect(),
        ));
        let cancel = tokio_util::sync::CancellationToken::new();
        let cancel_for_task = cancel.clone();
        let engine_for_task = Arc::clone(&engine);
        let handle = tokio::spawn(async move {
            let config = PresetRunConfig {
                resume_waiting: true,
                ..PresetRunConfig::default()
            };
            resume_driven_sessions(
                &*engine_for_task,
                &storage,
                &[summary("test:cancel")],
                &config,
                Some(&cancel_for_task),
            )
            .await
        });
        // Let the drive start stepping, then cancel.
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel.cancel();
        let decisions = handle.await.expect("resume task joins");
        assert_eq!(decisions.len(), 1);
        match &decisions[0] {
            ResumeDecision::ReDriven { outcome, .. } => {
                assert!(
                    matches!(outcome, PresetRunOutcome::Cancelled { .. }),
                    "resume must stop stepping after cancel fires, got {outcome:?}"
                );
            }
            other => panic!("expected ReDriven, got {other:?}"),
        }
        assert!(
            !engine.script.lock().is_empty(),
            "the drive must stop stepping after cancel, not exhaust the script"
        );
    }
}
