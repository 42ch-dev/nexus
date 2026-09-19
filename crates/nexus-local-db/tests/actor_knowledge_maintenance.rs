//! P2 Task 2 — stored-owner summary CAS and referent-guarded delete.

#![allow(clippy::unwrap_used)]

use nexus_contracts::BlockType;
use nexus_knowledge::world_kb::knowledge_entry::{
    KnowledgeEntryBody, KnowledgeEntryRecord, DISCLOSURE_OWNER_PRIVATE,
};
use nexus_local_db::kb_store::{
    cas_update_key_block_modules_in_tx, AuthoredAudience, SqliteKbStore,
};
use nexus_local_db::{
    author_actor_knowledge_entry, character_holder_entry_id, create_character_with_initial_binding,
    creator_holder_entry_id, delete_actor_knowledge_entry, get_actor_knowledge_entry,
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
        nexus_local_db::ensure_creator_row(pool, id, name)
            .await
            .unwrap();
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

/// Another Character owned by `OWNER`, created with its initial binding in
/// `world_id` (the same World unless the test seeds a second one).
async fn seed_character_in(pool: &SqlitePool, world_id: &str, display_name: &str) -> String {
    create_character_with_initial_binding(
        pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name,
            image_uri: None,
            persona_json: "{}",
            world_id,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap()
    .character
    .character_id
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
    // v1.191 P1 T6 (C1): the stored-owner read resolves the requested
    // Character's registry holder, so a foreign requested owner is refused
    // with the same `ActorNotFound` a missing Character yields — the refused
    // class still reveals no row and no holder fact. This assertion was left
    // on the pre-T6 `Option::None` shape when C1 landed; it is aligned here
    // while the read path's test file is in scope.
    let refused = get_actor_knowledge_entry(&pool, OTHER, &chr, &entry)
        .await
        .unwrap_err();
    assert!(matches!(refused, LocalDbError::ActorNotFound { .. }));
    // Positive control: the owning Creator still sees the row.
    assert!(get_actor_knowledge_entry(&pool, OWNER, &chr, &entry)
        .await
        .unwrap()
        .is_some());
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
    sqlx::query("UPDATE kb_key_blocks SET body_json = ? WHERE key_block_id = ?")
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
    let body_raw: String =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_one(&pool)
            .await
            .unwrap();
    let body: serde_json::Value = serde_json::from_str(&body_raw).unwrap();
    assert_eq!(body["summary"], "new");
    assert_eq!(body["custom_flag"], true);
    let modules: String =
        sqlx::query_scalar("SELECT modules_json FROM kb_key_blocks WHERE key_block_id = ?")
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
async fn clear_on_explicit_null_summary_removes_key_and_bumps_revision() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    sqlx::query("UPDATE kb_key_blocks SET body_json = ? WHERE key_block_id = ?")
        .bind(r#"{"summary":null}"#)
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
            summary: FieldPatch::Clear,
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.revision, Some(1));
    let body_raw: Option<String> =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_one(&pool)
            .await
            .unwrap();
    match body_raw {
        None => {}
        Some(raw) => {
            let body: serde_json::Value = serde_json::from_str(&raw).unwrap();
            assert!(body.get("summary").is_none());
        }
    }
}

#[tokio::test]
async fn clear_on_absent_summary_key_is_no_op() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    let before_updated: Option<String> =
        sqlx::query_scalar("SELECT updated_at FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_optional(&pool)
            .await
            .unwrap();
    let out = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Clear,
        },
    )
    .await
    .unwrap();
    assert_eq!(out.revision.unwrap_or(0), 0);
    let after_updated: Option<String> =
        sqlx::query_scalar("SELECT updated_at FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert_eq!(before_updated, after_updated);
}

#[tokio::test]
async fn clear_on_string_summary_removes_key_and_bumps_revision() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"note"}"#)).await;
    let updated = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Clear,
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.revision, Some(1));
    let body_raw: Option<String> =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(body_raw.is_none() || !body_raw.as_ref().expect("body").contains("\"summary\""));
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

async fn assert_body_and_revision_unchanged(
    pool: &SqlitePool,
    entry: &str,
    expected_body: &str,
    expected_revision: i64,
) {
    let (body_raw, revision): (Option<String>, Option<i64>) =
        sqlx::query_as("SELECT body_json, revision FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(entry)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(body_raw.as_deref(), Some(expected_body));
    assert_eq!(revision.unwrap_or(0), expected_revision);
}

#[tokio::test]
async fn malformed_numeric_summary_refuses_summary_edit() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":42}"#)).await;
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
    assert_body_and_revision_unchanged(&pool, &entry, r#"{"summary":42}"#, 0).await;
    let err = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Clear,
        },
    )
    .await
    .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeEntryNotMutable);
    assert_body_and_revision_unchanged(&pool, &entry, r#"{"summary":42}"#, 0).await;
}

#[tokio::test]
async fn malformed_object_summary_refuses_summary_edit() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":{}}"#)).await;
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
    assert_body_and_revision_unchanged(&pool, &entry, r#"{"summary":{}}"#, 0).await;
    let err = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Clear,
        },
    )
    .await
    .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeEntryNotMutable);
    assert_body_and_revision_unchanged(&pool, &entry, r#"{"summary":{}}"#, 0).await;
}

#[tokio::test]
async fn string_summary_edits_fine() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"text"}"#)).await;
    let updated = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("edited"),
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.revision, Some(1));
    let body_raw: Option<String> =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(body_raw.as_deref(), Some(r#"{"summary":"edited"}"#));
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
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id = ?")
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
    let bumped =
        cas_update_key_block_modules_in_tx(&mut tx, &entry, modules, 0, &chr, "awb_nonexistent")
            .await
            .unwrap();
    assert_eq!(bumped, 1);
    tx.commit().await.unwrap();
    let del_err = delete_actor_knowledge_entry(&pool, OWNER, &chr, &entry, 0)
        .await
        .unwrap_err();
    assert_conflict(del_err, ActorContractConflict::KnowledgeRevisionConflict);
    let rev: i64 =
        sqlx::query_scalar("SELECT COALESCE(revision,0) FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(rev, 1);
}

#[tokio::test]
async fn binding_owned_ke_foreign_world_read_returns_none() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, binding_id) = seed_character(&pool).await;
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, time_policy, metadata_json, created_at) \
         VALUES ('wld_foreign', 'ws', ?, 'Foreign', 'foreign', 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(OTHER)
    .execute(&pool)
    .await
    .unwrap();
    let store = SqliteKbStore::new(pool.clone());
    let kb = KnowledgeEntryRecord::for_binding(&binding_id, BlockType::InfoPoint, "bindingFact");
    let inserted = store
        .insert_actor_owned_key_block(OWNER, &chr, Some(&binding_id), kb)
        .await
        .unwrap();
    sqlx::query("UPDATE actor_world_bindings SET world_id = 'wld_foreign' WHERE binding_id = ?")
        .bind(&binding_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        get_actor_knowledge_entry(&pool, OWNER, &chr, &inserted.entry_id)
            .await
            .unwrap()
            .is_none(),
        "foreign-world binding KE must 404 on read"
    );
}

#[tokio::test]
async fn canonical_only_rename_allows_malformed_body() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "oldName", Some("[]")).await;
    let updated = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: Some("newName"),
            summary: FieldPatch::Keep,
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.canonical_name, "newName");
    let body_raw: Option<String> =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(body_raw.as_deref(), Some("[]"));
}

#[tokio::test]
async fn create_duplicate_maps_to_duplicate_actor_knowledge() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let _first = insert_character_ke(&pool, &chr, "dupName", None).await;
    let store = SqliteKbStore::new(pool.clone());
    let kb2 = KnowledgeEntryRecord::for_character(&chr, BlockType::InfoPoint, "dupName");
    let err = store
        .insert_actor_owned_key_block(OWNER, &chr, None, kb2)
        .await
        .unwrap_err();
    assert_conflict(err, ActorContractConflict::DuplicateActorKnowledge);
}

#[tokio::test]
async fn create_summary_utf8_byte_boundaries_multibyte() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let store = SqliteKbStore::new(pool.clone());
    let at_limit = "字".repeat(21845) + "a";
    assert_eq!(at_limit.len(), 65536);
    let mut kb_ok = KnowledgeEntryRecord::for_character(&chr, BlockType::InfoPoint, "okSummary");
    kb_ok.body = Some(KnowledgeEntryBody {
        summary: Some(at_limit.clone()),
        ..Default::default()
    });
    store
        .insert_actor_owned_key_block(OWNER, &chr, None, kb_ok)
        .await
        .unwrap();
    let over = format!("{at_limit}b");
    let mut kb_bad = KnowledgeEntryRecord::for_character(&chr, BlockType::InfoPoint, "badSummary");
    kb_bad.body = Some(KnowledgeEntryBody {
        summary: Some(over),
        ..Default::default()
    });
    let err = store
        .insert_actor_owned_key_block(OWNER, &chr, None, kb_bad)
        .await
        .unwrap_err();
    assert!(matches!(err, LocalDbError::ValidationError(_)));
}

#[tokio::test]
async fn delete_refused_when_target_row_self_references_in_modules() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "selfRef", None).await;
    sqlx::query("UPDATE kb_key_blocks SET modules_json = ? WHERE key_block_id = ?")
        .bind(format!(r#"{{"self":"{entry}"}}"#))
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
async fn delete_malformed_other_modules_json_is_reference_state_invalid() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "target", None).await;
    let other = insert_character_ke(&pool, &chr, "other", None).await;
    sqlx::query("UPDATE kb_key_blocks SET modules_json = ? WHERE key_block_id = ?")
        .bind("{not-json")
        .bind(&other)
        .execute(&pool)
        .await
        .unwrap();
    let err = delete_actor_knowledge_entry(&pool, OWNER, &chr, &entry, 0)
        .await
        .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeReferenceStateInvalid);
}

#[tokio::test]
async fn delete_succeeds_when_target_modules_reference_unrelated_kb_id() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let other = insert_character_ke(&pool, &chr, "otherKe", None).await;
    let entry = insert_character_ke(&pool, &chr, "target", None).await;
    sqlx::query("UPDATE kb_key_blocks SET modules_json = ? WHERE key_block_id = ?")
        .bind(format!(r#"{{"observation":"{other}"}}"#))
        .bind(&entry)
        .execute(&pool)
        .await
        .unwrap();
    delete_actor_knowledge_entry(&pool, OWNER, &chr, &entry, 0)
        .await
        .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(&entry)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[tokio::test]
async fn delete_refused_when_target_modules_reference_own_id() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "selfRef", None).await;
    sqlx::query("UPDATE kb_key_blocks SET modules_json = ? WHERE key_block_id = ?")
        .bind(format!(r#"{{"other":"{entry}"}}"#))
        .bind(&entry)
        .execute(&pool)
        .await
        .unwrap();
    let err = delete_actor_knowledge_entry(&pool, OWNER, &chr, &entry, 0)
        .await
        .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeEntryInUse);
}

// ── v1.191 P1 T7: admitted audience authoring (durable §3/§4.3) ──────────

/// Stored native governance pair + per-row OCC revision of one KE.
async fn stored_pair(pool: &SqlitePool, entry_id: &str) -> (Option<String>, Option<String>, i64) {
    sqlx::query_as(
        "SELECT holder_entry_id, disclosure, COALESCE(revision, 0) \
         FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(entry_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// Whole-row snapshot used to prove a refusal wrote nothing.
async fn row_snapshot(pool: &SqlitePool, entry_id: &str) -> String {
    sqlx::query_scalar::<_, String>(
        "SELECT COALESCE(holder_entry_id, '') || '|' || COALESCE(disclosure, '') || '|' || \
         COALESCE(revision, 0) || '|' || canonical_name || '|' || COALESCE(body_json, '') || '|' || \
         COALESCE(modules_json, '') \
         FROM kb_key_blocks WHERE key_block_id = ?",
    )
    .bind(entry_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn character_knowledge_revision(pool: &SqlitePool, character_id: &str) -> i64 {
    sqlx::query_scalar("SELECT knowledge_revision FROM characters WHERE character_id = ?")
        .bind(character_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn world_knowledge_revision(pool: &SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT knowledge_revision FROM narrative_worlds WHERE world_id = ?")
        .bind(WORLD)
        .fetch_one(pool)
        .await
        .unwrap()
}

const KEEP_CONTENT: ActorKnowledgePatch<'static> = ActorKnowledgePatch {
    canonical_name: None,
    summary: FieldPatch::Keep,
};

async fn create_binding_owned_ke(
    pool: &SqlitePool,
    chr: &str,
    binding: &str,
    name: &str,
) -> String {
    let store = SqliteKbStore::new(pool.clone());
    let kb = KnowledgeEntryRecord::for_binding(binding, BlockType::InfoPoint, name);
    let entry_id = kb.entry_id.clone();
    store
        .insert_actor_owned_key_block(OWNER, chr, Some(binding), kb)
        .await
        .unwrap();
    entry_id
}

#[tokio::test]
async fn v1191_audience_cas_authoring_writes_pair_and_bumps_both_revisions() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"hi"}"#)).await;
    let creator_holder = creator_holder_entry_id(OWNER);

    assert_eq!(
        stored_pair(&pool, &entry).await,
        (None, None, 0),
        "a fresh row is shared"
    );
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 0);

    let authored = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        KEEP_CONTENT,
        AuthoredAudience::Creator { creator_id: OWNER },
    )
    .await
    .unwrap();

    assert_eq!(
        authored.revision,
        Some(1),
        "the KE revision bumps exactly once"
    );
    let (holder, disclosure, revision) = stored_pair(&pool, &entry).await;
    assert_eq!(holder.as_deref(), Some(creator_holder.as_str()));
    assert_eq!(disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));
    assert_eq!(revision, 1);
    assert_eq!(
        character_knowledge_revision(&pool, &chr).await,
        1,
        "the owning Character's knowledge revision bumps exactly once"
    );
    assert_eq!(
        world_knowledge_revision(&pool).await,
        0,
        "a Character-owned row never moves a World revision"
    );
}

#[tokio::test]
async fn v1191_audience_cas_shared_clears_pair_and_identical_reauthoring_is_a_no_op() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;

    author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        KEEP_CONTENT,
        AuthoredAudience::Character { character_id: &chr },
    )
    .await
    .unwrap();
    assert_eq!(
        stored_pair(&pool, &entry).await.1.as_deref(),
        Some(DISCLOSURE_OWNER_PRIVATE)
    );
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 1);

    // Explicit `shared` clears both columns and is material.
    author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        1,
        KEEP_CONTENT,
        AuthoredAudience::Shared,
    )
    .await
    .unwrap();
    assert_eq!(stored_pair(&pool, &entry).await, (None, None, 2));
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 2);

    // Re-authoring the same (shared) pair with unchanged content is a no-op:
    // neither revision may move.
    let before = row_snapshot(&pool, &entry).await;
    let unchanged = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        2,
        KEEP_CONTENT,
        AuthoredAudience::Shared,
    )
    .await
    .unwrap();
    assert_eq!(unchanged.revision, Some(2));
    assert_eq!(
        row_snapshot(&pool, &entry).await,
        before,
        "byte-identical row"
    );
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 2);
}

#[tokio::test]
async fn v1191_audience_cas_no_op_preserves_both_revisions() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"hi"}"#)).await;

    author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        KEEP_CONTENT,
        AuthoredAudience::Character { character_id: &chr },
    )
    .await
    .unwrap();
    let before = row_snapshot(&pool, &entry).await;
    let knowledge_revision = character_knowledge_revision(&pool, &chr).await;

    // Identical content (same summary) + identical audience.
    let no_op = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        1,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("hi"),
        },
        AuthoredAudience::Character { character_id: &chr },
    )
    .await
    .unwrap();
    assert_eq!(no_op.revision, Some(1), "no-op keeps the KE revision");
    assert_eq!(row_snapshot(&pool, &entry).await, before);
    assert_eq!(
        character_knowledge_revision(&pool, &chr).await,
        knowledge_revision,
        "no-op keeps the knowledge revision"
    );
}

/// Regression (L2 C1): an explicit content field that re-states the stored
/// value may not mask a governance change — the no-op decision covers content
/// **and** governance, in both directions.
#[tokio::test]
async fn v1191_audience_cas_content_restating_the_stored_value_still_applies_governance() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"hi"}"#)).await;

    // shared → private while re-stating the stored summary.
    let authored = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("hi"),
        },
        AuthoredAudience::Character { character_id: &chr },
    )
    .await
    .unwrap();
    assert_eq!(
        authored.revision,
        Some(1),
        "the governance write is material"
    );
    assert_eq!(
        stored_pair(&pool, &entry).await,
        (
            Some(character_holder_entry_id(&chr)),
            Some(DISCLOSURE_OWNER_PRIVATE.to_string()),
            1
        ),
        "an unchanged content field must not discard the authored audience"
    );
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 1);

    // private → shared while re-stating the stored summary (the C1 example).
    let cleared = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        1,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("hi"),
        },
        AuthoredAudience::Shared,
    )
    .await
    .unwrap();
    assert_eq!(cleared.revision, Some(2));
    assert_eq!(stored_pair(&pool, &entry).await, (None, None, 2));
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 2);
}

#[tokio::test]
async fn v1191_audience_cas_stale_revision_refuses_with_zero_mutation() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    let before = row_snapshot(&pool, &entry).await;

    let err = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        7,
        KEEP_CONTENT,
        AuthoredAudience::Creator { creator_id: OWNER },
    )
    .await
    .unwrap_err();
    assert_conflict(err, ActorContractConflict::KnowledgeRevisionConflict);
    assert_eq!(
        row_snapshot(&pool, &entry).await,
        before,
        "refusal wrote nothing"
    );
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 0);
}

#[tokio::test]
async fn v1191_audience_cas_unregistered_holder_refuses_with_zero_mutation() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    let before = row_snapshot(&pool, &entry).await;
    // Remove the subject's registry row: the in-transaction resolution must
    // fail closed instead of admitting a silently derived holder id.
    sqlx::query("DELETE FROM knowledge_holders WHERE character_id = ?")
        .bind(&chr)
        .execute(&pool)
        .await
        .unwrap();

    let err = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        KEEP_CONTENT,
        AuthoredAudience::Character { character_id: &chr },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, LocalDbError::HolderStateInvalid { .. }),
        "missing registry state must fail closed, got {err:?}"
    );
    assert_eq!(
        row_snapshot(&pool, &entry).await,
        before,
        "refusal wrote nothing"
    );
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 0);
}

/// Regression (L2 C3/I1): the permitted-identity rules are re-resolved from
/// stored state inside the authoring transaction.
#[tokio::test]
async fn v1191_audience_cas_character_audience_refuses_a_non_owning_character() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let other = seed_character_in(&pool, WORLD, "Other").await;
    let entry = insert_character_ke(&pool, &chr, "fact", None).await;
    let before = row_snapshot(&pool, &entry).await;

    // A Character row may only target its own owning Character.
    let err = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        KEEP_CONTENT,
        AuthoredAudience::Character {
            character_id: &other,
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, LocalDbError::ActorNotFound { .. }),
        "another owned Character is not a permitted audience here, got {err:?}"
    );
    assert_eq!(row_snapshot(&pool, &entry).await, before);
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 0);

    // A foreign Creator's holder is likewise refused (no leak).
    let err = author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        KEEP_CONTENT,
        AuthoredAudience::Creator { creator_id: OTHER },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));
    assert_eq!(row_snapshot(&pool, &entry).await, before);
}

/// Regression (L2 C3): a World container's `character-private` audience is
/// resolved in-transaction against ownership, activity and an **active
/// binding** to that World.
const fn world_container() -> nexus_local_db::kb_store::AudienceContainer<'static> {
    nexus_local_db::kb_store::AudienceContainer {
        owner_creator_id: OWNER,
        owning_character_id: None,
        world_id: Some(WORLD),
    }
}

#[tokio::test]
async fn v1191_audience_cas_world_container_audience_requires_an_active_binding() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, binding) = seed_character(&pool).await;
    // A second owned World, so this Character holds no binding to `WORLD`.
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES ('wld_worldB', 'ws', ?, 'B', 'b', 'active', 'private', 'manual', '{}', \
                 datetime('now'))",
    )
    .bind(OWNER)
    .execute(&pool)
    .await
    .unwrap();
    let unbound = seed_character_in(&pool, "wld_worldB", "Unbound").await;
    // The bound Character resolves to its own registry holder.
    let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
    let resolved = nexus_local_db::kb_store::resolve_authored_audience_tx(
        &mut tx,
        AuthoredAudience::Character { character_id: &chr },
        world_container(),
    )
    .await
    .unwrap()
    .expect("private pair");
    tx.rollback().await.unwrap();
    assert_eq!(
        resolved.holder_entry_id.as_deref(),
        Some(character_holder_entry_id(&chr).as_str())
    );
    assert_eq!(
        resolved.disclosure.as_deref(),
        Some(DISCLOSURE_OWNER_PRIVATE)
    );

    // An owned Character without a binding to that World is refused.
    let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
    let err = nexus_local_db::kb_store::resolve_authored_audience_tx(
        &mut tx,
        AuthoredAudience::Character {
            character_id: &unbound,
        },
        world_container(),
    )
    .await
    .unwrap_err();
    tx.rollback().await.unwrap();
    assert!(
        matches!(err, LocalDbError::ValidationError(_)),
        "an unbound Character is not a permitted World-row audience, got {err:?}"
    );

    // Archiving the bound Character removes it from the permitted set even
    // though its binding row is untouched — the refusal is decided in-tx.
    transition_character(&pool, OWNER, &chr, 0, CharacterStatus::Archived)
        .await
        .unwrap();
    let mut tx = nexus_local_db::begin_immediate(&pool).await.unwrap();
    let err = nexus_local_db::kb_store::resolve_authored_audience_tx(
        &mut tx,
        AuthoredAudience::Character { character_id: &chr },
        world_container(),
    )
    .await
    .unwrap_err();
    tx.rollback().await.unwrap();
    assert_conflict(err, ActorContractConflict::CharacterInactive);

    // The binding itself still exists (the archive did not unlink it).
    let stored: String =
        sqlx::query_scalar("SELECT status FROM actor_world_bindings WHERE binding_id = ?")
            .bind(&binding)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, "active");
}

#[tokio::test]
async fn v1191_audience_cas_ordinary_content_patch_preserves_governance() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, _) = seed_character(&pool).await;
    let entry = insert_character_ke(&pool, &chr, "fact", Some(r#"{"summary":"hi"}"#)).await;
    let char_holder = character_holder_entry_id(&chr);
    author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        KEEP_CONTENT,
        AuthoredAudience::Character { character_id: &chr },
    )
    .await
    .unwrap();

    // The ordinary content lane cannot transfer or clear governance, and a
    // material content edit still bumps the owning Character's revision.
    let updated = update_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        1,
        ActorKnowledgePatch {
            canonical_name: None,
            summary: FieldPatch::Set("rewritten"),
        },
    )
    .await
    .unwrap();
    assert_eq!(updated.revision, Some(2));
    let (holder, disclosure, _) = stored_pair(&pool, &entry).await;
    assert_eq!(holder.as_deref(), Some(char_holder.as_str()));
    assert_eq!(disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));
    assert_eq!(character_knowledge_revision(&pool, &chr).await, 2);
}

#[tokio::test]
async fn v1191_audience_cas_binding_owned_governance_bumps_the_owning_character() {
    let (pool, _dir) = fresh_pool().await;
    seed(&pool).await;
    let (chr, binding) = seed_character(&pool).await;
    let entry = create_binding_owned_ke(&pool, &chr, &binding, "binding-fact").await;
    let char_holder = character_holder_entry_id(&chr);

    author_actor_knowledge_entry(
        &pool,
        OWNER,
        &chr,
        &entry,
        0,
        KEEP_CONTENT,
        AuthoredAudience::Character { character_id: &chr },
    )
    .await
    .unwrap();

    let (holder, disclosure, revision) = stored_pair(&pool, &entry).await;
    assert_eq!(holder.as_deref(), Some(char_holder.as_str()));
    assert_eq!(disclosure.as_deref(), Some(DISCLOSURE_OWNER_PRIVATE));
    assert_eq!(revision, 1);
    assert_eq!(
        character_knowledge_revision(&pool, &chr).await,
        1,
        "a binding-owned governance write bumps its owning Character"
    );
    assert_eq!(
        world_knowledge_revision(&pool).await,
        0,
        "a binding has no revision of its own and never moves a World revision"
    );
}
