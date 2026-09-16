//! Shared Work lifecycle validation; no scheduler or engine dependency.

use nexus_contracts::local::orchestration::{stage_index, FL_E_STAGES};

/// Error returned when a stage advance fails validation.
///
/// Carries a machine-readable `code` (stable error code for CLI automation)
/// and a human-readable `message`.
#[derive(Debug, Clone)]
pub struct StageGateError {
    /// Machine-readable error code (e.g. `UNKNOWN_STAGE`, `ACTIVE_SCHEDULE`).
    pub code: String,
    /// Human-readable error message.
    pub message: String,
}

impl std::fmt::Display for StageGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for StageGateError {}

/// Input state for gate validation.
#[derive(Debug, Clone)]
pub struct WorkStageState {
    /// Current stage name (e.g. "intake").
    pub current_stage: String,
    /// Current `stage_status` (e.g. "complete").
    pub stage_status: String,
    /// V1.33 `intake_status` (e.g. "complete").
    pub intake_status: String,
}

/// Validate a stage advance request (spec §3.3 gates).
///
/// # Errors
///
/// Returns `StageGateError` with a descriptive message if any gate fails.
pub fn check_stage_advance(
    work: &WorkStageState,
    target_stage: &str,
    force: bool,
) -> Result<(), StageGateError> {
    // Gate 0: target must be a known stage
    let target_idx = stage_index(target_stage).ok_or_else(|| StageGateError {
        code: "FL_E_UNKNOWN_STAGE".to_string(),
        message: format!(
            "Unknown stage '{target_stage}'. Valid stages: {}",
            FL_E_STAGES.join(", ")
        ),
    })?;

    let current_idx = stage_index(&work.current_stage).unwrap_or(0);

    if !force {
        // Gate 1: cannot advance to same stage
        if target_stage == work.current_stage {
            return Err(StageGateError {
                code: "FL_E_SAME_STAGE".to_string(),
                message: format!(
                    "Work is already at stage '{}' ({}). Use a different target stage.",
                    work.current_stage, work.stage_status
                ),
            });
        }

        // Gate 2: cannot advance backwards
        if target_idx <= current_idx {
            return Err(StageGateError {
                code: "FL_E_BACKWARDS_ADVANCE".to_string(),
                message: format!(
                    "Cannot advance backwards from '{}' to '{}'. Stage order: {}",
                    work.current_stage,
                    target_stage,
                    FL_E_STAGES.join(" → ")
                ),
            });
        }

        // Gate 3: strict linear advance (+1 only)
        if target_idx != current_idx + 1 {
            let next_stage = FL_E_STAGES.get(current_idx + 1).unwrap_or(&"(unknown)");
            return Err(StageGateError {
                code: "FL_E_STAGE_SKIP".to_string(),
                message: format!(
                    "Cannot skip from '{}' to '{}'; expected next stage is '{}'. \
                     Use --force to skip stages.",
                    work.current_stage, target_stage, next_stage
                ),
            });
        }

        // Gate 4: at most one active FL-E stage schedule per Work (spec §2 #4)
        // Checked before completion gate so active-stage errors are more specific.
        if work.stage_status == "active" {
            return Err(StageGateError {
                code: "FL_E_ACTIVE_SCHEDULE".to_string(),
                message: format!(
                    "Work already has an active stage schedule ('{}' is '{}'). \
                     Wait for the current stage to complete or cancel before advancing.",
                    work.current_stage, work.stage_status
                ),
            });
        }

        // Gate 5: current stage must be complete (except intake, handled separately)
        if work.stage_status != "complete" && work.stage_status != "skipped" && current_idx > 0 {
            return Err(StageGateError {
                code: "FL_E_INCOMPLETE_STAGE".to_string(),
                message: format!(
                    "Current stage '{}' is '{}', not 'complete'. \
                     Complete the current stage first, or use --force to override.",
                    work.current_stage, work.stage_status
                ),
            });
        }

        // Gate 6: intake must be complete before advancing past it
        // (uses intake_status from V1.33, not stage_status)
        if work.current_stage == "intake" && work.intake_status != "complete" {
            return Err(StageGateError {
                code: "FL_E_INTAKE_INCOMPLETE".to_string(),
                message: format!(
                    "Cannot advance past intake: intake_status is '{}'. \
                     Complete intake first, or use --force to override.",
                    work.intake_status
                ),
            });
        }
    }

    Ok(())
}

/// Remove the derived completion-lock artifact; the database remains authoritative.
/// This is the existing idempotent filesystem release, not a scheduler operation.
pub fn release_completion_lock(
    workspace_dir: &std::path::Path,
    work_ref: &str,
) -> Result<(), std::io::Error> {
    let path = workspace_dir
        .join("Works")
        .join(work_ref)
        .join(".completion-lock.json");
    if path.exists() {
        std::fs::remove_file(&path)?;
    }
    Ok(())
}
