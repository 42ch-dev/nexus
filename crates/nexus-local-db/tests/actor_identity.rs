//! P0 Task 2 — Character / `ActorWorldBinding` store transaction proofs.

#![allow(clippy::unwrap_used)]

use nexus_local_db::{
    add_actor_world_binding, character_holder_entry_id, create_character_with_initial_binding,
    creator_holder_entry_id, delete_character, delete_creator, ensure_creator_row,
    get_actor_world_binding, get_character, list_bindings_for_character, mint_character_id,
    remove_binding, require_character_holder, require_creator_holder, resolve_subject_holder,
    transition_character, update_actor_world_binding, update_character, ActorContractConflict,
    CharacterPatch, CharacterStatus, CreateBindingParams, CreateCharacterParams, FieldPatch,
    HolderSubject, LocalDbError,
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

/// Seed a World-owned `character` sheet governed by `OWNER`'s holder: the
/// native `owner-private` pair that replaced the retired `creator_only` bool
/// (v1.191 P1 T3/T4).
async fn seed_private_sheet(pool: &SqlitePool, key_block_id: &str, world_id: &str) {
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at, \
          holder_entry_id, disclosure) \
         VALUES (?, ?, 'character', 'private', 'confirmed', '{}', datetime('now'), ?, 'owner-private')",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(nexus_local_db::creator_holder_entry_id(OWNER))
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
async fn world_sheet_rejects_merged_deprecated_and_private() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    for (sheet, status) in [("kb_merged", "merged"), ("kb_deprecated", "deprecated")] {
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
    // v1.191 P1 T4: the retired `creator_only` column is replaced by the native
    // governance pair — a governed (disclosed) World sheet is not linkable.
    seed_private_sheet(&pool, "kb_private", WORLD_A).await;
    let err = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "PrivateSheet",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: Some("kb_private"),
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

    let body: String =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
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
    // v1.191 P1 T4: the retired `creator_only` bool is replaced by the native
    // governance pair — a governed sheet is rejected on relink as well.
    seed_private_sheet(&pool, "kb_private", WORLD_A).await;
    let err = update_actor_world_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        &created.binding.binding_id,
        0,
        FieldPatch::Set("kb_private"),
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
        LocalDbError::ActorNotFound {
            resource: "world",
            ..
        }
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
    assert!(get_actor_world_binding(
        &pool,
        OTHER,
        &created.character.character_id,
        &created.binding.binding_id
    )
    .await
    .unwrap()
    .is_none());
    assert!(get_actor_world_binding(
        &pool,
        OWNER,
        &created.character.character_id,
        "awb_00000000000000000000000000000000",
    )
    .await
    .unwrap()
    .is_none());
}

async fn seed_two_binding_link_fixture(pool: &SqlitePool) -> (String, String, String) {
    seed_sheet(pool, "kb_race", WORLD_B, "character", "confirmed").await;
    let created = create_character_with_initial_binding(
        pool,
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
        pool,
        CreateBindingParams {
            owner_creator_id: OWNER,
            character_id: &created.character.character_id,
            world_id: WORLD_B,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    (
        created.character.character_id,
        created.binding.binding_id,
        second.binding_id,
    )
}

#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn sequential_link_and_remove_orders() {
    // Subtest 1: link then remove the same non-last target — both succeed.
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let (character_id, primary, target) = seed_two_binding_link_fixture(&pool).await;

    let linked = update_actor_world_binding(
        &pool,
        OWNER,
        &character_id,
        &target,
        0,
        FieldPatch::Set("kb_race"),
    )
    .await
    .unwrap();
    assert_eq!(linked.binding_id, target);
    assert_eq!(linked.world_sheet_entry_id.as_deref(), Some("kb_race"));
    assert_eq!(linked.revision, 1);

    let mid = list_bindings_for_character(&pool, OWNER, &character_id, 100, 0)
        .await
        .unwrap();
    assert_eq!(mid.len(), 2);
    assert!(mid
        .iter()
        .any(|b| b.binding_id == target && b.world_sheet_entry_id.as_deref() == Some("kb_race")));

    remove_binding(&pool, OWNER, &character_id, &target)
        .await
        .unwrap();

    let kb_race_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id = 'kb_race'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        kb_race_count, 1,
        "WorldSheet kb_race must survive binding delete (not binding-owned)"
    );

    let after_link_then_remove = list_bindings_for_character(&pool, OWNER, &character_id, 100, 0)
        .await
        .unwrap();
    assert_eq!(after_link_then_remove.len(), 1);
    assert_eq!(after_link_then_remove[0].binding_id, primary);
    assert_eq!(after_link_then_remove[0].world_id, WORLD_A);
    let target_row: Option<String> =
        sqlx::query_scalar("SELECT binding_id FROM actor_world_bindings WHERE binding_id = ?")
            .bind(&target)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(target_row.is_none(), "target binding must be deleted");

    // Subtest 2: remove the other non-last binding first, then link the survivor.
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let (character_id, primary, target) = seed_two_binding_link_fixture(&pool).await;

    remove_binding(&pool, OWNER, &character_id, &primary)
        .await
        .unwrap();

    let after_remove = list_bindings_for_character(&pool, OWNER, &character_id, 100, 0)
        .await
        .unwrap();
    assert_eq!(after_remove.len(), 1);
    assert_eq!(after_remove[0].binding_id, target);
    assert_eq!(after_remove[0].world_id, WORLD_B);

    let linked = update_actor_world_binding(
        &pool,
        OWNER,
        &character_id,
        &target,
        0,
        FieldPatch::Set("kb_race"),
    )
    .await
    .unwrap();
    assert_eq!(linked.binding_id, target);
    assert_eq!(linked.world_sheet_entry_id.as_deref(), Some("kb_race"));
    assert_eq!(linked.revision, 1);

    let final_bindings = list_bindings_for_character(&pool, OWNER, &character_id, 100, 0)
        .await
        .unwrap();
    assert_eq!(final_bindings.len(), 1);
    assert_eq!(final_bindings[0].binding_id, target);
    assert_eq!(final_bindings[0].world_id, WORLD_B);
    assert_eq!(
        final_bindings[0].world_sheet_entry_id.as_deref(),
        Some("kb_race")
    );

    // Subtest 3: remove target first, then link the deleted binding — remove wins.
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let (character_id, primary, target) = seed_two_binding_link_fixture(&pool).await;
    let primary_sheet_before: Option<String> = sqlx::query_scalar(
        "SELECT world_sheet_entry_id FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(&primary)
    .fetch_one(&pool)
    .await
    .unwrap();

    remove_binding(&pool, OWNER, &character_id, &target)
        .await
        .unwrap();

    let err = update_actor_world_binding(
        &pool,
        OWNER,
        &character_id,
        &target,
        0,
        FieldPatch::Set("kb_race"),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorNotFound {
            resource: "actor_world_binding",
            ..
        }
    ));

    let target_row: Option<String> =
        sqlx::query_scalar("SELECT binding_id FROM actor_world_bindings WHERE binding_id = ?")
            .bind(&target)
            .fetch_optional(&pool)
            .await
            .unwrap();
    assert!(target_row.is_none(), "removed target must stay absent");

    let survivors = list_bindings_for_character(&pool, OWNER, &character_id, 100, 0)
        .await
        .unwrap();
    assert_eq!(survivors.len(), 1);
    assert_eq!(survivors[0].binding_id, primary);
    assert_eq!(
        survivors[0].world_sheet_entry_id.as_deref(),
        primary_sheet_before.as_deref()
    );

    let kb_race_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM kb_key_blocks WHERE key_block_id = 'kb_race'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(kb_race_count, 1, "kb_race WorldSheet row must remain");
}

#[tokio::test]
async fn add_binding_rejects_paused_world_with_zero_mutation() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let created = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "PausedAdd",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap();
    sqlx::query("UPDATE narrative_worlds SET status = 'paused' WHERE world_id = ?")
        .bind(WORLD_B)
        .execute(&pool)
        .await
        .unwrap();

    let before: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ?")
            .bind(&created.character.character_id)
            .fetch_one(&pool)
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
        LocalDbError::ActorNotFound {
            resource: "world",
            ..
        }
    ));

    let after: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings WHERE character_id = ?")
            .bind(&created.character.character_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, before, "paused world add must not mutate bindings");
}

#[tokio::test]
async fn create_character_rejects_paused_world_with_zero_mutation() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    sqlx::query("UPDATE narrative_worlds SET status = 'paused' WHERE world_id = ?")
        .bind(WORLD_A)
        .execute(&pool)
        .await
        .unwrap();

    let chars_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&pool)
        .await
        .unwrap();
    let binds_before: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();

    let err = create_character_with_initial_binding(
        &pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name: "PausedCreate",
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(
        err,
        LocalDbError::ActorNotFound {
            resource: "world",
            ..
        }
    ));

    let chars_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM characters")
        .fetch_one(&pool)
        .await
        .unwrap();
    let binds_after: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(chars_after, chars_before);
    assert_eq!(binds_after, binds_before);
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

// ── v1.191 P1 T4 — holder lifecycle provisioning ───────────────────────
//
// Durable contract: `.mstar/specs/holder-governance.md` §2.1 (subject and
// registry row commit together; one stable holder per stored identity) and
// §2.2 (identity stability across rename/archive/restore, retained archived
// reads, fail-closed missing/corrupt registry on a normal read,
// unreferenced-only Actor deletion).

/// Count the registry rows of a subject kind, so "no row was provisioned or
/// duplicated" is asserted on the storage itself.
async fn holder_rows(pool: &SqlitePool, column: &str, subject_id: &str) -> i64 {
    sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT COUNT(*) FROM knowledge_holders WHERE {column} = ?"
    )))
    .bind(subject_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn seed_character(pool: &SqlitePool, display_name: &str) -> String {
    create_character_with_initial_binding(
        pool,
        CreateCharacterParams {
            owner_creator_id: OWNER,
            display_name,
            image_uri: None,
            persona_json: "{}",
            world_id: WORLD_A,
            world_sheet_entry_id: None,
        },
    )
    .await
    .unwrap()
    .character
    .character_id
}

#[tokio::test]
async fn v1191_holder_lifecycle_creator_materialization_is_holder_stable() {
    let (pool_a, _dir_a) = fresh_pool().await;
    let (pool_b, _dir_b) = fresh_pool().await;

    // Two independently materialized workspaces of the same global Creator.
    ensure_creator_row(&pool_a, OWNER, "Owner").await.unwrap();
    ensure_creator_row(&pool_b, OWNER, "Owner Renamed")
        .await
        .unwrap();

    let holder_a = require_creator_holder(&pool_a, OWNER).await.unwrap();
    let holder_b = require_creator_holder(&pool_b, OWNER).await.unwrap();
    assert_eq!(holder_a, creator_holder_entry_id(OWNER));
    assert_eq!(holder_a, holder_b, "one identity resolves one holder id");
    assert_eq!(holder_rows(&pool_a, "creator_id", OWNER).await, 1);
    assert_eq!(holder_rows(&pool_b, "creator_id", OWNER).await, 1);

    // Re-materializing (rename/repair) keeps the same holder and one row.
    ensure_creator_row(&pool_a, OWNER, "Owner Renamed Again")
        .await
        .unwrap();
    assert_eq!(
        require_creator_holder(&pool_a, OWNER).await.unwrap(),
        holder_a
    );
    assert_eq!(holder_rows(&pool_a, "creator_id", OWNER).await, 1);

    // The subject-addressed read agrees with the id-addressed read.
    assert_eq!(
        resolve_subject_holder(&pool_a, &HolderSubject::Creator(OWNER.to_string()))
            .await
            .unwrap()
            .as_deref(),
        Some(holder_a.as_str())
    );
    // An unmaterialized Creator is absent, not a minted id.
    assert!(
        resolve_subject_holder(&pool_a, &HolderSubject::Creator("ctr_absent".to_string()))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test]
async fn v1191_holder_lifecycle_character_create_rename_archive_restore_keep_one_holder() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let character_id = seed_character(&pool, "Ava").await;

    let created = require_character_holder(&pool, OWNER, &character_id)
        .await
        .unwrap();
    assert_eq!(created, character_holder_entry_id(&character_id));
    assert_eq!(holder_rows(&pool, "character_id", &character_id).await, 1);

    // Rename: the projected label moves, the holder does not.
    let renamed = update_character(
        &pool,
        OWNER,
        &character_id,
        0,
        CharacterPatch {
            display_name: Some("Ava Renamed"),
            image_uri: FieldPatch::Keep,
            persona_json: FieldPatch::Keep,
        },
    )
    .await
    .unwrap();
    assert_eq!(renamed.display_name, "Ava Renamed");
    assert_eq!(
        require_character_holder(&pool, OWNER, &character_id)
            .await
            .unwrap(),
        created
    );

    // Archive increments the lifecycle epoch and retains the holder.
    let archived = transition_character(
        &pool,
        OWNER,
        &character_id,
        renamed.revision,
        CharacterStatus::Archived,
    )
    .await
    .unwrap();
    assert_eq!(archived.status, "archived");
    assert!(archived.lifecycle_epoch > renamed.lifecycle_epoch);
    assert_eq!(
        require_character_holder(&pool, OWNER, &character_id)
            .await
            .unwrap(),
        created,
        "an archived Character keeps its holder"
    );

    // Retained archived read: the detail row still answers, foreign owners do not.
    let retained = get_character(&pool, OWNER, &character_id).await.unwrap();
    assert_eq!(retained.map(|row| row.status), Some("archived".to_string()));
    assert!(matches!(
        require_character_holder(&pool, OTHER, &character_id)
            .await
            .unwrap_err(),
        LocalDbError::ActorNotFound { .. }
    ));

    // Restore reuses the same holder and the same epoch rules.
    let restored = transition_character(
        &pool,
        OWNER,
        &character_id,
        archived.revision,
        CharacterStatus::Active,
    )
    .await
    .unwrap();
    assert_eq!(restored.status, "active");
    assert!(restored.lifecycle_epoch > archived.lifecycle_epoch);
    assert_eq!(
        require_character_holder(&pool, OWNER, &character_id)
            .await
            .unwrap(),
        created
    );
    assert_eq!(holder_rows(&pool, "character_id", &character_id).await, 1);
}

#[tokio::test]
async fn v1191_holder_lifecycle_missing_registry_read_fails_without_provisioning() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let character_id = seed_character(&pool, "Ava").await;

    // Corrupt the registry by removing the Character's row behind the API (the
    // only reachable way to break the subject+holder invariant).
    sqlx::query("DELETE FROM knowledge_holders WHERE character_id = ?")
        .bind(&character_id)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(holder_rows(&pool, "character_id", &character_id).await, 0);

    let err = require_character_holder(&pool, OWNER, &character_id)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, LocalDbError::HolderStateInvalid { reason }
            if reason.contains("never provisions")),
        "expected holder_state_invalid, got {err:?}"
    );
    // The read may not repair the state, and the missing Registry must not
    // become a second (minted) holder.
    assert_eq!(holder_rows(&pool, "character_id", &character_id).await, 0);
    assert!(
        resolve_subject_holder(&pool, &HolderSubject::Character(character_id.clone()))
            .await
            .unwrap()
            .is_none()
    );

    // Rename/archive fail closed on the same state instead of writing an
    // identity that has no holder.
    assert!(matches!(
        update_character(
            &pool,
            OWNER,
            &character_id,
            0,
            CharacterPatch {
                display_name: Some("Ava"),
                image_uri: FieldPatch::Keep,
                persona_json: FieldPatch::Keep,
            },
        )
        .await
        .unwrap_err(),
        LocalDbError::HolderStateInvalid { .. }
    ));
    assert!(matches!(
        transition_character(&pool, OWNER, &character_id, 0, CharacterStatus::Archived)
            .await
            .unwrap_err(),
        LocalDbError::HolderStateInvalid { .. }
    ));
    // The Character row itself is untouched by either refusal.
    assert_eq!(
        get_character(&pool, OWNER, &character_id)
            .await
            .unwrap()
            .map(|row| (row.status, row.revision)),
        Some(("active".to_string(), 0))
    );

    // A non-deriving registry row is corrupt state, not a holder.
    sqlx::query(
        "INSERT INTO knowledge_holders (holder_entry_id, character_id, created_at) \
         VALUES ('hld_foreign', ?, datetime('now'))",
    )
    .bind(&character_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        require_character_holder(&pool, OWNER, &character_id)
            .await
            .unwrap_err(),
        LocalDbError::HolderStateInvalid { .. }
    ));
}

#[tokio::test]
async fn v1191_holder_lifecycle_referenced_character_delete_refuses_without_mutation() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let character_id = seed_character(&pool, "Ava").await;
    let holder = require_character_holder(&pool, OWNER, &character_id)
        .await
        .unwrap();

    // The active binding refuses deletion (zero mutation).
    assert!(matches!(
        delete_character(&pool, OWNER, &character_id)
            .await
            .unwrap_err(),
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::ActorInUse
        }
    ));
    assert!(get_character(&pool, OWNER, &character_id)
        .await
        .unwrap()
        .is_some());
    assert_eq!(holder_rows(&pool, "character_id", &character_id).await, 1);

    // A governance reference (a row governed by this Character's holder)
    // refuses deletion even after the binding is gone.
    sqlx::query("DELETE FROM actor_world_bindings WHERE character_id = ?")
        .bind(&character_id)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, owner_kind, world_id, character_id, block_type, canonical_name, status, \
          body_json, created_at, holder_entry_id, disclosure) \
         VALUES ('kb_private', 'character', NULL, ?, 'character', 'private', 'confirmed', '{}', \
                 datetime('now'), ?, 'owner-private')",
    )
    .bind(&character_id)
    .bind(&holder)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        delete_character(&pool, OWNER, &character_id)
            .await
            .unwrap_err(),
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::ActorInUse
        }
    ));
    assert_eq!(holder_rows(&pool, "character_id", &character_id).await, 1);

    // Retained Character-owned rows refuse deletion too (independent of the
    // governance pair).
    sqlx::query("DELETE FROM kb_key_blocks WHERE key_block_id = 'kb_private'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO character_soul_meta \
         (character_id, file_path, schema_version, created_at, updated_at) \
         VALUES (?, 'SOUL.md', 1, datetime('now'), datetime('now'))",
    )
    .bind(&character_id)
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        delete_character(&pool, OWNER, &character_id)
            .await
            .unwrap_err(),
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::ActorInUse
        }
    ));

    // Foreign owners cannot delete the subject at all.
    assert!(matches!(
        delete_character(&pool, OTHER, &character_id)
            .await
            .unwrap_err(),
        LocalDbError::ActorNotFound { .. }
    ));
}

#[tokio::test]
async fn v1191_holder_lifecycle_unreferenced_character_delete_removes_subject_and_holder() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let character_id = seed_character(&pool, "Ava").await;
    let holder = require_character_holder(&pool, OWNER, &character_id)
        .await
        .unwrap();

    // The last-active-binding rule keeps a Character bound in production, so
    // the unreferenced state is constructed directly (a Character whose
    // bindings and retained rows are gone) to exercise the removal path.
    sqlx::query("DELETE FROM actor_world_bindings WHERE character_id = ?")
        .bind(&character_id)
        .execute(&pool)
        .await
        .unwrap();

    delete_character(&pool, OWNER, &character_id).await.unwrap();

    assert!(get_character(&pool, OWNER, &character_id)
        .await
        .unwrap()
        .is_none());
    assert!(nexus_local_db::resolve_holder(&pool, &holder)
        .await
        .unwrap()
        .is_none());
    assert_eq!(holder_rows(&pool, "character_id", &character_id).await, 0);
}

#[tokio::test]
async fn v1191_holder_lifecycle_referenced_creator_delete_refuses_and_unreferenced_is_atomic() {
    let (pool, _dir) = fresh_pool().await;
    seed_creator_and_worlds(&pool).await;
    let holder = require_creator_holder(&pool, OWNER).await.unwrap();

    // An owned World would be CASCADE-deleted by the inherited FK; the guard
    // refuses first, so governed rows cannot be dropped silently.
    assert!(matches!(
        delete_creator(&pool, OWNER).await.unwrap_err(),
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::ActorInUse
        }
    ));
    assert_eq!(holder_rows(&pool, "creator_id", OWNER).await, 1);
    let worlds: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM narrative_worlds")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(worlds, 2, "the refusal cascades nothing");

    // Character-owned rows and a governance reference refuse as well.
    let character_id = seed_character(&pool, "Ava").await;
    assert!(matches!(
        delete_creator(&pool, OWNER).await.unwrap_err(),
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::ActorInUse
        }
    ));

    // A Creator with no Worlds/Characters but a governed row still refuses.
    ensure_creator_row(&pool, OTHER, "Other").await.unwrap();
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, block_type, canonical_name, status, body_json, created_at, \
          holder_entry_id, disclosure) \
         VALUES ('kb_other_private', ?, 'world', 'private', 'confirmed', '{}', datetime('now'), \
                 ?, 'owner-private')",
    )
    .bind(WORLD_B)
    .bind(creator_holder_entry_id(OTHER))
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        delete_creator(&pool, OTHER).await.unwrap_err(),
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::ActorInUse
        }
    ));
    sqlx::query("DELETE FROM kb_key_blocks WHERE key_block_id = 'kb_other_private'")
        .execute(&pool)
        .await
        .unwrap();

    // Unreferenced: holder and subject go together.
    delete_creator(&pool, OTHER).await.unwrap();
    assert_eq!(holder_rows(&pool, "creator_id", OTHER).await, 0);
    let creators: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM creators WHERE creator_id = ?")
        .bind(OTHER)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(creators, 0);
    assert!(
        nexus_local_db::resolve_holder(&pool, &creator_holder_entry_id(OTHER))
            .await
            .unwrap()
            .is_none()
    );

    // The Character (and its holder) of the still-referenced Creator is intact.
    assert!(matches!(
        delete_creator(&pool, OWNER).await.unwrap_err(),
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::ActorInUse
        }
    ));
    assert_eq!(
        require_character_holder(&pool, OWNER, &character_id)
            .await
            .unwrap(),
        character_holder_entry_id(&character_id)
    );
    assert_eq!(
        require_creator_holder(&pool, OWNER).await.unwrap(),
        holder,
        "removing another Creator leaves this registry row untouched"
    );
}
