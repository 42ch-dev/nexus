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
use nexus_local_db::spoke_rules::{get_spoke_rules_by_ids, insert_rule, SpokeRuleRow};
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
/// the draft rule (all statuses visible, PD-1/PD-2). The mental pair MUST
/// still run: a seeded false-belief candidate produces its finding beside
/// zero rule-family kinds (qc1-S4).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn zero_active_rules_draft_only_200_mental_pair_only() {
    let ctx = ctx().await;
    // Minimal mental fixture (one false-belief candidate, box/basket shape):
    // kb_bo holds a private `truth: False` belief; the informing event is
    // observed by kb_ana only → dramatic-irony finding on kb_bo. The belief
    // row and event follow the AC-V166-4 worked example verbatim.
    seed_kb_entry(
        &ctx.pool,
        DRAFT_WORLD,
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
    seed_event(
        &ctx.pool,
        DRAFT_WORLD,
        "evt_wld_draft_world_transfer",
        "Marble transfer",
        1,
        &json!({ "observation": { "observers": ["kb_ana"] } }),
    )
    .await;
    // A summary-less entry (would violate the draft rule if it evaluated).
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
        !findings.is_empty(),
        "mental pair fires on the false-belief candidate: {body}"
    );
    assert!(
        findings.iter().any(|f| {
            f["kind"] == "dramatic_irony_asymmetry" && f["target_entry_id"] == "kb_bo"
        }),
        "draft-only fast path keeps the mental pair running: {body}"
    );
    assert!(
        !findings.iter().any(|f| f["kind"] == "required_field"),
        "no rule-family findings — draft rule not auto-included: {body}"
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
/// The boundary itself is asserted: the returned 500 are the first 500 of
/// store order, decided by `canonical_name ASC, rule_id ASC` (S-004).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn read_route_caps_and_flags_truncation() {
    let ctx = ctx().await;

    // 502 stored rows (CAP 500 + 2 overflow) — the Bugbot scenario. Names
    // seed in store order; the three rows sharing the 500th name exercise
    // the `rule_id ASC` tie-break exactly at the truncation boundary.
    for i in 0..499 {
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
    // "Bulk rule 499" × 3 — the tie-break decides which survive the cap:
    // rul_aaa_499 < rul_bulk_499 < rul_zzz_499, so exactly rul_aaa_499 is
    // the 500th (last) returned item and the two later ids fall outside.
    for rule_id in ["rul_aaa_499", "rul_bulk_499", "rul_zzz_499"] {
        seed_rule(
            &ctx.pool,
            rule_id,
            EMPTY_WORLD,
            "Bulk rule 499",
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
    // The returned 500 ARE the first 500 of `canonical_name ASC, rule_id ASC`.
    assert_eq!(items[0]["canonical_name"], "Bulk rule 000", "first by name");
    assert_eq!(items[0]["rule_id"], "rul_bulk_000", "first by name+id");
    assert_eq!(
        items[499]["canonical_name"], "Bulk rule 499",
        "boundary name present"
    );
    assert_eq!(
        items[499]["rule_id"], "rul_aaa_499",
        "boundary decided by the rule_id ASC tie-break — the two later ids \
         of the shared name are truncated (S-004)"
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

// ═══════════════════════════════════════════════════════════════════════════
// V1.169 P1 (AR-2/AR-3/AR-5/AR-6) — the write surface: POST create + PATCH
// edit. AC-V169-3.
// ═══════════════════════════════════════════════════════════════════════════

fn create_url(world_id: &str) -> String {
    format!("/v1/daemon/worlds/{world_id}/rules")
}

fn patch_url(world_id: &str, rule_id: &str) -> String {
    format!("/v1/daemon/worlds/{world_id}/rules/{rule_id}")
}

/// Assert the closed `InvalidInput` envelope: 400 + `invalid_input` +
/// `details.field == field` (locks AR-2).
fn assert_invalid_input_field(resp: &axum_test::TestResponse, field: &str) {
    assert_error_envelope(resp, StatusCode::BAD_REQUEST, "invalid_input");
    let body: Value = resp.json();
    assert_eq!(
        body["error"]["details"]["field"], field,
        "details.field mismatch: body={body}"
    );
}

/// Framework extractor rejection (locks AR-2 not-envelope cases): 4xx with
/// a framework body — assert the 4xx only, never the daemon envelope.
fn assert_extractor_rejection(resp: &axum_test::TestResponse) {
    assert!(
        resp.status_code().is_client_error(),
        "expected 4xx client error, got {}: {}",
        resp.status_code(),
        resp.text()
    );
    assert!(
        !resp.text().contains("\"success\""),
        "extractor rejection must not use the daemon envelope: {}",
        resp.text()
    );
}

/// Fetch one stored row by id (panics when absent — test-only helper).
async fn fetch_rule(pool: &sqlx::SqlitePool, rule_id: &str) -> SpokeRuleRow {
    get_spoke_rules_by_ids(pool, &[rule_id.to_string()])
        .await
        .expect("fetch rule")
        .into_iter()
        .next()
        .expect("rule row exists")
}

/// Server-side mint contract (V1.166 AR-2): `rul_` ++ uuid v4 simple
/// (32 lowercase hex digits, no hyphens).
fn assert_minted_rule_id(id: &str) {
    assert!(id.starts_with("rul_"), "minted id prefix: {id}");
    let hex = &id[4..];
    assert_eq!(hex.len(), 32, "uuid v4 simple length: {id}");
    assert!(
        hex.chars().all(|c| c.is_ascii_hexdigit()),
        "uuid v4 simple is hex: {id}"
    );
}

// ─── Create: happy paths (one per family) ─────────────────────────────────

/// POST create succeeds for all four carrier families → 201 + the minted
/// item (echoed carrier, defaults, RFC 3339 timestamps).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_happy_path_all_four_families_201() {
    let ctx = ctx().await;
    let cases = [
        (
            "module_presence rule",
            json!({ "family": "module_presence", "module_key": "characters" }),
        ),
        (
            "module_absence rule",
            json!({ "family": "module_absence", "module_key": "lore" }),
        ),
        (
            "required field rule",
            json!({ "family": "required_field", "field": "body.summary" }),
        ),
        (
            "observer cardinality rule",
            json!({ "family": "observer_cardinality", "min": 1, "max": 3 }),
        ),
    ];
    for (name, carrier) in cases {
        let body = json!({
            "canonical_name": name,
            "statement": format!("Human summary for {name}"),
            "constraint": carrier,
        });
        let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
        assert_eq!(
            resp.status_code(),
            StatusCode::CREATED,
            "body={}",
            resp.text()
        );
        let item: Value = resp.json();
        assert_eq!(item["canonical_name"], name, "body={item}");
        assert_eq!(item["statement"], format!("Human summary for {name}"));
        assert_eq!(item["kind"], "rule", "default kind: {item}");
        assert_eq!(item["status"], "active", "default status: {item}");
        assert_eq!(
            item["severity_hint"],
            Value::Null,
            "default severity: {item}"
        );
        assert_eq!(
            item["target_entry_types"],
            json!([]),
            "default targets: {item}"
        );
        assert_eq!(
            item["constraint"], carrier,
            "carrier echoed verbatim: {item}"
        );
        let id = item["rule_id"].as_str().expect("rule_id").to_string();
        assert_minted_rule_id(&id);
        assert_rfc3339(item["created_at"].as_str().expect("created_at"));
        assert_rfc3339(item["updated_at"].as_str().expect("updated_at"));
    }
}

/// Create defaults are observable in the 201 item AND in storage (AR-3):
/// `status=active`, `kind=rule`, `severity_hint=NULL`,
/// `target_entry_types=[]`, `description/source_anchor_json=NULL`,
/// `extensions_json={"nexus":{"constraint":<carrier>}}`,
/// `created_at=updated_at=now`, `schema_version=1`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_defaults_observable_in_201_item_and_storage() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "Defaulted",
        "statement": "Statement",
        "constraint": { "family": "required_field", "field": "body.tags" },
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_eq!(
        resp.status_code(),
        StatusCode::CREATED,
        "body={}",
        resp.text()
    );
    let item: Value = resp.json();
    assert_eq!(item["description"], Value::Null);
    let id = item["rule_id"].as_str().expect("rule_id").to_string();

    // Storage reality (honest timestamps, opaque JSON columns).
    let row = fetch_rule(&ctx.pool, &id).await;
    assert_eq!(row.schema_version, 1);
    assert_eq!(row.canonical_name, "Defaulted");
    assert_eq!(row.kind, "rule");
    assert_eq!(row.statement.as_deref(), Some("Statement"));
    assert_eq!(row.description, None);
    assert_eq!(row.severity_hint, None);
    assert_eq!(row.status.as_deref(), Some("active"));
    assert_eq!(row.source_anchor_json, None);
    assert_eq!(row.created_at, row.updated_at, "create stamps both to now");
    assert!(row.created_at.is_some(), "create stamps epoch seconds");
    let targets: Vec<String> =
        serde_json::from_str(&row.target_entry_types_json).expect("targets json");
    assert!(targets.is_empty(), "default target axis is []");
    let bag: Value = serde_json::from_str(&row.extensions_json).expect("extensions json");
    assert_eq!(
        bag,
        json!({ "nexus": { "constraint": { "family": "required_field", "field": "body.tags" } } }),
        "namespace written fresh at create (CLI row-assembly parity)"
    );
}

// ─── Create: carrier grammar via the member-aware seam (AR-2) ─────────────

/// A carrier with an unknown extra member → envelope field `constraint`
/// (the closed-shape reject reports the carrier-level member; the reason
/// names the exact key). A non-object carrier is intercepted by the DTO
/// extractor (the schema requires an object) — see
/// `create_extractor_rejections_are_framework_not_envelope`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_unknown_carrier_member_field_constraint() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "module_presence", "module_key": "m", "bogus": true },
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "constraint");
    let body: Value = resp.json();
    assert!(
        body["error"]["details"]["reason"]
            .as_str()
            .expect("reason")
            .contains("bogus"),
        "reason names the exact unknown key: {body}"
    );
}

/// Unknown family → `constraint.family` (closed four-family set).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_unknown_family_field_constraint_family() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "tone", "module_key": "m" },
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "constraint.family");
}

/// `module_presence` with a missing operand → `constraint.module_key`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_missing_module_key_field_constraint_module_key() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "module_presence" },
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "constraint.module_key");
}

/// `module_absence` with an empty operand → `constraint.module_key`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_empty_module_key_field_constraint_module_key() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "module_absence", "module_key": "" },
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "constraint.module_key");
}

/// `required_field` with an entry-level value outside the closed set →
/// `constraint.field`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_unknown_entry_field_constraint_field() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "required_field", "field": "body.title" },
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "constraint.field");
}

/// `observer_cardinality` with `min > max` → `constraint.min`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_min_gt_max_constraint_min_field() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "observer_cardinality", "min": 5, "max": 3 },
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "constraint.min");
}

// ─── Create: meta-field value checks + the pair rule (AR-2/AR-5) ──────────

/// `observer_cardinality` × non-empty target axis → `target_entry_types`
/// (effective pair — both fresh at create).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_observer_cardinality_with_target_types() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "observer_cardinality", "max": 3 },
        "target_entry_types": ["body_summary"],
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "target_entry_types");
}

/// Empty target member (`[""]`) → `target_entry_types`; `[]` itself is
/// meaningful (all types in check scope, AR-3) and accepted.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_empty_target_member() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "module_presence", "module_key": "m" },
        "target_entry_types": [""],
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "target_entry_types");
}

/// `canonical_name` / `statement` empty after trim → their envelope fields.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_empty_name_or_statement_after_trim() {
    let ctx = ctx().await;
    let base = json!({
        "statement": "Y",
        "constraint": { "family": "module_presence", "module_key": "m" },
    });
    let mut body = base.clone();
    body["canonical_name"] = json!("   ");
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "canonical_name");

    let mut body = base.clone();
    body["canonical_name"] = json!("X");
    body["statement"] = json!("");
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "statement");
}

/// `status` outside draft/active/deprecated → `status` (the write path does
/// not invent statuses, AR-2).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_non_core_status() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "module_presence", "module_key": "m" },
        "status": "banana",
    });
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "status");
}

/// `severity_hint` / `kind` present but empty → their envelope fields.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_rejects_empty_severity_hint_or_kind() {
    let ctx = ctx().await;
    let base = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "module_presence", "module_key": "m" },
    });
    let mut body = base.clone();
    body["severity_hint"] = json!("  ");
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "severity_hint");

    let mut body = base.clone();
    body["kind"] = json!("");
    let resp = ctx.server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_invalid_input_field(&resp, "kind");
}

/// Malformed JSON, unknown top-level member, missing required member, and
/// a non-string target member are axum `Json` extractor rejections — 4xx
/// with a framework body, never the daemon envelope (AR-2 not-envelope
/// cases; the P2 form cannot produce them).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn create_extractor_rejections_are_framework_not_envelope() {
    let ctx = ctx().await;

    let malformed = ctx
        .server
        .post(&create_url(OWNED_WORLD))
        .content_type("application/json")
        .text("{oops")
        .await;
    assert_extractor_rejection(&malformed);

    let unknown_member = ctx
        .server
        .post(&create_url(OWNED_WORLD))
        .json(&json!({
            "canonical_name": "X",
            "statement": "Y",
            "constraint": { "family": "module_presence", "module_key": "m" },
            "bogus": 1,
        }))
        .await;
    assert_extractor_rejection(&unknown_member);

    let missing_required = ctx
        .server
        .post(&create_url(OWNED_WORLD))
        .json(&json!({ "canonical_name": "X" }))
        .await;
    assert_extractor_rejection(&missing_required);

    let non_string_target = ctx
        .server
        .post(&create_url(OWNED_WORLD))
        .json(&json!({
            "canonical_name": "X",
            "statement": "Y",
            "constraint": { "family": "module_presence", "module_key": "m" },
            "target_entry_types": ["ok", 5],
        }))
        .await;
    assert_extractor_rejection(&non_string_target);

    // A non-object carrier fails the DTO's type structure (schema requires
    // an object) before the seam can see it — extractor rejection, not
    // envelope (AR-1 type structure).
    let non_object_carrier = ctx
        .server
        .post(&create_url(OWNED_WORLD))
        .json(&json!({
            "canonical_name": "X",
            "statement": "Y",
            "constraint": "not an object",
        }))
        .await;
    assert_extractor_rejection(&non_object_carrier);
}

// ─── PATCH: happy paths + AR-3 semantics ──────────────────────────────────

/// PATCH replaces every mutable field incl. `canonical_name`; `description`
/// stays untouched (product field set); `updated_at` advances, `created_at`
/// never changes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_updates_every_mutable_field_incl_canonical_name() {
    let ctx = ctx().await;
    seed_rule(
        &ctx.pool,
        "rul_patch_all",
        OWNED_WORLD,
        "Before",
        Some("info"),
        "active",
        &["old_type"],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;

    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_patch_all"))
        .json(&json!({
            "canonical_name": "After",
            "statement": "New statement",
            "severity_hint": "error",
            "status": "draft",
            "kind": "prohibition",
            "target_entry_types": ["new_type"],
            "constraint": { "family": "module_absence", "module_key": "lore" },
        }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let item: Value = resp.json();
    assert_eq!(item["canonical_name"], "After");
    assert_eq!(item["statement"], "New statement");
    assert_eq!(item["severity_hint"], "error");
    assert_eq!(item["status"], "draft");
    assert_eq!(item["kind"], "prohibition");
    assert_eq!(item["target_entry_types"], json!(["new_type"]));
    assert_eq!(
        item["constraint"],
        json!({ "family": "module_absence", "module_key": "lore" })
    );

    let row = fetch_rule(&ctx.pool, "rul_patch_all").await;
    assert_eq!(row.canonical_name, "After");
    assert_eq!(row.statement.as_deref(), Some("New statement"));
    assert_eq!(row.severity_hint.as_deref(), Some("error"));
    assert_eq!(row.status.as_deref(), Some("draft"));
    assert_eq!(row.kind, "prohibition");
    let targets: Vec<String> =
        serde_json::from_str(&row.target_entry_types_json).expect("targets json");
    assert_eq!(targets, ["new_type"]);
    assert_eq!(
        row.description, None,
        "description is not mutable (product field set)"
    );
    assert_eq!(
        row.created_at,
        Some(1_700_000_000),
        "created_at never touched"
    );
    assert!(
        row.updated_at.unwrap() > 1_700_000_100,
        "updated_at refreshes to now"
    );
}

/// Whole-carrier replacement (AR-3): only `extensions.nexus.constraint` is
/// overwritten — other nexus keys and all other namespaces survive; the
/// response item carries the new carrier.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_whole_carrier_replacement_preserves_extensions_bag() {
    let ctx = ctx().await;
    let row = SpokeRuleRow {
        rule_id: "rul_bag".to_string(),
        world_id: OWNED_WORLD.to_string(),
        schema_version: 1,
        canonical_name: "Bag".to_string(),
        kind: "rule".to_string(),
        statement: Some("S".to_string()),
        description: None,
        target_entry_types_json: "[]".to_string(),
        severity_hint: None,
        status: Some("active".to_string()),
        source_anchor_json: None,
        extensions_json: json!({
            "nexus": {
                "constraint": { "family": "module_presence", "module_key": "old" },
                "other_nexus_key": "keep-me",
            },
            "other_namespace": { "k": [1, 2] },
        })
        .to_string(),
        created_at: Some(1_700_000_000),
        updated_at: Some(1_700_000_100),
    };
    insert_rule(&ctx.pool, &row).await.expect("seed");

    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_bag"))
        .json(&json!({ "constraint": { "family": "required_field", "field": "body.tags" } }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let item: Value = resp.json();
    assert_eq!(
        item["constraint"],
        json!({ "family": "required_field", "field": "body.tags" })
    );

    let row = fetch_rule(&ctx.pool, "rul_bag").await;
    let bag: Value = serde_json::from_str(&row.extensions_json).expect("extensions json");
    assert_eq!(
        bag["nexus"]["constraint"],
        json!({ "family": "required_field", "field": "body.tags" }),
        "carrier replaced"
    );
    assert_eq!(
        bag["nexus"]["other_nexus_key"], "keep-me",
        "nexus siblings survive"
    );
    assert_eq!(
        bag["other_namespace"],
        json!({ "k": [1, 2] }),
        "other namespaces survive"
    );
}

/// Deactivate = `PATCH status=deprecated` (product lock — no DELETE route).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_status_deprecated_deactivates() {
    let ctx = ctx().await;
    seed_rule(
        &ctx.pool,
        "rul_dep",
        OWNED_WORLD,
        "Dep",
        None,
        "active",
        &[],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;

    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_dep"))
        .json(&json!({ "status": "deprecated" }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let item: Value = resp.json();
    assert_eq!(item["status"], "deprecated");
    let row = fetch_rule(&ctx.pool, "rul_dep").await;
    assert_eq!(row.status.as_deref(), Some("deprecated"));
}

/// `target_entry_types: []` is an explicit clear (AR-3); an absent
/// `target_entry_types` leaves the stored axis unchanged.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_empty_target_entry_types_clears_axis_absent_leaves_unchanged() {
    let ctx = ctx().await;
    seed_rule(
        &ctx.pool,
        "rul_clear",
        OWNED_WORLD,
        "Clear",
        None,
        "active",
        &["x"],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;

    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_clear"))
        .json(&json!({ "target_entry_types": [] }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let item: Value = resp.json();
    assert_eq!(
        item["target_entry_types"],
        json!([]),
        "axis cleared: {item}"
    );
    let row = fetch_rule(&ctx.pool, "rul_clear").await;
    assert_eq!(
        row.target_entry_types_json, "[]",
        "stored as the empty array"
    );

    seed_rule(
        &ctx.pool,
        "rul_keep",
        OWNED_WORLD,
        "Keep",
        None,
        "active",
        &["y"],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;
    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_keep"))
        .json(&json!({ "statement": "s2" }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let item: Value = resp.json();
    assert_eq!(
        item["target_entry_types"],
        json!(["y"]),
        "absent ≠ clear: {item}"
    );
}

// ─── PATCH: validation order + 404 semantics (AR-5/AR-6) ──────────────────

/// Empty PATCH (no mutable field) → 400 `patch` with the locked reason, and
/// no write (`updated_at` untouched — fail-early beats a no-op refresh).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_empty_body_rejects_field_patch_no_write() {
    let ctx = ctx().await;
    seed_rule(
        &ctx.pool,
        "rul_empty_patch",
        OWNED_WORLD,
        "EmptyPatch",
        None,
        "active",
        &[],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;

    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_empty_patch"))
        .json(&json!({}))
        .await;
    assert_invalid_input_field(&resp, "patch");
    let body: Value = resp.json();
    assert_eq!(
        body["error"]["message"],
        "Invalid input: at least one of canonical_name | statement | severity_hint | \
         status | kind | target_entry_types | constraint is required",
        "locked reason verbatim (AR-3)"
    );
    let row = fetch_rule(&ctx.pool, "rul_empty_patch").await;
    assert_eq!(
        row.updated_at,
        Some(1_700_000_100),
        "empty PATCH must not refresh updated_at"
    );
    assert_eq!(row.canonical_name, "EmptyPatch");
}

/// Unknown `rule_id` → 404 `not_found` naming only the id (AR-6).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_unknown_rule_404() {
    let ctx = ctx().await;
    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_does_not_exist"))
        .json(&json!({ "statement": "x" }))
        .await;
    assert_error_envelope(&resp, StatusCode::NOT_FOUND, "not_found");
    let body: Value = resp.json();
    let message = body["error"]["message"].as_str().expect("message");
    assert!(
        message.contains("rul_does_not_exist"),
        "404 names only the id: {message}"
    );
}

/// A `rule_id` owned by a different world of the same creator → 404 with no
/// existence leak: the error names only the id — never the `canonical_name`,
/// never the owning world (AR-6); the rule stays intact in its own world.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_cross_world_rule_404_no_existence_leak() {
    let ctx = ctx().await;
    seed_rule(
        &ctx.pool,
        "rul_other_world",
        DRAFT_WORLD,
        "SecretOtherWorldRule",
        None,
        "active",
        &[],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;

    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_other_world"))
        .json(&json!({ "statement": "x" }))
        .await;
    assert_error_envelope(&resp, StatusCode::NOT_FOUND, "not_found");
    let message = resp.json::<Value>()["error"]["message"]
        .as_str()
        .expect("message")
        .to_string();
    assert!(
        message.contains("rul_other_world"),
        "404 names the id: {message}"
    );
    assert!(
        !message.contains("SecretOtherWorldRule"),
        "canonical_name must not leak: {message}"
    );
    assert!(
        !message.contains(DRAFT_WORLD),
        "owning world must not leak: {message}"
    );

    // No write side effects: the rule still lists in its own world.
    let read = ctx.server.get(&create_url(DRAFT_WORLD)).await;
    let list: Value = read.json();
    let ids: Vec<&str> = list["rules"]
        .as_array()
        .expect("rules")
        .iter()
        .map(|r| r["rule_id"].as_str().expect("rule_id"))
        .collect();
    assert!(ids.contains(&"rul_other_world"), "rule untouched: {ids:?}");
}

/// The pair rule judges the EFFECTIVE pair (AR-5): provided or stored on
/// each side. Changing one side into a conflict rejects on
/// `target_entry_types`; an unrelated statement PATCH on a stored observer
/// rule with an empty axis stays fine.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_effective_pair_conflict_rejects_target_entry_types() {
    let ctx = ctx().await;
    seed_rule(
        &ctx.pool,
        "rul_obs",
        OWNED_WORLD,
        "Obs",
        None,
        "active",
        &[],
        &json!({ "family": "observer_cardinality", "max": 3 }),
    )
    .await;
    seed_rule(
        &ctx.pool,
        "rul_targeted",
        OWNED_WORLD,
        "Targeted",
        None,
        "active",
        &["x"],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;

    // (a) stored observer family + provided non-empty targets → reject.
    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_obs"))
        .json(&json!({ "target_entry_types": ["x"] }))
        .await;
    assert_invalid_input_field(&resp, "target_entry_types");

    // (b) provided observer family + stored non-empty targets → reject.
    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_targeted"))
        .json(&json!({ "constraint": { "family": "observer_cardinality", "min": 1 } }))
        .await;
    assert_invalid_input_field(&resp, "target_entry_types");

    // (c) stored observer + empty effective target set: unrelated PATCH OK.
    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_obs"))
        .json(&json!({ "statement": "just a statement" }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
}

/// PATCH meta-field value checks mirror create (AR-2) and leave the row
/// untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_meta_field_value_checks() {
    let ctx = ctx().await;
    seed_rule(
        &ctx.pool,
        "rul_meta",
        OWNED_WORLD,
        "Meta",
        Some("info"),
        "active",
        &["x"],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;

    let cases: Vec<(&str, Value, &str)> = vec![
        ("status", json!("banana"), "status"),
        ("canonical_name", json!("   "), "canonical_name"),
        ("statement", json!(""), "statement"),
        ("severity_hint", json!(" "), "severity_hint"),
        ("kind", json!(""), "kind"),
        (
            "constraint",
            json!({ "family": "tone" }),
            "constraint.family",
        ),
        ("target_entry_types", json!([""]), "target_entry_types"),
    ];
    for (member, value, field) in cases {
        let mut body = serde_json::Map::new();
        body.insert(member.to_string(), value);
        let resp = ctx
            .server
            .patch(&patch_url(OWNED_WORLD, "rul_meta"))
            .json(&serde_json::Value::Object(body))
            .await;
        assert_invalid_input_field(&resp, field);
    }

    // Every rejected PATCH returned before any write — the row is untouched.
    let row = fetch_rule(&ctx.pool, "rul_meta").await;
    assert_eq!(row.canonical_name, "Meta");
    assert_eq!(row.status.as_deref(), Some("active"));
    assert_eq!(row.updated_at, Some(1_700_000_100));
}

// ─── PATCH: guards + honest timestamps ────────────────────────────────────

/// No active creator → the tier-2 `require_active_creator` middleware
/// rejects BOTH write routes with 409 `uninitialized` (Profile not
/// selected) — exactly like the read route (same guard chain). The
/// handler-level 401 `auth_required` guard stays as defense in depth.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_routes_reject_without_creator() {
    let tmp = tempfile::TempDir::new().unwrap();
    let nexus_home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    let db_path = nexus_home.join("state.db");
    let pool = nexus_local_db::open_pool(&db_path).await.unwrap();
    nexus_local_db::run_migrations(&pool).await.unwrap();
    nexus_local_db::seed_versions(&pool).await.unwrap();
    let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
    let app = api::create_router(state, DaemonApiConfig::keyless());
    let server = TestServer::new(app).expect("test server");

    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "module_presence", "module_key": "m" },
    });
    let resp = server.post(&create_url(OWNED_WORLD)).json(&body).await;
    assert_error_envelope(&resp, StatusCode::CONFLICT, "uninitialized");

    let resp = server
        .patch(&patch_url(OWNED_WORLD, "rul_x"))
        .json(&json!({ "statement": "y" }))
        .await;
    assert_error_envelope(&resp, StatusCode::CONFLICT, "uninitialized");

    drop(tmp); // explicit: temp root kept alive until here
}

/// Foreign-owned world → 403 `forbidden` on both write routes (world
/// ownership boundary; 404 is reserved for rules, 403 for worlds, AR-6).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn write_routes_guards_403_foreign_world() {
    let ctx = ctx().await;
    let body = json!({
        "canonical_name": "X",
        "statement": "Y",
        "constraint": { "family": "module_presence", "module_key": "m" },
    });
    let resp = ctx
        .server
        .post(&create_url(FOREIGN_WORLD))
        .json(&body)
        .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");

    let resp = ctx
        .server
        .patch(&patch_url(FOREIGN_WORLD, "rul_any"))
        .json(&json!({ "statement": "y" }))
        .await;
    assert_error_envelope(&resp, StatusCode::FORBIDDEN, "forbidden");
}

/// A matched PATCH refreshes `updated_at` and never touches `created_at`
/// (AR-4 honesty, no OCC).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn patch_updates_updated_at_keeps_created_at() {
    let ctx = ctx().await;
    seed_rule(
        &ctx.pool,
        "rul_ts",
        OWNED_WORLD,
        "Ts",
        None,
        "active",
        &[],
        &json!({ "family": "module_presence", "module_key": "m" }),
    )
    .await;

    let resp = ctx
        .server
        .patch(&patch_url(OWNED_WORLD, "rul_ts"))
        .json(&json!({ "statement": "updated" }))
        .await;
    assert_eq!(resp.status_code(), StatusCode::OK, "body={}", resp.text());
    let row = fetch_rule(&ctx.pool, "rul_ts").await;
    assert_eq!(
        row.created_at,
        Some(1_700_000_000),
        "created_at never touched"
    );
    assert!(
        row.updated_at.unwrap() > 1_700_000_100,
        "updated_at refreshes to current epoch seconds"
    );
}
