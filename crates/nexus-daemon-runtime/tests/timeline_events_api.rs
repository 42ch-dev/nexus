//! V1.147 P2 — daemon route `GET /v1/daemon/worlds/:world_id/timeline/events`:
//! HTTP integration tests.
//!
//! Exercises the tier-2 cursor-paginated timeline-events read route over a
//! real axum router + `SQLite`:
//!
//! - happy list (seeded world + events incl. `compute_result` with extensions)
//! - branch filter
//! - status filter (enum, default `canon`)
//! - `event_type` exact-match filter
//! - cursor pagination (keyset on (`branch_id`, `sequence_no`))
//! - not-owner → 403, unknown world → 404
//! - empty list
//!
//! Auth is keyless (`DaemonApiConfig::keyless`); the tier-2
//! `require_active_creator` gate reads the active creator from the seeded
//! `config.toml` (`test_creator`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use axum::http::StatusCode;
use axum_test::TestServer;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils::{self, TestTempRoot};
use nexus_daemon_runtime::workspace::WorkspaceState;
use serde_json::{json, Value};

const ROOT_BRANCH: &str = "fbk_root";
const OTHER_BRANCH: &str = "fbk_other";
const FOREIGN_WORLD: &str = "wld_foreign";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

/// Keyless server + seeded test creator/world (`wld_test_world`).
async fn ctx() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    let app = api::create_router(state, DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("test server");
    Ctx {
        _tmp: tmp,
        server,
        pool,
    }
}

/// Seed a timeline event with deterministic fields.
#[allow(clippy::too_many_arguments)]
async fn seed_event(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    branch_id: &str,
    event_type: &str,
    status: &str,
    sequence_no: i64,
    title: &str,
    extensions_nexus_json: Option<&str>,
) {
    // SAFETY: test-only seed against the known narrative_timeline_events schema.
    sqlx::query(
        "INSERT INTO narrative_timeline_events \
            (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, \
             title, summary, metadata_json, extensions_nexus_json, created_at) \
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, '{}', ?, '2026-07-31T00:00:00Z')",
    )
    .bind(format!("evt_{world_id}_{branch_id}_{sequence_no}"))
    .bind(world_id)
    .bind(branch_id)
    .bind(event_type)
    .bind(status)
    .bind(sequence_no)
    .bind(title)
    .bind(Some(format!("summary of {title}")))
    .bind(extensions_nexus_json)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a foreign (other-creator) world.
async fn seed_foreign_world(pool: &sqlx::SqlitePool) {
    // SAFETY: test-only seed — other creator row + owned world.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data) \
         VALUES ('ctr_other', 'Other Creator', 'active', datetime('now'), '{}')",
    )
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES (?, 'ws', 'ctr_other', 'Foreign World', 'foreign-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(FOREIGN_WORLD)
    .execute(pool)
    .await
    .unwrap();
}

// `axum_test`'s AutoFuture is not `Send`; this helper only runs inside
// current-thread `#[tokio::test]` bodies, so the future need not be `Send`.
#[allow(clippy::future_not_send)]
async fn list_events(ctx: &Ctx, world_id: &str, query: &str) -> axum_test::TestResponse {
    ctx.server
        .get(&format!(
            "/v1/daemon/worlds/{world_id}/timeline/events{query}"
        ))
        .await
}

/// T1: happy list — canon default, root-branch default, `compute_result` with
/// extensions parsed, keyset ordering.
#[tokio::test]
async fn happy_list_includes_compute_result_with_extensions() {
    let ctx = ctx().await;
    let world = "wld_test_world";
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "story_advance",
        "canon",
        0,
        "Opening scene",
        None,
    )
    .await;
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "compute_result",
        "canon",
        1,
        "Combat resolution",
        Some(
            r#"{"compute":{"module_id":"basic-combat","module_version":"1.0.0","run_id":"run_1","source_kind":"direct_invoke"}}"#,
        ),
    )
    .await;

    let resp = list_events(&ctx, world, "").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["has_more"], false);
    assert!(body["next_cursor"].is_null());
    let items = body["items"].as_array().expect("items array");
    assert_eq!(items.len(), 2);

    // Keyset ordering: sequence_no ASC on the root branch.
    assert_eq!(items[0]["event_type"], "story_advance");
    assert_eq!(items[0]["sequence_no"], 0);
    assert_eq!(items[0]["status"], "canon");
    assert_eq!(items[0]["branch_id"], ROOT_BRANCH);
    assert_eq!(items[0]["id"], format!("evt_{world}_{ROOT_BRANCH}_0"));
    assert_eq!(items[0]["metadata"], json!({}));

    // compute_result carries parsed extensions.nexus provenance.
    assert_eq!(items[1]["event_type"], "compute_result");
    assert_eq!(items[1]["sequence_no"], 1);
    assert_eq!(
        items[1]["extensions"]["compute"]["module_id"],
        "basic-combat"
    );
    assert_eq!(items[1]["extensions"]["compute"]["run_id"], "run_1");
    assert_eq!(items[1]["title"], "Combat resolution");
    assert_eq!(items[1]["summary"], "summary of Combat resolution");
    assert!(!items[1]["created_at"].is_null());
}

/// V1.164 P3 T1 (AR-2): `TimelineEventInfo.modules` carries the
/// functional-dialect modules verbatim from `narrative_timeline_events
/// .modules_json` (event seeded with `modules.observation.observers`), and
/// stays absent for events without modules data.
#[tokio::test]
async fn list_serializes_modules_from_modules_json() {
    let ctx = ctx().await;
    let world = "wld_test_world";
    // Event WITHOUT modules data — `modules` must be absent from the wire.
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "story_advance",
        "canon",
        0,
        "Plain event",
        None,
    )
    .await;
    // Event WITH `modules.observation` — carried verbatim.
    // SAFETY: test-only seed against the known narrative_timeline_events
    // schema (incl. the V1.164 P1 `modules_json` column).
    sqlx::query(
        "INSERT INTO narrative_timeline_events \
            (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, \
             title, summary, metadata_json, modules_json, created_at) \
           VALUES (?, ?, ?, 'story_advance', 'canon', 1, 'Observed event', \
             'summary of Observed event', '{}', ?, '2026-07-31T00:00:00Z')",
    )
    .bind(format!("evt_{world}_{ROOT_BRANCH}_1"))
    .bind(world)
    .bind(ROOT_BRANCH)
    .bind(
        r#"{"observation":{"observers":["kb_char_1","kb_char_2"],"access":{"line_of_sight":true}}}"#,
    )
    .execute(&ctx.pool)
    .await
    .unwrap();

    let resp = list_events(&ctx, world, "").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let items = resp.json::<Value>()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 2);

    // Plain event: no modules key on the wire.
    assert_eq!(items[0]["event_type"], "story_advance");
    assert!(
        items[0].get("modules").is_none(),
        "event without modules_json must not emit a modules key: {}",
        items[0]
    );

    // Observed event: modules.observation carried verbatim.
    assert_eq!(items[1]["event_type"], "story_advance");
    assert_eq!(
        items[1]["modules"]["observation"]["observers"],
        json!(["kb_char_1", "kb_char_2"])
    );
    assert_eq!(
        items[1]["modules"]["observation"]["access"],
        json!({"line_of_sight": true})
    );
}

/// T1: default status filter is `canon` — provisional/rejected excluded.
#[tokio::test]
async fn default_status_filter_is_canon() {
    let ctx = ctx().await;
    let world = "wld_test_world";
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "story_advance",
        "canon",
        0,
        "Canon event",
        None,
    )
    .await;
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "story_advance",
        "provisional",
        1,
        "Provisional event",
        None,
    )
    .await;
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "story_advance",
        "rejected",
        2,
        "Rejected event",
        None,
    )
    .await;

    let resp = list_events(&ctx, world, "").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let items = resp.json::<Value>()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Canon event");

    // Explicit provisional filter selects only provisional rows.
    let resp = list_events(&ctx, world, "?status=provisional").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let items = resp.json::<Value>()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["title"], "Provisional event");
}

/// T1: explicit branch filter narrows to that branch only.
#[tokio::test]
async fn branch_filter_narrows_results() {
    let ctx = ctx().await;
    let world = "wld_test_world";
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "story_advance",
        "canon",
        0,
        "Root event",
        None,
    )
    .await;
    seed_event(
        &ctx.pool,
        world,
        OTHER_BRANCH,
        "story_advance",
        "canon",
        0,
        "Other branch event",
        None,
    )
    .await;

    let resp = list_events(&ctx, world, &format!("?branch_id={OTHER_BRANCH}")).await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let items = resp.json::<Value>()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["branch_id"], OTHER_BRANCH);
    assert_eq!(items[0]["title"], "Other branch event");
}

/// T1: `event_type` exact-match filter.
#[tokio::test]
async fn event_type_filter_narrows_results() {
    let ctx = ctx().await;
    let world = "wld_test_world";
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "story_advance",
        "canon",
        0,
        "Story",
        None,
    )
    .await;
    seed_event(
        &ctx.pool,
        world,
        ROOT_BRANCH,
        "compute_result",
        "canon",
        1,
        "Compute",
        None,
    )
    .await;

    let resp = list_events(&ctx, world, "?event_type=compute_result").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let items = resp.json::<Value>()["items"].as_array().unwrap().clone();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["event_type"], "compute_result");

    // Unknown event_type → empty list, not an error.
    let resp = list_events(&ctx, world, "?event_type=no_such_type").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
}

/// T1: keyset cursor pagination on (`branch_id`, `sequence_no`).
#[tokio::test]
async fn cursor_pagination() {
    let ctx = ctx().await;
    let world = "wld_test_world";
    for seq in 0..5 {
        seed_event(
            &ctx.pool,
            world,
            ROOT_BRANCH,
            "story_advance",
            "canon",
            seq,
            &format!("Event {seq}"),
            None,
        )
        .await;
    }

    let resp = list_events(&ctx, world, "?limit=2").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let page1: Value = resp.json();
    assert_eq!(page1["has_more"], true);
    let next_cursor = page1["next_cursor"]
        .as_str()
        .expect("next_cursor")
        .to_string();
    let items1 = page1["items"].as_array().unwrap().clone();
    assert_eq!(items1.len(), 2);
    assert_eq!(items1[0]["sequence_no"], 0);
    assert_eq!(items1[1]["sequence_no"], 1);

    let resp = list_events(&ctx, world, &format!("?limit=2&cursor={next_cursor}")).await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let page2: Value = resp.json();
    assert_eq!(page2["has_more"], true);
    let items2 = page2["items"].as_array().unwrap().clone();
    assert_eq!(items2.len(), 2);
    assert_eq!(items2[0]["sequence_no"], 2);
    assert_eq!(items2[1]["sequence_no"], 3);

    let resp = list_events(
        &ctx,
        world,
        &format!("?limit=2&cursor={}", page2["next_cursor"].as_str().unwrap()),
    )
    .await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let page3: Value = resp.json();
    assert_eq!(page3["has_more"], false);
    assert!(page3["next_cursor"].is_null());
    let items3 = page3["items"].as_array().unwrap().clone();
    assert_eq!(items3.len(), 1);
    assert_eq!(items3[0]["sequence_no"], 4);
}

/// T1: invalid cursor → 400 `invalid_input`.
#[tokio::test]
async fn invalid_cursor_rejected() {
    let ctx = ctx().await;
    let resp = list_events(&ctx, "wld_test_world", "?cursor=garbage").await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
}

/// T1: invalid status filter → 400 `invalid_input`.
#[tokio::test]
async fn invalid_status_rejected() {
    let ctx = ctx().await;
    let resp = list_events(&ctx, "wld_test_world", "?status=not_a_status").await;
    assert_eq!(resp.status_code(), StatusCode::BAD_REQUEST);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input");
}

/// T1: world owned by another creator → 403 forbidden.
#[tokio::test]
async fn not_owner_forbidden() {
    let ctx = ctx().await;
    seed_foreign_world(&ctx.pool).await;
    let resp = list_events(&ctx, FOREIGN_WORLD, "").await;
    assert_eq!(resp.status_code(), StatusCode::FORBIDDEN);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "forbidden");
}

/// T1: unknown world → 404 `not_found`.
#[tokio::test]
async fn unknown_world_not_found() {
    let ctx = ctx().await;
    let resp = list_events(&ctx, "wld_does_not_exist", "").await;
    assert_eq!(resp.status_code(), StatusCode::NOT_FOUND);
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "not_found");
}

/// T1: world with no events → empty list, `has_more` false.
#[tokio::test]
async fn empty_list() {
    let ctx = ctx().await;
    let resp = list_events(&ctx, "wld_test_world", "").await;
    assert_eq!(resp.status_code(), StatusCode::OK);
    let body: Value = resp.json();
    assert_eq!(body["items"].as_array().unwrap().len(), 0);
    assert_eq!(body["has_more"], false);
    assert!(body["next_cursor"].is_null());
}

/// QC W-1: `limit=0` must return an empty page with `has_more=false` +
/// `next_cursor=null` (the wire contract ties `has_more` to a non-null
/// `next_cursor`; `has_more=true` with a null cursor would growth-loop a
/// keyset client).
#[tokio::test]
async fn limit_zero_returns_no_more_pages() {
    let ctx = ctx().await;
    let world = "wld_test_world";
    // Seed events that WOULD paginate at a normal limit — limit=0 must not
    // report them as a continuation.
    for seq in 0..3 {
        seed_event(
            &ctx.pool,
            world,
            ROOT_BRANCH,
            "story_advance",
            "canon",
            seq,
            &format!("Event {seq}"),
            None,
        )
        .await;
    }

    let resp = list_events(&ctx, world, "?limit=0").await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(
        body["items"].as_array().unwrap().len(),
        0,
        "limit=0 must return an empty page: {body}"
    );
    assert_eq!(
        body["has_more"], false,
        "limit=0 must not report has_more: {body}"
    );
    assert!(
        body["next_cursor"].is_null(),
        "limit=0 must return a null next_cursor: {body}"
    );
}
