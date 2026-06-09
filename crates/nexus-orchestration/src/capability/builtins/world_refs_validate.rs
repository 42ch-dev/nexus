//! `world_refs` validator for novel chapters (V1.40 DF-63 W4).
//!
//! Validates that `world_refs` entries in chapter frontmatter reference
//! existing World KB items under the Work's `world_id`.
//!
//! Validation timing (per spec §3.5.1.4):
//! - **Outline stage**: invalid entries produce warnings (non-blocking).
//! - **Finalize stage**: invalid entries produce errors (blocking) unless
//!   `--force` is provided.
//! - **Worldless Works**: no `world_id` gate failure; `world_refs` validation
//!   is warn-only (does not block).

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::BuildHasher;

/// A single validation finding for a `world_refs` entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorldRefFinding {
    /// The invalid `world_refs` entry.
    pub entry: String,
    /// Finding kind: `warning` or `error`.
    pub severity: WorldRefSeverity,
    /// Human-readable description.
    pub message: String,
}

/// Severity of a `world_refs` finding.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum WorldRefSeverity {
    /// Non-blocking advisory (outline stage or worldless Work).
    Warning,
    /// Blocking error (finalize stage, World-bound Work).
    Error,
}

/// Result of `world_refs` validation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldRefsValidationResult {
    /// All findings (warnings + errors).
    pub findings: Vec<WorldRefFinding>,
    /// Whether the validation blocks the transition.
    pub blocks: bool,
}

/// Parameters controlling `world_refs` validation behavior.
#[derive(Debug, Clone)]
pub struct WorldRefsValidationParams {
    /// The validation stage: `outline` or `finalize`.
    pub stage: ValidationStage,
    /// Whether the Work has a `world_id` (World-bound vs worldless).
    pub is_world_bound: bool,
    /// Whether the user passed `--force` to override blocking errors.
    pub force: bool,
}

/// Stage of validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValidationStage {
    /// Outline stage — warnings only.
    Outline,
    /// Finalize stage — errors for World-bound Works.
    Finalize,
}

/// Validate `world_refs` entries against a set of known valid World KB item ids.
///
/// # Arguments
///
/// * `world_refs` - The `world_refs` array from chapter frontmatter (may be empty).
/// * `valid_ids` - Set of known valid World KB item ids under `work.world_id`.
/// * `params` - Validation stage and context.
///
/// # Returns
///
/// A `WorldRefsValidationResult` with findings and whether the transition is blocked.
///
/// # Canonicalization (spec §3.5.1.4)
///
/// 1. Trim leading/trailing whitespace.
/// 2. Case-sensitive comparison.
/// 3. Reject duplicates after trimming.
/// 4. Preserve author order.
#[must_use = "validation result must be checked for blocking findings"]
pub fn validate_world_refs<S: BuildHasher>(
    world_refs: &[String],
    valid_ids: &HashSet<String, S>,
    params: &WorldRefsValidationParams,
) -> WorldRefsValidationResult {
    let mut findings = Vec::new();
    let mut seen = HashSet::new();
    let mut has_error = false;

    for raw in world_refs {
        let trimmed = raw.trim().to_string();

        // Reject duplicates after trimming
        if !seen.insert(trimmed.clone()) {
            findings.push(WorldRefFinding {
                entry: raw.clone(),
                severity: WorldRefSeverity::Warning,
                message: format!("duplicate world_refs entry (after trim): '{trimmed}'"),
            });
            continue;
        }

        // Reject empty after trim
        if trimmed.is_empty() {
            findings.push(WorldRefFinding {
                entry: raw.clone(),
                severity: WorldRefSeverity::Warning,
                message: "empty world_refs entry after trimming".to_string(),
            });
            continue;
        }

        // Check existence in valid_ids
        if !valid_ids.contains(&trimmed) {
            // Determine severity based on stage and world binding
            let (severity, message) = if !params.is_world_bound {
                // Worldless Work: always warn-only
                (
                    WorldRefSeverity::Warning,
                    format!(
                        "world_refs entry '{trimmed}' not found in World KB \
                         (Work is worldless — validation is advisory only)"
                    ),
                )
            } else if params.stage == ValidationStage::Outline {
                // Outline: warnings for World-bound Works
                (
                    WorldRefSeverity::Warning,
                    format!(
                        "world_refs entry '{trimmed}' not found in World KB \
                         (outline stage — entity may be provisional)"
                    ),
                )
            } else {
                // Finalize: errors for World-bound Works
                has_error = true;
                (
                    WorldRefSeverity::Error,
                    format!(
                        "world_refs entry '{trimmed}' not found in World KB \
                         (finalize stage — entity must exist or use --force)"
                    ),
                )
            };
            findings.push(WorldRefFinding {
                entry: raw.clone(),
                severity,
                message,
            });
        }
    }

    // Determine if validation blocks the transition
    let blocks = has_error && !params.force;

    WorldRefsValidationResult { findings, blocks }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_valid_ids() -> HashSet<String> {
        HashSet::from([
            "char_lin_xia".to_string(),
            "loc_neon_city".to_string(),
            "rule_magic_cost".to_string(),
        ])
    }

    #[test]
    fn all_valid_refs_no_findings() {
        let ids = make_valid_ids();
        let refs = vec!["char_lin_xia".to_string(), "loc_neon_city".to_string()];
        let params = WorldRefsValidationParams {
            stage: ValidationStage::Finalize,
            is_world_bound: true,
            force: false,
        };
        let result = validate_world_refs(&refs, &ids, &params);
        assert!(result.findings.is_empty());
        assert!(!result.blocks);
    }

    #[test]
    fn invalid_ref_at_outline_produces_warning() {
        let ids = make_valid_ids();
        let refs = vec!["char_lin_xia".to_string(), "char_unknown".to_string()];
        let params = WorldRefsValidationParams {
            stage: ValidationStage::Outline,
            is_world_bound: true,
            force: false,
        };
        let result = validate_world_refs(&refs, &ids, &params);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, WorldRefSeverity::Warning);
        assert!(!result.blocks);
    }

    #[test]
    fn invalid_ref_at_finalize_produces_error_and_blocks() {
        let ids = make_valid_ids();
        let refs = vec!["char_unknown".to_string()];
        let params = WorldRefsValidationParams {
            stage: ValidationStage::Finalize,
            is_world_bound: true,
            force: false,
        };
        let result = validate_world_refs(&refs, &ids, &params);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, WorldRefSeverity::Error);
        assert!(result.blocks);
    }

    #[test]
    fn invalid_ref_at_finalize_with_force_does_not_block() {
        let ids = make_valid_ids();
        let refs = vec!["char_unknown".to_string()];
        let params = WorldRefsValidationParams {
            stage: ValidationStage::Finalize,
            is_world_bound: true,
            force: true,
        };
        let result = validate_world_refs(&refs, &ids, &params);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, WorldRefSeverity::Error);
        assert!(!result.blocks);
    }

    #[test]
    fn worldless_work_invalid_ref_only_warns() {
        let ids = make_valid_ids();
        let refs = vec!["char_unknown".to_string()];
        let params = WorldRefsValidationParams {
            stage: ValidationStage::Finalize,
            is_world_bound: false,
            force: false,
        };
        let result = validate_world_refs(&refs, &ids, &params);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, WorldRefSeverity::Warning);
        assert!(!result.blocks);
    }

    #[test]
    fn worldless_work_empty_refs_no_error() {
        let ids = make_valid_ids();
        let refs: Vec<String> = vec![];
        let params = WorldRefsValidationParams {
            stage: ValidationStage::Finalize,
            is_world_bound: false,
            force: false,
        };
        let result = validate_world_refs(&refs, &ids, &params);
        assert!(result.findings.is_empty());
        assert!(!result.blocks);
    }

    #[test]
    fn duplicate_entry_produces_warning() {
        let ids = make_valid_ids();
        let refs = vec!["char_lin_xia".to_string(), "char_lin_xia".to_string()];
        let params = WorldRefsValidationParams {
            stage: ValidationStage::Outline,
            is_world_bound: true,
            force: false,
        };
        let result = validate_world_refs(&refs, &ids, &params);
        assert_eq!(result.findings.len(), 1);
        assert_eq!(result.findings[0].severity, WorldRefSeverity::Warning);
        assert!(result.findings[0].message.contains("duplicate"));
    }

    #[test]
    fn trim_whitespace_entries() {
        let ids = make_valid_ids();
        let refs = vec![" char_lin_xia ".to_string()];
        let params = WorldRefsValidationParams {
            stage: ValidationStage::Finalize,
            is_world_bound: true,
            force: false,
        };
        let result = validate_world_refs(&refs, &ids, &params);
        assert!(result.findings.is_empty());
        assert!(!result.blocks);
    }
}
