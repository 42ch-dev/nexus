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

    /// Add affected `WorldKbEntry` reference.
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

/// Parse a `created_at` value that may be RFC3339 or `SQLite` `datetime('now')`.
///
/// The `narrative_timeline_events.created_at` DEFAULT uses `SQLite`'s
/// `datetime('now')`, which returns `"YYYY-MM-DD HH:MM:SS"` (space separator,
/// no `T`/`Z`). Try RFC3339 first; on failure, normalize the space to `T` and
/// append `Z` (UTC), then try RFC3339 again. A final `NaiveDateTime` fallback
/// covers optional fractional seconds.
fn parse_created_at(s: &str) -> Option<chrono::DateTime<chrono::Utc>> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .or_else(|| {
            let normalized = s.replacen(' ', "T", 1) + "Z";
            chrono::DateTime::parse_from_rfc3339(&normalized)
                .ok()
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .or_else(|| {
                    chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S%.f")
                        .ok()
                        .map(|naive| naive.and_utc())
                })
        })
}

impl From<nexus_contracts::TimelineEvent> for TimelineEvent {
    fn from(c: nexus_contracts::TimelineEvent) -> Self {
        Self {
            schema_version: u32::try_from(c.schema_version.get())
                .expect("schema_version exceeds u32 range"),
            timeline_event_id: c.timeline_event_id.to_string(),
            world_id: c.world_id.to_string(),
            branch_id: c.branch_id,
            event_type: c.event_type.as_str().to_string(),
            status: c.status.as_str().to_string(),
            sequence_no: c.sequence_no,
            title: c.title.map(|t| t.to_string()),
            summary: c.summary,
            caused_by_event_ids: if c.caused_by_event_ids.is_empty() {
                None
            } else {
                Some(
                    c.caused_by_event_ids
                        .into_iter()
                        .map(|i| i.to_string())
                        .collect(),
                )
            },
            affected_key_block_ids: if c.affected_key_block_ids.is_empty() {
                None
            } else {
                Some(c.affected_key_block_ids)
            },
            source_command_id: c.source_command_id.map(|id| id.to_string()),
            created_at: c.created_at.to_rfc3339(),
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<TimelineEvent> for nexus_contracts::TimelineEvent {
    fn from(d: TimelineEvent) -> Self {
        Self {
            schema_version: std::num::NonZeroU64::new(u64::from(d.schema_version))
                .expect("schema_version must be non-zero"),
            timeline_event_id: d.timeline_event_id.parse().unwrap(),
            world_id: d.world_id.parse().unwrap(),
            branch_id: d.branch_id,
            event_type: nexus_contracts::TimelineEventType::from_str(&d.event_type).unwrap(),
            status: nexus_contracts::TimelineEventStatus::from_str(&d.status).unwrap(),
            sequence_no: d.sequence_no,
            title: d.title.map(|s| s.parse().unwrap()),
            summary: d.summary,
            caused_by_event_ids: d
                .caused_by_event_ids
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.parse().unwrap())
                .collect(),
            affected_key_block_ids: d
                .affected_key_block_ids
                .unwrap_or_default()
                .into_iter()
                .map(|s| s.parse().unwrap())
                .collect(),
            source_command_id: d.source_command_id.map(|s| s.parse().unwrap()),
            created_at: parse_created_at(&d.created_at)
                .expect("created_at must be RFC3339 or SQLite datetime('now')"),
        }
    }
}

// ── Conversion: nexus-narrative TimelineEvent ↔ spoke TimelineEvent ──────
//
// V1.143 P0 T1: the spoke standard `spoke_schemas::TimelineEvent` is the
// wire/standard boundary type for the L5 temporal axis (spec
// `spoke-adapter-architecture.md` §7.1). `TimelineEvent` (this aggregate) is
// the nexus domain type; these two `From` impls are the **sole conversion
// seam** — the adapter constructs the spoke type before calling
// spoke-operations. Call-boundary invariant §7 preserved: spoke helpers
// receive the converted spoke type only, never the nexus domain type.
//
// Field mapping (mapping contract from plan):
//   • timeline_event_id      — 1:1 String.
//   • created_at             — nexus RFC3339 String ↔ spoke Option<DateTime<Utc>>.
//   • title                  — nexus→spoke: canonical_name = first non-empty of
//                              [title, summary, timeline_event_id] (spoke
//                              canonical_name is required, minLength 1; the
//                              fallback chain guarantees it). Reverse reads
//                              canonical_name back into title.
//   • summary                — bidirectional ↔ spoke description.
//   • affected_key_block_ids — bidirectional ↔ spoke participant_entry_ids
//                              (Option<Vec<String>> ↔ Vec<String>; empty ≡ None).
//   • schema_version         — u32 ↔ NonZeroU64.
//   • caused_by_event_ids, world_id, branch_id, event_type, status (as
//     timeline_status), sequence_no, source_command_id — bidirectional,
//     carried in extensions.nexus.<field>.
//   • spoke-only fork fields (fork_id, parent_fork_id, timeline_scale,
//     source_anchor, computable_logs) — lossy: None/empty on forward, dropped
//     on reverse (nexus doesn't yet participate in spoke's fork model — V1.145).
//   • sort_key — forward only: sequence_no.to_string() (ordering hint).

use serde_json::Value;
use spoke_schemas::timeline_event::{TimelineEventCanonicalName, TimelineEventExtensionsKey};

/// Re-export the spoke standard `TimelineEvent` under a clarifying alias.
///
/// `TimelineEvent` exists in both `nexus_narrative` and `spoke_schemas`; use
/// `SpokeTimelineEvent` at conversion call sites to avoid the collision. Never
/// glob-import both types into the same scope.
pub use spoke_schemas::TimelineEvent as SpokeTimelineEvent;

/// The `extensions.nexus` namespace key (lowercase, matches the
/// `^[a-z][a-z0-9_-]*$` namespace convention — same pattern as
/// `nexus-spoke-adapter/src/extensions.rs` for `KnowledgeEntry`).
const NEXUS_NAMESPACE: &str = "nexus";

/// Construct the typed namespace lookup key for the `"nexus"` namespace.
///
/// `spoke_schemas::TimelineEvent.extensions` is keyed by the typify-generated
/// newtype `TimelineEventExtensionsKey` (regex-validated). The literal
/// `"nexus"` always satisfies the regex, so construction is infallible at
/// runtime. The type does not implement `Borrow<str>`, so a `HashMap::get`
/// lookup must construct the key explicitly.
fn nexus_ext_key() -> TimelineEventExtensionsKey {
    TimelineEventExtensionsKey::try_from(NEXUS_NAMESPACE)
        .expect("\"nexus\" matches the ^[a-z][a-z0-9_-]*$ namespace regex")
}

#[allow(clippy::fallible_impl_from)]
impl From<TimelineEvent> for SpokeTimelineEvent {
    fn from(d: TimelineEvent) -> Self {
        // canonical_name: first non-empty of [title, summary, timeline_event_id].
        // spoke requires minLength 1; the fallback chain guarantees it.
        // Scoped so the borrows end before any field moves below.
        let canonical_name = {
            let candidate = d
                .title
                .as_deref()
                .filter(|t| !t.is_empty())
                .or_else(|| d.summary.as_deref().filter(|s| !s.is_empty()))
                .unwrap_or(&d.timeline_event_id);
            TimelineEventCanonicalName::try_from(candidate)
                .expect("canonical_name fallback chain guarantees a non-empty value")
        };

        // created_at: RFC3339 String → Option<DateTime<Utc>>. Graceful on parse
        // failure; SQLite datetime('now') is normalized to RFC3339 first.
        let created_at = parse_created_at(&d.created_at);

        // Build extensions.nexus carrying the 7 typed nexus fields.
        let mut extensions: std::collections::HashMap<
            TimelineEventExtensionsKey,
            serde_json::Map<String, Value>,
        > = std::collections::HashMap::new();
        {
            let ns = extensions.entry(nexus_ext_key()).or_default();
            ns.insert("world_id".into(), Value::String(d.world_id.clone()));
            ns.insert("branch_id".into(), Value::String(d.branch_id.clone()));
            ns.insert("event_type".into(), Value::String(d.event_type.clone()));
            // status rides as `timeline_status` (avoids collision with spoke's
            // own lifecycle status semantics on the spoke type).
            ns.insert("timeline_status".into(), Value::String(d.status.clone()));
            ns.insert("sequence_no".into(), Value::Number(d.sequence_no.into()));
            if let Some(cmd) = d.source_command_id.as_ref() {
                ns.insert("source_command_id".into(), Value::String(cmd.clone()));
            }
            if let Some(causes) = d.caused_by_event_ids.as_ref() {
                ns.insert(
                    "caused_by_event_ids".into(),
                    Value::Array(causes.iter().cloned().map(Value::String).collect()),
                );
            }
        }

        Self {
            canonical_name,
            computable_logs: Vec::new(),
            created_at,
            description: d.summary,
            extensions,
            fork_id: None,
            occurred_at: None,
            parent_fork_id: None,
            participant_entry_ids: d.affected_key_block_ids.unwrap_or_default(),
            schema_version: std::num::NonZeroU64::new(u64::from(d.schema_version))
                .expect("schema_version >= 1"),
            // sort_key: ordering hint derived from sequence_no (nexus doesn't
            // author free-form sort keys; sequence_no is the canonical order).
            sort_key: Some(d.sequence_no.to_string()),
            source_anchor: None,
            timeline_event_id: d.timeline_event_id,
            timeline_scale: None,
            updated_at: None,
        }
    }
}

impl From<SpokeTimelineEvent> for TimelineEvent {
    fn from(s: SpokeTimelineEvent) -> Self {
        // Extract borrowed extensions.nexus data into owned values FIRST, so
        // subsequent field moves out of `s` are not blocked by outstanding
        // borrows (same ordering convention as WorldKbEntry↔KnowledgeEntry).
        let (
            world_id,
            branch_id,
            event_type,
            timeline_status,
            sequence_no,
            source_command_id,
            caused_by_event_ids,
        ) = {
            let ns = s.extensions.get(&nexus_ext_key());
            (
                ns.and_then(|m| m.get("world_id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                ns.and_then(|m| m.get("branch_id"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                ns.and_then(|m| m.get("event_type"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                ns.and_then(|m| m.get("timeline_status"))
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
                ns.and_then(|m| m.get("sequence_no"))
                    .and_then(Value::as_u64)
                    .unwrap_or_default(),
                ns.and_then(|m| m.get("source_command_id"))
                    .and_then(Value::as_str)
                    .map(String::from),
                ns.and_then(|m| m.get("caused_by_event_ids"))
                    .and_then(Value::as_array)
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect::<Vec<String>>()
                    })
                    .filter(|v| !v.is_empty()),
            )
        };

        // canonical_name → title (reverse of the forward fallback chain; the
        // spoke label is the nexus title carrier).
        let title = Some(String::from(s.canonical_name));

        Self {
            schema_version: u32::try_from(s.schema_version.get()).unwrap_or(1),
            timeline_event_id: s.timeline_event_id,
            world_id,
            branch_id,
            event_type,
            status: timeline_status,
            sequence_no,
            title,
            summary: s.description,
            caused_by_event_ids,
            affected_key_block_ids: if s.participant_entry_ids.is_empty() {
                None
            } else {
                Some(s.participant_entry_ids)
            },
            source_command_id,
            created_at: s
                .created_at
                .map_or_else(|| chrono::Utc::now().to_rfc3339(), |dt| dt.to_rfc3339()),
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
            let evt = TimelineEvent::new("wld_test", "fbk_root", et, 1);
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

    /// C-2: `promote_to_canon()` must enforce sequence monotonicity.
    /// When event's `sequence_no` conflicts with existing canon events, promotion should fail.
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

    /// C-2: `promote_to_canon()` succeeds when `sequence_no` is valid.
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

    // ── V1.143 P0 T1: spoke seam round-trip ─────────────────────────────
    //
    // Proves the conversion seam (`TimelineEvent ↔ spoke TimelineEvent`)
    // round-trips ALL 13 nexus fields. Mirrors the `WorldKbEntry ↔ spoke
    // KnowledgeEntry` round-trip suite in nexus-knowledge.

    #[test]
    fn spoke_seam_roundtrip_preserves_all_nexus_fields() {
        let mut evt =
            TimelineEvent::new("wld_test", "fbk_root", TimelineEventType::StoryAdvance, 42);
        evt.title = Some("The Turning Point".to_string());
        evt.summary = Some("A pivotal battle shifts the tide.".to_string());
        evt.add_cause("evt_pred_1");
        evt.add_cause("evt_pred_2");
        evt.add_affected_kb("kb_char_mira");
        evt.add_affected_kb("kb_loc_ashford");
        evt.source_command_id = Some("cmd_abc123".to_string());
        evt.status = TimelineEventStatus::Canon.as_str().to_string();
        // Deterministic RFC3339 in chrono's native `+00:00` format so
        // `to_rfc3339()` on the reverse path is byte-identical.
        evt.created_at = "2026-07-28T12:00:00+00:00".to_string();

        // Forward → spoke, reverse → nexus. `evt` is retained for comparison.
        let spoke: SpokeTimelineEvent = evt.clone().into();
        let roundtripped: TimelineEvent = spoke.into();

        // All 13 nexus fields survive the seam.
        assert_eq!(roundtripped.schema_version, evt.schema_version);
        assert_eq!(roundtripped.timeline_event_id, evt.timeline_event_id);
        assert_eq!(roundtripped.world_id, evt.world_id);
        assert_eq!(roundtripped.branch_id, evt.branch_id);
        assert_eq!(roundtripped.event_type, evt.event_type);
        assert_eq!(roundtripped.status, evt.status);
        assert_eq!(roundtripped.sequence_no, evt.sequence_no);
        assert_eq!(roundtripped.title, evt.title);
        assert_eq!(roundtripped.summary, evt.summary);
        assert_eq!(roundtripped.caused_by_event_ids, evt.caused_by_event_ids);
        assert_eq!(
            roundtripped.affected_key_block_ids,
            evt.affected_key_block_ids
        );
        assert_eq!(roundtripped.source_command_id, evt.source_command_id);
        assert_eq!(roundtripped.created_at, evt.created_at);
    }

    #[test]
    fn spoke_seam_forward_packs_extensions_nexus_correctly() {
        // The 7 typed nexus fields ride under extensions.nexus on the spoke
        // type; title maps to canonical_name (not extensions).
        let mut evt = TimelineEvent::new("wld_abc", "fbk_main", TimelineEventType::StoryAdvance, 7);
        evt.title = Some("Treaty Signing".to_string());
        evt.summary = Some("Peace at last.".to_string());
        evt.add_cause("evt_prior");
        evt.source_command_id = Some("cmd_xyz".to_string());
        evt.status = TimelineEventStatus::Provisional.as_str().to_string();

        let key = super::nexus_ext_key();
        let spoke: SpokeTimelineEvent = evt.into();
        let ns = spoke
            .extensions
            .get(&key)
            .expect("extensions.nexus namespace is always present on forward conversion");

        assert_eq!(ns["world_id"], "wld_abc");
        assert_eq!(ns["branch_id"], "fbk_main");
        assert_eq!(ns["event_type"], "story_advance");
        assert_eq!(ns["timeline_status"], "provisional");
        assert_eq!(ns["sequence_no"], 7);
        assert_eq!(ns["source_command_id"], "cmd_xyz");
        assert_eq!(ns["caused_by_event_ids"][0], "evt_prior");
        // sort_key is derived from sequence_no.
        assert_eq!(spoke.sort_key.as_deref(), Some("7"));
        // title → canonical_name (NOT in extensions).
        assert_eq!(spoke.canonical_name.to_string(), "Treaty Signing");
        assert!(!ns.contains_key("title"));
    }

    #[test]
    fn spoke_seam_canonical_name_falls_back_to_summary_then_id() {
        // When title is absent, canonical_name falls back to summary; when
        // summary is also absent, to timeline_event_id. This guarantees the
        // spoke required field (minLength 1) is always satisfied.
        let mut evt_no_title =
            TimelineEvent::new("wld_x", "fbk_y", TimelineEventType::StateUpdate, 1);
        evt_no_title.summary = Some("Summary-only event".to_string());
        let spoke: SpokeTimelineEvent = evt_no_title.into();
        assert_eq!(spoke.canonical_name.to_string(), "Summary-only event");

        let evt_bare = TimelineEvent::new("wld_x", "fbk_y", TimelineEventType::StateUpdate, 1);
        let id = evt_bare.timeline_event_id.clone();
        let spoke: SpokeTimelineEvent = evt_bare.into();
        assert_eq!(spoke.canonical_name.to_string(), id);
    }

    #[test]
    fn created_at_parses_rfc3339_and_sqlite_datetime() {
        let rfc = "2026-07-30T14:30:00+00:00";
        let sqlite = "2026-07-30 14:30:00";
        let expected = chrono::NaiveDateTime::parse_from_str(sqlite, "%Y-%m-%d %H:%M:%S")
            .unwrap()
            .and_utc();
        assert_eq!(parse_created_at(rfc), Some(expected));
        assert_eq!(parse_created_at(sqlite), Some(expected));
    }

    #[test]
    fn spoke_seam_created_at_accepts_sqlite_datetime_format() {
        let mut evt = TimelineEvent::new("wld_x", "fbk_y", TimelineEventType::StateUpdate, 1);
        evt.created_at = "2026-07-30 14:30:00".to_string();
        let spoke: SpokeTimelineEvent = evt.into();
        let expected =
            chrono::NaiveDateTime::parse_from_str("2026-07-30 14:30:00", "%Y-%m-%d %H:%M:%S")
                .unwrap()
                .and_utc();
        assert_eq!(spoke.created_at, Some(expected));
    }

    #[test]
    fn contract_seam_created_at_accepts_sqlite_datetime_format() {
        let mut evt = TimelineEvent::new("wld_x", "fbk_y", TimelineEventType::StateUpdate, 1);
        evt.created_at = "2026-07-30 14:30:00".to_string();
        let c: nexus_contracts::TimelineEvent = evt.into();
        assert_eq!(c.created_at.to_rfc3339(), "2026-07-30T14:30:00+00:00");
    }

    #[test]
    fn spoke_seam_empty_vecs_round_trip_to_none() {
        // nexus Option<Vec<String>> ≡ None maps to spoke empty Vec and back
        // to None (empty ≡ None contract for caused_by_event_ids and
        // affected_key_block_ids).
        let evt = TimelineEvent::new("wld_x", "fbk_y", TimelineEventType::StateUpdate, 1);
        let spoke: SpokeTimelineEvent = evt.into();
        let roundtripped: TimelineEvent = spoke.into();
        assert_eq!(roundtripped.caused_by_event_ids, None);
        assert_eq!(roundtripped.affected_key_block_ids, None);
    }
}
