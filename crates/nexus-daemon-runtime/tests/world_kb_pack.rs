//! V1.152 P0 — `POST /v1/daemon/worlds/:world_id/kb/pack/export` integration tests.
//!
//! Proves the Narrative Knowledge Pack export Daemon HTTP surface end-to-end
//! over a real `axum` router + `SQLite`:
//!
//! - Happy path (owned World; seeded entries + relation) → 200 with pack
//!   envelope (`modules.pack.title`, `entries`, `relations`).
//! - Ownership reject (World owned by a different creator) → 403.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};

/// World owned by `test_creator` (seeded by `seed_test_creator_and_world`).
const OWNED_WORLD: &str = "wld_test_world";
/// World owned by `other_creator` (ownership-gate tests).
const FOREIGN_WORLD: &str = "wld_foreign";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

async fn ctx() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    seed_foreign_world(&pool).await;
    let app = api::create_router(state, DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

async fn seed_foreign_world(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed against the known creators/narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    // SAFETY: test-only seed against the known narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES (?, 'ws', 'other_creator', 'Foreign World', 'foreign-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(FOREIGN_WORLD)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_key_block(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    canonical_name: &str,
) {
    // SAFETY: test-only seed against the known kb_key_blocks schema.
    sqlx::query(
        "INSERT OR IGNORE INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, \
             created_at, updated_at) \
           VALUES (?, ?, 'character', ?, 'confirmed', 0, '{}', datetime('now'), datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(canonical_name)
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_relation(
    pool: &sqlx::SqlitePool,
    relationship_id: &str,
    world_id: &str,
    source: &str,
    target: &str,
) {
    // SAFETY: test-only seed against the known kb_relationships schema.
    sqlx::query(
        "INSERT OR IGNORE INTO kb_relationships \
         (relationship_id, world_id, source_entity_id, target_entity_id, relation_type, \
          symmetric, confidence, source_anchor_ids, metadata, created_at, updated_at, \
          revision, needs_review, source) \
         VALUES (?, ?, ?, ?, 'mentors', 0, 1.0, '[]', '{}', \
          datetime('now'), datetime('now'), 0, 0, 'manual')",
    )
    .bind(relationship_id)
    .bind(world_id)
    .bind(source)
    .bind(target)
    .execute(pool)
    .await
    .unwrap();
}

fn export_url(world_id: &str) -> String {
    format!("/v1/daemon/worlds/{world_id}/kb/pack/export")
}

#[tokio::test]
async fn pack_export_owned_world_returns_pack_envelope() {
    let ctx = ctx().await;
    seed_key_block(&ctx.pool, "kb_pack_a", OWNED_WORLD, "Aria").await;
    seed_key_block(&ctx.pool, "kb_pack_b", OWNED_WORLD, "Kael").await;
    seed_key_block(&ctx.pool, "kb_pack_c", OWNED_WORLD, "Mira").await;
    seed_relation(&ctx.pool, "rel_pack_1", OWNED_WORLD, "kb_pack_a", "kb_pack_b").await;

    let resp = ctx
        .server
        .post(&export_url(OWNED_WORLD))
        .json(&json!({}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());

    let body: Value = resp.json();
    assert_eq!(
        body["modules"]["pack"]["title"], "Test World",
        "default title should be the world title: {body}"
    );
    let entries = body["entries"].as_array().expect("entries array");
    assert!(
        entries.len() >= 2,
        "expected at least two exported entries: {body}"
    );
    let relations = body["relations"].as_array().expect("relations array");
    assert!(
        relations.len() >= 1,
        "expected at least one exported relation: {body}"
    );
}

#[tokio::test]
async fn pack_export_foreign_world_returns_403() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post(&export_url(FOREIGN_WORLD))
        .json(&json!({}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::FORBIDDEN, "body={}", resp.text());

    let body: Value = resp.json();
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], "forbidden", "body={body}");
}
