//! `MindState` store CRUD + `validate_mind_state` gate tests (V1.164 P2 T2).
//!
//! Covers: insert → read-back verbatim; when-axis list ordering; delete;
//! wire-shape rejection (missing required field / unknown envelope key);
//! FK integrity (unknown holder rejected; holder delete cascades).

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::{KbStore, WorldKbEntry};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::mind_state_store::{
    delete_mind_state, get_mind_state, insert_mind_state, list_mind_states_by_holder,
};
use nexus_local_db::{open_pool, run_migrations, LocalDbError};
use serde_json::json;

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
    let kb = WorldKbEntry::new("wld_mind_test", BlockType::Character, "Bo");
    let holder_id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    holder_id
}

/// A valid `MindState` wire envelope (all required + optional when-axis fields).
fn valid_envelope(holder_entry_id: &str) -> serde_json::Value {
    json!({
        "schema_version": 1,
        "mind_state_id": "ms_001",
        "holder_entry_id": holder_entry_id,
        "canonical_name": "Bo at the transfer",
        "occurred_at": "2026-08-14T10:00:00Z",
        "sort_key": "0001",
        "snapshot": {
            "attention": "the box",
            "emotions": [{ "emotion": "hope", "intensity": 0.6 }]
        },
        "deltas": [{ "path": "attention", "previous": null, "next": "the box" }],
        "source_anchor": { "event_id": "evt_transfer" },
        "created_at": "2026-08-14T10:00:00Z",
        "updated_at": "2026-08-14T10:00:00Z",
        "extensions": { "nexus": { "note": "derivative snapshot" } }
    })
}

// ── insert + get (verbatim round-trip) ─────────────────────────

#[tokio::test]
async fn insert_valid_mind_state_roundtrips_verbatim() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    let envelope = valid_envelope(&holder_id);
    insert_mind_state(&pool, &envelope).await.unwrap();

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
    assert_eq!(
        row.snapshot_json.as_deref(),
        Some(envelope["snapshot"].to_string().as_str())
    );
    assert_eq!(
        row.deltas_json.as_deref(),
        Some(envelope["deltas"].to_string().as_str())
    );
    assert_eq!(
        row.source_anchor_json.as_deref(),
        Some(envelope["source_anchor"].to_string().as_str())
    );
    assert_eq!(
        row.extensions_json.as_deref(),
        Some(envelope["extensions"].to_string().as_str())
    );
    assert_eq!(row.created_at, "2026-08-14T10:00:00Z");
    assert_eq!(row.updated_at, "2026-08-14T10:00:00Z");

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

    let mk = |id: &str, occurred_at: Option<&str>, sort_key: Option<&str>| {
        let mut v = json!({
            "schema_version": 1,
            "mind_state_id": id,
            "holder_entry_id": holder_id,
            "extensions": {}
        });
        if let Some(ts) = occurred_at {
            v.as_object_mut()
                .unwrap()
                .insert("occurred_at".to_string(), json!(ts));
        }
        if let Some(sk) = sort_key {
            v.as_object_mut()
                .unwrap()
                .insert("sort_key".to_string(), json!(sk));
        }
        v
    };
    // Same occurred_at → sort_key tiebreak (reverse key order on purpose).
    insert_mind_state(
        &pool,
        &mk("ms_late", Some("2026-08-14T12:00:00Z"), Some("0003")),
    )
    .await
    .unwrap();
    insert_mind_state(
        &pool,
        &mk("ms_early", Some("2026-08-14T09:00:00Z"), Some("0001")),
    )
    .await
    .unwrap();
    insert_mind_state(
        &pool,
        &mk("ms_same", Some("2026-08-14T12:00:00Z"), Some("0002")),
    )
    .await
    .unwrap();
    // No occurred_at → NULL sorts first within the holder.
    insert_mind_state(&pool, &mk("ms_null", None, Some("0099")))
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

    let envelope = valid_envelope(&holder_id);
    insert_mind_state(&pool, &envelope).await.unwrap();

    assert!(delete_mind_state(&pool, "ms_001").await.unwrap());
    assert!(get_mind_state(&pool, "ms_001").await.unwrap().is_none());
    // Second delete of the same id reports no row matched.
    assert!(!delete_mind_state(&pool, "ms_001").await.unwrap());
}

// ── validate_mind_state gate ────────────────────────────────────

#[tokio::test]
async fn insert_missing_required_field_is_rejected_and_nothing_stored() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    let mut envelope = valid_envelope(&holder_id);
    envelope.as_object_mut().unwrap().remove("holder_entry_id");

    let err = insert_mind_state(&pool, &envelope).await.unwrap_err();
    match err {
        LocalDbError::ValidationError(msg) => {
            assert!(
                msg.contains("holder_entry_id"),
                "rejection must name the missing field: {msg}"
            );
        }
        other => panic!("expected ValidationError, got {other:?}"),
    }
    assert!(
        get_mind_state(&pool, "ms_001").await.unwrap().is_none(),
        "rejected envelope must not persist"
    );
}

#[tokio::test]
async fn insert_unknown_envelope_key_is_rejected_by_closed_envelope() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    let mut envelope = valid_envelope(&holder_id);
    envelope
        .as_object_mut()
        .unwrap()
        .insert("bogus_key".to_string(), json!(1));

    let err = insert_mind_state(&pool, &envelope).await.unwrap_err();
    match err {
        LocalDbError::ValidationError(msg) => {
            assert!(
                msg.contains("unknown property") || msg.contains("bogus_key"),
                "rejection must flag the unknown key: {msg}"
            );
        }
        other => panic!("expected ValidationError, got {other:?}"),
    }
}

// ── FK integrity (DDL) ──────────────────────────────────────────

#[tokio::test]
async fn insert_with_unknown_holder_fails_foreign_key() {
    let (pool, _dir) = setup_db().await;
    let envelope = valid_envelope("kb_no_such_holder");
    let err = insert_mind_state(&pool, &envelope).await.unwrap_err();
    assert!(
        matches!(err, LocalDbError::Sqlx(_)),
        "unknown holder must surface as a DB (FK) error, got {err:?}"
    );
}

#[tokio::test]
async fn deleting_holder_cascades_mind_states() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    insert_mind_state(&pool, &valid_envelope(&holder_id))
        .await
        .unwrap();
    let second = json!({
        "schema_version": 1,
        "mind_state_id": "ms_002",
        "holder_entry_id": holder_id,
        "occurred_at": "2026-08-14T11:00:00Z",
        "extensions": {}
    });
    insert_mind_state(&pool, &second).await.unwrap();

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
