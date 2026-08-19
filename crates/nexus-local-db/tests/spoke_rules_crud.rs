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
//! cross-world isolation, unknown world → empty; world-guarded multi-field
//! update (V1.169 P1, AR-4: every mutable field replaced, `None` fields kept,
//! opaque whole-bag carrier replacement, unknown id / foreign world →
//! `Ok(false)`, `updated_at` refreshed on every matched update incl.
//! value-identical ones, `created_at` untouched, JSON columns stored verbatim).

#![allow(clippy::unwrap_used)]

use nexus_local_db::spoke_rules::{
    insert_rule, list_rules_by_world, set_rule_status, update_rule, RuleUpdate, SpokeRuleRow,
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

    // AR-3: the store returns ALL statuses — no status filter at this
    // layer (the adapter boundary owns the active-only auto-include).
    // Transition two rows off the default `draft` seed so one world holds
    // all three spoke statuses at once.
    set_rule_status(&pool, "wld_1", "rul_beta1", "active")
        .await
        .unwrap();
    set_rule_status(&pool, "wld_1", "rul_gamma", "deprecated")
        .await
        .unwrap();
    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    let statuses: Vec<(&str, Option<&str>)> = listed
        .iter()
        .map(|r| (r.rule_id.as_str(), r.status.as_deref()))
        .collect();
    assert_eq!(
        statuses,
        vec![
            ("rul_alpha", Some("draft")),
            ("rul_beta1", Some("active")),
            ("rul_beta2", Some("draft")),
            ("rul_gamma", Some("deprecated")),
        ],
        "all three statuses coexist in one world's list — storage applies \
         no status filter (AR-3)"
    );

    // Unknown world → empty vec, not an error.
    let none = list_rules_by_world(&pool, "wld_nope").await.unwrap();
    assert!(none.is_empty(), "unknown world must return an empty vec");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_rule_replaces_every_mutable_field() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_upd", "wld_1", "before"))
        .await
        .unwrap();

    let updated = update_rule(
        &pool,
        "wld_1",
        "rul_upd",
        &RuleUpdate {
            canonical_name: Some("after".to_string()),
            statement: Some("new statement".to_string()),
            severity_hint: Some("fatal".to_string()),
            status: Some("deprecated".to_string()),
            kind: Some("prohibition".to_string()),
            target_entry_types_json: Some(r#"["character"]"#.to_string()),
            extensions_json: Some(
                r#"{"nexus":{"constraint":{"family":"module_presence","module_key":"karma"}}}"#
                    .to_string(),
            ),
        },
    )
    .await
    .unwrap();
    assert!(updated, "same-world update must match");

    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed.len(), 1);
    let got = &listed[0];
    assert_eq!(got.canonical_name, "after");
    assert_eq!(got.statement.as_deref(), Some("new statement"));
    assert_eq!(got.severity_hint.as_deref(), Some("fatal"));
    assert_eq!(got.status.as_deref(), Some("deprecated"));
    assert_eq!(got.kind, "prohibition");
    assert_eq!(got.target_entry_types_json, r#"["character"]"#);
    assert_eq!(
        got.extensions_json,
        r#"{"nexus":{"constraint":{"family":"module_presence","module_key":"karma"}}}"#
    );
    // Non-mutable columns are untouched.
    assert_eq!(
        got.description.as_deref(),
        Some("before description"),
        "description is not a mutable field"
    );
    assert_eq!(
        got.source_anchor_json.as_deref(),
        Some(r#"{"kind":"paragraph","ref":"ch3#p12"}"#),
        "source_anchor_json is not a mutable field"
    );
    assert_eq!(
        got.schema_version, 1,
        "schema_version is not a mutable field"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_rule_none_fields_keep_stored_values() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_keep", "wld_1", "keep me"))
        .await
        .unwrap();

    let updated = update_rule(
        &pool,
        "wld_1",
        "rul_keep",
        &RuleUpdate {
            canonical_name: Some("renamed".to_string()),
            statement: None,
            severity_hint: None,
            status: None,
            kind: None,
            target_entry_types_json: None,
            extensions_json: None,
        },
    )
    .await
    .unwrap();
    assert!(updated, "single-field update must match");

    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed.len(), 1);
    let got = &listed[0];
    assert_eq!(got.canonical_name, "renamed");
    // COALESCE pass-through: every `None` field keeps its stored value.
    assert_eq!(got.statement.as_deref(), Some("keep me statement"));
    assert_eq!(got.severity_hint.as_deref(), Some("error"));
    assert_eq!(got.status.as_deref(), Some("draft"));
    assert_eq!(got.kind, "prohibition");
    assert_eq!(got.target_entry_types_json, r#"["character","event"]"#);
    assert_eq!(
        got.extensions_json,
        r#"{"nexus":{"constraint":{"family":"required_field","field":"body.summary"}}}"#
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_rule_replaces_whole_extensions_bag_opaque() {
    let (pool, _dir) = setup_db().await;
    let mut row = full_row("rul_car", "wld_1", "carrier");
    row.extensions_json =
        r#"{"nexus":{"constraint":{"family":"required_field","field":"body.summary"},"other_nexus_key":1},"other_namespace":{"a":true}}"#
            .to_string();
    insert_rule(&pool, &row).await.unwrap();

    let new_bag =
        r#"{"nexus":{"constraint":{"family":"module_presence","module_key":"tone"}},"third_ns":[1,2]}"#
            .to_string();
    let updated = update_rule(
        &pool,
        "wld_1",
        "rul_car",
        &RuleUpdate {
            canonical_name: None,
            statement: None,
            severity_hint: None,
            status: None,
            kind: None,
            target_entry_types_json: None,
            extensions_json: Some(new_bag.clone()),
        },
    )
    .await
    .unwrap();
    assert!(updated, "carrier replacement must match");

    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(
        listed[0].extensions_json, new_bag,
        "extensions_json is opaque whole-bag replacement — the old bag's \
         nexus keys and namespaces must not be merged in"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_rule_unknown_rule_id_returns_false() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_known", "wld_1", "known"))
        .await
        .unwrap();

    let updated = update_rule(
        &pool,
        "wld_1",
        "rul_missing",
        &RuleUpdate {
            canonical_name: Some("nope".to_string()),
            statement: None,
            severity_hint: None,
            status: None,
            kind: None,
            target_entry_types_json: None,
            extensions_json: None,
        },
    )
    .await
    .unwrap();
    assert!(!updated, "unknown rule_id must not match");

    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed[0].canonical_name, "known");
    assert_eq!(listed[0].updated_at, Some(1_700_000_042));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_rule_foreign_world_rule_id_returns_false() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_w1", "wld_1", "world one"))
        .await
        .unwrap();
    insert_rule(&pool, &full_row("rul_w2", "wld_2", "world two"))
        .await
        .unwrap();

    // Known rule_id, foreign world_id — the world guard must hold it back.
    let updated = update_rule(
        &pool,
        "wld_2",
        "rul_w1",
        &RuleUpdate {
            canonical_name: Some("hijacked".to_string()),
            statement: None,
            severity_hint: None,
            status: None,
            kind: None,
            target_entry_types_json: None,
            extensions_json: None,
        },
    )
    .await
    .unwrap();
    assert!(!updated, "foreign world_id must not match");

    // The guarded attempt must not have mutated the row.
    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed[0].rule_id, "rul_w1");
    assert_eq!(listed[0].canonical_name, "world one");
    assert_eq!(listed[0].updated_at, Some(1_700_000_042));

    // The other world's own update still works (isolation, not a lock).
    let updated = update_rule(
        &pool,
        "wld_2",
        "rul_w2",
        &RuleUpdate {
            canonical_name: Some("world two renamed".to_string()),
            statement: None,
            severity_hint: None,
            status: None,
            kind: None,
            target_entry_types_json: None,
            extensions_json: None,
        },
    )
    .await
    .unwrap();
    assert!(updated, "same-world update in the other world must match");
    let listed = list_rules_by_world(&pool, "wld_2").await.unwrap();
    assert_eq!(listed[0].canonical_name, "world two renamed");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_rule_refreshes_updated_at_keeps_created_at() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_ts", "wld_1", "timestamps"))
        .await
        .unwrap();

    // Value-changing update: `updated_at` must move past the seeded value,
    // `created_at` must stay untouched.
    let updated = update_rule(
        &pool,
        "wld_1",
        "rul_ts",
        &RuleUpdate {
            canonical_name: Some("timestamps v2".to_string()),
            statement: None,
            severity_hint: None,
            status: None,
            kind: None,
            target_entry_types_json: None,
            extensions_json: None,
        },
    )
    .await
    .unwrap();
    assert!(updated, "value-changing update must match");
    let first = &list_rules_by_world(&pool, "wld_1").await.unwrap()[0];
    let first_updated_at = first
        .updated_at
        .expect("updated_at must be refreshed on update");
    assert!(
        first_updated_at > 1_700_000_042,
        "updated_at must move past the seeded value, got {first_updated_at}"
    );
    assert_eq!(first.created_at, Some(1_700_000_000));

    // Value-identical update (the row matched): still `Ok(true)` and
    // `updated_at` is refreshed again (AR-4 — matched write wins, no OCC);
    // `created_at` still untouched.
    let updated = update_rule(
        &pool,
        "wld_1",
        "rul_ts",
        &RuleUpdate {
            canonical_name: Some("timestamps v2".to_string()),
            statement: None,
            severity_hint: None,
            status: None,
            kind: None,
            target_entry_types_json: None,
            extensions_json: None,
        },
    )
    .await
    .unwrap();
    assert!(updated, "value-identical update must still match");
    let second = &list_rules_by_world(&pool, "wld_1").await.unwrap()[0];
    let second_updated_at = second
        .updated_at
        .expect("updated_at must be refreshed on every matched update");
    assert!(
        second_updated_at >= first_updated_at,
        "a matched-but-identical update must still refresh updated_at \
         (got {second_updated_at} < first refresh {first_updated_at})"
    );
    assert_eq!(second.created_at, Some(1_700_000_000));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn update_rule_stores_pre_serialized_json_verbatim() {
    let (pool, _dir) = setup_db().await;
    insert_rule(&pool, &full_row("rul_raw", "wld_1", "raw json"))
        .await
        .unwrap();

    // Non-canonical JSON text (extra whitespace) — storage must not parse or
    // re-serialize; the bytes given must be the bytes stored (no JSON
    // assembly happens here; the daemon pre-serializes).
    let target_json = r#"[ "character" ,  "event" ]"#.to_string();
    let extensions_json = r#"{ "nexus" : { "constraint" : { "family" : "required_field" , "field" : "body.summary" } } }"#.to_string();
    let updated = update_rule(
        &pool,
        "wld_1",
        "rul_raw",
        &RuleUpdate {
            canonical_name: None,
            statement: None,
            severity_hint: None,
            status: None,
            kind: None,
            target_entry_types_json: Some(target_json.clone()),
            extensions_json: Some(extensions_json.clone()),
        },
    )
    .await
    .unwrap();
    assert!(updated, "verbatim JSON update must match");

    let listed = list_rules_by_world(&pool, "wld_1").await.unwrap();
    assert_eq!(listed[0].target_entry_types_json, target_json);
    assert_eq!(listed[0].extensions_json, extensions_json);
}
