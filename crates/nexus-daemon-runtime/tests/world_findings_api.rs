//! V1.165 P1 T3 (DR-68, AR-3) — world findings read surface +
//! the R-V1164P2QC-001 completion E2E (non-empty-findings check).
//!
//! Proves end-to-end, over a real axum router + `SQLite`:
//!
//! - **AC-V165-1**: `POST /v1/daemon/check` on the box/basket World returns
//!   HTTP 200 (the V1.164 deterministic 500 is gone — DR-68) with ≥1
//!   `dramatic_irony_asymmetry` on Bo.
//! - **AC-V165-2/3**: world-scoped findings persist in the AR-1
//!   `world_findings` home with AC-V165-3 fields (`kind`, spoke severity
//!   `info` verbatim — NOT `minor`, `target_entry_id`, description naming
//!   actor + proposition + informing event, `extensions_json` with the
//!   stamped `nexus.world_id`).
//! - **AC-V165-4**: `GET /v1/daemon/worlds/:world_id/findings` lists them;
//!   owned world with zero findings → 200 + `[]`; unknown world → 404;
//!   foreign-owned world → 403 (guard parity with `kb/graph` /
//!   `timeline/events`).
//! - The read wire converts the stored INTEGER epoch → RFC 3339 and keeps
//!   severity/status spoke-verbatim (AR-1 vocabulary lock).
//!
//! Auth is keyless (`DaemonApiConfig::keyless`); the tier-2
//! `require_active_creator` gate reads the active creator from the seeded
//! `config.toml` (`test_creator`).
//!
//! Tests run on a multi-threaded tokio runtime (retained from the
//! pre-0.9.1 `block_in_place` bridge era; the adapter port methods are now
//! natively `async fn` — V1.153 P0 T2).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_local_db::world_findings::{insert_world_finding_in_tx, list_world_findings_by_world};
use serde_json::{json, Value};

/// World owned by `test_creator` (seeded by `seed_test_creator_and_world`).
const OWNED_WORLD: &str = "wld_test_world";
/// Second world owned by `test_creator`, with no false-belief candidates —
/// the AC-V165-4 empty case.
const EMPTY_WORLD: &str = "wld_empty_world";
/// World owned by `other_creator` (ownership-gate tests).
const FOREIGN_WORLD: &str = "wld_foreign";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

/// Standard server: seeded creator + owned world + foreign world under
/// keyless auth.
async fn ctx() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    seed_world(&pool, EMPTY_WORLD, "Empty World", "test_creator").await;
    seed_world(&pool, FOREIGN_WORLD, "Foreign World", "other_creator").await;
    let app = api::create_router(state, DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

/// Seed a world owned by `owner_id` (test-only; `INSERT OR IGNORE` keeps the
/// default test world intact).
async fn seed_world(pool: &sqlx::SqlitePool, world_id: &str, title: &str, owner_id: &str) {
    // SAFETY: test-only seed against the known creators/narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES (?, ?, 'active', datetime('now'), '{}')",
    )
    .bind(owner_id)
    .bind(title)
    .execute(pool)
    .await
    .unwrap();
    let slug = title.to_lowercase().replace(' ', "-");
    // SAFETY: test-only seed against the known narrative_worlds schema.
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES (?, 'ws', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(world_id)
    .bind(owner_id)
    .bind(title)
    .bind(&slug)
    .execute(pool)
    .await
    .unwrap();
}

/// Insert a `world_findings` row directly (test-only; bypasses the check
/// route to seed read-surface boundary cases).
async fn insert_finding(
    pool: &sqlx::SqlitePool,
    finding_id: &str,
    world_id: &str,
    created_at: i64,
) {
    let mut tx = pool.begin().await.unwrap();
    insert_world_finding_in_tx(
        &mut tx,
        finding_id,
        world_id,
        1,
        "info",
        "open",
        "A seeded finding",
        "Seeded body",
        Some("dramatic_irony_asymmetry"),
        None,
        Some(r#"{"event_id":"evt_transfer"}"#),
        None,
        r#"{"paragraph":1}"#,
        r#"{"nexus":{"world_id":"wld_any","creator_id":"test_creator"}}"#,
        created_at,
        created_at,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

/// Seed the box/basket World (spoke handbook worked example): the
/// world-state `info_point` + `kb_ana` / `kb_bo` character entries with
/// `modules.belief` rows, and the `evt_transfer` narrative timeline event
/// carrying `modules.observation.observers: ["kb_ana"]` (Bo does NOT
/// observe — the dramatic-irony thesis, AC-V1164-9).
async fn seed_box_basket(pool: &sqlx::SqlitePool, world_id: &str) {
    // World-state info_point: narrated facts, `holder: "world"` rows — never
    // candidates.
    seed_kb_entry(
        pool,
        world_id,
        "kb_world",
        "info_point",
        "World State",
        &json!({
            "belief": [
                { "holder": "world", "proposition": "The marble is in the basket",
                  "order": 0, "truth": "True" },
                { "holder": "world", "proposition": "Bo left the room",
                  "order": 0, "truth": "True" }
            ]
        }),
    )
    .await;
    // kb_ana: shared true belief — never a candidate.
    seed_kb_entry(
        pool,
        world_id,
        "kb_ana",
        "character",
        "Ana",
        &json!({
            "belief": [
                { "holder": "kb_ana", "proposition": "The marble is in the basket",
                  "order": 1, "truth": "True", "access": "Shared" }
            ]
        }),
    )
    .await;
    // kb_bo: the false-belief candidate (truth "False" — the divergence
    // signal; no world-row comparison).
    seed_kb_entry(
        pool,
        world_id,
        "kb_bo",
        "character",
        "Bo",
        &json!({
            "belief": [
                { "holder": "kb_bo", "proposition": "The marble is in the box",
                  "order": 1, "truth": "False", "access": "Private", "source": "Perception" }
            ]
        }),
    )
    .await;
    // evt_transfer: the informing event, observed by kb_ana only.
    // SAFETY: test-only seed against the known narrative_timeline_events
    // schema (incl. the V1.164 P1 `modules_json` column).
    sqlx::query(
        "INSERT INTO narrative_timeline_events \
            (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, \
             title, summary, metadata_json, modules_json, created_at) \
           VALUES (?, ?, ?, 'story_advance', 'canon', 1, 'Marble transfer', \
             'Ana moves the marble from box to basket', '{}', ?, '2026-08-01T00:00:00Z')",
    )
    .bind(format!("evt_{world_id}_transfer"))
    .bind(world_id)
    .bind("fbk_root")
    .bind(r#"{"observation":{"observers":["kb_ana"]}}"#)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed one `kb_key_blocks` row with its `modules_json` (V1.146 P4 column).
async fn seed_kb_entry(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    entry_id: &str,
    block_type: &str,
    canonical_name: &str,
    modules: &Value,
) {
    // SAFETY: test-only seed against the known kb_key_blocks schema (incl.
    // the V1.146 P4 `modules_json` column).
    sqlx::query(
        "INSERT INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, \
             created_at, updated_at, modules_json) \
           VALUES (?, ?, ?, ?, 'confirmed', '2026-08-01T00:00:00Z', \
             '2026-08-01T00:00:00Z', ?)",
    )
    .bind(entry_id)
    .bind(world_id)
    .bind(block_type)
    .bind(canonical_name)
    .bind(modules.to_string())
    .execute(pool)
    .await
    .unwrap();
}

/// Minimal valid check body: `scope.scope_id` anchored to `world_id`.
fn check_body(world_id: &str) -> Value {
    json!({ "world_id": world_id, "scope": { "scope_id": world_id } })
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

/// Assert an RFC 3339 datetime string (the epoch → RFC 3339 projection).
fn assert_rfc3339(s: &str) {
    chrono::DateTime::parse_from_rfc3339(s).expect("item timestamp must be RFC 3339");
}

// ─── The R-V1164P2QC-001 completion E2E (AC-V165-1/2/3) ───────────────────

/// Box/basket World → `POST /v1/daemon/check` returns **200** (not the
/// V1.164 deterministic 500), emits exactly one `dramatic_irony_asymmetry`
/// finding on Bo, persists it in the AR-1 `world_findings` home (spoke
/// severity `info` verbatim), and the read route returns it with the
/// AC-V165-3 fields.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn box_basket_check_200_persists_and_read_route_lists() {
    let ctx = ctx().await;
    seed_box_basket(&ctx.pool, OWNED_WORLD).await;

    // 1. POST /v1/daemon/check → 200 with the thesis finding (AC-V165-1).
    let resp = ctx
        .server
        .post("/v1/daemon/check")
        .json(&check_body(OWNED_WORLD))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    let findings = body["findings"].as_array().expect("findings array");
    let irony: Vec<&Value> = findings
        .iter()
        .filter(|f| f["kind"] == "dramatic_irony_asymmetry")
        .collect();
    assert_eq!(irony.len(), 1, "exactly one irony finding on Bo: {body}");
    let item = irony[0];
    assert_eq!(item["target_entry_id"], "kb_bo");
    assert_eq!(item["severity"], "info", "spoke severity verbatim");
    assert_eq!(item["status"], "open");
    let description = item["description"].as_str().expect("description");
    assert!(
        description.contains("kb_bo") && description.contains("The marble is in the box"),
        "description names actor + proposition: {description}"
    );

    // 2. Persistence in the AR-1 world-attached home (AC-V165-2/3).
    let rows = list_world_findings_by_world(&ctx.pool, OWNED_WORLD, 10)
        .await
        .expect("list world findings");
    assert_eq!(rows.len(), 1, "exactly one persisted row");
    let row = &rows[0];
    assert_eq!(row.severity, "info", "spoke severity verbatim in storage");
    assert_eq!(row.status, "open");
    assert_eq!(row.kind.as_deref(), Some("dramatic_irony_asymmetry"));
    assert_eq!(row.target_entry_id.as_deref(), Some("kb_bo"));
    assert!(
        row.description.contains("kb_bo") && row.description.contains("evt"),
        "description names actor + informing event: {}",
        row.description
    );
    let extensions: Value = serde_json::from_str(&row.extensions_json).expect("extensions_json");
    assert_eq!(
        extensions["nexus"]["world_id"], OWNED_WORLD,
        "stamped routing key rides extensions_json verbatim"
    );
    // The spoke finding carries no timestamps → the adapter stamps `now`
    // (legacy epoch convention); both columns must be populated.
    assert!(row.created_at > 0, "epoch created_at populated");
    assert!(row.updated_at > 0, "epoch updated_at populated");

    // 3. Read surface returns the persisted finding (AC-V165-3 + AR-3 wire).
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{OWNED_WORLD}/findings"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["truncated"], false);
    let items = body["findings"].as_array().expect("findings array");
    assert_eq!(items.len(), 1);
    let item = &items[0];
    assert_eq!(item["kind"], "dramatic_irony_asymmetry");
    assert_eq!(item["severity"], "info", "spoke severity verbatim on read");
    assert_eq!(item["target_entry_id"], "kb_bo");
    assert_eq!(item["finding_id"], row.finding_id);
    assert!(
        item["description"]
            .as_str()
            .expect("description")
            .contains("kb_bo"),
        "description names actor"
    );
    assert_rfc3339(item["created_at"].as_str().expect("created_at RFC 3339"));
    assert_rfc3339(item["updated_at"].as_str().expect("updated_at RFC 3339"));
    assert_eq!(
        item["extensions"]["nexus"]["world_id"], OWNED_WORLD,
        "stamped provenance rides extensions verbatim"
    );
}

// ─── AC-V165-4: empty / unknown / foreign ─────────────────────────────────

/// Owned world with no false-belief candidates → check 200 with `[]` and
/// the read route returns 200 + `{"findings": [], "truncated": false}`
/// (PD-3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn empty_world_check_200_and_read_200_empty() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .post("/v1/daemon/check")
        .json(&check_body(EMPTY_WORLD))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(
        body["findings"].as_array().expect("findings array").len(),
        0,
        "no false-belief candidates → zero findings: {body}"
    );

    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{EMPTY_WORLD}/findings"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["truncated"], false);
    assert_eq!(
        body["findings"].as_array().expect("findings array").len(),
        0,
        "empty world reads as an empty list: {body}"
    );
}

/// Unknown world → 404 (`require_world_owner` parity with `kb/graph`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_unknown_world_returns_404() {
    let ctx = ctx().await;
    let resp = ctx
        .server
        .get("/v1/daemon/worlds/wld_missing/findings")
        .await;
    assert_error_envelope(&resp, StatusCode::NOT_FOUND, "not_found");
}

/// Foreign-owned world → 403 (cross-author; world existence stays
/// unobservable — `require_world_owner` parity).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_foreign_world_returns_403() {
    let ctx = ctx().await;
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{FOREIGN_WORLD}/findings"))
        .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}

// ─── Bugbot 4bad2fca: SQL-side LIMIT boundary ─────────────────────────────

/// The read route bounds the store SQL-side (Bugbot 4bad2fca): with
/// `CAP + 2` (502) stored findings the response carries exactly the newest
/// 500 and flags `truncated: true`; with fewer than the cap it returns all
/// rows and flags `truncated: false`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_route_caps_and_flags_truncation() {
    let ctx = ctx().await;

    // 502 stored rows (CAP 500 + 2 overflow) — the Bugbot scenario.
    for i in 0..502 {
        insert_finding(
            &ctx.pool,
            &format!("fnd_bulk_{i:03}"),
            OWNED_WORLD,
            1_000 + i64::from(i), // monotonic created_at → deterministic order
        )
        .await;
    }
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{OWNED_WORLD}/findings"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["truncated"], true, "overflow must be flagged: {body}");
    let items = body["findings"].as_array().expect("findings array");
    assert_eq!(items.len(), 500, "exactly the newest 500: {body}");

    // Fewer than the cap → all rows, honest `truncated: false`.
    for i in 0..3 {
        insert_finding(
            &ctx.pool,
            &format!("fnd_few_{i}"),
            EMPTY_WORLD,
            2_000 + i64::from(i),
        )
        .await;
    }
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{EMPTY_WORLD}/findings"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["truncated"], false, "fewer than cap: {body}");
    assert_eq!(
        body["findings"].as_array().expect("findings array").len(),
        3,
        "all stored rows returned when under the cap: {body}"
    );
}
