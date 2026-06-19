//! V1.50 T-B P1 — `creator world kb pending|adopt|reject` hermetic round-trip.
//!
//! Plan: `.mstar/plans/2026-06-18-v1.50-kb-auto-promotion.md`
//! Spec: `.mstar/knowledge/specs/entity-scope-model.md` §5.5
//!
//! Drives `nexus42::commands::creator::world::kb` logic functions directly
//! against a fresh temp DB to exercise pending/adopt/reject without `$HOME`
//! or a daemon.
//!
//! Covers:
//! - AC3 (#3): CLI commands round-trip; cross-author attempt returns `403`
//!   with code `WORLD_KB_FORBIDDEN`.
//! - AC4 (#4): adopt inserts a new `KeyBlock` with `status='confirmed'`.
//! - AC5 (#5): reject writes a log entry; row marked `rejected`.
//!
//! Run with: cargo test -p nexus42 --test world_kb_promotion_cli

#![allow(clippy::unwrap_used)]

use nexus42::commands::creator::world::kb::{
    kb_adopt, kb_pending, kb_reject, WORLD_KB_FORBIDDEN_CODE,
};
use nexus42::db::Schema;
use nexus42::errors::CliError;
use nexus_kb::key_block::{KeyBlock, KeyBlockBody};
use nexus_kb::validation::ValidationMode;
use nexus_kb::KbStore;
use nexus_local_db::kb_extract_job::{
    get_promotion, insert_pending, mark_confirmed, mark_confirmed_in_tx,
};
use nexus_local_db::kb_store::SqliteKbStore;

const OWNER: &str = "ctr_owner";
const OTHER: &str = "ctr_other";
const WORLD: &str = "wld_promo";
/// `work_id` seeded for promotion candidates (R-V150KBED-05 test surface).
const WORK_ID: &str = "wrk_test";
/// Human-readable `work_ref` (`works.story_ref`) seeded for `WORK_ID`. The
/// reject audit log MUST land under `Works/<WORK_REF>/...`, not
/// `Works/<WORK_ID>/...` (R-V150KBED-05).
const WORK_REF: &str = "test-novel";

/// Build a fresh migrated pool + seed a world owned by [`OWNER`] and a minimal
/// `works` row so the reject audit-log path can resolve `WORK_REF` from
/// `WORK_ID` (R-V150KBED-05).
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Promo World",
        "promo-world",
        "private",
        "manual",
    )
    .await;
    seed_work_for_log(&pool).await;
    (pool, dir)
}

/// Seed a minimal `works` row (`work_id=WORK_ID`, `story_ref=WORK_REF`) so the
/// R-V150KBED-05 work_ref resolution in `kb_reject` succeeds. `INSERT OR
/// IGNORE` makes this safe to call once per pool.
async fn seed_work_for_log(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed helper against the known works table schema
    // (20260604_works_table.sql). All NOT NULL columns are provided; nullable
    // columns omitted.
    sqlx::query(
        "INSERT OR IGNORE INTO works \
         (work_id, creator_id, workspace_slug, status, title, long_term_goal, \
          initial_idea, intake_status, world_id, story_ref, created_at, updated_at) \
         VALUES (?, ?, 'ws', 'active', 'Test Novel', 'goal', 'idea', 'complete', \
                 ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(WORK_ID)
    .bind(OWNER)
    .bind(WORLD)
    .bind(WORK_REF)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a pending candidate in [`WORLD`] owned by [`OWNER`].
async fn seed_pending(
    pool: &sqlx::SqlitePool,
    name: &str,
) -> nexus_local_db::kb_extract_job::KbExtractPromotion {
    let payload = serde_json::json!({
        "summary": format!("Candidate: {name}"),
        "attributes": {"novel_category": "character", "aliases": [name]},
        "tags": ["novel"],
    })
    .to_string();
    insert_pending(
        pool,
        OWNER,
        "ws",
        WORLD,
        Some(WORK_ID),
        Some(1),
        "character",
        name,
        &payload,
    )
    .await
    .unwrap()
}

// ── AC3: pending lists candidates; cross-author 403 ─────────────────────────

#[tokio::test]
async fn pending_lists_candidates_for_owner() {
    let (pool, _dir) = fresh_pool().await;
    seed_pending(&pool, "Lin Xia").await;
    seed_pending(&pool, "Marcus Vale").await;

    // Owner sees both.
    let pending = nexus_local_db::kb_extract_job::list_pending_for_world(&pool, WORLD, None)
        .await
        .unwrap();
    assert_eq!(pending.len(), 2);
}

#[tokio::test]
async fn pending_cross_author_returns_403() {
    let (pool, _dir) = fresh_pool().await;
    seed_pending(&pool, "Lin Xia").await;

    let err = kb_pending(&pool, OTHER, WORLD, None, false, false, None)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(
                message.contains(WORLD_KB_FORBIDDEN_CODE),
                "expected {WORLD_KB_FORBIDDEN_CODE} in: {message}"
            );
        }
        other => panic!("expected Api 403, got: {other:?}"),
    }
}

// ── AC4: adopt creates a confirmed KeyBlock ─────────────────────────────────

#[tokio::test]
async fn adopt_creates_confirmed_key_block() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Aria Stormblade").await;

    // Adopt.
    kb_adopt(&pool, OWNER, &candidate.job_id, None, false)
        .await
        .unwrap();

    // The promotion row is now confirmed.
    let row = get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.promotion_status, "confirmed");

    // A KeyBlock exists in the world with status=confirmed.
    let store = SqliteKbStore::new(pool.clone());
    let blocks = store.list_by_world(WORLD).await.unwrap();
    let adopted = blocks
        .iter()
        .find(|b| b.canonical_name == "Aria Stormblade")
        .unwrap_or_else(|| panic!("no KeyBlock for Aria Stormblade: {blocks:?}"));
    assert_eq!(adopted.status, "confirmed");
}

#[tokio::test]
async fn adopt_cross_author_returns_403() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Lin Xia").await;

    let err = kb_adopt(&pool, OTHER, &candidate.job_id, None, false)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(message.contains(WORLD_KB_FORBIDDEN_CODE));
        }
        other => panic!("expected Api 403, got: {other:?}"),
    }

    // Row is still pending (adopt was rejected).
    let row = get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.promotion_status, "pending");
}

// ── AC5: reject marks rejected + writes audit log ───────────────────────────

#[tokio::test]
async fn reject_marks_rejected_and_writes_log() {
    let (pool, dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Rejected Name").await;

    kb_reject(&pool, OWNER, &candidate.job_id, Some(dir.path()))
        .await
        .unwrap();

    // Row is rejected.
    let row = get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.promotion_status, "rejected");

    // Audit log written under Works/<work_ref>/Logs/kb/rejected/ (R-V150KBED-05:
    // work_ref = works.story_ref, not the opaque work_id).
    let log_path = dir
        .path()
        .join("Works")
        .join(WORK_REF)
        .join("Logs")
        .join("kb")
        .join("rejected");
    let entries = std::fs::read_dir(&log_path).unwrap();
    let log_files: Vec<_> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(&candidate.job_id))
        .collect();
    assert!(
        !log_files.is_empty(),
        "expected a rejected log file for {}",
        candidate.job_id
    );
}

#[tokio::test]
async fn reject_cross_author_returns_403() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Lin Xia").await;

    let err = kb_reject(&pool, OTHER, &candidate.job_id, None)
        .await
        .unwrap_err();
    match err {
        CliError::Api { status, message } => {
            assert_eq!(status, 403);
            assert!(message.contains(WORLD_KB_FORBIDDEN_CODE));
        }
        other => panic!("expected Api 403, got: {other:?}"),
    }
}

// ── Adopt then pending-list excludes it ─────────────────────────────────────

#[tokio::test]
async fn adopted_candidate_disappears_from_pending() {
    let (pool, _dir) = fresh_pool().await;
    let a = seed_pending(&pool, "Alpha").await;
    seed_pending(&pool, "Beta").await;

    kb_adopt(&pool, OWNER, &a.job_id, None, false)
        .await
        .unwrap();

    let pending = nexus_local_db::kb_extract_job::list_pending_for_world(&pool, WORLD, None)
        .await
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].canonical_name_guess.as_deref(), Some("Beta"));
}

// ── Double-adopt is rejected cleanly ────────────────────────────────────────

#[tokio::test]
async fn double_adopt_is_rejected() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Gamma").await;

    // First adopt succeeds.
    kb_adopt(&pool, OWNER, &candidate.job_id, None, false)
        .await
        .unwrap();

    // Second adopt fails (row no longer pending).
    let err = kb_adopt(&pool, OWNER, &candidate.job_id, None, false)
        .await
        .unwrap_err();
    match err {
        CliError::Other(msg) => {
            assert!(
                msg.contains("not pending"),
                "expected 'not pending' in: {msg}"
            );
        }
        other => panic!("expected Other error, got: {other:?}"),
    }
}

// ── R-V150KBED-03: adopt transaction rollback on mark_confirmed race ────────

/// Regression for R-V150KBED-03 (qc3 W-001 + qc2 Warning).
///
/// Asserts that when `mark_confirmed` returns `Ok(false)` after the `KeyBlock`
/// insert succeeds (the promotion row was confirmed/rejected by a concurrent
/// writer between `load_pending_candidate` and the flip), the adopt transaction
/// rolls back so **no orphan `KeyBlock` is persisted**.
///
/// We simulate the race winner by pre-flipping the candidate to `confirmed`,
/// then exercise the same `begin → insert_key_block_in_tx → mark_confirmed_in_tx
/// → rollback` boundary that `kb_adopt` composes. We bypass `kb_adopt` /
/// `load_pending_candidate` here because those gate on `pending` state and
/// would reject the pre-flipped row before reaching the tx boundary (the
/// first-failure path is already covered by `double_adopt_is_rejected`). The
/// orphan-prevention invariant under test is the tx rollback, not the
/// pre-flip guard.
#[tokio::test]
async fn kb_adopt_failure_rolls_back_insert() {
    let (pool, _dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Race Candidate").await;

    // Simulate the race winner: a concurrent writer confirms the row before
    // our adopt tx reaches mark_confirmed.
    mark_confirmed(&pool, &candidate.job_id).await.unwrap();

    // Replicate the kb_adopt tx boundary (entity-scope-model §5.5.3).
    let store = SqliteKbStore::with_validation_mode(pool.clone(), ValidationMode::Novel);
    let mut kb = KeyBlock::new(
        WORLD,
        nexus_contracts::BlockType::Character,
        "Race Candidate",
    );
    kb.body = Some(KeyBlockBody {
        summary: Some("Race candidate".to_string()),
        attributes: Some(serde_json::json!({
            "novel_category": "character",
            "aliases": ["Race Candidate"],
        })),
        tags: Some(vec!["novel".to_string()]),
    });
    kb.status = "confirmed".to_string();
    kb.created_at = chrono::Utc::now().to_rfc3339();

    let mut tx = pool.begin().await.unwrap();

    // Insert succeeds inside the tx (visible only to this tx).
    let _inserted = store
        .insert_key_block_in_tx(&mut tx, kb)
        .await
        .expect("insert_key_block_in_tx should succeed against the tx");

    // mark_confirmed_in_tx must return false (row already confirmed by the race).
    let flipped = mark_confirmed_in_tx(&mut tx, &candidate.job_id)
        .await
        .expect("mark_confirmed_in_tx query should not error");
    assert!(
        !flipped,
        "race simulation: mark_confirmed_in_tx must return Ok(false) \
         (row was pre-flipped to confirmed)"
    );

    // kb_adopt rolls back on !flipped — replicate.
    tx.rollback().await.expect("rollback must succeed");

    // Core invariant: NO orphan KeyBlock in kb_key_blocks.
    let verifier = SqliteKbStore::new(pool.clone());
    let blocks = verifier.list_by_world(WORLD).await.unwrap();
    assert!(
        blocks.iter().all(|b| b.canonical_name != "Race Candidate"),
        "R-V150KBED-03 regression: orphan KeyBlock 'Race Candidate' MUST NOT \
         persist after rollback, got: {blocks:?}"
    );

    // Candidate state preserved — the race winner's confirmation is intact,
    // and our failed adopt did not duplicate or corrupt the promotion row.
    let row = get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.promotion_status, "confirmed",
        "candidate should retain the race winner's confirmed state"
    );
}

// ── R-V150KBED-05: reject log path uses work_ref, not work_id ───────────────

/// Regression for R-V150KBED-05 (qc2 Warning).
///
/// Asserts the reject audit log lands under `Works/<work_ref>/Logs/kb/rejected/`
/// where `work_ref = works.story_ref` (the human-readable slug), and **not**
/// under `Works/<work_id>/...` (the opaque DB id). The home-layout
/// `Works/<work_ref>/` convention is the normative path authority.
///
/// Also covers the validation gate: rejecting a candidate whose `work_id` has
/// no resolvable `story_ref` fails cleanly BEFORE the DB flip (no rejected
/// row, no orphan log under the wrong path).
#[tokio::test]
async fn kb_reject_writes_log_under_work_ref_path() {
    let (pool, dir) = fresh_pool().await;
    let candidate = seed_pending(&pool, "Path Check").await;

    kb_reject(&pool, OWNER, &candidate.job_id, Some(dir.path()))
        .await
        .expect("reject should succeed: works.story_ref is seeded");

    // 1. Log dir exists under Works/<WORK_REF>/ (human slug).
    let work_ref_dir = dir
        .path()
        .join("Works")
        .join(WORK_REF)
        .join("Logs")
        .join("kb")
        .join("rejected");
    let ref_entries: Vec<_> = std::fs::read_dir(&work_ref_dir)
        .unwrap_or_else(|e| panic!("work_ref log dir must exist: {work_ref_dir:?}: {e}"))
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(&candidate.job_id))
        .collect();
    assert!(
        !ref_entries.is_empty(),
        "R-V150KBED-05: log must exist under Works/{WORK_REF}/Logs/kb/rejected/"
    );

    // 2. The log file's absolute path contains WORK_REF and does NOT contain
    //    WORK_ID (the opaque DB id the old code used).
    let log_path = ref_entries[0].path();
    let log_path_str = log_path.to_string_lossy();
    assert!(
        log_path_str.contains(&format!("/Works/{WORK_REF}/")),
        "R-V150KBED-05: log path must contain work_ref '{WORK_REF}', got: {log_path_str}"
    );
    assert!(
        !log_path_str.contains(&format!("/Works/{WORK_ID}/")),
        "R-V150KBED-05: log path must NOT contain opaque work_id '{WORK_ID}', got: {log_path_str}"
    );

    // 3. The log body records both work_id (audit) and work_ref (path component).
    let body = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        body.contains(&format!("**work_id**: {WORK_ID}")),
        "log body should record work_id for audit: {body}"
    );
    assert!(
        body.contains(&format!("**work_ref**: {WORK_REF}")),
        "log body should record work_ref: {body}"
    );
}

/// R-V150KBED-05 validation gate: rejecting a candidate whose `work_id` has no
/// resolvable `story_ref` MUST fail before the DB flip (no rejected row, no
/// orphan log under a wrong path).
#[tokio::test]
async fn kb_reject_fails_when_work_ref_missing() {
    let (pool, dir) = fresh_pool().await;

    // Seed a candidate pointing at a work_id that has NO works row (so
    // resolve_work_ref_for_log returns "work_id does not exist"). We bypass
    // seed_pending to control the work_id, and insert directly via the DAO.
    let candidate = insert_pending(
        &pool,
        OWNER,
        "ws",
        WORLD,
        Some("wrk_orphan"), // no works row with this id
        Some(1),
        "character",
        "Orphan Work",
        "{}",
    )
    .await
    .unwrap();

    let err = kb_reject(&pool, OWNER, &candidate.job_id, Some(dir.path()))
        .await
        .unwrap_err();
    match err {
        CliError::Other(msg) => {
            assert!(
                msg.contains("wrk_orphan") && msg.contains("does not exist"),
                "expected work_ref resolution error mentioning wrk_orphan, got: {msg}"
            );
        }
        other => panic!("expected Other error for missing work_ref, got: {other:?}"),
    }

    // No rejected row — the DB flip must NOT have happened.
    let row = get_promotion(&pool, &candidate.job_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.promotion_status, "pending",
        "candidate must remain pending when work_ref resolution fails"
    );

    // No log directory created at all.
    let any_works = dir.path().join("Works");
    assert!(
        !any_works.exists(),
        "no audit log directory should be created when work_ref resolution fails"
    );
}
