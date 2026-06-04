//! Shared FL-E stage gate validation and schedule wiring (V1.34 creator-workflow-fl-e §3–4).
//!
//! Provides:
//! - `check_stage_advance` — the single authoritative gate function
//!   used by both CLI `stage_advance` and daemon `PATCH /v1/local/works/{id}`.
//! - `preset_for_stage` — normative stage → preset mapping (spec §4).
//! - `build_stage_schedule_label` — schedule label for stage advance.

use nexus_contracts::local::orchestration::{stage_index, FL_E_STAGES};

/// Normative stage → default preset mapping (spec §4).
///
/// Returns the canonical preset ID for a given FL-E stage.
/// Returns `None` for unknown stages.
///
/// This is the authoritative wiring function for the preset chain:
///
/// | Stage     | Preset                  |
/// |-----------|-------------------------|
/// | intake    | `creative-brief-intake` |
/// | research  | `research`              |
/// | produce   | `novel-writing`         |
/// | review    | `reflection-loop`       |
/// | persist   | `kb-extract`            |
#[must_use]
pub fn preset_for_stage(stage: &str) -> Option<&'static str> {
    crate::preset::validation::default_preset_for_stage(stage)
}

/// Build the schedule label for a stage advance (spec §4).
#[must_use]
pub fn build_stage_schedule_label(stage: &str, work_id: &str) -> String {
    format!("FL-E stage: {stage} (work: {work_id})")
}

/// Error returned when a stage advance fails validation.
#[derive(Debug, Clone)]
pub struct StageGateError {
    /// Human-readable error message.
    pub message: String,
}

impl std::fmt::Display for StageGateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
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
                message: format!(
                    "Work is already at stage '{}' ({}). Use a different target stage.",
                    work.current_stage, work.stage_status
                ),
            });
        }

        // Gate 2: cannot advance backwards
        if target_idx <= current_idx {
            return Err(StageGateError {
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

#[cfg(test)]
mod tests {
    use super::*;

    fn work_state(stage: &str, status: &str, intake: &str) -> WorkStageState {
        WorkStageState {
            current_stage: stage.to_string(),
            stage_status: status.to_string(),
            intake_status: intake.to_string(),
        }
    }

    #[test]
    fn valid_advance_intake_to_research() {
        let work = work_state("intake", "complete", "complete");
        assert!(check_stage_advance(&work, "research", false).is_ok());
    }

    #[test]
    fn reject_unknown_target() {
        let work = work_state("intake", "pending", "pending");
        let err = check_stage_advance(&work, "unknown", false).unwrap_err();
        assert!(err.message.contains("Unknown stage"));
    }

    #[test]
    fn reject_same_stage() {
        let work = work_state("research", "active", "complete");
        let err = check_stage_advance(&work, "research", false).unwrap_err();
        assert!(err.message.contains("already at stage"));
    }

    #[test]
    fn reject_backwards() {
        let work = work_state("research", "complete", "complete");
        let err = check_stage_advance(&work, "intake", false).unwrap_err();
        assert!(err.message.contains("backwards"));
    }

    #[test]
    fn reject_skip_without_force() {
        let work = work_state("intake", "complete", "complete");
        let err = check_stage_advance(&work, "produce", false).unwrap_err();
        assert!(err.message.contains("Cannot skip"));
    }

    #[test]
    fn allow_skip_with_force() {
        let work = work_state("intake", "pending", "pending");
        assert!(check_stage_advance(&work, "produce", true).is_ok());
    }

    #[test]
    fn reject_incomplete_current() {
        let work = work_state("research", "pending", "complete");
        let err = check_stage_advance(&work, "produce", false).unwrap_err();
        assert!(err.message.contains("not 'complete'"));
    }

    #[test]
    fn reject_intake_not_complete() {
        let work = work_state("intake", "complete", "pending");
        let err = check_stage_advance(&work, "research", false).unwrap_err();
        assert!(err.message.contains("intake_status"));
    }

    #[test]
    fn reject_active_schedule_exists() {
        let work = work_state("research", "active", "complete");
        let err = check_stage_advance(&work, "produce", false).unwrap_err();
        assert!(err.message.contains("active stage schedule"));
    }

    #[test]
    fn allow_advance_after_complete() {
        let work = work_state("research", "complete", "complete");
        assert!(check_stage_advance(&work, "produce", false).is_ok());
    }

    // ── T1: preset_for_stage schedule wiring for all 4 post-intake stages ──────

    #[test]
    fn preset_for_stage_research() {
        assert_eq!(preset_for_stage("research"), Some("research"));
    }

    #[test]
    fn preset_for_stage_produce() {
        assert_eq!(preset_for_stage("produce"), Some("novel-writing"));
    }

    #[test]
    fn preset_for_stage_review() {
        assert_eq!(preset_for_stage("review"), Some("reflection-loop"));
    }

    #[test]
    fn preset_for_stage_persist() {
        assert_eq!(preset_for_stage("persist"), Some("kb-extract"));
    }

    #[test]
    fn preset_for_stage_intake() {
        assert_eq!(
            preset_for_stage("intake"),
            Some("creative-brief-intake")
        );
    }

    #[test]
    fn preset_for_stage_unknown() {
        assert_eq!(preset_for_stage("unknown"), None);
    }

    #[test]
    fn schedule_label_format() {
        let label = build_stage_schedule_label("research", "wrk_abc123");
        assert_eq!(label, "FL-E stage: research (work: wrk_abc123)");
    }

    /// End-to-end gate + schedule wiring for each stage transition.
    /// Validates that stage advance gates pass AND the correct preset is resolved.
    #[test]
    fn full_chain_gate_and_preset_resolution() {
        // intake → research
        let intake_done = work_state("intake", "complete", "complete");
        assert!(check_stage_advance(&intake_done, "research", false).is_ok());
        assert_eq!(preset_for_stage("research"), Some("research"));

        // research → produce
        let research_done = work_state("research", "complete", "complete");
        assert!(check_stage_advance(&research_done, "produce", false).is_ok());
        assert_eq!(preset_for_stage("produce"), Some("novel-writing"));

        // produce → review
        let produce_done = work_state("produce", "complete", "complete");
        assert!(check_stage_advance(&produce_done, "review", false).is_ok());
        assert_eq!(preset_for_stage("review"), Some("reflection-loop"));

        // review → persist
        let review_done = work_state("review", "complete", "complete");
        assert!(check_stage_advance(&review_done, "persist", false).is_ok());
        assert_eq!(preset_for_stage("persist"), Some("kb-extract"));
    }
}
