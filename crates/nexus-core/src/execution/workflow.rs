//! Execution-only workflow driver, restart classification and the single
//! run coordinator (v1.190 P3-T1; extracted from the daemon's `preset_run`).
//!
//! This is the missing seam the daemon used to own privately: a bounded,
//! cancellable step-loop that drives a session via
//! [`OrchestrationEngine::run_step`] until terminal, error, or an
//! external-input stop, plus the ONE cancellation/join owner per session.
//!
//! Every Host-plane dependency is an injected port — the execution cohort
//! links no agent host and no daemon transport:
//!
//! - provider references in agent bindings are validated through
//!   [`ProviderCatalogPort`];
//! - per-run SSE ring reservation/publication goes through [`RunEventPort`].
//!
//! Both ports default to "absent", which reproduces exactly the daemon's
//! unwired behaviour (provider validation skipped, no SSE ring).
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

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use graph_flow::SessionStorage;
use nexus_orchestration::engine::{
    EngineError, EngineSignal, FailedStepWitness, FailurePersistenceDisposition, FailureSettlement,
    FailureSettlementSummary, OrchestrationEngine, SessionId, SessionStatus, SessionSummary,
    StepOutcome,
};
use nexus_orchestration::resume_rules;
use nexus_orchestration::run_state::{RunRecord, WorkflowStateStore};
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use tokio_util::sync::CancellationToken;

/// Per-run live-ring registry (v1.188 P4 §6.3).
///
/// The execution layer reserves and releases a run's ring and publishes the
/// durable run state; bounded item/byte/subscriber accounting, the shared
/// sink map and SSE framing stay wholly with the transport owner. Registering
/// inside the port keeps core from naming a host event or a sink type, and
/// keeps the hot event path free of any serialization.
///
/// `try_register_live` is the RESERVATION: `true` when a slot is free or this
/// run already owns a ring, `false` when the quota is exhausted — the drive
/// is then refused with a typed capacity error before any durable work is
/// touched (QC2 F-004).
#[async_trait]
pub trait RunEventPort: Send + Sync {
    /// Reserve (or reuse) the run's live ring.
    async fn try_register_live(&self, run_id: &str) -> bool;
    /// Release the run's reserved sink from the shared map.
    async fn remove_live(&self, run_id: &str);
    /// Publish the durable run record to the run's live ring.
    fn publish_run_state(&self, run_id: &str, record: &RunRecord);
    /// Close the run's ring as authoritative-terminal.
    fn mark_terminal(&self, run_id: &str);
}

/// Provider catalog used to validate agent-binding provider references
/// before a run is enqueued (N-4).
///
/// The execution layer must not link the Host plane, so it asks this port
/// whether a provider id is known rather than reading a host catalog.
#[async_trait]
pub trait ProviderCatalogPort: Send + Sync {
    /// Whether `provider_id` exists in the live provider catalog.
    ///
    /// # Errors
    /// Returns the catalog error text when the catalog itself is unavailable
    /// (never a silent "not found").
    async fn provider_available(&self, provider_id: &str) -> Result<bool, String>;
}

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
    Failed {
        steps: u32,
        error: String,
        settlement: FailureSettlementSummary,
    },
    /// A concurrent authoritative writer won the graph-flow OCC race while
    /// this drive was in flight. The losing drive stops without cancel,
    /// failure-context persistence, or coordinator durable-failure fencing.
    SessionConflict { steps: u32, error: String },
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
/// - A witnessed v1 engine error commits its failure, then cleans up owned
///   resources and settles durable `Failed`. Lost ownership or unconfirmed
///   persistence fences the owner instead of claiming settlement. Legacy
///   no-store handling retains best-effort session-context evidence.
/// - The `cancel` token is checked before every step, including before the
///   first.
///
/// `steps` counts `run_step` invocations performed by THIS call (a re-drive
/// starts at 0).
pub async fn drive_preset_run(
    engine: &dyn OrchestrationEngine,
    storage: Option<&Arc<dyn SessionStorage>>,
    workflow_store: Option<&Arc<dyn WorkflowStateStore>>,
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
            // Cancelled/Interrupted are terminal — never auto-driven (A2).
            Ok(SessionStatus::Cancelled | SessionStatus::Interrupted) => {
                return PresetRunOutcome::Cancelled { steps };
            }
            Ok(SessionStatus::WaitingForInput) if !config.resume_waiting => {
                return PresetRunOutcome::WaitingForInput { steps };
            }
            Ok(SessionStatus::WaitingForInput | SessionStatus::Running | SessionStatus::Paused) => {
            }
            Err(e) => {
                let error = format!("get_status failed: {e}");
                let settlement =
                    settle_drive_failure(engine, workflow_store, storage, session_id, &error, None)
                        .await;
                return failed_outcome(steps, error, &settlement);
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
            // graph-flow 0.8 conflict disposition: a SessionConflict means a
            // concurrent authoritative writer (control cancel, settle, or a
            // winning drive) owns the row. This drive LOST — stop it without
            // the blanket failure path: no Cancel signal and no failure
            // context save against the winner's state. The durable row keeps
            // the winner's status/revision; if this step's in-flight marker
            // persists, A7 recovery classifies it as interrupted.
            Err(e @ EngineError::GraphFlow(graph_flow::GraphError::SessionConflict(_))) => {
                let error = e.to_string();
                tracing::warn!(
                    session_id = %session_id.0,
                    error = %error,
                    "preset-run driver: losing drive stopped on session conflict                      (concurrent authoritative writer won)"
                );
                return PresetRunOutcome::SessionConflict {
                    steps: steps.saturating_add(1),
                    error,
                };
            }
            Err(e) => {
                let error = e.to_string();
                let witness = e.step_witness().cloned();
                let settlement = settle_drive_failure(
                    engine,
                    workflow_store,
                    storage,
                    session_id,
                    &error,
                    witness.as_ref(),
                )
                .await;
                return failed_outcome(steps.saturating_add(1), error, &settlement);
            }
        }
    }
}

fn failed_outcome(steps: u32, error: String, settlement: &FailureSettlement) -> PresetRunOutcome {
    if settlement.authoritative == FailurePersistenceDisposition::OwnershipLost
        || settlement.cleanup == FailurePersistenceDisposition::OwnershipLost
    {
        return PresetRunOutcome::SessionConflict { steps, error };
    }
    PresetRunOutcome::Failed {
        steps,
        error,
        settlement: FailureSettlementSummary::from_settlement(settlement),
    }
}

/// Authoritative + context failure persistence with anchored cleanup.
async fn settle_drive_failure(
    engine: &dyn OrchestrationEngine,
    workflow_store: Option<&Arc<dyn WorkflowStateStore>>,
    storage: Option<&Arc<dyn SessionStorage>>,
    session_id: &SessionId,
    error: &str,
    witness: Option<&FailedStepWitness>,
) -> FailureSettlement {
    let authoritative = record_driver_failure(workflow_store, session_id, error, witness).await;
    // Two DISTINCT legacy shapes, both selected from persisted facts rather
    // than from the presence of a store handle:
    //
    // - `no_store_legacy`: no workflow store at all — the historical engine
    //   contract, where an ordinary `Cancel` is the cleanup and (with no
    //   store) is a side-effect-free success.
    // - `legacy_context_only`: a persisted v0 row DRIVEN WITH a store. A v1
    //   cancel would attempt a workflow transition the v0 row cannot accept
    //   (and must never be promoted), so cleanup is explicitly context-only:
    //   the durable `_run_status`/`_run_error` pair is the failure evidence,
    //   A7 recovery skips the row, and the owner is fenced.
    let no_store_legacy = workflow_store.is_none();
    let legacy_context_only = authoritative.legacy_v0;
    let context = if no_store_legacy || legacy_context_only {
        persist_failure(storage, session_id, error).await
    } else if authoritative.disposition == FailurePersistenceDisposition::Committed {
        FailurePersistenceDisposition::Committed
    } else {
        FailurePersistenceDisposition::NoAuthority
    };
    let cleanup = if legacy_context_only {
        if context.permits_terminal_cleanup() {
            FailurePersistenceDisposition::LegacyContextOnly
        } else {
            FailurePersistenceDisposition::NoAuthority
        }
    } else if no_store_legacy {
        if context.permits_terminal_cleanup() {
            cleanup_failed_run_legacy(engine, session_id).await
        } else {
            FailurePersistenceDisposition::NoAuthority
        }
    } else if authoritative.disposition.permits_terminal_cleanup()
        && context.permits_terminal_cleanup()
    {
        // The anchor is the commit's OWN returned clocks: the coupled
        // failure commit advanced the workflow revision AND the graph clock,
        // and no later read may substitute for it.
        match &authoritative.committed {
            Some(record) => {
                cleanup_failed_run(
                    engine,
                    session_id,
                    record.state_revision,
                    record.graph_version,
                )
                .await
            }
            // A retry/duplicate that found the failure already durable owns
            // no commit clocks, so it must not cancel anything.
            None => FailurePersistenceDisposition::NoAuthority,
        }
    } else {
        FailurePersistenceDisposition::NoAuthority
    };
    FailureSettlement {
        witness: witness.cloned(),
        authoritative: authoritative.disposition,
        context,
        cleanup,
    }
}
/// Outcome of the authoritative v1 failure write: the disposition plus the
/// commit's own [`RunRecord`] when THIS call's coupled commit succeeded
/// (`None` when nothing was written by this call, e.g. a duplicate
/// settlement or a lost/absent authority). Downstream anchors (cleanup,
/// terminal write) must use these returned clocks — never a fresh load.
struct AuthoritativeSettlement {
    disposition: FailurePersistenceDisposition,
    committed: Option<nexus_orchestration::run_state::RunRecord>,
    /// True ONLY when the durable row was positively read as a legacy v0
    /// (`execution_version < 1`) run. The v0 context-only failure contract is
    /// selected from this persisted fact, never from whether a workflow-store
    /// handle happens to be present.
    legacy_v0: bool,
}

const fn authoritative_failed(
    disposition: FailurePersistenceDisposition,
) -> AuthoritativeSettlement {
    AuthoritativeSettlement {
        disposition,
        committed: None,
        legacy_v0: false,
    }
}

fn failure_authority_rejection(
    record: &nexus_orchestration::run_state::RunRecord,
    witness: &FailedStepWitness,
) -> Option<AuthoritativeSettlement> {
    if record.state.is_none() {
        return Some(authoritative_failed(
            FailurePersistenceDisposition::PersistenceFailed,
        ));
    }
    let has_failure = record
        .state
        .as_ref()
        .and_then(|state| state.failure.as_ref())
        .is_some();
    if record.status.is_terminal() {
        return Some(authoritative_failed(if has_failure {
            FailurePersistenceDisposition::ExternalTerminalWinner
        } else {
            FailurePersistenceDisposition::OwnershipLost
        }));
    }
    if record.state_revision != witness.owned_revision
        || witness
            .owned_graph_version
            .is_some_and(|version| record.graph_version != version)
    {
        return Some(authoritative_failed(
            FailurePersistenceDisposition::OwnershipLost,
        ));
    }
    // A duplicate owns no commit clocks, so it cannot authorize cleanup.
    has_failure.then(|| authoritative_failed(FailurePersistenceDisposition::Committed))
}

/// Persist the authoritative v1 [`RunFailure`] at the witnessed revision and
/// graph clock, coupling the legacy context keys in the same transaction.
async fn record_driver_failure(
    workflow_store: Option<&Arc<dyn WorkflowStateStore>>,
    session_id: &SessionId,
    error: &str,
    witness: Option<&FailedStepWitness>,
) -> AuthoritativeSettlement {
    let no_authority = AuthoritativeSettlement {
        disposition: FailurePersistenceDisposition::NoAuthority,
        committed: None,
        legacy_v0: false,
    };
    let Some(store) = workflow_store else {
        return no_authority;
    };
    // Classify the PERSISTED shape first: the legacy v0 contract is selected
    // from the durable execution version, so it holds even when this attempt
    // has no step witness (a scripted/legacy runner failure) — and never from
    // the mere presence of a workflow-store handle. This read only classifies;
    // authority comes exclusively from the CAS the coupled commit performs on
    // the witness clocks.
    match store.load_run(session_id).await {
        Ok(Some(record)) if record.execution_version < 1 => {
            return AuthoritativeSettlement {
                disposition: FailurePersistenceDisposition::NoAuthority,
                committed: None,
                legacy_v0: true,
            };
        }
        // An absent or unreadable row is never classified as legacy.
        _ => {}
    }
    let Some(witness) = witness else {
        return no_authority;
    };
    let Ok(Some(record)) = store.load_run(session_id).await else {
        return authoritative_failed(FailurePersistenceDisposition::PersistenceFailed);
    };
    if record.execution_version < 1 {
        return AuthoritativeSettlement {
            disposition: FailurePersistenceDisposition::NoAuthority,
            committed: None,
            legacy_v0: true,
        };
    }
    if let Some(rejection) = failure_authority_rejection(&record, witness) {
        return rejection;
    }
    let mut next_state = record.state.clone().expect("v1 state checked above");
    next_state.in_flight = None;
    next_state.step_in_flight = None;
    next_state.failure = Some(nexus_orchestration::run_state::RunFailure {
        code: "driver_failed".to_string(),
        message: error.to_string(),
    });
    let root_with_failure = witness.pre_step_root.clone();
    if let Err(e) = root_with_failure
        .context
        .set("_run_status", "failed")
        .and_then(|()| {
            root_with_failure
                .context
                .set("_run_error", error.to_string())
        })
    {
        tracing::warn!(
            session_id = %session_id.0,
            error = %e,
            "preset-run driver: coupled failure context write failed"
        );
        return authoritative_failed(FailurePersistenceDisposition::PersistenceFailed);
    }
    let checkpoint = nexus_orchestration::run_state::RunCheckpoint {
        root: &root_with_failure,
        children: &[],
    };
    match store
        .commit_transition_with_graph_fence(
            session_id,
            witness.owned_revision,
            witness.owned_graph_version,
            checkpoint,
            witness.pre_step_status.clone(),
            &next_state,
        )
        .await
    {
        Ok(record) => AuthoritativeSettlement {
            disposition: FailurePersistenceDisposition::Committed,
            committed: Some(record),
            legacy_v0: false,
        },
        Err(EngineError::RevisionMismatch { .. } | EngineError::TerminalState(_)) => {
            authoritative_failed(FailurePersistenceDisposition::OwnershipLost)
        }
        Err(e) => {
            tracing::warn!(
                session_id = %session_id.0,
                error = %e,
                "preset-run driver: could not persist authoritative failure record"
            );
            authoritative_failed(FailurePersistenceDisposition::PersistenceFailed)
        }
    }
}

/// Anchored failed-step cleanup. The anchor is the coupled commit's OWN
/// returned clocks (`commit_revision` / `commit_graph_version`) — the
/// cleanup path never loads the row, so a winner that commits after the
/// failure write can never be adopted as the cancel target.
async fn cleanup_failed_run(
    engine: &dyn OrchestrationEngine,
    session_id: &SessionId,
    commit_revision: u64,
    commit_graph_version: u64,
) -> FailurePersistenceDisposition {
    match engine
        .signal(
            session_id,
            EngineSignal::FailAnchored {
                expected_revision: commit_revision,
                expected_graph_version: commit_graph_version,
            },
        )
        .await
    {
        // The signal's `Ok` is a claim, not proof: mechanically confirm the
        // run actually reached Failed. Another terminal status is not proof
        // of successful failure settlement and must not release dependencies.
        Ok(()) => match engine.get_status(session_id).await {
            Ok(SessionStatus::Failed) => FailurePersistenceDisposition::Committed,
            Ok(status) => {
                tracing::warn!(
                    session_id = %session_id.0,
                    status = ?status,
                    "preset-run driver: anchored cleanup returned success without a Failed \
                     outcome; fencing instead of claiming failure settlement"
                );
                FailurePersistenceDisposition::PersistenceFailed
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %session_id.0,
                    error = %e,
                    "preset-run driver: could not verify anchored cleanup terminal proof"
                );
                FailurePersistenceDisposition::PersistenceFailed
            }
        },
        Err(EngineError::RevisionMismatch { .. } | EngineError::TerminalState(_)) => {
            FailurePersistenceDisposition::OwnershipLost
        }
        Err(e) => {
            tracing::warn!(
                session_id = %session_id.0,
                error = %e,
                "preset-run driver: anchored cleanup failed"
            );
            FailurePersistenceDisposition::PersistenceFailed
        }
    }
}

/// Legacy v0 cleanup path: ordinary cancel after context-only failure record.
async fn cleanup_failed_run_legacy(
    engine: &dyn OrchestrationEngine,
    session_id: &SessionId,
) -> FailurePersistenceDisposition {
    match engine.signal(session_id, EngineSignal::Cancel).await {
        Ok(()) => FailurePersistenceDisposition::Committed,
        Err(EngineError::RevisionMismatch { .. } | EngineError::TerminalState(_)) => {
            FailurePersistenceDisposition::OwnershipLost
        }
        Err(e) => {
            tracing::warn!(
                session_id = %session_id.0,
                error = %e,
                "preset-run driver: legacy cleanup failed"
            );
            FailurePersistenceDisposition::PersistenceFailed
        }
    }
}

/// Persist the failure record into the session context (legacy v0 path).
async fn persist_failure(
    storage: Option<&Arc<dyn SessionStorage>>,
    session_id: &SessionId,
    error: &str,
) -> FailurePersistenceDisposition {
    let Some(storage) = storage else {
        return FailurePersistenceDisposition::NoAuthority;
    };
    let Ok(Some(session)) = storage.get(&session_id.0).await else {
        return FailurePersistenceDisposition::PersistenceFailed;
    };
    if let Err(e) = session
        .context
        .set("_run_status", "failed")
        .and_then(|()| session.context.set("_run_error", error.to_string()))
    {
        tracing::warn!(
            session_id = %session_id.0,
            error = %e,
            "preset-run driver: failure-record context write failed; aborting persistence"
        );
        return FailurePersistenceDisposition::PersistenceFailed;
    }
    if let Err(e) = storage.save(session).await {
        tracing::warn!(
            session_id = %session_id.0,
            error = %e,
            "preset-run driver: failed to persist failure record"
        );
        return FailurePersistenceDisposition::PersistenceFailed;
    }
    FailurePersistenceDisposition::Committed
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
    /// Skipped: the per-run live event-ring quota is exhausted (QC2 F-004).
    /// The session stays tracked-but-not-driven with its durable record
    /// untouched; a later boot/retry reserves a freed slot.
    SkippedRunEventCapacity { session_id: SessionId },
    /// Skipped: the persisted session could not be loaded from storage.
    SkippedUnreadable {
        session_id: SessionId,
        error: String,
    },
    /// Skipped: the v1 durable record classifies Terminal (A7 rule 1).
    /// Authoritative terminal wins over a stale summary — even one the
    /// boot filter listed as running with live join keys — so the session
    /// is skipped immediately and can never fall through to legacy chain
    /// evidence or a re-drive.
    SkippedTerminal { session_id: SessionId },
    /// Skipped: the v1 durable record classifies Unreadable (A7 rule 2) —
    /// corrupt/unsupported metadata is non-replayable and skipped
    /// immediately, distinct from a storage load failure
    /// ([`ResumeDecision::SkippedUnreadable`]).
    SkippedUnreadableMetadata { session_id: SessionId },
    /// Skipped: the v1 durable record classifies as interrupted/uncertain
    /// in-flight work (A7 rule 3) — never auto-retried, even when old
    /// converge/merge join keys exist.
    SkippedInterrupted { session_id: SessionId },
    /// Skipped: the v1 durable record is `waiting_for_input` outside a
    /// converge/merge chain (A7 rule 4 — human wait). The A4 wait token is
    /// preserved; never stepped or approved at boot.
    SkippedHumanWait { session_id: SessionId },
    /// Skipped: the v1 durable record sits at a fully committed boundary
    /// with no in-flight work (A7 rule 6 — safe boundary, reconstructable).
    /// Boot does not auto-drive outside the converge/merge chain class.
    SkippedSafeBoundary { session_id: SessionId },
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
/// 1. It is non-terminal AND its durable v1 record (when a workflow store
///    is present) classifies as a live converge/merge chain — the
///    authoritative v1 status/state govern the recovery class. A durable
///    `Terminal`/`Unreadable`/`Interrupted`/`HumanWait`/`SafeBoundary`
///    class skips immediately and never falls through to legacy chain
///    evidence or a re-drive (A7 rules 1–6). v0 rows fall through to the
///    conservative legacy cascade below (rule 7).
/// 2. Its persisted context carries NO typed-failure record
///    (`_run_status` / `_run_error`) — v0/no-store rows only: a v1
///    `ConvergeMerge` row is classified by the authoritative A7 gate, so
///    stale context failure keys cannot suppress its re-drive.
/// 3. It is not a human wait: `waiting_for_input` without a live
///    converge/merge join key is never stepped at boot (A7 rule 4 — the
///    A4 wait token is preserved, not auto-advanced).
/// 4. Its persisted context carries a LIVE join-tracking key
///    (`_converge_arrivals_*`, `_merge_*`, or `_join_wait_start_*`) with a
///    non-Null value — written ONLY by merge/converge gate states. The
///    gates clear their keys by writing `Value::Null` (deadline exceeded /
///    success-leave), so a session whose join keys are all Null has LEFT
///    the chain and is not re-driven. This is the converge/merge chain
///    class: a `manual`/`llm_judge` wait (no live join keys) is never
///    auto-advanced, and sessions without the class behave
///    byte-identically to pre-T2 boot.
/// 5. A `FlowRunner` exists for the session (`engine.has_runner`) — the
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
#[allow(clippy::too_many_lines)] // linear recovery sweep classifying each persisted session; per-class helpers would fragment the order
pub async fn resume_driven_sessions(
    engine: &dyn OrchestrationEngine,
    storage: &Arc<dyn SessionStorage>,
    workflow_store: Option<&Arc<dyn WorkflowStateStore>>,
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
        //    typed-failed join. v1 typed failures are already excluded by the
        //    boot recovery filter (status `failed` is terminal); this context-
        //    key check is the conservative legacy (v0) projection — the DB
        //    status column stays `running` after a typed v0 failure, so the
        //    context keys are the only reliable failure record. Shared with
        //    `nexus42 ops inspect` (single source of truth, qc1 W1).
        let context_value = serde_json::to_value(&session.context).ok();
        let data = context_value.as_ref().and_then(resume_rules::context_data);

        // A7 v1 gate (v1.186 P0, Task 2): when the durable workflow store is
        // present, the authoritative v1 status/state govern the recovery
        // class. Interrupted/uncertain in-flight work is never re-driven
        // (even with old join keys — rule 3); human waits are never stepped
        // at boot (rule 4); a fully committed safe boundary is
        // reconstructable but not auto-driven outside the chain class
        // (rule 6); corrupt/unsupported v1 metadata is unreadable/
        // non-replayable (rule 2). Only the converge/merge class proceeds to
        // the bounded join re-drive below. v0 rows and in-memory engines
        // (no store) fall through to the conservative legacy cascade.
        //
        // Round-2 fix (Critical 1): `Terminal` and `Unreadable` skip
        // immediately with their own decisions instead of falling through —
        // the legacy chain filter must never see them. A stale summary
        // (listed running / join-resumable while the durable `load_run`
        // reports terminal, e.g. the status transitioned after the list)
        // may carry live join keys; falling through would let old join keys
        // reach `drive_preset_run` and re-execute a terminal session. The
        // legacy cascade is reserved for exact `LegacyUnverified` (v0 /
        // no-store) rows.
        //
        // Round-2 fix (Important): a valid v1 `ConvergeMerge` class must
        // also bypass the v0 typed-failure context guards below — stale
        // `_run_status`/`_run_error` keys in the session context cannot
        // suppress the durable A7 converge/merge re-drive. The legacy
        // cascade runs only for v0 / no-store rows.
        let mut v1_converge_merge = false;
        let mut v1_safe_boundary = false;
        if let Some(store) = workflow_store {
            match store.load_run(&session_id).await {
                Ok(Some(record)) if record.execution_version >= 1 => {
                    let gate_park = data.is_some_and(|data| {
                        resume_rules::gate_park_live(
                            data,
                            summary.current_task_id.as_deref().unwrap_or_default(),
                        )
                    });
                    let class = resume_rules::classify_recovery(
                        &record.status,
                        record.state.as_ref(),
                        gate_park,
                    );
                    match class {
                        resume_rules::RecoveryClass::ConvergeMerge => {
                            v1_converge_merge = true;
                        }
                        resume_rules::RecoveryClass::Terminal => {
                            decisions.push(ResumeDecision::SkippedTerminal { session_id });
                            continue;
                        }
                        resume_rules::RecoveryClass::Unreadable => {
                            decisions
                                .push(ResumeDecision::SkippedUnreadableMetadata { session_id });
                            continue;
                        }
                        resume_rules::RecoveryClass::Interrupted => {
                            decisions.push(ResumeDecision::SkippedInterrupted { session_id });
                            continue;
                        }
                        resume_rules::RecoveryClass::HumanWait => {
                            decisions.push(ResumeDecision::SkippedHumanWait { session_id });
                            continue;
                        }
                        // A7 rule 6: a durable v1 run at a fully committed
                        // safe boundary is reconstructed and re-driven —
                        // never stranded by a restart (P3 T1 rereview P1).
                        resume_rules::RecoveryClass::SafeBoundary => {
                            if let Err(e) = engine.ensure_recovered_runner(&session_id).await {
                                tracing::warn!(
                                    session_id = %session_id.0,
                                    error = %e,
                                    "recovery: safe-boundary reconstruction failed; \
                                     treating as non-replayable metadata"
                                );
                                decisions
                                    .push(ResumeDecision::SkippedUnreadableMetadata { session_id });
                                continue;
                            }
                            v1_safe_boundary = true;
                        }
                        // Exact-v0 rows never appear here (`execution_version
                        // >= 1` guard matched); the arms above cover all seven
                        // classes, so `LegacyUnverified` is unreachable.
                        resume_rules::RecoveryClass::LegacyUnverified => {
                            unreachable!("v1 load_run record cannot classify LegacyUnverified")
                        }
                    }
                }
                Ok(_) => {}
                Err(e) => {
                    decisions.push(ResumeDecision::SkippedUnreadable {
                        session_id,
                        error: e.to_string(),
                    });
                    continue;
                }
            }
        }
        // 2. Legacy (v0 / no-store) typed-failure filter: never re-tick a
        //    typed-failed join. A v1 ConvergeMerge row was already classified
        //    by the authoritative A7 gate above — stale `_run_status` /
        //    `_run_error` context keys must not suppress its re-drive.
        if !v1_converge_merge
            && !v1_safe_boundary
            && data.is_some_and(resume_rules::typed_failure_keys_present)
        {
            decisions.push(ResumeDecision::SkippedTypedFailed { session_id });
            continue;
        }

        // 3. Human wait (v0 / no-store): a non-chain wait is never stepped at
        //    boot; the A4 wait token is preserved, not auto-advanced.
        if !v1_converge_merge
            && !v1_safe_boundary
            && matches!(summary.status, SessionStatus::WaitingForInput)
            && !data.is_some_and(resume_rules::is_converge_merge_chain)
        {
            decisions.push(ResumeDecision::SkippedNotConvergeMergeClass { session_id });
            continue;
        }

        // 4. Converge/merge chain class filter (no-checkpoint default
        //    equivalence): only sessions provably inside a converge/merge
        //    chain are re-driven.
        if !v1_converge_merge
            && !v1_safe_boundary
            && !data.is_some_and(resume_rules::is_converge_merge_chain)
        {
            decisions.push(ResumeDecision::SkippedNotConvergeMergeClass { session_id });
            continue;
        }

        // 5. Runner-existence filter: a session whose runner failed
        //    reconstruction stays tracked-but-not-driven.
        if !engine.has_runner(&session_id).await {
            decisions.push(ResumeDecision::SkippedNoRunner { session_id });
            continue;
        }

        // 6. Re-drive from the persisted position.
        let outcome = drive_preset_run(
            engine,
            Some(storage),
            workflow_store.map(|s| s as &Arc<dyn WorkflowStateStore>),
            &session_id,
            config,
            cancel,
        )
        .await;
        decisions.push(ResumeDecision::ReDriven {
            session_id,
            outcome,
        });
    }
    decisions
}

// ---------------------------------------------------------------------------
// WorkflowRunCoordinator (v1.186 P2 T1, A3)
// ---------------------------------------------------------------------------

/// Error type for coordinator admission/control operations (A3).
#[derive(Debug, thiserror::Error)]
pub enum RunControlError {
    /// The schedule row does not exist.
    #[error("schedule {0} not found")]
    ScheduleNotFound(String),
    /// The schedule is not eligible for driven admission (legacy/system
    /// inert, terminal, or already running with an owned session).
    #[error("schedule {0} is not eligible for driven admission: {1}")]
    NotEligible(String, String),
    /// The preset could not be resolved (embedded or directory bundle).
    #[error("preset resolution failed for '{0}': {1}")]
    PresetLoad(String, String),
    /// The engine/store refused the run admission.
    #[error("run admission failed: {0}")]
    Admission(String),
    /// The engine/store failed while driving the run.
    #[error("drive failed: {0}")]
    Drive(String),
    /// The schedule row could not be updated to the driven state.
    #[error("schedule state update failed: {0}")]
    ScheduleUpdate(String),
    /// The daemon has no active creator DB / engine (Tier-0 boot).
    #[error("no active creator workspace: {0}")]
    NoWorkspace(String),
    /// A human-wait continuation token was stale, consumed, or wrong (A4).
    #[error(
        "wait conflict for session {session_id}: current status {status}, current_wait_id {current_wait_id:?}"
    )]
    WaitConflict {
        /// Session id.
        session_id: String,
        /// Current persisted status.
        status: String,
        /// Current durable wait id (null when the run is not waiting).
        current_wait_id: Option<String>,
    },
    /// The run is in a state that does not accept the requested signal.
    #[error("workflow state conflict for session {0}: {1}")]
    StateConflict(String, String),
    /// The per-run live event-ring quota is exhausted (QC2 F-004). No drive
    /// was started and no durable work was touched — the caller may retry
    /// once a live run closes its ring.
    #[error("run event capacity exhausted for session {0}")]
    RunEventCapacity(String),
    /// The run's source preset cannot be reconstructed (missing/changed/
    /// moved source or corrupt child identity — A7 rule 4). The human wait
    /// is preserved, legal actions are cancel-only, and continue returns
    /// this conflict instead of driving a session whose frozen source can
    /// no longer be reattached.
    #[error(
        "reconstruction_unavailable for session {session_id}: {reason} — \
         human wait preserved, cancel-only"
    )]
    ReconstructionUnavailable {
        /// Session id.
        session_id: String,
        /// Machine-stable reason (source missing/hash mismatch/child corrupt).
        reason: String,
    },
}

/// Public control signal for a driven run (A4/A5).
///
/// The coordinator routes the signal to the engine's revision-fenced
/// transition path and re-drives the run when the signal makes it runnable.
#[derive(Debug, Clone)]
pub enum RunSignal {
    /// Authorized continuation of a human wait (A4). `wait_id` must match
    /// the durable wait token exactly.
    Continue { wait_id: String },
    /// Cancel the run (A5): fences later steps, reaches the Host/process
    /// owner, and persists `Cancelled` only when stop is confirmed.
    Cancel,
    /// Pause the run.
    Pause,
    /// Resume a non-human pause.
    Resume,
}

/// Outcome of [`WorkflowRunCoordinator::signal_run`].
#[derive(Debug, Clone)]
pub struct RunControlResult {
    /// Persisted status after the signal.
    pub status: String,
    /// Current durable wait id (null when the run is not waiting).
    pub current_wait_id: Option<String>,
    /// Typed Cancel projection (QC2 F-002). `None` for non-cancel signals.
    pub cancel_outcome: Option<CancelOutcome>,
}

/// Typed projection of a Cancel signal's durable outcome (QC2 F-002).
///
/// Resolved by the coordinator from the durable record — never derived from
/// an error string — so every public cancel surface (session signal and
/// schedule signal) projects exactly the same truth.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CancelOutcome {
    /// Durable terminal `cancelled`: cleanup confirmed (or an idempotent
    /// re-cancel of an already-cancelled run).
    Confirmed,
    /// Durable `interrupted` with the cancel intent: cleanup unconfirmed.
    /// The run stays actionable and a retry cancel can still confirm it.
    Unconfirmed,
    /// Durable `failed` carrying the cancel intent and a `driver_failed`
    /// record: the cancel converged on the driver failure the run already
    /// committed.
    Failed,
}

impl CancelOutcome {
    /// Stable wire label for public responses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Confirmed => "confirmed",
            Self::Unconfirmed => "unconfirmed",
            Self::Failed => "failed",
        }
    }
}

/// Typed classification of a run's durable cancel outcome (QC2 F-002).
///
/// A `Cancel` signal whose goal is durably accomplished must report the
/// persisted outcome as success instead of a conflict that would contradict
/// the durable truth (A5 fence-loss idempotence), and an unconfirmed cleanup
/// must project `interrupted` rather than a 500. A run that settled
/// `completed`/`failed`-without-cancel-intent (or is still live) has no
/// cancel outcome: the cancel genuinely did not happen.
fn classify_cancel_outcome(
    record: &nexus_orchestration::run_state::RunRecord,
) -> Option<CancelOutcome> {
    if !nexus_orchestration::run_state::durable_cancel_outcome_accomplished(record) {
        return None;
    }
    match record.status {
        SessionStatus::Cancelled => Some(CancelOutcome::Confirmed),
        SessionStatus::Interrupted => Some(CancelOutcome::Unconfirmed),
        SessionStatus::Failed => Some(CancelOutcome::Failed),
        _ => None,
    }
}

/// One cancellation/join owner per (Creator DB identity, session) (A3).
///
/// The coordinator is the SINGLE production owner of the bounded
/// [`drive_preset_run`] loop for newly admitted creator runs and schedule
/// admissions. `ensure_driving` is single-flight per session: a second
/// caller observes the same owned run instead of starting a second driver.
///
/// Cancellation is out-of-band: the per-run token is registered in the
/// shared `session_cancels` map (the engine's prompt consumers resolve it)
/// and the drive loop checks it before every step.
#[derive(Clone)]
pub struct WorkflowRunCoordinator {
    /// The daemon's concrete orchestration engine (production
    /// `GraphFlowEngine`). Concrete so schedule admission can freeze the
    /// descriptor/input via `admit_schedule_run_with_input`.
    engine: Arc<nexus_orchestration::GraphFlowEngine>,
    /// Durable session storage (the SQLite adapter also implements the
    /// workflow store; used for failure-record persistence in the drive).
    storage: Arc<dyn SessionStorage>,
    /// The creator-DB pool the coordinator's durable store reads through
    /// (used for durable-state gating in `ensure_driving`, I-2).
    pool: Arc<sqlx::SqlitePool>,
    /// The daemon's Host plane (A1) — used to validate provider references
    /// in agent bindings before enqueue (N-4). `None` when no Host facade
    /// is wired (tests): provider validation is skipped then.
    provider_catalog: Option<Arc<dyn ProviderCatalogPort>>,
    /// The daemon's configured default binding provider (C-3): the
    /// sanctioned source for deriving agent bindings when an explicit
    /// legacy start prepares a historical row with no frozen descriptor.
    /// Mirrors the supervisor's `binding_provider` (the same source the
    /// internal insertion paths use).
    binding_provider: Option<String>,
    /// Shared per-run cancellation tokens (A1) — the same map the engine
    /// registers run tokens in and the prompt executor resolves.
    session_cancels: std::sync::Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
    /// Active drive owners: session id → cancellation token + join handle.
    /// One owner per session; re-entry returns the same run.
    drives: Arc<tokio::sync::Mutex<std::collections::HashMap<String, DriveOwner>>>,
    /// Fail-closed fence (closure 8): session ids whose drive loop failed
    /// AND whose durable non-replayable `Failed` transition could not be
    /// committed. `ensure_driving` refuses re-entry for these sessions
    /// until the transition is durable — a failed driver is never silently
    /// re-driven when the failure write itself failed.
    failed_owners: Arc<tokio::sync::Mutex<std::collections::HashSet<String>>>,
    /// The workflow state store every coordinator v1 read/write goes
    /// through (durable gate, drive loop, failure settlement, terminal
    /// write). Construction-defaults to the SQLite store over `pool`;
    /// tests inject a delegating wrapper to fault/race REAL store
    /// operations at the commit/marker/settle boundaries.
    workflow_store: Arc<dyn WorkflowStateStore>,
    /// Bounds for each drive call.
    config: PresetRunConfig,
    /// The schedule supervisor for terminal settlement (T3, A3). Set at
    /// boot / lazy attach AFTER the supervisor is constructed (the
    /// supervisor needs the coordinator's `ScheduleRunStarter`, so the
    /// handle is attached post-construction). `None` in tests / standalone
    /// coordinator: settlement is skipped.
    schedule_supervisor: std::sync::Arc<
        std::sync::RwLock<
            Option<Arc<nexus_orchestration::schedule::supervisor::ScheduleSupervisor>>,
        >,
    >,
    /// Bounded per-run SSE rings (P4 §6.3). `None` when the transport
    /// supplies no ring registry (tests, core-only callers).
    run_events: Option<Arc<dyn RunEventPort>>,
}

/// An in-flight (or completed) drive owner for one session.
struct DriveOwner {
    cancel: tokio_util::sync::CancellationToken,
    join: tokio::task::JoinHandle<()>,
}

impl WorkflowRunCoordinator {
    /// Abort every in-flight drive owner (daemon-level restart seam, A7).
    ///
    /// Fires each owner's cancellation token and awaits the drive loops so
    /// the restart boundary is quiescent: a subsequent
    /// [`WorkflowRunCoordinator::recover_persisted`] re-registers every
    /// still-valid owner from the durable records (the A7 recovery order
    /// decides which runs are re-driven). Idempotent — owners already
    /// finished are skipped.
    pub async fn abort_all_drives(&self) {
        let owners: Vec<(
            String,
            tokio_util::sync::CancellationToken,
            tokio::task::JoinHandle<()>,
        )> = {
            let mut drives = self.drives.lock().await;
            drives
                .drain()
                .map(|(sid, owner)| (sid, owner.cancel, owner.join))
                .collect()
        };
        for (sid, cancel, join) in owners {
            cancel.cancel();
            let _ = join.await;
            self.session_cancels
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .remove(&sid);
        }
    }

    /// Recover persisted non-terminal sessions after a daemon-level restart
    /// (A7) and re-drive the eligible converge/merge class through this
    /// coordinator.
    ///
    /// Identical to the production boot recovery path: the engine's runners
    /// are reconstructed from the frozen source identity (manifest + every
    /// referenced template hash verified; changed/missing source or corrupt
    /// child identity is non-replayable), then the shared A7 classifier
    /// decides each session's class — terminal/interrupted/human-wait/
    /// unreadable/safe-boundary skip, only converge/merge is re-driven
    /// from its persisted position (completed edges never re-executed).
    ///
    /// `shutdown_notify` (when present) aborts the background re-drive when
    /// the daemon shuts down, exactly like the boot path.
    ///
    /// Returns every [`ResumeDecision`] so the caller can assert the exact
    /// A7 outcome per session.
    pub async fn recover_persisted(
        &self,
        sqlite_storage: &Arc<SqliteSessionStorage>,
        shutdown_notify: Option<Arc<tokio::sync::Notify>>,
    ) -> Vec<ResumeDecision> {
        let summaries = sqlite_storage
            .list_non_terminal_sessions()
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(
                    "recover_persisted: failed to list non-terminal sessions: {}",
                    e
                );
                Vec::new()
            });
        if summaries.is_empty() {
            return Vec::new();
        }
        tracing::info!(
            "recover_persisted: recovering {} persisted session(s) into in-memory tracker",
            summaries.len()
        );
        self.engine.recover_sessions_inner(summaries.clone()).await;
        let mut drivable = Vec::new();
        {
            // One shared read lock for the whole batch (tri-QC P1-E): N
            // sequential acquisitions on the runners RwLock stall recovery
            // behind any writer.
            let shared = self.engine.shared_state();
            let runners = shared.runners.read().await;
            for summary in &summaries {
                if runners.contains_key(&summary.session_id.0) {
                    drivable.push(summary.clone());
                }
            }
        }
        if drivable.is_empty() {
            return Vec::new();
        }
        let workflow_store: Arc<dyn WorkflowStateStore> = sqlite_storage.clone();
        let resume_cancel = tokio_util::sync::CancellationToken::new();
        if let Some(notify) = shutdown_notify {
            let resume_cancel = resume_cancel.clone();
            tokio::spawn(async move {
                notify.notified().await;
                resume_cancel.cancel();
            });
        }
        let decisions = self
            .recover_driving(Some(&workflow_store), &drivable, Some(&resume_cancel))
            .await;
        for d in &decisions {
            tracing::info!(decision = ?d, "recover_persisted re-drive decision");
        }
        decisions
    }

    /// Construct the coordinator over the daemon's engine/storage.
    #[must_use]
    pub fn new(
        engine: Arc<nexus_orchestration::GraphFlowEngine>,
        storage: Arc<dyn SessionStorage>,
        pool: Arc<sqlx::SqlitePool>,
        session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    ) -> Self {
        let workflow_store: Arc<dyn WorkflowStateStore> =
            Arc::new(nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(pool.clone()));
        Self {
            engine,
            storage,
            pool,
            provider_catalog: None,
            binding_provider: None,
            session_cancels,
            drives: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            failed_owners: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
            workflow_store,
            config: PresetRunConfig::default(),
            schedule_supervisor: std::sync::Arc::new(std::sync::RwLock::new(None)),
            run_events: None,
        }
    }

    /// Override the workflow state store (tests inject a delegating
    /// wrapper around the real SQLite store to fault/race real boundary
    /// operations). Production uses the construction default.
    #[must_use]
    pub fn with_workflow_store(mut self, store: Arc<dyn WorkflowStateStore>) -> Self {
        self.workflow_store = store;
        self
    }
    /// Attach the bounded run-event port (P4 §6.3).
    #[must_use]
    pub fn with_run_events(mut self, port: Arc<dyn RunEventPort>) -> Self {
        self.run_events = Some(port);
        self
    }

    /// Publish the durable winner to the run's live SSE ring, then close it
    /// only for an **authoritative terminal** status (QC2 F-001).
    ///
    /// `Interrupted` is NOT authoritative terminal: it is the actionable
    /// "cleanup unconfirmed" outcome, so its ring stays live — a retry
    /// cancel can still confirm the run and publish the final `cancelled`
    /// winner on the SAME ring. Closing it there would strand the retry as
    /// replay-only history and drop every live subscriber.
    async fn publish_durable_run_state(&self, session_id: &SessionId) {
        if let Some(port) = &self.run_events {
            if let Ok(Some(record)) = self.workflow_store.load_run(session_id).await {
                port.publish_run_state(&session_id.0, &record);
                if matches!(
                    record.status,
                    SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Cancelled
                ) {
                    port.mark_terminal(&session_id.0);
                }
            }
        }
    }

    /// Attach the schedule supervisor for terminal settlement (T3, A3).
    ///
    /// The supervisor is constructed AFTER the coordinator (it needs the
    /// coordinator's `ScheduleRunStarter`), so boot / lazy attach call
    /// this once the supervisor exists. Settlement is skipped when the
    /// handle is absent (tests, standalone coordinator).
    #[must_use]
    pub fn with_schedule_supervisor(
        self,
        supervisor: Arc<nexus_orchestration::schedule::supervisor::ScheduleSupervisor>,
    ) -> Self {
        *self
            .schedule_supervisor
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(supervisor);
        self
    }

    /// Attach the schedule supervisor in place (T3, A3).
    ///
    /// The coordinator is `Arc`-wrapped and stored in `WorkspaceState`
    /// before the supervisor exists (the supervisor needs the coordinator's
    /// `ScheduleRunStarter`), so boot / lazy attach call this setter once
    /// the supervisor is constructed. Settlement is skipped when the handle
    /// is absent (tests, standalone coordinator).
    pub fn set_schedule_supervisor(
        &self,
        supervisor: Arc<nexus_orchestration::schedule::supervisor::ScheduleSupervisor>,
    ) {
        *self
            .schedule_supervisor
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(supervisor);
    }

    /// Attach the daemon's Host plane so admission can validate provider
    /// references in agent bindings before enqueue (N-4).
    #[must_use]
    pub fn with_provider_catalog(mut self, catalog: Arc<dyn ProviderCatalogPort>) -> Self {
        self.provider_catalog = Some(catalog);
        self
    }

    /// Attach the daemon's configured default binding provider (C-3) so an
    /// explicit legacy start can derive sanctioned agent bindings for a
    /// historical row with no frozen descriptor. Mirrors the supervisor's
    /// `with_binding_provider` (same config source).
    #[must_use]
    pub fn with_binding_provider(mut self, provider_id: String) -> Self {
        self.binding_provider = Some(provider_id);
        self
    }

    /// Read-only fence probe (N-14): whether `session_id` is currently
    /// fenced against re-entry because its durable failure transition
    /// could not be committed. Production observability surface.
    #[must_use]
    pub async fn is_fenced(&self, session_id: &SessionId) -> bool {
        self.failed_owners.lock().await.contains(&session_id.0)
    }

    /// The engine this coordinator drives (as the trait surface).
    #[must_use]
    pub fn engine(&self) -> Arc<dyn OrchestrationEngine> {
        self.engine.clone()
    }

    /// Start a new public session through the SAME v1 admission path
    /// schedule admission uses (I-4): resolve the preset, freeze the
    /// supplied seed/input, persist the v1 run (descriptor + initial
    /// checkpoint), and drive it through the coordinator — mandatory, never
    /// optional. `_system.*` presets are refused (A8).
    ///
    /// # Errors
    /// Returns [`RunControlError`] on any admission failure.
    #[allow(clippy::too_many_arguments)] // one public admission seam forwarding every runner dependency; a struct would obscure call sites
    pub async fn start_session(
        &self,
        preset_id: &str,
        creator_id: &str,
        seed: Option<&str>,
        agent_bindings: std::collections::HashMap<
            String,
            nexus_orchestration::run_state::AgentBinding,
        >,
        nexus_home: &std::path::Path,
        caps: &nexus_orchestration::CapabilityRegistryHolder,
        daemon_tool_dispatch: Option<
            std::sync::Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>,
        >,
        prompt_executor: Option<
            std::sync::Arc<dyn nexus_orchestration::capability::PromptExecutor>,
        >,
    ) -> Result<SessionId, RunControlError> {
        // A8 fail-closed: `_system.*` presets are never driven as user
        // sessions.
        if preset_id.starts_with("_system.") {
            return Err(RunControlError::NotEligible(
                preset_id.to_string(),
                format!("system preset '{preset_id}' cannot be started as a user session"),
            ));
        }

        let registry = caps.get().ok_or_else(|| {
            RunControlError::NoWorkspace("capability registry unavailable".into())
        })?;
        let loaded = nexus_preset::resolve_preset(preset_id, nexus_home, &registry)
            .map_err(|e| RunControlError::PresetLoad(preset_id.to_string(), e.to_string()))?;

        // N-4: validate role/provider references BEFORE enqueue. Every
        // binding role must be a declared preset role (or `default`), and
        // every provider id must exist in the Host provider catalog.
        validate_agent_bindings(&loaded, &agent_bindings, self.provider_catalog.as_ref()).await?;

        // Freeze the supplied seed: JSON object → structured `preset.input.*`;
        // anything else → core-context text.
        let (input, core_context) = seed.map_or_else(
            || (serde_json::Map::new(), None),
            |seed_text| match serde_json::from_str::<serde_json::Value>(seed_text) {
                Ok(serde_json::Value::Object(map)) => (map, None),
                _ => (serde_json::Map::new(), Some(seed_text.to_string())),
            },
        );

        let session_id = format!("{}:{}", preset_id, uuid::Uuid::new_v4());
        let engine_proxy: Arc<dyn OrchestrationEngine> = self.engine.clone();
        let wired = nexus_orchestration::preset_runtime::build_wired_outer_graph(
            &loaded,
            &engine_proxy,
            &registry,
            daemon_tool_dispatch,
            prompt_executor,
            self.session_cancels.clone(),
        )
        .map_err(|e| RunControlError::PresetLoad(preset_id.to_string(), e.to_string()))?;
        let sid = self
            .engine
            .start_preset_run_with_input(
                &session_id,
                &loaded,
                creator_id,
                None,
                input,
                core_context.as_deref(),
                agent_bindings,
                Arc::new(wired),
            )
            .await
            .map_err(|e| RunControlError::Admission(e.to_string()))?;
        self.ensure_driving(&sid).await?;
        Ok(sid)
    }

    /// Persist a durable non-replayable failure for a v1 run whose drive
    /// loop failed (closure 8 / I-2 residual).
    ///
    /// A failed driver must never leave the durable v1 record classified
    /// runnable: `drive_preset_run`'s context-key failure record
    /// (`_run_status`/`_run_error`) does not change the authoritative DB
    /// `status` column, so a later `ensure_driving` would classify the
    /// still-`running` row as runnable and spawn again. This method commits
    /// a revision-fenced `Failed` transition with a stable `RunFailure`
    /// record and clears in-flight markers — the run becomes terminal and
    /// is never re-driven. This is the Task 1 driver-failure boundary, not
    /// general Task 3 settlement: only the drive-loop failure path writes
    /// here.
    ///
    /// Returns `true` when the durable `Failed` transition committed. When
    /// the transition cannot be committed (load/get/commit failure), the
    /// caller registers the session in the `failed_owners` fence so
    /// `ensure_driving` refuses re-entry until the non-replayable
    /// transition is durable (closure 8 fail-closed).
    async fn persist_drive_failure(
        &self,
        session_id: &SessionId,
        settlement: &FailureSettlementSummary,
    ) -> bool {
        if settlement.authoritative.requires_owner_fence()
            || settlement.context.requires_owner_fence()
            || settlement.cleanup.requires_owner_fence()
        {
            return false;
        }
        if settlement.authoritative == FailurePersistenceDisposition::OwnershipLost
            || settlement.cleanup == FailurePersistenceDisposition::OwnershipLost
        {
            return true;
        }
        if settlement.authoritative != FailurePersistenceDisposition::Committed {
            return false;
        }
        if settlement.context != FailurePersistenceDisposition::Committed {
            return false;
        }
        if settlement.cleanup != FailurePersistenceDisposition::Committed {
            return false;
        }
        let store = self.workflow_store.clone();
        let Ok(Some(record)) = store.load_run(session_id).await else {
            return false;
        };
        if record.execution_version < 1 {
            // Legacy v0 rows carry the context-only failure record; there is
            // no v1 terminal transition to claim.
            return true;
        }
        let has_authoritative_failure = record
            .state
            .as_ref()
            .and_then(|s| s.failure.as_ref())
            .is_some_and(|f| f.code == "driver_failed");
        if !has_authoritative_failure {
            return false;
        }
        // Mechanical terminal proof: the anchored cleanup settles the durable
        // run itself (verified at the store boundary before it reports
        // `Committed`). Only Failed preserves the execution error; another
        // terminal status cannot substitute for this failure settlement.
        // There is deliberately no coordinator-side terminal write: the
        // previous revision-only fallback could rebase onto a newer
        // graph-only or revision winner.
        record.status == SessionStatus::Failed
    }

    /// Settle the schedule whose `current_session_id` equals a terminal run
    /// (T3, A3).
    ///
    /// Durable session terminal status is truth: the drive outcome only
    /// gates whether settlement is attempted; the terminal status is read
    /// from the durable v1 record and mapped to the supervisor's transition:
    /// - `Completed` → `ScheduleStatus::Completed`
    /// - `Failed` → `ScheduleStatus::Failed`
    /// - `Cancelled` → `ScheduleStatus::Cancelled`
    /// - `Interrupted` → **no settlement** (qc2 F-002): an unconfirmed
    ///   cancel/cleanup is not a successful cancel, so the schedule row must
    ///   never read `cancelled`. `creator_schedules.status` has no
    ///   `interrupted` variant (`ScheduleStatus`), and an unmapped string
    ///   projects as `Pending` (re-runnable) — so the row is left
    ///   non-terminal and public inspect projects the run's
    ///   `execution.recovery_class: "interrupted"` from the durable session.
    ///   Boot reconciliation skips the same class.
    ///
    /// Schedule and session inspect use the same durable terminal cause;
    /// execution failure never becomes a satisfied cancellation dependency.
    ///
    /// Only the schedule whose `current_session_id` matches the run is
    /// updated — a different schedule (or none) is left untouched. The
    /// supervisor's `on_schedule_terminal` flips the row, releases the
    /// runtime lock, fires the terminal hooks (review findings, auto-chain
    /// continuation), and ticks the next eligible schedule.
    ///
    /// Settlement is best-effort: a missing supervisor handle (tests /
    /// standalone coordinator) or a DB error is logged, never fatal to the
    /// drive loop. A checkpoint-before-settlement crash is reconciled at
    /// boot (`reconcile_terminal_schedules`).
    async fn settle_terminal_schedule(&self, session_id: &SessionId, outcome: &PresetRunOutcome) {
        // Only terminal outcomes settle.
        if !matches!(
            outcome,
            PresetRunOutcome::Completed { .. }
                | PresetRunOutcome::Failed { .. }
                | PresetRunOutcome::Cancelled { .. }
        ) {
            return;
        }

        let Some(supervisor) = self
            .schedule_supervisor
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
        else {
            tracing::debug!(
                session_id = %session_id.0,
                "coordinator: no schedule supervisor wired; skipping settlement"
            );
            return;
        };

        // Durable session terminal status is truth.
        let store = self.workflow_store.clone();
        let record = match store.load_run(session_id).await {
            Ok(Some(record)) => record,
            Ok(None) => {
                tracing::debug!(
                    session_id = %session_id.0,
                    "coordinator: no durable run record; nothing to settle"
                );
                return;
            }
            Err(e) => {
                tracing::warn!(
                    session_id = %session_id.0,
                    error = %e,
                    "coordinator: settlement durable-status read failed"
                );
                return;
            }
        };
        let terminal_status = match record.status {
            SessionStatus::Completed => nexus_contracts::local::schedule::ScheduleStatus::Completed,
            SessionStatus::Failed => nexus_contracts::local::schedule::ScheduleStatus::Failed,
            SessionStatus::Cancelled => nexus_contracts::local::schedule::ScheduleStatus::Cancelled,
            // Unconfirmed cancel/cleanup is NOT a successful cancel (A5).
            // `creator_schedules.status` has no `interrupted` variant and an
            // unmapped string projects as `Pending` (re-runnable), so the
            // schedule row is deliberately left non-terminal: public inspect
            // projects the run's `execution.recovery_class: "interrupted"`
            // from the durable session record (qc2 F-002).
            SessionStatus::Interrupted => {
                tracing::warn!(
                    session_id = %session_id.0,
                    "coordinator: run interrupted (unconfirmed cleanup); schedule row left \
                     non-terminal and projected as interrupted recovery class"
                );
                return;
            }
            // Non-terminal: the run is still live; never settle.
            _ => return,
        };

        // Only the schedule whose current_session_id equals this run.
        let schedule_id: Option<String> = sqlx::query_scalar!(
            r#"SELECT schedule_id as "schedule_id!" FROM creator_schedules WHERE current_session_id = ?"#,
            session_id.0
        )
        .fetch_optional(&*self.pool)
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(
                session_id = %session_id.0,
                error = %e,
                "coordinator: settlement lookup failed"
            );
            None
        });

        let Some(schedule_id) = schedule_id else {
            tracing::debug!(
                session_id = %session_id.0,
                "coordinator: no schedule owns this run; nothing to settle"
            );
            return;
        };

        if let Err(e) = supervisor
            .on_schedule_terminal(&schedule_id, terminal_status)
            .await
        {
            tracing::warn!(
                schedule_id = %schedule_id,
                session_id = %session_id.0,
                terminal_status = ?terminal_status,
                error = %e,
                "coordinator: schedule terminal settlement failed (non-fatal; \
                 boot reconciliation will retry)"
            );
        } else {
            tracing::info!(
                schedule_id = %schedule_id,
                session_id = %session_id.0,
                terminal_status = ?terminal_status,
                "coordinator: settled schedule from durable terminal session"
            );
        }
    }

    /// Ensure exactly one drive loop is running for `session_id` (A3).
    ///
    /// Single-flight: when a drive is already registered for the session,
    /// returns [`DriveDisposition::AlreadyDriving`] without spawning a
    /// second loop. When the registered drive has finished, the durable
    /// run state is consulted (I-2): terminal/interrupted/human-wait states
    /// return a non-driving disposition; only an explicitly runnable
    /// boundary (running/paused with no in-flight markers) creates a fresh
    /// owner. A finished owner is never silently re-driven.
    ///
    /// The per-run cancellation token is registered in the shared
    /// `session_cancels` map (idempotent — the engine already registers it
    /// at admission) so prompt consumers and out-of-band cancellation share
    /// one token.
    ///
    /// # Errors
    /// Returns [`RunControlError`] if the durable state cannot be loaded or
    /// the fresh drive owner cannot be spawned.
    pub async fn ensure_driving(
        &self,
        session_id: &SessionId,
    ) -> Result<DriveDisposition, RunControlError> {
        {
            let drives = self.drives.lock().await;
            if let Some(owner) = drives.get(&session_id.0) {
                if !owner.join.is_finished() {
                    return Ok(DriveDisposition::AlreadyDriving);
                }
            }
        }

        // Closure 8 fail-closed: a drive loop that failed AND whose durable
        // non-replayable `Failed` transition could not be committed is
        // fenced. `ensure_driving` refuses re-entry until the transition is
        // durable — the failed driver is never silently re-driven.
        if self.failed_owners.lock().await.contains(&session_id.0) {
            return Ok(DriveDisposition::NotDriving);
        }

        // Durable-state gate even when no owner exists (I-2): a reconstructed
        // coordinator must not start a fresh owner for terminal/wait/interrupt.
        let store = self.workflow_store.clone();
        let record = store
            .load_run(session_id)
            .await
            .map_err(|e| RunControlError::Drive(e.to_string()))?;
        if let Some(record) = record {
            if record.execution_version >= 1 {
                let class = nexus_orchestration::resume_rules::classify_recovery(
                    &record.status,
                    record.state.as_ref(),
                    false,
                );
                match class {
                    nexus_orchestration::resume_rules::RecoveryClass::Terminal
                    | nexus_orchestration::resume_rules::RecoveryClass::Interrupted
                    | nexus_orchestration::resume_rules::RecoveryClass::HumanWait
                    | nexus_orchestration::resume_rules::RecoveryClass::Unreadable => {
                        return Ok(DriveDisposition::NotDriving);
                    }
                    _ => {}
                }
            }
        }

        let mut drives = self.drives.lock().await;
        if let Some(owner) = drives.get(&session_id.0) {
            if !owner.join.is_finished() {
                return Ok(DriveDisposition::AlreadyDriving);
            }
            drives.remove(&session_id.0);
        }

        // QC2 F-004: RESERVE the run's live event-ring quota BEFORE any
        // drive is spawned. `try_register_live` is the reservation: it
        // succeeds when a slot is free OR when this run already owns a ring
        // (reuse). When the quota is exhausted the drive is refused with a
        // typed capacity error and NO durable work is touched — a driver can
        // never outlive its ring and silently lose every event.
        if let Some(port) = &self.run_events {
            if !port.try_register_live(&session_id.0).await {
                return Err(RunControlError::RunEventCapacity(session_id.0.clone()));
            }
        }

        let cancel = tokio_util::sync::CancellationToken::new();
        self.session_cancels
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .insert(session_id.0.clone(), cancel.clone());

        let engine = self.engine.clone();
        let storage = self.storage.clone();
        let workflow_store: Arc<dyn WorkflowStateStore> = self.workflow_store.clone();
        let config = self.config.clone();
        let sid = session_id.clone();
        let drive_cancel = cancel.clone();
        let coordinator = self.clone();
        let join = tokio::spawn(async move {
            let outcome = drive_preset_run(
                &*engine,
                Some(&storage),
                Some(&workflow_store),
                &sid,
                &config,
                Some(&drive_cancel),
            )
            .await;
            // Closure 8: a failed driver persists a durable non-replayable
            // `Failed` transition so the v1 record is never classified
            // runnable again after owner cleanup. When the transition
            // cannot be committed, the failed owner is fenced so
            // `ensure_driving` refuses re-entry until it is durable.
            if let PresetRunOutcome::Failed { settlement, .. } = &outcome {
                if !coordinator.persist_drive_failure(&sid, settlement).await {
                    coordinator.failed_owners.lock().await.insert(sid.0.clone());
                }
            }
            // T3 (A3): settle the matching schedule from the durable
            // terminal session status. Only the schedule whose
            // `current_session_id` equals this run is updated; a
            // checkpoint-before-settlement crash is reconciled at boot.
            coordinator.settle_terminal_schedule(&sid, &outcome).await;
            coordinator.publish_durable_run_state(&sid).await;
            if let Some(port) = &coordinator.run_events {
                port.remove_live(&sid.0).await;
            }
            tracing::debug!(
                session_id = %sid.0,
                outcome = ?outcome,
                "coordinator drive loop finished"
            );
        });
        drives.insert(session_id.0.clone(), DriveOwner { cancel, join });
        drop(drives);
        Ok(DriveDisposition::Started)
    }

    /// Re-drive recovered sessions through the SINGLE coordinator owner
    /// (I-1). Boot recovery must not spawn a second, unowned drive path.
    ///
    /// Each recovered session that is eligible to step (converge/merge chain
    /// class, runner present) is registered in the coordinator's drives map
    /// BEFORE the drive loop starts, so a later public re-entry
    /// (`ensure_driving`) observes the same owner and never spawns a second
    /// driver. Shutdown/cancellation ownership is the coordinator's token.
    ///
    /// Returns the same [`ResumeDecision`]s as the legacy
    /// [`resume_driven_sessions`] so boot observability is unchanged.
    #[allow(clippy::too_many_lines)] // linear recovery sweep with per-session classification + spawn; per-class helpers would hide ordering
    pub async fn recover_driving(
        &self,
        workflow_store: Option<&Arc<dyn WorkflowStateStore>>,
        summaries: &[SessionSummary],
        cancel: Option<&CancellationToken>,
    ) -> Vec<ResumeDecision> {
        let parent_cancel = cancel.cloned().unwrap_or_default();
        let mut decisions = Vec::with_capacity(summaries.len());
        for summary in summaries {
            if parent_cancel.is_cancelled() {
                break;
            }
            let session_id = summary.session_id.clone();

            // Load the persisted session (position + context).
            let session = match self.storage.get(&session_id.0).await {
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

            let context_value = serde_json::to_value(&session.context).ok();
            let data = context_value.as_ref().and_then(resume_rules::context_data);

            // A7 v1 gate: the durable workflow store governs the recovery
            // class. Only the converge/merge class proceeds to the bounded
            // join re-drive; terminal/interrupted/human-wait/unreadable
            // classes skip immediately.
            let mut v1_converge_merge = false;
            let mut v1_safe_boundary = false;
            if let Some(store) = workflow_store {
                match store.load_run(&session_id).await {
                    Ok(Some(record)) if record.execution_version >= 1 => {
                        let gate_park = data.is_some_and(|data| {
                            resume_rules::gate_park_live(
                                data,
                                summary.current_task_id.as_deref().unwrap_or_default(),
                            )
                        });
                        let class = resume_rules::classify_recovery(
                            &record.status,
                            record.state.as_ref(),
                            gate_park,
                        );
                        match class {
                            resume_rules::RecoveryClass::ConvergeMerge => {
                                v1_converge_merge = true;
                            }
                            resume_rules::RecoveryClass::Terminal => {
                                decisions.push(ResumeDecision::SkippedTerminal { session_id });
                                continue;
                            }
                            resume_rules::RecoveryClass::Unreadable => {
                                decisions
                                    .push(ResumeDecision::SkippedUnreadableMetadata { session_id });
                                continue;
                            }
                            resume_rules::RecoveryClass::Interrupted => {
                                decisions.push(ResumeDecision::SkippedInterrupted { session_id });
                                continue;
                            }
                            resume_rules::RecoveryClass::HumanWait => {
                                decisions.push(ResumeDecision::SkippedHumanWait { session_id });
                                continue;
                            }
                            // A7 rule 6: a durable v1 run at a fully
                            // committed safe boundary is reconstructed and
                            // re-driven through the single owner — never
                            // stranded by a restart (P3 T1 rereview P1).
                            resume_rules::RecoveryClass::SafeBoundary => {
                                if let Err(e) =
                                    self.engine.ensure_recovered_runner(&session_id).await
                                {
                                    tracing::warn!(
                                        session_id = %session_id.0,
                                        error = %e,
                                        "recovery: safe-boundary reconstruction failed; \
                                         treating as non-replayable metadata"
                                    );
                                    decisions.push(ResumeDecision::SkippedUnreadableMetadata {
                                        session_id,
                                    });
                                    continue;
                                }
                                v1_safe_boundary = true;
                            }
                            resume_rules::RecoveryClass::LegacyUnverified => {
                                unreachable!("v1 load_run record cannot classify LegacyUnverified")
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(e) => {
                        decisions.push(ResumeDecision::SkippedUnreadable {
                            session_id,
                            error: e.to_string(),
                        });
                        continue;
                    }
                }
            }

            // Legacy (v0 / no-store) typed-failure filter.
            if !v1_converge_merge
                && !v1_safe_boundary
                && data.is_some_and(resume_rules::typed_failure_keys_present)
            {
                decisions.push(ResumeDecision::SkippedTypedFailed { session_id });
                continue;
            }

            // Human wait (v0 / no-store).
            if !v1_converge_merge
                && !v1_safe_boundary
                && matches!(summary.status, SessionStatus::WaitingForInput)
                && !data.is_some_and(resume_rules::is_converge_merge_chain)
            {
                decisions.push(ResumeDecision::SkippedNotConvergeMergeClass { session_id });
                continue;
            }

            // Converge/merge chain class filter.
            if !v1_converge_merge
                && !v1_safe_boundary
                && !data.is_some_and(resume_rules::is_converge_merge_chain)
            {
                decisions.push(ResumeDecision::SkippedNotConvergeMergeClass { session_id });
                continue;
            }

            // Runner-existence filter.
            if !self.engine.has_runner(&session_id).await {
                decisions.push(ResumeDecision::SkippedNoRunner { session_id });
                continue;
            }

            // Register the owner BEFORE enqueue (I-1): the coordinator's
            // drives map is the single ownership registry. A concurrent
            // public re-entry observes the same owner.
            let mut drives = self.drives.lock().await;
            if let Some(owner) = drives.get(&session_id.0) {
                if !owner.join.is_finished() {
                    decisions.push(ResumeDecision::ReDriven {
                        session_id,
                        outcome: PresetRunOutcome::MaxStepsExceeded { steps: 0 },
                    });
                    continue;
                }
                drives.remove(&session_id.0);
            }
            // QC2 F-004: the recovery re-drive reserves the same live-ring
            // quota before spawning. A session that cannot reserve is left
            // tracked-but-not-driven (its durable record is untouched); the
            // next boot/retry reserves a freed slot.
            if let Some(port) = &self.run_events {
                if !port.try_register_live(&session_id.0).await {
                    decisions.push(ResumeDecision::SkippedRunEventCapacity { session_id });
                    continue;
                }
            }
            let cancel = parent_cancel.child_token();
            self.session_cancels
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .insert(session_id.0.clone(), cancel.clone());

            let engine = self.engine.clone();
            let storage = self.storage.clone();
            let coordinator = self.clone();
            let workflow_store: Arc<dyn WorkflowStateStore> = coordinator.workflow_store.clone();
            let config = PresetRunConfig {
                resume_waiting: true,
                ..self.config.clone()
            };
            let sid = session_id.clone();
            let drive_cancel = cancel.clone();
            let join = tokio::spawn(async move {
                let outcome = drive_preset_run(
                    &*engine,
                    Some(&storage),
                    Some(&workflow_store),
                    &sid,
                    &config,
                    Some(&drive_cancel),
                )
                .await;
                // Closure 8: a failed recovery drive persists a durable
                // non-replayable `Failed` transition (same boundary as
                // `ensure_driving`); a failed write fences the owner.
                if let PresetRunOutcome::Failed { settlement, .. } = &outcome {
                    if !coordinator.persist_drive_failure(&sid, settlement).await {
                        coordinator.failed_owners.lock().await.insert(sid.0.clone());
                    }
                }
                // T3 (A3): settle the matching schedule from the durable
                // terminal session status (same boundary as
                // `ensure_driving`).
                coordinator.settle_terminal_schedule(&sid, &outcome).await;
                coordinator.publish_durable_run_state(&sid).await;
                if let Some(port) = &coordinator.run_events {
                    port.remove_live(&sid.0).await;
                }
                tracing::debug!(
                    session_id = %sid.0,
                    outcome = ?outcome,
                    "coordinator recovery drive loop finished"
                );
            });
            drives.insert(session_id.0.clone(), DriveOwner { cancel, join });
            drop(drives);
            decisions.push(ResumeDecision::ReDriven {
                session_id,
                outcome: PresetRunOutcome::MaxStepsExceeded { steps: 0 },
            });
        }
        decisions
    }

    /// Explicit public `schedule start` for a legacy never-started row
    /// (C-3): pending, paused, or admission-only `Running` with no owned
    /// session. The row's policy is cut over to `driven_v1`
    /// transactionally, then the row is admitted through the SAME atomic
    /// admission path as new rows.
    ///
    /// `_system.*` rows are always refused (A8). A `Running` row WITH an
    /// owned session is re-entry — the existing run is driven (single-flight).
    ///
    /// # Errors
    /// Returns [`RunControlError`] on any admission failure.
    pub async fn start_legacy_schedule(
        &self,
        schedule_id: &str,
        pool: &sqlx::SqlitePool,
        nexus_home: &std::path::Path,
        caps: &nexus_orchestration::CapabilityRegistryHolder,
        daemon_tool_dispatch: Option<
            std::sync::Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>,
        >,
        prompt_executor: Option<
            std::sync::Arc<dyn nexus_orchestration::capability::PromptExecutor>,
        >,
    ) -> Result<SessionId, RunControlError> {
        // Load the schedule row.
        let row = sqlx::query_as!(
            ScheduleAdmissionRow,
            r#"SELECT schedule_id as "schedule_id!", creator_id as "creator_id!",
                    preset_id as "preset_id!", status as "status!",
                    current_core_context_version as "current_core_context_version!",
                    current_session_id, execution_policy as "execution_policy!",
                    work_id, execution_descriptor_json
             FROM creator_schedules WHERE schedule_id = ?"#,
            schedule_id
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?
        .ok_or_else(|| RunControlError::ScheduleNotFound(schedule_id.to_string()))?;

        // A8 fail-closed: `_system.*` is never opted in by a generic signal.
        if row.preset_id.starts_with("_system.") {
            return Err(RunControlError::NotEligible(
                schedule_id.to_string(),
                format!("system preset '{}' cannot be started", row.preset_id),
            ));
        }

        // Re-entry: the schedule already owns a run — drive it.
        if let Some(sid) = &row.current_session_id {
            let store = nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(
                std::sync::Arc::new(pool.clone()),
            );
            let run_exists = store
                .load_run(&nexus_orchestration::SessionId(sid.clone()))
                .await
                .map_err(|e| RunControlError::Admission(e.to_string()))?
                .is_some();
            if run_exists {
                let sid = nexus_orchestration::SessionId(sid.clone());
                self.ensure_driving(&sid).await?;
                return Ok(sid);
            }
            return Err(RunControlError::NotEligible(
                schedule_id.to_string(),
                format!(
                    "schedule has a stale claim on session {sid}; \
                     boot recovery must classify it before re-admission"
                ),
            ));
        }

        // Admissible states: pending, paused, or admission-only Running
        // (legacy never-started row with no owned session — C-3).
        if !matches!(row.status.as_str(), "pending" | "paused" | "running") {
            return Err(RunControlError::NotEligible(
                schedule_id.to_string(),
                format!("status is '{}'", row.status),
            ));
        }

        // C-3: a real historical row (migration default `legacy_inert`,
        // `execution_descriptor_json IS NULL`, no version-0 core-context
        // record) has no frozen payload. Prepare it BEFORE the atomic
        // cutover: resolve the preset through the shared resolver, freeze
        // the source identity + available structured input, choose the
        // sanctioned binding source, create the missing version-0 seed,
        // and persist the frozen descriptor. The row stays inert until
        // this explicit call; the cutover + owned-session claim remain
        // atomic inside `admit_schedule`'s store transaction.
        if row.execution_descriptor_json.is_none() {
            self.prepare_legacy_row(&row, pool, nexus_home, caps)
                .await?;
        }

        // Policy cutover happens inside admit_schedule_run (same claim
        // transaction). Do not UPDATE execution_policy before the payload
        // and run row exist (C-3).
        self.admit_schedule(
            schedule_id,
            pool,
            nexus_home,
            caps,
            daemon_tool_dispatch,
            prompt_executor,
        )
        .await
    }

    /// C-3: prepare a historical never-started row for explicit start.
    ///
    /// A pre-cutover row has `execution_descriptor_json IS NULL` and no
    /// version-0 core-context record. This method fills the missing frozen
    /// payload from the CURRENT resolver/provider configuration:
    ///
    /// 1. Resolves the preset through the shared resolver (same
    ///    [`nexus_preset::resolve_preset`] admission uses).
    /// 2. Freezes the content-addressed source identity from the loaded
    ///    preset and the row's `work_id` as the available structured input
    ///    (a historical row carries no frozen input map — empty).
    /// 3. Chooses the sanctioned binding source: the coordinator's
    ///    configured default binding provider (the same source the
    ///    supervisor/internal insertion paths use), falling back to the
    ///    Host provider catalog's first entry. A preset with no prompt
    ///    roles accepts an empty map.
    /// 4. Creates the missing version-0 seed (idempotent `INSERT OR
    ///    IGNORE` — the historical row's pointer is already 0, so no
    ///    pointer update is needed; concurrent explicit starts cannot
    ///    collide on the PK).
    /// 5. Persists the frozen descriptor into `execution_descriptor_json`
    ///    (idempotent `WHERE ... IS NULL`).
    ///
    /// The row remains inert across boot/tick/cron until this explicit
    /// call; the policy cutover + owned-session claim stay atomic in
    /// `admit_schedule`'s store transaction.
    async fn prepare_legacy_row(
        &self,
        row: &ScheduleAdmissionRow,
        pool: &sqlx::SqlitePool,
        nexus_home: &std::path::Path,
        caps: &nexus_orchestration::CapabilityRegistryHolder,
    ) -> Result<(), RunControlError> {
        // 1. Resolve the preset through the shared resolver.
        let registry = caps.get().ok_or_else(|| {
            RunControlError::NoWorkspace("capability registry unavailable".into())
        })?;
        let loaded =
            nexus_preset::resolve_preset(&row.preset_id, nexus_home, &registry)
                .map_err(|e| RunControlError::PresetLoad(row.preset_id.clone(), e.to_string()))?;

        // 2. Freeze the source identity (must be present — embedded and
        //    directory presets both carry it).
        let source = loaded.source_identity.clone().ok_or_else(|| {
            RunControlError::Admission(format!(
                "preset '{}' has no source identity; cannot prepare a legacy start",
                row.preset_id
            ))
        })?;

        // 3. Choose the sanctioned binding source: ONLY the coordinator's
        //    configured default binding provider (the same source the
        //    supervisor/internal insertion paths use). A catalog-order
        //    first entry is NEVER selected — a preset with no prompt roles
        //    accepts an empty map; a preset that needs prompt roles but has
        //    no configured default provider refuses with a typed admission
        //    error before any session/claim is created (qc1 F-001).
        let roles = nexus_preset::required_prompt_roles(&loaded);
        let agent_bindings = if roles.is_empty() {
            std::collections::HashMap::new()
        } else {
            let Some(provider_id) = &self.binding_provider else {
                return Err(RunControlError::Admission(format!(
                    "preset '{}' requires {} prompt role binding(s) but no configured \
                     default binding provider exists; cannot derive sanctioned bindings \
                     for legacy start",
                    row.preset_id,
                    roles.len()
                )));
            };
            roles
                .into_iter()
                .map(|role| {
                    (
                        role,
                        nexus_orchestration::run_state::AgentBinding {
                            provider_id: provider_id.clone(),
                            model: None,
                        },
                    )
                })
                .collect()
        };

        // 4. Create the missing version-0 seed. The historical row's
        //    `current_core_context_version` is already 0 (migration
        //    default), so the seed row alone makes the pointer valid.
        //    `INSERT OR IGNORE` keeps concurrent explicit starts
        //    idempotent (the PK is (schedule_id, version)).
        let now = chrono::Utc::now().timestamp();
        let payload = serde_json::json!({ "kind": "text", "body": "" });
        let derivation = serde_json::json!({ "kind": "seed", "raw": "" });
        let content = serde_json::to_vec(&payload).map_err(|e| {
            RunControlError::Admission(format!("seed payload serialization failed: {e}"))
        })?;
        let derivation_detail = serde_json::to_vec(&derivation).map_err(|e| {
            RunControlError::Admission(format!("seed derivation serialization failed: {e}"))
        })?;
        sqlx::query!(
            "INSERT OR IGNORE INTO core_context_versions
               (schedule_id, version, payload_kind, content,
                derivation_kind, derivation_detail,
                created_at, created_by_kind, created_by_user_id)
             VALUES (?, 0, 'text', ?, 'seed', ?, ?, 'system', NULL)",
            row.schedule_id,
            content,
            derivation_detail,
            now,
        )
        .execute(pool)
        .await
        .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?;

        // 5. Persist the frozen descriptor (idempotent under concurrent
        //    explicit starts — the winner's identical descriptor wins).
        let descriptor = nexus_orchestration::run_state::RunDescriptorV1 {
            creator_id: row.creator_id.clone(),
            work_id: row.work_id.clone().filter(|id| !id.is_empty()),
            workspace_root: std::path::PathBuf::new(),
            preset_id: loaded.id.clone(),
            preset_version: loaded.version,
            source,
            input: serde_json::Map::new(),
            agent_bindings,
            parent_session_id: None,
            graph_name: None,
        };
        let descriptor_bytes = serde_json::to_vec(&descriptor).map_err(|e| {
            RunControlError::Admission(format!("descriptor serialization failed: {e}"))
        })?;
        sqlx::query!(
            "UPDATE creator_schedules
             SET execution_descriptor_json = ?
             WHERE schedule_id = ? AND execution_descriptor_json IS NULL",
            descriptor_bytes,
            row.schedule_id
        )
        .execute(pool)
        .await
        .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?;

        Ok(())
    }

    /// Admit a `driven_v1` schedule as an owned run and drive it (A3).
    ///
    /// Atomic admission contract (C-1/C-2):
    /// 1. Loads the schedule row and verifies eligibility (exists, policy
    ///    `driven_v1`, status pending/paused, no owned session).
    /// 2. Resolves the preset, freezes the descriptor + core-context seed,
    ///    and calls the P0 store's atomic `admit_schedule_run` — ONE
    ///    Creator-DB transaction linearizes the schedule claim
    ///    (`status='running'`, `current_session_id`, core-context version),
    ///    the v1 session row (initial checkpoint + descriptor + seeded
    ///    context), and the schedule→session identity. A concurrent
    ///    admission loses the claim and returns the winner's owned run
    ///    (exactly one `SessionId` per schedule).
    /// 3. A stale claim (schedule `Running` with a `current_session_id`
    ///    whose run row does not exist) is NOT cleared — boot recovery
    ///    classifies it; a second admission never mints another session.
    /// 4. `_system.*` presets are refused regardless of stored policy
    ///    (A8 fail-closed, C-4).
    /// 5. Only after the run exists does the coordinator register the
    ///    runner and call [`ensure_driving`](Self::ensure_driving).
    ///
    /// Re-entry (a concurrent tick / explicit start racing) returns the same
    /// owned run: the schedule already carries `current_session_id`, so the
    /// second caller observes the existing run and drives it (single-flight).
    ///
    /// # Errors
    /// Returns [`RunControlError`] on any admission failure. A failed
    /// admission never marks the row `Running` without a session.
    #[allow(clippy::too_many_lines)] // a single eligibility + admission + drive handoff sequence; splitting obscures ordering
    pub async fn admit_schedule(
        &self,
        schedule_id: &str,
        pool: &sqlx::SqlitePool,
        nexus_home: &std::path::Path,
        caps: &nexus_orchestration::CapabilityRegistryHolder,
        daemon_tool_dispatch: Option<
            std::sync::Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>,
        >,
        prompt_executor: Option<
            std::sync::Arc<dyn nexus_orchestration::capability::PromptExecutor>,
        >,
    ) -> Result<SessionId, RunControlError> {
        // 1. Load the schedule row.
        let row = sqlx::query_as!(
            ScheduleAdmissionRow,
            r#"SELECT schedule_id as "schedule_id!", creator_id as "creator_id!",
                    preset_id as "preset_id!", status as "status!",
                    current_core_context_version as "current_core_context_version!",
                    current_session_id, execution_policy as "execution_policy!",
                    work_id, execution_descriptor_json
             FROM creator_schedules WHERE schedule_id = ?"#,
            schedule_id
        )
        .fetch_optional(pool)
        .await
        .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?
        .ok_or_else(|| RunControlError::ScheduleNotFound(schedule_id.to_string()))?;

        // A8 fail-closed (C-4): a `_system.*` preset is never driven,
        // regardless of the stored policy.
        if row.preset_id.starts_with("_system.") {
            return Err(RunControlError::NotEligible(
                schedule_id.to_string(),
                format!(
                    "system preset '{}' cannot be admitted as a driven run",
                    row.preset_id
                ),
            ));
        }
        let policy_ok =
            row.execution_policy == "driven_v1" || row.execution_policy == "legacy_inert";
        if !policy_ok {
            return Err(RunControlError::NotEligible(
                schedule_id.to_string(),
                format!("execution_policy is '{}'", row.execution_policy),
            ));
        }
        let status_ok = row.status == "pending"
            || row.status == "paused"
            || (row.status == "running" && row.current_session_id.is_none());
        if !status_ok {
            return Err(RunControlError::NotEligible(
                schedule_id.to_string(),
                format!("status is '{}'", row.status),
            ));
        }
        if let Some(sid) = &row.current_session_id {
            // Re-entry: the schedule already owns a run. Verify the run
            // actually exists; a stale claim (crash between claim and run
            // creation) is NOT cleared — boot recovery classifies it (C-1).
            let store = nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(
                std::sync::Arc::new(pool.clone()),
            );
            let run_exists = store
                .load_run(&nexus_orchestration::SessionId(sid.clone()))
                .await
                .map_err(|e| RunControlError::Admission(e.to_string()))?
                .is_some();
            if run_exists {
                let sid = nexus_orchestration::SessionId(sid.clone());
                self.ensure_driving(&sid).await?;
                return Ok(sid);
            }
            return Err(RunControlError::NotEligible(
                schedule_id.to_string(),
                format!(
                    "schedule has a stale claim on session {sid}; \
                     boot recovery must classify it before re-admission"
                ),
            ));
        }

        // N-1: apply the SAME scheduled-at/dependency/concurrency eligibility
        // decision the supervisor tick uses. A blocked row (unfinished
        // dependency, serial conflict, future scheduled_at) remains pending
        // and unowned — immediate public admission must not bypass the
        // admission matrix. The preflight is advisory; the store re-verifies
        // the matrix inside the claim transaction (N-1).
        if !schedule_eligible(pool, &row).await? {
            return Err(RunControlError::NotEligible(
                schedule_id.to_string(),
                "dependency/concurrency/scheduled_at gate not satisfied".to_string(),
            ));
        }
        // Snapshot the matrix for the store's in-transaction recheck (N-1).
        let admission_gate = nexus_orchestration::run_state::ScheduleAdmissionGate {
            creator_id: row.creator_id.clone(),
            scheduled_at: sqlx::query_scalar!(
                "SELECT scheduled_at FROM creator_schedules WHERE schedule_id = ?",
                schedule_id
            )
            .fetch_one(pool)
            .await
            .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?,
            depends_on: sqlx::query_scalar!(
                r#"SELECT depends_on as "depends_on!" FROM schedule_dependencies WHERE schedule_id = ?"#,
                schedule_id
            )
            .fetch_all(pool)
            .await
            .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?,
            concurrency_kind: sqlx::query_scalar!(
                r#"SELECT concurrency_kind as "concurrency_kind!" FROM creator_schedules WHERE schedule_id = ?"#,
                schedule_id
            )
            .fetch_one(pool)
            .await
            .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?,
            concurrency_whitelist: sqlx::query_scalar!(
                "SELECT concurrency_whitelist FROM creator_schedules WHERE schedule_id = ?",
                schedule_id
            )
            .fetch_one(pool)
            .await
            .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?,
        };

        // 2. Resolve the preset and freeze the descriptor + core seed.
        let registry = caps.get().ok_or_else(|| {
            RunControlError::NoWorkspace("capability registry unavailable".into())
        })?;
        let loaded =
            nexus_preset::resolve_preset(&row.preset_id, nexus_home, &registry)
                .map_err(|e| RunControlError::PresetLoad(row.preset_id.clone(), e.to_string()))?;

        // 3. Freeze the core-context seed at the EXACT version persisted with
        //    the schedule row (N-3/I-6): admission reads the frozen version,
        //    never the latest snapshot — a concurrent edit cannot change the
        //    run payload between seed and admission. A read/migration/
        //    corruption error FAILS CLOSED. A missing version record at ANY
        //    pointer (including 0) is a pre-seed row and is refused — never
        //    interpreted as a valid empty seed (N-3).
        let (core_context, core_context_version) = {
            let mgr = nexus_orchestration::schedule::derivation::CoreContextManager::new(
                std::sync::Arc::new(pool.clone()),
            );
            let sid = nexus_contracts::local::schedule::ScheduleId(schedule_id.to_string());
            let frozen_version = u32::try_from(row.current_core_context_version).unwrap_or(0);
            match mgr
                .read(
                    &sid,
                    nexus_contracts::local::schedule::CoreContextVersion(frozen_version),
                )
                .await
            {
                Ok(record) => {
                    let version = record.version.0;
                    let body = match record.content {
                        nexus_contracts::local::schedule::CoreContextPayload::Text { body } => body,
                        nexus_contracts::local::schedule::CoreContextPayload::Struct { body } => {
                            serde_json::to_string(&body).map_err(|e| {
                                RunControlError::Admission(format!(
                                    "core-context struct serialization failed: {e}"
                                ))
                            })?
                        }
                    };
                    (Some(body), version)
                }
                Err(
                    nexus_orchestration::schedule::derivation::CoreContextError::VersionNotFound(
                        _,
                        version,
                    ),
                ) => {
                    return Err(RunControlError::Admission(format!(
                        "core-context version {version} is missing; \
                         the schedule row is not durably seeded (pre-seed rows are never admitted)"
                    )));
                }
                Err(e) => {
                    return Err(RunControlError::Admission(format!(
                        "core-context snapshot read failed: {e}"
                    )));
                }
            }
        };

        // 4. Recover the frozen admission payload (I-5): the schedule's
        //    work_id, structured input, agent bindings, and content-addressed
        //    source identity. The descriptor JSON (when present) carries the
        //    frozen input/bindings/source; the schedule row carries work_id.
        //    New rows created through the public add path persist the
        //    descriptor at insertion.
        let (work_id, input, agent_bindings, frozen_source) = match &row.execution_descriptor_json {
            Some(bytes) => {
                let descriptor: nexus_orchestration::run_state::RunDescriptorV1 =
                    serde_json::from_slice(bytes).map_err(|e| {
                        RunControlError::Admission(format!(
                            "schedule execution descriptor is corrupt: {e}"
                        ))
                    })?;
                (
                    descriptor.work_id.clone(),
                    descriptor.input.clone(),
                    descriptor.agent_bindings.clone(),
                    Some(descriptor.source),
                )
            }
            None => (
                row.work_id.clone().filter(|id| !id.is_empty()),
                serde_json::Map::new(),
                std::collections::HashMap::new(),
                None,
            ),
        };
        // N-4: validate role/provider references BEFORE enqueue — the same
        // gate session POST uses. Unknown roles/providers refuse before any
        // run row is created.
        validate_agent_bindings(&loaded, &agent_bindings, self.provider_catalog.as_ref()).await?;

        // 5. Mint the owned session id and admit atomically (C-1).
        let session_id = format!("{}:{}", row.preset_id, uuid::Uuid::new_v4());

        // Build the wired outer graph (production prompt executor + tool
        // dispatch + cancellation tokens) — mirrors boot.rs.
        let engine_proxy: Arc<dyn OrchestrationEngine> = self.engine.clone();
        let wired = nexus_orchestration::preset_runtime::build_wired_outer_graph(
            &loaded,
            &engine_proxy,
            &registry,
            daemon_tool_dispatch,
            prompt_executor,
            self.session_cancels.clone(),
        )
        .map_err(|e| RunControlError::PresetLoad(row.preset_id.clone(), e.to_string()))?;

        // 6. One Creator-DB transaction: schedule claim + v1 session row +
        //    schedule→session identity (C-1/C-2). A concurrent admission
        //    loses the claim inside the transaction and the run row is
        //    rolled back with it.
        let admitted = self
            .engine
            .admit_schedule_run_with_input(
                schedule_id,
                &session_id,
                &loaded,
                &row.creator_id,
                work_id,
                input,
                core_context.as_deref(),
                core_context_version,
                core_context_version,
                agent_bindings,
                frozen_source,
                Some(admission_gate),
                Arc::new(wired),
            )
            .await;

        match admitted {
            Ok(sid) => {
                // 7. Drive the owned run (single-flight).
                self.ensure_driving(&sid).await?;
                Ok(sid)
            }
            Err(e) => {
                let msg = e.to_string();
                // A concurrent admission won the claim between our read and
                // the store transaction: the store reports the row as
                // already claimed ("claimed concurrently", "already owns
                // run") or as `running` with an owned session ("status is
                // 'running'" — the store's status gate rejects the claimed
                // row before its re-entry check). In every case the winner's
                // owned run is re-read and driven (single-flight).
                if msg.contains("claimed concurrently")
                    || msg.contains("already owns run")
                    || msg.contains("status is 'running'")
                {
                    let winner: Option<String> = sqlx::query_scalar!(
                        "SELECT current_session_id FROM creator_schedules WHERE schedule_id = ?",
                        schedule_id
                    )
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?
                    .flatten();
                    if let Some(sid) = winner {
                        let sid = SessionId(sid);
                        self.ensure_driving(&sid).await?;
                        return Ok(sid);
                    }
                }
                // The row settled TERMINAL between our preflight read and
                // the store transaction (an operator cancel/fail/settle
                // landed in between): the admission is refused for the same
                // reason a preflight terminal status is — not eligible.
                // Surfacing it as `Admission` would project a 500 for a
                // legitimate control race.
                if msg.contains("status is 'cancelled'")
                    || msg.contains("status is 'failed'")
                    || msg.contains("status is 'completed'")
                {
                    return Err(RunControlError::NotEligible(schedule_id.to_string(), msg));
                }
                // N-1: the store's in-transaction matrix recheck refused the
                // row (dependency not satisfied, not due, or the per-creator
                // concurrency gate) — the row stays pending and unowned, the
                // same disposition as a preflight refusal.
                if msg.contains("fails the per-creator concurrency gate")
                    || msg.contains("dependency")
                    || msg.contains("not due")
                {
                    return Err(RunControlError::NotEligible(schedule_id.to_string(), msg));
                }
                Err(RunControlError::Admission(msg))
            }
        }
    }

    /// Route a public control signal to the run's engine transition path
    /// (A4/A5) and re-drive when the signal makes the run runnable.
    ///
    /// - `Continue { wait_id }` → the engine's revision-fenced wait CAS
    ///   clears the exact durable wait (root + nested child atomically) and
    ///   flips the run to `Running`; the coordinator then re-drives the
    ///   session (single-flight — a duplicate consumer loses the CAS and
    ///   never starts a second driver).
    /// - `Cancel` → the engine's A5 cancel path: durable cancel-intent
    ///   fence, coordinator token fire, bounded owned-Host teardown, then
    ///   terminal `Cancelled` only when stop is confirmed (unconfirmed →
    ///   `Interrupted`).
    /// - `Pause` / `Resume` → the engine's non-wait transitions.
    ///
    /// Returns the persisted status and current wait id (null when not
    /// waiting) so the caller can project the exact A4/A5 envelope.
    ///
    /// # Errors
    /// Returns [`RunControlError::WaitConflict`] for a stale/consumed/wrong
    /// wait token (no mutation), [`RunControlError::StateConflict`] for a
    /// wrong current state, and other [`RunControlError`] variants for
    /// storage/engine failures.
    // Signal dispatch is one linear match over the five public signals; the
    // branches share the same CAS/settlement discipline, so splitting would
    // duplicate it.
    #[allow(clippy::too_many_lines)]
    pub async fn signal_run(
        &self,
        session_id: &SessionId,
        signal: RunSignal,
    ) -> Result<RunControlResult, RunControlError> {
        // A7 rule 4: an authorized continuation of a human wait requires the
        // run's runner to be REATTACHED from the frozen source identity —
        // attach on demand when recovery did not (or could not) leave a
        // runner. A changed/missing source, corrupt child identity, or any
        // other reconstruction failure surfaces `reconstruction_unavailable`
        // BEFORE any signal mutates the durable wait: the human wait stays
        // preserved and legal actions become cancel-only. The gate is
        // evaluated only for Continue (the only signal that would drive a
        // recovered session); Cancel/Pause/Resume do not require a runner.
        //
        // The gate is idempotent and race-safe: a session already being
        // driven (or reconstructed) by another caller has a runner present
        // and passes immediately; the durable continue CAS below is still
        // the single linearization point for consuming the wait.
        if matches!(signal, RunSignal::Continue { .. }) {
            if let Err(e) = self.engine.ensure_recovered_runner_inner(session_id).await {
                return Err(RunControlError::ReconstructionUnavailable {
                    session_id: session_id.0.clone(),
                    reason: e.to_string(),
                });
            }
        }

        // QC2 F-002: Cancel has ONE projection owner (`cancel_run`) shared by
        // the session and schedule public surfaces — no per-handler error
        // strings, no divergent 500, and the durable `interrupted` outcome is
        // reported as a typed success instead of an engine error.
        if matches!(signal, RunSignal::Cancel) {
            return self.cancel_run(session_id).await;
        }

        let engine_signal = match &signal {
            RunSignal::Continue { wait_id } => EngineSignal::Continue {
                wait_id: wait_id.clone(),
            },
            RunSignal::Cancel => unreachable!("cancel is projected by `cancel_run`"),
            RunSignal::Pause => EngineSignal::Pause,
            RunSignal::Resume => EngineSignal::Resume,
        };

        let engine_result = self.engine.signal(session_id, engine_signal).await;
        let engine_result = match engine_result {
            Ok(()) => Ok(()),
            // Finding 2: a CAS/revision loss is a linearized control race —
            // the durable winner is safe, but the losing public operation
            // must be an exact conflict response, never a 500. Reload the
            // authoritative row and project the exact envelope: a still-
            // present wait keeps the A4 `workflow_wait_conflict` details
            // (session_id, current status, current_wait_id); a
            // cancel-requested/terminal/incompatible state is
            // `workflow_state_conflict`.
            Err(nexus_orchestration::engine::EngineError::RevisionMismatch {
                session_id: lost_sid,
                ..
            }) => match self.workflow_store.load_run(session_id).await {
                Ok(Some(record)) => {
                    let state = record.state.as_ref();
                    let cancel_requested = state.is_some_and(|s| s.cancel_requested);
                    if record.status == SessionStatus::WaitingForInput && !cancel_requested {
                        let current_wait_id = state
                            .and_then(|s| s.wait.as_ref())
                            .map(|w| w.wait_id.clone());
                        Err(RunControlError::WaitConflict {
                            session_id: lost_sid,
                            status: record.status.as_db_str().to_string(),
                            current_wait_id,
                        })
                    } else {
                        Err(RunControlError::StateConflict(
                            lost_sid,
                            format!(
                                "revision moved; current status is {}",
                                record.status.as_db_str()
                            ),
                        ))
                    }
                }
                Ok(None) => Err(RunControlError::ScheduleNotFound(lost_sid)),
                Err(e) => Err(RunControlError::Drive(e.to_string())),
            },
            Err(e) => Err(match e {
                nexus_orchestration::engine::EngineError::WaitConflict {
                    session_id,
                    status,
                    current_wait_id,
                } => RunControlError::WaitConflict {
                    session_id,
                    status: status.as_db_str().to_string(),
                    current_wait_id,
                },
                nexus_orchestration::engine::EngineError::TerminalState(sid) => {
                    RunControlError::StateConflict(
                        sid,
                        "run is terminal or not in a signalable state".to_string(),
                    )
                }
                nexus_orchestration::engine::EngineError::SessionNotFound(sid) => {
                    RunControlError::ScheduleNotFound(sid)
                }
                other => RunControlError::Drive(other.to_string()),
            }),
        };
        engine_result?;

        // A successful Continue makes the run runnable: re-drive it through
        // the single coordinator owner (single-flight — a duplicate consumer
        // that lost the CAS never reaches this point).
        if matches!(signal, RunSignal::Continue { .. }) {
            self.ensure_driving(session_id).await?;
        }

        // Project the persisted status + current wait id for the response.
        let store =
            nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(self.pool.clone());
        let record = store
            .load_run(session_id)
            .await
            .map_err(|e| RunControlError::Drive(e.to_string()))?
            .ok_or_else(|| RunControlError::ScheduleNotFound(session_id.0.clone()))?;
        let current_wait_id = record
            .state
            .as_ref()
            .and_then(|s| s.wait.as_ref())
            .map(|w| w.wait_id.clone());
        if record.status.is_terminal() {
            self.publish_durable_run_state(session_id).await;
        }
        Ok(RunControlResult {
            status: record.status.as_db_str().to_string(),
            current_wait_id,
            cancel_outcome: None,
        })
    }

    /// Cancel one driven run and project the DURABLE outcome (QC2 F-002).
    ///
    /// The single production cancel path for BOTH public surfaces (session
    /// signal and schedule signal). The engine performs the revision-fenced
    /// cancel (durable cancel-intent fence → bounded owned-Host teardown →
    /// terminal `cancelled`, or durable `interrupted` when cleanup cannot be
    /// confirmed). Whatever the engine returns, the coordinator re-reads the
    /// durable record and:
    ///
    /// - publishes the durable winner to the run's SSE ring and projects it
    ///   as a TYPED success ([`CancelOutcome`]) whenever the run's cancel
    ///   goal is durably accomplished — confirmed `cancelled`, unconfirmed
    ///   `interrupted`, or the `driver_failed` failure the cancel converged
    ///   on — including a fence loss that lost to a concurrent winner;
    /// - otherwise maps the failure to the exact typed conflict envelope
    ///   (wait conflict / state conflict / not-found).
    ///
    /// It never inspects an error message and never fabricates success for a
    /// run that did not actually stop.
    ///
    /// # Errors
    /// Returns [`RunControlError`] for the typed conflict envelopes and for
    /// storage failures.
    pub async fn cancel_run(
        &self,
        session_id: &SessionId,
    ) -> Result<RunControlResult, RunControlError> {
        let engine_result = self.engine.signal(session_id, EngineSignal::Cancel).await;

        let durable = self
            .workflow_store
            .load_run(session_id)
            .await
            .map_err(|e| RunControlError::Drive(e.to_string()))?;

        if let Some(record) = &durable {
            if let Some(outcome) = classify_cancel_outcome(record) {
                self.publish_durable_run_state(session_id).await;
                return Ok(RunControlResult {
                    status: record.status.as_db_str().to_string(),
                    current_wait_id: None,
                    cancel_outcome: Some(outcome),
                });
            }
        }

        match engine_result {
            Ok(()) => {
                let record = durable
                    .ok_or_else(|| RunControlError::ScheduleNotFound(session_id.0.clone()))?;
                self.publish_durable_run_state(session_id).await;
                Ok(RunControlResult {
                    status: record.status.as_db_str().to_string(),
                    current_wait_id: None,
                    cancel_outcome: None,
                })
            }
            Err(nexus_orchestration::engine::EngineError::RevisionMismatch {
                session_id: lost_sid,
                ..
            }) => {
                let record =
                    durable.ok_or_else(|| RunControlError::ScheduleNotFound(lost_sid.clone()))?;
                let state = record.state.as_ref();
                let cancel_requested = state.is_some_and(|s| s.cancel_requested);
                if record.status == SessionStatus::WaitingForInput && !cancel_requested {
                    let current_wait_id = state
                        .and_then(|s| s.wait.as_ref())
                        .map(|w| w.wait_id.clone());
                    Err(RunControlError::WaitConflict {
                        session_id: lost_sid,
                        status: record.status.as_db_str().to_string(),
                        current_wait_id,
                    })
                } else {
                    Err(RunControlError::StateConflict(
                        lost_sid,
                        format!(
                            "revision moved; current status is {}",
                            record.status.as_db_str()
                        ),
                    ))
                }
            }
            Err(e) => Err(match e {
                nexus_orchestration::engine::EngineError::WaitConflict {
                    session_id,
                    status,
                    current_wait_id,
                } => RunControlError::WaitConflict {
                    session_id,
                    status: status.as_db_str().to_string(),
                    current_wait_id,
                },
                nexus_orchestration::engine::EngineError::TerminalState(sid) => {
                    RunControlError::StateConflict(
                        sid,
                        "run is terminal or not in a signalable state".to_string(),
                    )
                }
                nexus_orchestration::engine::EngineError::SessionNotFound(sid) => {
                    RunControlError::ScheduleNotFound(sid)
                }
                other => RunControlError::Drive(other.to_string()),
            }),
        }
    }
}

/// Outcome of [`WorkflowRunCoordinator::ensure_driving`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveDisposition {
    /// A fresh drive loop was started for the session.
    Started,
    /// A drive loop was already running for the session; no second driver.
    AlreadyDriving,
    /// The session's durable state is terminal/interrupted/human-wait —
    /// no fresh drive is authorized (I-2).
    NotDriving,
}

/// Schedule row projection for coordinator admission.
#[derive(sqlx::FromRow)]
struct ScheduleAdmissionRow {
    schedule_id: String,
    creator_id: String,
    preset_id: String,
    status: String,
    current_core_context_version: i64,
    current_session_id: Option<String>,
    execution_policy: String,
    work_id: Option<String>,
    execution_descriptor_json: Option<Vec<u8>>,
}

/// Evaluate the SAME scheduled-at/dependency/concurrency eligibility
/// decision the supervisor tick uses (N-1), for immediate public admission.
///
/// A row is eligible only when: `scheduled_at` is absent or due, every
/// `depends_on` entry is completed/cancelled, and the per-creator
/// concurrency rule passes (serial: no other driven run for the creator;
/// `parallel_with`: every running driven run is whitelisted; `parallel_any`:
/// always). Only `driven_v1` rows count toward the running set — legacy/
/// system-inert rows are excluded from execution capacity (A3).
async fn schedule_eligible(
    pool: &sqlx::SqlitePool,
    row: &ScheduleAdmissionRow,
) -> Result<bool, RunControlError> {
    // scheduled_at gate (same shape as the supervisor's clocked tick).
    let scheduled_at: Option<i64> = sqlx::query_scalar!(
        "SELECT scheduled_at FROM creator_schedules WHERE schedule_id = ?",
        row.schedule_id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?;
    if let Some(at) = scheduled_at {
        if at > chrono::Utc::now().timestamp() {
            return Ok(false);
        }
    }

    // Load the candidate's concurrency + dependencies.
    let concurrency_kind: String =
        sqlx::query_scalar!(
            r#"SELECT concurrency_kind as "concurrency_kind!" FROM creator_schedules WHERE schedule_id = ?"#,
            row.schedule_id
        )
        .fetch_one(pool)
        .await
        .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?;
    let concurrency_whitelist: Option<String> = sqlx::query_scalar!(
        "SELECT concurrency_whitelist FROM creator_schedules WHERE schedule_id = ?",
        row.schedule_id
    )
    .fetch_one(pool)
    .await
    .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?;
    let deps: Vec<String> = sqlx::query_scalar!(
        r#"SELECT depends_on as "depends_on!" FROM schedule_dependencies WHERE schedule_id = ?"#,
        row.schedule_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?;

    // Completed/cancelled set for dependency satisfaction (Failed does not
    // satisfy — spec §4).
    let completed: std::collections::HashSet<String> = sqlx::query_scalar!(
        r#"SELECT schedule_id as "schedule_id!" FROM creator_schedules WHERE status IN ('completed', 'cancelled')"#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?
    .into_iter()
    .collect();
    for dep in &deps {
        if !completed.contains(dep) {
            return Ok(false);
        }
    }

    // Per-creator running set: only driven_v1 rows count toward capacity.
    // The candidate's OWN row is excluded — a concurrent admission that
    // already claimed this row must not make the candidate look
    // serial-blocked; the loser re-reads the winner's owned run instead.
    let running: Vec<String> = sqlx::query_scalar!(
        r#"SELECT schedule_id as "schedule_id!" FROM creator_schedules
         WHERE creator_id = ? AND status = 'running'
           AND execution_policy = 'driven_v1'
           AND schedule_id != ?"#,
        row.creator_id,
        row.schedule_id
    )
    .fetch_all(pool)
    .await
    .map_err(|e| RunControlError::ScheduleUpdate(e.to_string()))?;

    match concurrency_kind.as_str() {
        "parallel_any" => Ok(true),
        "parallel_with" => {
            let whitelist: std::collections::HashSet<String> = concurrency_whitelist
                .as_deref()
                .and_then(|json| serde_json::from_str(json).ok())
                .unwrap_or_default();
            Ok(running.iter().all(|id| whitelist.contains(id)))
        }
        // "serial" (and any unknown kind — conservative fail-closed).
        _ => Ok(running.is_empty()),
    }
}

/// Validate role/provider references in agent bindings BEFORE enqueue (N-4).
///
/// Every binding role must be a declared preset role (or `default`), and
/// every provider id must exist in the Host provider catalog. Unresolved
/// keys refuse before any run row is created or any external effect runs.
/// When no Host facade is wired (tests / Tier-0), provider existence cannot
/// be verified — only the nonempty shape is enforced (the Host executor
/// still refuses unknown providers at prompt time).
///
/// N-4b: the COMPLETE prompt-role set is derived from the resolved outer
/// and inner graphs and every effective binding (including `default`) is
/// REQUIRED before enqueue. A prompt path is any `llm_judge` exit (the
/// judge capability resolves the `default` role) or any `acp_prompt` inner
/// node (its `agent` field, or `default` when absent). A preset with NO
/// prompt path may accept an empty binding map — proven by graph
/// inspection, never by deferring to the prompt executor.
async fn validate_agent_bindings(
    loaded: &nexus_preset::LoadedPreset,
    bindings: &std::collections::HashMap<String, nexus_orchestration::run_state::AgentBinding>,
    catalog: Option<&Arc<dyn ProviderCatalogPort>>,
) -> Result<(), RunControlError> {
    let role_ids: std::collections::HashSet<&str> =
        loaded.roles.iter().map(|r| r.id.as_str()).collect();

    // N-4b: the COMPLETE prompt-role set is derived from the resolved outer
    // and inner graphs via the shared helper (same source the daemon's
    // internal binding builder uses — they can never drift).
    let required = nexus_preset::required_prompt_roles(loaded);

    // Every required role must have an effective binding (N-4b). An empty
    // map is accepted ONLY when the preset has no prompt path at all.
    for role in &required {
        if !bindings.contains_key(role) {
            return Err(RunControlError::Admission(format!(
                "missing agent binding for required role '{role}' \
                 (the preset's graphs issue prompts for this role; \
                 bind every prompt role before enqueue)"
            )));
        }
    }

    for (role, binding) in bindings {
        if role.is_empty() || binding.provider_id.trim().is_empty() {
            return Err(RunControlError::Admission(format!(
                "invalid agent binding for role '{role}'"
            )));
        }
        if role != "default" && !role_ids.contains(role.as_str()) {
            return Err(RunControlError::Admission(format!(
                "unknown role '{role}' in agent bindings (declared roles: {})",
                if role_ids.is_empty() {
                    "none — single-agent presets accept only 'default'".to_string()
                } else {
                    role_ids
                        .iter()
                        .map(|r| format!("'{r}'"))
                        .collect::<Vec<_>>()
                        .join(", ")
                }
            )));
        }
        if let Some(catalog) = catalog {
            let known = catalog
                .provider_available(&binding.provider_id)
                .await
                .map_err(|e| {
                    RunControlError::Admission(format!("provider catalog unavailable: {e}"))
                })?;
            if !known {
                return Err(RunControlError::Admission(format!(
                    "unknown provider '{}' for role '{role}'",
                    binding.provider_id
                )));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
