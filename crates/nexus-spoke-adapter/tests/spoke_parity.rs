// note: contract — docs/specs/architecture/spoke-adapter-architecture.md.
//! V1.139 P1 T6 — Parity: spoke-operations delegation (T3) respects the
//! transition cross-product the pre-refactor nexus hand-rolled rules relied on.
//!
//! **V1.145 P1a:** relocated from `nexus-knowledge/tests/` to
//! `nexus-spoke-adapter/tests/` — the conversion seam (`knowledge_record_to_spoke`) and
//! the lifecycle trait (`KnowledgeEntryRecordSpokeExt`) now live in this crate (spec
//! §8 dep-graph reversal), so the parity tests exercise them here.
//!
//! This file pins the spoke cross-product table nexus relies on (the 6 edges
//! nexus uses, `deprecated → merged` correctly excluded, terminal-state
//! enforcement) and nexus's own `AlreadyInState` guard. The end-to-end routing
//! of the domain methods is covered by the owner tests in
//! `src/conversion/knowledge_entry.rs`.

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::{KbError, KnowledgeEntryRecord};
use nexus_spoke_adapter::conversion::{knowledge_record_to_spoke, KnowledgeEntryRecordSpokeExt};
use nexus_spoke_adapter::ops::transition_status;
use nexus_spoke_adapter::KnowledgeEntry;


/// note: Build a `KnowledgeEntryRecord` seeded directly into `status` (bypassing the lifecycle
/// methods) so parity cases can start from any state.
fn entry_in(status: &str) -> KnowledgeEntryRecord {
    let mut e = KnowledgeEntryRecord::new("wld_test", BlockType::Character, "Test Hero");
    e.status = status.to_string();
    e
}

/// note: Convert a domain entry (seeded into `from`) to the spoke type and ask spoke
/// whether the `from → to` transition is allowed.
fn spoke_allows(from: &str, to: &str) -> bool {
    let spoke: KnowledgeEntry = knowledge_record_to_spoke(&entry_in(from));
    transition_status(&spoke, to).is_ok()
}


#[test]
fn spoke_allows_the_six_edges_nexus_uses() {
    // note: The 6 allowed transitions the prior nexus hand-rolled rules relied on.
    assert!(
        spoke_allows("provisional", "confirmed"),
        "provisional → confirmed"
    );
    assert!(
        spoke_allows("confirmed", "deprecated"),
        "confirmed → deprecated"
    );
    assert!(spoke_allows("confirmed", "merged"), "confirmed → merged");
    assert!(
        spoke_allows("deprecated", "deleted"),
        "deprecated → deleted"
    );
    assert!(spoke_allows("confirmed", "deleted"), "confirmed → deleted");
    assert!(
        spoke_allows("deprecated", "confirmed"),
        "deprecated → confirmed (restore)"
    );
}

#[test]
fn spoke_excludes_deprecated_to_merged() {
    // note: Architect review §5.2: spoke's table correctly excludes this edge (a
    // deprecated entry cannot be merged directly — it must be restored first).
    assert!(
        !spoke_allows("deprecated", "merged"),
        "deprecated → merged must be rejected by spoke"
    );
}

#[test]
fn spoke_rejects_terminal_outbound_except_self_loop() {
    // note: merged / deleted are terminal: only self-loops are permitted.
    assert!(!spoke_allows("merged", "confirmed"));
    assert!(!spoke_allows("merged", "deleted"));
    assert!(!spoke_allows("deleted", "confirmed"));
    assert!(!spoke_allows("deleted", "deprecated"));
    // note: Idempotent self-loops on terminal states are accepted by spoke:
    assert!(spoke_allows("merged", "merged"));
    assert!(spoke_allows("deleted", "deleted"));
}

#[test]
fn spoke_rejects_unknown_status() {
    assert!(!spoke_allows("provisional", "frobnicated"));
    assert!(!spoke_allows("frobnicated", "confirmed"));
}


#[test]
fn already_in_state_guard_is_preserved_for_self_transition() {
    // note: nexus keeps the AlreadyInState guard (spoke would self-loop accept); the
    // guard preserves the pre-refactor UX of rejecting a redundant deprecate.
    let mut e = entry_in("deprecated");
    let err = e
        .deprecate(None)
        .expect_err("re-deprecate rejected by nexus guard");
    assert!(matches!(err, KbError::AlreadyInState(_)), "got {err:?}");
    assert_eq!(e.status, "deprecated");
}
