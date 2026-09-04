//! `validate_and_store_mind_state` adapter-boundary gate tests (V1.164 P2
//! layering fix) — the spoke `validate_mind_state` wire-shape gate lives at
//! the adapter boundary (the sole `spoke-operations` consumer), NOT in
//! `nexus-local-db` (pure storage).
//!
//! Covers: valid envelope → stored + read back verbatim (field mapping);
//! missing required `holder_entry_id` → rejected, nothing persisted;
//! unknown envelope key → closed-envelope rejection, nothing persisted.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::{KbStore, KnowledgeEntryRecord};
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::mind_state_store::get_mind_state;
use nexus_local_db::{open_pool, run_migrations, LocalDbError};
use nexus_spoke_adapter::adapter::mind_state::validate_and_store_mind_state;
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
    let kb = KnowledgeEntryRecord::new("wld_mind_test", BlockType::Character, "Bo");
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

// ── gate success path (field mapping) ──────────────────────────

#[tokio::test]
async fn gate_stores_valid_mind_state_verbatim() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    let envelope = valid_envelope(&holder_id);
    validate_and_store_mind_state(&pool, &envelope)
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
    // created_at / updated_at are store-stamped at insert (RFC 3339).
    assert!(!row.created_at.is_empty());
    assert!(!row.updated_at.is_empty());
}

// ── gate rejection paths ────────────────────────────────────────

#[tokio::test]
async fn gate_rejects_missing_holder_entry_id_and_persists_nothing() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    let mut envelope = valid_envelope(&holder_id);
    envelope.as_object_mut().unwrap().remove("holder_entry_id");

    let err = validate_and_store_mind_state(&pool, &envelope)
        .await
        .unwrap_err();
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
async fn gate_rejects_unknown_envelope_key_and_persists_nothing() {
    let (pool, _dir) = setup_db().await;
    let holder_id = seed_holder(&pool).await;

    let mut envelope = valid_envelope(&holder_id);
    envelope
        .as_object_mut()
        .unwrap()
        .insert("bogus_key".to_string(), json!(1));

    let err = validate_and_store_mind_state(&pool, &envelope)
        .await
        .unwrap_err();
    match err {
        LocalDbError::ValidationError(msg) => {
            assert!(
                msg.contains("unknown property") || msg.contains("bogus_key"),
                "rejection must flag the unknown key: {msg}"
            );
        }
        other => panic!("expected ValidationError, got {other:?}"),
    }
    assert!(
        get_mind_state(&pool, "ms_001").await.unwrap().is_none(),
        "rejected envelope must not persist"
    );
}
