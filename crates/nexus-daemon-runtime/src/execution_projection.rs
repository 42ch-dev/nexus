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

/// Build the shared projection from a durable record (if any) and the
/// schedule's execution policy.
#[must_use]
pub fn project_execution(
    record: Option<&RunRecord>,
    execution_policy: &str,
) -> ExecutionProjection {
    let Some(record) = record else {
        // No owned run: the class comes from the schedule policy (A2).
        let (recovery_class, allowed_actions) = match execution_policy {
            "system_inert" => ("system_inert", Vec::new()),
            "legacy_inert" => ("legacy_inert", vec!["start".to_string()]),
            // A driven_v1 row without a run is a durable v1 initial state:
            // A7 rule 6 reconstructs and drives it.
            _ => ("safe_boundary", vec!["start".to_string()]),
        };
        return ExecutionProjection {
            execution_version: None,
            state_revision: None,
            recovery_class: recovery_class.to_string(),
            wait: None,
            reason_code: None,
            allowed_actions,
        };
    };

    let class = classify_recovery(&record.status, record.state.as_ref(), false);
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
    let wait = record.state.as_ref().and_then(|state| {
        state.wait.as_ref().map(|wait| ExecutionWait {
            wait_id: wait.wait_id.clone(),
            task_id: wait.task_id.clone(),
            child_session_id: wait.child_session_id.clone(),
            child_task_id: wait.child_task_id.clone(),
            kind: format!("{:?}", wait.kind).to_lowercase(),
        })
    });
    let reason_code = match (&record.status, record.state.as_ref()) {
        (SessionStatus::Interrupted, _) => Some("interrupted".to_string()),
        (_, Some(state)) => state.failure.as_ref().map(|failure| failure.code.clone()),
        _ => None,
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
