//! Server-free tests for the `creator world rule add|list|deactivate` author
//! CLI (V1.166 PD-1 / AR-2 / AR-3, DR-64; direct-core retarget v1.193 P0-T3).
//!
//! Plan: `.mstar/plans/2026-08-15-v1.166-p1-rules-driven-check-evaluator.md`
//! Spec: `.mstar/iterations/v1.166/specs/v1.166-quality-locks.md` §PD-1 / §AR-2 / §AR-3
//!
//! Drives the leaf functions (`rule_add` / `rule_list` / `rule_deactivate`)
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
use nexus42::commands::creator::world::rule::{rule_add, rule_deactivate, rule_list};
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
/// renders.
async fn stored_rules(env: &RuleEnv, world_id: &str) -> Vec<WorldRulesListResponseRulesItem> {
    env.core
        .list_world_rules(&env.principal, world_id.to_string())
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
    .unwrap();
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

/// `creator world rule --help` lists the three subcommands.
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
    for subcmd in &["add", "list", "deactivate"] {
        assert!(
            help_text.contains(subcmd),
            "creator world rule --help must list '{subcmd}' subcommand: {help_text}"
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
    .expect("add on an owned world must succeed");

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
    rule_list(&env.core, &env.principal, WORLD, false)
        .await
        .unwrap();
    rule_list(&env.core, &env.principal, WORLD, true)
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
    rule_list(&env.core, &env.principal, WORLD, false)
        .await
        .unwrap();
    rule_list(&env.core, &env.principal, WORLD, true)
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
    .unwrap();

    let rows = stored_rules(&env, WORLD).await;
    let summary = &rows[0];
    assert_eq!(summary.rule_id, rule_id);
    assert_eq!(summary.canonical_name, "Observer bound");
    assert_eq!(summary.kind, "prohibition");
    assert_eq!(summary.status.as_deref(), Some("active"));
    assert_eq!(summary.severity_hint.as_deref(), Some("error"));
    assert_eq!(summary.statement.as_deref(), Some("At most three observers per event."));
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
        (r#"{"family":"tone","module_key":"x"}"#, r#"unknown family "tone""#),
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
    .unwrap();

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
