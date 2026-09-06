//! P2 Task 2 — stored-owner summary CAS and referent-guarded delete.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{KnowledgeEntryBody, KnowledgeEntryRecord};
use nexus_local_db::kb_store::{cas_update_key_block_modules_in_tx, SqliteKbStore};
use nexus_local_db::{
    create_character_with_initial_binding, delete_actor_knowledge_entry, get_actor_knowledge_entry,
    transition_character, update_actor_knowledge_entry, ActorContractConflict, ActorKnowledgePatch,
    CharacterStatus, CreateCharacterParams, FieldPatch, LocalDbError,
};
use sqlx::SqlitePool;

const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD: &str = "wld_worldA";

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = nexus_local_db::open_pool(&dir.path().join("test.db"))
        .await
        .unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

async fn seed(pool: &SqlitePool) {
    for (id, name) in [(OWNER, "Owner"), (OTHER, "Other")] {
        nexus_local_db::ensure_creator_row(pool, id, name).await.unwrap();
    }
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(WORLD)
    .bind(OWNER)
    .bind(WORLD)
    .bind(WORLD)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_character(pool: &SqlitePool) -> (String, String) {
    let created = create_character_with_initial_binding(
        pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Hero",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    (created.character.character_id, created.binding.binding_id)
}

async fn insert_character_ke(
    pool: &SqlitePool,
    character_id: &str,
    name: &str,
    body_json: Option<&str>,
) -> String {
    let store = SqliteKbStore::new(pool.clone());
    let mut kb = KnowledgeEntryRecord::for_character(character_id, BlockType::InfoPoint, name);
    if let Some(raw) = body_json {
        kb.body = serde_json::from_str::<KnowledgeEntryBody>(raw).ok();
        if kb.body.is_none() {
            // raw object stored via manual insert path below for malformed tests
            let entry_id = kb.entry_id.clone();
            sqlx::query(
                "INSERT INTO kb_key_blocks (key_block_id, owner_kind, character_id, block_type, canonical_name, status, body_json, created_at) \
                 VALUES (?, 'character', ?, 'info_point', ?, 'confirmed', ?, datetime('now'))",
            )
            .bind(&entry_id)
            .bind(character_id)
            .bind(name)
            .bind(raw)
            .execute(pool)
            .await
            .unwrap();
            return entry_id;
        }
    }
    let entry_id = kb.entry_id.clone();
    store
        .insert_actor_owned_key_block(OWNER, character_id, None, kb)
        .await
        .unwrap();
    entry_id
}

fn assert_conflict(err: LocalDbError, code: ActorContractConflict) {
    match err {
        LocalDbError::ActorContractConflict { code: got } => assert_eq!(got, code),
        other => panic!("expected conflict {code:?}, got {other:?}"),
    }
}

#[tokio::test]
async fn foreign_path_returns_none_without_leaking() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"hi"}"#)).await;
    assert!(get_actor_knowledge_entry(&pool, OTHER, &chr, &entry)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn archived_character_read_still_works() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"hi"}"#)).await;
    let before = get_actor_knowledge_entry(&pool, OWNER, &chr, &entry)
        .await
        .unwrap()
        .expect("readable");
    transition_character(&pool, OWNER, &chr, 0, CharacterStatus::Archived)
        .await
        .unwrap();
    let after = get_actor_knowledge_entry(&pool, OWNER, &chr, &entry)
        .await
        .unwrap()
        .expect("archive read retained");
    assert_eq!(before.canonical_name, after.canonical_name);
}

#[tokio::test]
async fn summary_patch_preserves_unknown_body_keys_and_modules() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    sqlx::query(
        "UPDATE kb_key_blocks SET body_json = ? WHERE key_block_id = ?",
    )
    .bind(r#"{"summary":"old","custom_flag":true}"#)
    .bind(&entry)
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("UPDATE kb_key_blocks SET modules_json = ? WHERE key_block_id = ?")
        .bind(r#"{"pack":{"tier":1}}"#)
        .bind(&entry)
        .execute(&pool)
        .await
        .unwrap();
    let updated = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("new"),
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.revision, Some(1));
    let body_raw: String = sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(&entry)
        .fetch_one(&pool)
        .await
        .unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_raw).unwrap();
    assert_eq!(body["summary"], "new");
    assert_eq!(body["custom_flag"], true);
    let modules: String = sqlx::query_scalar("SELECT modules_json FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(&entry)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(modules.contains("pack"));
}

#[tokio::test]
async fn no_op_patch_does_not_bump_revision() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"same"}"#)).await;
    let out = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("same"),
        },
    )
    .await
    .unwrap();
    assert_eq!(out.revision.unwrap_or(0), 0);
}

#[tokio::test]
async fn stale_revision_is_knowledge_revision_conflict() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    let err = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        9,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("x"),
        },
    )
    .await
    .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeRevisionConflict);
}

#[tokio::test]
async fn archived_character_write_refused() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    transition_character(&pool, OWNER, &chr, 0, CharacterStatus::Archived)
        .await
        .unwrap();
    let err = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("nope"),
        },
    )
    .await
    .unwrap_err();
    assert_conflict(err, ActorContractConflict::CharacterInactive);
}

#[tokio::test]
async fn non_live_row_not_mutable() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    sqlx::query("UPDATE kb_key_blocks SET status = 'deleted' WHERE key_block_id = ?")
        .bind(&entry)
        .execute(&pool)
        .await
        .unwrap();
    let err = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("nope"),
        },
    )
    .await
    .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeEntryNotMutable);
}

#[tokio::test]
async fn malformed_body_refuses_summary_edit() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some("[]")).await;
    let err = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("nope"),
        },
    )
    .await
    .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeEntryNotMutable);
}

#[tokio::test]
async fn summary_utf8_byte_boundaries_multibyte() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    let at_limit = "字".repeat(21845) + "a";
    assert_eq!(at_limit.len(), 65536);
    update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set(&at_limit),
        },
    )
    .await
    .unwrap();
    let over = format!("{at_limit}b");
    assert!(over.len() > 65536);
    let err = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        1,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set(&over),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LocalDbError::ValidationError(_)));
}

#[tokio::test]
async fn unreferenced_delete_removes_row() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"bye"}"#)).await;
    delete_actor_knowledge_entry(&pool, OWNER, &chr, &entry, 0)
        .await
        .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(&entry)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn delete_refused_when_source_anchor_exists() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    sqlx::query(
        "INSERT INTO kb_source_anchors (key_block_id, anchor_ordinal, source_anchor_json) VALUES (?, 0, '{}')",
    )
    .bind(&entry)
    .execute(&pool)
    .await
    .unwrap();
    let err = delete_actor_knowledge_entry(&pool, OWNER, &chr, &entry, 0)
        .await
        .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeEntryInUse);
}

#[tokio::test]
async fn delete_refused_when_authoritative_mental_module_present() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    sqlx::query("UPDATE kb_key_blocks SET modules_json = ? WHERE key_block_id = ?")
        .bind(r#"{"mental":{"beliefs":[]}}"#)
        .bind(&entry)
        .execute(&pool)
        .await
        .unwrap();
    let err = delete_actor_knowledge_entry(&pool, OWNER, &chr, &entry, 0)
        .await
        .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeEntryInUse);
}

#[tokio::test]
async fn duplicate_canonical_name_maps_to_duplicate_actor_knowledge() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let _a = insert_character_ke(&pool, &chr, "alpha", None).await;
    let b = insert_character_ke(&pool, &chr, "beta", None).await;
    let err = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &b,
        0,
        ActorKnowledgePatch {
            canonical_name: Some("alpha"),
            summary: FieldPatch::Keep,
        },
    )
    .await
    .unwrap_err();
    assert_conflict(err, ActorContractConflict::DuplicateActorKnowledge);
}

#[tokio::test]
async fn tom_cas_bumps_revision_blocking_stale_delete() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "carrier", None).await;
    let modules = r#"{"mental":{"beliefs":[]}}"#;
    let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
    let bumped = cas_update_key_block_modules_in_tx(
        &mut tx,
        &entry,
        modules,
        0,
        &chr,
        "awb_nonexistent",
    )
    .await
    .unwrap();
    assert_eq!(bumped, 1);
    tx.commit().await.unwrap();
    let del_err = delete_actor_knowledge_entry(&pool, OWNER, &chr, &entry, 0)
        .await
        .unwrap_err();
    assert_conflict(del_err, ActorContractConflict::KnowledgeRevisionConflict);
    let rev: i64 = sqlx::query_scalar("SELECT COALESCE(revision,0) FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(&entry)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rev, 1);
}
