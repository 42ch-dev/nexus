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
//! `"failed"`, `_run_error` = error text). Authoritative v1 cleanup preserves
//! durable `Failed` status after stopping owned resources; user cancellation
//! remains a distinct outcome.

use std::sync::Arc;
use std::time::Duration;

use graph_flow::SessionStorage;
use nexus_orchestration::engine::{
    EngineError, EngineSignal, FailedStepWitness, FailurePersistenceDisposition, FailureSettlement,
    FailureSettlementSummary, OrchestrationEngine, SessionId, SessionStatus, SessionSummary,
    StepOutcome,
};
use nexus_orchestration::resume_rules;
use nexus_orchestration::run_state::WorkflowStateStore;
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
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
            Ok(StepOutcome::Error(msg)) => {
                let settlement =
                    settle_drive_failure(engine, workflow_store, storage, session_id, &msg, None)
                        .await;
                return failed_outcome(steps.saturating_add(1), msg, &settlement);
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
            EngineSignal::CancelAnchored {
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
}

/// Cancel fence-loss idempotence (A5): a `Cancel` signal that exhausts the
/// engine's revision-fence retry bound and loses the race still has its
/// goal durably accomplished when the record already settled `cancelled`
/// (confirmed stop) or `interrupted` (unconfirmed stop) — a concurrent
/// cancel committed the same intent. The loser reports the persisted
/// outcome as success instead of a conflict that would contradict the
/// durable truth. A run that settled `completed`/`failed` (or is still
/// live) keeps the conflict: the cancel genuinely did not happen.
const fn cancel_fence_loss_accomplished(status: &SessionStatus, cancel_requested: bool) -> bool {
    // `Cancelled` is the confirmed A5 outcome. `Interrupted` only counts when
    // the durable state carries the cancel intent: an `interrupted` produced
    // by a failed state transition (no `cancel_requested`) is NOT proof that
    // this cancel succeeded, so it must stay a conflict (Greptile P1).
    matches!(status, SessionStatus::Cancelled)
        || (matches!(status, SessionStatus::Interrupted) && cancel_requested)
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
    agent_host: Option<Arc<dyn nexus_agent_host::HostFacade>>,
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
            agent_host: None,
            binding_provider: None,
            session_cancels,
            drives: Arc::new(tokio::sync::Mutex::new(std::collections::HashMap::new())),
            failed_owners: Arc::new(tokio::sync::Mutex::new(std::collections::HashSet::new())),
            workflow_store,
            config: PresetRunConfig::default(),
            schedule_supervisor: std::sync::Arc::new(std::sync::RwLock::new(None)),
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
    pub fn with_agent_host(mut self, host: Arc<dyn nexus_agent_host::HostFacade>) -> Self {
        self.agent_host = Some(host);
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
        let loaded = nexus_orchestration::preset::resolve_preset(preset_id, nexus_home, &registry)
            .map_err(|e| RunControlError::PresetLoad(preset_id.to_string(), e.to_string()))?;

        // N-4: validate role/provider references BEFORE enqueue. Every
        // binding role must be a declared preset role (or `default`), and
        // every provider id must exist in the Host provider catalog.
        validate_agent_bindings(&loaded, &agent_bindings, self.agent_host.as_ref()).await?;

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
        let wired = nexus_orchestration::preset::loader::build_wired_outer_graph(
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

        // Register (or refresh) the run's cancellation token in the shared
        // map so the prompt executor and out-of-band cancel resolve the SAME
        // token the drive loop checks.
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
    ///    [`nexus_orchestration::preset::resolve_preset`] admission uses).
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
            nexus_orchestration::preset::resolve_preset(&row.preset_id, nexus_home, &registry)
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
        let roles = nexus_orchestration::preset::required_prompt_roles(&loaded);
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
            nexus_orchestration::preset::resolve_preset(&row.preset_id, nexus_home, &registry)
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
        validate_agent_bindings(&loaded, &agent_bindings, self.agent_host.as_ref()).await?;

        // 5. Mint the owned session id and admit atomically (C-1).
        let session_id = format!("{}:{}", row.preset_id, uuid::Uuid::new_v4());

        // Build the wired outer graph (production prompt executor + tool
        // dispatch + cancellation tokens) — mirrors boot.rs.
        let engine_proxy: Arc<dyn OrchestrationEngine> = self.engine.clone();
        let wired = nexus_orchestration::preset::loader::build_wired_outer_graph(
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

        let engine_signal = match &signal {
            RunSignal::Continue { wait_id } => EngineSignal::Continue {
                wait_id: wait_id.clone(),
            },
            RunSignal::Cancel => EngineSignal::Cancel,
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
                session_id, ..
            }) => {
                let store = nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(
                    self.pool.clone(),
                );
                match store.load_run(&SessionId(session_id.clone())).await {
                    Ok(Some(record)) => {
                        let state = record.state.as_ref();
                        let cancel_requested = state.is_some_and(|s| s.cancel_requested);
                        // A5 idempotence: a Cancel that lost the fence race
                        // but finds the run durably settled (cancelled /
                        // interrupted) has its goal accomplished by the
                        // concurrent winner — project the persisted outcome
                        // as success, never a conflict.
                        if matches!(signal, RunSignal::Cancel)
                            && cancel_fence_loss_accomplished(&record.status, cancel_requested)
                        {
                            Ok(())
                        } else if record.status == SessionStatus::WaitingForInput
                            && !cancel_requested
                        {
                            let current_wait_id = state
                                .and_then(|s| s.wait.as_ref())
                                .map(|w| w.wait_id.clone());
                            Err(RunControlError::WaitConflict {
                                session_id,
                                status: record.status.as_db_str().to_string(),
                                current_wait_id,
                            })
                        } else {
                            Err(RunControlError::StateConflict(
                                session_id,
                                format!(
                                    "revision moved; current status is {}",
                                    record.status.as_db_str()
                                ),
                            ))
                        }
                    }
                    Ok(None) => Err(RunControlError::ScheduleNotFound(session_id)),
                    Err(e) => Err(RunControlError::Drive(e.to_string())),
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
        Ok(RunControlResult {
            status: record.status.as_db_str().to_string(),
            current_wait_id,
        })
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
    loaded: &nexus_orchestration::preset::LoadedPreset,
    bindings: &std::collections::HashMap<String, nexus_orchestration::run_state::AgentBinding>,
    host: Option<&Arc<dyn nexus_agent_host::HostFacade>>,
) -> Result<(), RunControlError> {
    let role_ids: std::collections::HashSet<&str> =
        loaded.roles.iter().map(|r| r.id.as_str()).collect();

    // N-4b: the COMPLETE prompt-role set is derived from the resolved outer
    // and inner graphs via the shared helper (same source the daemon's
    // internal binding builder uses — they can never drift).
    let required = nexus_orchestration::preset::required_prompt_roles(loaded);

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
        if let Some(host) = host {
            let catalog = host.provider_catalog().await.map_err(|e| {
                RunControlError::Admission(format!("provider catalog unavailable: {e}"))
            })?;
            if catalog
                .find(&nexus_agent_host::ProviderId::new(
                    binding.provider_id.clone(),
                ))
                .is_none()
            {
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
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::collections::VecDeque;
    use std::sync::atomic::{AtomicBool, Ordering};

    use nexus_orchestration::engine::{
        ChildSessionParams, Context, EngineError, SessionFilter, SessionKey, SessionSummary,
    };

    /// A5 idempotence: a fence-losing Cancel is a success only when the run
    /// durably settled cancelled/interrupted; every other state (still
    /// live, or settled completed/failed) keeps the conflict — the cancel
    /// genuinely did not happen.
    #[test]
    fn cancel_fence_loss_accomplished_only_for_cancel_outcomes() {
        assert!(cancel_fence_loss_accomplished(
            &SessionStatus::Cancelled,
            false
        ));
        // Interrupted only counts with the durable cancel intent (Greptile P1).
        assert!(cancel_fence_loss_accomplished(
            &SessionStatus::Interrupted,
            true
        ));
        assert!(!cancel_fence_loss_accomplished(
            &SessionStatus::Interrupted,
            false
        ));
        for status in [
            SessionStatus::Running,
            SessionStatus::Paused,
            SessionStatus::WaitingForInput,
            SessionStatus::Completed,
            SessionStatus::Failed,
        ] {
            assert!(
                !cancel_fence_loss_accomplished(&status, true),
                "{status:?} must keep the conflict"
            );
        }
    }
    use nexus_orchestration::storage::sqlite::SqliteSessionStorage;

    /// Delegating [`SessionStorage`] that reports a specific session as
    /// ABSENT (`Ok(None)`) while every other read/write hits the real store —
    /// the "root row vanished before the marker" seam.
    struct MissingRootStorage {
        inner: Arc<dyn SessionStorage>,
        missing: Arc<std::sync::Mutex<Option<String>>>,
    }

    impl MissingRootStorage {
        fn new(inner: Arc<dyn SessionStorage>) -> Self {
            Self {
                inner,
                missing: Arc::new(std::sync::Mutex::new(None)),
            }
        }

        /// Arm the seam for one session id (the row exists; reads report it
        /// absent).
        fn hide(&self, id: &str) {
            *self
                .missing
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(id.to_string());
        }
    }

    #[async_trait]
    impl SessionStorage for MissingRootStorage {
        async fn save(&self, session: graph_flow::Session) -> graph_flow::Result<()> {
            self.inner.save(session).await
        }

        async fn get(&self, id: &str) -> graph_flow::Result<Option<graph_flow::Session>> {
            let hidden = self
                .missing
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if hidden.as_deref() == Some(id) {
                return Ok(None);
            }
            self.inner.get(id).await
        }

        async fn delete(&self, id: &str) -> graph_flow::Result<()> {
            self.inner.delete(id).await
        }
    }

    /// Test wrapper: fail the next `get_status` call, then delegate.
    struct FailGetStatusOnce {
        inner: Arc<dyn OrchestrationEngine>,
        armed: AtomicBool,
        message: String,
    }

    impl FailGetStatusOnce {
        fn new(inner: Arc<dyn OrchestrationEngine>, message: impl Into<String>) -> Self {
            Self {
                inner,
                armed: AtomicBool::new(true),
                message: message.into(),
            }
        }
    }

    #[async_trait]
    impl OrchestrationEngine for FailGetStatusOnce {
        async fn run_step(&self, session_id: &SessionId) -> Result<StepOutcome, EngineError> {
            self.inner.run_step(session_id).await
        }

        async fn new_session(
            &self,
            key: SessionKey,
            ctx: Context,
        ) -> Result<SessionId, EngineError> {
            self.inner.new_session(key, ctx).await
        }

        async fn start_session_with_graph(
            &self,
            id_prefix: &str,
            graph: Arc<graph_flow::Graph>,
        ) -> Result<SessionId, EngineError> {
            self.inner.start_session_with_graph(id_prefix, graph).await
        }

        async fn get_status(&self, session_id: &SessionId) -> Result<SessionStatus, EngineError> {
            if self.armed.swap(false, Ordering::SeqCst) {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(self.message.clone()),
                ));
            }
            self.inner.get_status(session_id).await
        }

        async fn has_runner(&self, session_id: &SessionId) -> bool {
            self.inner.has_runner(session_id).await
        }

        async fn recover_sessions(&self, summaries: Vec<SessionSummary>) {
            self.inner.recover_sessions(summaries).await;
        }

        async fn ensure_recovered_runner(&self, session_id: &SessionId) -> Result<(), EngineError> {
            self.inner.ensure_recovered_runner(session_id).await
        }

        async fn signal(
            &self,
            session_id: &SessionId,
            signal: EngineSignal,
        ) -> Result<(), EngineError> {
            self.inner.signal(session_id, signal).await
        }

        async fn list_active(
            &self,
            filter: SessionFilter,
        ) -> Result<Vec<SessionSummary>, EngineError> {
            self.inner.list_active(filter).await
        }

        async fn spawn_child_session(
            &self,
            params: ChildSessionParams,
        ) -> Result<SessionId, EngineError> {
            self.inner.spawn_child_session(params).await
        }

        async fn attach_existing_child_session(
            &self,
            parent_session_id: &str,
            inner_graph: Arc<graph_flow::Graph>,
        ) -> Result<Option<SessionId>, EngineError> {
            self.inner
                .attach_existing_child_session(parent_session_id, inner_graph)
                .await
        }

        async fn get_context(
            &self,
            session_id: &SessionId,
        ) -> Result<graph_flow::Context, EngineError> {
            self.inner.get_context(session_id).await
        }

        async fn get_current_task_id(
            &self,
            session_id: &SessionId,
        ) -> Result<Option<String>, EngineError> {
            self.inner.get_current_task_id(session_id).await
        }

        async fn start_session_with_preset(
            &self,
            loaded: &nexus_orchestration::preset::LoadedPreset,
        ) -> Result<SessionId, EngineError> {
            self.inner.start_session_with_preset(loaded).await
        }

        async fn start_session_with_preset_for_creator(
            &self,
            loaded: &nexus_orchestration::preset::LoadedPreset,
            creator_id: &str,
        ) -> Result<SessionId, EngineError> {
            self.inner
                .start_session_with_preset_for_creator(loaded, creator_id)
                .await
        }
    }

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

        async fn recover_sessions(&self, _summaries: Vec<SessionSummary>) {
            unreachable!("not used by driver unit tests")
        }

        async fn ensure_recovered_runner(&self, session_id: &SessionId) -> Result<(), EngineError> {
            if !*self.runner_present.lock() {
                return Err(EngineError::GraphFlow(
                    graph_flow::GraphError::StorageError(format!(
                        "scripted engine has no runner for '{}' (reconstruction_unavailable)",
                        session_id.0
                    )),
                ));
            }
            Ok(())
        }

        async fn signal(
            &self,
            _session_id: &SessionId,
            signal: EngineSignal,
        ) -> Result<(), EngineError> {
            self.signals.lock().push(signal.clone());
            if matches!(
                signal,
                EngineSignal::Cancel | EngineSignal::CancelAnchored { .. }
            ) {
                *self.status.lock() = SessionStatus::Cancelled;
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

        async fn attach_existing_child_session(
            &self,
            _: &str,
            _: Arc<graph_flow::Graph>,
        ) -> Result<Option<SessionId>, EngineError> {
            unreachable!("not used by driver unit tests")
        }

        async fn get_context(
            &self,
            _session_id: &SessionId,
        ) -> Result<graph_flow::Context, EngineError> {
            unreachable!("not used by driver unit tests")
        }

        async fn get_current_task_id(
            &self,
            _session_id: &SessionId,
        ) -> Result<Option<String>, EngineError> {
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
        let outcome = drive_preset_run(
            &engine,
            None,
            None,
            &sid(),
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(
            outcome,
            PresetRunOutcome::Completed { steps: 3 },
            "inter-task Paused outcomes step through to completion"
        );
    }

    #[tokio::test]
    async fn stops_at_waiting_for_input_without_resume_flag() {
        let engine = ScriptedEngine::with_script(vec![Ok(waiting())]);
        let outcome = drive_preset_run(
            &engine,
            None,
            None,
            &sid(),
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(outcome, PresetRunOutcome::WaitingForInput { steps: 1 });
    }

    #[tokio::test]
    async fn does_not_step_a_parked_session_without_resume_flag() {
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        *engine.status.lock() = SessionStatus::WaitingForInput;
        let outcome = drive_preset_run(
            &engine,
            None,
            None,
            &sid(),
            &PresetRunConfig::default(),
            None,
        )
        .await;
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
        let outcome = drive_preset_run(&engine, None, None, &sid(), &config, None).await;
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
    async fn step_error_returns_typed_failure_and_cancels_session() {
        let engine = ScriptedEngine::with_script(vec![Err(EngineError::GraphFlow(
            graph_flow::GraphError::TaskExecutionFailed(
                "converge_timeout: gate=converge, state_id=join, arrived=1, expected=2, \
                 elapsed_ms=123"
                    .to_string(),
            ),
        ))]);
        let outcome = drive_preset_run(
            &engine,
            None,
            None,
            &sid(),
            &PresetRunConfig::default(),
            None,
        )
        .await;
        match outcome {
            PresetRunOutcome::Failed { steps, error, .. } => {
                assert_eq!(steps, 1);
                assert!(
                    error.contains("converge_timeout: gate=converge, state_id=join, arrived=1"),
                    "typed discriminator must surface verbatim: {error}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }
        assert_eq!(
            *engine.status.lock(),
            SessionStatus::Running,
            "without a step witness the driver must not send cleanup signals"
        );
        assert!(
            engine.signals.lock().is_empty(),
            "fail-closed: no Cancel without an ownership anchor"
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
            None,
            &sid(),
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert!(matches!(outcome, PresetRunOutcome::Failed { .. }));

        let persisted = storage.get("test:session").await.unwrap().unwrap();
        let status: String = persisted.context.get("_run_status").unwrap();
        let error: String = persisted.context.get("_run_error").unwrap();
        assert_eq!(status, "failed");
        assert!(error.contains("converge_timeout: gate=merge"));
    }
    /// graph-flow 0.8 conflict disposition: a `SessionConflict` from the
    /// engine means a concurrent authoritative writer won the row. The
    /// losing drive must stop WITHOUT the blanket failure path — no Cancel
    /// signal, no status flip, no failure context write against the winner.
    #[tokio::test]
    async fn session_conflict_stops_losing_drive_without_winner_mutation() {
        let storage: Arc<dyn SessionStorage> = Arc::new(graph_flow::InMemorySessionStorage::new());
        let session = graph_flow::Session::new_from_task("test:session".to_string(), "join");
        storage.save(session).await.unwrap();

        let engine = ScriptedEngine::with_script(vec![Err(EngineError::GraphFlow(
            graph_flow::GraphError::SessionConflict(
                "save session 'test:session': graph_version mismatch".to_string(),
            ),
        ))]);
        let outcome = drive_preset_run(
            &engine,
            Some(&storage),
            None,
            &sid(),
            &PresetRunConfig::default(),
            None,
        )
        .await;
        match outcome {
            PresetRunOutcome::SessionConflict { steps, error } => {
                assert_eq!(steps, 1);
                assert!(
                    error.contains("graph_version mismatch"),
                    "the conflict discriminator must surface verbatim: {error}"
                );
            }
            other => panic!("expected SessionConflict, got {other:?}"),
        }
        assert!(
            engine.signals.lock().is_empty(),
            "a losing drive must NOT send Cancel against the winner"
        );
        assert_eq!(
            *engine.status.lock(),
            SessionStatus::Running,
            "the in-memory status must stay the winner's, not flip to Cancelled"
        );
        let persisted = storage.get("test:session").await.unwrap().unwrap();
        assert!(
            persisted.context.get::<String>("_run_status").is_none(),
            "no failure record may be written onto the winner's context"
        );
        assert!(
            persisted.context.get::<String>("_run_error").is_none(),
            "no failure record may be written onto the winner's context"
        );
    }

    /// A task error survives cleanup as durable Failed, including schedule
    /// reconciliation and downstream dependency admission.
    #[tokio::test]
    async fn failed_drive_persists_failure_record_before_terminal_settlement() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "deterministic failure for regression test".to_string(),
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("fail-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("test graph build"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
            storage.clone(),
            store.clone(),
            caps,
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let outcome = drive_preset_run(
            &engine,
            Some(&storage),
            Some(&store),
            &session_id,
            &PresetRunConfig::default(),
            None,
        )
        .await;
        match &outcome {
            PresetRunOutcome::Failed { error, .. } => {
                assert!(
                    error.contains("deterministic failure"),
                    "the task failure must surface verbatim: {error}"
                );
            }
            other => panic!("expected Failed, got {other:?}"),
        }

        // The failure record survived terminal settlement (the fix ordering).
        let persisted = storage
            .get(&session_id.0)
            .await
            .expect("get session")
            .expect("session exists");
        let run_status: String = persisted
            .context
            .get("_run_status")
            .expect("durable failure record must exist");
        let run_error: String = persisted
            .context
            .get("_run_error")
            .expect("durable failure record must exist");
        assert_eq!(run_status, "failed");
        assert!(run_error.contains("deterministic failure"), "{run_error}");

        // Resource cleanup must not change the execution outcome to Cancelled.
        let record = store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(
            record.status,
            SessionStatus::Failed,
            "execution failure must not settle as user cancellation"
        );
        let failure = record
            .state
            .as_ref()
            .and_then(|s| s.failure.as_ref())
            .expect("authoritative RunFailure must be populated");
        assert_eq!(failure.code, "driver_failed");
        assert!(
            failure.message.contains("deterministic failure"),
            "{failure:?}"
        );

        assert_failed_schedule_blocks_dependents(&pool, &session_id).await;
    }

    async fn assert_failed_schedule_blocks_dependents(
        pool: &Arc<sqlx::SqlitePool>,
        session_id: &SessionId,
    ) {
        // Reconcile a checkpoint-before-settlement restart through the real
        // supervisor, then exercise the shared dependency admission gate.
        for (schedule_id, status, current_session_id) in [
            (
                "failed-prerequisite",
                "running",
                Some(session_id.0.as_str()),
            ),
            ("dependent", "pending", None),
        ] {
            // SAFETY: test-only minimal drive-enabled schedule fixtures.
            sqlx::query(
                "INSERT INTO creator_schedules
                 (schedule_id, creator_id, preset_id, preset_version, status,
                  concurrency_kind, current_core_context_version,
                  created_at, updated_at, execution_policy, current_session_id)
                 VALUES (?, 'test-creator', 'novel-writing', 1, ?, 'serial', 0,
                         0, 0, 'driven_v1', ?)",
            )
            .bind(schedule_id)
            .bind(status)
            .bind(current_session_id)
            .execute(pool.as_ref())
            .await
            .expect("schedule fixture");
        }
        // SAFETY: test-only dependency fixture.
        sqlx::query(
            "INSERT INTO schedule_dependencies (schedule_id, depends_on)
             VALUES ('dependent', 'failed-prerequisite')",
        )
        .execute(pool.as_ref())
        .await
        .expect("dependency fixture");
        let supervisor =
            nexus_orchestration::schedule::supervisor::ScheduleSupervisor::new(pool.clone());
        assert_eq!(supervisor.reconcile_terminal_schedules().await.unwrap(), 1);
        assert_eq!(
            supervisor.status_of("failed-prerequisite").await.unwrap(),
            nexus_contracts::local::schedule::ScheduleStatus::Failed,
        );
        // SAFETY: test-only load of the real admission projection.
        let dependent = sqlx::query_as::<_, ScheduleAdmissionRow>(
            "SELECT schedule_id, creator_id, preset_id, status,
                    current_core_context_version, current_session_id,
                    execution_policy, work_id, execution_descriptor_json
             FROM creator_schedules WHERE schedule_id = 'dependent'",
        )
        .fetch_one(pool.as_ref())
        .await
        .expect("dependent admission row");
        assert!(
            !schedule_eligible(pool.as_ref(), &dependent).await.unwrap(),
            "failed prerequisite must keep downstream work blocked"
        );
    }

    #[tokio::test]
    async fn max_steps_bound_is_enforced() {
        let engine = ScriptedEngine::with_script(vec![Ok(paused()), Ok(paused())]);
        let config = PresetRunConfig {
            max_steps: 2,
            ..PresetRunConfig::default()
        };
        let outcome = drive_preset_run(&engine, None, None, &sid(), &config, None).await;
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
        let outcome = drive_preset_run(
            &engine,
            None,
            None,
            &sid(),
            &PresetRunConfig::default(),
            None,
        )
        .await;
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
            session.context.set(*k, v.clone()).unwrap();
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
            None,
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
                None,
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

    // ------------------------------------------------------------------
    // v1.186 P0 Task 2 — A7 canonical recovery gate (durable store path)
    // ------------------------------------------------------------------

    /// Seed a v1 durable row through the SQLite workflow store. `skip_state`
    /// leaves the state blob NULL (corrupt/unsupported metadata).
    async fn seed_v1_row(
        pool: &sqlx::SqlitePool,
        session_id: &str,
        status: &str,
        step_in_flight: Option<&str>,
        cancel_requested: bool,
        wait: bool,
        chain_context: bool,
    ) {
        let context = if chain_context {
            br#"{"data": {"_converge_arrivals_j1": ["a"], "_join_wait_start_j1": 1}, "chat_history": {"messages": [], "max_messages": 1000}}"#
                .to_vec()
        } else {
            br#"{"data": {"_creator_id": "ctr"}, "chat_history": {"messages": [], "max_messages": 1000}}"#
                .to_vec()
        };
        let run_state = serde_json::json!({
            "wait": wait.then(|| serde_json::json!({
                "wait_id": "wait-tok-1", "task_id": "task_7",
                "child_session_id": null, "child_task_id": null, "kind": "manual"
            })),
            "step_in_flight": step_in_flight,
            "in_flight": null,
            "failure": null,
            "cancel_requested": cancel_requested,
        })
        .to_string()
        .into_bytes();
        sqlx::query(
            "INSERT INTO orchestration_sessions
                (session_id, creator_id, preset_id, preset_version, status,
                 current_task_id, context_json, created_at, updated_at,
                 execution_version, state_revision, run_state_json, run_descriptor_json)
             VALUES (?, 'ctr', 'e2e-converge', 3, ?, ?, ?, 1_756_990_000, 1_756_990_300,
                     1, 5, ?, ?)",
        )
        .bind(session_id)
        .bind(status)
        .bind(if wait { Some("task_7") } else { None })
        .bind(context)
        .bind(run_state)
        .bind(
            // A v1 row must carry a valid frozen RunDescriptorV1 (A2).
            br#"{"creator_id":"ctr","work_id":null,"workspace_root":"/tmp/ws",
                 "preset_id":"e2e-converge","preset_version":3,
                 "source":{"Embedded":{"preset_id":"e2e-converge","content_hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},
                 "input":{},"agent_bindings":{},"parent_session_id":null,"graph_name":null}"#
                .as_slice(),
        )
        .execute(pool)
        .await
        .expect("seed v1 row");
    }

    async fn a7_storage(
        db_path: &std::path::Path,
    ) -> (Arc<dyn SessionStorage>, Arc<SqliteSessionStorage>) {
        let pool = nexus_local_db::open_pool(db_path).await.expect("open pool");
        // The create_test_workspace fixture already ran migrations; opening
        // the pool again is idempotent. The same adapter implements both
        // SessionStorage and WorkflowStateStore (A2).
        let sqlite: Arc<SqliteSessionStorage> = Arc::new(SqliteSessionStorage::new(Arc::new(pool)));
        let dyn_storage: Arc<dyn SessionStorage> = sqlite.clone();
        (dyn_storage, sqlite)
    }

    #[tokio::test]
    async fn resume_a7_never_redrives_v1_interrupted_even_with_old_join_keys() {
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        {
            let pool = nexus_local_db::open_pool(&db_path)
                .await
                .expect("open pool");
            seed_v1_row(
                &pool,
                "v1:crashed",
                "running",
                Some("task_3"),
                false,
                false,
                true,
            )
            .await;
            pool.close().await;
        }
        let (storage, sqlite) = a7_storage(&db_path).await;
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        // The summary status the boot recovery filter produced.
        let sum = SessionSummary {
            session_id: SessionId("v1:crashed".to_string()),
            creator_id: "ctr".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("task_3".to_string()),
        };
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            Some(&store),
            &[sum],
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::SkippedInterrupted {
                session_id: SessionId("v1:crashed".to_string())
            }],
            "v1 interrupted in-flight work wins over old join keys and never auto-retries"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for v1 interrupted work"
        );
    }

    #[tokio::test]
    async fn resume_a7_human_wait_is_not_stepped_at_boot_and_token_survives() {
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        {
            let pool = nexus_local_db::open_pool(&db_path)
                .await
                .expect("open pool");
            seed_v1_row(
                &pool,
                "v1:wait",
                "waiting_for_input",
                None,
                false,
                true,
                false,
            )
            .await;
            // A token-bearing wait WITH live scheduler join keys must still
            // classify as a human wait (L2 Important 1): the durable A4 wait
            // token beats old join keys; daemon must not re-drive it.
            seed_v1_row(
                &pool,
                "v1:wait_joins",
                "waiting_for_input",
                None,
                false,
                true,
                true,
            )
            .await;
            pool.close().await;
        }
        let (storage, sqlite) = a7_storage(&db_path).await;
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let sums = vec![
            SessionSummary {
                session_id: SessionId("v1:wait".to_string()),
                creator_id: "ctr".to_string(),
                preset_id: "e2e-converge".to_string(),
                status: SessionStatus::WaitingForInput,
                current_task_id: Some("task_7".to_string()),
            },
            SessionSummary {
                session_id: SessionId("v1:wait_joins".to_string()),
                creator_id: "ctr".to_string(),
                preset_id: "e2e-converge".to_string(),
                status: SessionStatus::WaitingForInput,
                current_task_id: Some("task_7".to_string()),
            },
        ];
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            Some(&store),
            &sums,
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![
                ResumeDecision::SkippedHumanWait {
                    session_id: SessionId("v1:wait".to_string())
                },
                ResumeDecision::SkippedHumanWait {
                    session_id: SessionId("v1:wait_joins".to_string())
                },
            ],
            "a v1 human wait (token-bearing, even with old join keys) is never \
             stepped or approved at boot"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for either token-bearing wait"
        );
        // The A4 wait token is preserved in the durable state.
        let record = sqlite
            .load_run(&SessionId("v1:wait".to_string()))
            .await
            .expect("load_run")
            .expect("row exists");
        assert_eq!(
            record.state.and_then(|s| s.wait).map(|w| w.wait_id),
            Some("wait-tok-1".to_string()),
            "the durable wait token must survive boot untouched"
        );
    }

    #[tokio::test]
    async fn resume_a7_v1_corrupt_state_is_unreadable_non_replayable() {
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        {
            let pool = nexus_local_db::open_pool(&db_path)
                .await
                .expect("open pool");
            // v1 row with a corrupt state blob — non-replayable, never
            // silently reinterpreted (A7 rule 2).
            sqlx::query(
                "INSERT INTO orchestration_sessions
                    (session_id, creator_id, preset_id, preset_version, status,
                     current_task_id, context_json, created_at, updated_at,
                     execution_version, state_revision, run_state_json, run_descriptor_json)
                 VALUES ('v1:corrupt', 'ctr', 'e2e-converge', 3, 'running', 'task_1',
                         '{}', 1_756_990_000, 1_756_990_300, 1, 5, 'not-json', '{}')",
            )
            .execute(&pool)
            .await
            .expect("seed corrupt v1 row");
            pool.close().await;
        }
        let (storage, sqlite) = a7_storage(&db_path).await;
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let sum = SessionSummary {
            session_id: SessionId("v1:corrupt".to_string()),
            creator_id: "ctr".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("task_1".to_string()),
        };
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            Some(&store),
            &[sum],
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert!(
            matches!(&decisions[0], ResumeDecision::SkippedUnreadable { session_id, .. }
                if session_id.0 == "v1:corrupt"),
            "corrupt v1 metadata must be surfaced as unreadable/non-replayable: {decisions:?}"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for a corrupt v1 row"
        );
    }

    // ------------------------------------------------------------------
    // v1.186 P0 Task 2 fix round 2 — A7 immediate skip + legacy bypass
    // ------------------------------------------------------------------

    #[tokio::test]
    async fn resume_a7_stale_running_summary_terminal_load_skips_immediately() {
        // Review round-2 Critical 1: the daemon-visible summary can be stale
        // (listed before the durable status transitioned terminal). The
        // summary says running/join-resumable while `load_run` reports
        // terminal — the session must skip immediately with SkippedTerminal
        // and consume ZERO drive steps; old join keys must never reach
        // `drive_preset_run`.
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        {
            let pool = nexus_local_db::open_pool(&db_path)
                .await
                .expect("open pool");
            seed_v1_row(
                &pool,
                "v1:stale-terminal",
                "completed",
                None,
                false,
                false,
                true,
            )
            .await;
            pool.close().await;
        }
        let (storage, sqlite) = a7_storage(&db_path).await;
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        // Stale summary: says Running with live join keys; the durable row
        // has already transitioned to completed.
        let sum = SessionSummary {
            session_id: SessionId("v1:stale-terminal".to_string()),
            creator_id: "ctr".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("join".to_string()),
        };
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            Some(&store),
            &[sum],
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::SkippedTerminal {
                session_id: SessionId("v1:stale-terminal".to_string())
            }],
            "a durable terminal record wins over a stale running summary"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for a terminal session, stale summary or not"
        );
    }

    #[tokio::test]
    async fn resume_a7_stale_running_summary_unreadable_load_skips_immediately() {
        // Review round-2 Critical 1 (unreadable half): stale summary says
        // running/join-resumable while the durable metadata is corrupt. At
        // the daemon boundary `load_run` surfaces corrupt v1 run-state as an
        // error (strict deserialization) — SkippedUnreadable, zero drive
        // steps, never falling through to legacy join evidence. (The
        // `Unreadable` class immediate-skip in the A7 gate covers the same
        // rows whenever `classify_recovery` yields it.)
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        {
            let pool = nexus_local_db::open_pool(&db_path)
                .await
                .expect("open pool");
            // v1 row with structurally invalid run state (string where bool
            // required) → `load_run` errors (non-replayable). Context is
            // byte-valid with `data` so the failure is definitively in the
            // v1 run metadata, not the session context.
            sqlx::query(
                "INSERT INTO orchestration_sessions
                    (session_id, creator_id, preset_id, preset_version, status,
                     current_task_id, context_json, created_at, updated_at,
                     execution_version, state_revision, run_state_json, run_descriptor_json)
                 VALUES ('v1:stale-unreadable', 'ctr', 'e2e-converge', 3, 'running', 'join',
                         ?, 1_756_990_000, 1_756_990_300, 1, 5, ?, ?)",
            )
            .bind(
                br#"{"data": {"_converge_arrivals_j1": ["a"], "_join_wait_start_j1": 1},
                     "chat_history": {"messages": [], "max_messages": 1000}}"#
                    .as_slice(),
            )
            .bind(br#"{"cancel_requested":"false"}"#.as_slice())
            .bind(
                br#"{"creator_id":"ctr","work_id":null,"workspace_root":"/tmp/ws",
                     "preset_id":"e2e-converge","preset_version":3,
                     "source":{"Embedded":{"preset_id":"e2e-converge","content_hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},
                     "input":{},"agent_bindings":{},"parent_session_id":null,"graph_name":null}"#
                    .as_slice(),
            )
            .execute(&pool)
            .await
            .expect("seed structurally corrupt v1 row");
            pool.close().await;
        }
        let (storage, sqlite) = a7_storage(&db_path).await;
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        // Stale summary: says Running with live join keys (chain context
        // would be present in a real row); the durable metadata is corrupt.
        let sum = SessionSummary {
            session_id: SessionId("v1:stale-unreadable".to_string()),
            creator_id: "ctr".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("join".to_string()),
        };
        let decisions = resume_driven_sessions(
            &engine,
            &storage,
            Some(&store),
            &[sum],
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert!(
            matches!(&decisions[0], ResumeDecision::SkippedUnreadable { session_id, error }
                if session_id.0 == "v1:stale-unreadable"
                    && error.contains("run_state_json")),
            "corrupt v1 metadata must surface as unreadable from the v1 run-state \
             deserialization, not legacy chain evidence: {decisions:?}"
        );
        assert!(
            engine.script.lock().len() == 1,
            "no run_step may be consumed for an unreadable v1 row"
        );
    }

    #[tokio::test]
    async fn resume_a7_v1_converge_merges_despite_stale_typed_failure_keys() {
        // Review round-2 Important: a valid v1 ConvergeMerge class must not
        // fall through the v0 typed-failure guards. Stale `_run_status` /
        // `_run_error` context keys (leftover from an earlier v0-era
        // failure) cannot suppress the durable A7 converge/merge re-drive —
        // the legacy cascade runs only for v0 / no-store rows.
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        {
            let pool = nexus_local_db::open_pool(&db_path)
                .await
                .expect("open pool");
            // v1 running row at a live converge/merge chain, context carrying
            // STALE typed-failure keys (v0-era leftovers) alongside the live
            // join keys.
            let context = br#"{"data": {
                "_converge_arrivals_j1": ["a"], "_join_wait_start_j1": 1,
                "_gate_park_join": true,
                "_run_status": "failed", "_run_error": "converge_timeout: gate=converge"
            }, "chat_history": {"messages": [], "max_messages": 1000}}"#
                .to_vec();
            let run_state = br#"{
                "wait": null, "step_in_flight": null, "in_flight": null,
                "failure": null, "cancel_requested": false
            }"#
            .to_vec();
            sqlx::query(
                "INSERT INTO orchestration_sessions
                    (session_id, creator_id, preset_id, preset_version, status,
                     current_task_id, context_json, created_at, updated_at,
                     execution_version, state_revision, run_state_json, run_descriptor_json)
                 VALUES ('v1:stale-typed', 'ctr', 'e2e-converge', 3, 'running', 'join',
                         ?, 1_756_990_000, 1_756_990_300, 1, 5, ?, ?)",
            )
            .bind(context)
            .bind(run_state)
            .bind(
                br#"{"creator_id":"ctr","work_id":null,"workspace_root":"/tmp/ws",
                     "preset_id":"e2e-converge","preset_version":3,
                     "source":{"Embedded":{"preset_id":"e2e-converge","content_hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]}},
                     "input":{},"agent_bindings":{},"parent_session_id":null,"graph_name":null}"#
                    .as_slice(),
            )
            .execute(&pool)
            .await
            .expect("seed v1 stale-typed row");
            pool.close().await;
        }
        let (storage, sqlite) = a7_storage(&db_path).await;
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let engine =
            ScriptedEngine::with_script(vec![Ok(StepOutcome::Completed { response: None })]);
        let config = PresetRunConfig {
            resume_waiting: true,
            ..PresetRunConfig::default()
        };
        let sum = SessionSummary {
            session_id: SessionId("v1:stale-typed".to_string()),
            creator_id: "ctr".to_string(),
            preset_id: "e2e-converge".to_string(),
            status: SessionStatus::WaitingForInput,
            current_task_id: Some("join".to_string()),
        };
        let decisions =
            resume_driven_sessions(&engine, &storage, Some(&store), &[sum], &config, None).await;
        assert_eq!(
            decisions,
            vec![ResumeDecision::ReDriven {
                session_id: SessionId("v1:stale-typed".to_string()),
                outcome: PresetRunOutcome::Completed { steps: 1 },
            }],
            "a v1 ConvergeMerge row re-drives despite stale context failure keys"
        );
        assert!(
            engine.script.lock().is_empty(),
            "exactly one run_step may be consumed for the v1 converge re-drive"
        );
    }

    // ------------------------------------------------------------------
    // T2 fix round 1 — Finding 2: continue-v-cancel CAS race → exact 409
    // ------------------------------------------------------------------

    /// Delegating [`WorkflowStateStore`] that injects a competing transition
    /// on the first `commit_transition` call (the deterministic race gate):
    /// the injected write wins the revision CAS, so the delegated call loses
    /// with [`EngineError::RevisionMismatch`] — exactly the interleaving
    /// where a control signal loaded the row before a concurrent transition
    /// committed.
    struct RaceInjectingStore {
        inner: Arc<dyn WorkflowStateStore>,
        storage: Arc<dyn SessionStorage>,
        injected: AtomicBool,
    }

    impl RaceInjectingStore {
        fn new(inner: Arc<dyn WorkflowStateStore>, storage: Arc<dyn SessionStorage>) -> Self {
            Self {
                inner,
                storage,
                injected: AtomicBool::new(false),
            }
        }
    }

    #[async_trait]
    impl WorkflowStateStore for RaceInjectingStore {
        async fn load_run(
            &self,
            session_id: &SessionId,
        ) -> Result<Option<nexus_orchestration::run_state::RunRecord>, EngineError> {
            self.inner.load_run(session_id).await
        }

        async fn start_run(
            &self,
            session_id: &SessionId,
            descriptor: &nexus_orchestration::run_state::RunDescriptorV1,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            self.inner
                .start_run(session_id, descriptor, checkpoint, next_state)
                .await
        }

        async fn admit_schedule_run(
            &self,
            schedule_id: &str,
            session_id: &SessionId,
            descriptor: &nexus_orchestration::run_state::RunDescriptorV1,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
            core_context_version: u32,
            expected_core_context_version: u32,
            admission_gate: Option<&nexus_orchestration::run_state::ScheduleAdmissionGate>,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            self.inner
                .admit_schedule_run(
                    schedule_id,
                    session_id,
                    descriptor,
                    checkpoint,
                    next_state,
                    core_context_version,
                    expected_core_context_version,
                    admission_gate,
                )
                .await
        }

        async fn commit_transition(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_status: SessionStatus,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            if !self.injected.swap(true, Ordering::SeqCst) {
                // The competing transition wins the CAS first: a concurrent
                // cancel fence commits `cancel_requested` at the SAME
                // revision the delegated call is about to lose.
                let record = self
                    .inner
                    .load_run(session_id)
                    .await?
                    .expect("run exists at race gate");
                let root = self
                    .storage
                    .get(&session_id.0)
                    .await
                    .expect("root session at race gate")
                    .expect("root exists at race gate");
                let mut competing_state = record.state.unwrap_or_default();
                competing_state.cancel_requested = true;
                self.inner
                    .commit_transition(
                        session_id,
                        record.state_revision,
                        nexus_orchestration::run_state::RunCheckpoint {
                            root: &root,
                            children: &[],
                        },
                        record.status.clone(),
                        &competing_state,
                    )
                    .await
                    .expect("competing cancel fence wins the CAS");
            }
            self.inner
                .commit_transition(
                    session_id,
                    expected_revision,
                    checkpoint,
                    next_status,
                    next_state,
                )
                .await
        }

        async fn commit_transition_with_graph_fence(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_status: SessionStatus,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            let _ = expected_graph_version;
            self.commit_transition(
                session_id,
                expected_revision,
                checkpoint,
                next_status,
                next_state,
            )
            .await
        }

        async fn settle_cleanup(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
            terminal_status: SessionStatus,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            self.inner
                .settle_cleanup(
                    session_id,
                    expected_revision,
                    expected_graph_version,
                    checkpoint,
                    next_state,
                    terminal_status,
                )
                .await
        }

        async fn restore_pre_step(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            pre_step: &graph_flow::Session,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            self.inner
                .restore_pre_step(session_id, expected_revision, pre_step)
                .await
        }

        async fn mark_step_in_flight(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            step_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            self.inner
                .mark_step_in_flight(
                    session_id,
                    expected_revision,
                    expected_graph_version,
                    checkpoint,
                    step_state,
                )
                .await
        }

        async fn persist_prompt_attempt(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_step: Option<&str>,
            expected_attempt_id: Option<&str>,
            attempt: &nexus_orchestration::run_state::PromptAttempt,
        ) -> Result<(), EngineError> {
            self.inner
                .persist_prompt_attempt(
                    session_id,
                    expected_revision,
                    expected_step,
                    expected_attempt_id,
                    attempt,
                )
                .await
        }

        async fn clear_prompt_attempt(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_step: Option<&str>,
            attempt_id: &str,
        ) -> Result<(), EngineError> {
            self.inner
                .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
                .await
        }

        async fn load_children(
            &self,
            parent_session_id: &SessionId,
        ) -> Result<Vec<nexus_orchestration::run_state::RunRecord>, EngineError> {
            self.inner.load_children(parent_session_id).await
        }
    }

    /// Which REAL store boundary a fixture faults or races. Every variant
    /// fires exactly once, AT the delegated operation (never before it), so
    /// the observable contract is the one production sees.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum BoundaryFault {
        /// The coupled authoritative failure commit fails with a storage
        /// error at `commit_transition_with_graph_fence` (matched on the
        /// failure-carrying state, so the step transition is untouched).
        FailFailureCommit,
        /// The STEP transition (the post-run commit carrying no failure)
        /// fails with a storage error: the attempt must keep its marker
        /// witness so the durable failure record is still written.
        FailStepCommit,
        /// A graph-only writer wins between the pre-step root load and the
        /// STEP MARKER, so the stale pre-step checkpoint must lose the
        /// marker CAS instead of clobbering the winner's context.
        GraphOnlyWinnerBeatsStepMarker,
        /// A graph-only writer (same workflow revision, advanced graph
        /// clock) wins between the step marker and the failure commit.
        GraphOnlyWinnerBeatsFailureCommit,
        /// A winner commits immediately AFTER the failure commit and before
        /// the anchored cleanup runs.
        WinnerBetweenCommitAndCleanup,
        /// The anchored cleanup write (`settle_cleanup`) fails with a
        /// storage error.
        FailCleanupSettle,
        /// A competing owner wins `mark_step_in_flight` first (pre-marker
        /// winner).
        WinnerBeatsStepMarker,
        /// Another owner settles the run `Cancelled` right after the anchored
        /// phase-1 cancel-intent fence and before the final settlement.
        WinnerSettlesCancelledAfterPhaseOne,
        /// The v1 run-state blob is corrupted in the DB on the step's load.
        CorruptStateOnStepLoad,
        /// The step transition fails AND the deterministic pre-step restore
        /// then fails at its own storage boundary (a two-stage failure whose
        /// witness must survive to failure settlement).
        StepCommitAndRestoreFail,
    }

    /// Delegating [`WorkflowStateStore`] around the REAL SQLite store that
    /// injects one boundary fault (or one competing REAL write) at the
    /// operation named by [`BoundaryFault`]. Reads are never short-circuited
    /// into fake dispositions: the fixture either fails the actual write or
    /// lets a real winner write first.
    struct BoundaryFaultStore {
        inner: Arc<dyn WorkflowStateStore>,
        storage: Arc<dyn SessionStorage>,
        pool: Arc<sqlx::SqlitePool>,
        fault: BoundaryFault,
        fired: AtomicBool,
        /// Separate stage flag: `StepCommitAndRestoreFail` fires the step
        /// commit first and the restore second.
        restore_fired: AtomicBool,
    }

    impl BoundaryFaultStore {
        fn new(
            inner: Arc<dyn WorkflowStateStore>,
            storage: Arc<dyn SessionStorage>,
            pool: Arc<sqlx::SqlitePool>,
            fault: BoundaryFault,
        ) -> Self {
            Self {
                inner,
                storage,
                pool,
                fault,
                fired: AtomicBool::new(false),
                restore_fired: AtomicBool::new(false),
            }
        }

        fn file_once(&self) -> bool {
            !self.fired.swap(true, Ordering::SeqCst)
        }

        fn injected(message: &str) -> EngineError {
            EngineError::GraphFlow(graph_flow::GraphError::StorageError(message.to_string()))
        }

        async fn inject_graph_only_winner(&self, session_id: &SessionId, marker: &str) {
            let winner = self
                .storage
                .get(&session_id.0)
                .await
                .expect("winner root session")
                .expect("winner root exists");
            winner
                .context
                .set("winner.marker", marker)
                .expect("winner context write");
            self.storage
                .save(winner)
                .await
                .expect("graph-only winner save wins the OCC");
        }

        async fn winner_commit_cancelled_after_failure(
            &self,
            session_id: &SessionId,
            committed: &nexus_orchestration::run_state::RunRecord,
            marker: &str,
        ) {
            let winner_root = self
                .storage
                .get(&session_id.0)
                .await
                .expect("winner root session")
                .expect("winner root exists");
            winner_root
                .context
                .set("winner.marker", marker)
                .expect("winner context write");
            let mut winner_state = committed.state.clone().unwrap_or_default();
            winner_state.cancel_requested = true;
            self.inner
                .commit_transition(
                    session_id,
                    committed.state_revision,
                    nexus_orchestration::run_state::RunCheckpoint {
                        root: &winner_root,
                        children: &[],
                    },
                    SessionStatus::Cancelled,
                    &winner_state,
                )
                .await
                .expect("winner commits after the failure commit");
        }

        async fn winner_settle_cancelled_after_phase_one(
            &self,
            session_id: &SessionId,
            committed: &nexus_orchestration::run_state::RunRecord,
            marker: &str,
        ) {
            let winner_root = self
                .storage
                .get(&session_id.0)
                .await
                .expect("winner root session")
                .expect("winner root exists");
            winner_root
                .context
                .set("winner.marker", marker)
                .expect("winner context write");
            let mut winner_state = committed.state.clone().unwrap_or_default();
            winner_state.cancel_requested = true;
            self.inner
                .settle_cleanup(
                    session_id,
                    committed.state_revision,
                    None,
                    nexus_orchestration::run_state::RunCheckpoint {
                        root: &winner_root,
                        children: &[],
                    },
                    &winner_state,
                    SessionStatus::Cancelled,
                )
                .await
                .expect("concurrent owner settles Cancelled after phase 1");
        }
    }

    #[async_trait]
    impl WorkflowStateStore for BoundaryFaultStore {
        async fn load_run(
            &self,
            session_id: &SessionId,
        ) -> Result<Option<nexus_orchestration::run_state::RunRecord>, EngineError> {
            if self.fault == BoundaryFault::CorruptStateOnStepLoad && self.file_once() {
                // REAL external corruption of the authoritative state blob:
                // the row becomes non-replayable, exactly like an outside
                // writer or disk fault damaging it mid-run.
                sqlx::query(
                    "UPDATE orchestration_sessions SET run_state_json = 'not-json' \
                     WHERE session_id = ?",
                )
                .bind(&session_id.0)
                .execute(&*self.pool)
                .await
                .expect("inject corrupt v1 state blob");
            }
            self.inner.load_run(session_id).await
        }

        async fn start_run(
            &self,
            session_id: &SessionId,
            descriptor: &nexus_orchestration::run_state::RunDescriptorV1,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            self.inner
                .start_run(session_id, descriptor, checkpoint, next_state)
                .await
        }

        #[allow(clippy::too_many_arguments)]
        async fn admit_schedule_run(
            &self,
            schedule_id: &str,
            session_id: &SessionId,
            descriptor: &nexus_orchestration::run_state::RunDescriptorV1,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
            core_context_version: u32,
            expected_core_context_version: u32,
            admission_gate: Option<&nexus_orchestration::run_state::ScheduleAdmissionGate>,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            self.inner
                .admit_schedule_run(
                    schedule_id,
                    session_id,
                    descriptor,
                    checkpoint,
                    next_state,
                    core_context_version,
                    expected_core_context_version,
                    admission_gate,
                )
                .await
        }

        async fn commit_transition(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_status: SessionStatus,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            self.inner
                .commit_transition(
                    session_id,
                    expected_revision,
                    checkpoint,
                    next_status,
                    next_state,
                )
                .await
        }

        async fn commit_transition_with_graph_fence(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_status: SessionStatus,
            next_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            match self.fault {
                BoundaryFault::FailFailureCommit
                    if next_state.failure.is_some() && self.file_once() =>
                {
                    return Err(Self::injected(
                        "injected authoritative failure-commit storage fault",
                    ));
                }
                BoundaryFault::FailStepCommit | BoundaryFault::StepCommitAndRestoreFail
                    if next_state.failure.is_none() && self.file_once() =>
                {
                    return Err(Self::injected("injected step-transition storage fault"));
                }
                BoundaryFault::GraphOnlyWinnerBeatsFailureCommit if self.file_once() => {
                    self.inject_graph_only_winner(session_id, "graph-only")
                        .await;
                }
                BoundaryFault::WinnerBetweenCommitAndCleanup if self.file_once() => {
                    let committed = self
                        .inner
                        .commit_transition_with_graph_fence(
                            session_id,
                            expected_revision,
                            expected_graph_version,
                            checkpoint,
                            next_status,
                            next_state,
                        )
                        .await?;
                    self.winner_commit_cancelled_after_failure(
                        session_id,
                        &committed,
                        "post-commit",
                    )
                    .await;
                    return Ok(committed);
                }
                _ => {}
            }
            let committed = self
                .inner
                .commit_transition_with_graph_fence(
                    session_id,
                    expected_revision,
                    expected_graph_version,
                    checkpoint,
                    next_status,
                    next_state,
                )
                .await?;
            if self.fault == BoundaryFault::WinnerSettlesCancelledAfterPhaseOne
                && next_state.cancel_requested
                && next_state.failure.is_some()
                && self.file_once()
            {
                self.winner_settle_cancelled_after_phase_one(
                    session_id,
                    &committed,
                    "phase1-cancelled",
                )
                .await;
            }
            Ok(committed)
        }

        async fn settle_cleanup(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            next_state: &nexus_orchestration::run_state::RunStateV1,
            terminal_status: SessionStatus,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            if self.fault == BoundaryFault::FailCleanupSettle && self.file_once() {
                return Err(Self::injected("injected cleanup-settle storage fault"));
            }
            self.inner
                .settle_cleanup(
                    session_id,
                    expected_revision,
                    expected_graph_version,
                    checkpoint,
                    next_state,
                    terminal_status,
                )
                .await
        }

        async fn restore_pre_step(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            pre_step: &graph_flow::Session,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            if self.fault == BoundaryFault::StepCommitAndRestoreFail
                && !self.restore_fired.swap(true, Ordering::SeqCst)
            {
                return Err(Self::injected("injected pre-step restore storage fault"));
            }
            self.inner
                .restore_pre_step(session_id, expected_revision, pre_step)
                .await
        }

        async fn mark_step_in_flight(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_graph_version: Option<u64>,
            checkpoint: nexus_orchestration::run_state::RunCheckpoint<'_>,
            step_state: &nexus_orchestration::run_state::RunStateV1,
        ) -> Result<nexus_orchestration::run_state::RunRecord, EngineError> {
            let nexus_orchestration::run_state::RunCheckpoint { root, children } = checkpoint;
            if self.fault == BoundaryFault::GraphOnlyWinnerBeatsStepMarker && self.file_once() {
                self.inject_graph_only_winner(session_id, "graph-only-before-marker")
                    .await;
            }
            if self.fault == BoundaryFault::WinnerBeatsStepMarker && self.file_once() {
                // A competing owner wins the REAL marker CAS at the same
                // expected revision AND graph preimage; the delegated marker
                // then loses.
                self.inner
                    .mark_step_in_flight(
                        session_id,
                        expected_revision,
                        expected_graph_version,
                        nexus_orchestration::run_state::RunCheckpoint { root, children },
                        step_state,
                    )
                    .await
                    .expect("competing owner wins the step marker");
            }
            self.inner
                .mark_step_in_flight(
                    session_id,
                    expected_revision,
                    expected_graph_version,
                    nexus_orchestration::run_state::RunCheckpoint { root, children },
                    step_state,
                )
                .await
        }

        async fn persist_prompt_attempt(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_step: Option<&str>,
            expected_attempt_id: Option<&str>,
            attempt: &nexus_orchestration::run_state::PromptAttempt,
        ) -> Result<(), EngineError> {
            self.inner
                .persist_prompt_attempt(
                    session_id,
                    expected_revision,
                    expected_step,
                    expected_attempt_id,
                    attempt,
                )
                .await
        }

        async fn clear_prompt_attempt(
            &self,
            session_id: &SessionId,
            expected_revision: u64,
            expected_step: Option<&str>,
            attempt_id: &str,
        ) -> Result<(), EngineError> {
            self.inner
                .clear_prompt_attempt(session_id, expected_revision, expected_step, attempt_id)
                .await
        }

        async fn load_children(
            &self,
            parent_session_id: &SessionId,
        ) -> Result<Vec<nexus_orchestration::run_state::RunRecord>, EngineError> {
            self.inner.load_children(parent_session_id).await
        }
    }

    async fn assert_authoritative_commit_fault_fence(
        coordinator: &WorkflowRunCoordinator,
        real_store: &Arc<dyn WorkflowStateStore>,
        storage: &Arc<dyn SessionStorage>,
        session_id: &SessionId,
    ) {
        assert_eq!(
            coordinator
                .ensure_driving(session_id)
                .await
                .expect("ensure_driving"),
            DriveDisposition::NotDriving,
            "fenced owner must refuse re-entry"
        );

        let record = real_store
            .load_run(session_id)
            .await
            .expect("load")
            .expect("row");
        assert_ne!(
            record.status,
            SessionStatus::Failed,
            "must not claim a durable Failed transition when the coupled write faulted"
        );
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "authoritative RunFailure must be absent when the commit faulted"
        );
        assert_eq!(
            record.state_revision, 2,
            "only the step marker may be durable: the faulted failure commit wrote nothing"
        );
        assert!(
            record
                .state
                .as_ref()
                .is_some_and(|s| s.step_in_flight.is_some()),
            "the failed stage must have reached the in-flight marker (not an early bail)"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert!(
            root.context.get::<String>("_run_status").is_none(),
            "context failure keys must not be written when the coupled write faulted"
        );
    }

    /// Wait (bounded) until a fixture's durable/observable condition holds.
    async fn wait_until<F, Fut>(what: &str, mut cond: F)
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = bool>,
    {
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                if cond().await {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
    }

    /// Deterministic continue-v-cancel race (Finding 2): the cancel fence
    /// wins the root revision CAS while the continue holds a stale load. The
    /// losing continue must surface the exact 409 `workflow_state_conflict`
    /// (never a 500), the durable winner is the cancel intent, and the run
    /// stays actionable for a real cancel.
    #[tokio::test]
    #[allow(clippy::too_many_lines)] // deterministic race scenario needs full setup + assertions in one flow
    async fn signal_continue_loses_cas_to_cancel_returns_state_conflict() {
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let race_store: Arc<dyn WorkflowStateStore> =
            Arc::new(RaceInjectingStore::new(real_store.clone(), storage.clone()));

        // Real engine over the race-injecting store + real coordinator.
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                race_store.clone(),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );

        // Start a v1 run and park it at a durable human wait.
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("fence-graph")
                .add_task(Arc::new(nexus_orchestration::tasks::ManualWaitTask))
                .build()
                .expect("test graph build"),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");
        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get root")
            .expect("root exists");
        let waiting_state = nexus_orchestration::run_state::RunStateV1 {
            wait: Some(nexus_orchestration::run_state::WaitRecord {
                wait_id: "wait-tok-1".to_string(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_orchestration::run_state::WaitKind::Manual,
            }),
            ..nexus_orchestration::run_state::RunStateV1::default()
        };
        real_store
            .commit_transition(
                &session_id,
                record.state_revision,
                nexus_orchestration::run_state::RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::WaitingForInput,
                &waiting_state,
            )
            .await
            .expect("park at human wait");

        // Continue with the exact token: the race gate injects the cancel
        // fence between the continue's load and its commit, so the continue
        // loses the CAS. The coordinator must map the loss to the exact 409
        // state conflict — never a 500.
        let err = coordinator
            .signal_run(
                &session_id,
                RunSignal::Continue {
                    wait_id: "wait-tok-1".to_string(),
                },
            )
            .await
            .expect_err("the losing continue must surface a conflict");
        match &err {
            RunControlError::StateConflict(sid, msg) => {
                assert_eq!(sid, &session_id.0);
                assert!(
                    msg.contains("revision moved"),
                    "state conflict must name the revision loss, got {msg}"
                );
            }
            other => panic!("expected StateConflict, got {other:?}"),
        }

        // Exactly one durable winner: the injected cancel intent. The run is
        // still waiting with cancel_requested persisted.
        let after = real_store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(after.status, SessionStatus::WaitingForInput);
        assert!(
            after.state.as_ref().is_some_and(|s| s.cancel_requested),
            "the cancel fence must be the durable winner"
        );
        assert_eq!(
            after
                .state
                .as_ref()
                .and_then(|s| s.wait.as_ref())
                .map(|w| w.wait_id.as_str()),
            Some("wait-tok-1"),
            "the wait is still present (the continue did not consume it)"
        );

        // The run stays actionable: a real cancel now re-fences at the new
        // revision and settles cancelled.
        let result = coordinator
            .signal_run(&session_id, RunSignal::Cancel)
            .await
            .expect("cancel after the race must succeed");
        assert_eq!(result.status, "cancelled");
        let final_record = real_store
            .load_run(&session_id)
            .await
            .expect("load run")
            .expect("run exists");
        assert_eq!(final_record.status, SessionStatus::Cancelled);
    }

    /// Coordinator boundary (Critical 1): a losing drive that surfaces
    /// `SessionConflict` must not run `persist_drive_failure` or mutate the
    /// concurrent winner's durable row.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_conflict_loser_leaves_winner_untouched() {
        use graph_flow::{NextAction, Task, TaskResult};

        struct CompetingWinnerTask {
            store: Arc<SqliteSessionStorage>,
        }

        #[async_trait]
        impl Task for CompetingWinnerTask {
            fn id(&self) -> &'static str {
                "competing_winner_task"
            }

            async fn run(
                &self,
                context: graph_flow::Context,
            ) -> Result<TaskResult, graph_flow::GraphError> {
                let session_id: String = context
                    .get("_session_id")
                    .expect("engine seeds _session_id");
                let winner = graph_flow::Session::new_from_task(session_id.clone(), "winner-task");
                winner
                    .context
                    .set("_session_id", session_id.clone())
                    .expect("session id");
                winner
                    .context
                    .set("winner.marker", "committed")
                    .expect("marker");
                self.store
                    .commit_transition(
                        &SessionId(session_id),
                        2,
                        nexus_orchestration::run_state::RunCheckpoint {
                            root: &winner,
                            children: &[],
                        },
                        SessionStatus::Running,
                        &nexus_orchestration::run_state::RunStateV1::default(),
                    )
                    .await
                    .map_err(|e| {
                        graph_flow::GraphError::StorageError(format!(
                            "fixture winner commit failed: {e}"
                        ))
                    })?;
                Ok(TaskResult::new(
                    Some("done".to_string()),
                    NextAction::Continue,
                ))
            }
        }

        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );

        let race_graph = Arc::new(
            graph_flow::GraphBuilder::new("race-graph")
                .add_task(Arc::new(CompetingWinnerTask {
                    store: sqlite.clone(),
                }))
                .build()
                .expect("race graph"),
        );
        let session_id = engine
            .start_session("novel-writing", race_graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start drive");

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let record = store
                    .load_run(&session_id)
                    .await
                    .expect("load")
                    .expect("row");
                if record.state_revision >= 3 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        })
        .await
        .expect("drive finished");

        assert!(
            !coordinator.is_fenced(&session_id).await,
            "conflict loser must not fence the owner"
        );

        let after = store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        let after_root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(after.status, SessionStatus::Running);
        assert_eq!(
            after.state_revision, 3,
            "winner revision retained (1 start + 1 mark + 1 win)"
        );
        assert_eq!(after_root.current_task_id, "winner-task");
        assert_eq!(
            after_root.context.get::<String>("winner.marker").as_deref(),
            Some("committed")
        );
        assert!(after_root.context.get::<String>("_run_status").is_none());
        assert!(after_root.context.get::<String>("_run_error").is_none());
        assert!(
            after
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none()
        );
    }

    /// Critical 1 regression: ordinary task error AFTER a concurrent winner
    /// commits must not rebase failure onto the winner's revision.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_ordinary_failure_loser_leaves_winner_untouched() {
        use graph_flow::{Task, TaskResult};

        struct CompetingWinnerThenFailTask {
            store: Arc<SqliteSessionStorage>,
        }

        #[async_trait]
        impl Task for CompetingWinnerThenFailTask {
            fn id(&self) -> &'static str {
                "competing_winner_then_fail_task"
            }

            async fn run(
                &self,
                context: graph_flow::Context,
            ) -> Result<TaskResult, graph_flow::GraphError> {
                let session_id: String = context
                    .get("_session_id")
                    .expect("engine seeds _session_id");
                let winner = graph_flow::Session::new_from_task(session_id.clone(), "winner-task");
                winner
                    .context
                    .set("_session_id", session_id.clone())
                    .expect("session id");
                winner
                    .context
                    .set("winner.marker", "ordinary-failure-race")
                    .expect("marker");
                self.store
                    .commit_transition(
                        &SessionId(session_id),
                        2,
                        nexus_orchestration::run_state::RunCheckpoint {
                            root: &winner,
                            children: &[],
                        },
                        SessionStatus::Running,
                        &nexus_orchestration::run_state::RunStateV1::default(),
                    )
                    .await
                    .map_err(|e| {
                        graph_flow::GraphError::StorageError(format!(
                            "fixture winner commit failed: {e}"
                        ))
                    })?;
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "ordinary failure after winner commit".to_string(),
                ))
            }
        }

        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );

        let race_graph = Arc::new(
            graph_flow::GraphBuilder::new("ordinary-failure-race-graph")
                .add_task(Arc::new(CompetingWinnerThenFailTask {
                    store: sqlite.clone(),
                }))
                .build()
                .expect("race graph"),
        );
        let session_id = engine
            .start_session("novel-writing", race_graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start drive");

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let record = store
                    .load_run(&session_id)
                    .await
                    .expect("load")
                    .expect("row");
                if record.state_revision >= 3 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        })
        .await
        .expect("drive finished");

        assert!(
            !coordinator.is_fenced(&session_id).await,
            "ownership-lost ordinary failure must not fence the winner owner"
        );

        let after = store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        let after_root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(after.status, SessionStatus::Running);
        assert_eq!(after.state_revision, 3);
        assert_eq!(after_root.current_task_id, "winner-task");
        assert_eq!(
            after_root.context.get::<String>("winner.marker").as_deref(),
            Some("ordinary-failure-race")
        );
        assert!(after_root.context.get::<String>("_run_status").is_none());
        assert!(
            after
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none()
        );
    }

    /// Recovery path: same ordinary-failure ownership loss as `ensure_driving`.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn recovery_ordinary_failure_loser_leaves_winner_untouched() {
        use graph_flow::{Task, TaskResult};

        struct CompetingWinnerThenFailTask {
            store: Arc<SqliteSessionStorage>,
        }

        #[async_trait]
        impl Task for CompetingWinnerThenFailTask {
            fn id(&self) -> &'static str {
                "competing_winner_then_fail_task"
            }

            async fn run(
                &self,
                context: graph_flow::Context,
            ) -> Result<TaskResult, graph_flow::GraphError> {
                let session_id: String = context
                    .get("_session_id")
                    .expect("engine seeds _session_id");
                let winner = graph_flow::Session::new_from_task(session_id.clone(), "winner-task");
                winner
                    .context
                    .set("_session_id", session_id.clone())
                    .expect("session id");
                winner
                    .context
                    .set("winner.marker", "recovery-ordinary-race")
                    .expect("marker");
                self.store
                    .commit_transition(
                        &SessionId(session_id),
                        2,
                        nexus_orchestration::run_state::RunCheckpoint {
                            root: &winner,
                            children: &[],
                        },
                        SessionStatus::Running,
                        &nexus_orchestration::run_state::RunStateV1::default(),
                    )
                    .await
                    .map_err(|e| {
                        graph_flow::GraphError::StorageError(format!(
                            "fixture winner commit failed: {e}"
                        ))
                    })?;
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "recovery ordinary failure after winner".to_string(),
                ))
            }
        }

        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );

        let race_graph = Arc::new(
            graph_flow::GraphBuilder::new("recovery-ordinary-failure-race-graph")
                .add_task(Arc::new(CompetingWinnerThenFailTask {
                    store: sqlite.clone(),
                }))
                .build()
                .expect("race graph"),
        );
        let session_id = engine
            .start_session("novel-writing", race_graph)
            .await
            .expect("start session");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        let summary = SessionSummary {
            session_id: session_id.clone(),
            creator_id: "test-creator".to_string(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some(root.current_task_id.clone()),
        };

        coordinator
            .recover_driving(Some(&store), &[summary], None)
            .await;

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let record = store
                    .load_run(&session_id)
                    .await
                    .expect("load")
                    .expect("row");
                if record.state_revision >= 3 {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        })
        .await
        .expect("recovery drive finished");

        let after = store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        let after_root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(after.status, SessionStatus::Running);
        assert_eq!(after.state_revision, 3);
        assert_eq!(
            after_root.context.get::<String>("winner.marker").as_deref(),
            Some("recovery-ordinary-race")
        );
        assert!(
            after
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none()
        );
    }

    /// Coordinator path (assignment proof): an ordinary driver failure through
    /// `ensure_driving` must populate authoritative `RunStateV1.failure` on
    /// real SQLite — not only the direct `drive_preset_run` harness.
    #[tokio::test]
    async fn coordinator_ensure_driving_failure_persists_authoritative_run_failure() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "coordinator-path deterministic failure".to_string(),
                ))
            }
        }
        let (_tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-fail-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            loop {
                let finished = coordinator
                    .drives
                    .lock()
                    .await
                    .get(&session_id.0)
                    .is_none_or(|owner| owner.join.is_finished());
                if finished {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
        })
        .await
        .expect("coordinator drive finished");

        assert!(
            !coordinator.is_fenced(&session_id).await,
            "successful durable failure path must not fence the owner"
        );

        let record = store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.status,
            SessionStatus::Failed,
            "coordinator failure must remain a failed prerequisite"
        );
        let failure = record
            .state
            .as_ref()
            .and_then(|s| s.failure.as_ref())
            .expect("coordinator failure path must populate authoritative RunFailure");
        assert_eq!(failure.code, "driver_failed");
        assert!(
            failure
                .message
                .contains("coordinator-path deterministic failure"),
            "{failure:?}"
        );
    }

    /// Coordinator fail-closed (assignment proof): when the authoritative
    /// failure write is faulted, neither authoritative `RunFailure` nor
    /// Coordinator fail-closed at the REAL boundary: the coupled
    /// authoritative failure commit itself fails with a storage error. No
    /// `RunFailure`, no context failure keys, no `Failed` claim; the owner
    /// is fenced and refuses re-entry.
    #[tokio::test]
    async fn coordinator_authoritative_commit_fault_fences_without_durable_failed_claim() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "authoritative commit fault failure".to_string(),
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-commit-fault-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                real_store.clone(),
                caps,
            ),
        );
        let fault_store: Arc<dyn WorkflowStateStore> = Arc::new(BoundaryFaultStore::new(
            real_store.clone(),
            storage.clone(),
            pool.clone(),
            BoundaryFault::FailFailureCommit,
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        )
        .with_workflow_store(fault_store);
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        wait_until("failed owner fence", || coordinator.is_fenced(&session_id)).await;

        assert_authoritative_commit_fault_fence(&coordinator, &real_store, &storage, &session_id)
            .await;
    }

    /// Coordinator fail-closed at the REAL cleanup boundary: the
    /// authoritative failure commit lands, but the anchored cleanup write
    /// fails at its own settlement boundary. The durable `RunFailure` stays
    /// honest, no terminal success is claimed, and the owner is fenced.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_cleanup_settle_fault_fences_with_authoritative_present() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "cleanup settle fault failure".to_string(),
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-cleanup-fault-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                Arc::new(BoundaryFaultStore::new(
                    real_store.clone(),
                    storage.clone(),
                    pool.clone(),
                    BoundaryFault::FailCleanupSettle,
                )),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        wait_until("failed owner fence", || coordinator.is_fenced(&session_id)).await;

        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_some_and(|f| f.code == "driver_failed"),
            "the authoritative RunFailure must be durable before the cleanup fault"
        );
        assert!(
            record.state.as_ref().is_some_and(|s| s.cancel_requested),
            "phase-1 cancel intent is durable even though the cleanup settlement faulted"
        );
        assert_eq!(
            record.status,
            SessionStatus::Running,
            "a faulted cleanup settlement must not claim terminal success"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(
            root.context.get::<String>("_run_status").as_deref(),
            Some("failed"),
            "coupled context keys land with the authoritative failure commit"
        );
        assert_eq!(
            coordinator
                .ensure_driving(&session_id)
                .await
                .expect("ensure_driving"),
            DriveDisposition::NotDriving,
            "a faulted cleanup must fence re-entry"
        );
    }

    /// `get_status` failure before any step witness must not fabricate
    /// authority from a durable-row reload (fail-closed): no `RunFailure`,
    /// no context failure keys, and no cleanup signal against the row.
    #[tokio::test]
    async fn get_status_error_before_step_witness_fails_closed_without_mutation() {
        struct NoopTask;
        #[async_trait]
        impl graph_flow::Task for NoopTask {
            fn id(&self) -> &'static str {
                "noop"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Ok(graph_flow::TaskResult::new(
                    Some("done".to_string()),
                    graph_flow::NextAction::Continue,
                ))
            }
        }

        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let inner: Arc<dyn OrchestrationEngine> = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            ),
        );
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("get-status-fail-graph")
                .add_task(Arc::new(NoopTask))
                .build()
                .expect("graph"),
        );
        let session_id = inner
            .start_session_with_graph("novel-writing", graph)
            .await
            .expect("start session");
        let engine = FailGetStatusOnce::new(inner, "injected get_status failure");

        let outcome = drive_preset_run(
            &engine,
            Some(&storage),
            Some(&store),
            &session_id,
            &PresetRunConfig::default(),
            None,
        )
        .await;
        let PresetRunOutcome::Failed {
            settlement, error, ..
        } = outcome
        else {
            panic!("expected Failed, got {outcome:?}");
        };
        assert!(
            error.contains("get_status failed"),
            "error must surface get_status failure: {error}"
        );
        assert_eq!(
            settlement.authoritative,
            FailurePersistenceDisposition::NoAuthority
        );
        assert_eq!(
            settlement.context,
            FailurePersistenceDisposition::NoAuthority
        );
        assert_eq!(
            settlement.cleanup,
            FailurePersistenceDisposition::NoAuthority
        );

        let record = store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "must not fabricate RunFailure without a step witness"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert!(
            root.context.get::<String>("_run_status").is_none(),
            "context failure keys must not be written without step witness"
        );
    }

    /// `get_status` failure without a durable witness must fail closed without
    /// fabricating authoritative `RunFailure`.
    #[tokio::test]
    async fn get_status_error_without_durable_witness_fails_closed() {
        struct NoopTask;
        #[async_trait]
        impl graph_flow::Task for NoopTask {
            fn id(&self) -> &'static str {
                "noop"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Ok(graph_flow::TaskResult::new(
                    Some("done".to_string()),
                    graph_flow::NextAction::Continue,
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let inner: Arc<dyn OrchestrationEngine> = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            ),
        );
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("get-status-no-witness-graph")
                .add_task(Arc::new(NoopTask))
                .build()
                .expect("graph"),
        );
        let session_id = inner
            .start_session_with_graph("novel-writing", graph)
            .await
            .expect("start session");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        store
            .commit_transition(
                &session_id,
                1,
                nexus_orchestration::run_state::RunCheckpoint {
                    root: &root,
                    children: &[],
                },
                SessionStatus::Cancelled,
                &nexus_orchestration::run_state::RunStateV1::default(),
            )
            .await
            .expect("terminalize row");

        let engine = FailGetStatusOnce::new(inner, "injected get_status failure without witness");
        let outcome = drive_preset_run(
            &engine,
            Some(&storage),
            Some(&store),
            &session_id,
            &PresetRunConfig::default(),
            None,
        )
        .await;
        let PresetRunOutcome::Failed { settlement, .. } = outcome else {
            panic!("expected Failed, got {outcome:?}");
        };
        assert_eq!(
            settlement.authoritative,
            FailurePersistenceDisposition::NoAuthority
        );
        assert_eq!(
            settlement.context,
            FailurePersistenceDisposition::NoAuthority
        );

        let record = store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(record.status, SessionStatus::Cancelled);
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "must not fabricate authoritative failure without witness"
        );
    }

    /// A graph-only winner (same workflow revision, advanced graph clock)
    /// between the step marker and the failure commit defeats the failure
    /// write: the winner's context/clock survive and no failure is recorded.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_graph_only_winner_is_never_overwritten_by_failure_commit() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "graph-only winner failure".to_string(),
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-graph-only-winner-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                real_store.clone(),
                caps,
            ),
        );
        let fault_store: Arc<dyn WorkflowStateStore> = Arc::new(BoundaryFaultStore::new(
            real_store.clone(),
            storage.clone(),
            pool.clone(),
            BoundaryFault::GraphOnlyWinnerBeatsFailureCommit,
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        )
        .with_workflow_store(fault_store);
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        wait_until("graph-only winner write", || async {
            let root = storage
                .get(&session_id.0)
                .await
                .expect("get")
                .expect("root");
            root.context.get::<String>("winner.marker").as_deref() == Some("graph-only")
        })
        .await;
        coordinator.abort_all_drives().await;

        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "the failure commit must lose the graph fence, never overwrite the winner"
        );
        assert_eq!(record.status, SessionStatus::Running);
        assert_eq!(
            record.state_revision, 2,
            "a graph-only winner keeps the workflow revision (marker only)"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(
            root.context.get::<String>("winner.marker").as_deref(),
            Some("graph-only"),
            "the winner's context survives verbatim"
        );
        assert!(root.context.get::<String>("_run_status").is_none());
        assert!(
            !coordinator.is_fenced(&session_id).await,
            "an ownership loss to a live winner must not fence re-entry"
        );
    }

    /// Recovery owner path, same REAL boundary as `ensure_driving`: the
    /// coupled authoritative failure commit faults, so the recovered drive
    /// must fence without a durable `Failed` claim.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn recovery_authoritative_commit_fault_fences_without_durable_claim() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "recovery commit fault failure".to_string(),
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("recovery-commit-fault-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                real_store.clone(),
                caps,
            ),
        );
        let fault_store: Arc<dyn WorkflowStateStore> = Arc::new(BoundaryFaultStore::new(
            real_store.clone(),
            storage.clone(),
            pool.clone(),
            BoundaryFault::FailFailureCommit,
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        )
        .with_workflow_store(fault_store.clone());
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        let summary = SessionSummary {
            session_id: session_id.clone(),
            creator_id: "test-creator".to_string(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some(root.current_task_id.clone()),
        };

        coordinator
            .recover_driving(Some(&fault_store), &[summary], None)
            .await;

        wait_until("recovered failed owner fence", || {
            coordinator.is_fenced(&session_id)
        })
        .await;

        assert_eq!(
            coordinator
                .ensure_driving(&session_id)
                .await
                .expect("ensure_driving"),
            DriveDisposition::NotDriving
        );
        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_ne!(
            record.status,
            SessionStatus::Failed,
            "recovery must not claim a durable Failed transition when the commit faulted"
        );
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "no RunFailure may be fabricated when the commit faulted"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert!(root.context.get::<String>("_run_status").is_none());
    }

    /// Pre-marker winner (REAL boundary): a competing owner wins
    /// `mark_step_in_flight` first. The losing drive has no step witness and
    /// must not fabricate authority: nothing is written against the winner,
    /// and the owner is fenced against re-entry.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_pre_marker_winner_is_refused_without_mutation() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "pre-marker winner failure".to_string(),
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-pre-marker-winner-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                Arc::new(BoundaryFaultStore::new(
                    real_store.clone(),
                    storage.clone(),
                    pool.clone(),
                    BoundaryFault::WinnerBeatsStepMarker,
                )),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        wait_until("failed owner fence", || coordinator.is_fenced(&session_id)).await;

        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.state_revision, 2,
            "the competing owner's marker is the only durable write"
        );
        assert!(
            record
                .state
                .as_ref()
                .is_some_and(|s| s.step_in_flight.is_some()),
            "the winner's in-flight marker stands untouched"
        );
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "a pre-marker loser has no authority and must not write a failure"
        );
        assert_eq!(record.status, SessionStatus::Running);
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert!(root.context.get::<String>("_run_status").is_none());
        assert_eq!(
            coordinator
                .ensure_driving(&session_id)
                .await
                .expect("ensure_driving"),
            DriveDisposition::NotDriving,
            "a no-authority loser must be fenced, never silently re-driven"
        );
    }

    /// Post-persistence / pre-cleanup winner (REAL boundary): a winner takes
    /// the row right after the authoritative failure commit. The anchored
    /// cleanup CASes the commit's OWN clocks and must lose — the winner's
    /// terminal outcome is never re-cancelled or rebased.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_winner_between_failure_commit_and_cleanup_is_untouched() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "post-commit winner failure".to_string(),
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-post-commit-winner-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                real_store.clone(),
                caps,
            ),
        );
        let fault_store: Arc<dyn WorkflowStateStore> = Arc::new(BoundaryFaultStore::new(
            real_store.clone(),
            storage.clone(),
            pool.clone(),
            BoundaryFault::WinnerBetweenCommitAndCleanup,
        ));
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        )
        .with_workflow_store(fault_store);
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        wait_until("post-commit winner terminal row", || async {
            match real_store.load_run(&session_id).await {
                Ok(Some(record)) => {
                    record.status == SessionStatus::Cancelled
                        && record.state.as_ref().is_some_and(|s| s.cancel_requested)
                }
                _ => false,
            }
        })
        .await;
        coordinator.abort_all_drives().await;

        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.status,
            SessionStatus::Cancelled,
            "the winner's terminal outcome stands"
        );
        assert_eq!(
            record.state_revision, 4,
            "marker (2) + failure commit (3) + winner (4): the anchored cleanup added no write"
        );
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_some_and(|f| f.code == "driver_failed"),
            "the winner carried the durable failure record"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(
            root.context.get::<String>("winner.marker").as_deref(),
            Some("post-commit"),
            "the winner's context survives the losing cleanup"
        );
        assert!(
            !coordinator.is_fenced(&session_id).await,
            "a cleanup ownership loss must not fence the winner"
        );
    }

    /// Corrupt v1 state (REAL boundary): the state blob is damaged on the
    /// step's load. The drive must fail closed with no fabricated authority,
    /// leave the corrupt row byte-identical (never sanitized), and fence the
    /// owner against re-entry.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_corrupt_state_fault_fences_without_mutation() {
        struct NoopTask;
        #[async_trait]
        impl graph_flow::Task for NoopTask {
            fn id(&self) -> &'static str {
                "noop"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Ok(graph_flow::TaskResult::new(
                    Some("done".to_string()),
                    graph_flow::NextAction::Continue,
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-corrupt-state-graph")
                .add_task(Arc::new(NoopTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                Arc::new(BoundaryFaultStore::new(
                    real_store.clone(),
                    storage.clone(),
                    pool.clone(),
                    BoundaryFault::CorruptStateOnStepLoad,
                )),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        wait_until("failed owner fence", || coordinator.is_fenced(&session_id)).await;

        let (revision, run_state_json, status): (i64, Option<String>, String) = sqlx::query_as(
            "SELECT state_revision, run_state_json, status FROM orchestration_sessions \
             WHERE session_id = ?",
        )
        .bind(&session_id.0)
        .fetch_one(&*pool)
        .await
        .expect("read corrupt row");
        assert_eq!(
            run_state_json.as_deref(),
            Some("not-json"),
            "a corrupt v1 state blob is never sanitized into a new failure state"
        );
        assert_eq!(revision, 1, "the corrupt row is not mutated");
        assert_eq!(status, "running");
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert!(root.context.get::<String>("_run_status").is_none());
        assert_eq!(
            coordinator
                .ensure_driving(&session_id)
                .await
                .expect("ensure_driving"),
            DriveDisposition::NotDriving,
            "a corrupt-state failure must fence the owner against re-entry"
        );
    }

    /// C4 / Important 3 (REAL engine + SQLite): a persisted v0 row driven
    /// WITH a workflow store is never stepped by v1 authority. The drive
    /// fails before any dispatch, the row stays byte-identical v0
    /// (`status`/`revision`/`execution_version`), the context-only failure record
    /// persists, the owner is fenced, and A7 recovery then skips the row.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn v0_drive_with_store_handle_is_context_only_and_recovery_skips() {
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        // A genuine v0 row: `SessionStorage::save` inserts with the legacy
        // `execution_version = 0` marker and no `_run_status`/`_run_error`.
        let session = graph_flow::Session::new_from_task("v0:recovery".to_string(), "task_1");
        session
            .context
            .set("_join_wait_start_task_1", 1_756_990_000_i64)
            .expect("seed v0 context");
        storage.save(session).await.expect("seed v0 row");

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let session_id = SessionId("v0:recovery".to_string());

        // v1 authority must refuse the legacy row at the STEP boundary too
        // (not only via the drive's status probe): a direct step returns the
        // typed non-replayable error and dispatches nothing.
        let step_err = engine
            .run_step(&session_id)
            .await
            .expect_err("a v0 row must not be steppable under v1 authority");
        let step_err = step_err.to_string();
        assert!(
            step_err.contains("not steppable under v1 authority"),
            "the v0 refusal must be the typed non-replayable error: {step_err}"
        );

        // Drive the REAL engine with a store: the drive fails without any
        // marker, dispatch, or failure-record fabrication.
        let outcome = drive_preset_run(
            &*engine,
            Some(&storage),
            Some(&store),
            &session_id,
            &PresetRunConfig::default(),
            None,
        )
        .await;
        let PresetRunOutcome::Failed { settlement, .. } = outcome else {
            panic!("expected Failed, got {outcome:?}");
        };
        assert_eq!(
            settlement.authoritative,
            FailurePersistenceDisposition::NoAuthority
        );
        assert_eq!(
            settlement.context,
            FailurePersistenceDisposition::Committed,
            "the legacy context-only record is the failure evidence"
        );
        assert_eq!(
            settlement.cleanup,
            FailurePersistenceDisposition::LegacyContextOnly,
            "v0 cleanup is explicitly context-only: no v1 cancel is attempted"
        );

        // Durable v0 shape: status/revision/format version untouched.
        let (execution_version, revision, status): (i64, i64, String) = sqlx::query_as(
            "SELECT execution_version, state_revision, status FROM orchestration_sessions \
             WHERE session_id = ?",
        )
        .bind("v0:recovery")
        .fetch_one(&*pool)
        .await
        .expect("read v0 row");
        assert_eq!(execution_version, 0, "the row stays legacy v0");
        assert_eq!(revision, 0, "no v1 transition touched the v0 row");
        assert_eq!(status, "running", "the v0 status is not repurposed");
        let record = store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "no v1 RunFailure is fabricated for a v0 row"
        );

        // Context-only evidence is durable, so the owner must be fenced.
        let root = storage
            .get("v0:recovery")
            .await
            .expect("get")
            .expect("root");
        assert_eq!(
            root.context.get::<String>("_run_status").as_deref(),
            Some("failed"),
            "the v0 legacy failure record must be durable"
        );
        assert!(
            root.context
                .get::<String>("_run_error")
                .as_deref()
                .is_some_and(|e| !e.is_empty()),
            "the v0 legacy error text must be durable: {:?}",
            root.context.get::<String>("_run_error")
        );

        // A7 recovery must now SKIP the row (typed-failure keys present) and
        // must not dispatch anything.
        let summary = SessionSummary {
            session_id: session_id.clone(),
            creator_id: "test-creator".to_string(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("task_1".to_string()),
        };
        let decisions = coordinator
            .recover_driving(Some(&store), &[summary], None)
            .await;
        assert!(
            matches!(&decisions[0], ResumeDecision::SkippedTypedFailed { session_id }
                if session_id.0 == "v0:recovery"),
            "recovery must skip the v0 row: {decisions:?}"
        );
    }

    /// C1 (real boundary): the root session row is gone before the marker.
    /// The engine must refuse BEFORE any dispatch, fabricate no marker and no
    /// failure record, and the coordinator owner must be fenced.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn missing_root_fails_stop_without_dispatch_or_write() {
        use std::sync::atomic::AtomicUsize;
        struct CountingTask {
            dispatched: Arc<AtomicUsize>,
        }
        #[async_trait]
        impl graph_flow::Task for CountingTask {
            fn id(&self) -> &'static str {
                "count"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                self.dispatched.fetch_add(1, Ordering::SeqCst);
                Ok(graph_flow::TaskResult::new(
                    Some("done".to_string()),
                    graph_flow::NextAction::Continue,
                ))
            }
        }

        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let real_storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let dispatched = Arc::new(AtomicUsize::new(0));
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("missing-root-graph")
                .add_task(Arc::new(CountingTask {
                    dispatched: dispatched.clone(),
                }))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        // The engine sees the REAL v1 row through its store, but its session
        // storage reports the root as absent — the seam under test.
        let seam = Arc::new(MissingRootStorage::new(real_storage.clone()));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                seam.clone(),
                store.clone(),
                caps,
            ),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        // The row is durable; from here on its root reads report absence.
        seam.hide(&session_id.0);

        let outcome = drive_preset_run(
            &*engine,
            Some(&real_storage),
            Some(&store),
            &session_id,
            &PresetRunConfig::default(),
            None,
        )
        .await;
        let PresetRunOutcome::Failed {
            settlement, error, ..
        } = outcome
        else {
            panic!("expected Failed, got {outcome:?}");
        };
        assert!(
            error.contains("no pre-step root session"),
            "the fail-stop must be the typed rootless refusal: {error}"
        );
        assert_eq!(
            dispatched.load(Ordering::SeqCst),
            0,
            "a missing pre-step root must fail closed before any task dispatch"
        );
        assert_eq!(
            settlement.authoritative,
            FailurePersistenceDisposition::NoAuthority,
            "no authority may be fabricated for a rootless step"
        );
        assert_eq!(
            settlement.context,
            FailurePersistenceDisposition::NoAuthority
        );
        assert_eq!(
            settlement.cleanup,
            FailurePersistenceDisposition::NoAuthority
        );

        let record = store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.state_revision, 1,
            "no marker was written for the rootless step"
        );
        assert!(
            record
                .state
                .as_ref()
                .is_some_and(|s| s.step_in_flight.is_none()),
            "no in-flight marker may be fabricated"
        );
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "no failure record may be fabricated without authority"
        );
        let root = real_storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert!(
            root.context.get::<String>("_run_status").is_none(),
            "no context failure keys may be written without authority"
        );

        // Mechanical fence proof: the no-authority settlement must fence the
        // coordinator owner (no terminal claim).
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            real_storage.clone(),
            pool.clone(),
            Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        );
        assert!(
            !coordinator
                .persist_drive_failure(&session_id, &settlement)
                .await,
            "a no-authority rootless step must fence re-entry"
        );
    }

    /// C2 (real boundary): the step transition AND the deterministic pre-step
    /// restore both fail. The failed-step witness must survive to failure
    /// settlement so the coupled authoritative failure record still lands.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_step_commit_and_restore_fault_still_settles_failure() {
        struct PausingTask;
        #[async_trait]
        impl graph_flow::Task for PausingTask {
            fn id(&self) -> &'static str {
                "pause"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Ok(graph_flow::TaskResult::new(
                    Some("paused".to_string()),
                    graph_flow::NextAction::Continue,
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("step-and-restore-fault-graph")
                .add_task(Arc::new(PausingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                Arc::new(BoundaryFaultStore::new(
                    real_store.clone(),
                    storage.clone(),
                    pool.clone(),
                    BoundaryFault::StepCommitAndRestoreFail,
                )),
                caps,
            ),
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        let outcome = drive_preset_run(
            &*engine,
            Some(&storage),
            Some(&real_store),
            &session_id,
            &PresetRunConfig::default(),
            None,
        )
        .await;
        let PresetRunOutcome::SessionConflict { error, .. } = outcome else {
            panic!("expected the fenced conflict outcome, got {outcome:?}");
        };
        assert!(
            error.contains("pre-step restore also failed"),
            "the chained commit+restore failure must surface: {error}"
        );

        // The witness REACHED settlement: without it the dispositions would be
        // `NoAuthority`. With it, the commit CAS is attempted against the
        // step's owned clocks — which the runner's own position save has
        // since advanced past — so the honest outcome is an ownership loss
        // (fence), never a fabricated failure/cleanup claim.
        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.state_revision, 2,
            "only the step marker is durable (the faulted commit and restore wrote nothing)"
        );
        assert!(
            record
                .state
                .as_ref()
                .is_some_and(|s| s.step_in_flight.is_some()),
            "the in-flight marker remains for A7 recovery (non-replayable)"
        );
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "no RunFailure may be fabricated when the owned clocks were lost"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert!(
            root.context.get::<String>("_run_status").is_none(),
            "no context failure keys may be written on an ownership loss"
        );

        // The SessionConflict outcome IS the mechanical witness proof:
        // `failed_outcome` maps to it only when the settlement carries an
        // `OwnershipLost` disposition, which is computed from the step's
        // witness — a dropped witness would have produced `NoAuthority` and
        // the `Failed` outcome instead.
    }

    /// C1 (real boundary): a graph-only writer wins between the pre-step root
    /// load and the STEP MARKER. The marker's graph predicate must reject the
    /// stale checkpoint — the winner's context/clock survive, no step is
    /// dispatched, and the losing owner is fenced.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_graph_only_winner_before_step_marker_is_not_clobbered() {
        use std::sync::atomic::AtomicUsize;
        struct CountingTask {
            dispatched: Arc<AtomicUsize>,
        }
        #[async_trait]
        impl graph_flow::Task for CountingTask {
            fn id(&self) -> &'static str {
                "count"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                self.dispatched.fetch_add(1, Ordering::SeqCst);
                Ok(graph_flow::TaskResult::new(
                    Some("done".to_string()),
                    graph_flow::NextAction::Continue,
                ))
            }
        }

        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let dispatched = Arc::new(AtomicUsize::new(0));
        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-graph-only-marker-graph")
                .add_task(Arc::new(CountingTask {
                    dispatched: dispatched.clone(),
                }))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                Arc::new(BoundaryFaultStore::new(
                    real_store.clone(),
                    storage.clone(),
                    pool.clone(),
                    BoundaryFault::GraphOnlyWinnerBeatsStepMarker,
                )),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        wait_until("failed owner fence", || coordinator.is_fenced(&session_id)).await;

        assert_eq!(
            dispatched.load(Ordering::SeqCst),
            0,
            "no external step may be dispatched when the marker lost its graph fence"
        );
        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.state_revision, 1,
            "the rejected marker wrote nothing (the winner kept the revision)"
        );
        assert!(
            record
                .state
                .as_ref()
                .is_some_and(|s| s.step_in_flight.is_none()),
            "no in-flight marker may be persisted over the winner"
        );
        assert!(
            record
                .state
                .as_ref()
                .and_then(|s| s.failure.as_ref())
                .is_none(),
            "a graph-only marker loss has no authority to write a failure"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(
            root.context.get::<String>("winner.marker").as_deref(),
            Some("graph-only-before-marker"),
            "the winner's context survives verbatim"
        );
        assert!(root.context.get::<String>("_run_status").is_none());
        assert_eq!(
            coordinator
                .ensure_driving(&session_id)
                .await
                .expect("ensure_driving"),
            DriveDisposition::NotDriving,
            "a no-authority marker loser must be fenced, never re-driven"
        );
    }

    /// C2 (real boundary): the STEP transition fails after the marker. The
    /// attempt must retain its marker witness, restore the pre-step boundary,
    /// and still write the coupled authoritative failure + context keys.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_step_commit_fault_keeps_marker_witness_and_settles() {
        struct PausingTask;
        #[async_trait]
        impl graph_flow::Task for PausingTask {
            fn id(&self) -> &'static str {
                "pause"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Ok(graph_flow::TaskResult::new(
                    Some("paused".to_string()),
                    graph_flow::NextAction::Continue,
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-step-commit-fault-graph")
                .add_task(Arc::new(PausingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                Arc::new(BoundaryFaultStore::new(
                    real_store.clone(),
                    storage.clone(),
                    pool.clone(),
                    BoundaryFault::FailStepCommit,
                )),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        // The durable failure record and the anchored terminal settlement are
        // separate writes: wait for BOTH before asserting the settled shape.
        wait_until("anchored settlement", || async {
            match real_store.load_run(&session_id).await {
                Ok(Some(record)) => {
                    record.status == SessionStatus::Failed
                        && record
                            .state
                            .as_ref()
                            .and_then(|s| s.failure.as_ref())
                            .is_some_and(|f| f.code == "driver_failed")
                }
                _ => false,
            }
        })
        .await;

        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.status,
            SessionStatus::Failed,
            "the failed drive preserves its cause through anchored cleanup"
        );
        assert!(
            record
                .state
                .as_ref()
                .is_some_and(|s| s.step_in_flight.is_none()),
            "the restored pre-step boundary clears the in-flight marker"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(
            root.context.get::<String>("_run_status").as_deref(),
            Some("failed"),
            "the coupled context keys land with the authoritative failure commit"
        );
        assert!(
            !coordinator.is_fenced(&session_id).await,
            "a settled failure is not fenced"
        );
    }

    /// C3 (real boundary): another owner settles the run `Cancelled` right
    /// after the anchored phase-1 fence. The failed drive's final settlement
    /// must treat the moved clocks as ownership loss — never adopt the newer
    /// Cancelled row as its own cleanup success.
    #[tokio::test]
    #[allow(clippy::too_many_lines)]
    async fn coordinator_newer_cancelled_after_phase_one_is_not_adopted_as_cleanup() {
        struct FailingTask;
        #[async_trait]
        impl graph_flow::Task for FailingTask {
            fn id(&self) -> &'static str {
                "fail"
            }
            async fn run(
                &self,
                _context: graph_flow::Context,
            ) -> Result<graph_flow::TaskResult, graph_flow::GraphError> {
                Err(graph_flow::GraphError::TaskExecutionFailed(
                    "phase-1 winner failure".to_string(),
                ))
            }
        }
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let real_store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        let graph = Arc::new(
            graph_flow::GraphBuilder::new("coord-phase1-winner-graph")
                .add_task(Arc::new(FailingTask))
                .build()
                .expect("graph"),
        );
        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        // The phase-1 cancel-intent fence is committed by the ENGINE's store
        // (the coordinator's store handles the drive/failure commit), so the
        // boundary fixture must wrap the engine's store.
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                Arc::new(BoundaryFaultStore::new(
                    real_store.clone(),
                    storage.clone(),
                    pool.clone(),
                    BoundaryFault::WinnerSettlesCancelledAfterPhaseOne,
                )),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let session_id = engine
            .start_session("novel-writing", graph)
            .await
            .expect("start session");

        coordinator
            .ensure_driving(&session_id)
            .await
            .expect("start coordinator drive");

        wait_until("phase-1 winner settlement", || async {
            let root = storage
                .get(&session_id.0)
                .await
                .expect("get")
                .expect("root");
            root.context.get::<String>("winner.marker").as_deref() == Some("phase1-cancelled")
        })
        .await;
        coordinator.abort_all_drives().await;

        let record = real_store
            .load_run(&session_id)
            .await
            .expect("load")
            .expect("row");
        assert_eq!(
            record.status,
            SessionStatus::Cancelled,
            "the newer owner's terminal outcome stands"
        );
        assert_eq!(
            record.state_revision, 5,
            "start (1) + marker (2) + failure commit (3) + phase-1 fence (4) + winner settle (5); \
             the failed drive's final settlement must add no write of its own"
        );
        let root = storage
            .get(&session_id.0)
            .await
            .expect("get")
            .expect("root");
        assert_eq!(
            root.context.get::<String>("winner.marker").as_deref(),
            Some("phase1-cancelled"),
            "the newer owner's context survives the losing cleanup"
        );
        assert!(
            !coordinator.is_fenced(&session_id).await,
            "an ownership loss to a live winner must not fence"
        );
    }

    /// v0 recovery skip (REAL SQLite): a legacy `execution_version = 0` row
    /// carrying the v0 typed-failure context keys is never re-driven and is
    /// left byte-identical.
    #[tokio::test]
    async fn recovery_skips_v0_typed_failed_row_without_mutation() {
        let (tmp, _nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let _ = &tmp;
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        let pool = Arc::new(pool);
        let sqlite = Arc::new(SqliteSessionStorage::new(pool.clone()));
        let storage: Arc<dyn SessionStorage> = sqlite.clone();
        let store: Arc<dyn WorkflowStateStore> = sqlite.clone();

        // v0 row: the graph-flow `save` insert leaves `execution_version = 0`
        // (the legacy evidence marker) with the context-only failure record.
        let session = graph_flow::Session::new_from_task("v0:typed-failed".to_string(), "task_1");
        session
            .context
            .set("_run_status", "failed")
            .expect("v0 status key");
        session
            .context
            .set("_run_error", "legacy drive failed")
            .expect("v0 error key");
        storage.save(session).await.expect("seed v0 row");

        let caps = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
            nexus_orchestration::CapabilityRegistry::with_builtins(),
        ));
        let engine = Arc::new(
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store(
                storage.clone(),
                store.clone(),
                caps,
            ),
        );
        let session_cancels: std::sync::Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()));
        let coordinator = WorkflowRunCoordinator::new(
            engine.clone(),
            storage.clone(),
            pool.clone(),
            session_cancels,
        );
        let summary = SessionSummary {
            session_id: SessionId("v0:typed-failed".to_string()),
            creator_id: "test-creator".to_string(),
            preset_id: "novel-writing".to_string(),
            status: SessionStatus::Running,
            current_task_id: Some("task_1".to_string()),
        };

        let decisions = coordinator
            .recover_driving(Some(&store), &[summary], None)
            .await;

        assert!(
            matches!(&decisions[0], ResumeDecision::SkippedTypedFailed { session_id }
                if session_id.0 == "v0:typed-failed"),
            "a v0 typed-failed row must skip recovery: {decisions:?}"
        );
        let (execution_version, revision, status): (i64, i64, String) = sqlx::query_as(
            "SELECT execution_version, state_revision, status FROM orchestration_sessions \
             WHERE session_id = ?",
        )
        .bind("v0:typed-failed")
        .fetch_one(&*pool)
        .await
        .expect("read v0 row");
        assert_eq!(execution_version, 0, "v0 row stays legacy (never promoted)");
        assert_eq!(revision, 0, "v0 row is untouched by recovery");
        assert_eq!(status, "running", "v0 status column untouched");
        let root = storage
            .get("v0:typed-failed")
            .await
            .expect("get")
            .expect("root");
        assert_eq!(
            root.context.get::<String>("_run_status").as_deref(),
            Some("failed"),
            "the v0 context-only failure record survives verbatim"
        );
        assert!(
            !coordinator
                .is_fenced(&SessionId("v0:typed-failed".to_string()))
                .await,
            "a v0 skip must not fence the session"
        );
    }
}
