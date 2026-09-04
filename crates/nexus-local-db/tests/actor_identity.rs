//! P0 Task 2 — Character / ActorWorldBinding store transaction proofs.

#![allow(clippy::unwrap_used)]

use nexus_local_db::{
    add_actor_world_binding, create_character_with_initial_binding, get_character,
    list_bindings_for_character, mint_character_id, remove_binding, ActorContractConflict,
    CreateBindingParams, CreateCharacterParams, LocalDbError,
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
    assert!(matches!(
        err,
        LocalDbError::ActorContractConflict {
            code: ActorContractConflict::DuplicateActiveBinding
        }
    ));
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

    let remaining = list_bindings_for_character(&pool, OWNER, &created.character.character_id)
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
    let err = if r1.is_err() {
        r1.unwrap_err()
    } else {
        r2.unwrap_err()
    };
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
    assert!(id[4..].chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
}
