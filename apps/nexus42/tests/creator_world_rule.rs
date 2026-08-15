//! Hermetic tests for the `creator world rule add|list|deactivate` author CLI
//! (V1.166 PD-1 / AR-2 / AR-3, DR-64) + the CLI-surface smoke via `assert_cmd`.
//!
//! Plan: `.mstar/plans/2026-08-15-v1.166-p1-rules-driven-check-evaluator.md`
//! Spec: `.mstar/iterations/v1.166/specs/v1.166-quality-loop-locks.md` §PD-1 / §AR-2 / §AR-3
//!
//! Drives the hermetic logic functions (`rule_add` / `rule_list` /
//! `rule_deactivate` / `rule_summary_json`) directly against a fresh temp DB —
//! no `$HOME`, no daemon (`world_kb_cli.rs` precedent). Storage assertions
//! read `spoke_rules` via `list_rules_by_world` (the ground truth).
//!
//! Run with: cargo test -p nexus42 --test `creator_world_rule`

#![allow(clippy::unwrap_used)]

use assert_cmd::Command;
use nexus42::commands::creator::world::rule::{
    rule_add, rule_deactivate, rule_list, rule_summary_json,
};
use nexus42::db::Schema;
use nexus_local_db::spoke_rules::list_rules_by_world;

const OWNER: &str = "ctr_owner";
const OTHER: &str = "ctr_other";
const WORLD: &str = "wld_rule_test";
const FOREIGN_WORLD: &str = "wld_rule_foreign";

/// Fresh migrated pool with two worlds: `WORLD` owned by `OWNER`, and
/// `FOREIGN_WORLD` owned by `OTHER` (cross-world isolation fixture).
async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("state.db");
    let pool = Schema::init(&db_path).await.unwrap();
    nexus_local_db::kb_store::seed::world(
        &pool,
        WORLD,
        OWNER,
        "Rule Test World",
        "rule-test-world",
        "private",
        "manual",
    )
    .await;
    nexus_local_db::kb_store::seed::world(
        &pool,
        FOREIGN_WORLD,
        OTHER,
        "Foreign Rule World",
        "foreign-rule-world",
        "private",
        "manual",
    )
    .await;
    (pool, dir)
}

/// The canonical valid carriers used across the round-trip tests.
const MODULE_PRESENCE_CARRIER: &str = r#"{"family":"module_presence","module_key":"characters"}"#;
const OBSERVER_CARDINALITY_CARRIER: &str = r#"{"family":"observer_cardinality","min":0,"max":3}"#;

// =============================================================================
// CLI surface (assert_cmd)
// =============================================================================

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
// Hermetic round-trip (fresh pool per test — no $HOME, no daemon)
// =============================================================================

/// add → storage `status=active` → list (human + JSON) → deactivate →
/// storage `status=deprecated`; list still shows the row (all statuses).
#[tokio::test]
async fn add_list_deactivate_round_trip() {
    let (pool, _dir) = fresh_pool().await;

    let rule_id = rule_add(
        &pool,
        OWNER,
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

    // Storage ground truth: default status=active (auto-include needs no step).
    let rows = list_rules_by_world(&pool, WORLD).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].rule_id, rule_id);
    assert_eq!(rows[0].status.as_deref(), Some("active"));
    assert_eq!(rows[0].canonical_name, "Characters need summaries");
    assert_eq!(rows[0].kind, "rule");
    assert_eq!(rows[0].severity_hint.as_deref(), Some("warning"));
    assert_eq!(
        rows[0].target_entry_types_json, r#"["character"]"#,
        "target_entry_types_json carries the --entry-type array"
    );
    assert_eq!(
        rows[0].extensions_json,
        r#"{"nexus":{"constraint":{"family":"required_field","field":"body.summary"}}}"#,
        "extensions_json = {{\"nexus\": {{\"constraint\": <carrier verbatim>}}}}"
    );

    // list: human + JSON paths do not error.
    rule_list(&pool, WORLD, false).await.unwrap();
    rule_list(&pool, WORLD, true).await.unwrap();

    // deactivate: spoke vocabulary "deprecated" (never "inactive").
    rule_deactivate(&pool, OWNER, WORLD, &rule_id)
        .await
        .expect("deactivate on an owned world must succeed");
    let rows = list_rules_by_world(&pool, WORLD).await.unwrap();
    assert_eq!(rows.len(), 1, "deactivate keeps the row");
    assert_eq!(rows[0].status.as_deref(), Some("deprecated"));
    assert_ne!(rows[0].status.as_deref(), Some("inactive"), "spoke vocab");

    // list after deactivate still shows the row (all statuses visible).
    rule_list(&pool, WORLD, false).await.unwrap();
    rule_list(&pool, WORLD, true).await.unwrap();
}

/// `--json` list shape: `rule_summary_json` exposes `rule_id` /
/// `canonical_name` / `kind` / `status` / `severity_hint` / `statement` /
/// `target_entry_types` and the first-class `constraint` projection from
/// `extensions.nexus.constraint`.
#[tokio::test]
async fn json_summary_shape_projects_carrier_first_class() {
    let (pool, _dir) = fresh_pool().await;
    let rule_id = rule_add(
        &pool,
        OWNER,
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

    let rows = list_rules_by_world(&pool, WORLD).await.unwrap();
    let summary = rule_summary_json(&rows[0]);
    assert_eq!(summary["rule_id"], rule_id);
    assert_eq!(summary["canonical_name"], "Observer bound");
    assert_eq!(summary["kind"], "prohibition");
    assert_eq!(summary["status"], "active");
    assert_eq!(summary["severity_hint"], "error");
    assert_eq!(summary["statement"], "At most three observers per event.");
    assert_eq!(summary["target_entry_types"], serde_json::json!([]));
    assert_eq!(
        summary["constraint"],
        serde_json::json!({"family": "observer_cardinality", "min": 0, "max": 3}),
        "carrier projected first-class from extensions_json"
    );
}

// ── Malformed carrier rejects (CLI-only gate, fail early) ─────────────

/// Each malformed carrier is rejected with a `--constraint:` message naming
/// the offending member, and nothing is written to storage.
#[tokio::test]
async fn malformed_carrier_rejects_naming_member_no_write() {
    let (pool, _dir) = fresh_pool().await;
    let cases: &[(&str, &str)] = &[
        // non-object JSON
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
        // invalid JSON entirely
        ("{not json", "invalid JSON"),
    ];

    for (carrier, expected) in cases {
        let err = rule_add(
            &pool,
            OWNER,
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
            msg.contains("--constraint:") && msg.contains(expected),
            "carrier {carrier}: expected '--constraint:' + {expected:?}, got: {msg}"
        );
    }

    let rows = list_rules_by_world(&pool, WORLD).await.unwrap();
    assert!(
        rows.is_empty(),
        "no rule may be written when the carrier is rejected"
    );
}

/// `--entry-type` alongside an `observer_cardinality` carrier is rejected
/// early (events carry no `entry_type` — AR-2; no silent ignore).
#[tokio::test]
async fn entry_type_with_observer_cardinality_rejected() {
    let (pool, _dir) = fresh_pool().await;
    let err = rule_add(
        &pool,
        OWNER,
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
    assert!(
        err.to_string().contains("--entry-type"),
        "expected --entry-type rejection, got: {err}"
    );

    let rows = list_rules_by_world(&pool, WORLD).await.unwrap();
    assert!(rows.is_empty(), "rejected add must not write a row");
}

/// `--entry-type` alongside an entry-family carrier is fine (targeting axis).
#[tokio::test]
async fn entry_type_with_entry_family_carrier_accepted() {
    let (pool, _dir) = fresh_pool().await;
    rule_add(
        &pool,
        OWNER,
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
}

// ── Ownership guards (named reject, no write) ─────────────────────────

/// Foreign world (active creator does not own it) → named reject, no write.
#[tokio::test]
async fn add_on_foreign_world_rejected_no_write() {
    let (pool, _dir) = fresh_pool().await;
    let err = rule_add(
        &pool,
        OWNER,
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
        msg.contains("does not own world") && msg.contains(FOREIGN_WORLD),
        "named reject naming the world, got: {msg}"
    );
    let rows = list_rules_by_world(&pool, FOREIGN_WORLD).await.unwrap();
    assert!(rows.is_empty(), "no write on a foreign world");
}

/// `deactivate` on a foreign rule id → named reject naming the rule id, and
/// the foreign rule's status is untouched.
#[tokio::test]
async fn deactivate_foreign_rule_rejected_naming_rule_id() {
    let (pool, _dir) = fresh_pool().await;
    // OTHER adds a rule to FOREIGN_WORLD; OWNER tries to deactivate it from WORLD.
    let foreign_rule_id = rule_add(
        &pool,
        OTHER,
        FOREIGN_WORLD,
        "Foreign rule",
        "rule",
        "statement",
        "warning",
        &[],
        "active",
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap();

    let err = rule_deactivate(&pool, OWNER, WORLD, &foreign_rule_id)
        .await
        .expect_err("foreign rule id must reject");
    let msg = err.to_string();
    assert!(
        msg.contains(&foreign_rule_id) && msg.contains("not found in world"),
        "named reject naming the rule id, got: {msg}"
    );

    let rows = list_rules_by_world(&pool, FOREIGN_WORLD).await.unwrap();
    assert_eq!(
        rows[0].status.as_deref(),
        Some("active"),
        "foreign rule status must be untouched"
    );
}

/// `deactivate` on an unknown rule id → named reject naming the rule id.
#[tokio::test]
async fn deactivate_unknown_rule_rejected_naming_rule_id() {
    let (pool, _dir) = fresh_pool().await;
    let err = rule_deactivate(&pool, OWNER, WORLD, "rul_doesnotexist")
        .await
        .expect_err("unknown rule id must reject");
    let msg = err.to_string();
    assert!(
        msg.contains("rul_doesnotexist") && msg.contains("not found in world"),
        "named reject naming the rule id, got: {msg}"
    );
}

/// `deactivate` on a foreign world (creator does not own the world at all)
/// → world-level named reject before any per-rule lookup.
#[tokio::test]
async fn deactivate_on_foreign_world_rejected() {
    let (pool, _dir) = fresh_pool().await;
    let err = rule_deactivate(&pool, OWNER, FOREIGN_WORLD, "rul_whatever")
        .await
        .expect_err("foreign world must reject");
    assert!(err.to_string().contains("does not own world"), "got: {err}");
}

// ── --status draft staging ────────────────────────────────────────────

/// `--status draft` creates a row whose stored status is `draft` — it stays
/// out of the auto-include set (status filtering is the adapter boundary,
/// AR-1/T3; storage keeps the verbatim value).
#[tokio::test]
async fn draft_status_row_stored_verbatim() {
    let (pool, _dir) = fresh_pool().await;
    rule_add(
        &pool,
        OWNER,
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

    let rows = list_rules_by_world(&pool, WORLD).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].status.as_deref(), Some("draft"));
    assert_eq!(rows[0].canonical_name, "Staged rule");
}

/// S-002: a non-core `--status` value (typo, capitalization, dialect) is
/// stored **verbatim** — the `add` path emits a soft stderr warning but
/// never coerces at rest (PD-1 open strings; the AR-1 auto-include filter
/// matches exactly `active`, so such a rule simply never auto-includes).
#[tokio::test]
async fn non_core_status_stored_verbatim_no_coercion() {
    let (pool, _dir) = fresh_pool().await;
    rule_add(
        &pool,
        OWNER,
        WORLD,
        "Typos happen",
        "rule",
        "statement",
        "warning",
        &[],
        "Active", // capitalized typo — outside the documented core set
        MODULE_PRESENCE_CARRIER,
    )
    .await
    .unwrap();

    let rows = list_rules_by_world(&pool, WORLD).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(
        rows[0].status.as_deref(),
        Some("Active"),
        "non-core status must be stored verbatim — the CLI warns, never coerces (PD-1)"
    );
}
