//! `MindState` store CRUD tests (V1.164 P2 T2 fix) — pure storage, no
//! validation. The store persists raw column values; the spoke
//! `validate_mind_state` gate (and its rejection tests) lives at the
//! adapter boundary — see
//! `crates/nexus-spoke-adapter/tests/mind_state_gate.rs`.
//!
//! Covers: insert → read-back verbatim; when-axis list ordering; delete;
//! FK integrity (unknown holder rejected; holder delete cascades).

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::{KbStore, KnowledgeEntryRecord};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::mind_state_store::{
    delete_mind_state, get_mind_state, insert_mind_state, list_mind_states_by_holder,
};
use nexus_local_db::{open_pool, run_migrations, LocalDbError};

async fn setup_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (pool, dir)
}

/// Seed creator + world + a holder `kb_key_blocks` row (FK target for
/// `mind_states.holder_entry_id`), returning the holder entry id.
async fn seed_holder(pool: &sqlx::SqlitePool) -> String {
    // SAFETY: test-only fixture scaffolding — inserts match the creators /
    // narrative_worlds / kb_key_blocks DDL.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_mind_test', 'Mind Test', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json) \
         VALUES ('wld_mind_test', 'wrk_mind_test', 'ctr_mind_test', 'Mind Test World', \
                 'mind-test-world', 'active', 'private', 'manual', '{}')",
    )
    .execute(pool)
    .await
    .unwrap();

    let store = SqliteKbStore::new(pool.clone());
    let kb = KnowledgeEntryRecord::new("wld_mind_test", BlockType::Character, "Bo");
    let holder_id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    holder_id
}

/// Raw column values for the store — the full valid row used by the
/// verbatim round-trip (mirrors the spoke `mind-state.schema.json` keys;
/// `snapshot` / `deltas` / `source_anchor` / `extensions` arrive as JSON
/// strings).
const VALID_SNAPSHOT_JSON: &str =
    r#"{"attention":"the box","emotions":[{"emotion":"hope","intensity":0.6}]}"#;
const VALID_DELTAS_JSON: &str = r#"[{"path":"attention","previous":null,"next":"the box"}]"#;
const VALID_SOURCE_ANCHOR_JSON: &str = r#"{"event_id":"evt_transfer"}"#;
const VALID_EXTENSIONS_JSON: &str = r#"{"nexus":{"note":"derivative snapshot"}}"#;

// ── insert + get (verbatim round-trip) ─────────────────────────

#[tokio::test]
async fn insert_valid_mind_state_roundtrips_verbatim() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    insert_mind_state(
        &pool,
        "ms_001",
        1,
        &holder_id,
        Some("Bo at the transfer"),
        Some("2026-08-14T10:00:00Z"),
        Some("0001"),
        Some(VALID_SNAPSHOT_JSON),
        Some(VALID_DELTAS_JSON),
        Some(VALID_SOURCE_ANCHOR_JSON),
        Some(VALID_EXTENSIONS_JSON),
    )
    .await
    .unwrap();

    let row = get_mind_state(&pool, "ms_001")
        .await
        .unwrap()
        .expect("row must exist");
    assert_eq!(row.mind_state_id, "ms_001");
    assert_eq!(row.schema_version, 1);
    assert_eq!(row.holder_entry_id, holder_id);
    assert_eq!(row.canonical_name.as_deref(), Some("Bo at the transfer"));
    assert_eq!(row.occurred_at.as_deref(), Some("2026-08-14T10:00:00Z"));
    assert_eq!(row.sort_key.as_deref(), Some("0001"));
    assert_eq!(row.snapshot_json.as_deref(), Some(VALID_SNAPSHOT_JSON));
    assert_eq!(row.deltas_json.as_deref(), Some(VALID_DELTAS_JSON));
    assert_eq!(
        row.source_anchor_json.as_deref(),
        Some(VALID_SOURCE_ANCHOR_JSON)
    );
    assert_eq!(row.extensions_json.as_deref(), Some(VALID_EXTENSIONS_JSON));
    // created_at / updated_at are store-stamped at insert (RFC 3339).
    assert!(!row.created_at.is_empty());
    assert!(!row.updated_at.is_empty());

    // Exactly one row persisted (no duplicate envelope expansion).
    // SAFETY: test-only — row-count verification against the mind_states DDL.
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM mind_states WHERE mind_state_id = ?")
            .bind("ms_001")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn get_missing_mind_state_returns_none() {
    let (pool, _dir) = setup_db().await;
    assert!(get_mind_state(&pool, "ms_nope").await.unwrap().is_none());
}

// ── list (when-axis ordering) ───────────────────────────────────

#[tokio::test]
async fn list_mind_states_by_holder_orders_by_occurred_at_then_sort_key() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    // Same occurred_at → sort_key tiebreak (reverse key order on purpose).
    insert_mind_state(
        &pool,
        "ms_late",
        1,
        &holder_id,
        None,
        Some("2026-08-14T12:00:00Z"),
        Some("0003"),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    insert_mind_state(
        &pool,
        "ms_early",
        1,
        &holder_id,
        None,
        Some("2026-08-14T09:00:00Z"),
        Some("0001"),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    insert_mind_state(
        &pool,
        "ms_same",
        1,
        &holder_id,
        None,
        Some("2026-08-14T12:00:00Z"),
        Some("0002"),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();
    // No occurred_at → NULL sorts first within the holder.
    insert_mind_state(
        &pool,
        "ms_null",
        1,
        &holder_id,
        None,
        None,
        Some("0099"),
        None,
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let rows = list_mind_states_by_holder(&pool, &holder_id).await.unwrap();
    let ids: Vec<&str> = rows.iter().map(|r| r.mind_state_id.as_str()).collect();
    assert_eq!(ids, vec!["ms_null", "ms_early", "ms_same", "ms_late"]);

    // Other holders are not returned.
    let other = list_mind_states_by_holder(&pool, "kb_other_holder")
        .await
        .unwrap();
    assert!(other.is_empty());
}

// ── delete ──────────────────────────────────────────────────────

#[tokio::test]
async fn delete_mind_state_removes_row_and_reports_existence() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    insert_mind_state(
        &pool,
        "ms_001",
        1,
        &holder_id,
        Some("Bo at the transfer"),
        Some("2026-08-14T10:00:00Z"),
        Some("0001"),
        None,
        None,
        None,
        Some("{}"),
    )
    .await
    .unwrap();

    assert!(delete_mind_state(&pool, "ms_001").await.unwrap());
    assert!(get_mind_state(&pool, "ms_001").await.unwrap().is_none());
    // Second delete of the same id reports no row matched.
    assert!(!delete_mind_state(&pool, "ms_001").await.unwrap());
}

// ── FK integrity (DDL) ──────────────────────────────────────────

#[tokio::test]
async fn insert_with_unknown_holder_fails_foreign_key() {
    let (pool, _dir) = setup_db().await;
    let err = insert_mind_state(
        &pool,
        "ms_001",
        1,
        "kb_no_such_holder",
        None,
        None,
        None,
        None,
        None,
        None,
        Some("{}"),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, LocalDbError::Sqlx(_)),
        "unknown holder must surface as a DB (FK) error, got {err:?}"
    );
}

#[tokio::test]
async fn deleting_holder_cascades_mind_states() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    insert_mind_state(
        &pool,
        "ms_001",
        1,
        &holder_id,
        Some("Bo at the transfer"),
        Some("2026-08-14T10:00:00Z"),
        Some("0001"),
        None,
        None,
        None,
        Some("{}"),
    )
    .await
    .unwrap();
    insert_mind_state(
        &pool,
        "ms_002",
        1,
        &holder_id,
        None,
        Some("2026-08-14T11:00:00Z"),
        None,
        None,
        None,
        None,
        Some("{}"),
    )
    .await
    .unwrap();

    // The store API soft-deletes (status='deleted'); the DDL cascade fires on
    // a hard DELETE of the holder row, so exercise the row deletion directly.
    // SAFETY: test-only — hard DELETE matching the kb_key_blocks DDL PK.
    sqlx::query("DELETE FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(&holder_id)
        .execute(&pool)
        .await
        .unwrap();

    assert!(
        list_mind_states_by_holder(&pool, &holder_id)
            .await
            .unwrap()
            .is_empty(),
        "ON DELETE CASCADE must remove the holder's mind_states rows"
    );
}
