use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};

use crate::FixtureError;

/// The fixture-owned schema, embedded at compile time.
static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

/// File-backed SQLite store owned outright by the fixture provider.
pub struct FixtureStore {
    pool: SqlitePool,
}

impl FixtureStore {
    /// Open (creating when absent) the scenario database file, then apply the
    /// fixture migrations. Reopening the same file is a no-op migration pass
    /// over the rows already stored in it.
    pub(crate) async fn open(database_path: &Path) -> Result<Self, FixtureError> {
        let options = SqliteConnectOptions::new()
            .filename(database_path)
            .create_if_missing(true)
            // Two admitted clients write the same space, so readers must not
            // block the writer for the duration of a page read.
            .journal_mode(SqliteJournalMode::Wal);
        let pool = SqlitePoolOptions::new()
            .max_connections(5)
            .connect_with(options)
            .await?;
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }

    /// The pool every fixture operation runs its SQL against.
    pub(crate) const fn pool(&self) -> &SqlitePool {
        &self.pool
    }

    /// Close every connection. The next open re-reads the same file.
    pub(crate) async fn close(&self) {
        self.pool.close().await;
    }
}

// ---------------------------------------------------------------------------
// Reflection rows (§7.25/§7.26): immutable, never deleted.
// ---------------------------------------------------------------------------

/// One stored `reflections` row. Every pinned protocol column is kept verbatim:
/// absent optional filters stay `None` (never substituted with a null value),
/// and the published array shapes inside the JSON payloads are preserved
/// byte-for-byte because they are stored and returned as opaque payloads.
#[derive(Debug, Clone)]
pub(crate) struct ReflectionRow {
    pub reflection_id: String,
    pub space_id: String,
    pub subject_id: String,
    pub requester_principal_id: String,
    pub selection_json: String,
    pub filter_json: Option<String>,
    pub cognition_filter_json: Option<String>,
    pub basis_json: String,
    pub body_text: String,
    pub body_format: String,
    pub cost_unit_kind: Option<String>,
    pub cost_units: Option<i64>,
    pub engine_policy_version: String,
    pub model: Option<String>,
    pub schema_version: i64,
    pub revision: i64,
    pub extensions_json: String,
    pub created_at: String,
}

impl ReflectionRow {
    fn from_row(row: &sqlx::sqlite::SqliteRow) -> Result<Self, FixtureError> {
        Ok(Self {
            reflection_id: row.try_get("reflection_id")?,
            space_id: row.try_get("space_id")?,
            subject_id: row.try_get("subject_id")?,
            requester_principal_id: row.try_get("requester_principal_id")?,
            selection_json: row.try_get("selection_json")?,
            filter_json: row.try_get("filter_json")?,
            cognition_filter_json: row.try_get("cognition_filter_json")?,
            basis_json: row.try_get("basis_json")?,
            body_text: row.try_get("body_text")?,
            body_format: row.try_get("body_format")?,
            cost_unit_kind: row.try_get("cost_unit_kind")?,
            cost_units: row.try_get("cost_units")?,
            engine_policy_version: row.try_get("engine_policy_version")?,
            model: row.try_get("model")?,
            schema_version: row.try_get("schema_version")?,
            revision: row.try_get("revision")?,
            extensions_json: row.try_get("extensions_json")?,
            created_at: row.try_get("created_at")?,
        })
    }

    /// Insert one immutable reflection row. There is no update or delete path:
    /// a duplicate id is a storage error, not an overwrite.
    pub(crate) async fn insert(&self, pool: &SqlitePool) -> Result<(), FixtureError> {
        sqlx::query(
            "INSERT INTO reflections (reflection_id, space_id, subject_id, \
             requester_principal_id, selection_json, filter_json, cognition_filter_json, \
             basis_json, body_text, body_format, cost_unit_kind, cost_units, \
             engine_policy_version, model, schema_version, revision, extensions_json, created_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&self.reflection_id)
        .bind(&self.space_id)
        .bind(&self.subject_id)
        .bind(&self.requester_principal_id)
        .bind(&self.selection_json)
        .bind(&self.filter_json)
        .bind(&self.cognition_filter_json)
        .bind(&self.basis_json)
        .bind(&self.body_text)
        .bind(&self.body_format)
        .bind(&self.cost_unit_kind)
        .bind(&self.cost_units)
        .bind(&self.engine_policy_version)
        .bind(&self.model)
        .bind(self.schema_version)
        .bind(self.revision)
        .bind(&self.extensions_json)
        .bind(&self.created_at)
        .execute(pool)
        .await?;
        Ok(())
    }

    /// Look one reflection up within its subject.
    pub(crate) async fn load(
        pool: &SqlitePool,
        space_id: &str,
        subject_id: &str,
        reflection_id: &str,
    ) -> Result<Option<Self>, FixtureError> {
        let row = sqlx::query(
            "SELECT reflection_id, space_id, subject_id, requester_principal_id, \
             selection_json, filter_json, cognition_filter_json, basis_json, body_text, \
             body_format, cost_unit_kind, cost_units, engine_policy_version, model, \
             schema_version, revision, extensions_json, created_at FROM reflections \
             WHERE space_id = ? AND subject_id = ? AND reflection_id = ?",
        )
        .bind(space_id)
        .bind(subject_id)
        .bind(reflection_id)
        .fetch_optional(pool)
        .await?;
        row.map(|row| Self::from_row(&row)).transpose()
    }

    /// Page one subject's reflections oldest-first, `limit` rows strictly
    /// after `after_created_at`/`after_id` when a cursor tail is given.
    pub(crate) async fn load_page(
        pool: &SqlitePool,
        space_id: &str,
        subject_id: &str,
        after_created_at: Option<&str>,
        after_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Self>, FixtureError> {
        let rows = match (after_created_at, after_id) {
            (Some(after_created_at), Some(after_id)) => sqlx::query(
                "SELECT reflection_id, space_id, subject_id, requester_principal_id, \
                 selection_json, filter_json, cognition_filter_json, basis_json, body_text, \
                 body_format, cost_unit_kind, cost_units, engine_policy_version, model, \
                 schema_version, revision, extensions_json, created_at FROM reflections \
                 WHERE space_id = ? AND subject_id = ? \
                 AND (created_at, reflection_id) > (?, ?) \
                 ORDER BY created_at, reflection_id LIMIT ?",
            )
            .bind(space_id)
            .bind(subject_id)
            .bind(after_created_at)
            .bind(after_id)
            .bind(limit)
            .fetch_all(pool)
            .await?,
            _ => {
                sqlx::query(
                    "SELECT reflection_id, space_id, subject_id, requester_principal_id, \
                     selection_json, filter_json, cognition_filter_json, basis_json, body_text, \
                     body_format, cost_unit_kind, cost_units, engine_policy_version, model, \
                     schema_version, revision, extensions_json, created_at FROM reflections \
                     WHERE space_id = ? AND subject_id = ? \
                     ORDER BY created_at, reflection_id LIMIT ?",
                )
                .bind(space_id)
                .bind(subject_id)
                .bind(limit)
                .fetch_all(pool)
                .await?
            }
        };
        rows.iter().map(Self::from_row).collect()
    }
}
