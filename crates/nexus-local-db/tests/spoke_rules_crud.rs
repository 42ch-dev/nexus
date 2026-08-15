//! `spoke_rules` production CRUD tests (V1.166 P1 T1 / DR-64, AR-3) — pure
//! storage, no spoke vocabulary. The store persists raw column values; JSON
//! columns (`target_entry_types_json` / `source_anchor_json` /
//! `extensions_json`) stay opaque strings; `created_at` / `updated_at` are
//! Unix epoch seconds.
//!
//! Covers: full-row insert round-trip verbatim; PK conflict classified as
//! `LocalDbError::ConstraintViolation { table: "spoke_rules", constraint:
//! "rule_id" }`; world-guarded status transition (same world → `Ok(true)` +
//! `status`/`updated_at` refreshed; unknown id → `Ok(false)`; foreign world with
//! known id → `Ok(false)`, row untouched); world-scoped list with
//! `canonical_name ASC, rule_id ASC` ordering incl. tie-break, all statuses,
//! cross-world isolation, unknown world → empty.

#![allow(clippy::unwrap_used)]

use nexus_local_db::spoke_rules::{
    insert_rule, list_rules_by_world, set_rule_status, SpokeRuleRow,
};
use nexus_local_db::{open_pool, run_migrations, LocalDbError};

async fn setup_db() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let pool = open_pool(&db_path).await.unwrap();
    run_migrations(&pool).await.unwrap();
    (pool, dir)
}

/// Full row exercising every column, including the opaque JSON carriers and
/// epoch timestamps.
fn full_row(rule_id: &str, world_id: &str, canonical_name: &str) -> SpokeRuleRow {
    SpokeRuleRow {
        rule_id: rule_id.to_string(),
        world_id: world_id.to_string(),
        schema_version: 1,
        canonical_name: canonical_name.to_string(),
        kind: "prohibition".to_string(),
        statement: Some(format!("{canonical_name} statement")),
        description: Some(format!("{canonical_name} description")),
        target_entry_types_json: r#"["character","event"]"#.to_string(),
        severity_hint: Some("error".to_string()),
        status: Some("draft".to_string()),
        source_anchor_json: Some(r#"{"kind":"paragraph","ref":"ch3#p12"}"#.to_string()),
        extensions_json:
            r#"{"nexus":{"constraint":{"family":"required_field","field":"body.summary"}}}"#
                .to_string(),
        created_at: Some(1_700_000_000),
        updated_at: Some(1_700_000_042),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn insert_rule_round_trips_full_row_verbatim() {
    let (pool, _dir) = setup_db().await;
    let row = full_row("rul_full", "wld_1", "full row");

    insert_rule(&pool, &row).await.unwrap();

    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed.len(), 1, "one row for the seeded world");
    let got = &listed[0];
    assert_eq!(got.rule_id, "rul_full");
    assert_eq!(got.world_id, "wld_1");
    assert_eq!(got.schema_version, 1);
    assert_eq!(got.canonical_name, "full row");
    assert_eq!(got.kind, "prohibition");
    assert_eq!(got.statement.as_deref(), Some("full row statement"));
    assert_eq!(got.description.as_deref(), Some("full row description"));
    // Opaque JSON columns round-trip byte-for-byte (no nexus-side parsing).
    assert_eq!(got.target_entry_types_json, r#"["character","event"]"#);
    assert_eq!(got.severity_hint.as_deref(), Some("error"));
    assert_eq!(got.status.as_deref(), Some("draft"));
    assert_eq!(
        got.source_anchor_json.as_deref(),
        Some(r#"{"kind":"paragraph","ref":"ch3#p12"}"#)
    );
    assert_eq!(
        got.extensions_json,
        r#"{"nexus":{"constraint":{"family":"required_field","field":"body.summary"}}}"#
    );
    assert_eq!(got.created_at, Some(1_700_000_000));
    assert_eq!(got.updated_at, Some(1_700_000_042));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn insert_rule_duplicate_rule_id_classifies_constraint_violation() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_dup", "wld_1", "first"))
        .await
        .unwrap();

    let err = insert_rule(&pool, &full_row("rul_dup", "wld_1", "second"))
        .await
        .expect_err("duplicate rule_id must be rejected");
    match err {
        LocalDbError::ConstraintViolation { table, constraint } => {
            assert_eq!(table, "spoke_rules");
            assert_eq!(constraint, "rule_id");
        }
        other => panic!("expected ConstraintViolation, got {other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_status_same_world_transition_updates_status_and_updated_at() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_st", "wld_1", "status rule"))
        .await
        .unwrap();

    let updated = set_rule_status(&pool, "wld_1", "rul_st", "deprecated")
        .await
        .unwrap();
    assert!(updated, "same-world transition must match");

    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].status.as_deref(), Some("deprecated"));
    let refreshed_at = listed[0]
        .updated_at
        .expect("updated_at must be refreshed on transition");
    assert!(
        refreshed_at > 1_700_000_042,
        "updated_at must move past the seeded value, got {refreshed_at}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn set_rule_status_unknown_id_and_foreign_world_return_false() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_w1", "wld_1", "world one"))
        .await
        .unwrap();
    insert_rule(&pool, &full_row("rul_w2", "wld_2", "world two"))
        .await
        .unwrap();

    // Unknown rule_id in an otherwise valid world.
    let updated = set_rule_status(&pool, "wld_1", "rul_missing", "deprecated")
        .await
        .unwrap();
    assert!(!updated, "unknown rule_id must not match");

    // Known rule_id, foreign world_id — the world guard must hold it back.
    let updated = set_rule_status(&pool, "wld_2", "rul_w1", "deprecated")
        .await
        .unwrap();
    assert!(!updated, "foreign world_id must not match");

    // The guarded attempt must not have mutated the row.
    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed[0].rule_id, "rul_w1");
    assert_eq!(
        listed[0].status.as_deref(),
        Some("draft"),
        "foreign-world attempt must leave the row untouched"
    );
    assert_eq!(listed[0].updated_at, Some(1_700_000_042));

    // The other world's own transition still works (isolation, not a lock).
    let updated = set_rule_status(&pool, "wld_2", "rul_w2", "active")
        .await
        .unwrap();
    assert!(
        updated,
        "same-world transition in the other world must match"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn list_rules_by_world_orders_and_isolates() {
    let (pool, _dir) = setup_db().await;
    // Insert in shuffled order; names chosen to exercise the
    // `canonical_name ASC, rule_id ASC` tie-break (two "beta" rows).
    insert_rule(&pool, &full_row("rul_beta2", "wld_1", "beta"))
        .await
        .unwrap();
    insert_rule(&pool, &full_row("rul_gamma", "wld_1", "gamma"))
        .await
        .unwrap();
    insert_rule(&pool, &full_row("rul_alpha", "wld_1", "alpha"))
        .await
        .unwrap();
    insert_rule(&pool, &full_row("rul_beta1", "wld_1", "beta"))
        .await
        .unwrap();
    insert_rule(&pool, &full_row("rul_other", "wld_2", "other world"))
        .await
        .unwrap();

    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    let ids: Vec<&str> = listed.iter().map(|r| r.rule_id.as_str()).collect();
    assert_eq!(
        ids,
        vec!["rul_alpha", "rul_beta1", "rul_beta2", "rul_gamma"],
        "canonical_name ASC with rule_id ASC tie-break; other worlds absent"
    );

    // Unknown world → empty vec, not an error.
    let none = list_rules_by_world(&pool, "wld_nope").await.unwrap();
    assert!(none.is_empty(), "unknown world must return an empty vec");
}
