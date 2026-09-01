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
    EngineSignal, OrchestrationEngine, SessionId, SessionStatus, StepOutcome,
};
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
    }

    impl ScriptedEngine {
        fn with_script(script: Vec<Result<StepOutcome, EngineError>>) -> Self {
            Self {
                script: parking_lot::Mutex::new(VecDeque::from(script)),
                status: parking_lot::Mutex::new(SessionStatus::Running),
                signals: parking_lot::Mutex::new(Vec::new()),
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
}
