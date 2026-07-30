//! Delegation wrapper tests.
//!
//! Each wrapper is invoked with a spoke-standard operand and the result is
//! compared against the underlying `spoke_operations` function for the same
//! operand — proving the wrapper is a transparent pass-through that returns
//! the spoke result unchanged. The one exception is
//! [`nexus_spoke_adapter::ops::build_assemble_packet`], which wraps each
//! `KnowledgeEntry` into a `KnowledgeEntryForAssemble` before delegating;
//! for that one we verify behavior against the equivalent direct call.

use std::num::NonZeroU64;

use nexus_spoke_adapter::ops::{
    apply_promote, assert_revision, build_assemble_packet, merge_extensions,
    transition_finding_status, transition_status, validate_promote,
};
use nexus_spoke_adapter::{ExtensionMap, SpokeResult};
use serde_json::{json, Map};
use spoke_operations::{
    apply_promote_acceptance, assert_revision_match,
    build_assemble_packet as spoke_build_assemble_packet, merge_extension_maps,
    transition_finding_status as spoke_transition_finding_status,
    transition_knowledge_entry_status, validate_promote_request, BuildAssemblePacketInput,
    KnowledgeEntryForAssemble,
};
use spoke_schemas::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryCanonicalName};
use spoke_schemas::{Finding, KnowledgeEntry, PromoteRequest};

// ── fixtures ───────────────────────────────────────────────────────────

fn make_knowledge_entry() -> KnowledgeEntry {
    KnowledgeEntry {
        body: KnowledgeEntryBody::default(),
        canonical_name: KnowledgeEntryCanonicalName::try_from("Mira Vale".to_owned())
            .expect("non-empty canonical name"),
        created_at: None,
        entry_id: "kb_1".into(),
        entry_type: "character".into(),
        extensions: HashMap::new(),
        modules: HashMap::new(),
        revision: None,
        schema_version: NonZeroU64::new(1).expect("1 is non-zero"),
        source_anchor: None,
        status: "provisional".into(),
        updated_at: None,
    }
}

fn make_finding() -> Finding {
    serde_json::from_value(json!({
        "description": "Description",
        "extensions": {},
        "finding_id": "fnd_1",
        "schema_version": 1,
        "severity": "warning",
        "status": "open",
        "text_position": {},
        "title": "Title"
    }))
    .expect("valid Finding wire JSON")
}

fn make_promote_request() -> PromoteRequest {
    serde_json::from_value(json!({
        "candidate": {
            "schema_version": 1,
            "entry_id": "kb_1",
            "entry_type": "character",
            "canonical_name": "Mira Vale",
            "status": "provisional",
            "body": {},
            "extensions": {}
        },
        "extensions": {}
    }))
    .expect("valid PromoteRequest wire JSON")
}

use std::collections::HashMap;

// ── pure pass-through wrappers: wrapper == underlying spoke function ────

#[test]
fn validate_promote_matches_spoke_result() {
    let request = make_promote_request();
    let via_wrapper = validate_promote(&request);
    let direct = validate_promote_request(&request);
    assert_eq!(
        result_kind(&via_wrapper),
        result_kind(&direct),
        "wrapper must return the same verdict as spoke_operations"
    );
    assert!(via_wrapper.is_ok(), "fixture should be a valid promote");
}

#[test]
fn validate_promote_propagates_reject_unchanged() {
    // Build a request that fails validation: non-provisional candidate.
    let request: PromoteRequest = serde_json::from_value(json!({
        "candidate": {
            "schema_version": 1,
            "entry_id": "kb_1",
            "entry_type": "character",
            "canonical_name": "Mira Vale",
            "status": "confirmed",
            "body": {},
            "extensions": {}
        },
        "extensions": {}
    }))
    .expect("valid PromoteRequest wire JSON");

    let via_wrapper = validate_promote(&request);
    let direct = validate_promote_request(&request);
    assert_eq!(
        via_wrapper, direct,
        "reject payload must be forwarded verbatim"
    );
}

#[test]
fn apply_promote_matches_spoke_result() {
    let request = make_promote_request();
    let via_wrapper = apply_promote(&request);
    let direct = apply_promote_acceptance(&request);
    // KnowledgeEntry (typify-generated) does not derive PartialEq, so compare
    // the observable promotion result: both Ok, and the promoted entry has the
    // same status / revision / entry_id in both paths.
    assert_eq!(result_kind(&via_wrapper), result_kind(&direct));
    assert!(via_wrapper.is_ok(), "fixture should be a valid promote");
    match (via_wrapper, direct) {
        (SpokeResult::Ok(w), SpokeResult::Ok(d)) => {
            assert_eq!(w.status, d.status);
            assert_eq!(w.revision, d.revision);
            assert_eq!(w.entry_id, d.entry_id);
            assert_eq!(w.status, "confirmed");
            assert_eq!(w.revision, Some(1));
        }
        (w, d) => panic!("expected both Ok; wrapper={w:?} direct={d:?}"),
    }
}

#[test]
fn transition_status_matches_spoke_result() {
    let entry = make_knowledge_entry();
    let via_wrapper = transition_status(&entry, "confirmed");
    let direct = transition_knowledge_entry_status(&entry, "confirmed");
    // Compare observable fields (KnowledgeEntry is not PartialEq).
    assert_eq!(result_kind(&via_wrapper), result_kind(&direct));
    match (via_wrapper, direct) {
        (SpokeResult::Ok(w), SpokeResult::Ok(d)) => {
            assert_eq!(w.status, d.status);
            assert_eq!(w.entry_id, d.entry_id);
            assert_eq!(w.status, "confirmed");
            // Input not mutated by either path.
            assert_eq!(entry.status, "provisional");
        }
        (w, d) => panic!("expected both Ok; wrapper={w:?} direct={d:?}"),
    }
}

#[test]
fn transition_finding_status_matches_spoke_result() {
    let finding = make_finding();
    let via_wrapper = transition_finding_status(&finding, "resolved");
    let direct = spoke_transition_finding_status(&finding, "resolved");
    // updated_at is set to Utc::now() inside spoke; compare status only to
    // avoid a flaky timestamp equality check.
    assert_eq!(result_kind(&via_wrapper), result_kind(&direct));
    assert!(via_wrapper.is_ok());
    if let SpokeResult::Ok(updated) = via_wrapper {
        assert_eq!(updated.status, "resolved");
        assert!(updated.updated_at.is_some());
    }
}

#[test]
fn merge_extensions_matches_spoke_result() {
    let mut base = ExtensionMap::new();
    base.insert(
        "nexus".into(),
        Map::from_iter([("world_id".into(), json!("w1"))]),
    );
    let mut overlay = ExtensionMap::new();
    overlay.insert(
        "nexus".into(),
        Map::from_iter([("editor".into(), json!("v2"))]),
    );

    let via_wrapper = merge_extensions(&base, &overlay);
    let direct = merge_extension_maps(&base, &overlay);
    assert_eq!(via_wrapper, direct, "merge result must be identical");
    assert_eq!(
        via_wrapper.get("nexus").and_then(|m| m.get("world_id")),
        Some(&json!("w1"))
    );
    assert_eq!(
        via_wrapper.get("nexus").and_then(|m| m.get("editor")),
        Some(&json!("v2"))
    );
}

#[test]
fn assert_revision_matches_spoke_result() {
    // Equal case → Ok.
    assert_eq!(assert_revision(3, 3), assert_revision_match(3, 3));
    assert!(assert_revision(3, 3).is_ok());

    // Stale case → StoredRevisionStale reject.
    assert_eq!(assert_revision(2, 5), assert_revision_match(2, 5));
    assert!(assert_revision(2, 5).is_reject());

    // Conflict case → RevisionConflict reject.
    assert_eq!(assert_revision(5, 2), assert_revision_match(5, 2));
    assert!(assert_revision(5, 2).is_reject());
}

// ── build_assemble_packet: behavior parity with the equivalent direct call ──

#[test]
fn build_assemble_packet_produces_equivalent_packet() {
    let entries = vec![
        KnowledgeEntry {
            entry_id: "kb_a".into(),
            canonical_name: KnowledgeEntryCanonicalName::try_from("A".to_owned()).unwrap(),
            ..make_knowledge_entry()
        },
        KnowledgeEntry {
            entry_id: "kb_b".into(),
            canonical_name: KnowledgeEntryCanonicalName::try_from("B".to_owned()).unwrap(),
            ..make_knowledge_entry()
        },
        KnowledgeEntry {
            entry_id: "kb_c".into(),
            canonical_name: KnowledgeEntryCanonicalName::try_from("C".to_owned()).unwrap(),
            ..make_knowledge_entry()
        },
    ];

    // Wrapper: §7.2 signature.
    let via_wrapper = build_assemble_packet("pkt_wrap", &entries, Some(2));
    assert!(via_wrapper.is_ok(), "wrapper should build a packet");

    // Equivalent direct call: same entries wrapped, same truncation.
    let wrapped: Vec<_> = entries
        .iter()
        .cloned()
        .map(KnowledgeEntryForAssemble::from_entry)
        .collect();
    let direct = spoke_build_assemble_packet(BuildAssemblePacketInput {
        packet_id: "pkt_wrap",
        knowledge_entries: &wrapped,
        extensions: None,
        max_entries: Some(2),
    });

    match (via_wrapper, direct) {
        (SpokeResult::Ok(w), SpokeResult::Ok(d)) => {
            assert_eq!(w.packet_id, d.packet_id);
            assert_eq!(w.schema_version, d.schema_version);
            assert_eq!(w.entries.len(), d.entries.len());
            assert_eq!(w.entries.len(), 2, "truncation honored");
            assert_eq!(w.entries[0].entry_id, "kb_a");
            assert_eq!(w.entries[1].entry_id, "kb_b");
            assert_eq!(w.extensions, d.extensions);
        }
        (w, d) => panic!("expected both Ok; wrapper={w:?} direct={d:?}"),
    }
}

#[test]
fn build_assemble_packet_rejects_empty_packet_id() {
    let via_wrapper = build_assemble_packet("   ", &[], None);
    let direct = spoke_build_assemble_packet(BuildAssemblePacketInput {
        packet_id: "   ",
        knowledge_entries: &[],
        extensions: None,
        max_entries: None,
    });
    assert_eq!(
        result_kind(&via_wrapper),
        result_kind(&direct),
        "reject path must match spoke"
    );
    assert!(via_wrapper.is_reject());
}

// ── helpers ────────────────────────────────────────────────────────────

/// Reduce a `SpokeResult` to a comparable ok/reject tag without requiring `T:
/// PartialEq` (used where timestamp fields make full equality flaky).
const fn result_kind<T>(result: &SpokeResult<T>) -> &'static str {
    match result {
        SpokeResult::Ok(_) => "ok",
        SpokeResult::Reject(_) => "reject",
    }
}
