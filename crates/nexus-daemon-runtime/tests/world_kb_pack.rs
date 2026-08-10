//! V1.152 P0 — World KB pack export/import Daemon HTTP integration tests.
//!
//! Proves the Narrative Knowledge Pack export/import surfaces end-to-end over a
//! real `axum` router + `SQLite`:
//!
//! - Export: owned World → 200 pack envelope; foreign World → 403.
//! - Import skip: cross-world import + idempotent re-import.
//! - Import rename / overwrite conflict policies.
//! - Ownership reject on import.
//! - `pack_import` provenance stamp on imported rows.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::pack_import::IMPORT_PROVENANCE;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};
use std::collections::HashMap;

/// World owned by `test_creator` (seeded by `seed_test_creator_and_world`).
const OWNED_WORLD: &str = "wld_test_world";
/// Second world owned by `test_creator` — import target.
const TARGET_WORLD: &str = "wld_import_target";
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
    seed_import_target_world(&pool).await;
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

async fn seed_import_target_world(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed against the known narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES (?, 'ws', 'test_creator', 'Import Target', 'import-target', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(TARGET_WORLD)
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
    seed_key_block_with_body(
        pool,
        key_block_id,
        world_id,
        canonical_name,
        "{}",
        "confirmed",
    )
    .await;
}

async fn seed_key_block_with_body(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    canonical_name: &str,
    body_json: &str,
    status: &str,
) {
    // SAFETY: test-only seed against the known kb_key_blocks schema.
    sqlx::query(
        "INSERT OR IGNORE INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, \
             created_at, updated_at) \
           VALUES (?, ?, 'character', ?, ?, 0, ?, datetime('now'), datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(canonical_name)
    .bind(status)
    .bind(body_json)
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

fn import_url(world_id: &str) -> String {
    format!("/v1/daemon/worlds/{world_id}/kb/pack/import")
}

/// Same-DB cross-world import cannot reuse `entry_ids` owned by the source world.
/// Mint fresh ids (and remap relation endpoints) so import exercises create paths.
fn fresh_entry_ids_in_pack(pack: &mut Value) {
    let entries = pack
        .as_object_mut()
        .and_then(|obj| obj.get_mut("entries"))
        .and_then(|v| v.as_array_mut())
        .expect("pack entries array");
    let mut id_map = HashMap::new();
    for (idx, entry) in entries.iter_mut().enumerate() {
        let old_id = entry
            .get("entry_id")
            .and_then(|v| v.as_str())
            .expect("entry_id")
            .to_string();
        let new_id = format!("kb_import_test_{idx:03}");
        entry["entry_id"] = json!(new_id);
        id_map.insert(old_id, new_id);
    }
    let relations = pack
        .as_object_mut()
        .and_then(|obj| obj.get_mut("relations"))
        .and_then(|v| v.as_array_mut())
        .expect("pack relations array");
    for (idx, relation) in relations.iter_mut().enumerate() {
        relation["relation_id"] = json!(format!("rel_import_test_{idx:03}"));
        if let Some(from) = relation.get("from_id").and_then(|v| v.as_str()) {
            if let Some(mapped) = id_map.get(from) {
                relation["from_id"] = json!(mapped);
            }
        }
        if let Some(to) = relation.get("to_id").and_then(|v| v.as_str()) {
            if let Some(mapped) = id_map.get(to) {
                relation["to_id"] = json!(mapped);
            }
        }
    }
}

async fn export_pack(server: &TestServer, world_id: &str) -> Value {
    let resp = server.post(&export_url(world_id)).json(&json!({})).await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    resp.json()
}

async fn import_pack_http(
    server: &TestServer,
    world_id: &str,
    pack: &Value,
    conflict: &str,
) -> (StatusCode, Value) {
    let resp = server
        .post(&import_url(world_id))
        .json(&json!({
            "pack": pack,
            "conflict": conflict,
        }))
        .await;
    let status = resp.status_code();
    let body: Value = resp.json();
    (status, body)
}

async fn seed_export_source_world(pool: &sqlx::SqlitePool) {
    seed_key_block_with_body(
        pool,
        "kb_pack_a",
        OWNED_WORLD,
        "Aria",
        r#"{"summary":"Aria summary"}"#,
        "confirmed",
    )
    .await;
    seed_key_block_with_body(
        pool,
        "kb_pack_b",
        OWNED_WORLD,
        "Kael",
        r#"{"summary":"Kael from pack"}"#,
        "confirmed",
    )
    .await;
    seed_key_block_with_body(
        pool,
        "kb_pack_c",
        OWNED_WORLD,
        "Mira",
        r#"{"summary":"Mira summary"}"#,
        "confirmed",
    )
    .await;
    seed_relation(pool, "rel_pack_1", OWNED_WORLD, "kb_pack_a", "kb_pack_b").await;
}

async fn assert_entry_provenance(pool: &sqlx::SqlitePool, world_id: &str, canonical_name: &str) {
    // SAFETY: test-only SELECT against known kb_key_blocks schema.
    let provenance: Option<String> = sqlx::query_scalar(
        "SELECT source_provenance_kind FROM kb_key_blocks WHERE world_id = ? AND canonical_name = ?",
    )
    .bind(world_id)
    .bind(canonical_name)
    .fetch_optional(pool)
    .await
    .unwrap()
    .flatten();
    assert_eq!(
        provenance.as_deref(),
        Some(IMPORT_PROVENANCE),
        "entry {canonical_name} in {world_id} must have pack_import provenance"
    );
}

#[tokio::test]
async fn pack_export_owned_world_returns_pack_envelope() {
    let ctx = ctx().await;
    seed_key_block(&ctx.pool, "kb_pack_a", OWNED_WORLD, "Aria").await;
    seed_key_block(&ctx.pool, "kb_pack_b", OWNED_WORLD, "Kael").await;
    seed_key_block(&ctx.pool, "kb_pack_c", OWNED_WORLD, "Mira").await;
    seed_relation(
        &ctx.pool,
        "rel_pack_1",
        OWNED_WORLD,
        "kb_pack_a",
        "kb_pack_b",
    )
    .await;

    let body = export_pack(&ctx.server, OWNED_WORLD).await;
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
        !relations.is_empty(),
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
    assert_eq!(
        resp.status_code(),
        StatusCode::FORBIDDEN,
        "body={}",
        resp.text()
    );

    let body: Value = resp.json();
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], "forbidden", "body={body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn pack_import_skip_cross_world_and_reimport_is_idempotent() {
    let ctx = ctx().await;
    seed_export_source_world(&ctx.pool).await;

    let mut pack = export_pack(&ctx.server, OWNED_WORLD).await;
    fresh_entry_ids_in_pack(&mut pack);

    let (status, body) = import_pack_http(&ctx.server, TARGET_WORLD, &pack, "skip").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(
        body["entries"]["created"].as_u64().unwrap_or(0) >= 2,
        "expected created entries >= 2: {body}"
    );
    assert!(
        body["relations"]["created"].as_u64().unwrap_or(0) >= 1,
        "expected created relations >= 1: {body}"
    );

    let (status2, body2) = import_pack_http(&ctx.server, TARGET_WORLD, &pack, "skip").await;
    assert_eq!(status2, StatusCode::OK, "body={body2}");
    assert_eq!(
        body2["entries"]["created"].as_u64().unwrap_or(0),
        0,
        "re-import must be idempotent: {body2}"
    );

    assert_entry_provenance(&ctx.pool, TARGET_WORLD, "Aria").await;
    assert_entry_provenance(&ctx.pool, TARGET_WORLD, "Kael").await;
    assert_entry_provenance(&ctx.pool, TARGET_WORLD, "Mira").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pack_import_rename_creates_disambiguated_entry() {
    let ctx = ctx().await;
    seed_export_source_world(&ctx.pool).await;
    let mut pack = export_pack(&ctx.server, OWNED_WORLD).await;
    fresh_entry_ids_in_pack(&mut pack);

    seed_key_block_with_body(
        &ctx.pool,
        "kb_target_kael",
        TARGET_WORLD,
        "Kael",
        r#"{"summary":"Pre-existing Kael"}"#,
        "confirmed",
    )
    .await;

    let (status, body) = import_pack_http(&ctx.server, TARGET_WORLD, &pack, "rename").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(body["entries"]["renamed"].as_u64().unwrap_or(0) >= 1);

    let names: Vec<String> = sqlx::query_scalar(
        "SELECT canonical_name FROM kb_key_blocks WHERE world_id = ? ORDER BY canonical_name",
    )
    .bind(TARGET_WORLD)
    .fetch_all(&ctx.pool)
    .await
    .unwrap();
    assert!(names.iter().any(|n| n.contains("imported")));
    assert_eq!(
        names.len(),
        4,
        "Aria + Mira + pre-existing Kael + renamed Kael"
    );

    let renamed_kael_id: String = sqlx::query_scalar(
        "SELECT key_block_id FROM kb_key_blocks WHERE world_id = ? AND canonical_name LIKE '%imported%'",
    )
    .bind(TARGET_WORLD)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    let imported_aria_id: String = sqlx::query_scalar(
        "SELECT key_block_id FROM kb_key_blocks WHERE world_id = ? AND canonical_name = 'Aria'",
    )
    .bind(TARGET_WORLD)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();

    let row: (String, String) = sqlx::query_as(
        "SELECT source_entity_id, target_entity_id FROM kb_relationships WHERE world_id = ? LIMIT 1",
    )
    .bind(TARGET_WORLD)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(row.0, imported_aria_id);
    assert_eq!(row.1, renamed_kael_id);

    assert_entry_provenance(&ctx.pool, TARGET_WORLD, "Aria").await;
    assert_entry_provenance(&ctx.pool, TARGET_WORLD, "Mira").await;
    let renamed_name = names.iter().find(|n| n.contains("imported")).unwrap();
    assert_entry_provenance(&ctx.pool, TARGET_WORLD, renamed_name).await;
    let preexisting: Option<String> = sqlx::query_scalar(
        "SELECT source_provenance_kind FROM kb_key_blocks WHERE world_id = ? AND key_block_id = 'kb_target_kael'",
    )
    .bind(TARGET_WORLD)
    .fetch_optional(&ctx.pool)
    .await
    .unwrap()
    .flatten();
    assert_ne!(
        preexisting.as_deref(),
        Some(IMPORT_PROVENANCE),
        "pre-seeded collision row must not be stamped"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn pack_import_overwrite_replaces_body_preserves_status() {
    let ctx = ctx().await;
    seed_export_source_world(&ctx.pool).await;
    let mut pack = export_pack(&ctx.server, OWNED_WORLD).await;
    fresh_entry_ids_in_pack(&mut pack);

    seed_key_block_with_body(
        &ctx.pool,
        "kb_target_kael",
        TARGET_WORLD,
        "Kael",
        r#"{"summary":"Pre-existing Kael body"}"#,
        "provisional",
    )
    .await;

    let (status, body) = import_pack_http(&ctx.server, TARGET_WORLD, &pack, "overwrite").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(
        body["entries"]["overwritten"].as_u64().unwrap_or(0) >= 1,
        "expected overwritten entries >= 1: {body}"
    );

    // SAFETY: test-only SELECT against known kb_key_blocks schema.
    let row: (String, String) = sqlx::query_as(
        "SELECT status, body_json FROM kb_key_blocks \
         WHERE world_id = ? AND canonical_name = 'Kael'",
    )
    .bind(TARGET_WORLD)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert_eq!(
        row.0, "provisional",
        "overwrite must preserve target provisional status, not pack confirmed"
    );
    assert!(
        row.1.contains("Kael from pack"),
        "overwrite must replace body with pack content; got body_json={}",
        row.1
    );

    assert_entry_provenance(&ctx.pool, TARGET_WORLD, "Aria").await;
    assert_entry_provenance(&ctx.pool, TARGET_WORLD, "Kael").await;
    assert_entry_provenance(&ctx.pool, TARGET_WORLD, "Mira").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn pack_import_same_world_reimport_overwrite_updates_body() {
    let ctx = ctx().await;
    seed_export_source_world(&ctx.pool).await;

    let pack = export_pack(&ctx.server, OWNED_WORLD).await;

    // Stale Kael body before same-world re-import.
    sqlx::query(
        "UPDATE kb_key_blocks SET body_json = ? WHERE world_id = ? AND key_block_id = 'kb_pack_b'",
    )
    .bind(r#"{"summary":"Stale Kael body"}"#)
    .bind(OWNED_WORLD)
    .execute(&ctx.pool)
    .await
    .unwrap();

    let (status, body) = import_pack_http(&ctx.server, OWNED_WORLD, &pack, "overwrite").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert!(
        body["entries"]["overwritten"].as_u64().unwrap_or(0) >= 1,
        "same-world re-import must overwrite, not skip: {body}"
    );

    let row: (String,) = sqlx::query_as(
        "SELECT body_json FROM kb_key_blocks WHERE world_id = ? AND key_block_id = 'kb_pack_b'",
    )
    .bind(OWNED_WORLD)
    .fetch_one(&ctx.pool)
    .await
    .unwrap();
    assert!(
        row.0.contains("Kael from pack"),
        "overwrite must restore pack body on same-world re-import; got body_json={}",
        row.0
    );
}

#[tokio::test]
async fn pack_import_foreign_world_returns_403() {
    let ctx = ctx().await;
    seed_export_source_world(&ctx.pool).await;
    let mut pack = export_pack(&ctx.server, OWNED_WORLD).await;
    fresh_entry_ids_in_pack(&mut pack);

    let (status, body) = import_pack_http(&ctx.server, FOREIGN_WORLD, &pack, "skip").await;
    assert_eq!(status, StatusCode::FORBIDDEN, "body={body}");
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], "forbidden", "body={body}");
}
