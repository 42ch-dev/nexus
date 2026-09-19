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
//! - v1.191 P1 T10 identity safety over the wire: foreign/colliding holders
//!   held with their original governance, unknown disclosure held even when its
//!   holder is mapped, `holder_map` adoption + held-atom release, the
//!   read-only owner-only bounded review and its mutual exclusion, and the
//!   intent-scoped export.

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
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
    let server = TestServer::new(app);
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

// axum_test's AutoFuture is not Send; this helper is awaited directly by #[tokio::test], never spawned
#[allow(clippy::future_not_send)]
async fn export_pack(server: &TestServer, world_id: &str) -> Value {
    let resp = server.post(&export_url(world_id)).json(&json!({})).await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    resp.json()
}

// axum_test's AutoFuture is not Send; this helper is awaited directly by #[tokio::test], never spawned
#[allow(clippy::future_not_send)]
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
        Some("pack_import"),
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

/// Domain counters/provenance/idempotency live in nexus-core's named service
/// regression. Keep this route-level check for both retained 200 responses.
#[tokio::test(flavor = "multi_thread")]
async fn pack_import_skip_reimport_http_status() {
    let ctx = ctx().await;
    seed_export_source_world(&ctx.pool).await;
    let mut pack = export_pack(&ctx.server, OWNED_WORLD).await;
    fresh_entry_ids_in_pack(&mut pack);
    let (status, body) = import_pack_http(&ctx.server, TARGET_WORLD, &pack, "skip").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let (status, body) = import_pack_http(&ctx.server, TARGET_WORLD, &pack, "skip").await;
    assert_eq!(status, StatusCode::OK, "body={body}");
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
        Some("pack_import"),
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

// ── v1.191 P1 T10: identity safety over the wire ────────────────────────────
//
// The frozen pack schemas carry the identity-safety arms T10 implemented in
// core. These cases exercise them through the daemon router only; the domain
// scenarios (rollback, provenance, mapping admission) stay in nexus-core's
// `world_pack_services` regression.

/// POST one raw pack-import body (both frozen arms ride the same route).
// axum_test's AutoFuture is not Send; this helper is awaited directly by #[tokio::test], never spawned
#[allow(clippy::future_not_send)]
async fn post_import(server: &TestServer, world_id: &str, body: &Value) -> (StatusCode, Value) {
    let resp = server.post(&import_url(world_id)).json(body).await;
    let status = resp.status_code();
    (status, resp.json())
}

/// POST one raw pack-export body.
// axum_test's AutoFuture is not Send; this helper is awaited directly by #[tokio::test], never spawned
#[allow(clippy::future_not_send)]
async fn post_export(server: &TestServer, world_id: &str, body: &Value) -> (StatusCode, Value) {
    let resp = server.post(&export_url(world_id)).json(body).await;
    let status = resp.status_code();
    (status, resp.json())
}

/// The registered holder id of the fixture Creator — the only local identity an
/// `author-only` mapping may adopt into.
fn local_holder() -> String {
    nexus_local_db::holders::creator_holder_entry_id("test_creator")
}

/// Pack atoms carrying wire governance (`owner` / `disclosure`), the frozen
/// v1.191 P1 T10 wire shape.
fn governed_pack(entries: &[(&str, &str, Option<&str>, Option<&str>)]) -> Value {
    let entries: Vec<Value> = entries
        .iter()
        .map(|(entry_id, name, owner, disclosure)| {
            let mut entry = json!({
                "schema_version": 1,
                "entry_id": entry_id,
                "entry_type": "character",
                "canonical_name": name,
                "status": "confirmed",
                "body": { "summary": format!("{name} summary") },
                "extensions": { "nexus": { "world_id": TARGET_WORLD } }
            });
            if let Some(owner) = owner {
                entry["owner"] = json!(owner);
            }
            if let Some(disclosure) = disclosure {
                entry["disclosure"] = json!(disclosure);
            }
            entry
        })
        .collect();
    json!({
        "modules": { "pack": { "title": "Governed", "version": "0.1.0", "creator": "packAuthor" } },
        "entries": entries,
        "relations": []
    })
}

/// A pack of `count` foreign-governed atoms; every one of them is held.
fn foreign_atoms_pack(count: usize) -> Value {
    let entries: Vec<Value> = (0..count)
        .map(|idx| {
            json!({
                "schema_version": 1,
                "entry_id": format!("kb_gov_bulk_{idx:03}"),
                "entry_type": "character",
                "canonical_name": format!("Bulk Row {idx:03}"),
                "status": "confirmed",
                "body": { "summary": "bulk" },
                "owner": "hld_foreign_peer",
                "disclosure": "owner-private",
                "extensions": { "nexus": { "world_id": TARGET_WORLD } }
            })
        })
        .collect();
    json!({
        "modules": { "pack": { "title": "Bulk", "version": "0.1.0", "creator": "packAuthor" } },
        "entries": entries,
        "relations": []
    })
}

/// The held atoms of the workspace: reason, pack `entry_id` and original wire
/// owner, both projected out of the immutable original entry JSON.
async fn quarantine_rows(pool: &sqlx::SqlitePool) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as(
        "SELECT quarantine_reason, json_extract(original_entry_json, '$.entry_id'), \
                json_extract(original_entry_json, '$.owner') \
         FROM knowledge_import_quarantine ORDER BY 2",
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

/// Seed one KB row carrying native governance (`holder_entry_id`/`disclosure`).
async fn seed_governed_key_block(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
    world_id: &str,
    canonical_name: &str,
    holder_entry_id: Option<&str>,
    disclosure: Option<&str>,
) {
    // SAFETY: test-only seed against the known kb_key_blocks schema.
    sqlx::query(
        "INSERT OR IGNORE INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, revision, body_json, \
             holder_entry_id, disclosure, created_at, updated_at) \
           VALUES (?, ?, 'character', ?, 'confirmed', 0, '{}', ?, ?, datetime('now'), datetime('now'))",
    )
    .bind(key_block_id)
    .bind(world_id)
    .bind(canonical_name)
    .bind(holder_entry_id)
    .bind(disclosure)
    .execute(pool)
    .await
    .unwrap();
}

/// The stored governance pair of one KB row.
async fn stored_governance(
    pool: &sqlx::SqlitePool,
    key_block_id: &str,
) -> (Option<String>, Option<String>) {
    sqlx::query_as("SELECT holder_entry_id, disclosure FROM kb_key_blocks WHERE key_block_id = ?")
        .bind(key_block_id)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// How many KB rows the World holds for one pack `entry_id`.
async fn stored_row_count(pool: &sqlx::SqlitePool, world_id: &str, key_block_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM kb_key_blocks WHERE world_id = ? AND key_block_id = ?",
    )
    .bind(world_id)
    .bind(key_block_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// The exported canonical names of one pack envelope, in response order.
fn exported_names(envelope: &Value) -> Vec<&str> {
    envelope["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .map(|entry| {
            entry["canonical_name"]
                .as_str()
                .expect("exported canonical_name")
        })
        .collect()
}

/// Stable `qrn_<digest>` quarantine ids (durable §6).
fn assert_quarantine_id(quarantine_id: &str) {
    let digest = quarantine_id
        .strip_prefix("qrn_")
        .unwrap_or_else(|| panic!("quarantine id {quarantine_id} must carry the qrn_ prefix"));
    assert_eq!(
        digest.len(),
        64,
        "quarantine id {quarantine_id} must carry the 32-byte atom digest"
    );
    assert!(
        digest.chars().all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c)),
        "quarantine id {quarantine_id} must be lowercase hex"
    );
}

/// A foreign holder id is never adopted by string equality, and an unmapped
/// atom is held with its original wire governance instead of being stored.
#[tokio::test(flavor = "multi_thread")]
async fn v1191_holder_pack_http_unmapped_and_colliding_holders_stay_quarantined() {
    let ctx = ctx().await;
    let holder = local_holder();
    let pack = governed_pack(&[
        ("kb_gov_shared", "Shared Row", None, None),
        (
            "kb_gov_collide",
            "Colliding Row",
            Some(holder.as_str()),
            Some("owner-private"),
        ),
        (
            "kb_gov_foreign",
            "Foreign Row",
            Some("hld_foreign_peer"),
            Some("owner-private"),
        ),
    ]);

    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({ "pack": pack, "conflict": "skip" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["entries"]["created"], 1,
        "only the shared atom is storable: {body}"
    );
    assert_eq!(body["entries"]["rejected"], 2, "body={body}");

    let quarantined = body["quarantined"].as_array().expect("quarantined array");
    assert_eq!(quarantined.len(), 2, "body={body}");
    let batch_id = quarantined[0]["batch_id"]
        .as_str()
        .expect("batch id")
        .to_string();
    assert!(!batch_id.is_empty(), "body={body}");
    for atom in quarantined {
        assert_eq!(atom["reason"], "unresolved_holder", "body={body}");
        assert_quarantine_id(atom["quarantine_id"].as_str().expect("quarantine id"));
        assert_eq!(
            atom["batch_id"].as_str(),
            Some(batch_id.as_str()),
            "one batch per run: {body}"
        );
    }
    let colliding = quarantined
        .iter()
        .find(|atom| atom["entry_id"] == "kb_gov_collide")
        .expect("colliding atom held");
    assert_eq!(
        colliding["original_owner"],
        json!(holder),
        "the atom's own holder id adopts nothing"
    );
    assert_eq!(colliding["original_disclosure"], "owner-private");
    let foreign = quarantined
        .iter()
        .find(|atom| atom["entry_id"] == "kb_gov_foreign")
        .expect("foreign atom held");
    assert_eq!(foreign["original_owner"], "hld_foreign_peer");

    // The retained per-atom detail list still reports both held atoms.
    let rejected = body["details"]
        .as_array()
        .expect("details array")
        .iter()
        .filter(|detail| detail["outcome"] == "rejected")
        .count();
    assert_eq!(rejected, 2, "body={body}");

    // A held atom is not a KB row: no row at all, not even under the local
    // holder the colliding atom names by equality.
    assert_eq!(
        stored_row_count(&ctx.pool, TARGET_WORLD, "kb_gov_collide").await,
        0
    );
    assert_eq!(
        stored_row_count(&ctx.pool, TARGET_WORLD, "kb_gov_foreign").await,
        0
    );
    assert_eq!(stored_governance(&ctx.pool, "kb_gov_shared").await, (None, None));
    assert_eq!(
        quarantine_rows(&ctx.pool).await,
        vec![
            (
                "unresolved_holder".to_string(),
                "kb_gov_collide".to_string(),
                Some(holder)
            ),
            (
                "unresolved_holder".to_string(),
                "kb_gov_foreign".to_string(),
                Some("hld_foreign_peer".to_string())
            ),
        ]
    );

    // Held atoms stay out of every ordinary read: the admitted export lists the
    // shared row only.
    let exported = export_pack(&ctx.server, TARGET_WORLD).await;
    assert_eq!(
        exported_names(&exported),
        vec!["Shared Row"],
        "body={exported}"
    );
}

/// Unknown disclosure vocabulary is not native: the atom stays held even when
/// its holder is explicitly mapped, and is never rewritten to shared.
#[tokio::test(flavor = "multi_thread")]
async fn v1191_holder_pack_http_unknown_disclosure_stays_quarantined_when_mapped() {
    let ctx = ctx().await;
    let pack = governed_pack(&[(
        "kb_gov_unknown",
        "Unknown Disclosure Row",
        Some("hld_foreign_peer"),
        Some("team-shared"),
    )]);

    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({
            "pack": pack,
            "conflict": "skip",
            "holder_map": [{ "foreign_id": "hld_foreign_peer", "selector": "author-only" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["entries"]["created"], 0, "body={body}");
    let quarantined = body["quarantined"].as_array().expect("quarantined array");
    assert_eq!(quarantined.len(), 1, "body={body}");
    assert_eq!(quarantined[0]["reason"], "unknown_disclosure", "body={body}");
    assert_eq!(quarantined[0]["original_owner"], "hld_foreign_peer");
    assert_eq!(quarantined[0]["original_disclosure"], "team-shared");

    assert_eq!(
        stored_row_count(&ctx.pool, TARGET_WORLD, "kb_gov_unknown").await,
        0,
        "an unknown disclosure is never stored"
    );
    assert_eq!(
        quarantine_rows(&ctx.pool).await,
        vec![(
            "unknown_disclosure".to_string(),
            "kb_gov_unknown".to_string(),
            Some("hld_foreign_peer".to_string())
        )]
    );
}

/// `holder_map` adopts one foreign atom into the permitted local identity, in
/// one transaction with the removal of its held atom; an inadmissible mapping
/// refuses the whole import and keeps the held atom.
#[tokio::test(flavor = "multi_thread")]
async fn v1191_holder_pack_http_holder_map_adopts_and_releases_quarantine() {
    let ctx = ctx().await;
    let holder = local_holder();
    let pack = governed_pack(&[(
        "kb_gov_foreign",
        "Foreign Row",
        Some("hld_foreign_peer"),
        Some("owner-private"),
    )]);

    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({ "pack": pack, "conflict": "skip" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["quarantined"].as_array().expect("quarantined").len(),
        1,
        "body={body}"
    );
    assert_eq!(quarantine_rows(&ctx.pool).await.len(), 1);

    // A selector the authoring admission cannot resolve refuses the whole
    // import: zero atom writes, the held atom intact.
    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({
            "pack": pack,
            "conflict": "skip",
            "holder_map": [{
                "foreign_id": "hld_foreign_peer",
                "selector": "character-private:chr_missing",
            }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["error"]["code"], "invalid_input", "body={body}");
    assert_eq!(
        stored_row_count(&ctx.pool, TARGET_WORLD, "kb_gov_foreign").await,
        0
    );
    assert_eq!(
        quarantine_rows(&ctx.pool).await.len(),
        1,
        "a refused import releases nothing"
    );

    // An unknown selector spelling is input refusal on the same route.
    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({
            "pack": pack,
            "conflict": "skip",
            "holder_map": [{ "foreign_id": "hld_foreign_peer", "selector": "adopt-anything" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
    assert_eq!(body["error"]["code"], "invalid_input", "body={body}");

    // The admitted mapping adopts the atom natively and releases its held atom.
    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({
            "pack": pack,
            "conflict": "skip",
            "holder_map": [{ "foreign_id": "hld_foreign_peer", "selector": "author-only" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["entries"]["created"], 1, "body={body}");
    // The held-atom arm is optional on the wire: an adopting run reports none.
    if let Some(held) = body["quarantined"].as_array() {
        assert!(held.is_empty(), "the adopted atom releases its held row: {body}");
    }
    assert_eq!(
        stored_governance(&ctx.pool, "kb_gov_foreign").await,
        (Some(holder), Some("owner-private".to_string())),
        "the adopted atom carries the mapped local holder and its own disclosure"
    );
    assert!(
        quarantine_rows(&ctx.pool).await.is_empty(),
        "adoption removes the held atom"
    );

    // A repeat import re-creates nothing and re-quarantines nothing.
    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({
            "pack": pack,
            "conflict": "skip",
            "holder_map": [{ "foreign_id": "hld_foreign_peer", "selector": "author-only" }],
        }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(body["entries"]["created"], 0, "body={body}");
    assert_eq!(body["entries"]["skipped"], 1, "body={body}");
    assert!(quarantine_rows(&ctx.pool).await.is_empty());
}

/// The review arm is read-only and owner-only: it returns one batch's atoms with
/// their immutable original wire KE JSON to the stored controlling Creator.
#[tokio::test(flavor = "multi_thread")]
async fn v1191_holder_pack_http_review_is_owner_only() {
    let ctx = ctx().await;
    let pack = governed_pack(&[
        (
            "kb_gov_foreign",
            "Foreign Row",
            Some("hld_foreign_peer"),
            Some("owner-private"),
        ),
        (
            "kb_gov_unknown",
            "Unknown Row",
            Some("hld_unknown_peer"),
            Some("team-shared"),
        ),
    ]);
    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({ "pack": pack, "conflict": "skip" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    let batch_id = body["quarantined"][0]["batch_id"]
        .as_str()
        .expect("batch id")
        .to_string();

    let (status, review) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({ "review_import": batch_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "review={review}");
    assert!(batch_id.starts_with("pib_"), "review={review}");
    // The review arm reports zero counts and an empty detail list, exactly as
    // the retained response contract promises existing consumers.
    assert_eq!(review["entries"]["created"], 0, "review={review}");
    assert_eq!(review["entries"]["rejected"], 0, "review={review}");
    assert_eq!(review["relations"]["created"], 0, "review={review}");
    assert_eq!(review["relations"]["rejected"], 0, "review={review}");
    assert_eq!(review["details"], json!([]), "review={review}");
    // The review arm re-reports no held atoms of its own — the batch's atoms
    // are in `review` — and the arm is optional on the wire.
    if let Some(held) = review["quarantined"].as_array() {
        assert!(held.is_empty(), "review={review}");
    }
    assert_eq!(review["review"]["batch_id"], json!(batch_id), "review={review}");
    assert_eq!(review["review"]["truncated"], false, "review={review}");
    let atoms = review["review"]["atoms"]
        .as_array()
        .expect("review atoms array");
    assert_eq!(atoms.len(), 2, "review={review}");
    let foreign = atoms
        .iter()
        .find(|atom| atom["entry_id"] == "kb_gov_foreign")
        .expect("held atom is reviewable");
    assert_eq!(foreign["reason"], "unresolved_holder");
    assert_eq!(foreign["batch_id"], json!(batch_id));
    assert_eq!(foreign["original_owner"], "hld_foreign_peer");
    assert_eq!(foreign["original_disclosure"], "owner-private");
    assert_quarantine_id(foreign["quarantine_id"].as_str().expect("quarantine id"));
    assert_eq!(foreign["original_entry"]["entry_id"], "kb_gov_foreign");
    assert_eq!(foreign["original_entry"]["canonical_name"], "Foreign Row");
    assert_eq!(
        foreign["original_entry"]["owner"], "hld_foreign_peer",
        "the immutable original wire entry travels verbatim"
    );

    // The same batch is not reviewable for a World this Creator does not own.
    let (status, denied) = post_import(
        &ctx.server,
        FOREIGN_WORLD,
        &json!({ "review_import": batch_id }),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN, "denied={denied}");
    assert_eq!(denied["error"]["code"], "forbidden", "denied={denied}");
}

/// The review is bounded: a batch above the review cap reports `truncated` and
/// returns exactly the cap.
#[tokio::test(flavor = "multi_thread")]
async fn v1191_holder_pack_http_review_is_bounded() {
    let ctx = ctx().await;
    let (status, body) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({ "pack": foreign_atoms_pack(101), "conflict": "skip" }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={body}");
    assert_eq!(
        body["quarantined"].as_array().expect("quarantined").len(),
        101,
        "body={body}"
    );
    let batch_id = body["quarantined"][0]["batch_id"]
        .as_str()
        .expect("batch id")
        .to_string();

    let (status, review) = post_import(
        &ctx.server,
        TARGET_WORLD,
        &json!({ "review_import": batch_id }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "review={review}");
    assert_eq!(review["review"]["truncated"], true, "review={review}");
    assert_eq!(
        review["review"]["atoms"]
            .as_array()
            .expect("atoms")
            .len(),
        100,
        "the bounded review returns its cap: {review}"
    );
}

/// The review selector is exclusive: mixing it with import input is refused as
/// invalid input, so no boundary silently drops what the caller sent.
#[tokio::test]
async fn v1191_holder_pack_http_review_arm_refuses_import_input() {
    let ctx = ctx().await;
    let pack = governed_pack(&[("kb_gov_shared", "Shared Row", None, None)]);

    for mixed in [
        json!({ "review_import": "pib_any", "pack": pack, "conflict": "skip" }),
        json!({
            "review_import": "pib_any",
            "holder_map": [{ "foreign_id": "hld_foreign_peer", "selector": "author-only" }],
        }),
        json!({ "review_import": "pib_any", "include_anchors": true }),
    ] {
        let (status, body) = post_import(&ctx.server, TARGET_WORLD, &mixed).await;
        assert_eq!(status, StatusCode::BAD_REQUEST, "body={body}");
        assert_eq!(body["error"]["code"], "invalid_input", "body={body}");
    }

    // A refused request writes nothing at all.
    assert_eq!(
        stored_row_count(&ctx.pool, TARGET_WORLD, "kb_gov_shared").await,
        0
    );
    assert!(quarantine_rows(&ctx.pool).await.is_empty());
}

/// The export reads through the exporting Creator's admitted selection: owned
/// known-private material only under the explicit author intent, with the wire
/// governance preserved exactly.
#[tokio::test]
async fn v1191_holder_pack_http_export_intent_includes_owned_private() {
    let ctx = ctx().await;
    let holder = local_holder();
    seed_export_source_world(&ctx.pool).await;
    seed_governed_key_block(
        &ctx.pool,
        "kb_pack_private",
        OWNED_WORLD,
        "Private Row",
        Some(&holder),
        Some("owner-private"),
    )
    .await;

    let (status, filtered) = post_export(&ctx.server, OWNED_WORLD, &json!({})).await;
    assert_eq!(status, StatusCode::OK, "body={filtered}");
    assert_eq!(
        exported_names(&filtered),
        vec!["Aria", "Kael", "Mira"],
        "owned private material is excluded without explicit intent: {filtered}"
    );

    let (status, with_intent) = post_export(
        &ctx.server,
        OWNED_WORLD,
        &json!({ "include_owned_private": true }),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "body={with_intent}");
    assert_eq!(
        exported_names(&with_intent),
        vec!["Aria", "Kael", "Mira", "Private Row"],
        "explicit author intent admits the owned private row: {with_intent}"
    );
    let private = with_intent["entries"]
        .as_array()
        .expect("entries array")
        .iter()
        .find(|entry| entry["canonical_name"] == "Private Row")
        .expect("the private row is exported");
    assert_eq!(private["owner"], json!(holder), "governance travels verbatim");
    assert_eq!(private["disclosure"], "owner-private");
}
