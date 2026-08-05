//! V1.151 P0 — `POST /v1/daemon/inspector/moment` integration tests.
//!
//! Proves the enriched assembly inspector packet Daemon HTTP surface
//! end-to-end (DF-76 T2) over a real `axum` router + `SQLite`:
//!
//! - Happy path (owned World; seeded `kb_key_blocks` lore row carrying a
//!   `constant` activation module — always fires) → 200 with the full packet
//!   contract: `modules.placement` (non-empty — the constant seed is
//!   accepted), `modules.activation_trace`, `slot_map` (entry routed to the
//!   `default` slot), `budget` accounting, and `moment_directive`
//!   status/metadata (`"none"` + nulls — no directive store on this path;
//!   the directive body is never on the wire, AC-I3).
//! - Unauthenticated request (keyed-all auth, no `X-API-Key` header) → 401
//!   `auth_required`.
//! - Ownership reject (World owned by a different creator) → 403
//!   `forbidden`.
//!
//! Auth is keyless for the happy/ownership tests (`DaemonApiConfig::keyless`);
//! the unauthenticated case uses a keyed-all config
//! (`DaemonApiConfig::keyed`). The tier-2 `require_active_creator` gate reads
//! the active creator from the seeded `config.toml` (`test_creator`).

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
/// Lore entry id seeded into `kb_key_blocks` for the happy path.
const LORE_ENTRY: &str = "kb_inspector_lore";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

/// Standard server: seeded creator + owned world (+ foreign world for the
/// ownership gate) under keyless auth.
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

/// Server under keyed-all auth (no active-creator removal): a request
/// without the `X-API-Key` header must be rejected 401 by `require_api_key`
/// before any handler runs.
async fn ctx_keyed() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    let app = api::create_router(state, DaemonApiConfig::keyed("test-key"));
    let server = TestServer::new(app).expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

/// Seed a second World owned by a *different* creator (ownership-gate tests).
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

/// Seed one lore row into `kb_key_blocks` for the happy path: a `constant`
/// activation module (empty keys) is an always-on seed — it fires
/// regardless of scan text, so `modules.placement` is deterministically
/// non-empty (spec §2.1 / V1.149 P0 T3 constant band).
async fn seed_inspector_lore(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed against the known kb_key_blocks schema
    // (20260525_kb_key_blocks.sql + 20260731120000_modules_json.sql).
    sqlx::query(
        "INSERT OR IGNORE INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, created_at, modules_json) \
           VALUES (?, ?, 'character', 'Harbor Master', 'confirmed', datetime('now'), \
             '{\"activation\":{\"keys\":[],\"constant\":true}}')",
    )
    .bind(LORE_ENTRY)
    .bind(OWNED_WORLD)
    .execute(pool)
    .await
    .unwrap();
}

/// Minimal valid inspector body: `world_id` required, `work_id` /
/// `generation_stage` optional.
fn inspect_body(world_id: &str) -> Value {
    json!({ "world_id": world_id })
}

/// Seed an active World-scoped Moment Directive for the owned World (T3
/// wire-up test): the inspector packet then reflects it — status/metadata
/// only, never the body (AC-I3).
async fn seed_active_world_directive(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed through the public directive repository.
    let new = nexus_local_db::moment_directive::NewMomentDirective {
        directive_id: "dir_inspector_active",
        creator_id: "test_creator",
        scope_kind: nexus_local_db::moment_directive::scope_kind::WORLD,
        scope_id: OWNED_WORLD,
        body: "Keep the prose terse.",
        insert_depth: "mid",
        ttl_kind: "generations",
        ttl_remaining: 5,
        clear_on_scene_change: false,
        now: 1_780_000_000_000,
    };
    nexus_local_db::moment_directive::set_active(pool, &new)
        .await
        .expect("seed active world directive");
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

// ─── POST /v1/daemon/inspector/moment ────────────────────────────────────

/// Happy path: owned World + seeded lore with a constant activation module
/// → 200 with the full packet contract — `modules.placement` (the constant
/// seed is accepted), `modules.activation_trace`, `slot_map` (entry routed
/// to the `default` slot), `budget` accounting, and `moment_directive`
/// status/metadata (`"none"` + nulls; no directive store on this path —
/// AC-I1b).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspect_moment_owned_world_returns_200_full_packet() {
    let ctx = ctx().await;
    seed_inspector_lore(&ctx.pool).await;

    let resp = ctx
        .server
        .post("/v1/daemon/inspector/moment")
        .json(&inspect_body(OWNED_WORLD))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());

    let body: Value = resp.json();

    // modules.placement — the constant seed must be accepted.
    let placement = body["modules"]["placement"]
        .as_array()
        .expect("modules.placement array");
    assert_eq!(placement.len(), 1, "constant seed must be placed: {body}");
    assert_eq!(placement[0]["entry_id"], LORE_ENTRY, "body={body}");
    assert_eq!(placement[0]["canonical_name"], "Harbor Master", "body={body}");

    // modules.activation_trace — full per-entry fire/miss trace.
    let trace = body["modules"]["activation_trace"]
        .as_array()
        .expect("modules.activation_trace array");
    assert_eq!(trace.len(), 1, "one entry in the trace: {body}");
    assert_eq!(trace[0]["accepted"], true, "constant seed must fire: {body}");
    assert!(
        trace[0]["reason"]
            .as_str()
            .is_some_and(|r| r.contains("constant seed")),
        "fire reason must name the constant band: {body}"
    );

    // slot_map — the accepted entry routed to the `default` slot (no
    // position_hint on the seed).
    let slot_map = body["slot_map"].as_array().expect("slot_map array");
    assert_eq!(slot_map.len(), 1, "one routed entry: {body}");
    assert_eq!(slot_map[0]["entry_id"], LORE_ENTRY, "body={body}");
    assert_eq!(slot_map[0]["slot"], "default", "body={body}");

    // budget — activation token accounting always present.
    let budget = body["budget"].as_object().expect("budget object");
    for key in [
        "primary_tokens_est",
        "hop_tokens_est",
        "cap",
        "remaining",
    ] {
        assert!(budget.contains_key(key), "budget must carry {key}: {body}");
    }

    // moment_directive — status/metadata only, "none" + nulls when no
    // directive injected (AC-I3: the directive body is NEVER on the wire).
    let directive = body["moment_directive"]
        .as_object()
        .expect("moment_directive object");
    assert_eq!(directive["status"], "none", "body={body}");
    assert!(directive["scope"].is_null(), "body={body}");
    assert!(
        !directive.contains_key("body"),
        "directive body must never appear on the wire (AC-I3): {body}"
    );
}

/// T3 wire-up: with an active World-scoped directive seeded, the inspector
/// packet's `moment_directive` section reflects it (status/metadata only —
/// no `body`, AC-I3). Without a directive the section renders `"none"`
/// (AC-I1b, covered by the happy path above).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspect_moment_reflects_active_directive() {
    let ctx = ctx().await;
    seed_active_world_directive(&ctx.pool).await;

    let resp = ctx
        .server
        .post("/v1/daemon/inspector/moment")
        .json(&inspect_body(OWNED_WORLD))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());

    let body: Value = resp.json();
    let directive = body["moment_directive"]
        .as_object()
        .expect("moment_directive object");
    assert_eq!(directive["status"], "active", "body={body}");
    assert_eq!(directive["scope"], "world", "body={body}");
    assert_eq!(directive["scope_id"], OWNED_WORLD, "body={body}");
    assert_eq!(directive["insert_depth"], "mid", "body={body}");
    assert_eq!(directive["ttl_kind"], "generations", "body={body}");
    assert!(
        !directive.contains_key("body"),
        "directive body must never appear on the wire (AC-I3): {body}"
    );
}

/// Unauthenticated request: under keyed-all auth, a request without the
/// `X-API-Key` header is rejected 401 `auth_required` by `require_api_key`
/// before any handler runs.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspect_moment_unauthenticated_rejects_401() {
    let ctx = ctx_keyed().await;

    let resp = ctx
        .server
        .post("/v1/daemon/inspector/moment")
        .json(&inspect_body(OWNED_WORLD))
        .await;
    assert_error_envelope(&resp, StatusCode::UNAUTHORIZED, "auth_required");
}

/// Ownership gate: a World owned by another creator must reject with 403
/// before any assembly behavior runs (`compute_runs` pattern — world existence
/// stays unobservable to foreign creators).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn inspect_moment_foreign_world_rejects_403() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/inspector/moment")
        .json(&inspect_body(FOREIGN_WORLD))
        .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}
