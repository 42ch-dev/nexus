//! Shared durable execution projection (A2/A7) for operator-facing inspection.
//!
//! One classification + legal-action projection built from the durable v1
//! record (or the schedule policy when no run exists), consumed by schedule
//! inspect/list, session get, and (for parity) `ops inspect`. Prose is never
//! parsed by callers: `recovery_class` and `allowed_actions` are the contract.

use nexus_contracts::local::orchestration::http::{ExecutionProjection, ExecutionWait};
use nexus_orchestration::engine::SessionStatus;
use nexus_orchestration::resume_rules::{classify_recovery, RecoveryClass};
use nexus_orchestration::run_state::RunRecord;

/// Projection for a run whose durable record could not be read: the row is
/// non-replayable and only cancel/new-run are legal (A7 rule 2).
#[must_use]
pub fn unreadable_projection(reason: &str) -> ExecutionProjection {
    ExecutionProjection {
        execution_version: None,
        state_revision: None,
        recovery_class: "unreadable".to_string(),
        wait: None,
        reason_code: Some(reason.to_string()),
        allowed_actions: vec!["cancel".to_string(), "new_run".to_string()],
    }
}

/// Projection for a schedule with no owned run (class from the policy, A2).
#[must_use]
pub fn project_no_run(execution_policy: &str) -> ExecutionProjection {
    let (recovery_class, allowed_actions) = match execution_policy {
        "system_inert" => ("system_inert", Vec::new()),
        "legacy_inert" => ("legacy_inert", vec!["start".to_string()]),
        // A driven_v1 row without a run is a durable v1 initial state:
        // A7 rule 6 reconstructs and drives it.
        "driven_v1" => ("safe_boundary", vec!["start".to_string()]),
        // Unknown policy is corrupt metadata: never a fabricated class.
        _ => (
            "unreadable",
            vec!["cancel".to_string(), "new_run".to_string()],
        ),
    };
    ExecutionProjection {
        execution_version: None,
        state_revision: None,
        recovery_class: recovery_class.to_string(),
        wait: None,
        reason_code: (recovery_class == "unreadable")
            .then(|| "unknown_execution_policy".to_string()),
        allowed_actions,
    }
}

/// Read-only check that the frozen source identity still resolves and
/// matches (A7): no runner is attached, no state mutated.
fn frozen_source_reconstructable(
    descriptor: &nexus_orchestration::run_state::RunDescriptorV1,
    caps: &nexus_orchestration::capability::CapabilityRegistry,
) -> bool {
    use nexus_orchestration::preset::{load_embedded_preset, load_preset};
    use nexus_orchestration::run_state::PresetSourceIdentity;
    let loaded = match &descriptor.source {
        PresetSourceIdentity::Embedded { preset_id, .. } => load_embedded_preset(preset_id, caps),
        PresetSourceIdentity::Directory { root, .. } => load_preset(root, caps),
    };
    match loaded {
        Ok(loaded) => {
            loaded.source_identity.as_ref() == Some(&descriptor.source)
                && loaded.version == descriptor.preset_version
        }
        Err(_) => false,
    }
}

/// Build the projection for an owned session id (schedule or session surface).
///
/// A load error is projected as `unreadable` (non-replayable), never silently
/// downgraded to the no-run policy projection.
pub async fn project_for_session(
    pool: &sqlx::SqlitePool,
    session_id: Option<&str>,
    execution_policy: &str,
    caps: Option<&std::sync::Arc<nexus_orchestration::capability::CapabilityRegistry>>,
) -> ExecutionProjection {
    use graph_flow::SessionStorage as _;
    use nexus_orchestration::engine::SessionId;
    use nexus_orchestration::run_state::WorkflowStateStore;
    use nexus_orchestration::storage::sqlite::SqliteSessionStorage;

    let Some(session_id) = session_id else {
        return project_no_run(execution_policy);
    };
    let store = SqliteSessionStorage::new(std::sync::Arc::new(pool.clone()));
    let record = match store.load_run(&SessionId(session_id.to_string())).await {
        Ok(Some(record)) => record,
        // A schedule claiming an owned run whose row is absent is corrupt.
        Ok(None) => return unreadable_projection("owned run row missing"),
        Err(e) => return unreadable_projection(&e.to_string()),
    };
    // A7 rule 5: the converge/merge park is identified by the exact
    // `_gate_park_<current_task>` marker on the durable context. A context
    // that cannot be read/serialized is non-replayable — never projected as
    // a fabricated safe_boundary.
    let gate_park_live = match store.get(session_id).await {
        Ok(Some(session)) => {
            let context = match serde_json::to_value(&session.context) {
                Ok(context) => context,
                Err(e) => return unreadable_projection(&format!("context unreadable: {e}")),
            };
            nexus_orchestration::resume_rules::context_data(&context).is_some_and(|data| {
                nexus_orchestration::resume_rules::gate_park_live(data, &session.current_task_id)
            })
        }
        Ok(None) => return unreadable_projection("owned session row missing"),
        Err(e) => return unreadable_projection(&e.to_string()),
    };
    project_execution(&record, gate_park_live, caps)
}

///
/// `gate_park_live` is the exact A7 rule 5 marker for the run's current task
/// (computed by the caller from the session context); without it a parked
/// converge/merge run would misproject as `safe_boundary`.
#[must_use]
pub fn project_execution(
    record: &RunRecord,
    gate_park_live: bool,
    caps: Option<&std::sync::Arc<nexus_orchestration::capability::CapabilityRegistry>>,
) -> ExecutionProjection {
    // v0 rows are explicitly legacy/unverified — never reinterpreted through
    // the v1 classifier (A2/A7 rule 7).
    if record.execution_version < 1 {
        return ExecutionProjection {
            execution_version: Some(record.execution_version),
            state_revision: Some(record.state_revision),
            recovery_class: "legacy_unverified".to_string(),
            wait: None,
            reason_code: None,
            allowed_actions: vec!["cancel".to_string(), "new_run".to_string()],
        };
    }

    let class = classify_recovery(&record.status, record.state.as_ref(), gate_park_live);
    let recovery_class = match class {
        RecoveryClass::Terminal => "terminal",
        RecoveryClass::HumanWait => "human_wait",
        RecoveryClass::SafeBoundary => "safe_boundary",
        RecoveryClass::ConvergeMerge => "converge_merge",
        RecoveryClass::Interrupted => "interrupted",
        RecoveryClass::LegacyUnverified => "legacy_unverified",
        RecoveryClass::Unreadable => "unreadable",
    };
    let allowed_actions: Vec<String> = match class {
        RecoveryClass::Terminal => vec!["new_run".to_string()],
        RecoveryClass::HumanWait => vec!["continue".to_string(), "cancel".to_string()],
        // An unconfirmed stop is never retried (A5).
        RecoveryClass::Interrupted => vec!["cancel".to_string(), "new_run".to_string()],
        RecoveryClass::SafeBoundary | RecoveryClass::ConvergeMerge => {
            vec!["cancel".to_string()]
        }
        RecoveryClass::LegacyUnverified | RecoveryClass::Unreadable => {
            vec!["cancel".to_string(), "new_run".to_string()]
        }
    };
    // A human wait whose frozen source can no longer be reconstructed must
    // not advertise `continue` (tri-QC P1-C): legal actions degrade to
    // cancel/new-run and the reason names the reconstruction failure.
    let (allowed_actions, source_unavailable) = if class == RecoveryClass::HumanWait
        && !record.descriptor.as_ref().is_some_and(|descriptor| {
            let builtin = nexus_orchestration::capability::CapabilityRegistry::with_builtins();
            let registry = caps.map_or(&builtin, |caps| &**caps);
            frozen_source_reconstructable(descriptor, registry)
        }) {
        (vec!["cancel".to_string(), "new_run".to_string()], true)
    } else {
        (allowed_actions, false)
    };
    // The wait token is only operator-actionable for a human wait; exposing a
    // stale token for another class would invite an invalid continue.
    let wait = (class == RecoveryClass::HumanWait)
        .then(|| {
            record.state.as_ref().and_then(|state| {
                state.wait.as_ref().map(|wait| ExecutionWait {
                    wait_id: wait.wait_id.clone(),
                    task_id: wait.task_id.clone(),
                    child_session_id: wait.child_session_id.clone(),
                    child_task_id: wait.child_task_id.clone(),
                    kind: format!("{:?}", wait.kind).to_lowercase(),
                })
            })
        })
        .flatten();
    let reason_code = if source_unavailable {
        Some("reconstruction_unavailable".to_string())
    } else {
        match (&record.status, record.state.as_ref()) {
            (SessionStatus::Interrupted, _) => Some("interrupted".to_string()),
            (_, Some(state)) => state.failure.as_ref().map(|failure| failure.code.clone()),
            _ => None,
        }
    };

    ExecutionProjection {
        execution_version: Some(record.execution_version),
        state_revision: Some(record.state_revision),
        recovery_class: recovery_class.to_string(),
        wait,
        reason_code,
        allowed_actions,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_orchestration::run_state::{RunRecord, RunStateV1, WaitKind, WaitRecord};

    fn record(
        status: SessionStatus,
        execution_version: u32,
        state: Option<RunStateV1>,
    ) -> RunRecord {
        RunRecord {
            session_id: nexus_orchestration::engine::SessionId("s1".to_string()),
            status,
            state_revision: 7,
            execution_version,
            descriptor: None,
            state,
            graph_version: 0,
        }
    }

    #[test]
    fn v0_records_are_legacy_unverified_never_classified() {
        let projection = project_execution(
            &record(SessionStatus::Running, 0, Some(RunStateV1::default())),
            false,
            None,
        );
        assert_eq!(projection.recovery_class, "legacy_unverified");
        assert_eq!(projection.allowed_actions, vec!["cancel", "new_run"]);
    }

    #[test]
    fn unreadable_projection_never_fabricates_a_class() {
        let projection = unreadable_projection("db gone");
        assert_eq!(projection.recovery_class, "unreadable");
        assert_eq!(projection.reason_code.as_deref(), Some("db gone"));
        assert_eq!(projection.allowed_actions, vec!["cancel", "new_run"]);
    }

    #[test]
    fn converge_merge_park_requires_the_gate_marker() {
        let parked = record(SessionStatus::Paused, 1, Some(RunStateV1::default()));
        let with_marker = project_execution(&parked, true, None);
        assert_eq!(with_marker.recovery_class, "converge_merge");
        assert_eq!(with_marker.allowed_actions, vec!["cancel"]);
        let without_marker = project_execution(&parked, false, None);
        assert_eq!(without_marker.recovery_class, "safe_boundary");
    }

    #[test]
    fn human_wait_projects_the_exact_token_and_actions() {
        let state = RunStateV1 {
            wait: Some(WaitRecord {
                wait_id: "wait-1".to_string(),
                task_id: "task_7".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: WaitKind::Manual,
            }),
            ..RunStateV1::default()
        };
        // A reconstructable frozen source keeps `continue` legal.
        let mut with_source = record(SessionStatus::WaitingForInput, 1, Some(state.clone()));
        with_source.descriptor = Some(reconstructable_descriptor());
        let projection = project_execution(&with_source, false, None);
        assert_eq!(projection.recovery_class, "human_wait");
        assert_eq!(
            projection.wait.as_ref().map(|w| w.wait_id.as_str()),
            Some("wait-1")
        );
        assert_eq!(projection.allowed_actions, vec!["continue", "cancel"]);

        // Without a verifiable frozen source the wait is preserved but
        // `continue` is not offered (tri-QC P1-C).
        let degraded = project_execution(
            &record(SessionStatus::WaitingForInput, 1, Some(state)),
            false,
            None,
        );
        assert_eq!(degraded.recovery_class, "human_wait");
        assert_eq!(degraded.allowed_actions, vec!["cancel", "new_run"]);
        assert_eq!(
            degraded.reason_code.as_deref(),
            Some("reconstruction_unavailable")
        );
    }

    /// A descriptor whose embedded source resolves and matches.
    fn reconstructable_descriptor() -> nexus_orchestration::run_state::RunDescriptorV1 {
        let caps = nexus_orchestration::capability::CapabilityRegistry::with_builtins();
        let loaded = nexus_orchestration::preset::load_embedded_preset("memory-augmented", &caps)
            .expect("embedded preset");
        nexus_orchestration::run_state::RunDescriptorV1 {
            creator_id: "c".to_string(),
            work_id: None,
            workspace_root: std::path::PathBuf::from("/tmp"),
            preset_id: "memory-augmented".to_string(),
            preset_version: loaded.version,
            source: loaded.source_identity.expect("source identity"),
            input: serde_json::Map::new(),
            agent_bindings: std::collections::HashMap::new(),
            parent_session_id: None,
            graph_name: None,
        }
    }

    #[test]
    fn unknown_policy_projects_unreadable_not_start() {
        let projection = project_no_run("bogus_policy");
        assert_eq!(projection.recovery_class, "unreadable");
        assert_eq!(projection.allowed_actions, vec!["cancel", "new_run"]);
        assert_eq!(
            projection.reason_code.as_deref(),
            Some("unknown_execution_policy")
        );
    }

    #[test]
    fn interrupted_never_offers_continue_or_retry() {
        let state = RunStateV1 {
            cancel_requested: true,
            ..RunStateV1::default()
        };
        let projection = project_execution(
            &record(SessionStatus::Interrupted, 1, Some(state)),
            false,
            None,
        );
        assert_eq!(projection.recovery_class, "interrupted");
        assert_eq!(projection.allowed_actions, vec!["cancel", "new_run"]);
        assert!(!projection
            .allowed_actions
            .iter()
            .any(|a| a == "continue" || a == "resume"));
    }

    #[test]
    fn stale_wait_is_not_exposed_for_non_wait_classes() {
        let state = RunStateV1 {
            wait: Some(WaitRecord {
                wait_id: "stale".to_string(),
                task_id: "t".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: WaitKind::Manual,
            }),
            cancel_requested: true,
            ..RunStateV1::default()
        };
        let projection = project_execution(
            &record(SessionStatus::Interrupted, 1, Some(state)),
            false,
            None,
        );
        assert_eq!(projection.recovery_class, "interrupted");
        assert!(
            projection.wait.is_none(),
            "a stale wait token must not be offered for a non-wait class"
        );
    }
}
