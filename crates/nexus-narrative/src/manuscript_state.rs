//! `ManuscriptState` aggregate — local-only manuscript phase machine.
//!
//! `ManuscriptState` tracks the creation phase progression (brainstorm → draft →
//! review → finalize → published). Platform does NOT own this in V1.0.
//! See data-model-v1.md §5.9B, consistency-rules-v1.md §3.3.

use crate::errors::NarrativeError;
use nexus_contracts::ManuscriptPhase;
use serde::{Deserialize, Serialize};

/// Helper to convert `ManuscriptPhase` to string.
const fn phase_to_str(phase: &ManuscriptPhase) -> &str {
    match phase {
        ManuscriptPhase::Brainstorm => "brainstorm",
        ManuscriptPhase::Draft => "draft",
        ManuscriptPhase::Review => "review",
        ManuscriptPhase::Finalize => "finalize",
        ManuscriptPhase::Published => "published",
    }
}

/// Helper to convert string to `ManuscriptPhase`.
#[allow(dead_code)]
fn str_to_phase(s: &str) -> ManuscriptPhase {
    match s {
        "draft" => ManuscriptPhase::Draft,
        "review" => ManuscriptPhase::Review,
        "finalize" => ManuscriptPhase::Finalize,
        "published" => ManuscriptPhase::Published,
        _ => ManuscriptPhase::Brainstorm,
    }
}

/// `ManuscriptState` aggregate — local manuscript phase machine.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManuscriptState {
    pub schema_version: u32,
    pub manuscript_state_id: String,
    pub workspace_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub manuscript_phase: ManuscriptPhase,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_manifest_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_confirmed_delta_sequence: Option<u64>,
    pub updated_at: String,
}

impl ManuscriptState {
    #[must_use]
    /// Create new manuscript state in brainstorm phase.
    pub fn new(workspace_id: &str, world_id: &str, creator_id: &str) -> Self {
        let manuscript_state_id =
            format!("mss_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            schema_version: 1,
            manuscript_state_id,
            workspace_id: workspace_id.to_string(),
            world_id: world_id.to_string(),
            creator_id: creator_id.to_string(),
            manuscript_phase: ManuscriptPhase::Brainstorm,
            active_manifest_id: None,
            last_confirmed_delta_sequence: None,
            updated_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Transition to next phase.
    /// Valid transitions: brainstorm→draft→review→finalize→published
    pub fn promote(&mut self) -> Result<(), NarrativeError> {
        let current = &self.manuscript_phase;
        let next = match current {
            ManuscriptPhase::Brainstorm => ManuscriptPhase::Draft,
            ManuscriptPhase::Draft => ManuscriptPhase::Review,
            ManuscriptPhase::Review => ManuscriptPhase::Finalize,
            ManuscriptPhase::Finalize => ManuscriptPhase::Published,
            ManuscriptPhase::Published => {
                return Err(NarrativeError::InvalidPhaseTransition {
                    from: phase_to_str(current).to_string(),
                    to: "none (already published)".to_string(),
                });
            }
        };

        self.manuscript_phase = next;
        self.updated_at = chrono::Utc::now().to_rfc3339();
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Set active manifest.
    pub fn set_active_manifest(&mut self, manifest_id: &str) {
        self.active_manifest_id = Some(manifest_id.to_string());
        self.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// Get current phase.
    #[must_use]
    pub const fn current_phase(&self) -> &ManuscriptPhase {
        &self.manuscript_phase
    }
    #[must_use]
    /// Check if phase transition is valid.
    pub const fn can_transition_to(&self, target: &ManuscriptPhase) -> bool {
        matches!(
            (&self.manuscript_phase, target),
            (ManuscriptPhase::Brainstorm, ManuscriptPhase::Draft)
                | (ManuscriptPhase::Draft, ManuscriptPhase::Review)
                | (ManuscriptPhase::Review, ManuscriptPhase::Finalize)
                | (ManuscriptPhase::Finalize, ManuscriptPhase::Published)
        )
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Validate provisional cleanup before finalize/published gate.
    /// Per consistency-rules-v1.md §3.3: provisional records must be promoted
    /// or cleaned before entering finalize/published.
    pub const fn validate_pre_gate_cleanup(
        &self,
        provisional_count: usize,
    ) -> Result<(), NarrativeError> {
        match &self.manuscript_phase {
            ManuscriptPhase::Finalize | ManuscriptPhase::Published if provisional_count > 0 => {
                return Err(NarrativeError::ProvisionalRecordsExist {
                    count: provisional_count,
                });
            }
            _ => {}
        }
        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::local::domain::ManuscriptState> for ManuscriptState {
    fn from(c: nexus_contracts::local::domain::ManuscriptState) -> Self {
        Self {
            schema_version: c.schema_version,
            manuscript_state_id: c.manuscript_state_id,
            workspace_id: c.workspace_id,
            world_id: c.world_id,
            creator_id: c.creator_id,
            manuscript_phase: c.manuscript_phase,
            active_manifest_id: c.active_manifest_id,
            last_confirmed_delta_sequence: c.last_confirmed_delta_sequence,
            updated_at: c.updated_at,
        }
    }
}

impl From<ManuscriptState> for nexus_contracts::local::domain::ManuscriptState {
    fn from(d: ManuscriptState) -> Self {
        Self {
            schema_version: d.schema_version,
            manuscript_state_id: d.manuscript_state_id,
            workspace_id: d.workspace_id,
            world_id: d.world_id,
            creator_id: d.creator_id,
            manuscript_phase: d.manuscript_phase,
            active_manifest_id: d.active_manifest_id,
            last_confirmed_delta_sequence: d.last_confirmed_delta_sequence,
            updated_at: d.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_brainstorm_phase() {
        let ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_author");
        assert_eq!(ms.manuscript_phase, ManuscriptPhase::Brainstorm);
        assert!(ms.manuscript_state_id.starts_with("mss_"));
    }

    #[test]
    fn test_full_phase_progression() {
        let mut ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_author");

        // brainstorm → draft
        ms.promote().unwrap();
        assert_eq!(ms.manuscript_phase, ManuscriptPhase::Draft);

        // draft → review
        ms.promote().unwrap();
        assert_eq!(ms.manuscript_phase, ManuscriptPhase::Review);

        // review → finalize
        ms.promote().unwrap();
        assert_eq!(ms.manuscript_phase, ManuscriptPhase::Finalize);

        // finalize → published
        ms.promote().unwrap();
        assert_eq!(ms.manuscript_phase, ManuscriptPhase::Published);
    }

    #[test]
    fn test_invalid_transition_brainstorm_to_finalize() {
        let ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_author");
        assert!(!ms.can_transition_to(&ManuscriptPhase::Finalize));
    }

    #[test]
    fn test_published_is_final() {
        let mut ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_author");
        for _ in 0..4 {
            ms.promote().unwrap();
        }
        assert_eq!(ms.manuscript_phase, ManuscriptPhase::Published);
        assert!(ms.promote().is_err());
    }

    #[test]
    fn test_pre_gate_cleanup_with_provisionals() {
        let mut ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_author");
        for _ in 0..3 {
            ms.promote().unwrap();
        }
        // ms is now finalize
        assert!(ms.validate_pre_gate_cleanup(5).is_err());
    }

    #[test]
    fn test_pre_gate_cleanup_no_provisionals() {
        let mut ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_author");
        for _ in 0..4 {
            ms.promote().unwrap();
        }
        // ms is now published
        assert!(ms.validate_pre_gate_cleanup(0).is_ok());
    }

    #[test]
    fn test_set_active_manifest() {
        let mut ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_author");
        ms.set_active_manifest("stm_chapter1");
        assert_eq!(ms.active_manifest_id.as_deref(), Some("stm_chapter1"));
    }

    #[test]
    fn test_phase_serialize_roundtrip() {
        let phases = vec![
            ManuscriptPhase::Brainstorm,
            ManuscriptPhase::Draft,
            ManuscriptPhase::Review,
            ManuscriptPhase::Finalize,
            ManuscriptPhase::Published,
        ];
        for phase in phases {
            let json = serde_json::to_string(&phase).unwrap();
            let deserialized: ManuscriptPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(phase, deserialized);
        }
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut ms = ManuscriptState::new("wrk_test", "wld_test", "ctr_author");
        ms.promote().unwrap();
        ms.set_active_manifest("stm_ch1");
        let json = serde_json::to_string(&ms).unwrap();
        let deserialized: ManuscriptState = serde_json::from_str(&json).unwrap();
        assert_eq!(ms, deserialized);
    }
}
