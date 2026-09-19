//! Hermetic DAO test for the V1.50 T-B P2 refreshable-scan upsert/cleanup
//! primitives in `nexus_local_db::kb_extract_job`.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-kb-refreshable-scan.md` §5 T1/T5
//! Spec: `.mstar/specs/entity-scope-model.md` §5.5
//!
//! Covers:
//! - **Idempotency**: `upsert_pending_candidate` Inserts on first call,
//!   returns `Unchanged` on an identical re-call (AC1: re-run on unchanged
//!   text produces no diff), and `Updated` when the payload changes.
//! - **Composite key**: the DB uniqueness is `(creator, work_entry_id=name,
//!   world)` (V1.50 P1), so a second chapter's rescan of the same name reuses
//!   the existing row and refreshes `source_chapter_id` rather than duplicating.
//! - **Confirmed is terminal**: a confirmed row is never mutated by upsert
//!   (returns `Unchanged`).
//! - **Stale cleanup**: `delete_pending_for_chapter` removes only pending rows;
//!   confirmed/rejected are left intact.
//!
//! Run with: cargo test -p nexus-local-db --test `kb_extract_jobs_upsert`

#![allow(clippy::unwrap_used)]

use nexus_local_db::kb_extract_job::{
    delete_pending_for_chapter, insert_pending, list_for_chapter, mark_confirmed,
    upsert_pending_candidate, UpsertOutcome,
};

const CREATOR: &str = "ctr_1";
const WORLD: &str = "wld_1";
const WORK: &str = "wrk_1";

async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let guarded = nexus_local_db::init_engine_pool(&db_path).await.unwrap();
    (guarded.clone_pool(), dir)
}

fn payload(summary: &str, name: &str) -> String {
    serde_json::json!({
        "summary": summary,
        "attributes": {"novel_category": "character", "aliases": [name]},
        "tags": ["novel"],
    })
    .to_string()
}

// ── AC1: idempotent re-run produces no diff ──────────────────────────────

#[tokio::test]
async fn upsert_inserts_then_is_unchanged_on_identical_recall() {
    let (pool, _dir) = fresh_pool().await;
    let p = payload("v1", "Lin Xia");

    let first = upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Lin Xia",
        &p,
    )
    .await
    .unwrap();
    assert!(matches!(first, UpsertOutcome::Inserted(_)));

    // Identical re-call → Unchanged (no diff).
    let second = upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Lin Xia",
        &p,
    )
    .await
    .unwrap();
    assert!(matches!(second, UpsertOutcome::Unchanged(_)));
}

#[tokio::test]
async fn upsert_reports_updated_when_payload_changes() {
    let (pool, _dir) = fresh_pool().await;

    upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Lin Xia",
        &payload("v1", "Lin Xia"),
    )
    .await
    .unwrap();

    let outcome = upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Lin Xia",
        &payload("v2-edited", "Lin Xia"),
    )
    .await
    .unwrap();
    assert!(matches!(outcome, UpsertOutcome::Updated(_)));

    // The pending row now carries the refreshed payload.
    let expected = payload("v2-edited", "Lin Xia");
    let rows = list_for_chapter(&pool, WORK, 1).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].proposed_payload.as_deref(), Some(expected.as_str()));
}

// ── Composite key: one row per (creator, world, name) ────────────────────

#[tokio::test]
async fn upsert_never_duplicates_across_chapters_for_same_name() {
    let (pool, _dir) = fresh_pool().await;

    // Chapter 3 extraction of "Lin Xia".
    upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(3),
        "character",
        "Lin Xia",
        &payload("ch3", "Lin Xia"),
    )
    .await
    .unwrap();

    // Chapter 5 extraction of the SAME name reuses the existing row (DB
    // uniqueness on (creator, work_entry_id=name, world)) and refreshes its
    // source_chapter_id + payload instead of duplicating.
    let outcome = upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(5),
        "character",
        "Lin Xia",
        &payload("ch5", "Lin Xia"),
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, UpsertOutcome::Updated(_)),
        "cross-chapter same-name must reuse row (Updated), got {outcome:?}"
    );

    // Exactly one row exists for this name — no duplicate.
    let pending = nexus_local_db::kb_extract_job::list_pending_for_world(&pool, WORLD, None)
        .await
        .unwrap();
    let lin_xia: Vec<_> = pending
        .iter()
        .filter(|r| r.canonical_name_guess.as_deref() == Some("Lin Xia"))
        .collect();
    assert_eq!(
        lin_xia.len(),
        1,
        "no duplicate rows for same name across chapters"
    );

    // source_chapter_id was refreshed to the latest rescan (5).
    assert_eq!(lin_xia[0].source_chapter_id, Some(5));
}

#[tokio::test]
async fn upsert_distinct_names_get_distinct_rows() {
    let (pool, _dir) = fresh_pool().await;
    upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Lin Xia",
        &payload("a", "Lin Xia"),
    )
    .await
    .unwrap();
    upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Marcus Vale",
        &payload("b", "Marcus Vale"),
    )
    .await
    .unwrap();

    let rows = list_for_chapter(&pool, WORK, 1).await.unwrap();
    assert_eq!(rows.len(), 2);
}

// ── Confirmed is terminal ─────────────────────────────────────────────────

#[tokio::test]
async fn upsert_leaves_confirmed_row_unchanged() {
    let (pool, _dir) = fresh_pool().await;
    // Seed a pending candidate via insert_pending, then confirm it.
    let candidate = insert_pending(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Aria",
        &payload("orig", "Aria"),
    )
    .await
    .unwrap();
    mark_confirmed(&pool, &candidate.job_id).await.unwrap();

    // Rescan attempts to upsert a CHANGED payload for the now-confirmed name.
    let outcome = upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Aria",
        &payload("edited", "Aria"),
    )
    .await
    .unwrap();
    assert!(
        matches!(outcome, UpsertOutcome::Unchanged(_)),
        "confirmed rows are terminal — upsert must not mutate them"
    );

    // The confirmed row retains its original payload.
    let expected_orig = payload("orig", "Aria");
    let row = nexus_local_db::kb_extract_job::get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.promotion_status, "confirmed");
    assert_eq!(
        row.proposed_payload.as_deref(),
        Some(expected_orig.as_str())
    );
}

// ── Stale cleanup ─────────────────────────────────────────────────────────

#[tokio::test]
async fn delete_pending_for_chapter_removes_only_pending() {
    let (pool, _dir) = fresh_pool().await;

    // Two pending candidates sourced from chapter 1.
    let a = insert_pending(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Keep",
        &payload("k", "Keep"),
    )
    .await
    .unwrap();
    let b = insert_pending(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Gone",
        &payload("g", "Gone"),
    )
    .await
    .unwrap();
    // Confirm "Keep" so it becomes terminal.
    mark_confirmed(&pool, &a.job_id).await.unwrap();

    // Stale cleanup targets "Gone" (pending) and "Keep" (confirmed) for chapter 1.
    let deleted_gone = delete_pending_for_chapter(&pool, WORK, 1, "Gone")
        .await
        .unwrap();
    assert!(deleted_gone, "pending 'Gone' should be deleted");

    // Confirmed "Keep" is NOT deleted by the pending-only cleanup.
    let deleted_keep = delete_pending_for_chapter(&pool, WORK, 1, "Keep")
        .await
        .unwrap();
    assert!(
        !deleted_keep,
        "confirmed 'Keep' must NOT be deleted by stale cleanup"
    );

    // "Gone" is gone; "Keep" is retained as confirmed.
    let gone = nexus_local_db::kb_extract_job::get_promotion(&pool, &b.job_id)
        .await
        .unwrap();
    assert!(gone.is_none(), "pending 'Gone' row must be removed");

    let keep = nexus_local_db::kb_extract_job::get_promotion(&pool, &a.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(keep.promotion_status, "confirmed");
}

// ═══════════════════════════════════════════════════════════════════════════
// v1.191 P1 T13 — the atomic job result the extraction callers depend on
// ═══════════════════════════════════════════════════════════════════════════

/// A rescan of the same extraction run reuses the candidate row and the
/// relationship suggestion: both ids are retained, and neither is duplicated.
/// This is the job-result identity the review-time caller persists into.
#[tokio::test]
async fn v1191_extract_candidate_and_relationship_ids_survive_a_rescan() {
    let (pool, _dir) = fresh_pool().await;
    nexus_local_db::kb_store::seed::world(&pool, WORLD, CREATOR, "T13", "t13", "private", "manual")
        .await;
    for (id, name) in [("kb_t13_aria", "Aria"), ("kb_t13_kael", "Kael")] {
        nexus_local_db::kb_store::seed::knowledge_entry(
            &pool,
            id,
            WORLD,
            "character",
            name,
            "confirmed",
        )
        .await;
    }
    let body = payload("A warrior", "Lin Xia");

    // ── First extraction pass ───────────────────────────────────────────────
    let first = upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Lin Xia",
        &body,
    )
    .await
    .unwrap();
    let candidate_id = match first {
        UpsertOutcome::Inserted(job_id) => job_id,
        other => panic!("expected Inserted, got {other:?}"),
    };

    let now = "2026-09-18T00:00:00Z";
    let inserted = nexus_local_db::kb_relationships::upsert_extraction_relationship(
        &pool,
        WORLD,
        "kb_t13_aria",
        "kb_t13_kael",
        "allied_with",
        None,
        true,
        Some(0.7),
        Some("Aria and Kael fought together"),
        now,
    )
    .await
    .unwrap();
    assert!(inserted, "the first suggestion is inserted");
    let before =
        nexus_local_db::kb_relationships::list_relationships_for_world(&pool, WORLD, true, 10)
            .await
            .unwrap();
    assert_eq!(before.len(), 1);
    let relationship_id = before[0].relationship_id.clone();

    // ── Rescan of the same run ──────────────────────────────────────────────
    let second = upsert_pending_candidate(
        &pool,
        CREATOR,
        "ws",
        WORLD,
        Some(WORK),
        Some(1),
        "character",
        "Lin Xia",
        &body,
    )
    .await
    .unwrap();
    let retained_id = match second {
        UpsertOutcome::Unchanged(job_id) => job_id,
        other => panic!("expected Unchanged on rescan, got {other:?}"),
    };
    assert_eq!(retained_id, candidate_id, "candidate id is retained");

    let reinserted = nexus_local_db::kb_relationships::upsert_extraction_relationship(
        &pool,
        WORLD,
        "kb_t13_aria",
        "kb_t13_kael",
        "allied_with",
        None,
        true,
        Some(0.7),
        Some("Aria and Kael fought together"),
        now,
    )
    .await
    .unwrap();
    assert!(!reinserted, "a rescan does not re-insert the suggestion");
    let after =
        nexus_local_db::kb_relationships::list_relationships_for_world(&pool, WORLD, true, 10)
            .await
            .unwrap();
    assert_eq!(after.len(), 1, "no duplicate suggestion row");
    assert_eq!(
        after[0].relationship_id, relationship_id,
        "relationship id is retained"
    );
    assert_eq!(after[0].source, "extraction");
    assert_eq!(
        after[0].needs_review, 1,
        "an extraction suggestion stays hidden"
    );
}

/// A failed relationship write persists nothing: an unresolved endpoint cannot
/// leave a half-written suggestion behind, which is why the caller resolves both
/// endpoints before it writes anything.
#[tokio::test]
async fn v1191_extract_unresolved_endpoint_leaves_zero_relationship_rows() {
    let (pool, _dir) = fresh_pool().await;
    nexus_local_db::kb_store::seed::world(&pool, WORLD, CREATOR, "T13", "t13", "private", "manual")
        .await;
    nexus_local_db::kb_store::seed::knowledge_entry(
        &pool,
        "kb_t13_aria",
        WORLD,
        "character",
        "Aria",
        "confirmed",
    )
    .await;

    // The endpoint resolution the caller performs: an unknown name resolves to
    // nothing, so the caller skips instead of writing a foreign endpoint.
    let unknown = nexus_local_db::kb_relationships::resolve_entity_by_canonical_name(
        &pool,
        WORLD,
        "Nobody",
        Some("character"),
    )
    .await
    .unwrap();
    assert!(unknown.is_none(), "an unknown endpoint resolves to nothing");
    let known = nexus_local_db::kb_relationships::resolve_entity_by_canonical_name(
        &pool,
        WORLD,
        "Aria",
        Some("character"),
    )
    .await
    .unwrap();
    assert_eq!(known.as_deref(), Some("kb_t13_aria"));

    // Skipping is what keeps the write honest: a forced write with a
    // non-existent endpoint is refused by the FK and leaves zero rows.
    let forced = nexus_local_db::kb_relationships::upsert_extraction_relationship(
        &pool,
        WORLD,
        "kb_t13_aria",
        "kb_t13_missing",
        "allied_with",
        None,
        true,
        None,
        None,
        "2026-09-18T00:00:00Z",
    )
    .await;
    assert!(forced.is_err(), "an unresolved endpoint cannot be written");

    let rows =
        nexus_local_db::kb_relationships::list_relationships_for_world(&pool, WORLD, true, 10)
            .await
            .unwrap();
    assert!(rows.is_empty(), "zero relationship rows after the failure");
    let pending = list_for_chapter(&pool, WORK, 1).await.unwrap();
    assert!(pending.is_empty(), "zero candidate rows after the failure");
}
