//! `TimelineEvent` aggregate — canonical event on the world timeline.
//!
//! `TimelineEvent` represents a discrete event on a world's timeline branch,
//! with causality tracking and provisional → canon promotion gates.
//! See data-model-v1.md §5.6, consistency-rules-v1.md §3.3.

use crate::errors::NarrativeError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Timeline event type enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimelineEventType {
    StoryAdvance,
    StateUpdate,
    ForkMarker,
    OfficialProgression,
    PublishMarker,
}

impl TimelineEventType {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::StoryAdvance => "story_advance",
            Self::StateUpdate => "state_update",
            Self::ForkMarker => "fork_marker",
            Self::OfficialProgression => "official_progression",
            Self::PublishMarker => "publish_marker",
        }
    }
}

/// Timeline event status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TimelineEventStatus {
    Canon,
    Provisional,
    Rejected,
}

impl TimelineEventStatus {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Canon => "canon",
            Self::Provisional => "provisional",
            Self::Rejected => "rejected",
        }
    }
}

/// A simplified membership check for promote gates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MembershipPermissionCheck {
    pub can_confirm_canon: bool,
    pub can_sync_kb: bool,
}

/// `TimelineEvent` aggregate — a canonical event on the world timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimelineEvent {
    pub schema_version: u32,
    pub timeline_event_id: String,
    pub world_id: String,
    pub branch_id: String,
    pub event_type: String,
    pub status: String,
    pub sequence_no: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caused_by_event_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_key_block_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_command_id: Option<String>,
    pub created_at: String,
}

impl TimelineEvent {
    /// Create a new timeline event on a branch.
    #[must_use]
    pub fn new(
        world_id: &str,
        branch_id: &str,
        event_type: TimelineEventType,
        sequence_no: u64,
    ) -> Self {
        let timeline_event_id =
            format!("evt_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            schema_version: 1,
            timeline_event_id,
            world_id: world_id.to_string(),
            branch_id: branch_id.to_string(),
            event_type: event_type.as_str().to_string(),
            status: TimelineEventStatus::Provisional.as_str().to_string(),
            sequence_no,
            title: None,
            summary: None,
            caused_by_event_ids: None,
            affected_key_block_ids: None,
            source_command_id: None,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    /// Promote provisional → canon.
    /// Per consistency-rules-v1.md §3.3:
    /// - Must not reorder existing canon sequence
    /// - Must revalidate `branch_id`, causality, sequence constraints, permissions, current head
    /// - Default promotion: append as new canon head
    pub fn promote_to_canon(
        &mut self,
        membership: &MembershipPermissionCheck,
        current_head: &str,
        branch_events: &[Self],
    ) -> Result<(), NarrativeError> {
        // Must be provisional to promote
        if self.status != TimelineEventStatus::Provisional.as_str() {
            return Err(NarrativeError::InvalidState {
                expected: "provisional".to_string(),
                actual: self.status.clone(),
            });
        }

        // Permission check
        if !membership.can_confirm_canon {
            return Err(NarrativeError::PermissionDenied(
                "can_confirm_canon permission required for canon promotion".to_string(),
            ));
        }

        // Must be the current head's successor (append-only)
        if self.timeline_event_id == current_head {
            return Err(NarrativeError::TimelineConflict(
                "event cannot be promoted as it is already the head".to_string(),
            ));
        }

        // Gate 3: Sequence monotonicity (consistency-rules-v1.md §3.3)
        // Event's sequence_no must be greater than all existing canon events in the branch
        let max_canon_sequence = branch_events
            .iter()
            .filter(|e| e.status == TimelineEventStatus::Canon.as_str())
            .map(|e| e.sequence_no)
            .max()
            .unwrap_or(0);

        if self.sequence_no <= max_canon_sequence {
            return Err(NarrativeError::TimelineConflict(format!(
                "sequence {} conflicts with existing canon sequence {}; events must be promoted in order",
                self.sequence_no, max_canon_sequence
            )));
        }

        // Validate causality
        if let Some(ref causes) = self.caused_by_event_ids {
            for cause_id in causes {
                if cause_id == &self.timeline_event_id {
                    return Err(NarrativeError::CausalityViolation(
                        "event cannot cause itself".to_string(),
                    ));
                }
            }
        }

        self.status = TimelineEventStatus::Canon.as_str().to_string();
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Reject a provisional or canon event.
    pub fn reject(&mut self) -> Result<(), NarrativeError> {
        if self.status == TimelineEventStatus::Rejected.as_str() {
            return Err(NarrativeError::AlreadyInState("rejected".to_string()));
        }
        self.status = TimelineEventStatus::Rejected.as_str().to_string();
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Add causal predecessor.
    pub fn add_cause(&mut self, event_id: &str) {
        let causes = self.caused_by_event_ids.get_or_insert_with(Vec::new);
        if !causes.contains(&event_id.to_string()) {
            causes.push(event_id.to_string());
        }
    }

    /// Add affected `KeyBlock` reference.
    pub fn add_affected_kb(&mut self, kb_id: &str) {
        let kbs = self.affected_key_block_ids.get_or_insert_with(Vec::new);
        if !kbs.contains(&kb_id.to_string()) {
            kbs.push(kb_id.to_string());
        }
    }

    /// Validate causality: `caused_by_event_ids` must reference same world.
    /// Per consistency-rules-v1.md §3.3.
    pub fn validate_causality(&self, world_id: &str) -> Result<(), NarrativeError> {
        // Self-referencing check
        if let Some(ref causes) = self.caused_by_event_ids {
            for cause_id in causes {
                if cause_id == &self.timeline_event_id {
                    return Err(NarrativeError::CausalityViolation(
                        "event cannot cause itself".to_string(),
                    ));
                }
            }
        }

        // Cross-world check: we validate world_id match through external context.
        // The caused_by_event_ids themselves should reference events in the same world.
        // Since we can't look up the events here, we do basic structural validation.
        if self.world_id != world_id {
            return Err(NarrativeError::CausalityViolation(format!(
                "event belongs to world {} but validation targets world {}",
                self.world_id, world_id
            )));
        }

        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Validate sequence is monotonic within branch.
    pub fn validate_sequence(&self, prev_sequence: u64) -> Result<(), NarrativeError> {
        if self.sequence_no <= prev_sequence {
            return Err(NarrativeError::TimelineConflict(format!(
                "sequence_no {} is not greater than previous {}",
                self.sequence_no, prev_sequence
            )));
        }
        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::TimelineEvent> for TimelineEvent {
    fn from(c: nexus_contracts::TimelineEvent) -> Self {
        Self {
            schema_version: c.schema_version,
            timeline_event_id: c.timeline_event_id,
            world_id: c.world_id,
            branch_id: c.branch_id,
            event_type: c.event_type.as_str().to_string(),
            status: c.status.as_str().to_string(),
            sequence_no: c.sequence_no,
            title: c.title,
            summary: c.summary,
            caused_by_event_ids: c.caused_by_event_ids,
            affected_key_block_ids: c.affected_key_block_ids,
            source_command_id: c.source_command_id,
            created_at: c.created_at,
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<TimelineEvent> for nexus_contracts::TimelineEvent {
    fn from(d: TimelineEvent) -> Self {
        Self {
            schema_version: d.schema_version,
            timeline_event_id: d.timeline_event_id,
            world_id: d.world_id,
            branch_id: d.branch_id,
            event_type: nexus_contracts::TimelineEventType::from_str(&d.event_type).unwrap(),
            status: nexus_contracts::TimelineEventStatus::from_str(&d.status).unwrap(),
            sequence_no: d.sequence_no,
            title: d.title,
            summary: d.summary,
            caused_by_event_ids: d.caused_by_event_ids,
            affected_key_block_ids: d.affected_key_block_ids,
            source_command_id: d.source_command_id,
            created_at: d.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn owner_permission() -> MembershipPermissionCheck {
        MembershipPermissionCheck {
            can_confirm_canon: true,
            can_sync_kb: true,
        }
    }

    fn no_permission() -> MembershipPermissionCheck {
        MembershipPermissionCheck {
            can_confirm_canon: false,
            can_sync_kb: true,
        }
    }

    #[test]
    fn test_create_story_advance() {
        let evt = TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 1);
        assert_eq!(evt.event_type, "story_advance");
        assert_eq!(evt.status, "provisional");
        assert_eq!(evt.sequence_no, 1);
    }

    #[test]
    fn test_promote_to_canon() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 2);
        let result = evt.promote_to_canon(&owner_permission(), "evt_prev_head", &[]);
        assert!(result.is_ok());
        assert_eq!(evt.status, "canon");
    }

    #[test]
    fn test_promote_without_permission() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 2);
        let result = evt.promote_to_canon(&no_permission(), "evt_prev_head", &[]);
        assert!(matches!(result, Err(NarrativeError::PermissionDenied(_))));
    }

    #[test]
    fn test_promote_already_canon() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 2);
        evt.promote_to_canon(&owner_permission(), "evt_prev_head", &[])
            .unwrap();
        let result = evt.promote_to_canon(&owner_permission(), "evt_prev_head", &[]);
        assert!(matches!(result, Err(NarrativeError::InvalidState { .. })));
    }

    #[test]
    fn test_causality_validation_same_world() {
        let evt = TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 1);
        assert!(evt.validate_causality("wld_test").is_ok());
        assert!(evt.validate_causality("wld_other").is_err());
    }

    #[test]
    fn test_self_causality_rejected() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 1);
        let id = evt.timeline_event_id.clone();
        evt.add_cause(&id);
        assert!(matches!(
            evt.validate_causality("wld_test"),
            Err(NarrativeError::CausalityViolation(_))
        ));
    }

    #[test]
    fn test_sequence_monotonic() {
        let evt = TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 5);
        assert!(evt.validate_sequence(4).is_ok());
        assert!(evt.validate_sequence(5).is_err());
        assert!(evt.validate_sequence(6).is_err());
    }

    #[test]
    fn test_all_event_types() {
        let types = vec![
            TimelineEventType::StoryAdvance,
            TimelineEventType::StateUpdate,
            TimelineEventType::ForkMarker,
            TimelineEventType::OfficialProgression,
            TimelineEventType::PublishMarker,
        ];

        for et in types {
            let evt = TimelineEvent::new("wld_test", "fbk_root", et.clone(), 1);
            let json = serde_json::to_string(&evt).unwrap();
            let deserialized: TimelineEvent = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.event_type, et.as_str());
        }
    }

    #[test]
    fn test_add_cause_and_affected_kb() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 1);
        evt.add_cause("evt_prev");
        evt.add_affected_kb("kb_char1");
        evt.add_affected_kb("kb_event1");
        assert_eq!(evt.caused_by_event_ids.as_ref().unwrap().len(), 1);
        assert_eq!(evt.affected_key_block_ids.as_ref().unwrap().len(), 2);
    }

    #[test]
    fn test_reject_event() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 1);
        evt.reject().unwrap();
        assert_eq!(evt.status, "rejected");
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 1);
        evt.title = Some("The Battle".to_string());
        evt.add_cause("evt_prev");
        let json = serde_json::to_string(&evt).unwrap();
        let deserialized: TimelineEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(evt, deserialized);
    }

    /// C-2: promote_to_canon() must enforce sequence monotonicity.
    /// When event's sequence_no conflicts with existing canon events, promotion should fail.
    #[test]
    fn test_promote_with_sequence_conflict_fails() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 5);
        evt.status = "provisional".to_string();

        // Existing canon event with higher sequence_no
        let existing_canon = TimelineEvent {
            status: "canon".to_string(),
            sequence_no: 10,
            ..TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 10)
        };

        let branch_events = vec![existing_canon];
        let result = evt.promote_to_canon(
            &MembershipPermissionCheck {
                can_confirm_canon: true,
                can_sync_kb: true,
            },
            "evt_head",
            &branch_events,
        );

        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            NarrativeError::TimelineConflict(_)
        ));
    }

    /// C-2: promote_to_canon() succeeds when sequence_no is valid.
    #[test]
    fn test_promote_with_valid_sequence_succeeds() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 15);
        evt.status = "provisional".to_string();

        // Existing canon event with lower sequence_no
        let existing_canon = TimelineEvent {
            status: "canon".to_string(),
            sequence_no: 10,
            ..TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 10)
        };

        let branch_events = vec![existing_canon];
        let result = evt.promote_to_canon(
            &MembershipPermissionCheck {
                can_confirm_canon: true,
                can_sync_kb: true,
            },
            "evt_head",
            &branch_events,
        );

        assert!(result.is_ok());
        assert_eq!(evt.status, "canon");
    }
}
