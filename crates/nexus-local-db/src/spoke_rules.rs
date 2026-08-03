//! Spoke `Rule` storage (V1.148 P1) — read primitives for the `spoke_rules`
//! table.
//!
//! Pure storage: this module knows nothing about spoke wire types (spec §8
//! dep-graph reversal — `nexus-local-db` has no `spoke-schemas` dependency).
//! JSON-shaped columns (`target_entry_types_json`, `source_anchor_json`,
//! `extensions_json`) are carried as opaque strings and parsed at the
//! `nexus-spoke-adapter` boundary.
//!
//! P1 scope is deliberately narrow: read-by-ids + a test/dev seed helper.
//! Author-facing rule write/CRUD is out of scope (plan
//! `2026-08-04-v1.148-p1-rule-query-port-production` residuals).

use crate::LocalDbError;
use sqlx::SqlitePool;

/// Row type matching the `spoke_rules` DDL (`20260804_000001_spoke_rules.sql`).
///
/// `schema_version` is stored as a plain `i64` (`0` is invalid — spoke wire
/// requires `NonZeroU64`); `created_at` / `updated_at` are Unix epoch seconds
/// (`NULL` when unknown).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SpokeRuleRow {
    /// Globally unique rule id (spoke `list_rules` looks up by `rule_id` only).
    pub rule_id: String,
    /// Owning world — ownership/CRUD later; not part of the spoke `Rule` wire.
    pub world_id: String,
    pub schema_version: i64,
    pub canonical_name: String,
    pub kind: String,
    pub statement: Option<String>,
    pub description: Option<String>,
    /// JSON array of entry-type strings (`spoke Rule.target_entry_types`).
    pub target_entry_types_json: String,
    pub severity_hint: Option<String>,
    pub status: Option<String>,
    /// JSON object of the optional spoke `SourceAnchor` (`NULL` when absent).
    pub source_anchor_json: Option<String>,
    /// JSON object: the product namespace bag (`spoke Rule.extensions`).
    pub extensions_json: String,
    /// Unix epoch seconds; `NULL` when unknown.
    pub created_at: Option<i64>,
    /// Unix epoch seconds; `NULL` when unknown.
    pub updated_at: Option<i64>,
}

/// Fetch the rows whose `rule_id` is in `rule_ids`.
///
/// Unknown ids are silently omitted (the caller resolves a subset — spoke
/// `list_rules` semantics); duplicate ids in the input yield one row each.
/// Returns `Ok(vec![])` for an empty `rule_ids` without touching the DB.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn get_spoke_rules_by_ids(
    pool: &SqlitePool,
    rule_ids: &[String],
) -> Result<Vec<SpokeRuleRow>, LocalDbError> {
    if rule_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids_json = serde_json::to_string(rule_ids).unwrap_or_else(|_| "[]".to_string());
    // SAFETY: static SQL; the variable-length IN list is delegated to SQLite's
    // `json_each` with a single JSON-array bind (same idiom as
    // `narrative_gateway::list_timeline_events_scoped` and
    // `kb_store::list_by_world_scoped`). No user-controlled SQL fragments.
    let rows = sqlx::query_as::<_, SpokeRuleRow>(
        "SELECT rule_id, world_id, schema_version, canonical_name, kind, statement, \
         description, target_entry_types_json, severity_hint, status, source_anchor_json, \
         extensions_json, created_at, updated_at \
         FROM spoke_rules \
         WHERE rule_id IN (SELECT value FROM json_each(?))",
    )
    .bind(ids_json)
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

/// Insert a `spoke_rules` row (test/dev seed helper — P1 has no production
/// write path).
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure (including duplicate
/// `rule_id` primary key).
pub async fn insert_spoke_rule_for_test(
    pool: &SqlitePool,
    row: &SpokeRuleRow,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        r#"INSERT INTO spoke_rules
           (rule_id, world_id, schema_version, canonical_name, kind, statement,
            description, target_entry_types_json, severity_hint, status,
            source_anchor_json, extensions_json, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        row.rule_id,
        row.world_id,
        row.schema_version,
        row.canonical_name,
        row.kind,
        row.statement,
        row.description,
        row.target_entry_types_json,
        row.severity_hint,
        row.status,
        row.source_anchor_json,
        row.extensions_json,
        row.created_at,
        row.updated_at,
    )
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{open_pool, run_migrations};

    async fn fresh_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = open_pool(&db_path).await.unwrap();
        run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    fn rule_row(rule_id: &str, world_id: &str) -> SpokeRuleRow {
        SpokeRuleRow {
            rule_id: rule_id.to_string(),
            world_id: world_id.to_string(),
            schema_version: 1,
            canonical_name: format!("Rule {rule_id}"),
            kind: "rule".to_string(),
            statement: Some(format!("Statement for {rule_id}")),
            description: None,
            target_entry_types_json: "[\"character\", \"event\"]".to_string(),
            severity_hint: Some("warning".to_string()),
            status: Some("active".to_string()),
            source_anchor_json: None,
            extensions_json: "{}".to_string(),
            created_at: Some(1_700_000_000),
            updated_at: None,
        }
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_by_ids_returns_known_subset_and_omits_missing() {
        let (pool, _dir) = fresh_pool().await;
        insert_spoke_rule_for_test(&pool, &rule_row("rule_a", "wld_1"))
            .await
            .unwrap();
        insert_spoke_rule_for_test(&pool, &rule_row("rule_b", "wld_1"))
            .await
            .unwrap();
        insert_spoke_rule_for_test(&pool, &rule_row("rule_c", "wld_2"))
            .await
            .unwrap();

        // Known ids returned in any order; the missing id is omitted.
        let refs = [
            "rule_a".to_string(),
            "rule_missing".to_string(),
            "rule_b".to_string(),
        ];
        let rows = get_spoke_rules_by_ids(&pool, &refs).await.unwrap();
        assert_eq!(rows.len(), 2, "only known rule ids are returned");
        let mut ids: Vec<&str> = rows.iter().map(|r| r.rule_id.as_str()).collect();
        ids.sort_unstable();
        assert_eq!(ids, vec!["rule_a", "rule_b"]);

        // Column values round-trip.
        let row = rows.iter().find(|r| r.rule_id == "rule_a").unwrap();
        assert_eq!(row.world_id, "wld_1");
        assert_eq!(row.schema_version, 1);
        assert_eq!(row.canonical_name, "Rule rule_a");
        assert_eq!(row.kind, "rule");
        assert_eq!(row.statement.as_deref(), Some("Statement for rule_a"));
        assert!(row.description.is_none());
        assert_eq!(row.target_entry_types_json, "[\"character\", \"event\"]");
        assert_eq!(row.severity_hint.as_deref(), Some("warning"));
        assert_eq!(row.status.as_deref(), Some("active"));
        assert!(row.source_anchor_json.is_none());
        assert_eq!(row.extensions_json, "{}");
        assert_eq!(row.created_at, Some(1_700_000_000));
        assert!(row.updated_at.is_none());
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_by_ids_empty_refs_returns_empty() {
        let (pool, _dir) = fresh_pool().await;
        insert_spoke_rule_for_test(&pool, &rule_row("rule_a", "wld_1"))
            .await
            .unwrap();

        let rows: Vec<SpokeRuleRow> = get_spoke_rules_by_ids(&pool, &[]).await.unwrap();
        assert!(rows.is_empty(), "empty refs must return an empty vec");
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn get_by_ids_all_missing_returns_empty() {
        let (pool, _dir) = fresh_pool().await;
        let rows =
            get_spoke_rules_by_ids(&pool, &["rule_nope".to_string(), "rule_nada".to_string()])
                .await
                .unwrap();
        assert!(rows.is_empty(), "all-missing refs must return an empty vec");
    }
}
