//! `world_findings` store CRUD tests (V1.165 P1 T1 / DR-68, AR-1) — pure
//! storage, no validation. The store persists raw column values; the spoke
//! `Finding` → `world_findings` mapping and the `extensions.nexus` routing
//! gate live at the adapter boundary (`nexus-spoke-adapter` `finding_port`,
//! AR-2).
//!
//! Covers: insert (in caller tx) → read-back verbatim incl.
//! `extensions_json` / `source_anchor_json` / `text_position_json`; list
//! ordering (`created_at DESC, finding_id ASC`); target-scoped list; FK
//! integrity (unknown world rejected; world delete cascades).

#![allow(clippy::unwrap_used)]

use nexus_local_db::world_findings::{
    get_world_finding, insert_world_finding_in_tx, list_world_findings_by_target,
    list_world_findings_by_world,
};
use nexus_local_db::{open_pool, run_migrations, LocalDbError};

async fn setup_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (pool, dir)
}

/// Seed creator + world (FK target for `world_findings.world_id`), returning
/// the world id.
async fn seed_world(pool: &sqlx::SqlitePool, world_id: &str) {
    // SAFETY: test-only fixture scaffolding — inserts match the creators /
    // narrative_worlds DDL.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_findings_test', 'Findings Test', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json) \
         VALUES (?, 'wrk_findings_test', 'ctr_findings_test', 'Findings Test World', \
                 'findings-test-world', 'active', 'private', 'manual', '{}')",
    )
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a row inside a caller-owned transaction (the batch path AR-2 uses),
/// committing on success.
async fn insert_finding(
    pool: &sqlx::SqlitePool,
    finding_id: &str,
    world_id: &str,
    severity: &str,
    status: &str,
    target_entry_id: Option<&str>,
    created_at: i64,
) {
    let mut tx = pool.begin().await.unwrap();
    insert_world_finding_in_tx(
        &mut tx,
        finding_id,
        world_id,
        1,
        severity,
        status,
        "A finding title",
        "The finding body",
        Some("dramatic_irony_asymmetry"),
        target_entry_id,
        Some(r#"{"event_id":"evt_transfer"}"#),
        Some("Consider updating the belief"),
        r#"{"paragraph":3}"#,
        r#"{"nexus":{"world_id":"wld_any","creator_id":"ctr_findings_test"}}"#,
        created_at,
        created_at,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

// ── insert + get (verbatim round-trip) ─────────────────────────

#[tokio::test]
async fn insert_world_finding_roundtrips_verbatim() {
    let (pool, _dir) = setup_db().await;
    seed_world(&pool, "wld_findings_test").await;

    let extensions_json =
        r#"{"nexus":{"world_id":"wld_findings_test","creator_id":"ctr_findings_test"}}"#;
    let source_anchor_json = r#"{"event_id":"evt_transfer","block_id":"kb_bo"}"#;
    let text_position_json = r#"{"paragraph":3,"offset":12}"#;

    let mut tx = pool.begin().await.unwrap();
    insert_world_finding_in_tx(
        &mut tx,
        "fnd_test_001",
        "wld_findings_test",
        1,
        "warning",
        "open",
        "Bo's belief drifted",
        "Bo believes the box is safe, but evt_transfer shows it is not.",
        Some("dramatic_irony_asymmetry"),
        Some("kb_bo"),
        Some(source_anchor_json),
        Some("Update Bo's belief to reflect the transfer."),
        text_position_json,
        extensions_json,
        1_752_000_000,
        1_752_000_100,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let row = get_world_finding(&pool, "fnd_test_001")
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.finding_id, "fnd_test_001");
    assert_eq!(row.world_id, "wld_findings_test");
    assert_eq!(row.schema_version, 1);
    // Spoke vocabulary persists verbatim on the world path (AR-1) — no nexus
    // mapping.
    assert_eq!(row.severity, "warning");
    assert_eq!(row.status, "open");
    assert_eq!(row.title, "Bo's belief drifted");
    assert_eq!(
        row.description,
        "Bo believes the box is safe, but evt_transfer shows it is not."
    );
    assert_eq!(row.kind.as_deref(), Some("dramatic_irony_asymmetry"));
    assert_eq!(row.target_entry_id.as_deref(), Some("kb_bo"));
    assert_eq!(row.source_anchor_json.as_deref(), Some(source_anchor_json));
    assert_eq!(
        row.suggested_fix.as_deref(),
        Some("Update Bo's belief to reflect the transfer.")
    );
    assert_eq!(row.text_position_json, text_position_json);
    assert_eq!(row.extensions_json, extensions_json);
    assert_eq!(row.created_at, 1_752_000_000);
    assert_eq!(row.updated_at, 1_752_000_100);

    // Exactly one row persisted.
    // SAFETY: test-only — row-count verification against the world_findings DDL.
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM world_findings WHERE finding_id = ?")
            .bind("fnd_test_001")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn get_missing_world_finding_returns_none() {
    let (pool, _dir) = setup_db().await;
    assert!(
        get_world_finding(&pool, "fnd_nope")
            .await
            .unwrap()
            .is_none(),
        "unknown finding_id must return None"
    );
}

// ── list (world-scoped, newest-first) ───────────────────────────

#[tokio::test]
async fn list_by_world_orders_newest_first_with_finding_id_tiebreak() {
    let (pool, _dir) = setup_db().await;
    seed_world(&pool, "wld_findings_test").await;
    seed_world(&pool, "wld_other").await;

    // Same created_at → finding_id ASC tiebreak (reversed key order on purpose).
    insert_finding(
        &pool,
        "fnd_zzz",
        "wld_findings_test",
        "info",
        "open",
        None,
        100,
    )
    .await;
    insert_finding(
        &pool,
        "fnd_aaa",
        "wld_findings_test",
        "info",
        "open",
        None,
        100,
    )
    .await;
    // Newer created_at sorts first.
    insert_finding(
        &pool,
        "fnd_newest",
        "wld_findings_test",
        "error",
        "resolved",
        None,
        300,
    )
    .await;
    insert_finding(
        &pool,
        "fnd_mid",
        "wld_findings_test",
        "warning",
        "dismissed",
        None,
        200,
    )
    .await;
    // Other worlds are not returned.
    insert_finding(&pool, "fnd_other", "wld_other", "info", "open", None, 500).await;

    let rows = list_world_findings_by_world(&pool, "wld_findings_test")
        .await
        .unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.finding_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["fnd_newest", "fnd_mid", "fnd_aaa", "fnd_zzz"],
        "expected created_at DESC, then finding_id ASC"
    );

    let other = list_world_findings_by_world(&pool, "wld_other")
        .await
        .unwrap();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].finding_id, "fnd_other");

    // World with no findings → empty list.
    let empty = list_world_findings_by_world(&pool, "wld_empty")
        .await
        .unwrap();
    assert!(empty.is_empty());
}

// ── list (target-scoped) ────────────────────────────────────────

#[tokio::test]
async fn list_by_target_filters_within_world() {
    let (pool, _dir) = setup_db().await;
    seed_world(&pool, "wld_findings_test").await;
    seed_world(&pool, "wld_other").await;

    insert_finding(
        &pool,
        "fnd_bo_1",
        "wld_findings_test",
        "warning",
        "open",
        Some("kb_bo"),
        100,
    )
    .await;
    insert_finding(
        &pool,
        "fnd_bo_2",
        "wld_findings_test",
        "info",
        "open",
        Some("kb_bo"),
        200,
    )
    .await;
    insert_finding(
        &pool,
        "fnd_ana_1",
        "wld_findings_test",
        "info",
        "open",
        Some("kb_ana"),
        150,
    )
    .await;
    insert_finding(
        &pool,
        "fnd_other_bo",
        "wld_other",
        "info",
        "open",
        Some("kb_bo"),
        300,
    )
    .await;

    let rows = list_world_findings_by_target(&pool, "wld_findings_test", "kb_bo")
        .await
        .unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.finding_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["fnd_bo_2", "fnd_bo_1"],
        "target-scoped list must be newest-first and world-scoped"
    );

    // Same target entry id in another world is not returned.
    let other = list_world_findings_by_target(&pool, "wld_other", "kb_bo")
        .await
        .unwrap();
    assert_eq!(other.len(), 1);
    assert_eq!(other[0].finding_id, "fnd_other_bo");

    // No rows for the target → empty list.
    let empty = list_world_findings_by_target(&pool, "wld_findings_test", "kb_missing")
        .await
        .unwrap();
    assert!(empty.is_empty());
}

// ── FK integrity (DDL) ──────────────────────────────────────────

#[tokio::test]
async fn insert_with_unknown_world_fails_foreign_key() {
    let (pool, _dir) = setup_db().await;
    let mut tx = pool.begin().await.unwrap();
    let err = insert_world_finding_in_tx(
        &mut tx,
        "fnd_test_001",
        "wld_no_such_world",
        1,
        "info",
        "open",
        "Title",
        "",
        None,
        None,
        None,
        None,
        "{}",
        "{}",
        1_752_000_000,
        1_752_000_000,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, LocalDbError::Sqlx(_)),
        "unknown world must surface as a DB (FK) error, got {err:?}"
    );
}

#[tokio::test]
async fn deleting_world_cascades_world_findings() {
    let (pool, _dir) = setup_db().await;
    seed_world(&pool, "wld_findings_test").await;

    insert_finding(
        &pool,
        "fnd_001",
        "wld_findings_test",
        "info",
        "open",
        None,
        100,
    )
    .await;
    insert_finding(
        &pool,
        "fnd_002",
        "wld_findings_test",
        "info",
        "open",
        None,
        200,
    )
    .await;

    // The store has no delete surface (findings dedup/retention is roadmap);
    // the DDL cascade fires on a hard DELETE of the owning world row.
    // SAFETY: test-only — hard DELETE matching the narrative_worlds DDL PK.
    sqlx::query("DELETE FROM narrative_worlds WHERE world_id = ?")
        .bind("wld_findings_test")
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        list_world_findings_by_world(&pool, "wld_findings_test")
            .await
            .unwrap()
            .is_empty(),
        "ON DELETE CASCADE must remove the world's world_findings rows"
    );
}
