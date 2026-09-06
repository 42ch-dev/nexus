//! v1.184 P4 Task 1 — atomic carrier CAS + derivative `MindState` seam tests.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
use nexus_knowledge::world_kb::KbStore;
use nexus_local_db::kb_store::SqliteKbStore;
use nexus_local_db::mind_state_store::{get_mind_state, LEGACY_MIND_STATE_WIRE_FIXTURE};
use nexus_local_db::{open_pool, run_migrations, LocalDbError};
use nexus_spoke_adapter::adapter::mind_state::{
    atomic_cas_carrier_modules_and_insert_mind_state_in_tx, validate_and_store_mind_state,
};
use serde_json::{json, Value};

const CREATOR: &str = "ctr_tom_task1_cccccccccccccccccccccccccccc";
const CHARACTER: &str = "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORLD: &str = "wld_tom_task1";

async fn setup_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    nexus_local_db::ensure_creator_row(&pool, CREATOR, "TomTask1")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', '2026-08-01T00:00:00Z')",
    )
    .bind(WORLD)
    .bind(CREATOR)
    .bind(WORLD)
    .bind(WORLD)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO characters \
         (character_id, owner_creator_id, display_name, status, image_uri, persona_json, \
          created_at, updated_at) \
         VALUES (?, ?, ?, 'active', NULL, '{}', '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
    )
    .bind(CHARACTER)
    .bind(CREATOR)
    .bind(CHARACTER)
    .execute(&pool)
    .await
    .unwrap();
    (pool, dir)
}

async fn seed_character_carrier(pool: &sqlx::SqlitePool) -> String {
    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KnowledgeEntryRecord::for_character(CHARACTER, BlockType::Character, "Carrier");
    kb.modules = Some(json!({ "belief": [] }));
    let id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    id
}

fn mind_state_for_carrier(carrier_id: &str, mind_state_id: &str) -> Value {
    let mut wire: Value = serde_json::from_str(LEGACY_MIND_STATE_WIRE_FIXTURE).unwrap();
    wire["mind_state_id"] = json!(mind_state_id);
    wire["holder_entry_id"] = json!(carrier_id);
    wire
}

async fn modules_json(pool: &sqlx::SqlitePool, carrier_id: &str) -> String {
    sqlx::query_scalar("SELECT modules_json FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(carrier_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn revision(pool: &sqlx::SqlitePool, carrier_id: &str) -> i64 {
    sqlx::query_scalar::<_, Option<i64>>(
        "SELECT revision FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(carrier_id)
    .fetch_one(pool)
    .await
    .unwrap()
    .unwrap_or(0)
}

#[tokio::test]
async fn legacy_mind_state_wire_fixture_still_validates_through_gate() {
    let (pool, _dir) = setup_db().await;
    let carrier = seed_character_carrier(&pool).await;
    let wire = mind_state_for_carrier(&carrier, "ms_legacy_pin");
    validate_and_store_mind_state(&pool, &wire).await.unwrap();
    assert!(get_mind_state(&pool, "ms_legacy_pin")
        .await
        .unwrap()
        .is_some());
}

#[tokio::test]
async fn atomic_write_commits_carrier_cas_and_derivative_mind_state() {
    let (pool, _dir) = setup_db().await;
    let carrier = seed_character_carrier(&pool).await;
    let modules = json!({
        "belief": [{
            "holder": CHARACTER,
            "proposition": "I know the dock",
            "order": 1,
            "truth": "True",
            "access": "Private",
            "representation": "Explicit",
            "content_type": "Location",
            "source": "Perception",
            "context": "Neutral"
        }]
    });
    let modules_str = serde_json::to_string(&modules).unwrap();
    let wire = mind_state_for_carrier(&carrier, "ms_atomic_ok");

    let mut tx = pool.begin().await.unwrap();
    let new_rev = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
        &mut tx,
        &carrier,
        0,
        &modules_str,
        &wire,
        CHARACTER,
        "awb_seam_unused",
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    assert_eq!(new_rev, 1);
    assert_eq!(revision(&pool, &carrier).await, 1);
    assert!(modules_json(&pool, &carrier)
        .await
        .contains("I know the dock"));
    let row = get_mind_state(&pool, "ms_atomic_ok")
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.holder_entry_id, carrier);
}

#[tokio::test]
async fn atomic_write_rolls_back_on_mind_state_validation_failure() {
    let (pool, _dir) = setup_db().await;
    let carrier = seed_character_carrier(&pool).await;
    let before = modules_json(&pool, &carrier).await;
    let mut wire = mind_state_for_carrier(&carrier, "ms_bad");
    wire.as_object_mut().unwrap().remove("extensions");

    let mut tx = pool.begin().await.unwrap();
    let err = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
        &mut tx,
        &carrier,
        0,
        r#"{"belief":[]}"#,
        &wire,
        CHARACTER,
        "awb_seam_unused",
    )
    .await
    .unwrap_err();
    let _ = tx.rollback().await;

    assert!(matches!(err, LocalDbError::ValidationError(_)));
    assert_eq!(revision(&pool, &carrier).await, 0);
    assert_eq!(modules_json(&pool, &carrier).await, before);
    assert!(get_mind_state(&pool, "ms_bad").await.unwrap().is_none());
}

#[tokio::test]
async fn atomic_write_rolls_back_on_cas_miss() {
    let (pool, _dir) = setup_db().await;
    let carrier = seed_character_carrier(&pool).await;
    let before = modules_json(&pool, &carrier).await;
    let wire = mind_state_for_carrier(&carrier, "ms_cas_miss");

    let mut tx = pool.begin().await.unwrap();
    let err = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
        &mut tx,
        &carrier,
        99,
        r#"{"belief":[]}"#,
        &wire,
        CHARACTER,
        "awb_seam_unused",
    )
    .await
    .unwrap_err();
    let _ = tx.rollback().await;

    assert!(matches!(err, LocalDbError::VersionMismatch { .. }));
    assert_eq!(modules_json(&pool, &carrier).await, before);
    assert!(get_mind_state(&pool, "ms_cas_miss")
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn rejects_chr_subject_on_mind_state_holder_entry_id() {
    let (pool, _dir) = setup_db().await;
    let carrier = seed_character_carrier(&pool).await;
    let mut wire = mind_state_for_carrier(&carrier, "ms_chr_bad");
    wire["holder_entry_id"] = json!(CHARACTER);

    let mut tx = pool.begin().await.unwrap();
    let err = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
        &mut tx,
        &carrier,
        0,
        r#"{"belief":[]}"#,
        &wire,
        CHARACTER,
        "awb_seam_unused",
    )
    .await
    .unwrap_err();
    let _ = tx.rollback().await;

    assert!(matches!(err, LocalDbError::ValidationError(_)));
}

#[tokio::test]
async fn atomic_write_rejects_concurrent_soft_delete_without_revision_bump() {
    // QC fix round 1 (F-004): soft-delete flips status without bumping
    // revision; the in-transaction CAS predicate must revalidate non-deleted
    // status so the write cannot commit onto a deleted carrier.
    let (pool, _dir) = setup_db().await;
    let carrier = seed_character_carrier(&pool).await;
    let before = modules_json(&pool, &carrier).await;
    let rev = revision(&pool, &carrier).await;
    sqlx::query(
        "UPDATE kb_key_blocks SET status = 'deleted', updated_at = '2026-09-05T00:00:00Z' \
         WHERE key_block_id = ?",
    )
    .bind(&carrier)
    .execute(&pool)
    .await
    .unwrap();

    let wire = mind_state_for_carrier(&carrier, "ms_softdelete_race");
    let mut tx = pool.begin().await.unwrap();
    let err = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
        &mut tx,
        &carrier,
        rev,
        r#"{"belief":[]}"#,
        &wire,
        CHARACTER,
        "awb_seam_unused",
    )
    .await
    .expect_err("soft-deleted carrier must miss the in-transaction CAS predicate");
    let _ = tx.rollback().await;

    assert_eq!(modules_json(&pool, &carrier).await, before);
    assert!(get_mind_state(&pool, "ms_softdelete_race")
        .await
        .unwrap()
        .is_none());
    drop(err);
}

#[tokio::test]
async fn atomic_write_rejects_concurrent_owner_drift() {
    // QC fix round 1 (F-004): ownership drift to another Character between
    // admission and commit must miss the CAS predicate inside the transaction.
    let (pool, _dir) = setup_db().await;
    let carrier = seed_character_carrier(&pool).await;
    let before = modules_json(&pool, &carrier).await;
    let rev = revision(&pool, &carrier).await;
    sqlx::query(
        "INSERT INTO characters \
         (character_id, owner_creator_id, display_name, status, image_uri, persona_json, \
          created_at, updated_at) \
         VALUES ('chr_cccccccccccccccccccccccccccccccc', ?, 'Drifted', 'active', NULL, '{}', \
          '2026-08-01T00:00:00Z', '2026-08-01T00:00:00Z')",
    )
    .bind(CREATOR)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE kb_key_blocks SET character_id = 'chr_cccccccccccccccccccccccccccccccc' WHERE key_block_id = ?")
        .bind(&carrier)
        .execute(&pool)
        .await
        .unwrap();

    let wire = mind_state_for_carrier(&carrier, "ms_owner_drift");
    let mut tx = pool.begin().await.unwrap();
    let err = atomic_cas_carrier_modules_and_insert_mind_state_in_tx(
        &mut tx,
        &carrier,
        rev,
        r#"{"belief":[]}"#,
        &wire,
        CHARACTER,
        "awb_seam_unused",
    )
    .await
    .expect_err("owner-drifted carrier must miss the in-transaction CAS predicate");
    let _ = tx.rollback().await;

    assert_eq!(modules_json(&pool, &carrier).await, before);
    assert!(get_mind_state(&pool, "ms_owner_drift")
        .await
        .unwrap()
        .is_none());
    drop(err);
}
