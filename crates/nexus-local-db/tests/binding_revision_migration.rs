//! v1.185 P1 Task 1 — `actor_world_bindings.revision` upgrade fidelity.
//!
//! Covers migration `20260906000002_binding_revision.sql` on a populated
//! pre-revision (schema v21) database.

#![allow(clippy::unwrap_used)]

use nexus_local_db::{read_versions, seed_versions, DB_SCHEMA_VERSION};
use sqlx::migrate::{Migration, Migrator};
use sqlx::SqlitePool;

const BINDING_REVISION_MIGRATION_VERSION: i64 = 20_260_906_000_002;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const CHARACTER: &str = "chr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORLD: &str = "wld_worldA";
const BINDING: &str = "awb_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const SHEET: &str = "kb_linked_sheet";
const CREATED_AT: &str = "2026-08-01T00:00:00Z";

async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}?mode=rwc", dir.path().join("test.db").display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::query("PRAGMA foreign_keys = ON")
        .execute(&pool)
        .await
        .unwrap();
    (pool, dir)
}

fn pre_binding_revision_migrator() -> Migrator {
    let full = sqlx::migrate!("./migrations");
    let pre: Vec<Migration> = full
        .migrations
        .iter()
        .filter(|m| m.version < BINDING_REVISION_MIGRATION_VERSION)
        .cloned()
        .collect();
    Migrator::with_migrations(pre)
}

async fn run_migrator(pool: &SqlitePool, migrator: Migrator) {
    let mut conn = pool.acquire().await.unwrap();
    migrator.run_direct(None, &mut *conn, false).await.unwrap();
}

async fn seed_v21_fixture(pool: &SqlitePool) {
    nexus_local_db::ensure_creator_row(pool, CREATOR, "Owner")
        .await
        .unwrap();
    sqlx::query(
        "INSERT INTO narrative_worlds \
         (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
          time_policy, metadata_json, created_at) \
         VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', ?)",
    )
    .bind(WORLD)
    .bind(CREATOR)
    .bind(WORLD)
    .bind(WORLD)
    .bind(CREATED_AT)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO characters \
         (character_id, owner_creator_id, display_name, status, image_uri, persona_json, \
          revision, lifecycle_epoch, created_at, updated_at) \
         VALUES (?, ?, 'Hero', 'active', NULL, '{}', 0, 0, ?, ?)",
    )
    .bind(CHARACTER)
    .bind(CREATOR)
    .bind(CREATED_AT)
    .bind(CREATED_AT)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO kb_key_blocks \
         (key_block_id, world_id, owner_kind, block_type, canonical_name, status, body_json, \
          creator_only, created_at) \
         VALUES (?, ?, 'world', 'character', 'sheet', 'confirmed', '{}', 0, ?)",
    )
    .bind(SHEET)
    .bind(WORLD)
    .bind(CREATED_AT)
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO actor_world_bindings \
         (binding_id, character_id, world_id, status, world_sheet_entry_id, created_at, updated_at) \
         VALUES (?, ?, ?, 'active', ?, ?, ?)",
    )
    .bind(BINDING)
    .bind(CHARACTER)
    .bind(WORLD)
    .bind(SHEET)
    .bind(CREATED_AT)
    .bind(CREATED_AT)
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn binding_revision_migration_upgrades_populated_v21_db() {
    let (pool, _dir) = fresh_pool().await;
    run_migrator(&pool, pre_binding_revision_migrator()).await;
    seed_v21_fixture(&pool).await;

    let pre_count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM actor_world_bindings")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(pre_count, 1);

    nexus_local_db::run_migrations(&pool).await.unwrap();
    seed_versions(&pool).await.unwrap();

    let row: (String, String, String, String, Option<String>, i64) = sqlx::query_as(
        "SELECT binding_id, character_id, world_id, status, world_sheet_entry_id, revision \
         FROM actor_world_bindings WHERE binding_id = ?",
    )
    .bind(BINDING)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            BINDING.to_string(),
            CHARACTER.to_string(),
            WORLD.to_string(),
            "active".to_string(),
            Some(SHEET.to_string()),
            0
        )
    );

    let versions = read_versions(&pool).await.unwrap();
    assert_eq!(versions.db_schema_version, DB_SCHEMA_VERSION);
}
