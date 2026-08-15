//! V1.166 P1 T4 (DR-64, AR-3/AR-4) — rules read surface +
//! the AC-V166-1/2/4 composition E2E over a real axum router + `SQLite`.
//!
//! Proves end-to-end:
//!
//! - **AC-V166-1**: CLI-authored rules (storage insert via the T2 carrier
//!   shape) + `POST /v1/daemon/check` with **empty `rule_refs`** (auto
//!   include) → **200**; findings = mental pair ∪ rule-derived (`kind` =
//!   family), persisted in the `world_findings` home (read back via
//!   `GET .../findings`); rules readable via `GET .../rules` (PD-2 fields +
//!   first-class AR-2 constraint, no extensions bag).
//! - **AC-V166-2**: a foreign-world `rule_refs` id rejects the **whole**
//!   check with 400 `invalid_input` naming the id — no partial evaluation,
//!   no partial persistence.
//! - **AR-1**: a request carrying embedded `rules` → 400 `invalid_input`
//!   (embedded interchange is not an authoring path).
//! - **PD-1 fast path**: zero active rules (only draft) → 200 with
//!   mental-pair-only findings (no rule-family kinds).
//! - **AC-V166-4**: the mental checker pair regression, composed beside the
//!   rule evaluator (the box/basket irony finding still fires).
//! - **AR-3 read surface**: guards (401/404/403 parity with findings), cap
//!   500 + SQL-side `LIMIT ?` probe + honest `truncated`, store order
//!   `canonical_name ASC, rule_id ASC`, epoch → RFC 3339, spoke vocabulary
//!   verbatim.
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
use nexus_local_db::spoke_rules::{insert_rule, SpokeRuleRow};
use nexus_local_db::world_findings::list_world_findings_by_world;
use serde_json::{json, Value};

/// World owned by `test_creator` (seeded by `seed_test_creator_and_world`).
const OWNED_WORLD: &str = "wld_test_world";
/// Second world owned by `test_creator` — the zero-active-rules (draft-only)
/// case and the truncation bulk target.
const DRAFT_WORLD: &str = "wld_draft_world";
/// Third world owned by `test_creator` — the zero-rules empty case.
const EMPTY_WORLD: &str = "wld_empty_world";
/// World owned by `other_creator` (ownership-gate tests).
const FOREIGN_WORLD: &str = "wld_foreign";

struct Ctx {
    _tmp: TestTempRoot,
    server: TestServer,
    pool: sqlx::SqlitePool,
}

/// Standard server: seeded creator + owned worlds + foreign world under
/// keyless auth.
async fn ctx() -> Ctx {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let pool = state.pool().expect("pool").clone();
    test_utils::seed_test_creator_and_world(&pool).await;
    seed_world(&pool, DRAFT_WORLD, "Draft World", "test_creator").await;
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

/// Seed one `kb_key_blocks` row with `body_json` + `modules_json`
/// (V1.146 P4 column).
async fn seed_kb_entry(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    entry_id: &str,
    block_type: &str,
    canonical_name: &str,
    body: &Value,
    modules: &Value,
) {
    // SAFETY: test-only seed against the known kb_key_blocks schema.
    sqlx::query(
        "INSERT INTO kb_key_blocks \
            (key_block_id, world_id, block_type, canonical_name, status, \
             created_at, updated_at, body_json, modules_json) \
           VALUES (?, ?, ?, ?, 'confirmed', '2026-08-01T00:00:00Z', \
             '2026-08-01T00:00:00Z', ?, ?)",
    )
    .bind(entry_id)
    .bind(world_id)
    .bind(block_type)
    .bind(canonical_name)
    .bind(body.to_string())
    .bind(modules.to_string())
    .execute(pool)
    .await
    .unwrap();
}

/// Seed one `narrative_timeline_events` row with `modules_json`
/// (V1.164 P1 column).
async fn seed_event(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    event_id: &str,
    canonical_name: &str,
    sequence_no: i64,
    modules: &Value,
) {
    // SAFETY: test-only seed against the known narrative_timeline_events
    // schema (incl. the V1.164 P1 `modules_json` column).
    sqlx::query(
        "INSERT INTO narrative_timeline_events \
            (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, \
             title, summary, metadata_json, modules_json, created_at) \
           VALUES (?, ?, ?, 'story_advance', 'canon', ?, ?, ?, '{}', ?, '2026-08-01T00:00:00Z')",
    )
    .bind(event_id)
    .bind(world_id)
    .bind("fbk_root")
    .bind(sequence_no)
    .bind(canonical_name)
    .bind(canonical_name)
    .bind(modules.to_string())
    .execute(pool)
    .await
    .unwrap();
}

/// Production rule insert (T2 carrier shape): `extensions_json` =
/// `{"nexus": {"constraint": <carrier verbatim>}}` — the CLI `rule add`
/// row-assembly contract, exercised here via the storage seam.
#[allow(clippy::too_many_arguments)]
async fn seed_rule(
    pool: &sqlx::SqlitePool,
    rule_id: &str,
    world_id: &str,
    canonical_name: &str,
    severity_hint: Option<&str>,
    status: &str,
    target_entry_types: &[&str],
    carrier: &Value,
) {
    let row = SpokeRuleRow {
        rule_id: rule_id.to_string(),
        world_id: world_id.to_string(),
        schema_version: 1,
        canonical_name: canonical_name.to_string(),
        kind: "rule".to_string(),
        statement: Some(format!("Human summary for {canonical_name}")),
        description: None,
        target_entry_types_json: serde_json::to_string(target_entry_types).expect("json"),
        severity_hint: severity_hint.map(str::to_string),
        status: Some(status.to_string()),
        source_anchor_json: None,
        extensions_json: json!({ "nexus": { "constraint": carrier } }).to_string(),
        created_at: Some(1_700_000_000),
        updated_at: Some(1_700_000_100),
    };
    insert_rule(pool, &row).await.unwrap();
}

/// Minimal valid check body: `scope.scope_id` anchored to `world_id`
/// (no `rule_refs` → AR-1 auto-include of the world's active rules).
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

// ─── The AC-V166-1 composition E2E ────────────────────────────────────────

/// Box/basket World + rule-authoring entries/events + three active rules →
/// `POST /v1/daemon/check` with **empty `rule_refs`** (auto-include) → 200;
/// findings = mental pair (irony on Bo) ∪ rule-derived (`required_field`
/// error on the summary-less entry, `observer_cardinality` warning on the
/// over-observed event); non-matching rule emits nothing; persisted in
/// `world_findings`; both read routes list them (PD-2 fields + first-class
/// constraint, RFC 3339, no extensions bag on the rules wire).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
// ^ one linear end-to-end scenario (seed → check → persist → both read
// routes); splitting would fragment the AC-V166-1 flow it documents
// (findings_api.rs too_many_lines precedent).
#[allow(clippy::too_many_lines)]
async fn check_composes_mental_and_rule_findings_and_read_routes_list() {
    let ctx = ctx().await;

    // Mental-pair fixture (box/basket worked example): kb_world (info_point),
    // kb_ana (shared true belief), kb_bo (false belief — the irony thesis
    // when the informing event is observed by kb_ana only). All carry body
    // summaries so they satisfy the required_field rule below.
    seed_kb_entry(
        &ctx.pool,
        OWNED_WORLD,
        "kb_world",
        "info_point",
        "World State",
        &json!({}),
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
    seed_kb_entry(
        &ctx.pool,
        OWNED_WORLD,
        "kb_ana",
        "character",
        "Ana",
        &json!({ "summary": "Ana's summary" }),
        &json!({
            "belief": [
                { "holder": "kb_ana", "proposition": "The marble is in the basket",
                  "order": 1, "truth": "True", "access": "Shared" }
            ]
        }),
    )
    .await;
    seed_kb_entry(
        &ctx.pool,
        OWNED_WORLD,
        "kb_bo",
        "character",
        "Bo",
        &json!({ "summary": "Bo's summary" }),
        &json!({
            "belief": [
                { "holder": "kb_bo", "proposition": "The marble is in the box",
                  "order": 1, "truth": "False", "access": "Private", "source": "Perception" }
            ]
        }),
    )
    .await;
    // Rule-evaluator fixtures: kb_hero satisfies required_field body.summary;
    // kb_shadow violates it (no populated summary).
    seed_kb_entry(
        &ctx.pool,
        OWNED_WORLD,
        "kb_hero",
        "character",
        "Hero",
        &json!({ "summary": "A hero" }),
        &json!({}),
    )
    .await;
    seed_kb_entry(
        &ctx.pool,
        OWNED_WORLD,
        "kb_shadow",
        "character",
        "Shadow",
        &json!({}),
        &json!({}),
    )
    .await;
    // The informing event, observed by kb_ana only (1 observer — violates an
    // observer_cardinality max 0 rule).
    seed_event(
        &ctx.pool,
        OWNED_WORLD,
        "evt_wld_test_world_transfer",
        "Marble transfer",
        1,
        &json!({ "observation": { "observers": ["kb_ana"] } }),
    )
    .await;

    // CLI-equivalent rule authoring (T2 carrier shape): one matching
    // required_field rule (severity_hint error), one matching
    // observer_cardinality rule (no hint → uniform warning), one non-matching
    // module_absence rule (no entry carries modules.backstory → nothing to
    // forbid → no finding).
    seed_rule(
        &ctx.pool,
        "rul_need_summary",
        OWNED_WORLD,
        "Characters need summaries",
        Some("error"),
        "active",
        &["character"],
        &json!({ "family": "required_field", "field": "body.summary" }),
    )
    .await;
    seed_rule(
        &ctx.pool,
        "rul_obs_cap",
        OWNED_WORLD,
        "Observer cap",
        None,
        "active",
        &[],
        &json!({ "family": "observer_cardinality", "min": 0, "max": 0 }),
    )
    .await;
    seed_rule(
        &ctx.pool,
        "rul_backstory",
        OWNED_WORLD,
        "Backstory required",
        Some("info"),
        "active",
        &["character"],
        &json!({ "family": "module_absence", "module_key": "backstory" }),
    )
    .await;

    // 1. POST /v1/daemon/check with EMPTY rule_refs → 200, composed findings.
    let resp = ctx
        .server
        .post("/v1/daemon/check")
        .json(&check_body(OWNED_WORLD))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    let findings = body["findings"].as_array().expect("findings array");

    let by_kind: Vec<(&str, &str)> = findings
        .iter()
        .map(|f| {
            (
                f["kind"].as_str().expect("kind"),
                f["target_entry_id"].as_str().expect("target"),
            )
        })
        .collect();
    assert!(
        by_kind.contains(&("dramatic_irony_asymmetry", "kb_bo")),
        "mental pair fires beside the rules: {by_kind:?}"
    );
    assert!(
        by_kind.contains(&("required_field", "kb_shadow")),
        "required_field on the summary-less entry: {by_kind:?}"
    );
    assert!(
        by_kind.contains(&("observer_cardinality", "evt_wld_test_world_transfer")),
        "observer_cardinality on the over-observed event: {by_kind:?}"
    );
    assert!(
        !by_kind.contains(&("module_absence", "kb_shadow")),
        "non-matching module_absence rule emits nothing (nothing carries the key): {by_kind:?}"
    );
    assert!(
        !by_kind.contains(&("required_field", "kb_hero")),
        "satisfying entry emits no required_field finding: {by_kind:?}"
    );
    assert_eq!(
        findings.len(),
        3,
        "exactly the three expected findings: {body}"
    );

    // Finding identity per AR-4: kind = family; severity = severity_hint
    // verbatim else uniform warning; title = "{family}: {canonical_name}";
    // description names the rule canonical_name, never quotes statement.
    let required = findings
        .iter()
        .find(|f| f["kind"] == "required_field")
        .expect("required_field finding");
    assert_eq!(required["severity"], "error", "severity_hint verbatim");
    assert_eq!(required["status"], "open");
    assert_eq!(required["title"], "required_field: Shadow");
    assert_eq!(
        required["extensions"]["nexus"]["world_id"], OWNED_WORLD,
        "routing key stamped"
    );
    assert_eq!(
        required["extensions"]["nexus"]["creator_id"], "test_creator",
        "creator provenance stamped"
    );
    let description = required["description"].as_str().expect("description");
    assert!(
        description.contains("Characters need summaries")
            && description.contains("body.summary")
            && description.contains("kb_shadow"),
        "deterministic English line naming rule + operator + entry: {description}"
    );
    assert!(
        !description.contains("Human summary for"),
        "description never quotes statement (PD-1): {description}"
    );
    let obs = findings
        .iter()
        .find(|f| f["kind"] == "observer_cardinality")
        .expect("observer_cardinality finding");
    assert_eq!(
        obs["severity"], "warning",
        "no severity_hint → uniform warning default (AR-4)"
    );

    // 2. Persistence: rule-derived findings land in world_findings (AC-V166-1).
    let rows = list_world_findings_by_world(&ctx.pool, OWNED_WORLD, 10)
        .await
        .expect("list world findings");
    assert_eq!(rows.len(), 3, "all three findings persisted");
    assert_eq!(rows[0].severity, "info"); // irony
    let kinds: Vec<Option<String>> = rows.iter().map(|r| r.kind.clone()).collect();
    for expected in [
        "dramatic_irony_asymmetry",
        "required_field",
        "observer_cardinality",
    ] {
        assert!(
            kinds.iter().any(|k| k.as_deref() == Some(expected)),
            "persisted kind {expected}: {kinds:?}"
        );
    }
    let required_row = rows
        .iter()
        .find(|r| r.kind.as_deref() == Some("required_field"))
        .expect("required_field row");
    assert_eq!(
        required_row.severity, "error",
        "spoke severity verbatim at rest"
    );
    assert_eq!(required_row.target_entry_id.as_deref(), Some("kb_shadow"));

    // 3. GET /v1/daemon/worlds/:world_id/findings → the persisted findings.
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{OWNED_WORLD}/findings"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    let items = body["findings"].as_array().expect("findings array");
    assert_eq!(items.len(), 3, "read surface lists all findings");
    let read_kinds: Vec<&str> = items
        .iter()
        .map(|f| f["kind"].as_str().expect("kind"))
        .collect();
    for expected in [
        "dramatic_irony_asymmetry",
        "required_field",
        "observer_cardinality",
    ] {
        assert!(
            read_kinds.contains(&expected),
            "read route carries kind {expected}: {read_kinds:?}"
        );
    }

    // 4. GET /v1/daemon/worlds/:world_id/rules → PD-2 fields + first-class
    // constraint, store order, RFC 3339, no extensions bag.
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{OWNED_WORLD}/rules"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["truncated"], false, "body={body}");
    let rules = body["rules"].as_array().expect("rules array");
    assert_eq!(rules.len(), 3);
    let names: Vec<&str> = rules
        .iter()
        .map(|r| r["canonical_name"].as_str().expect("canonical_name"))
        .collect();
    assert_eq!(
        names,
        vec![
            "Backstory required",
            "Characters need summaries",
            "Observer cap"
        ],
        "store order canonical_name ASC (author-metadata list, not newest-first)"
    );
    let item = &rules[1]; // "Characters need summaries"
    assert_eq!(item["rule_id"], "rul_need_summary");
    assert_eq!(
        item["kind"], "rule",
        "Rule.kind verbatim (author classification)"
    );
    assert_eq!(item["status"], "active", "status verbatim");
    assert_eq!(item["severity_hint"], "error", "severity_hint verbatim");
    assert_eq!(
        item["statement"], "Human summary for Characters need summaries",
        "statement is the human summary — carried verbatim, never evaluated"
    );
    assert_eq!(
        item["target_entry_types"],
        json!(["character"]),
        "targeting axis verbatim"
    );
    assert_eq!(
        item["constraint"],
        json!({ "family": "required_field", "field": "body.summary" }),
        "AR-2 carrier projected first-class"
    );
    assert!(
        !item
            .as_object()
            .expect("item object")
            .contains_key("extensions"),
        "the extensions bag itself is NOT exposed (AR-3)"
    );
    assert_rfc3339(item["created_at"].as_str().expect("created_at RFC 3339"));
    assert_rfc3339(item["updated_at"].as_str().expect("updated_at RFC 3339"));
    // The observer-cardinality rule has no severity_hint → omitted on wire.
    assert!(
        rules[2].get("severity_hint").is_none(),
        "absent severity_hint is omitted, not null: {}",
        rules[2]
    );
}

// ─── AC-V166-2: foreign rule_ref → whole-check reject ─────────────────────

/// A `rule_refs` id owned by a different world rejects the WHOLE check with
/// 400 `invalid_input` naming the id — no partial evaluation, no partial
/// persistence (PD-3 fail closed).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn foreign_rule_ref_rejects_whole_check_no_partial_persist() {
    let ctx = ctx().await;
    // A rule row whose owner world differs from the check's world — the
    // `spoke_rules.world_id` has no FK to `narrative_worlds`, so the seeded
    // foreign world id is a valid owner.
    seed_rule(
        &ctx.pool,
        "rul_foreign",
        FOREIGN_WORLD,
        "Foreign rule",
        None,
        "active",
        &[],
        &json!({ "family": "module_presence", "module_key": "x" }),
    )
    .await;

    let body = json!({
        "world_id": OWNED_WORLD,
        "scope": { "scope_id": OWNED_WORLD },
        "rule_refs": ["rul_foreign"],
    });
    let resp = ctx.server.post("/v1/daemon/check").json(&body).await;
    assert_eq!(
        resp.status_code(),
        StatusCode::BAD_REQUEST,
        "foreign rule_ref must reject the whole check: {}",
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["success"], false, "body={body}");
    assert_eq!(body["error"]["code"], "invalid_input", "body={body}");
    assert_eq!(body["error"]["details"]["field"], "check", "body={body}");
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("rul_foreign"),
        "reason must name the foreign rule id: {message}"
    );
    assert!(
        !message.contains("Foreign rule"),
        "foreign canonical_name must not leak: {message}"
    );

    // Whole-check rejection: nothing evaluated, nothing persisted.
    let rows = list_world_findings_by_world(&ctx.pool, OWNED_WORLD, 10)
        .await
        .expect("list world findings");
    assert!(rows.is_empty(), "no partial findings persisted: {rows:?}");
}

// ─── AR-1: embedded rules reject ─────────────────────────────────────────

/// A request carrying embedded `rules` is not an authoring path — reject
/// with 400 `invalid_input` and the locked message (they would bypass world
/// binding via spoke's by-`rule_id` priority over resolved refs).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn embedded_rules_reject_400_invalid_input() {
    let ctx = ctx().await;

    let body = json!({
        "world_id": OWNED_WORLD,
        "scope": { "scope_id": OWNED_WORLD },
        "rules": [{
            "schema_version": 1,
            "rule_id": "rul_embedded",
            "canonical_name": "Embedded",
            "kind": "rule",
            "extensions": {},
        }],
    });
    let resp = ctx.server.post("/v1/daemon/check").json(&body).await;
    assert_eq!(
        resp.status_code(),
        StatusCode::BAD_REQUEST,
        "embedded rules must reject: {}",
        resp.text()
    );
    let body: Value = resp.json();
    assert_eq!(body["error"]["code"], "invalid_input", "body={body}");
    assert_eq!(body["error"]["details"]["field"], "check", "body={body}");
    assert_eq!(
        body["error"]["message"],
        "Invalid input: embedded rules are not an authoring path this iteration; \
         use rule_refs or world auto-include",
        "locked message verbatim (daemon InvalidInput prefixes the reason)"
    );
}

// ─── PD-1 fast path: zero active rules (draft only) ───────────────────────

/// A world whose only rule is `status=draft` auto-includes nothing → check
/// 200 with mental-pair-only findings (no rule-family kinds) — the emergent
/// empty-rules fast path (AR-4), never an error. The read route still lists
/// the draft rule (all statuses visible, PD-1/PD-2).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zero_active_rules_draft_only_200_mental_pair_only() {
    let ctx = ctx().await;
    // A summary-less entry (would violate the draft rule if it evaluated)
    // with no belief rows → mental pair runs, produces nothing.
    seed_kb_entry(
        &ctx.pool,
        DRAFT_WORLD,
        "kb_draft_violator",
        "character",
        "Draft Violator",
        &json!({}),
        &json!({}),
    )
    .await;
    seed_rule(
        &ctx.pool,
        "rul_draft",
        DRAFT_WORLD,
        "Draft summary rule",
        None,
        "draft",
        &["character"],
        &json!({ "family": "required_field", "field": "body.summary" }),
    )
    .await;

    let resp = ctx
        .server
        .post("/v1/daemon/check")
        .json(&check_body(DRAFT_WORLD))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    let findings = body["findings"].as_array().expect("findings array");
    assert!(
        findings.is_empty(),
        "draft rule not auto-included; mental pair finds no candidates: {body}"
    );
    assert!(
        !findings.iter().any(|f| f["kind"] == "required_field"),
        "no rule-family findings: {body}"
    );

    // The read route lists the draft rule (all statuses visible).
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{DRAFT_WORLD}/rules"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    let rules = body["rules"].as_array().expect("rules array");
    assert_eq!(rules.len(), 1);
    assert_eq!(rules[0]["rule_id"], "rul_draft");
    assert_eq!(
        rules[0]["status"], "draft",
        "status verbatim — authors see what auto-include skips"
    );
}

// ─── AR-3 read surface: guards + empty + cap ──────────────────────────────

/// Owned world with zero rules → 200 + `{"rules": [], "truncated": false}`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_zero_rules_returns_200_empty() {
    let ctx = ctx().await;

    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{EMPTY_WORLD}/rules"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["truncated"], false);
    assert_eq!(
        body["rules"].as_array().expect("rules array").len(),
        0,
        "owned world with zero rules → 200 + empty list: {body}"
    );
}

/// Unknown world → 404 (`require_world_owner` parity with `kb/graph` /
/// findings).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_unknown_world_returns_404() {
    let ctx = ctx().await;
    let resp = ctx.server.get("/v1/daemon/worlds/wld_missing/rules").await;
    assert_error_envelope(&resp, StatusCode::NOT_FOUND, "not_found");
}

/// Foreign-owned world → 403 (cross-author; world existence stays
/// unobservable — `require_world_owner` parity).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_foreign_world_returns_403() {
    let ctx = ctx().await;
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{FOREIGN_WORLD}/rules"))
        .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}

/// The read route bounds the store SQL-side (Bugbot 4bad2fca): with
/// `CAP + 2` (502) stored rules the response carries exactly the first 500
/// of `canonical_name ASC, rule_id ASC` and flags `truncated: true`; with
/// fewer than the cap it returns all rows and flags `truncated: false`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_route_caps_and_flags_truncation() {
    let ctx = ctx().await;

    // 502 stored rows (CAP 500 + 2 overflow) — the Bugbot scenario.
    for i in 0..502 {
        seed_rule(
            &ctx.pool,
            &format!("rul_bulk_{i:03}"),
            EMPTY_WORLD,
            &format!("Bulk rule {i:03}"),
            None,
            "active",
            &[],
            &json!({ "family": "module_presence", "module_key": "x" }),
        )
        .await;
    }
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{EMPTY_WORLD}/rules"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["truncated"], true, "overflow must be flagged: {body}");
    let items = body["rules"].as_array().expect("rules array");
    assert_eq!(
        items.len(),
        500,
        "exactly the first 500 of store order: {body}"
    );

    // Fewer than the cap → all rows, honest `truncated: false`.
    for i in 0..3 {
        seed_rule(
            &ctx.pool,
            &format!("rul_few_{i}"),
            DRAFT_WORLD,
            &format!("Few rule {i}"),
            None,
            "active",
            &[],
            &json!({ "family": "module_presence", "module_key": "x" }),
        )
        .await;
    }
    let resp = ctx
        .server
        .get(&format!("/v1/daemon/worlds/{DRAFT_WORLD}/rules"))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let body: Value = resp.json();
    assert_eq!(body["truncated"], false, "fewer than cap: {body}");
    assert_eq!(
        body["rules"].as_array().expect("rules array").len(),
        3,
        "all stored rows returned when under the cap: {body}"
    );
}
