//! V1.139 P1 T6 — Parity tests: spoke-operations delegation (T3) produces
//! identical transition outcomes to the pre-refactor nexus hand-rolled rules.
//!
//! Two complementary proof strategies:
//! 1. **Pure spoke conformance** — drive `nexus_spoke_adapter::ops::transition_status`
//!    / `assert_revision` directly across the full status cross-product, asserting
//!    the accept/reject edges match the table the architect review confirmed spoke
//!    exposes (the 6 edges nexus uses, plus `deprecated → merged` correctly
//!    excluded, plus terminal-state enforcement).
//! 2. **End-to-end routing** — exercise the `WorldKbEntry` domain methods
//!    (`confirm` / `deprecate` / `merge_into` / `delete`), which T3 routes through
//!    spoke via the conversion seam, and assert the same accept/reject + final
//!    status the prior nexus hand-rolled rules produced.
//!
//! Together these prove: the spoke cross-product table is the authority, the
//! nexus domain methods delegate to it correctly, and authors see the same
//! promote/reject/merge/delete outcomes as before the refactor.

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{ConflictCheckResult, MembershipPermissionCheck};
use nexus_knowledge::world_kb::{KbError, KnowledgeEntry, WorldKbEntry};
use nexus_spoke_adapter::ops::{assert_revision, transition_status};
use nexus_spoke_adapter::SpokeResult;

// ── Fixtures ───────────────────────────────────────────────────────────────

/// Build a `WorldKbEntry` seeded directly into `status` (bypassing the lifecycle
/// methods) so parity cases can start from any state.
fn entry_in(status: &str) -> WorldKbEntry {
    let mut e = WorldKbEntry::new("wld_test", BlockType::Character, "Test Hero");
    e.status = status.to_string();
    e
}

const fn owner() -> MembershipPermissionCheck {
    MembershipPermissionCheck {
        can_confirm_canon: true,
        can_sync_kb: true,
    }
}

const fn no_conflicts() -> ConflictCheckResult {
    ConflictCheckResult::no_conflicts()
}

/// Convert a domain entry (seeded into `from`) to the spoke type and ask spoke
/// whether the `from → to` transition is allowed.
fn spoke_allows(from: &str, to: &str) -> bool {
    let spoke: KnowledgeEntry = entry_in(from).into();
    transition_status(&spoke, to).is_ok()
}

// ── 1. Pure spoke cross-product conformance ────────────────────────────────

#[test]
fn spoke_allows_the_six_edges_nexus_uses() {
    // The 6 allowed transitions the prior nexus hand-rolled rules relied on.
    assert!(spoke_allows("provisional", "confirmed"), "provisional → confirmed");
    assert!(spoke_allows("confirmed", "deprecated"), "confirmed → deprecated");
    assert!(spoke_allows("confirmed", "merged"), "confirmed → merged");
    assert!(spoke_allows("deprecated", "deleted"), "deprecated → deleted");
    assert!(spoke_allows("confirmed", "deleted"), "confirmed → deleted");
    assert!(
        spoke_allows("deprecated", "confirmed"),
        "deprecated → confirmed (restore)"
    );
}

#[test]
fn spoke_excludes_deprecated_to_merged() {
    // Architect review §5.2: spoke's table correctly excludes this edge (a
    // deprecated entry cannot be merged directly — it must be restored first).
    assert!(
        !spoke_allows("deprecated", "merged"),
        "deprecated → merged must be rejected by spoke"
    );
}

#[test]
fn spoke_rejects_terminal_outbound_except_self_loop() {
    // merged / deleted are terminal: only self-loops are permitted.
    assert!(!spoke_allows("merged", "confirmed"));
    assert!(!spoke_allows("merged", "deleted"));
    assert!(!spoke_allows("deleted", "confirmed"));
    assert!(!spoke_allows("deleted", "deprecated"));
    // Idempotent self-loops on terminal states are accepted by spoke:
    assert!(spoke_allows("merged", "merged"));
    assert!(spoke_allows("deleted", "deleted"));
}

#[test]
fn spoke_rejects_unknown_status() {
    assert!(!spoke_allows("provisional", "frobnicated"));
    assert!(!spoke_allows("frobnicated", "confirmed"));
}

// ── 2. End-to-end routing via WorldKbEntry domain methods (T3) ──────────────

#[test]
fn confirm_routes_provisional_to_confirmed_and_bumps_revision() {
    let mut e = entry_in("provisional");
    e.confirm(&owner(), 0, &no_conflicts(), &[])
        .expect("confirm with owner + matching revision succeeds");
    assert_eq!(e.status, "confirmed");
    // T3: transition_status does not bump revision; the nexus confirm path keeps
    // the prior hand-rolled revision bump, so authors see revision 1 after promote.
    assert_eq!(e.revision, Some(1));
}

#[test]
fn deprecate_from_confirmed_routes_via_spoke() {
    let mut e = entry_in("confirmed");
    e.deprecate(None).expect("confirmed → deprecated");
    assert_eq!(e.status, "deprecated");
}

#[test]
fn merge_from_confirmed_routes_via_spoke() {
    let mut e = entry_in("confirmed");
    e.merge_into("kb_other").expect("confirmed → merged");
    assert_eq!(e.status, "merged");
}

#[test]
fn delete_from_confirmed_and_deprecated_routes_via_spoke() {
    let mut from_confirmed = entry_in("confirmed");
    from_confirmed.delete().expect("confirmed → deleted");
    assert_eq!(from_confirmed.status, "deleted");

    let mut from_deprecated = entry_in("deprecated");
    from_deprecated.delete().expect("deprecated → deleted");
    assert_eq!(from_deprecated.status, "deleted");
}

#[test]
fn already_in_state_guard_is_preserved_for_self_transition() {
    // nexus keeps the AlreadyInState guard (spoke would self-loop accept); the
    // guard preserves the pre-refactor UX of rejecting a redundant deprecate.
    let mut e = entry_in("deprecated");
    let err = e.deprecate(None).expect_err("re-deprecate rejected by nexus guard");
    assert!(matches!(err, KbError::AlreadyInState(_)), "got {err:?}");
    assert_eq!(e.status, "deprecated");
}

#[test]
fn merge_from_deprecated_rejected_by_spoke_exclusion() {
    // nexus's AlreadyInState guard passes (deprecated ≠ merged); spoke's
    // cross-product rejects deprecated → merged. Pre-refactor nexus would have
    // accepted this (it had no cross-product) — this is the documented place T3
    // is correctly stricter, and the outcome matches spoke's table.
    let mut e = entry_in("deprecated");
    let err = e.merge_into("kb_other").expect_err("deprecated → merged rejected");
    assert!(matches!(err, KbError::ValidationError(_)), "got {err:?}");
    assert_eq!(e.status, "deprecated", "status unchanged on reject");
}

#[test]
fn delete_from_terminal_rejected_by_spoke() {
    // nexus's AlreadyInState guard passes for delete-from-merged (merged ≠
    // deleted); spoke's terminal rule rejects merged → deleted.
    let mut e = entry_in("merged");
    assert!(e.delete().is_err(), "merged → deleted rejected by spoke terminal rule");
    assert_eq!(e.status, "merged", "status unchanged on reject");
}

// ── 3. Revision assertion parity (T3 gate 2) ───────────────────────────────

#[test]
fn assert_revision_matches_the_prior_equality_check() {
    // T3: confirm() gate 2 delegates to spoke's assert_revision, mapping the
    // reject back to KbError::RevisionMismatch. The spoke invariant is equality.
    assert!(matches!(assert_revision(5, 5), SpokeResult::Ok(())), "equal → ok");
    assert!(matches!(assert_revision(0, 0), SpokeResult::Ok(())), "zero/zero → ok");
    assert!(
        matches!(assert_revision(5, 4), SpokeResult::Reject(_)),
        "mismatch → reject"
    );
}

#[test]
fn confirm_with_revision_mismatch_maps_back_to_revision_mismatch() {
    // End-to-end: confirm() with a stale base_revision returns the same
    // KbError::RevisionMismatch the pre-refactor hand-rolled gate produced.
    let mut e = entry_in("provisional");
    e.revision = Some(3);
    let err = e
        .confirm(&owner(), 5, &no_conflicts(), &[])
        .expect_err("mismatched base_revision rejected");
    assert!(
        matches!(err, KbError::RevisionMismatch { expected: 5, actual: 3 }),
        "got {err:?}"
    );
}
