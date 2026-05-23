//! Reference source repository — registry metadata in `SQLite` + `body.md` on disk.
//!
//! V1.26 reference store layout: registry row (metadata only) in `reference_sources`,
//! canonical body text in `~/.nexus42/creators/<creator_id>/references/units/<id>/body.md`.

use sqlx::SqlitePool;

use crate::error::LocalDbError;

/// Mutability policy for a reference source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SourceMutability {
    /// Body is fixed after registration (default).
    Static,
    /// Body may be refreshed by a future scan/import pipeline.
    Refreshable,
}

impl SourceMutability {
    /// Returns the string representation stored in the database.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Static => "static",
            Self::Refreshable => "refreshable",
        }
    }
}

/// Registry metadata for a reference source — mirrors the `reference_sources` DB row.
///
/// Does NOT contain the full body text. Body is stored on disk at `content_path`.
#[derive(Debug, Clone)]
pub struct ReferenceSourceRow {
    /// Registry primary key and disk unit directory name.
    pub reference_source_id: String,
    /// Workspace binding.
    pub workspace_id: String,
    /// Contract enum string (`file`, `url`, `pdf`, `note`).
    pub source_type: String,
    /// Mutability policy: `static` or `refreshable`.
    pub source_mutability: String,
    /// Logical locator URI.
    pub uri: String,
    /// Human-readable title.
    pub title: String,
    /// Serialized tag list.
    pub tags: Option<String>,
    /// Hash of canonical `body.md` when available.
    pub content_hash: Option<String>,
    /// Relative path from Creator root to canonical `body.md`.
    pub content_path: Option<String>,
    /// Scan lifecycle status.
    pub scan_status: String,
    /// Creation timestamp.
    pub created_at: String,
    /// Last registry update timestamp.
    pub updated_at: Option<String>,
}

/// Parameters for registering a new reference source.
pub struct RegisterParams<'a> {
    /// User home directory (for path helpers).
    pub home: &'a std::path::Path,
    /// Active creator ID.
    pub creator_id: &'a str,
    /// Workspace binding.
    pub workspace_id: &'a str,
    /// Contract enum string (`file`, `url`, `pdf`, `note`).
    pub source_type: &'a str,
    /// Mutability policy.
    pub source_mutability: SourceMutability,
    /// Logical locator URI.
    pub uri: &'a str,
    /// Human-readable title.
    pub title: &'a str,
    /// Serialized tag list (optional).
    pub tags: Option<&'a str>,
    /// Canonical body text.
    pub body: &'a str,
}

/// Register a new reference source: creates directories + `body.md`, writes metadata to `SQLite`.
///
/// The `params.home` parameter is the user's home directory (used with `nexus-home-layout` helpers).
/// The `params.creator_id` identifies the active creator under whose root the body file is stored.
/// The `params.body` parameter is the canonical body text to write to `body.md`.
///
/// # Errors
///
/// Returns `LocalDbError` if:
/// - The database insert fails
/// - The body file or directories cannot be created
pub async fn register(
    pool: &SqlitePool,
    params: RegisterParams<'_>,
) -> Result<ReferenceSourceRow, LocalDbError> {
    let reference_source_id = format!("ref_{}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now().to_rfc3339();
    let mutability_str = params.source_mutability.as_str();

    // Relative path from Creator root
    let content_path = format!("references/units/{reference_source_id}/body.md");

    // Absolute path for body.md on disk
    let body_abs = nexus_home_layout::reference_body_path(
        params.home,
        params.creator_id,
        &reference_source_id,
    );

    // Create unit directory and write body.md
    if let Some(parent) = body_abs.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| LocalDbError::Io {
                path: parent.display().to_string(),
                source: e,
            })?;
    }
    tokio::fs::write(&body_abs, params.body)
        .await
        .map_err(|e| LocalDbError::Io {
            path: body_abs.display().to_string(),
            source: e,
        })?;

    // Insert metadata into SQLite
    let row = sqlx::query!(
        r#"INSERT INTO reference_sources
            (reference_source_id, workspace_id, source_type, source_mutability, uri, title, tags, content_hash, content_path, content, scan_status, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, NULL, ?, NULL, 'pending', ?, NULL)
           RETURNING
             reference_source_id as "reference_source_id!",
             workspace_id as "workspace_id!",
             source_type as "source_type!",
             source_mutability as "source_mutability!",
             uri as "uri!",
             title as "title!",
             tags,
             content_hash,
             content_path,
             scan_status as "scan_status!",
             created_at as "created_at!",
             updated_at"#,
        reference_source_id,
        params.workspace_id,
        params.source_type,
        mutability_str,
        params.uri,
        params.title,
        params.tags,
        content_path,
        now,
    )
    .fetch_one(pool)
    .await?;

    Ok(ReferenceSourceRow {
        reference_source_id: row.reference_source_id,
        workspace_id: row.workspace_id,
        source_type: row.source_type,
        source_mutability: row.source_mutability,
        uri: row.uri,
        title: row.title,
        tags: row.tags,
        content_hash: row.content_hash,
        content_path: row.content_path,
        scan_status: row.scan_status,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

/// List all reference sources — returns registry metadata WITHOUT loading body content.
///
/// Ordered by `created_at` descending (newest first).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list(pool: &SqlitePool) -> Result<Vec<ReferenceSourceRow>, LocalDbError> {
    let rows = sqlx::query!(
        r#"SELECT
             reference_source_id as "reference_source_id!",
             workspace_id as "workspace_id!",
             source_type as "source_type!",
             source_mutability as "source_mutability!",
             uri as "uri!",
             title as "title!",
             tags,
             content_hash,
             content_path,
             scan_status as "scan_status!",
             created_at as "created_at!",
             updated_at
           FROM reference_sources ORDER BY created_at DESC"#
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ReferenceSourceRow {
            reference_source_id: r.reference_source_id,
            workspace_id: r.workspace_id,
            source_type: r.source_type,
            source_mutability: r.source_mutability,
            uri: r.uri,
            title: r.title,
            tags: r.tags,
            content_hash: r.content_hash,
            content_path: r.content_path,
            scan_status: r.scan_status,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })
        .collect())
}

/// Get a single reference source by ID — returns registry metadata only (no body content).
///
/// Returns `None` if the record doesn't exist.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_by_id(
    pool: &SqlitePool,
    reference_source_id: &str,
) -> Result<Option<ReferenceSourceRow>, LocalDbError> {
    let row = sqlx::query!(
        r#"SELECT
             reference_source_id as "reference_source_id!",
             workspace_id as "workspace_id!",
             source_type as "source_type!",
             source_mutability as "source_mutability!",
             uri as "uri!",
             title as "title!",
             tags,
             content_hash,
             content_path,
             scan_status as "scan_status!",
             created_at as "created_at!",
             updated_at
           FROM reference_sources WHERE reference_source_id = ?"#,
        reference_source_id
    )
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| ReferenceSourceRow {
        reference_source_id: r.reference_source_id,
        workspace_id: r.workspace_id,
        source_type: r.source_type,
        source_mutability: r.source_mutability,
        uri: r.uri,
        title: r.title,
        tags: r.tags,
        content_hash: r.content_hash,
        content_path: r.content_path,
        scan_status: r.scan_status,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    async fn fresh_pool() -> (SqlitePool, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = crate::open_pool(&db_path).await.unwrap();
        crate::run_migrations(&pool).await.unwrap();
        (pool, dir)
    }

    #[tokio::test]
    async fn test_register_and_get() {
        let (pool, dir) = fresh_pool().await;
        let home = dir.path();

        let row = register(
            &pool,
            RegisterParams {
                home,
                creator_id: "ctr_test",
                workspace_id: "wrk_default",
                source_type: "note",
                source_mutability: SourceMutability::Static,
                uri: "nexus42://references/units/test",
                title: "Test Reference",
                tags: None,
                body: "Body text here",
            },
        )
        .await
        .unwrap();

        assert!(row.reference_source_id.starts_with("ref_"));
        assert_eq!(row.workspace_id, "wrk_default");
        assert_eq!(row.source_type, "note");
        assert_eq!(row.source_mutability, "static");
        assert_eq!(row.scan_status, "pending");
        assert!(row.content_path.is_some());
        assert!(row.updated_at.is_none());

        // Body file exists on disk
        let body_path =
            nexus_home_layout::reference_body_path(home, "ctr_test", &row.reference_source_id);
        let body_content = tokio::fs::read_to_string(&body_path).await.unwrap();
        assert_eq!(body_content, "Body text here");

        // Get by ID
        let fetched = get_by_id(&pool, &row.reference_source_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.reference_source_id, row.reference_source_id);
        assert_eq!(fetched.title, "Test Reference");
    }

    #[tokio::test]
    async fn test_list() {
        let (pool, dir) = fresh_pool().await;
        let home = dir.path();

        register(
            &pool,
            RegisterParams {
                home,
                creator_id: "ctr_test",
                workspace_id: "wrk_default",
                source_type: "note",
                source_mutability: SourceMutability::Static,
                uri: "nexus42://references/units/a",
                title: "Ref A",
                tags: None,
                body: "Body A",
            },
        )
        .await
        .unwrap();

        register(
            &pool,
            RegisterParams {
                home,
                creator_id: "ctr_test",
                workspace_id: "wrk_default",
                source_type: "url",
                source_mutability: SourceMutability::Refreshable,
                uri: "https://example.com",
                title: "Ref B",
                tags: Some("research,tutorial"),
                body: "Body B",
            },
        )
        .await
        .unwrap();

        let all = list(&pool).await.unwrap();
        assert_eq!(all.len(), 2);

        // Ordered by created_at DESC — newest first
        assert_eq!(all[0].title, "Ref B");
        assert_eq!(all[0].source_mutability, "refreshable");
        assert_eq!(all[0].tags.as_deref(), Some("research,tutorial"));

        assert_eq!(all[1].title, "Ref A");
        assert_eq!(all[1].source_mutability, "static");
    }

    #[tokio::test]
    async fn test_get_by_id_not_found() {
        let (pool, _dir) = fresh_pool().await;
        assert!(get_by_id(&pool, "ref_ghost").await.unwrap().is_none());
    }
}
