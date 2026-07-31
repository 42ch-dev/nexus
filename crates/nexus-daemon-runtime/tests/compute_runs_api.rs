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
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{WorldKbBody, WorldKbEntry};
    use nexus_knowledge::world_kb::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;

    let kb = WorldKbEntry {
        entry_id: entry_id.to_string(),
        world_id: WORLD.to_string(),
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
        ..WorldKbEntry::new(WORLD, BlockType::Character, name)
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
async fn timeline_events(pool: &sqlx::SqlitePool) -> Vec<(String, String, Option<String>)> {
    sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT timeline_event_id, event_type, extensions_nexus_json \
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

    // Timeline events created with `compute_result` + compute provenance.
    let events = timeline_events(&c.pool).await;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0].1, "compute_result");
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
