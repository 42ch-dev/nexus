//! V1.147 P0 — compute invoke daemon routes: HTTP integration tests.
//!
//! Exercises the five tier-2 routes end-to-end (route spec §6 matrix) over a
//! real axum router + SQLite + wasmtime engine + embedded `basic-combat`
//! module:
//!
//! - `POST /v1/daemon/compute/run` — invoke, proposals, error taxonomy
//! - `POST /v1/daemon/compute/runs/:run_id/accept` — atomic apply
//! - `POST /v1/daemon/compute/runs/:run_id/discard` — no-write discard
//! - `GET  /v1/daemon/compute/runs` — pagination + ownership scoping
//! - `GET  /v1/daemon/compute/runs/:run_id` — detail with proposals
//!
//! Auth is keyless (`DaemonApiConfig::keyless`); the tier-2
//! `require_active_creator` gate reads the active creator from the seeded
//! `config.toml` (`test_creator`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use std::sync::Arc;
use std::time::Duration;

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_wasm_host::{CachedModule, ModuleCache, ModuleManifest, SandboxConfig, WasmEngine};
use serde_json::{json, Value};
use serial_test::serial;

// NOTE: must match the ComputeInput world_id newtype pattern `^wld_[a-zA-Z0-9]+$`
// (no underscores) — the shared `wld_test_world` fixture id does not.
const WORLD: &str = "wld_combat1";
const MODULE: &str = "basic-combat";
const FOREIGN_WORLD: &str = "wld_other";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

/// Standard server: real engine + embedded basic-combat warmed + seeded
/// creator/world under keyless auth.
async fn ctx() -> Ctx {
    ctx_with_engine(WasmEngine::new().expect("wasm engine"), &[]).await
}

/// Server with a custom engine (sandbox overrides) and extra cache entries.
async fn ctx_with_engine(engine: WasmEngine, extra: &[(&str, ModuleManifest, Vec<u8>)]) -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let engine = Arc::new(engine);
    let cache = Arc::new(ModuleCache::new());
    cache
        .warm_embedded(&engine)
        .expect("warm embedded basic-combat");
    for (id, manifest, bytes) in extra {
        let module = engine.load_module(bytes).expect("extra module compiles");
        cache.insert(
            *id,
            Arc::new(CachedModule {
                module,
                manifest: manifest.clone(),
                // The cache identity is (id, bytes_hash, manifest_hash) —
                // the entry must record the exact artifacts it was
                // compiled from so a loader's get_checked lookup can match
                // them (manifest half: Greptile P1).
                bytes_hash: nexus_wasm_host::hash_module_bytes(bytes),
                manifest_hash: nexus_wasm_host::hash_module_bytes(
                    &serde_json::to_vec(manifest).expect("manifest serializes"),
                ),
            }),
        );
    }
    state.set_wasm_engine(engine);
    state.set_module_cache(cache);
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    seed_world(&pool, WORLD).await;
    let app = api::create_router(state, DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

/// Seed a world owned by `test_creator` (ids must match the ComputeInput
/// `^wld_[a-zA-Z0-9]+$` pattern for the run handler's world_ref assembly).
async fn seed_world(pool: &sqlx::SqlitePool, world_id: &str) {
    // SAFETY: test-only seed against the known narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES (?, 'ws', 'test_creator', 'Combat World', 'combat-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(world_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a computable character `WorldKbEntry` with deterministic entry id and
/// the V1.61 KB body shape (`attributes` flat object + `state.character.*`).
async fn seed_character(
    pool: &sqlx::SqlitePool,
    entry_id: &str,
    name: &str,
    base_atk: i64,
    base_def: i64,
    current_hp: i64,
    max_hp: i64,
) {
    seed_character_in_world(
        pool, WORLD, entry_id, name, base_atk, base_def, current_hp, max_hp,
    )
    .await;
}

/// World-aware variant of [`seed_character`] (foreign-world KB fixtures).
async fn seed_character_in_world(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    entry_id: &str,
    name: &str,
    base_atk: i64,
    base_def: i64,
    current_hp: i64,
    max_hp: i64,
) {
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;

    let kb = WorldKbEntry {
        entry_id: entry_id.to_string(),
        world_id: world_id.to_string(),
        block_type: BlockType::Character,
        canonical_name: name.to_string(),
        body: Some(WorldKbBody {
            summary: Some(format!("{name} combatant")),
            attributes: Some(json!({
                "max_hp": max_hp,
                "base_atk": base_atk,
                "base_def": base_def,
            })),
            computable: Some(true),
            state: Some(json!({
                "character": {
                    "current_hp": current_hp,
                    "is_alive": true,
                    "status_effects": [],
                }
            })),
            ..Default::default()
        }),
        ..WorldKbEntry::new(world_id, BlockType::Character, name)
    };
    SqliteKbStore::new(pool.clone())
        .insert_knowledge_entry(kb)
        .await
        .unwrap();
}

/// Seed a second world owned by a *different* creator (ownership-gate tests).
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
           VALUES (?, 'ws', 'other_creator', 'Other World', 'other-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(FOREIGN_WORLD)
    .execute(pool)
    .await
    .unwrap();
}

async fn post_run(
    server: &TestServer,
    world_id: &str,
    module_id: &str,
    invocation_params: Value,
) -> axum_test::TestResponse {
    server
        .post("/v1/daemon/compute/run")
        .json(&json!({
            "world_id": world_id,
            "module_id": module_id,
            "invocation_params": invocation_params,
        }))
        .await
}

/// POST /run with the standard seeded combatants; returns the run_id.
async fn run_succeeded(c: &Ctx) -> String {
    let resp = post_run(
        &c.server,
        WORLD,
        MODULE,
        json!({"attacker_id": "kb_atk", "defender_id": "kb_def"}),
    )
    .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    body["run_id"].as_str().expect("run_id").to_string()
}

/// Assert the canonical daemon API error envelope
/// (`{"success": false, "error": {"code", "message", ...}}`).
fn assert_error_envelope(resp: &axum_test::TestResponse, status: StatusCode, code: &str) {
    assert_eq!(resp.status_code(), status, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], code, "body={body}");
    assert!(
        body["error"]["message"].is_string(),
        "error.message must be a string: {body}"
    );
}

/// Read back a character's `current_hp` from the KB store.
async fn defender_hp(pool: &sqlx::SqlitePool, entry_id: &str) -> i64 {
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;
    let kb = SqliteKbStore::new(pool.clone())
        .get_knowledge_entry(entry_id)
        .await
        .unwrap();
    kb.body.unwrap().state.unwrap()["character"]["current_hp"]
        .as_i64()
        .unwrap()
}

/// `(timeline_event_id, event_type, extensions_nexus_json)` for a world's timeline.
async fn timeline_events(pool: &sqlx::SqlitePool) -> Vec<(String, String, Option<String>, String)> {
    sqlx::query_as::<_, (String, String, Option<String>, String)>(
        "SELECT timeline_event_id, event_type, extensions_nexus_json, status \
         FROM narrative_timeline_events WHERE world_id = ? ORDER BY sequence_no",
    )
    .bind(WORLD)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// The most recent direct-lane run row for the seeded world.
async fn latest_run(pool: &sqlx::SqlitePool) -> (String, String, Option<String>) {
    sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT run_id, status, error_json FROM compute_sessions \
         WHERE world_id = ? AND run_id IS NOT NULL ORDER BY created_at DESC LIMIT 1",
    )
    .bind(WORLD)
    .fetch_one(pool)
    .await
    .unwrap()
}

// ── §6 row 1 — POST /run happy path ────────────────────────────────────────

#[tokio::test]
#[serial]
async fn run_happy_path_returns_succeeded_with_proposals() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;

    let resp = post_run(
        &c.server,
        WORLD,
        MODULE,
        json!({"attacker_id": "kb_atk", "defender_id": "kb_def"}),
    )
    .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["status"], "succeeded", "body={body}");
    let run_id = body["run_id"].as_str().expect("run_id").to_string();
    assert!(run_id.starts_with("run_"), "run_id prefix: {run_id}");
    assert_eq!(body["module_id"], MODULE);
    assert_eq!(body["module_version"], "1.0.0");
    // `truncated` defaults to false and may be omitted.
    if let Some(t) = body.get("truncated") {
        assert_eq!(t, &json!(false));
    }

    // Combat math: damage = max(0, 20 − 5) = 15 on the defender.
    let proposals = &body["proposals"];
    let hp_delta = proposals["state_delta"]
        .as_array()
        .unwrap()
        .iter()
        .find(|d| d["target_key_block_id"] == "kb_def")
        .expect("defender delta present");
    assert_eq!(hp_delta["op"], "sub");
    assert_eq!(hp_delta["path"], "character.current_hp");
    assert_eq!(hp_delta["value"], 15);
    assert_eq!(proposals["timeline_events"].as_array().unwrap().len(), 1);
    assert_eq!(proposals["battle_report"]["kind"], "combat");

    // Proposals only — the World is untouched by run alone.
    assert_eq!(defender_hp(&c.pool, "kb_def").await, 30);
    assert!(timeline_events(&c.pool).await.is_empty());
}

// ── §6 row 2/3 — sandbox limits ────────────────────────────────────────────

/// A module that exports the V1 ABI but whose `compute` is an infinite loop
/// (mirrors `nexus-wasm-host/tests/sandbox_limits.rs`).
fn loop_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (global $heap (mut i32) (i32.const 1024))
            (func (export "alloc") (param $len i32) (result i32)
              (local $p i32)
              (local.set $p (global.get $heap))
              (global.set $heap (i32.add (global.get $heap) (local.get $len)))
              (local.get $p))
            (func (export "init"))
            (func (export "compute")
              (param i32 i32 i32 i32) (result i64)
              (loop $forever (br $forever))
              (i64.const 0)))
        "#,
    )
    .expect("valid wat")
}

fn loop_manifest() -> ModuleManifest {
    serde_json::from_str(
        r#"{"module_id":"loop","name":"Loop","version":"0.1.0","nexus_abi_version":1,
           "required_key_block_types":[],"compute_export":"compute","init_export":"init",
           "host_functions":[]}"#,
    )
    .unwrap()
}

#[tokio::test]
#[serial]
async fn run_fuel_exhaustion_returns_422_with_honest_code() {
    let c = ctx_with_engine(
        WasmEngine::new().expect("engine"),
        &[("loop", loop_manifest(), loop_wasm())],
    )
    .await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;

    let resp = post_run(&c.server, WORLD, "loop", json!({})).await;
    assert_error_envelope(
        &resp,
        StatusCode::UNPROCESSABLE_ENTITY,
        "compute_fuel_exhausted",
    );

    // The failed run is persisted (honest detail) and the direct lane writes
    // NO timeline events on failure.
    let (run_id, status, error_json) = latest_run(&c.pool).await;
    assert_eq!(status, "failed");
    let error: Value = serde_json::from_str(error_json.as_deref().unwrap()).unwrap();
    assert_eq!(error["code"], "compute_fuel_exhausted");
    assert!(timeline_events(&c.pool).await.is_empty());
    assert!(!run_id.is_empty());
}

#[tokio::test]
#[serial]
async fn run_wall_time_exceeded_returns_422_with_honest_code() {
    // Huge fuel so the loop cannot exhaust it; a 300 ms wall-time watchdog
    // traps first via epoch interruption.
    let cfg = SandboxConfig {
        fuel: 100_000_000_000,
        max_memory_bytes: 64 * 1024 * 1024,
        wall_time: Duration::from_millis(300),
    };
    let c = ctx_with_engine(
        WasmEngine::with_config(cfg).expect("engine"),
        &[("loop", loop_manifest(), loop_wasm())],
    )
    .await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;

    let resp = post_run(&c.server, WORLD, "loop", json!({})).await;
    assert_error_envelope(
        &resp,
        StatusCode::UNPROCESSABLE_ENTITY,
        "compute_wall_time_exceeded",
    );

    let (_run_id, status, error_json) = latest_run(&c.pool).await;
    assert_eq!(status, "failed");
    let error: Value = serde_json::from_str(error_json.as_deref().unwrap()).unwrap();
    assert_eq!(error["code"], "compute_wall_time_exceeded");
    assert!(timeline_events(&c.pool).await.is_empty());
}

// ── §6 rows 4–6 — invalid module / not owner / no computable entries ───────

#[tokio::test]
#[serial]
async fn run_invalid_module_returns_404() {
    let c = ctx().await;
    let resp = post_run(&c.server, WORLD, "no-such-module", json!({})).await;
    assert_error_envelope(&resp, StatusCode::NOT_FOUND, "not_found");
}

#[tokio::test]
#[serial]
async fn run_not_owner_returns_403() {
    let c = ctx().await;
    seed_foreign_world(&c.pool).await;
    let resp = post_run(&c.server, FOREIGN_WORLD, MODULE, json!({})).await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}

#[tokio::test]
#[serial]
async fn run_no_computable_entries_returns_422() {
    let c = ctx().await;
    let resp = post_run(&c.server, WORLD, MODULE, json!({})).await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");
}

// ── §6 row 7/8 — accept happy path + double accept ─────────────────────────

#[tokio::test]
#[serial]
async fn accept_happy_path_applies_atomically_and_creates_events() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    let run_id = run_succeeded(&c).await;

    let resp = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["status"], "applied", "body={body}");
    assert_eq!(body["applied"]["state_delta_count"], 1);
    assert_eq!(body["applied"]["events_created"], 1);
    assert_eq!(body["applied"]["new_entries_created"], 0);
    let event_ids = body["timeline_event_ids"].as_array().unwrap();
    assert_eq!(event_ids.len(), 1);

    // KB delta visible: defender 30 → 15; attacker untouched.
    assert_eq!(defender_hp(&c.pool, "kb_def").await, 15);
    assert_eq!(defender_hp(&c.pool, "kb_atk").await, 100);

    // Timeline events created with `compute_result` + compute provenance,
    // committed as canon (P2 dogfood: the Timeline projection reads canon
    // events only — an accepted Run must surface as a Narrative node).
    let events = timeline_events(&c.pool).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, "compute_result");
    assert_eq!(
        events[0].3, "canon",
        "accepted events must be canon, not provisional"
    );
    assert_eq!(
        event_ids[0],
        json!(events[0].0),
        "timeline_event_ids must match DB"
    );
    let prov: Value = serde_json::from_str(events[0].2.as_deref().expect("provenance")).unwrap();
    assert_eq!(prov["compute"]["module_id"], MODULE);
    assert_eq!(prov["compute"]["module_version"], "1.0.0");
    assert_eq!(prov["compute"]["run_id"], run_id);
    assert_eq!(prov["compute"]["source_kind"], "direct_invoke");
}

#[tokio::test]
#[serial]
async fn accept_on_already_accepted_run_returns_409() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    let run_id = run_succeeded(&c).await;

    let first = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_eq!(first.status_code(), StatusCode::OK, "body={}", first.text());

    let second = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_error_envelope(&second, StatusCode::CONFLICT, "conflict");
}

/// QC S-affected: Accept must persist `affected_key_block_ids` from
/// `proposals.timeline_events` onto the appended events — the P2 compute
/// inspector resolves its "Affected knowledge" section from this column, so
/// dropping it would leave the section permanently empty (behavior §5).
#[tokio::test]
#[serial]
async fn accept_persists_affected_key_block_ids() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;

    let run_id = craft_succeeded_run(
        &c.pool,
        crafted_proposals(
            vec![json!({
                "op": "sub",
                "path": "character.current_hp",
                "target_key_block_id": "kb_def",
                "value": 15,
            })],
            &["Combat resolved"],
            Some(&["kb_def", "kb_atk"]),
        ),
    )
    .await;

    let resp = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());

    // SAFETY: test-only read against the known narrative_timeline_events schema.
    let row: (Option<String>,) = sqlx::query_as(
        "SELECT affected_key_block_ids_json FROM narrative_timeline_events \
         WHERE world_id = ? ORDER BY sequence_no",
    )
    .bind(WORLD)
    .fetch_one(&c.pool)
    .await
    .unwrap();
    let affected: Vec<String> =
        serde_json::from_str(row.0.as_deref().expect("affected ids must be persisted")).unwrap();
    assert_eq!(affected, vec!["kb_def".to_string(), "kb_atk".to_string()]);
}

// ── §6 row 9 — discard ─────────────────────────────────────────────────────

#[tokio::test]
#[serial]
async fn discard_marks_run_discarded_and_leaves_world_unchanged() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    let run_id = run_succeeded(&c).await;

    let resp = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/discard"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["status"], "discarded");
    assert_eq!(body["run_id"], run_id);

    // KB unchanged (no state delta applied) and no timeline events.
    assert_eq!(defender_hp(&c.pool, "kb_def").await, 30);
    assert!(timeline_events(&c.pool).await.is_empty());

    // Detail reflects the discarded status.
    let detail = c
        .server
        .get(&format!("/v1/daemon/compute/runs/{run_id}"))
        .await;
    assert_eq!(
        detail.status_code(),
        StatusCode::OK,
        "body={}",
        detail.text()
    );
    let detail_body: Value = detail.json();
    assert_eq!(detail_body["status"], "discarded");
}

// ── brief — accept on failed / discarded, discard on applied ───────────────

#[tokio::test]
#[serial]
async fn accept_on_failed_run_returns_422_invalid_state() {
    let c = ctx_with_engine(
        WasmEngine::new().expect("engine"),
        &[("loop", loop_manifest(), loop_wasm())],
    )
    .await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;

    let resp = post_run(&c.server, WORLD, "loop", json!({})).await;
    assert_error_envelope(
        &resp,
        StatusCode::UNPROCESSABLE_ENTITY,
        "compute_fuel_exhausted",
    );
    let (run_id, status, _) = latest_run(&c.pool).await;
    assert_eq!(status, "failed");

    let accept = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_error_envelope(&accept, StatusCode::UNPROCESSABLE_ENTITY, "invalid_state");
}

#[tokio::test]
#[serial]
async fn accept_on_discarded_run_returns_409() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    let run_id = run_succeeded(&c).await;

    let discard = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/discard"))
        .await;
    assert_eq!(
        discard.status_code(),
        StatusCode::OK,
        "body={}",
        discard.text()
    );

    let accept = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_error_envelope(&accept, StatusCode::CONFLICT, "conflict");
}

#[tokio::test]
#[serial]
async fn discard_on_applied_run_returns_409() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    let run_id = run_succeeded(&c).await;

    let accept = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_eq!(
        accept.status_code(),
        StatusCode::OK,
        "body={}",
        accept.text()
    );

    let discard = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/discard"))
        .await;
    assert_error_envelope(&discard, StatusCode::CONFLICT, "conflict");
}

// ── §6 rows 10/11 — list + detail ──────────────────────────────────────────

#[tokio::test]
#[serial]
async fn list_paginates_and_scopes_to_owned_worlds() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    let a = run_succeeded(&c).await;
    let b = run_succeeded(&c).await;
    let d = run_succeeded(&c).await;

    // A run targeting a world owned by another creator must never appear.
    seed_foreign_world(&c.pool).await;
    let foreign = nexus_local_db::compute_runs::insert_run(
        &c.pool,
        FOREIGN_WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let page1 = c.server.get("/v1/daemon/compute/runs?limit=2").await;
    assert_eq!(page1.status_code(), StatusCode::OK, "body={}", page1.text());
    let p1: Value = page1.json();
    assert_eq!(p1["items"].as_array().unwrap().len(), 2);
    assert_eq!(p1["has_more"], true);
    let cursor = p1["next_cursor"].as_str().expect("cursor").to_string();
    let page1_ids: Vec<&str> = p1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["run_id"].as_str().unwrap())
        .collect();
    assert!(!page1_ids.contains(&foreign.as_str()));

    let page2 = c
        .server
        .get(&format!("/v1/daemon/compute/runs?limit=2&cursor={cursor}"))
        .await;
    assert_eq!(page2.status_code(), StatusCode::OK, "body={}", page2.text());
    let p2: Value = page2.json();
    assert_eq!(p2["items"].as_array().unwrap().len(), 1);
    assert_eq!(p2["has_more"], false);
    assert!(p2["next_cursor"].is_null(), "no more pages: {p2}");

    let all_ids: Vec<&str> = p1["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(p2["items"].as_array().unwrap().iter())
        .map(|i| i["run_id"].as_str().unwrap())
        .collect();
    for expected in [&a, &b, &d] {
        assert!(
            all_ids.contains(&expected.as_str()),
            "{expected} in {all_ids:?}"
        );
    }
}

#[tokio::test]
#[serial]
async fn get_run_detail_returns_proposals_and_invocation_params() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    let run_id = run_succeeded(&c).await;

    let resp = c
        .server
        .get(&format!("/v1/daemon/compute/runs/{run_id}"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["run_id"], run_id);
    assert_eq!(body["world_id"], WORLD);
    assert_eq!(body["module_id"], MODULE);
    assert_eq!(body["module_version"], "1.0.0");
    assert_eq!(body["status"], "succeeded");
    assert_eq!(body["invocation_params"]["attacker_id"], "kb_atk");
    assert_eq!(body["invocation_params"]["defender_id"], "kb_def");
    assert_eq!(body["proposals"]["battle_report"]["kind"], "combat");
    assert!(body["created_at"].is_string());
}

#[tokio::test]
#[serial]
async fn get_run_detail_unknown_run_returns_404() {
    let c = ctx().await;
    let resp = c
        .server
        .get("/v1/daemon/compute/runs/run_does_not_exist")
        .await;
    assert_error_envelope(&resp, StatusCode::NOT_FOUND, "not_found");
}

// ── brief — cross-owner accept / detail / discard on a foreign run ─────────

#[tokio::test]
#[serial]
async fn foreign_run_accept_detail_discard_return_403() {
    let c = ctx().await;
    seed_foreign_world(&c.pool).await;
    let foreign = nexus_local_db::compute_runs::insert_run(
        &c.pool,
        FOREIGN_WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let detail = c
        .server
        .get(&format!("/v1/daemon/compute/runs/{foreign}"))
        .await;
    assert_error_envelope(&detail, StatusCode::FORBIDDEN, "forbidden");

    let accept = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{foreign}/accept"))
        .json(&json!({}))
        .await;
    assert_error_envelope(&accept, StatusCode::FORBIDDEN, "forbidden");

    let discard = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{foreign}/discard"))
        .await;
    assert_error_envelope(&discard, StatusCode::FORBIDDEN, "forbidden");
}

// ── §6 row 12 — failed run leaves Timeline empty (direct lane) ─────────────

#[tokio::test]
#[serial]
async fn failed_run_leaves_timeline_empty() {
    let c = ctx_with_engine(
        WasmEngine::new().expect("engine"),
        &[("loop", loop_manifest(), loop_wasm())],
    )
    .await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;

    let resp = post_run(&c.server, WORLD, "loop", json!({})).await;
    assert_error_envelope(
        &resp,
        StatusCode::UNPROCESSABLE_ENTITY,
        "compute_fuel_exhausted",
    );

    // Direct lane writes NO timeline events on failure (no compute_error rows).
    assert!(timeline_events(&c.pool).await.is_empty());
}

// ── W-1/W-2 fix wave — concurrent runs must not share the watchdog budget ──

/// Two staggered-budget manifests for the infinite-loop module: the long run
/// may execute up to 1500 ms, the short one only 300 ms.
fn staggered_loop_manifests() -> [(&'static str, ModuleManifest); 2] {
    let long = serde_json::from_str(
        r#"{"module_id":"loop_long","name":"Loop Long","version":"0.1.0","nexus_abi_version":1,
           "required_key_block_types":[],"compute_export":"compute","init_export":"init",
           "host_functions":[],"max_wall_time_ms":1500}"#,
    )
    .unwrap();
    let short = serde_json::from_str(
        r#"{"module_id":"loop_short","name":"Loop Short","version":"0.1.0","nexus_abi_version":1,
           "required_key_block_types":[],"compute_export":"compute","init_export":"init",
           "host_functions":[],"max_wall_time_ms":300}"#,
    )
    .unwrap();
    [("loop_long", long), ("loop_short", short)]
}

/// W-1/W-2: two concurrent POST /run against the shared engine must run
/// serially — the long run (1500 ms budget) must survive the short run's
/// (300 ms) watchdog.  Without the compute serializer, the engine-global
/// epoch counter makes the first watchdog to fire trap BOTH invocations at
/// the shortest budget (~300 ms); with serialization, whichever order the
/// runs take, the long one executes for its own full budget, so the slower
/// response arrives >= 1200 ms after its request started.
#[tokio::test]
#[serial]
async fn concurrent_runs_serialize_compute_and_long_survives_short_watchdog() {
    let manifests = staggered_loop_manifests();
    let cfg = SandboxConfig {
        fuel: 100_000_000_000, // huge — the loops cannot exhaust fuel
        max_memory_bytes: 64 * 1024 * 1024,
        wall_time: Duration::from_millis(5000), // host ceiling above both manifests
    };
    let c = ctx_with_engine(
        WasmEngine::with_config(cfg).expect("engine"),
        &[
            ("loop_long", manifests[0].1.clone(), loop_wasm()),
            ("loop_short", manifests[1].1.clone(), loop_wasm()),
        ],
    )
    .await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;

    // Fire both runs concurrently (no required_key_block_types → the seeded
    // character is not needed for the manifest filter; the builder still
    // requires at least one computable entry, which the seed provides).
    let long_req = post_run(&c.server, WORLD, "loop_long", json!({}));
    let short_req = post_run(&c.server, WORLD, "loop_short", json!({}));
    let started = std::time::Instant::now();
    let (long_resp, short_resp) = tokio::join!(long_req, short_req);
    let both_elapsed = started.elapsed();

    // Both must honestly report wall-time exhaustion.
    assert_error_envelope(
        &long_resp,
        StatusCode::UNPROCESSABLE_ENTITY,
        "compute_wall_time_exceeded",
    );
    assert_error_envelope(
        &short_resp,
        StatusCode::UNPROCESSABLE_ENTITY,
        "compute_wall_time_exceeded",
    );

    // Serialization proof: with the compute serializer, the two runs execute
    // back-to-back (300 ms + 1500 ms in either order) so both complete in
    // >= 1500 ms — the LONG run always runs for its own full budget and
    // survives the short run's watchdog.  Had the engine-global epoch
    // counter cross-tripped the invocations, both would have died at the
    // short budget (~300 ms).
    assert!(
        both_elapsed >= Duration::from_millis(1200),
        "concurrent runs must serialize (long survives short watchdog), took {both_elapsed:?}"
    );

    // Both runs were persisted as failed (honest detail).
    let runs: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT run_id, status FROM compute_sessions \
         WHERE world_id = ? AND run_id IS NOT NULL ORDER BY created_at DESC LIMIT 2",
    )
    .bind(WORLD)
    .fetch_all(&c.pool)
    .await
    .unwrap();
    assert_eq!(runs.len(), 2, "both staggered runs persisted: {runs:?}");
    for (_, status) in &runs {
        assert_eq!(status, "failed", "all loop runs must be persisted failed");
    }
}

// ── F-001 / S-4 fix wave — world-scoped Accept + rollback ──────────────────

/// Insert a direct-lane run row and flip it to `succeeded` with crafted
/// proposals (bypasses the module — lets tests drive the Accept path with
/// arbitrary proposal payloads).
async fn craft_succeeded_run(pool: &sqlx::SqlitePool, proposals: Value) -> String {
    let run_id = nexus_local_db::compute_runs::insert_run(
        pool,
        WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        Some(r#"{}"#),
    )
    .await
    .unwrap();
    nexus_local_db::compute_runs::set_run_succeeded(pool, &run_id, &proposals.to_string())
        .await
        .unwrap();
    run_id
}

/// Craft a `ComputeOutput` envelope with the given state deltas and timeline
/// event titles (each event carries the minimal valid NexusTimelineEvent
/// fields; the Accept handler consumes title/summary/affected_key_block_ids).
/// `affected_ids`, when `Some`, is stamped onto every event.
fn crafted_proposals(
    state_delta: Vec<Value>,
    event_titles: &[&str],
    affected_ids: Option<&[&str]>,
) -> Value {
    let timeline_events: Vec<Value> = event_titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let mut evt = json!({
                "schema_version": 1,
                "timeline_event_id": format!("evt_p{i}"),
                "world_id": WORLD,
                "branch_id": "fbk_root",
                "event_type": "story_advance",
                "status": "provisional",
                "sequence_no": i,
                "title": title,
                "summary": format!("summary {title}"),
                "created_at": "2026-07-31T12:00:00Z",
            });
            if let Some(ids) = affected_ids {
                evt["affected_key_block_ids"] = json!(ids);
            }
            evt
        })
        .collect();
    json!({
        "schema_version": 1,
        "state_delta": state_delta,
        "timeline_events": timeline_events,
        "new_key_blocks": [],
        "battle_report": {"kind": "combat"},
    })
}

/// Read a KB entry's `body.state.character.current_hp` (world-aware).
async fn world_entry_hp(pool: &sqlx::SqlitePool, entry_id: &str) -> i64 {
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;
    let kb = SqliteKbStore::new(pool.clone())
        .get_knowledge_entry(entry_id)
        .await
        .unwrap();
    kb.body.unwrap().state.unwrap()["character"]["current_hp"]
        .as_i64()
        .unwrap()
}

/// F-001: Accept with a state_delta targeting another world's KB must reject
/// the whole Accept (422 invalid_input) and leave every table untouched.
#[tokio::test]
#[serial]
async fn accept_foreign_delta_target_rejects_with_422_and_rolls_back() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    // Foreign world + foreign KB with a distinctive current_hp.
    seed_foreign_world(&c.pool).await;
    seed_character_in_world(
        &c.pool,
        FOREIGN_WORLD,
        "kb_foreign",
        "Stranger",
        99,
        99,
        777,
        777,
    )
    .await;

    let run_id = craft_succeeded_run(
        &c.pool,
        crafted_proposals(
            vec![json!({
                "op": "sub",
                "path": "character.current_hp",
                "target_key_block_id": "kb_foreign",
                "value": 100,
            })],
            &[],
            None,
        ),
    )
    .await;

    let resp = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");

    // Foreign KB untouched; own KB untouched; timeline empty; run still
    // succeeded (nothing partially applied).
    assert_eq!(world_entry_hp(&c.pool, "kb_foreign").await, 777);
    assert_eq!(world_entry_hp(&c.pool, "kb_def").await, 30);
    assert!(timeline_events(&c.pool).await.is_empty());
    let row = nexus_local_db::compute_runs::get_run(&c.pool, &run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "succeeded");
}

/// S-4: a mid-loop failure inside the Accept TX (1st delta valid, 2nd delta
/// foreign) must roll back EVERYTHING — the first delta's mutation included.
#[tokio::test]
#[serial]
async fn accept_mid_loop_failure_rolls_back_entire_tx() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    seed_foreign_world(&c.pool).await;
    seed_character_in_world(
        &c.pool,
        FOREIGN_WORLD,
        "kb_foreign",
        "Stranger",
        99,
        99,
        777,
        777,
    )
    .await;

    let run_id = craft_succeeded_run(
        &c.pool,
        crafted_proposals(
            vec![
                // 1st delta: valid, applies inside the TX (def 30 → 15).
                json!({
                    "op": "sub",
                    "path": "character.current_hp",
                    "target_key_block_id": "kb_def",
                    "value": 15,
                }),
                // 2nd delta: foreign target → InputInvalid mid-loop.
                json!({
                    "op": "sub",
                    "path": "character.current_hp",
                    "target_key_block_id": "kb_foreign",
                    "value": 1,
                }),
            ],
            &["Battle"],
            None,
        ),
    )
    .await;

    let resp = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");

    // FULL rollback: the first delta must NOT be visible, no events appended,
    // run still succeeded.
    assert_eq!(world_entry_hp(&c.pool, "kb_def").await, 30);
    assert_eq!(world_entry_hp(&c.pool, "kb_foreign").await, 777);
    assert!(timeline_events(&c.pool).await.is_empty());
    let row = nexus_local_db::compute_runs::get_run(&c.pool, &run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "succeeded");
}

// ── F-002/F-003 fix wave — branch scoping + snapshot ───────────────────────

/// Seed a timeline event on a named (non-root) branch of the combat world.
async fn seed_branch_event(pool: &sqlx::SqlitePool, branch_id: &str) {
    seed_branch_event_in_world(pool, WORLD, branch_id).await;
}

/// World-aware variant of [`seed_branch_event`] (other-world branch fixtures).
async fn seed_branch_event_in_world(pool: &sqlx::SqlitePool, world_id: &str, branch_id: &str) {
    // SAFETY: test-only seed against the known narrative_timeline_events schema.
    sqlx::query(
        "INSERT INTO narrative_timeline_events \
            (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, metadata_json) \
         VALUES (?, ?, ?, 'fork_marker', 'provisional', 0, '{}')",
    )
    .bind(format!("evt_bside{}", branch_id.replace('_', "")))
    .bind(world_id)
    .bind(branch_id)
    .execute(pool)
    .await
    .unwrap();
}

/// POST /run with an explicit branch_id.
async fn post_run_on_branch(server: &TestServer, branch_id: &str) -> axum_test::TestResponse {
    server
        .post("/v1/daemon/compute/run")
        .json(&json!({
            "world_id": WORLD,
            "module_id": MODULE,
            "branch_id": branch_id,
            "invocation_params": {"attacker_id": "kb_atk", "defender_id": "kb_def"},
        }))
        .await
}

/// F-002/F-003: a run scoped to a named branch snapshots that branch and
/// Accept appends timeline events to the SNAPSHOT — not the world root.
#[tokio::test]
#[serial]
async fn run_on_named_branch_snapshots_and_accept_lands_events_there() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    seed_branch_event(&c.pool, "fbk_side1").await;

    let resp = post_run_on_branch(&c.server, "fbk_side1").await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    let run_id = body["run_id"].as_str().expect("run_id").to_string();

    // Snapshot on the run row.
    let row = nexus_local_db::compute_runs::get_run(&c.pool, &run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.branch_id.as_deref(),
        Some("fbk_side1"),
        "run row must snapshot the requested branch"
    );

    // Accept → events land on the SNAPSHOTTED branch.
    let accept = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({}))
        .await;
    assert_eq!(
        accept.status_code(),
        StatusCode::OK,
        "body={}",
        accept.text()
    );

    let events: Vec<(String, String)> = sqlx::query_as::<_, (String, String)>(
        "SELECT branch_id, event_type FROM narrative_timeline_events \
         WHERE world_id = ? ORDER BY branch_id, sequence_no",
    )
    .bind(WORLD)
    .fetch_all(&c.pool)
    .await
    .unwrap();
    let compute_events: Vec<&(String, String)> = events
        .iter()
        .filter(|(_, t)| t == "compute_result")
        .collect();
    assert_eq!(compute_events.len(), 1);
    assert_eq!(
        compute_events[0].0, "fbk_side1",
        "compute_result must land on the snapshotted branch: {events:?}"
    );
}

/// F-002: an unknown / other-world branch must be rejected at run time.
#[tokio::test]
#[serial]
async fn run_with_unknown_branch_returns_422() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;

    let resp = post_run_on_branch(&c.server, "fbk_nonexistent").await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");
}

/// Branch parity (V1.147 P3 T1): membership is world-scoped — a branch that is
/// event-bearing in ANOTHER world must still be rejected on this world's run
/// (422 `invalid_input`), matching the invoke path's membership semantics
/// (unknown / other-world branches are never bindable).
#[tokio::test]
#[serial]
async fn run_with_other_world_branch_returns_422() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    seed_foreign_world(&c.pool).await;
    // The branch exists — but only under FOREIGN_WORLD (owned by another
    // creator, which makes the rejection even stronger: the check is
    // world-scoped, not ownership-scoped).
    seed_branch_event_in_world(&c.pool, FOREIGN_WORLD, "fbk_other").await;

    let resp = post_run_on_branch(&c.server, "fbk_other").await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");
}

// ── V1.147 P3 T1 — F2: manifest validation failures → 422 with per-entry detail

/// Seed a computable character whose `state.character` violates the
/// basic-combat manifest schema (`current_hp` must be an integer) — the
/// "poison entry" dogfood case (F2): one invalid entry must fail the whole
/// run honestly (422 + per-entry detail), never 500.
///
/// NOTE: flat `attributes` cannot poison the direct lane — the spoke
/// conversion rewrites them into the ERC721-array form, which validates
/// per-item shape only (`trait_type`/`value`); a missing key is silently
/// absent from the array. `state` is preserved verbatim, so a malformed
/// state is the honest manifest violation.
async fn seed_broken_character(pool: &sqlx::SqlitePool, entry_id: &str) {
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;

    let kb = WorldKbEntry {
        entry_id: entry_id.to_string(),
        world_id: WORLD.to_string(),
        block_type: BlockType::Character,
        canonical_name: format!("Broken {entry_id}"),
        body: Some(WorldKbBody {
            summary: Some("Broken combatant".to_string()),
            attributes: Some(json!({
                "max_hp": 100,
                "base_atk": 5,
                "base_def": 10,
            })),
            computable: Some(true),
            state: Some(json!({
                "character": {
                    "current_hp": "one-hundred", // must be integer per manifest
                    "is_alive": true,
                    "status_effects": [],
                }
            })),
            ..Default::default()
        }),
        ..WorldKbEntry::new(WORLD, BlockType::Character, "Broken")
    };
    SqliteKbStore::new(pool.clone())
        .insert_knowledge_entry(kb)
        .await
        .unwrap();
}

/// F2 (dogfood, V1.147 P3): an input entry that violates the manifest schema
/// poisons the run — HTTP **422** `invalid_input` with per-entry failure
/// detail (`error.details.invalid_entries` = entry id + reason), NOT a 500.
/// The run row is still recorded `failed` with the honest per-entry error_json;
/// no timeline events are written.
#[tokio::test]
#[serial]
async fn run_with_invalid_entry_returns_422_with_per_entry_detail() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    seed_broken_character(&c.pool, "kb_broken").await;

    let resp = post_run(
        &c.server,
        WORLD,
        MODULE,
        json!({"attacker_id": "kb_atk", "defender_id": "kb_def"}),
    )
    .await;
    assert_eq!(
        resp.status_code(),
        StatusCode::UNPROCESSABLE_ENTITY,
        "body={}",
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], "invalid_input", "body={body}");
    let invalid_entries = &body["error"]["details"]["invalid_entries"];
    let invalid_entries = invalid_entries
        .as_array()
        .unwrap_or_else(|| panic!("invalid_entries must be an array: {body}"));
    assert!(!invalid_entries.is_empty(), "body={body}");
    assert_eq!(invalid_entries[0]["entry_id"], "kb_broken", "body={body}");
    let reason = invalid_entries[0]["reason"].as_str().unwrap();
    assert!(
        reason.contains("current_hp"),
        "reason must name the failing field: {reason}"
    );
    assert!(
        reason.contains("expected type integer"),
        "reason must explain the violation: {reason}"
    );

    // Run row persisted as Failed with the honest per-entry error_json; the
    // direct lane writes NO timeline events on failure.
    let (run_id, status, error_json) = latest_run(&c.pool).await;
    assert_eq!(status, "failed");
    let error: Value = serde_json::from_str(error_json.as_deref().unwrap()).unwrap();
    assert_eq!(error["code"], "invalid_input");
    // `invalid_entries` lives under `details`: the persisted error_json is a
    // strict `NexusErrorResponse` (`code`/`details`/`message`) and
    // `GET /runs/:id` re-deserializes it into `RunDetail.error` — a
    // top-level `invalid_entries` key 500s the detail read (V1.147 P3 T4
    // dogfood regression; the detail-read assertion below is the guard).
    assert_eq!(
        error["details"]["invalid_entries"][0]["entry_id"],
        "kb_broken"
    );
    assert!(timeline_events(&c.pool).await.is_empty());
    assert!(!run_id.is_empty());

    // Regression (T4 dogfood): the detail endpoint must still serve this
    // failed run — 200, `error.code = invalid_input`, per-entry detail
    // readable — instead of 500 `SERIALIZATION_ERROR`.
    let detail = c
        .server
        .get(&format!("/v1/daemon/compute/runs/{run_id}"))
        .await;
    assert_eq!(
        detail.status_code(),
        StatusCode::OK,
        "detail body={}",
        detail.text()
    );
    let detail_body: Value = detail.json();
    assert_eq!(detail_body["status"], "failed");
    assert_eq!(detail_body["error"]["code"], "invalid_input");
    assert_eq!(
        detail_body["error"]["details"]["invalid_entries"][0]["entry_id"],
        "kb_broken"
    );
}

// ── W2 fix wave — subset-accept ────────────────────────────────────────────

/// W2: `timeline_event_ids_to_accept` appends ONLY the referenced proposed
/// events (state updates stay all-or-nothing).
#[tokio::test]
#[serial]
async fn accept_subset_appends_only_listed_events() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;

    let run_id = craft_succeeded_run(
        &c.pool,
        crafted_proposals(
            vec![json!({
                "op": "sub",
                "path": "character.current_hp",
                "target_key_block_id": "kb_def",
                "value": 15,
            })],
            &["First", "Second"],
            None,
        ),
    )
    .await;

    let resp = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({"timeline_event_ids_to_accept": ["evt_0"]}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["applied"]["events_created"], 1);
    assert_eq!(body["timeline_event_ids"].as_array().unwrap().len(), 1);

    // Only the first proposed event was appended (title "First"); the
    // second was NOT.  State delta still applied (all-or-nothing).
    let events = timeline_events(&c.pool).await;
    assert_eq!(events.len(), 1, "only the referenced event appended");
    let detail = c
        .server
        .get(&format!("/v1/daemon/compute/runs/{run_id}"))
        .await;
    let detail_body: Value = detail.json();
    let proposed: &Value = &detail_body["proposals"]["timeline_events"];
    assert_eq!(proposed[0]["title"], "First");
    assert_eq!(proposed[1]["title"], "Second");
    assert_eq!(defender_hp(&c.pool, "kb_def").await, 15);
}

/// W2: an unknown `timeline_event_ids_to_accept` id rejects the whole Accept
/// BEFORE any write (422 invalid_input; run stays succeeded).
#[tokio::test]
#[serial]
async fn accept_subset_with_unknown_id_returns_422_and_writes_nothing() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;

    let run_id = craft_succeeded_run(
        &c.pool,
        crafted_proposals(
            vec![json!({
                "op": "sub",
                "path": "character.current_hp",
                "target_key_block_id": "kb_def",
                "value": 15,
            })],
            &["Only"],
            None,
        ),
    )
    .await;

    let resp = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({"timeline_event_ids_to_accept": ["evt_9"]}))
        .await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");

    // Nothing written: no delta, no events, run still succeeded.
    assert_eq!(defender_hp(&c.pool, "kb_def").await, 30);
    assert!(timeline_events(&c.pool).await.is_empty());
    let row = nexus_local_db::compute_runs::get_run(&c.pool, &run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "succeeded");
}

/// N1: the wire field is nullable (`["array", "null"]`) — an explicit JSON
/// `null` must deserialize and behave exactly like an absent field (accept
/// ALL timeline events, state delta still applied).
#[tokio::test]
#[serial]
async fn accept_with_explicit_null_timeline_ids_accepts_all() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;

    let run_id = craft_succeeded_run(
        &c.pool,
        crafted_proposals(
            vec![json!({
                "op": "sub",
                "path": "character.current_hp",
                "target_key_block_id": "kb_def",
                "value": 15,
            })],
            &["First", "Second"],
            None,
        ),
    )
    .await;

    let resp = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{run_id}/accept"))
        .json(&json!({"timeline_event_ids_to_accept": null}))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["status"], "applied", "body={body}");
    assert_eq!(body["applied"]["events_created"], 2);
    assert_eq!(body["timeline_event_ids"].as_array().unwrap().len(), 2);

    // Both proposed events appended (accept-all), state delta still applied.
    let events = timeline_events(&c.pool).await;
    assert_eq!(events.len(), 2, "explicit null must accept all events");
    assert_eq!(defender_hp(&c.pool, "kb_def").await, 15);
}

// ── V1.147 P3 T2 — DELETE /v1/daemon/compute/runs (Clear history) ───────────

/// `DELETE /v1/daemon/compute/runs` with optional `world_id` / `status`.
async fn delete_runs(
    server: &TestServer,
    world_id: Option<&str>,
    status: Option<&str>,
) -> axum_test::TestResponse {
    let mut query: Vec<String> = Vec::new();
    if let Some(w) = world_id {
        query.push(format!("world_id={w}"));
    }
    if let Some(s) = status {
        query.push(format!("status={s}"));
    }
    let path = if query.is_empty() {
        "/v1/daemon/compute/runs".to_string()
    } else {
        format!("/v1/daemon/compute/runs?{}", query.join("&"))
    };
    server.delete(&path).await
}

/// Seed one run in EVERY lifecycle status on the owned combat world (plus a
/// terminal run on a foreign world) so Clear-scope tests can assert exactly
/// which rows survive. Returns the run ids by status.
struct RunStatusSeed {
    succeeded: String,
    applied: String,
    discarded: String,
    failed: String,
    running: String,
    foreign_failed: String,
}

async fn seed_run_status_mix(c: &Ctx) -> RunStatusSeed {
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;

    let succeeded = run_succeeded(c).await;

    let applied = run_succeeded(c).await;
    let accept = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{applied}/accept"))
        .json(&json!({}))
        .await;
    assert_eq!(
        accept.status_code(),
        StatusCode::OK,
        "body={}",
        accept.text()
    );

    let discarded = run_succeeded(c).await;
    let discard = c
        .server
        .post(&format!("/v1/daemon/compute/runs/{discarded}/discard"))
        .await;
    assert_eq!(
        discard.status_code(),
        StatusCode::OK,
        "body={}",
        discard.text()
    );

    let failed = nexus_local_db::compute_runs::insert_run(
        &c.pool,
        WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    nexus_local_db::compute_runs::set_run_failed(&c.pool, &failed, r#"{"code":"internal"}"#)
        .await
        .unwrap();

    let running = nexus_local_db::compute_runs::insert_run(
        &c.pool,
        WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // A terminal (failed) run on a world owned by ANOTHER creator — the
    // ownership gate must keep it untouched.
    seed_foreign_world(&c.pool).await;
    let foreign_failed = nexus_local_db::compute_runs::insert_run(
        &c.pool,
        FOREIGN_WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    nexus_local_db::compute_runs::set_run_failed(
        &c.pool,
        &foreign_failed,
        r#"{"code":"internal"}"#,
    )
    .await
    .unwrap();

    RunStatusSeed {
        succeeded,
        applied,
        discarded,
        failed,
        running,
        foreign_failed,
    }
}

/// Clear requires explicit scope — `DELETE /runs` without `world_id` is 422.
#[tokio::test]
#[serial]
async fn delete_runs_without_world_id_returns_422_scope_required() {
    let c = ctx().await;
    let resp = delete_runs(&c.server, None, None).await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");
}

/// Clear deletes ONLY terminal runs (applied|discarded|failed) of the OWNED
/// world and returns the deleted count. `running`, `succeeded` (needs review)
/// and foreign-world rows survive.
#[tokio::test]
#[serial]
async fn delete_runs_removes_terminal_runs_only_and_returns_deleted_count() {
    let c = ctx().await;
    let seed = seed_run_status_mix(&c).await;

    let resp = delete_runs(&c.server, Some(WORLD), None).await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["deleted"], 3, "body={body}");

    // Survivors: needs-review (succeeded), running, foreign terminal.
    for id in [&seed.succeeded, &seed.running, &seed.foreign_failed] {
        assert!(
            nexus_local_db::compute_runs::get_run(&c.pool, id)
                .await
                .unwrap()
                .is_some(),
            "{id} must survive Clear"
        );
    }
    // Deleted: applied, discarded, failed (the 3 terminal states).
    for id in [&seed.applied, &seed.discarded, &seed.failed] {
        assert!(
            nexus_local_db::compute_runs::get_run(&c.pool, id)
                .await
                .unwrap()
                .is_none(),
            "{id} must be deleted"
        );
    }
}

/// A non-terminal `status` filter is rejected — running/succeeded are never
/// deletable (422 `invalid_input`).
#[tokio::test]
#[serial]
async fn delete_runs_with_non_terminal_status_returns_422() {
    let c = ctx().await;
    let resp = delete_runs(&c.server, Some(WORLD), Some("running")).await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");

    let resp = delete_runs(&c.server, Some(WORLD), Some("succeeded")).await;
    assert_error_envelope(&resp, StatusCode::UNPROCESSABLE_ENTITY, "invalid_input");
}

/// A terminal `status` filter narrows Clear to exactly that state.
#[tokio::test]
#[serial]
async fn delete_runs_with_terminal_status_filter_deletes_only_matching() {
    let c = ctx().await;
    let seed = seed_run_status_mix(&c).await;

    let resp = delete_runs(&c.server, Some(WORLD), Some("failed")).await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["deleted"], 1, "body={body}");
    assert!(
        nexus_local_db::compute_runs::get_run(&c.pool, &seed.failed)
            .await
            .unwrap()
            .is_none(),
        "failed row must be deleted"
    );
    for id in [
        &seed.applied,
        &seed.discarded,
        &seed.succeeded,
        &seed.running,
    ] {
        assert!(
            nexus_local_db::compute_runs::get_run(&c.pool, id)
                .await
                .unwrap()
                .is_some(),
            "{id} must survive a failed-only Clear"
        );
    }
}

/// Ownership gate: a foreign world (and an unknown world) return 403 and no
/// row is touched.
#[tokio::test]
#[serial]
async fn delete_runs_on_foreign_or_unknown_world_returns_403() {
    let c = ctx().await;
    seed_foreign_world(&c.pool).await;
    let foreign = nexus_local_db::compute_runs::insert_run(
        &c.pool,
        FOREIGN_WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    nexus_local_db::compute_runs::set_run_failed(&c.pool, &foreign, r#"{"code":"internal"}"#)
        .await
        .unwrap();

    let resp = delete_runs(&c.server, Some(FOREIGN_WORLD), None).await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
    assert!(
        nexus_local_db::compute_runs::get_run(&c.pool, &foreign)
            .await
            .unwrap()
            .is_some(),
        "foreign run must survive a 403 Clear"
    );

    let resp = delete_runs(&c.server, Some("wld_nonexistent"), None).await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}

// ── W1 fix wave — newest-first list ordering ───────────────────────────────

/// W1: `GET /runs` returns runs newest-first and the cursor walks that order.
#[tokio::test]
#[serial]
async fn list_orders_newest_first() {
    let c = ctx().await;
    seed_character(&c.pool, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(&c.pool, "kb_def", "Guardian", 10, 5, 30, 50).await;
    let a = run_succeeded(&c).await;
    let b = run_succeeded(&c).await;
    let d = run_succeeded(&c).await;

    // Pin distinct created_at values so ordering does not depend on clock
    // precision within the same second.
    // SAFETY: test-only — pinning timestamps to verify ORDER BY semantics.
    for (run_id, ts) in [
        (&a, "2026-07-31T10:00:00.000Z"),
        (&b, "2026-07-31T10:00:01.000Z"),
        (&d, "2026-07-31T10:00:02.000Z"),
    ] {
        sqlx::query("UPDATE compute_sessions SET created_at = ? WHERE run_id = ?")
            .bind(ts)
            .bind(run_id)
            .execute(&c.pool)
            .await
            .unwrap();
    }

    let page1 = c.server.get("/v1/daemon/compute/runs?limit=2").await;
    assert_eq!(page1.status_code(), StatusCode::OK, "body={}", page1.text());
    let p1: Value = page1.json();
    let ids1: Vec<&str> = p1["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["run_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids1, vec![d.as_str(), b.as_str()], "newest first");
    let cursor = p1["next_cursor"].as_str().expect("cursor").to_string();

    let page2 = c
        .server
        .get(&format!("/v1/daemon/compute/runs?limit=2&cursor={cursor}"))
        .await;
    let p2: Value = page2.json();
    let ids2: Vec<&str> = p2["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["run_id"].as_str().unwrap())
        .collect();
    assert_eq!(ids2, vec![a.as_str()], "cursor continues the order");
    assert_eq!(p2["has_more"], false);
}
