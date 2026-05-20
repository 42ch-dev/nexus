//! Consistency rules — cross-aggregate validation.
//!
//! Implements global invariants (G1-G6) and domain-specific validation
//! from consistency-rules-v1.md.

use crate::errors::NarrativeError;
use crate::timeline_event::TimelineEvent;
use nexus_kb::key_block::KeyBlock;

/// Maximum excerpt length per G6.
pub const MAX_EXCERPT_LENGTH: usize = 1024;

/// Maximum provisional TTL in days per consistency-rules-v1.md §3.3.
pub const PROVISIONAL_TTL_DAYS: i64 = 30;

/// Validate global hard invariant G1: Envelope integrity.
/// Checks that required fields are present.
pub fn validate_envelope_integrity(
    bundle_id: &str,
    world_id: &str,
    creator_id: &str,
    deltas_count: usize,
) -> Result<(), NarrativeError> {
    if bundle_id.is_empty() {
        return Err(NarrativeError::ValidationError(
            "bundle_id is required".to_string(),
        ));
    }
    if world_id.is_empty() {
        return Err(NarrativeError::ValidationError(
            "world_id is required".to_string(),
        ));
    }
    if creator_id.is_empty() {
        return Err(NarrativeError::ValidationError(
            "creator_id is required".to_string(),
        ));
    }
    if deltas_count == 0 {
        return Err(NarrativeError::ValidationError(
            "deltas must not be empty".to_string(),
        ));
    }
    Ok(())
}
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
/// Validate global hard invariant G4: Schema and enum closure.
/// Ensures all enum values are valid.
pub fn validate_block_type(block_type: &str) -> Result<(), NarrativeError> {
    match block_type {
        "character" | "ability" | "scene" | "organization" | "item" | "conflict" | "info_point"
        | "event" => Ok(()),
        other => Err(NarrativeError::ValidationError(format!(
            "invalid BlockType: {}",
            other
        ))),
    }
}

/// Validate memory type enum closure.
pub fn validate_memory_type(memory_type: &str) -> Result<(), NarrativeError> {
    match memory_type {
        "canon" | "working" | "experience" => Ok(()),
        other => Err(NarrativeError::ValidationError(format!(
            "invalid MemoryType: {}",
            other
        ))),
    }
}
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
/// Validate manuscript phase enum closure.
pub fn validate_manuscript_phase(phase: &str) -> Result<(), NarrativeError> {
    match phase {
        "brainstorm" | "draft" | "review" | "finalize" | "published" => Ok(()),
        other => Err(NarrativeError::ValidationError(format!(
            "invalid ManuscriptPhase: {}",
            other
        ))),
    }
}
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
/// Validate global hard invariant G6: Sync boundary — excerpt length.
pub const fn validate_excerpt_length(excerpt: &str) -> Result<(), NarrativeError> {
    if excerpt.len() > MAX_EXCERPT_LENGTH {
        return Err(NarrativeError::ExcerptTooLong {
            actual: excerpt.len(),
            max: MAX_EXCERPT_LENGTH,
        });
    }
    Ok(())
}
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
/// Validate `KeyBlock` provisional → confirmed gate (consistency-rules-v1.md §3.2).
pub fn validate_kb_confirm_gate(
    kb: &KeyBlock,
    has_permission: bool,
    base_revision: u64,
    has_conflicts: bool,
) -> Result<(), NarrativeError> {
    // Gate 1: Permission
    if !has_permission {
        return Err(NarrativeError::PermissionDenied(
            "can_confirm_canon permission required".to_string(),
        ));
    }
    // Gate 2: Version match
    let current_rev = kb.revision.unwrap_or(0);
    if current_rev != base_revision {
        return Err(NarrativeError::RevisionMismatch {
            expected: base_revision,
            actual: current_rev,
        });
    }
    // Gate 3: Required fields
    if kb.canonical_name.trim().is_empty() {
        return Err(NarrativeError::ValidationError(
            "canonical_name is required".to_string(),
        ));
    }
    // Gate 5: No conflicts
    if has_conflicts {
        return Err(NarrativeError::UnresolvedConflict(
            "unresolved hard conflict".to_string(),
        ));
    }
    Ok(())
}
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
/// Validate `TimelineEvent` provisional → canon gate (consistency-rules-v1.md §3.3).
pub fn validate_timeline_promote_gate(
    event: &TimelineEvent,
    has_permission: bool,
) -> Result<(), NarrativeError> {
    if event.status != "provisional" {
        return Err(NarrativeError::InvalidState {
            expected: "provisional".to_string(),
            actual: event.status.clone(),
        });
    }
    if !has_permission {
        return Err(NarrativeError::PermissionDenied(
            "can_confirm_canon permission required".to_string(),
        ));
    }
    Ok(())
}
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
/// Validate Fork creation (consistency-rules-v1.md §3.4).
pub fn validate_fork_creation(
    parent_world_id: &str,
    parent_branch_id: &str,
    forked_from_event_id: &str,
) -> Result<(), NarrativeError> {
    if parent_world_id.is_empty() {
        return Err(NarrativeError::ValidationError(
            "parent_world_id is required".to_string(),
        ));
    }
    if parent_branch_id.is_empty() {
        return Err(NarrativeError::ValidationError(
            "parent_branch_id is required".to_string(),
        ));
    }
    if forked_from_event_id.is_empty() {
        return Err(NarrativeError::ValidationError(
            "forked_from_event_id is required".to_string(),
        ));
    }
    Ok(())
}
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
///
/// # Errors
/// Returns `Err(NarrativeError::...)` if validation fails.
/// Validate Memory scope (consistency-rules-v1.md §3.6).
pub fn validate_memory_scope(
    memory_type: &str,
    creator_id: &str,
    world_id: &str,
) -> Result<(), NarrativeError> {
    if memory_type.is_empty() {
        return Err(NarrativeError::ValidationError(
            "memory_type is required".to_string(),
        ));
    }
    if creator_id.is_empty() {
        return Err(NarrativeError::ValidationError(
            "creator_id is required".to_string(),
        ));
    }
    if world_id.is_empty() {
        return Err(NarrativeError::ValidationError(
            "world_id is required".to_string(),
        ));
    }
    validate_memory_type(memory_type)?;
    Ok(())
}
#[must_use]
/// Check if a provisional record has exceeded its TTL.
pub fn is_provisional_expired(created_at: &str) -> bool {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(created_at) {
        let now = chrono::Utc::now();
        let age = now.signed_duration_since(dt.with_timezone(&chrono::Utc));
        age.num_days() > PROVISIONAL_TTL_DAYS
    } else {
        // If we can't parse the date, don't consider it expired
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_contracts::BlockType;

    #[test]
    fn test_validate_envelope_integrity_valid() {
        assert!(validate_envelope_integrity("bdl_1", "wld_1", "ctr_1", 1).is_ok());
    }

    #[test]
    fn test_validate_envelope_integrity_missing_fields() {
        assert!(validate_envelope_integrity("", "wld_1", "ctr_1", 1).is_err());
        assert!(validate_envelope_integrity("bdl_1", "", "ctr_1", 1).is_err());
        assert!(validate_envelope_integrity("bdl_1", "wld_1", "", 1).is_err());
        assert!(validate_envelope_integrity("bdl_1", "wld_1", "ctr_1", 0).is_err());
    }

    #[test]
    fn test_validate_block_type_valid() {
        assert!(validate_block_type("character").is_ok());
        assert!(validate_block_type("ability").is_ok());
        assert!(validate_block_type("scene").is_ok());
        assert!(validate_block_type("organization").is_ok());
        assert!(validate_block_type("item").is_ok());
        assert!(validate_block_type("conflict").is_ok());
        assert!(validate_block_type("info_point").is_ok());
        assert!(validate_block_type("event").is_ok());
    }

    #[test]
    fn test_validate_block_type_invalid() {
        assert!(validate_block_type("location").is_err());
        assert!(validate_block_type("concept").is_err());
        assert!(validate_block_type("").is_err());
    }

    #[test]
    fn test_validate_memory_type_valid() {
        assert!(validate_memory_type("canon").is_ok());
        assert!(validate_memory_type("working").is_ok());
        assert!(validate_memory_type("experience").is_ok());
    }

    #[test]
    fn test_validate_memory_type_invalid() {
        assert!(validate_memory_type("knowledge").is_err());
        assert!(validate_memory_type("soul").is_err());
    }

    #[test]
    fn test_validate_manuscript_phase_valid() {
        assert!(validate_manuscript_phase("brainstorm").is_ok());
        assert!(validate_manuscript_phase("draft").is_ok());
        assert!(validate_manuscript_phase("review").is_ok());
        assert!(validate_manuscript_phase("finalize").is_ok());
        assert!(validate_manuscript_phase("published").is_ok());
    }

    #[test]
    fn test_validate_manuscript_phase_invalid() {
        assert!(validate_manuscript_phase("write").is_err());
        assert!(validate_manuscript_phase("canon").is_err());
    }

    #[test]
    fn test_validate_excerpt_length_valid() {
        assert!(validate_excerpt_length(&"x".repeat(1024)).is_ok());
    }

    #[test]
    fn test_validate_excerpt_length_invalid() {
        assert!(validate_excerpt_length(&"x".repeat(1025)).is_err());
    }

    #[test]
    fn test_validate_kb_confirm_gate() {
        let kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        assert!(validate_kb_confirm_gate(&kb, true, 0, false).is_ok());
    }

    #[test]
    fn test_validate_kb_confirm_gate_no_permission() {
        let kb = KeyBlock::new("wld_test", BlockType::Character, "Hero");
        assert!(validate_kb_confirm_gate(&kb, false, 0, false).is_err());
    }

    #[test]
    fn test_validate_fork_creation() {
        assert!(validate_fork_creation("wld_parent", "fbk_root", "evt_1").is_ok());
        assert!(validate_fork_creation("", "fbk_root", "evt_1").is_err());
    }

    #[test]
    fn test_validate_memory_scope() {
        assert!(validate_memory_scope("canon", "ctr_1", "wld_1").is_ok());
        assert!(validate_memory_scope("invalid", "ctr_1", "wld_1").is_err());
    }

    #[test]
    fn test_provisional_ttl() {
        // Recent timestamp — not expired
        let recent = chrono::Utc::now().to_rfc3339();
        assert!(!is_provisional_expired(&recent));

        // Old timestamp (31 days ago) — expired
        let old = (chrono::Utc::now() - chrono::Duration::days(31)).to_rfc3339();
        assert!(is_provisional_expired(&old));
    }
}
