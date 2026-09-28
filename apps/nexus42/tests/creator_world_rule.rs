//! Server-free tests for the `creator world rule add|list|deactivate|archive`
//! CLI (V1.166 PD-1 / AR-2 / AR-3, DR-64; direct-core retarget v1.193 P0-T3).
//!
//! Plan: `.mstar/plans/2026-08-15-v1.166-p1-rules-driven-check-evaluator.md`
//! Spec: `.mstar/iterations/v1.166/specs/v1.166-quality-locks.md` §PD-1 / §AR-2 / §AR-3
//!
//! Drives the leaf functions (`rule_add` / `rule_list` / `rule_deactivate` / `rule_archive`)
//! against a hermetic direct-core home — no `$HOME`, no daemon, no Node child
//! (`common/direct.rs` precedent). Storage truth is read through the same core
//! projection the CLI renders (`list_world_rules`), so the assertions describe
//! what a consumer observes rather than the row's internal JSON columns (the
//! row assembly is core-owned).
//!
//! The World-rule seam is `CoreService::{create_world_rule, list_world_rules,
//! update_world_rule}`: the core owns World ownership, the closed carrier
//! grammar, the `rul_` id mint and the AR-1 status set.

#![allow(clippy::unwrap_used)]

#[path = "common/direct.rs"]
mod direct;

use assert_cmd::Command;
use direct::DirectFixture;
use nexus42::commands::creator::world::rule::{rule_add, rule_archive, rule_deactivate, rule_list};
use nexus_contracts::worlds::world_rules_list_response::WorldRulesListResponseRulesItem;
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService, Principal};
use nexus_home_layout::{nexus_root_from_home, workspace_state_db_path};
use nexus_local_db::writer_protocol::release_retained_writer_guards;

/// World owned by the fixture's active creator.
const WORLD: &str = "wld_rule_test";
/// Second World of the SAME creator — the cross-World rule-id filter (AR-6).
const OTHER_OWNED_WORLD: &str = "wld_rule_other_owned";
/// World owned by another creator — the World-ownership guard (AR-3).
const FOREIGN_WORLD: &str = "wld_rule_foreign";

/// A hermetic direct-core home: one active creator/workspace, the owned and
/// foreign Worlds seeded, and the seed writer released before the direct core
/// is opened.
struct RuleEnv {
    fixture: DirectFixture,
    core: CoreService,
    principal: Principal,
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under `~/.nexus42/creators/`.
fn fixture_creator_id(fixture: &DirectFixture) -> String {
    let creators_root = nexus_root_from_home(fixture.home.path()).join("creators");
    let mut entries: Vec<_> = std::fs::read_dir(&creators_root)
        .expect("read fixture creators root")
        .map(|entry| entry.expect("creator dir entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "fixture registers exactly one creator");
    entries
        .pop()
        .expect("one creator")
        .to_string_lossy()
        .into_owned()
}

/// Seed the owned / second-owned / foreign World rows, release the seed
/// writer, then open the direct-writer core the leaves run on.
async fn fresh_env() -> RuleEnv {
    let fixture = DirectFixture::new().await;
    let creator_id = fixture_creator_id(&fixture);
    let db_path = workspace_state_db_path(fixture.home.path(), &creator_id, "default");

    let pool = nexus_local_db::init_engine_pool(&db_path)
        .await
        .unwrap()
        .clone_pool();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        &creator_id,
        "Rule Test World",
        "rule-test-world",
        "private",
        "manual",
    )
    .await;
    nexus_local_db::kb_store::seed::world(
        &pool,
        OTHER_OWNED_WORLD,
        &creator_id,
        "Other Rule World",
        "other-rule-world",
        "private",
        "manual",
    )
    .await;
    nexus_local_db::kb_store::seed::world(
        &pool,
        FOREIGN_WORLD,
        "ctr_other",
        "Foreign Rule World",
        "foreign-rule-world",
        "private",
        "manual",
    )
    .await;
    pool.close().await;
    release_retained_writer_guards(&db_path);

    let core = CoreService::open(CoreOpenOptions {
        user_home: fixture.home.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("direct core opens on the isolated home");
    let principal = core.active_principal().await.expect("active principal");
    RuleEnv {
        fixture,
        core,
        principal,
    }
}

/// The stored rules of an owned World, through the core projection the CLI
/// renders. The default read (archived omitted) is what the CLI renders.
async fn stored_rules(env: &RuleEnv, world_id: &str) -> Vec<WorldRulesListResponseRulesItem> {
    stored_rules_including(env, world_id, false).await
}

/// The same projection with the caller-selected inclusion (V1.198 §13): the
/// default read omits archived rows, `include_archived = true` reveals them.
async fn stored_rules_including(
    env: &RuleEnv,
    world_id: &str,
    include_archived: bool,
) -> Vec<WorldRulesListResponseRulesItem> {
    env.core
        .list_world_rules(&env.principal, world_id.to_string(), include_archived)
        .await
        .expect("list rules of an owned World")
        .rules
}

/// The canonical valid carriers used across the round-trip tests.
const MODULE_PRESENCE_CARRIER: &str = r#"{"family":"module_presence","module_key":"characters"}"#;
const OBSERVER_CARDINALITY_CARRIER: &str = r#"{"family":"observer_cardinality","min":0,"max":3}"#;

// =============================================================================
// CLI surface (assert_cmd)
// =============================================================================

/// The real binary renders the same core projection: `creator world rule list
/// --json` against the hermetic home (no server, no Node child). The in-process
/// seed core is closed first so the child admits its own direct writer.
#[tokio::test]
async fn cli_rule_list_json_renders_core_projection() {
    let env = fresh_env().await;
    let rule_id = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "CLI wired rule",
        "rule",
        "statement",
        "warning",
        &["character".to_string()],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;
    env.core.close().await.expect("seed core closes");

    let out = env
        .fixture
        .command()
        .args([
            "creator",
            "world",
            "rule",
            "list",
            "--world-id",
            WORLD,
            "--json",
        ])
        .output()
        .expect("spawn nexus42 rule list");
    assert!(
        out.status.success(),
        "rule list failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let json: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("json rule list");
    let items = json.as_array().expect("rules array");
    assert_eq!(items.len(), 1, "{json}");
    assert_eq!(items[0]["rule_id"], rule_id);
    assert_eq!(items[0]["status"], "active");
    assert_eq!(items[0]["canonical_name"], "CLI wired rule");
    assert_eq!(
        items[0]["constraint"],
        serde_json::json!({"family": "module_presence", "module_key": "characters"})
    );
}

/// `creator world rule --help` lists the four subcommands.
#[test]
fn world_rule_help_lists_subcommands() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "world", "rule", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help_text = String::from_utf8(output).unwrap();
    for subcmd in &["add", "list", "deactivate", "archive"] {
        assert!(
            help_text.contains(subcmd),
            "creator world rule --help must list '{subcmd}' subcommand: {help_text}"
        );
    }
}

/// `creator world rule list --help` documents the inclusion switch.
#[test]
fn world_rule_list_help_shows_include_archived() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "world", "rule", "list", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help_text = String::from_utf8(output).unwrap();
    for flag in ["--world-id", "--include-archived", "--json"] {
        assert!(
            help_text.contains(flag),
            "rule list --help must document {flag}: {help_text}"
        );
    }
}

/// `creator world rule add --help` documents the PD-1 flag surface.
#[test]
fn world_rule_add_help_shows_flags() {
    let output = Command::cargo_bin("nexus42")
        .unwrap()
        .args(["creator", "world", "rule", "add", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help_text = String::from_utf8(output).unwrap();
    for flag in [
        "--world-id",
        "--name",
        "--statement",
        "--constraint",
        "--kind",
        "--severity",
        "--entry-type",
        "--status",
    ] {
        assert!(
            help_text.contains(flag),
            "rule add --help must document {flag}: {help_text}"
        );
    }
}

// =============================================================================
// Round-trip (one hermetic direct-core home per test — no $HOME, no daemon)
// =============================================================================

/// add → stored `status=active` → list (human + JSON) → deactivate →
/// stored `status=deprecated`; list still shows the row (all statuses).
#[tokio::test]
async fn add_list_deactivate_round_trip() {
    let env = fresh_env().await;

    let rule_id = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Characters need summaries",
        "rule",
        "Every character entry must carry a summary.",
        "warning",
        &["character".to_string()],
        "active",
        r#"{"family":"required_field","field":"body.summary"}"#,
    )
    .await
    .expect("add on an owned world must succeed")
    .rule_id;

    assert!(
        rule_id.starts_with("rul_") && rule_id.len() == 4 + 32,
        "rule_id must be rul_ + 32 hex (uuid v4 simple), got '{rule_id}' (len {})",
        rule_id.len()
    );

    // Stored truth: default status=active (auto-include needs no step).
    let rows = stored_rules(&env, WORLD).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].rule_id, rule_id);
    assert_eq!(rows[0].status.as_deref(), Some("active"));
    assert_eq!(rows[0].canonical_name, "Characters need summaries");
    assert_eq!(rows[0].kind, "rule");
    assert_eq!(rows[0].severity_hint.as_deref(), Some("warning"));
    assert_eq!(
        rows[0].target_entry_types,
        vec!["character".to_string()],
        "target_entry_types carries the --entry-type array"
    );
    assert_eq!(
        rows[0].constraint,
        serde_json::json!({"family": "required_field", "field": "body.summary"})
            .as_object()
            .unwrap()
            .clone(),
        "constraint is projected first-class from extensions.nexus.constraint"
    );

    // list: human + JSON paths do not error.
    rule_list(&env.core, &env.principal, WORLD, false, false)
        .await
        .unwrap();
    rule_list(&env.core, &env.principal, WORLD, false, true)
        .await
        .unwrap();

    // deactivate: spoke vocabulary "deprecated" (never "inactive").
    rule_deactivate(&env.core, &env.principal, WORLD, &rule_id)
        .await
        .expect("deactivate on an owned world must succeed");
    let rows = stored_rules(&env, WORLD).await;
    assert_eq!(rows.len(), 1, "deactivate keeps the row");
    assert_eq!(rows[0].status.as_deref(), Some("deprecated"));
    assert_ne!(rows[0].status.as_deref(), Some("inactive"), "spoke vocab");

    // list after deactivate still shows the row (all statuses visible).
    rule_list(&env.core, &env.principal, WORLD, false, false)
        .await
        .unwrap();
    rule_list(&env.core, &env.principal, WORLD, false, true)
        .await
        .unwrap();
}

/// The listed item exposes `rule_id` / `canonical_name` / `kind` / `status` /
/// `severity_hint` / `statement` / `target_entry_types` and the first-class
/// `constraint` projection from `extensions.nexus.constraint`.
#[tokio::test]
async fn json_summary_shape_projects_carrier_first_class() {
    let env = fresh_env().await;
    let rule_id = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Observer bound",
        "prohibition",
        "At most three observers per event.",
        "error",
        &[],
        "active",
        OBSERVER_CARDINALITY_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;

    let rows = stored_rules(&env, WORLD).await;
    let summary = &rows[0];
    assert_eq!(summary.rule_id, rule_id);
    assert_eq!(summary.canonical_name, "Observer bound");
    assert_eq!(summary.kind, "prohibition");
    assert_eq!(summary.status.as_deref(), Some("active"));
    assert_eq!(summary.severity_hint.as_deref(), Some("error"));
    assert_eq!(
        summary.statement.as_deref(),
        Some("At most three observers per event.")
    );
    assert!(summary.target_entry_types.is_empty());
    assert_eq!(
        summary.constraint,
        serde_json::json!({"family": "observer_cardinality", "min": 0, "max": 3})
            .as_object()
            .unwrap()
            .clone(),
        "carrier projected first-class from extensions.nexus.constraint"
    );
}

// ── Malformed carrier rejects (core member-aware gate, fail early) ─────

/// Each malformed carrier is rejected with a message naming the offending
/// member, and nothing is written to storage. The closed-shape grammar is
/// owned by the spoke adapter behind the core seam (`constraint.<member>`);
/// only the JSON-object root shape is checked at the CLI boundary.
#[tokio::test]
async fn malformed_carrier_rejects_naming_member_no_write() {
    let env = fresh_env().await;
    let cases: &[(&str, &str)] = &[
        // non-object JSON (CLI boundary: the request carries a JSON object)
        (r"[1,2,3]", "constraint must be a JSON object"),
        (r#""tone""#, "constraint must be a JSON object"),
        // unknown family
        (
            r#"{"family":"tone","module_key":"x"}"#,
            r#"unknown family "tone""#,
        ),
        // entry-level field outside the closed set
        (
            r#"{"family":"required_field","field":"body.plot"}"#,
            r#"unknown "field" value "body.plot""#,
        ),
        // required_field with none of the operand forms
        (
            r#"{"family":"required_field"}"#,
            r#"missing required member "field""#,
        ),
        // required_field with both operand forms (entry field + module_key)
        (
            r#"{"family":"required_field","field":"body.summary","module_key":"characters"}"#,
            "entry-level",
        ),
        // min > max
        (
            r#"{"family":"observer_cardinality","min":5,"max":3}"#,
            r#""min" (5) must not exceed "max" (3)"#,
        ),
        // empty module_key
        (
            r#"{"family":"module_presence","module_key":""}"#,
            r#""module_key" must be a non-empty string"#,
        ),
        // unknown extra member (closed shapes)
        (
            r#"{"family":"module_presence","module_key":"x","bogus":1}"#,
            r#"unknown member "bogus""#,
        ),
        // invalid JSON entirely (CLI boundary)
        ("{not json", "invalid JSON"),
    ];

    for (carrier, expected) in cases {
        let err = rule_add(
            &env.core,
            &env.principal,
            WORLD,
            "Bad carrier",
            "rule",
            "statement",
            "warning",
            &[],
            "active",
            carrier,
        )
        .await
        .expect_err(&format!("carrier {carrier} must be rejected"));
        let msg = err.to_string();
        assert!(
            msg.contains("constraint") && msg.contains(expected),
            "carrier {carrier}: expected a constraint refusal containing {expected:?}, got: {msg}"
        );
    }

    assert!(
        stored_rules(&env, WORLD).await.is_empty(),
        "no rule may be written when the carrier is rejected"
    );
}

/// `--entry-type` alongside an `observer_cardinality` carrier is rejected
/// early (events carry no `entry_type` — AR-2; no silent ignore).
#[tokio::test]
async fn entry_type_with_observer_cardinality_rejected() {
    let env = fresh_env().await;
    let err = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Bad targeting",
        "rule",
        "statement",
        "warning",
        &["character".to_string()],
        "active",
        OBSERVER_CARDINALITY_CARRIER,
    )
    .await
    .expect_err("observer_cardinality + --entry-type must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("target_entry_types") && msg.contains("observer_cardinality"),
        "expected the effective-pair rejection, got: {msg}"
    );

    assert!(
        stored_rules(&env, WORLD).await.is_empty(),
        "rejected add must not write a row"
    );
}

/// `--entry-type` alongside an entry-family carrier is fine (targeting axis).
#[tokio::test]
async fn entry_type_with_entry_family_carrier_accepted() {
    let env = fresh_env().await;
    rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Targeted presence",
        "rule",
        "statement",
        "warning",
        &["character".to_string()],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .expect("entry-family carrier with --entry-type must be accepted");

    let rows = stored_rules(&env, WORLD).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].target_entry_types, vec!["character".to_string()]);
}

// ── Ownership guards (named reject, no write) ─────────────────────────

/// Foreign world (active creator does not own it) → named reject, no write.
#[tokio::test]
async fn add_on_foreign_world_rejected_no_write() {
    let env = fresh_env().await;
    let err = rule_add(
        &env.core,
        &env.principal,
        FOREIGN_WORLD,
        "Sneaky rule",
        "rule",
        "statement",
        "warning",
        &[],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .expect_err("foreign world must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("does not own") && msg.contains(FOREIGN_WORLD),
        "named reject naming the world, got: {msg}"
    );

    // The core refuses the read of a foreign World too, so the write refusal
    // is asserted on the owned World staying empty.
    assert!(stored_rules(&env, WORLD).await.is_empty());
}

/// `deactivate` with a `rule_id` that belongs to another World → named reject
/// naming the rule id (AR-6: unknown and cross-World ids are
/// indistinguishable), and the rule's status is untouched.
#[tokio::test]
async fn deactivate_cross_world_rule_rejected_naming_rule_id() {
    let env = fresh_env().await;
    // The rule lives in a second World of the SAME creator.
    let cross_world_rule_id = rule_add(
        &env.core,
        &env.principal,
        OTHER_OWNED_WORLD,
        "Cross-world rule",
        "rule",
        "statement",
        "warning",
        &[],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;

    let err = rule_deactivate(&env.core, &env.principal, WORLD, &cross_world_rule_id)
        .await
        .expect_err("cross-world rule id must reject");
    let msg = err.to_string();
    assert!(
        msg.contains(&cross_world_rule_id) && msg.contains("404"),
        "named 404 reject naming the rule id, got: {msg}"
    );

    let rows = stored_rules(&env, OTHER_OWNED_WORLD).await;
    assert_eq!(
        rows[0].status.as_deref(),
        Some("active"),
        "cross-world rule status must be untouched"
    );
}

/// `deactivate` on an unknown rule id → named reject naming the rule id.
#[tokio::test]
async fn deactivate_unknown_rule_rejected_naming_rule_id() {
    let env = fresh_env().await;
    let err = rule_deactivate(&env.core, &env.principal, WORLD, "rul_doesnotexist")
        .await
        .expect_err("unknown rule id must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("rul_doesnotexist") && msg.contains("404"),
        "named 404 reject naming the rule id, got: {msg}"
    );
}

/// `deactivate` on a foreign world (creator does not own the world at all)
/// → world-level named reject before any per-rule lookup.
#[tokio::test]
async fn deactivate_on_foreign_world_rejected() {
    let env = fresh_env().await;
    let err = rule_deactivate(&env.core, &env.principal, FOREIGN_WORLD, "rul_whatever")
        .await
        .expect_err("foreign world must reject");
    assert!(err.to_string().contains("does not own"), "got: {err}");
}

// ── --status staging ──────────────────────────────────────────────────

/// `--status draft` creates a row whose stored status is `draft` — it stays
/// out of the auto-include set (status filtering is the adapter boundary,
/// AR-1/T3; storage keeps the verbatim value).
#[tokio::test]
async fn draft_status_row_stored_verbatim() {
    let env = fresh_env().await;
    rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Staged rule",
        "rule",
        "statement",
        "warning",
        &[],
        "draft",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap();

    let rows = stored_rules(&env, WORLD).await;
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status.as_deref(), Some("draft"));
    assert_eq!(rows[0].canonical_name, "Staged rule");
}

/// A `--status` value outside the AR-1 core set is **rejected** by the core's
/// typed rule gate instead of being stored (the old CLI-side "store verbatim
/// + warn" path is gone with the direct-core retarget; an unvalidated status
/// could never auto-include anyway) — and nothing is written.
#[tokio::test]
async fn non_core_status_rejected_no_write() {
    let env = fresh_env().await;
    let err = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Typos happen",
        "rule",
        "statement",
        "warning",
        &[],
        "Active", // capitalized typo — outside the core set
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .expect_err("a non-core status must be rejected");
    let msg = err.to_string();
    assert!(
        msg.contains("status") && msg.contains("draft | active | deprecated"),
        "expected the core's closed status rejection, got: {msg}"
    );

    assert!(
        stored_rules(&env, WORLD).await.is_empty(),
        "rejected add must not write a row"
    );
}

// =============================================================================
// Archive lifecycle (V1.198 §13: terminal, retained, hidden by default)
// =============================================================================

/// `archive` writes the terminal `archived` status, retains the row and
/// succeeds idempotently on repetition. The default read omits it; only the
/// explicit inclusion returns it.
#[tokio::test]
async fn archive_round_trip_terminal_retained_and_idempotent() {
    let env = fresh_env().await;
    let rule_id = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Retire me",
        "rule",
        "Every character entry must carry a summary.",
        "warning",
        &["character".to_string()],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;

    rule_archive(&env.core, &env.principal, WORLD, &rule_id)
        .await
        .expect("archive on an owned world must succeed");

    assert!(
        stored_rules(&env, WORLD).await.is_empty(),
        "the default read omits the archived row"
    );
    let stored = stored_rules_including(&env, WORLD, true).await;
    assert_eq!(stored.len(), 1, "the archived row is retained");
    assert_eq!(stored[0].status.as_deref(), Some("archived"));
    assert_eq!(stored[0].canonical_name, "Retire me");
    assert_eq!(stored[0].target_entry_types, vec!["character".to_string()]);
    assert_eq!(
        stored[0].constraint,
        serde_json::json!({"family": "module_presence", "module_key": "characters"})
            .as_object()
            .unwrap()
            .clone(),
        "archiving retains the carrier"
    );

    // Repetition is retry-safe for scripts and loses no field.
    rule_archive(&env.core, &env.principal, WORLD, &rule_id)
        .await
        .expect("repeating archive must succeed");
    let after_repeat = stored_rules_including(&env, WORLD, true).await;
    assert_eq!(after_repeat.len(), 1);
    assert_eq!(after_repeat[0].status.as_deref(), Some("archived"));
    assert_eq!(after_repeat[0].canonical_name, "Retire me");
}

/// Archiving reaches `archived` from each prior status — an `active`, a
/// create-time `draft` and a `deactivate`d (`deprecated`) rule all end as
/// retained archived rows.
#[tokio::test]
async fn archive_transitions_from_each_prior_status() {
    let env = fresh_env().await;
    let mut ids = Vec::new();
    for (name, status) in [
        ("Active rule", "active"),
        ("Draft rule", "draft"),
        ("Deprecated rule", "active"),
    ] {
        let id = rule_add(
            &env.core,
            &env.principal,
            WORLD,
            name,
            "rule",
            "statement",
            "warning",
            &[],
            status,
            MODULE_PRESENCE_CARRIER,
        )
        .await
        .unwrap()
        .rule_id;
        ids.push((name.to_string(), id));
    }
    // The third row reaches `deprecated` through the CLI seam, not at create.
    let deprecated_id = ids.last().expect("three seeded rules").1.clone();
    rule_deactivate(&env.core, &env.principal, WORLD, &deprecated_id)
        .await
        .expect("deactivate must succeed before archive");
    assert_eq!(
        stored_rules(&env, WORLD).await.len(),
        3,
        "draft and deprecated stay visible in the default read"
    );

    for (_, id) in &ids {
        rule_archive(&env.core, &env.principal, WORLD, id)
            .await
            .expect("archiving a non-archived row must succeed");
    }

    assert!(
        stored_rules(&env, WORLD).await.is_empty(),
        "all three archived rows leave the default read"
    );
    let stored = stored_rules_including(&env, WORLD, true).await;
    assert_eq!(stored.len(), 3);
    assert!(
        stored
            .iter()
            .all(|row| row.status.as_deref() == Some("archived")),
        "each prior status ends as archived: {stored:?}"
    );
    for (name, _) in &ids {
        assert!(
            stored.iter().any(|row| row.canonical_name == *name),
            "the archived row '{name}' is retained"
        );
    }
}

/// The human table and the `--json` array agree on the selected set: the
/// default read omits the archived row from both, and the explicit inclusion
/// shows it in both — with the `STATUS` column printing the stored value.
#[tokio::test]
async fn archive_list_table_and_json_parity() {
    let env = fresh_env().await;
    let kept_id = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Kept rule",
        "rule",
        "statement",
        "warning",
        &[],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;
    let archived_id = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Archived rule",
        "rule",
        "statement",
        "warning",
        &[],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;
    rule_archive(&env.core, &env.principal, WORLD, &archived_id)
        .await
        .unwrap();

    let default_json = rule_list(&env.core, &env.principal, WORLD, false, true)
        .await
        .unwrap()
        .unwrap();
    let default_table = rule_list(&env.core, &env.principal, WORLD, false, false)
        .await
        .unwrap()
        .unwrap();
    let parsed: Vec<WorldRulesListResponseRulesItem> =
        serde_json::from_str(&default_json).expect("default JSON is the rule-item array");
    assert_eq!(
        parsed.len(),
        1,
        "default JSON omits archived: {default_json}"
    );
    assert_eq!(parsed[0].rule_id, kept_id);
    assert!(
        !default_table.contains(&archived_id),
        "default table omits archived: {default_table}"
    );

    let included_json = rule_list(&env.core, &env.principal, WORLD, true, true)
        .await
        .unwrap()
        .unwrap();
    let included_table = rule_list(&env.core, &env.principal, WORLD, true, false)
        .await
        .unwrap()
        .unwrap();
    let parsed: Vec<WorldRulesListResponseRulesItem> =
        serde_json::from_str(&included_json).expect("included JSON is the rule-item array");
    assert_eq!(
        parsed.len(),
        2,
        "included JSON reveals archived: {included_json}"
    );
    let archived_item = parsed
        .iter()
        .find(|row| row.rule_id == archived_id)
        .expect("the archived row is in the included JSON");
    assert_eq!(archived_item.status.as_deref(), Some("archived"));
    assert!(
        included_table.contains(&kept_id) && included_table.contains(&archived_id),
        "the included table lists both rows: {included_table}"
    );
    assert!(
        included_table.lines().any(|line| {
            line.contains("Archived rule") && line.split_whitespace().any(|cell| cell == "archived")
        }),
        "the STATUS column prints the stored value: {included_table}"
    );
}

fn assert_cli_archives_rule(fixture: &DirectFixture, rule_id: &str) {
    let archived = fixture
        .command()
        .args([
            "creator",
            "world",
            "rule",
            "archive",
            "--world-id",
            WORLD,
            "--rule-id",
            rule_id,
        ])
        .output()
        .expect("spawn nexus42 rule archive");
    assert!(
        archived.status.success(),
        "archive failed: {}",
        String::from_utf8_lossy(&archived.stderr)
    );
    assert!(
        String::from_utf8_lossy(&archived.stdout).contains(rule_id),
        "archive success output names the rule: {}",
        String::from_utf8_lossy(&archived.stdout)
    );
}

/// The real binary honours `--include-archived` (AC-7): the default read omits
/// the archived row, the flag reveals it in both renderings.
#[tokio::test]
async fn cli_rule_list_include_archived_flag() {
    let env = fresh_env().await;
    let archived_id = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Archived via CLI",
        "rule",
        "statement",
        "warning",
        &[],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;
    env.core.close().await.expect("seed core closes");
    assert_cli_archives_rule(&env.fixture, &archived_id);

    let default_json = env
        .fixture
        .command()
        .args([
            "creator",
            "world",
            "rule",
            "list",
            "--world-id",
            WORLD,
            "--json",
        ])
        .output()
        .expect("spawn nexus42 rule list");
    assert!(
        default_json.status.success(),
        "default list failed: {}",
        String::from_utf8_lossy(&default_json.stderr)
    );
    let items: Vec<WorldRulesListResponseRulesItem> =
        serde_json::from_str(&String::from_utf8_lossy(&default_json.stdout)).expect("json list");
    assert!(
        items.is_empty(),
        "the default CLI read omits the archived row: {items:?}"
    );

    let included_json = env
        .fixture
        .command()
        .args([
            "creator",
            "world",
            "rule",
            "list",
            "--world-id",
            WORLD,
            "--include-archived",
            "--json",
        ])
        .output()
        .expect("spawn nexus42 rule list --include-archived");
    assert!(
        included_json.status.success(),
        "included list failed: {}",
        String::from_utf8_lossy(&included_json.stderr)
    );
    let items: Vec<WorldRulesListResponseRulesItem> =
        serde_json::from_str(&String::from_utf8_lossy(&included_json.stdout))
            .expect("json included list");
    assert_eq!(items.len(), 1, "the flag reveals the retained row");
    assert_eq!(items[0].rule_id, archived_id);
    assert_eq!(items[0].status.as_deref(), Some("archived"));

    let included_table = env
        .fixture
        .command()
        .args([
            "creator",
            "world",
            "rule",
            "list",
            "--world-id",
            WORLD,
            "--include-archived",
        ])
        .output()
        .expect("spawn nexus42 rule list --include-archived");
    let table = String::from_utf8_lossy(&included_table.stdout).into_owned();
    assert!(
        table.contains(&archived_id) && table.contains("Archived via CLI"),
        "the table lists the archived row: {table}"
    );
    assert!(
        table.lines().any(|line| {
            line.contains("Archived via CLI")
                && line.split_whitespace().any(|cell| cell == "archived")
        }),
        "the STATUS column prints `archived`: {table}"
    );
}

/// The archived row is a terminal tombstone on the CLI seams: the
/// `deactivate` status exit is refused by name (`status`) with no mutation,
/// and `add` refuses `archived` as a create state.
#[tokio::test]
async fn archived_row_refuses_status_exit_and_create_archived() {
    let env = fresh_env().await;
    let rule_id = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Terminal rule",
        "rule",
        "statement",
        "warning",
        &[],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;
    rule_archive(&env.core, &env.principal, WORLD, &rule_id)
        .await
        .unwrap();

    let err = rule_deactivate(&env.core, &env.principal, WORLD, &rule_id)
        .await
        .expect_err("a status exit from archived must be refused");
    let msg = err.to_string();
    assert!(
        msg.contains("invalid input (status)") && msg.contains("terminal"),
        "the refusal names `status` and the terminal policy, got: {msg}"
    );
    let stored = stored_rules_including(&env, WORLD, true).await;
    assert_eq!(stored.len(), 1, "the refused exit wrote nothing");
    assert_eq!(stored[0].status.as_deref(), Some("archived"));
    assert_eq!(stored[0].canonical_name, "Terminal rule");

    // `--status archived` is refused at create (archive is a transition only).
    let err = rule_add(
        &env.core,
        &env.principal,
        WORLD,
        "Born archived",
        "rule",
        "statement",
        "warning",
        &[],
        "archived",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .expect_err("archived must not be a create state");
    let msg = err.to_string();
    assert!(
        msg.contains("status") && msg.contains("draft | active | deprecated"),
        "expected the closed create-status rejection, got: {msg}"
    );
    assert_eq!(
        stored_rules_including(&env, WORLD, true).await.len(),
        1,
        "the refused create wrote no row"
    );
}

/// `creator world rule add --status archived` (the real binary) still refuses
/// the create with the field-level status error — `archived` is a transition,
/// never an authoring state (V1.198 §13).
#[tokio::test]
async fn cli_rule_add_status_archived_refused() {
    let env = fresh_env().await;
    env.core.close().await.expect("seed core closes");

    let out = env
        .fixture
        .command()
        .args([
            "creator",
            "world",
            "rule",
            "add",
            "--world-id",
            WORLD,
            "--name",
            "Born archived",
            "--statement",
            "statement",
            "--constraint",
            MODULE_PRESENCE_CARRIER,
            "--status",
            "archived",
        ])
        .output()
        .expect("spawn nexus42 rule add");
    assert!(
        !out.status.success(),
        "add --status archived must fail: {}",
        String::from_utf8_lossy(&out.stdout)
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("status") && stderr.contains("draft | active | deprecated"),
        "the refusal names the closed create set, got: {stderr}"
    );
}

/// Ownership and id-addressing guards are unchanged for `archive`: an unknown
/// or cross-World rule id is the named 404 (indistinguishable, AR-6) and a
/// foreign World is the world-ownership refusal.
#[tokio::test]
async fn archive_guard_parity_unknown_cross_world_and_foreign() {
    let env = fresh_env().await;
    let cross_world_id = rule_add(
        &env.core,
        &env.principal,
        OTHER_OWNED_WORLD,
        "Cross-world rule",
        "rule",
        "statement",
        "warning",
        &[],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap()
    .rule_id;

    for rule_id in ["rul_doesnotexist", cross_world_id.as_str()] {
        let err = rule_archive(&env.core, &env.principal, WORLD, rule_id)
            .await
            .expect_err("unknown and cross-World ids must reject");
        let msg = err.to_string();
        assert!(
            msg.contains(rule_id) && msg.contains("404"),
            "named 404 naming the rule id, got: {msg}"
        );
    }

    let err = rule_archive(&env.core, &env.principal, FOREIGN_WORLD, "rul_whatever")
        .await
        .expect_err("a foreign World must reject before any rule lookup");
    assert!(err.to_string().contains("does not own"), "got: {err}");

    let rows = stored_rules(&env, OTHER_OWNED_WORLD).await;
    assert_eq!(
        rows[0].status.as_deref(),
        Some("active"),
        "a refused archive leaves the cross-World rule untouched"
    );
}
