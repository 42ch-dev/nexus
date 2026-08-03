//! V1.148 P2 — `POST /v1/daemon/check` integration tests.
//!
//! Proves the spoke `orchestrate_check` Daemon HTTP surface end-to-end
//! (closes V1.146 Non-Goal 5a — the check op is lib-import-only until this
//! route) over a real axum router + SQLite:
//!
//! - Happy path (owned World; seeded `spoke_rules` row resolved via
//!   `RuleQueryPort`) → 200, `findings` array (empty with the baseline
//!   no-op checker — rule *resolution* is still exercised)
//! - Ownership reject (World owned by a different creator) → 403
//! - Scope consistency (`scope.scope_id != world_id`) → 400
//! - Empty `rule_refs` + no embedded rules → 200, `findings: []`
//! - No active creator (tier2 `require_active_creator` gate) → 409
//!
//! Auth is keyless (`DaemonApiConfig::keyless`); the tier-2
//! `require_active_creator` gate reads the active creator from the seeded
//! `config.toml` (`test_creator`).
//!
//! All tests run on a multi-threaded tokio runtime: `NexusAdapter::new`
//! panics under a `current_thread` runtime (`block_in_place` requirement —
//! same rationale as the `world_kb_patch` tests).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::spoke_rules::{insert_spoke_rule_for_test, SpokeRuleRow};
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

/// Server without an active creator (config.toml removed) — the tier2
/// `require_active_creator` gate must reject with 409 before the handler runs.
async fn ctx_without_creator() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    std::fs::remove_file(nexus_home.join("config.toml")).expect("remove config.toml");
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    let app = api::create_router(state, DaemonApiConfig::keyless());
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

/// Seed a `spoke_rules` row through the P1 test helper (the only rule writer —
/// no author-facing rule write API exists) so the happy path proves
/// `RuleQueryPort` resolution inside `orchestrate_check` end-to-end.
async fn seed_rule(pool: &sqlx::SqlitePool, rule_id: &str, world_id: &str) {
    let row = SpokeRuleRow {
        rule_id: rule_id.to_string(),
        world_id: world_id.to_string(),
        schema_version: 1,
        canonical_name: format!("Rule {rule_id}"),
        kind: "rule".to_string(),
        statement: Some(format!("Statement for {rule_id}")),
        description: Some(format!("Description for {rule_id}")),
        target_entry_types_json: "[\"character\", \"event\"]".to_string(),
        severity_hint: Some("warning".to_string()),
        status: Some("active".to_string()),
        source_anchor_json: None,
        extensions_json: "{}".to_string(),
        created_at: Some(1_700_000_000),
        updated_at: Some(1_700_000_100),
    };
    insert_spoke_rule_for_test(pool, &row).await.unwrap();
}

/// Minimal valid check body: `scope.scope_id` anchored to `world_id`.
fn check_body(world_id: &str, scope_id: &str) -> Value {
    json!({ "world_id": world_id, "scope": { "scope_id": scope_id } })
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

// ─── POST /v1/daemon/check ────────────────────────────────────────────────

/// Happy path: owned World + seeded rule referenced via `rule_refs` → 200 with
/// a `findings` array. The baseline no-op checker produces zero findings, but
/// the seeded rule is still *resolved* by `RuleQueryPort` inside
/// `orchestrate_check` (a broken rule-resolution path would reject here).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_owned_world_with_seeded_rule_returns_200_findings() {
    let ctx = ctx().await;
    seed_rule(&ctx.pool, "check_rule_1", OWNED_WORLD).await;

    let body = json!({
        "world_id": OWNED_WORLD,
        "scope": { "scope_id": OWNED_WORLD },
        "rule_refs": ["check_rule_1"],
    });
    let resp = ctx.server.post("/v1/daemon/check").json(&body).await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());

    let body: Value = resp.json();
    let findings = body["findings"].as_array().expect("findings array");
    assert!(
        findings.is_empty(),
        "baseline no-op checker must report zero findings: {body}"
    );
}

/// Ownership gate: a World owned by another creator must reject with 403
/// before any check behavior runs (compute_runs pattern — world existence
/// stays unobservable to foreign creators).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_foreign_world_rejects_403() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/check")
        .json(&check_body(FOREIGN_WORLD, FOREIGN_WORLD))
        .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}

/// Scope consistency: `scope.scope_id` must equal `world_id` (architect lock —
/// the spoke scope selector is anchored to the owned World). Mismatch → 400.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_scope_id_mismatch_rejects_400() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/check")
        .json(&check_body(OWNED_WORLD, FOREIGN_WORLD))
        .await;
    assert_error_envelope(&resp, StatusCode::BAD_REQUEST, "invalid_input");
}

/// Empty `rule_refs` + no embedded `rules`: still a valid check (rules are
/// optional in the spoke wire) → 200 with an empty findings array.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_empty_rules_returns_200_empty_findings() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/check")
        .json(&check_body(OWNED_WORLD, OWNED_WORLD))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());

    let body: Value = resp.json();
    let findings = body["findings"].as_array().expect("findings array");
    assert!(
        findings.is_empty(),
        "empty-rule check must report zero findings: {body}"
    );
}

/// Tier2 gate: with no active creator configured, `require_active_creator`
/// rejects with 409 `uninitialized` before the handler runs (same status the
/// compute_runs / memory routes use — verified against tier2 reality).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_without_active_creator_rejects_409() {
    let ctx = ctx_without_creator().await;

    let resp = ctx
        .server
        .post("/v1/daemon/check")
        .json(&check_body(OWNED_WORLD, OWNED_WORLD))
        .await;
    assert_error_envelope(&resp, StatusCode::CONFLICT, "uninitialized");
}
