//! P0 Task 2 — Character / `ActorWorldBinding` store transaction proofs.

#![allow(clippy::unwrap_used)]

use nexus_local_db::{
    add_actor_world_binding, create_character_with_initial_binding, get_actor_world_binding,
    get_character, list_bindings_for_character, mint_character_id, remove_binding,
    transition_character, update_actor_world_binding, update_character, ActorContractConflict,
    CharacterPatch, CharacterStatus, CreateBindingParams, CreateCharacterParams, FieldPatch,
    LocalDbError,
};
use sqlx::SqlitePool;

const OWNER: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const OTHER: &str = "ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
const WORLD_A: &str = "wld_worldA";
const WORLD_B: &str = "wld_worldB";

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = nexus_local_db::open_pool(&dir.path().join("test.db"))
        .await
        .unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    (pool, dir)
}

async fn seed_creator_and_worlds(pool: &SqlitePool) {
    for (id, name) in [(OWNER, "Owner"), (OTHER, "Other")] {
        nexus_local_db::ensure_creator_row(pool, id, name)
            .await
            .unwrap();
    }
    for (world_id, owner) in [(WORLD_A, OWNER), (WORLD_B, OWNER)] {
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(world_id)
        .bind(owner)
        .bind(world_id)
        .bind(world_id)
        .execute(pool)
        .await
        .unwrap();
    }
}

async fn seed_sheet(
    pool: &SqlitePool,
    key_block_id: &str,
    world_id: &str,
    block_type: &str,
    status: &str,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at) \
         VALUES (?, ?, ?, 'sheet', ?, '{}', datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(block_type)
    .bind(status)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn atomic_create_commits_character_and_binding() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;

    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Ava",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();

    assert_eq!(created.character.status, "active");
    assert!(created.character.character_id.starts_with("chr_"));
    assert_eq!(created.binding.world_id, WORLD_A);
    assert_eq!(created.binding.status, "active");

    let chars: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&pool)
        .await
        .unwrap();
    let binds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(chars, 1);
    assert_eq!(binds, 1);
}

#[tokio::test]
async fn atomic_create_rolls_back_when_world_is_missing() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;

    let err = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Ghost",
            image_uri: None,
            persona_json: "{}",
            world_id: "wld_missing",
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, LocalDbError::ActorNotFound { .. }));

    let chars: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&pool)
        .await
        .unwrap();
    let binds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(chars, 0);
    assert_eq!(binds, 0);
}

#[tokio::test]
async fn stored_ownership_hides_foreign_character() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Owned",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();

    let found = get_character(&pool, OTHER, &created.character.character_id)
        .await
        .unwrap();
    assert!(found.is_none());
}

#[tokio::test]
async fn duplicate_active_binding_is_rejected() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Dup",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();

    let err = add_actor_world_binding(
        &pool,
        CreateBindingParams {
            owner_creator_id: OWNER,
            character_id: &created.character.character_id,
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LocalDbError::ActorContractConflict {
                code: ActorContractConflict::DuplicateActiveBinding
            }
        ),
        "unexpected duplicate mapping: {err:?} display={err}"
    );
}

#[tokio::test]
async fn migration_preserves_binding_tuple_and_revision_default() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    seed_sheet(&pool, "kb_linked", WORLD_A, "character", "confirmed").await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Linked",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: Some("kb_linked"),
        },
    )
    .await
    .unwrap();
    let row: (String, String, String, String, Option<String>, i64) = sqlx::query_as(
        "SELECT binding_id, character_id, world_id, status, world_sheet_entry_id, revision FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(&created.binding.binding_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(row.0, created.binding.binding_id);
    assert_eq!(row.1, created.character.character_id);
    assert_eq!(row.2, WORLD_A);
    assert_eq!(row.3, "active");
    assert_eq!(row.4.as_deref(), Some("kb_linked"));
    assert_eq!(row.5, 0);
    assert_eq!(created.binding.revision, 0);
}

#[tokio::test]
async fn world_sheet_rejects_wrong_world_type_or_deleted() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    seed_sheet(&pool, "kb_wrong_world", WORLD_B, "character", "confirmed").await;
    seed_sheet(&pool, "kb_wrong_type", WORLD_A, "location", "confirmed").await;
    seed_sheet(&pool, "kb_deleted", WORLD_A, "character", "deleted").await;

    for sheet in ["kb_wrong_world", "kb_wrong_type", "kb_deleted"] {
        let err = create_character_with_initial_binding(
            &pool,
            CreateCharacterParams {
                owner_creator_id: OWNER,
                display_name: sheet,
                image_uri: None,
                persona_json: "{}",
                world_id: WORLD_A,
                world_sheet_entry_id: Some(sheet),
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                err,
                LocalDbError::ActorContractConflict {
                    code: ActorContractConflict::InvalidWorldSheet
                }
            ),
            "sheet {sheet} should fail validation, got {err:?}"
        );
    }

    let chars: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(chars, 0);
}

#[tokio::test]
async fn world_sheet_rejects_merged_deprecated_and_creator_only() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    for (sheet, status) in [
        ("kb_merged", "merged"),
        ("kb_deprecated", "deprecated"),
    ] {
        seed_sheet(&pool, sheet, WORLD_A, "character", status).await;
        let err = create_character_with_initial_binding(
            &pool,
            CreateCharacterParams {
                owner_creator_id: OWNER,
                display_name: sheet,
                image_uri: None,
                persona_json: "{}",
                world_id: WORLD_A,
                world_sheet_entry_id: Some(sheet),
            },
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                err,
                LocalDbError::ActorContractConflict {
                    code: ActorContractConflict::InvalidWorldSheet
                }
            ),
            "sheet {sheet} status {status} should fail, got {err:?}"
        );
    }
    sqlx::query(
        "INSERT INTO kb_key_blocks          (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at, creator_only)          VALUES (?, ?, 'character', 'private', 'confirmed', '{}', datetime('now'), 1)",
    )
    .bind("kb_creator_only")
    .bind(WORLD_A)
    .execute(&pool)
    .await
    .unwrap();
    let err = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "PrivateSheet",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: Some("kb_creator_only"),
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::InvalidWorldSheet
        }
    ));
}

#[tokio::test]
async fn world_sheet_rejects_overlength_id_without_leaking_facts() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let overlength = format!("kb_{}", "a".repeat(126));

    let err = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Overlength",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: Some(&overlength),
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LocalDbError::ActorContractConflict {
                code: ActorContractConflict::InvalidWorldSheet
            }
        ),
        "overlength sheet id must map to invalid_world_sheet, got {err:?}"
    );
    assert!(
        !matches!(err, LocalDbError::ValidationError(_)),
        "must not leak target facts via ValidationError"
    );

    let chars: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&pool)
        .await
        .unwrap();
    let binds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(chars, 0);
    assert_eq!(binds, 0);
}

#[tokio::test]
async fn last_binding_remove_is_zero_mutation_conflict() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Last",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();

    let err = remove_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        &created.binding.binding_id,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::LastActiveBinding
        }
    ));

    let binds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    let status: String = sqlx::query_scalar("SELECT status FROM characters WHERE character_id = ?")
        .bind(&created.character.character_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(binds, 1);
    assert_eq!(status, "active");
}

#[tokio::test]
async fn non_last_remove_deletes_only_that_row() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Two",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let second = add_actor_world_binding(
        &pool,
        CreateBindingParams {
            owner_creator_id: OWNER,
            character_id: &created.character.character_id,
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();

    remove_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        &second.binding_id,
    )
    .await
    .unwrap();

    let remaining =
        list_bindings_for_character(&pool, OWNER, &created.character.character_id, 100, 0)
            .await
            .unwrap();
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].binding_id, created.binding.binding_id);
    assert_eq!(remaining[0].world_id, WORLD_A);
}

#[tokio::test]
async fn concurrent_last_binding_removes_leave_one_active() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Race",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let second = add_actor_world_binding(
        &pool,
        CreateBindingParams {
            owner_creator_id: OWNER,
            character_id: &created.character.character_id,
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();

    let pool_a = pool.clone();
    let pool_b = pool.clone();
    let character_id = created.character.character_id.clone();
    let first_id = created.binding.binding_id.clone();
    let second_id = second.binding_id.clone();

    let (r1, r2) = tokio::join!(
        remove_binding(&pool_a, OWNER, &character_id, &first_id),
        remove_binding(&pool_b, OWNER, &character_id, &second_id),
    );

    let outcomes = [r1.is_ok(), r2.is_ok()];
    assert_eq!(
        outcomes.iter().filter(|ok| **ok).count(),
        1,
        "exactly one concurrent remove must succeed, got {r1:?} / {r2:?}"
    );
    assert!(r1.is_err() || r2.is_err());
    let err = r1
        .err()
        .or_else(|| r2.err())
        .expect("exactly one concurrent remove must fail");
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::LastActiveBinding
        }
    ));

    let remaining: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ? AND status = 'active'",
    )
    .bind(&character_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 1);
}

#[test]
fn mint_character_id_matches_db_shape() {
    let id = mint_character_id();
    assert_eq!(id.len(), 36);
    assert!(id.starts_with("chr_"));
    assert!(id[4..]
        .chars()
        .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}

#[tokio::test]
async fn null_primary_keys_are_rejected() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let chr_err = sqlx::query("INSERT INTO characters (character_id, owner_creator_id, display_name, status, persona_json, created_at, updated_at) VALUES (NULL, ?, 'N', 'active', '{}', datetime('now'), datetime('now'))")
        .bind(OWNER)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        chr_err.to_string().contains("NOT NULL")
            || chr_err.to_string().to_lowercase().contains("constraint")
    );

    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "NullPk",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let bind_err = sqlx::query("INSERT INTO actor_world_bindings (binding_id, character_id, world_id, status, created_at, updated_at) VALUES (NULL, ?, ?, 'inactive', datetime('now'), datetime('now'))")
        .bind(&created.character.character_id)
        .bind(WORLD_B)
        .execute(&pool)
        .await
        .unwrap_err();
    assert!(
        bind_err.to_string().contains("NOT NULL")
            || bind_err.to_string().to_lowercase().contains("constraint")
    );
}

#[tokio::test]
async fn binding_admission_rejects_archived_character() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Archived",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(&created.character.character_id)
        .execute(&pool)
        .await
        .unwrap();
    let err = add_actor_world_binding(
        &pool,
        CreateBindingParams {
            owner_creator_id: OWNER,
            character_id: &created.character.character_id,
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::CharacterInactive
        }
    ));
    let extra: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ? AND world_id = ?",
    )
    .bind(&created.character.character_id)
    .bind(WORLD_B)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(extra, 0);
}

#[tokio::test]
async fn duplicate_character_display_name_is_not_duplicate_binding() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Same",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let err = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Same",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(
        matches!(
            err,
            LocalDbError::ActorContractConflict {
                code: ActorContractConflict::DuplicateCharacterDisplayName
            }
        ),
        "owner/display unique must map to duplicate display name, got {err:?}"
    );
}

#[tokio::test]
async fn actor_identity_indexes_include_world_sheet_fk() {
    let (pool, _dir) = fresh_pool().await;
    let name: Option<String> = sqlx::query_scalar(
        "SELECT name FROM sqlite_master WHERE type = 'index' AND name = 'idx_actor_world_bindings_world_sheet_entry_id'",
    )
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(
        name.as_deref(),
        Some("idx_actor_world_bindings_world_sheet_entry_id")
    );
}

#[tokio::test]
async fn actor_conflict_display_is_human_readable() {
    let msg = LocalDbError::ActorContractConflict {
        code: ActorContractConflict::LastActiveBinding,
    }
    .to_string();
    assert_ne!(msg, "last_active_actor_world_binding");
    assert!(msg.contains("last active"));
}


#[tokio::test]
async fn stale_character_revision_rejects_update() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Ava",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let id = &created.character.character_id;
    let err = update_character(
        &pool,
        OWNER,
        id,
        1,
        CharacterPatch {
            display_name: Some("Renamed"),
            image_uri: FieldPatch::Keep,
            persona_json: FieldPatch::Keep,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::CharacterRevisionConflict
        }
    ));
}

#[tokio::test]
async fn update_character_nullable_clear_and_omit() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Ava",
            image_uri: Some("https://example.test/x.png"),
            persona_json: r#"{"k":1}"#,
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let id = &created.character.character_id;
    let cleared = update_character(
        &pool,
        OWNER,
        id,
        0,
        CharacterPatch {
            display_name: None,
            image_uri: FieldPatch::Clear,
            persona_json: FieldPatch::Clear,
        },
    )
    .await
    .unwrap();
    assert_eq!(cleared.revision, 1);
    assert!(cleared.image_uri.is_none());
    assert_eq!(cleared.persona_json, "{}");
    let noop = update_character(
        &pool,
        OWNER,
        id,
        cleared.revision,
        CharacterPatch {
            display_name: None,
            image_uri: FieldPatch::Keep,
            persona_json: FieldPatch::Keep,
        },
    )
    .await
    .unwrap();
    assert_eq!(noop.revision, cleared.revision);
    assert_eq!(noop.updated_at, cleared.updated_at);
}

#[tokio::test]
async fn lifecycle_same_state_is_revision_checked_noop() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Ava",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let id = &created.character.character_id;
    let again = transition_character(&pool, OWNER, id, 0, CharacterStatus::Active)
        .await
        .unwrap();
    assert_eq!(again.revision, 0);
    assert_eq!(again.lifecycle_epoch, 0);
}

#[tokio::test]
async fn restore_requires_active_binding_and_name_collision() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Shared",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let id = &created.character.character_id;
    sqlx::query("UPDATE narrative_worlds SET status = 'archived' WHERE world_id = ?")
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .unwrap();
    let archived = transition_character(&pool, OWNER, id, 0, CharacterStatus::Archived)
        .await
        .unwrap();
    assert_eq!(archived.status, "archived");
    assert_eq!(archived.lifecycle_epoch, 1);
    let err = transition_character(&pool, OWNER, id, archived.revision, CharacterStatus::Active)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::CharacterRestoreRequiresActiveBinding
        }
    ));
    sqlx::query("UPDATE narrative_worlds SET status = 'active' WHERE world_id = ?")
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .unwrap();
    create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Shared",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let err = transition_character(&pool, OWNER, id, archived.revision, CharacterStatus::Active)
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::DuplicateCharacterDisplayName
        }
    ));
    let row: String = sqlx::query_scalar("SELECT status FROM characters WHERE character_id = ?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(row, "archived");
}

async fn seed_sheet_with_body(
    pool: &SqlitePool,
    key_block_id: &str,
    world_id: &str,
    canonical_name: &str,
    body_json: &str,
) {
    sqlx::query(
        "INSERT INTO kb_key_blocks          (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at)          VALUES (?, ?, 'character', ?, 'confirmed', ?, datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(canonical_name)
    .bind(body_json)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn binding_detail_link_relink_clear_and_noop() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    seed_sheet_with_body(&pool, "kb_sheet_a", WORLD_A, "sheet_a", r#"{"name":"Ava"}"#).await;
    seed_sheet_with_body(&pool, "kb_sheet_b", WORLD_A, "sheet_b", r#"{"name":"B"}"#).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "SheetFlow",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let character_id = &created.character.character_id;
    let binding_id = &created.binding.binding_id;

    let linked = update_actor_world_binding(
        &pool,
        OWNER,
        character_id,
        binding_id,
        0,
        FieldPatch::Set("kb_sheet_a"),
    )
    .await
    .unwrap();
    assert_eq!(linked.revision, 1);
    assert_eq!(linked.world_sheet_entry_id.as_deref(), Some("kb_sheet_a"));

    let body: String = sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
        .bind("kb_sheet_a")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(body, r#"{"name":"Ava"}"#);

    let relinked = update_actor_world_binding(
        &pool,
        OWNER,
        character_id,
        binding_id,
        linked.revision,
        FieldPatch::Set("kb_sheet_b"),
    )
    .await
    .unwrap();
    assert_eq!(relinked.revision, 2);
    assert_eq!(relinked.world_sheet_entry_id.as_deref(), Some("kb_sheet_b"));

    let cleared = update_actor_world_binding(
        &pool,
        OWNER,
        character_id,
        binding_id,
        relinked.revision,
        FieldPatch::Clear,
    )
    .await
    .unwrap();
    assert_eq!(cleared.revision, 3);
    assert!(cleared.world_sheet_entry_id.is_none());

    let noop = update_actor_world_binding(
        &pool,
        OWNER,
        character_id,
        binding_id,
        cleared.revision,
        FieldPatch::Keep,
    )
    .await
    .unwrap();
    assert_eq!(noop.revision, cleared.revision);
    assert_eq!(noop.updated_at, cleared.updated_at);
}

#[tokio::test]
async fn stale_binding_update_rejects_with_revision_conflict() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    seed_sheet(&pool, "kb_stale", WORLD_A, "character", "confirmed").await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Stale",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let err = update_actor_world_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        &created.binding.binding_id,
        1,
        FieldPatch::Set("kb_stale"),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingRevisionConflict
        }
    ));
}

#[tokio::test]
async fn binding_update_rejects_invalid_world_sheets() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    seed_sheet(&pool, "kb_wrong_world", WORLD_B, "character", "confirmed").await;
    seed_sheet(&pool, "kb_wrong_type", WORLD_A, "location", "confirmed").await;
    seed_sheet(&pool, "kb_deleted", WORLD_A, "character", "deleted").await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Invalid",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    for sheet in ["kb_wrong_world", "kb_wrong_type", "kb_deleted"] {
        let err = update_actor_world_binding(
            &pool,
            OWNER,
            &created.character.character_id,
            &created.binding.binding_id,
            0,
            FieldPatch::Set(sheet),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(
                err,
                LocalDbError::ActorContractConflict {
                    code: ActorContractConflict::InvalidWorldSheet
                }
            ),
            "sheet {sheet} should fail, got {err:?}"
        );
    }
    sqlx::query(
        "INSERT INTO kb_key_blocks          (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at, creator_only)          VALUES (?, ?, 'character', 'private', 'confirmed', '{}', datetime('now'), 1)",
    )
    .bind("kb_creator_only")
    .bind(WORLD_A)
    .execute(&pool)
    .await
    .unwrap();
    let err = update_actor_world_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        &created.binding.binding_id,
        0,
        FieldPatch::Set("kb_creator_only"),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::InvalidWorldSheet
        }
    ));
    let sheet: Option<String> = sqlx::query_scalar(
        "SELECT world_sheet_entry_id FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(&created.binding.binding_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(sheet.is_none());
}

#[tokio::test]
async fn binding_update_rejects_archived_character_and_world() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    seed_sheet(&pool, "kb_arch", WORLD_A, "character", "confirmed").await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Archived",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(&created.character.character_id)
        .execute(&pool)
        .await
        .unwrap();
    let err = update_actor_world_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        &created.binding.binding_id,
        0,
        FieldPatch::Set("kb_arch"),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::CharacterInactive
        }
    ));

    sqlx::query("UPDATE characters SET status = 'active' WHERE character_id = ?")
        .bind(&created.character.character_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE narrative_worlds SET status = 'archived' WHERE world_id = ?")
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .unwrap();
    let err = update_actor_world_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        &created.binding.binding_id,
        0,
        FieldPatch::Set("kb_arch"),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorNotFound { resource: "world", .. }
    ));
}

#[tokio::test]
async fn binding_detail_retained_read_survives_archive() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    seed_sheet(&pool, "kb_retained", WORLD_A, "character", "confirmed").await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Retain",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: Some("kb_retained"),
        },
    )
    .await
    .unwrap();
    let character_id = &created.character.character_id;
    let binding_id = &created.binding.binding_id;
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(character_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE narrative_worlds SET status = 'archived' WHERE world_id = ?")
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .unwrap();
    let detail = get_actor_world_binding(&pool, OWNER, character_id, binding_id)
        .await
        .unwrap()
        .expect("retained binding detail");
    assert_eq!(detail.world_sheet_entry_id.as_deref(), Some("kb_retained"));
    assert_eq!(detail.status, "active");
}

#[tokio::test]
async fn binding_detail_hides_foreign_owner_and_tuple_mismatch() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Hidden",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    assert!(
        get_actor_world_binding(&pool, OTHER, &created.character.character_id, &created.binding.binding_id)
            .await
            .unwrap()
            .is_none()
    );
    assert!(
        get_actor_world_binding(
            &pool,
            OWNER,
            &created.character.character_id,
            "awb_00000000000000000000000000000000",
        )
        .await
        .unwrap()
        .is_none()
    );
}

#[tokio::test]
async fn concurrent_link_and_remove_leave_exactly_one_outcome() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    seed_sheet(&pool, "kb_race", WORLD_A, "character", "confirmed").await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "Race",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let second = add_actor_world_binding(
        &pool,
        CreateBindingParams {
            owner_creator_id: OWNER,
            character_id: &created.character.character_id,
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let pool_link = pool.clone();
    let pool_remove = pool.clone();
    let character_id = created.character.character_id.clone();
    let target = second.binding_id.clone();
    let (link_result, remove_result) = tokio::join!(
        update_actor_world_binding(
            &pool_link,
            OWNER,
            &character_id,
            &target,
            0,
            FieldPatch::Set("kb_race"),
        ),
        remove_binding(&pool_remove, OWNER, &character_id, &target),
    );
    let link_ok = link_result.is_ok();
    let remove_ok = remove_result.is_ok();
    assert!(
        link_ok ^ remove_ok,
        "exactly one of link/remove must succeed: link={link_result:?} remove={remove_result:?}"
    );
    if remove_ok {
        let row: Option<String> = sqlx::query_scalar(
            "SELECT binding_id FROM actor_world_bindings WHERE binding_id = ?",
        )
        .bind(&target)
        .fetch_optional(&pool)
        .await
        .unwrap();
        assert!(row.is_none());
    } else {
        let linked = link_result.unwrap();
        assert_eq!(linked.world_sheet_entry_id.as_deref(), Some("kb_race"));
        let err = remove_result.unwrap_err();
        assert!(matches!(
            err,
            LocalDbError::ActorNotFound { resource: "actor_world_binding", .. }
                | LocalDbError::ActorContractConflict {
                    code: ActorContractConflict::LastActiveBinding
                }
        ));
    }
}

#[tokio::test]
async fn noop_binding_clear_still_checks_revision() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "NoopClear",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    let err = update_actor_world_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        &created.binding.binding_id,
        1,
        FieldPatch::Clear,
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::BindingRevisionConflict
        }
    ));
}
