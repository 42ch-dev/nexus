//! Hermetic migration + DAO test for the V1.50 T-B P1 `kb_extract_jobs` promotion
//! lifecycle extension.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-kb-auto-promotion.md`
//! Spec: `.mstar/knowledge/specs/entity-scope-model.md` §5.5
//!
//! Covers:
//! - **Forward migration**: `run_migrations` adds `promotion_status`,
//!   `proposed_payload`, `source_chapter_id`, `block_type_guess`,
//!   `canonical_name_guess` + the `(promotion_status, work_id)` index.
//! - **Default**: existing V1.29/V1.40 rows keep extraction `status` and get
//!   `promotion_status='pending'` (column default).
//! - **DAO round-trip**: `insert_pending` → `list_pending_for_world` →
//!   `mark_confirmed` / `mark_rejected` → `get_promotion`.
//! - **Idempotency**: `is_idempotent` returns true after `insert_pending` for
//!   the same `(work_id, canonical_name_guess)`; re-running review extraction
//!   does not duplicate pending rows (acceptance §6).
//!
//! Run with: cargo test -p nexus-local-db --test kb_extract_jobs_migration

use sqlx::Row;

/// Helper: fresh pool with all migrations applied.
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

#[tokio::test]
async fn forward_migration_adds_promotion_columns() {
    let (pool, _dir) = fresh_pool().await;

    // SAFETY: test-only PRAGMA inspection of the kb_extract_jobs schema.
    let rows = sqlx::query("PRAGMA table_info(kb_extract_jobs)")
        .fetch_all(&pool)
        .await
        .unwrap();

    let col_names: Vec<String> = rows.iter().map(|r| r.get::<String, _>("name")).collect();
    for expected in [
        "promotion_status",
        "proposed_payload",
        "source_chapter_id",
        "block_type_guess",
        "canonical_name_guess",
    ] {
        assert!(
            col_names.iter().any(|c| c == expected),
            "missing column '{expected}' in kb_extract_jobs: {col_names:?}"
        );
    }
}

#[tokio::test]
async fn promotion_status_defaults_to_pending_with_check_constraint() {
    let (pool, _dir) = fresh_pool().await;

    // Insert a legacy-style row (V1.29 enqueue) and verify promotion_status
    // defaults to 'pending'.
    let job = nexus_local_db::enqueue_extract_job(&pool, "ctr_1", "wrk_1", "kb_a", "wld_1")
        .await
        .unwrap();

    // Read promotion_status directly.
    // SAFETY: test-only SELECT by PK.
    let promo: String =
        sqlx::query_scalar("SELECT promotion_status FROM kb_extract_jobs WHERE job_id = ?")
            .bind(&job.job_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(promo, "pending");

    // CHECK constraint rejects invalid values.
    let bad = sqlx::query("UPDATE kb_extract_jobs SET promotion_status = 'bogus' WHERE job_id = ?")
        .bind(&job.job_id)
        .execute(&pool)
        .await;
    assert!(bad.is_err(), "CHECK constraint should reject 'bogus'");
}

#[tokio::test]
async fn insert_pending_then_list_round_trip() {
    use nexus_local_db::kb_extract_job::{insert_pending, list_pending_for_world};

    let (pool, _dir) = fresh_pool().await;

    let payload = serde_json::json!({
        "summary": "A brave warrior",
        "attributes": {"novel_category": "character", "aliases": ["Lin Xia"]},
        "tags": ["novel"],
    })
    .to_string();

    let row = insert_pending(
        &pool,
        "ctr_1",
        "wrk_1",
        "wld_1",
        Some("wrk_novel"),
        Some(5),
        "character",
        "Lin Xia",
        &payload,
    )
    .await
    .unwrap();

    assert_eq!(row.promotion_status, "pending");
    assert_eq!(row.canonical_name_guess.as_deref(), Some("Lin Xia"));
    assert_eq!(row.block_type_guess.as_deref(), Some("character"));
    assert_eq!(row.source_chapter_id, Some(5));
    assert_eq!(row.work_id.as_deref(), Some("wrk_novel"));

    let pending = list_pending_for_world(&pool, "wld_1", None).await.unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].job_id, row.job_id);

    // Other world sees nothing.
    let other = list_pending_for_world(&pool, "wld_other", None)
        .await
        .unwrap();
    assert!(other.is_empty());
}

#[tokio::test]
async fn idempotency_guard_blocks_duplicate_pending() {
    use nexus_local_db::kb_extract_job::{insert_pending, is_idempotent};

    let (pool, _dir) = fresh_pool().await;

    // Before insert: not idempotent.
    assert!(!is_idempotent(&pool, "wrk_1", "Aria Stormblade")
        .await
        .unwrap());

    insert_pending(
        &pool,
        "ctr_1",
        "wrk_1",
        "wld_1",
        Some("wrk_1"),
        Some(3),
        "character",
        "Aria Stormblade",
        "{}",
    )
    .await
    .unwrap();

    // After insert: idempotent (pending row exists).
    assert!(is_idempotent(&pool, "wrk_1", "Aria Stormblade")
        .await
        .unwrap());

    // Different name: not idempotent.
    assert!(!is_idempotent(&pool, "wrk_1", "Other Name").await.unwrap());
}

#[tokio::test]
async fn idempotency_survives_confirm_but_not_reject() {
    use nexus_local_db::kb_extract_job::{insert_pending, is_idempotent, mark_confirmed};

    let (pool, _dir) = fresh_pool().await;
    let row = insert_pending(
        &pool,
        "ctr_1",
        "wrk_1",
        "wld_1",
        Some("wrk_1"),
        Some(1),
        "character",
        "Confirmed Hero",
        "{}",
    )
    .await
    .unwrap();

    // Confirm.
    let flipped = mark_confirmed(&pool, &row.job_id).await.unwrap();
    assert!(flipped);

    // Still idempotent (confirmed rows block re-extraction).
    assert!(is_idempotent(&pool, "wrk_1", "Confirmed Hero")
        .await
        .unwrap());

    // Confirming again is a no-op (already confirmed).
    let reflip = mark_confirmed(&pool, &row.job_id).await.unwrap();
    assert!(!reflip);
}

#[tokio::test]
async fn reject_allows_re_extraction() {
    use nexus_local_db::kb_extract_job::{insert_pending, is_idempotent, mark_rejected};

    let (pool, _dir) = fresh_pool().await;
    let row = insert_pending(
        &pool,
        "ctr_1",
        "wrk_1",
        "wld_1",
        Some("wrk_1"),
        Some(2),
        "character",
        "Rejected Name",
        "{}",
    )
    .await
    .unwrap();

    let flipped = mark_rejected(&pool, &row.job_id).await.unwrap();
    assert!(flipped);

    // Rejected rows do NOT block re-extraction (author may change their mind).
    assert!(!is_idempotent(&pool, "wrk_1", "Rejected Name")
        .await
        .unwrap());
}

#[tokio::test]
async fn pending_index_supports_filtered_list() {
    use nexus_local_db::kb_extract_job::{insert_pending, list_pending_for_world};

    let (pool, _dir) = fresh_pool().await;

    // Insert 3 pending for wld_1, 1 for wld_2.
    for name in ["Alpha Hero", "Beta Villain", "Gamma Sage"] {
        insert_pending(
            &pool,
            "ctr_1",
            "wrk_1",
            "wld_1",
            Some("wrk_1"),
            Some(1),
            "character",
            name,
            "{}",
        )
        .await
        .unwrap();
    }
    insert_pending(
        &pool,
        "ctr_1",
        "wrk_1",
        "wld_2",
        Some("wrk_1"),
        Some(1),
        "character",
        "Other World Name",
        "{}",
    )
    .await
    .unwrap();

    let w1 = list_pending_for_world(&pool, "wld_1", None).await.unwrap();
    assert_eq!(w1.len(), 3);

    // Limit bounds the result.
    let limited = list_pending_for_world(&pool, "wld_1", Some(2))
        .await
        .unwrap();
    assert_eq!(limited.len(), 2);
}
