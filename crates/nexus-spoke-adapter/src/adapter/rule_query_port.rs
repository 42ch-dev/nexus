//! Production `RuleQueryPort` impl — resolves spoke `rule_refs` against the
//! persisted `spoke_rules` table (spec §7.4; V1.148 P1, closes R-V1142P1-001).
//!
//! # Wire ↔ row mapping (spoke 0.8.2 `data/rule.schema.json`)
//!
//! [`RuleQueryPort::list_rules`] looks up [`SpokeRuleRow`]s by `rule_id` and
//! projects them onto the spoke [`Rule`] wire type at this boundary — the
//! local-db row stays spoke-unaware (spec §8 dep-graph reversal):
//!
//! | Spoke `Rule` field       | `spoke_rules` column                  |
//! |--------------------------|---------------------------------------|
//! | `schema_version`         | `schema_version` (`i64` → `NonZeroU64`) |
//! | `rule_id`                | `rule_id`                             |
//! | `canonical_name`         | `canonical_name`                      |
//! | `kind`                   | `kind`                                |
//! | `statement`              | `statement`                           |
//! | `description`            | `description`                         |
//! | `target_entry_types`     | `target_entry_types_json`             |
//! | `severity_hint`          | `severity_hint`                       |
//! | `source_anchor`          | `source_anchor_json` (optional)       |
//! | `status`                 | `status`                              |
//! | `created_at` / `updated_at` | epoch-second `INTEGER`s ↔ `DateTime<Utc>` (optional) |
//! | `extensions`             | `extensions_json` (opaque bag)        |
//!
//! `world_id` has no wire target (spoke `list_rules` has no world parameter):
//! it stays a storage column for ownership/CRUD later. The adapter does **not**
//! inject a `nexus.world_id` mirror into `extensions` — the bag round-trips
//! verbatim (spoke round-trip preservation; the plan marks the mirror
//! optional, and injecting would fabricate data on the read path).
//!
//! # Semantics
//!
//! Unknown `rule_refs` are omitted — an empty subset, **not** an error (spoke
//! semantics); `list_rules(&[])` returns `Ok(vec![])`. Rows that cannot be
//! projected onto the wire (invalid `schema_version`/`canonical_name` or
//! unparseable JSON columns) are skipped like absent refs: `list_rules`
//! resolves a subset and never fails the whole list on one unreadable row.
//!
//! # Scope
//!
//! Read path only. Author-facing rule write/CRUD is **out of scope** for P1
//! (plan residuals); the only writer today is the test/dev seed helper
//! [`insert_spoke_rule_for_test`].

use super::NexusAdapter;
use crate::{Rule, RuleQueryPort, SpokeReject, SpokeRejectCode, SpokeResult};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use nexus_local_db::spoke_rules::{get_spoke_rules_by_ids, SpokeRuleRow};
use serde_json::{json, Map, Value};
use spoke_schemas::data::rule::{RuleCanonicalName, RuleExtensionsKey};
use std::collections::HashMap;
use std::num::NonZeroU64;

#[async_trait]
impl RuleQueryPort for NexusAdapter<'_> {
    async fn list_rules(&self, rule_refs: &[String]) -> SpokeResult<Vec<Rule>> {
        let pool = self.pool.clone();
        let rule_refs = rule_refs.to_vec();
        let rows = match get_spoke_rules_by_ids(&pool, &rule_refs).await {
            Ok(rows) => rows,
            Err(e) => {
                return reject(
                    SpokeRejectCode::InternalError,
                    format!("storage error on rule lookup: {e}"),
                    json!({}),
                );
            }
        };
        SpokeResult::Ok(rows.iter().filter_map(row_to_rule).collect())
    }
}

/// Project a persisted `spoke_rules` row onto the spoke [`Rule`] wire type.
///
/// Returns `None` for rows that cannot be represented on the wire — spoke
/// requires `schema_version >= 1` and a non-empty `canonical_name`, and the
/// JSON columns must parse — which the caller treats like an absent ref (see
/// module docs).
fn row_to_rule(row: &SpokeRuleRow) -> Option<Rule> {
    let schema_version = NonZeroU64::new(u64::try_from(row.schema_version).ok()?)?;
    let canonical_name = RuleCanonicalName::try_from(row.canonical_name.clone()).ok()?;
    let target_entry_types: Vec<String> =
        serde_json::from_str(&row.target_entry_types_json).ok()?;
    let extensions: HashMap<RuleExtensionsKey, Map<String, Value>> =
        serde_json::from_str(&row.extensions_json).ok()?;
    let source_anchor = row
        .source_anchor_json
        .as_deref()
        .map(serde_json::from_str)
        .transpose()
        .ok()?;
    Some(Rule {
        canonical_name,
        created_at: row
            .created_at
            .and_then(|ts| DateTime::<Utc>::from_timestamp(ts, 0)),
        description: row.description.clone(),
        extensions,
        kind: row.kind.clone(),
        rule_id: row.rule_id.clone(),
        schema_version,
        severity_hint: row.severity_hint.clone(),
        source_anchor,
        statement: row.statement.clone(),
        status: row.status.clone(),
        target_entry_types,
        updated_at: row
            .updated_at
            .and_then(|ts| DateTime::<Utc>::from_timestamp(ts, 0)),
    })
}

/// Construct a `SpokeResult::Reject` (mirrors the helper in
/// `knowledge_entry_port.rs` / `relation_port.rs`).
fn reject<T>(code: SpokeRejectCode, message: impl Into<String>, details: Value) -> SpokeResult<T> {
    let details_map = match details {
        Value::Object(map) => Some(map),
        other => {
            let mut map = Map::new();
            map.insert("detail".to_string(), other);
            Some(map)
        }
    };
    SpokeResult::Reject(SpokeReject {
        code,
        message: message.into(),
        details: details_map,
    })
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RuleQueryPort;
    use nexus_local_db::spoke_rules::{insert_spoke_rule_for_test, SpokeRuleRow};
    use nexus_local_db::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    /// Seed a `spoke_rules` row through the local-db test helper (the only
    /// writer in P1 — no author-facing rule write API exists).
    async fn seed_rule(
        pool: &sqlx::SqlitePool,
        rule_id: &str,
        world_id: &str,
        source_anchor_json: Option<&str>,
        extensions_json: &str,
    ) {
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
            source_anchor_json: source_anchor_json.map(String::from),
            extensions_json: extensions_json.to_string(),
            created_at: Some(1_700_000_000),
            updated_at: Some(1_700_000_100),
        };
        insert_spoke_rule_for_test(pool, &row).await.unwrap();
    }

    /// Test helper: unwrap a `SpokeResult::Ok` or panic with the reject payload.
    fn unwrap_ok<T>(result: SpokeResult<T>, label: &str) -> T {
        match result {
            SpokeResult::Ok(v) => v,
            SpokeResult::Reject(r) => panic!("{label}: expected ok, got reject {r:?}"),
        }
    }

    // ── list_rules ────────────────────────────────────────────────────

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_rules_returns_known_rules_and_omits_missing() {
        let (pool, _dir) = fresh_pool().await;
        seed_rule(&pool, "rule_a", "wld_1", None, "{}").await;
        seed_rule(&pool, "rule_b", "wld_1", None, "{}").await;

        let adapter = NexusAdapter::new(pool);
        let rules = unwrap_ok(
            adapter
                .list_rules(&[
                    "rule_a".to_string(),
                    "rule_missing".to_string(),
                    "rule_b".to_string(),
                ])
                .await,
            "list_rules",
        );

        // Known ids returned; the unknown ref is omitted (empty subset, not an
        // error — spoke semantics).
        assert_eq!(rules.len(), 2, "missing ref must be omitted");
        let mut ids: Vec<&str> = rules.iter().map(|r| r.rule_id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["rule_a", "rule_b"]);

        // Full field mapping on the projected wire `Rule`.
        let rule = rules.iter().find(|r| r.rule_id == "rule_a").unwrap();
        assert_eq!(rule.schema_version.get(), 1, "INTEGER column → NonZeroU64");
        assert_eq!(rule.canonical_name.to_string(), "Rule rule_a");
        assert_eq!(rule.kind, "rule");
        assert_eq!(rule.statement.as_deref(), Some("Statement for rule_a"));
        assert_eq!(rule.description.as_deref(), Some("Description for rule_a"));
        assert_eq!(rule.target_entry_types, vec!["character", "event"]);
        assert_eq!(rule.severity_hint.as_deref(), Some("warning"));
        assert_eq!(rule.status.as_deref(), Some("active"));
        assert!(rule.source_anchor.is_none());
        assert!(
            rule.extensions.is_empty(),
            "empty extensions bag stays empty"
        );
        assert_eq!(
            rule.created_at,
            DateTime::<Utc>::from_timestamp(1_700_000_000, 0),
            "epoch-second INTEGER → DateTime<Utc>"
        );
        assert_eq!(
            rule.updated_at,
            DateTime::<Utc>::from_timestamp(1_700_000_100, 0)
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_rules_round_trips_source_anchor_and_extensions_bag() {
        let (pool, _dir) = fresh_pool().await;
        seed_rule(
            &pool,
            "rule_rich",
            "wld_1",
            Some(r#"{"source_id": "src_01", "schema_version": 1, "extensions": {}}"#),
            r#"{"nexus": {"world_id": "wld_1", "audience": "reader"}}"#,
        )
        .await;

        let adapter = NexusAdapter::new(pool);
        let rules = unwrap_ok(
            adapter.list_rules(&["rule_rich".to_string()]).await,
            "list_rules",
        );
        assert_eq!(rules.len(), 1);
        let rule = &rules[0];

        // `source_anchor` deserialized from `source_anchor_json`.
        let anchor = rule.source_anchor.as_ref().expect("source_anchor present");
        assert_eq!(anchor.source_id, "src_01");
        assert_eq!(anchor.schema_version.get(), 1);

        // `extensions` bag round-trips verbatim (no fabrication, no drops).
        let ext = serde_json::to_value(&rule.extensions).unwrap();
        assert_eq!(
            ext,
            json!({"nexus": {"world_id": "wld_1", "audience": "reader"}})
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_rules_empty_refs_returns_empty_vec() {
        let (pool, _dir) = fresh_pool().await;
        seed_rule(&pool, "rule_a", "wld_1", None, "{}").await;

        let adapter = NexusAdapter::new(pool);
        let rules = unwrap_ok(adapter.list_rules(&[]).await, "list_rules empty");
        assert!(
            rules.is_empty(),
            "empty refs must return Ok(vec![]) without error"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_rules_all_missing_returns_empty_vec() {
        let (pool, _dir) = fresh_pool().await;
        seed_rule(&pool, "rule_a", "wld_1", None, "{}").await;

        let adapter = NexusAdapter::new(pool);
        let rules = unwrap_ok(
            adapter
                .list_rules(&["rule_nope".to_string(), "rule_nada".to_string()])
                .await,
            "list_rules all missing",
        );
        assert!(
            rules.is_empty(),
            "all-missing refs must return an empty subset, not an error"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn list_rules_on_dropped_table_surfaces_internal_error() {
        let (pool, _dir) = fresh_pool().await;
        seed_rule(&pool, "rule_a", "wld_1", None, "{}").await;
        sqlx::query("DROP TABLE spoke_rules")
            .execute(&pool)
            .await
            .unwrap();

        let adapter = NexusAdapter::new(pool);
        match adapter.list_rules(&["rule_a".to_string()]).await {
            SpokeResult::Reject(r) => {
                assert_eq!(
                    r.code,
                    SpokeRejectCode::InternalError,
                    "storage failure must surface INTERNAL_ERROR"
                );
            }
            SpokeResult::Ok(_) => panic!("expected InternalError reject"),
        }
    }
}
