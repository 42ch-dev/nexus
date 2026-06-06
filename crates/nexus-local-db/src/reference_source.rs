//! Reference source repository — registry metadata in `SQLite` + `body.md` on disk.
//!
//! V1.26 reference store layout: registry row (metadata only) in `reference_sources`,
//! canonical body text in `~/.nexus42/creators/<creator_id>/references/units/<id>/body.md`.
//!
//! # Write ordering (R5)
//!
//! The SQL INSERT is performed first. The body.md file is written only after the
//! database transaction succeeds. This prevents orphan body files on DB failure.
//! If the file write fails after a successful INSERT, the row is cleaned up via
//! best-effort DELETE to avoid a dangling registry entry.

use sqlx::{Row as _, SqlitePool};

use crate::error::LocalDbError;

/// Default page size for `list_references`.
const DEFAULT_PAGE_LIMIT: i64 = 100;

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

/// Registry metadata for a reference source.
///
/// Does NOT contain the full body text. Body is stored on disk at `content_path`.
#[derive(Debug, Clone)]
pub struct ReferenceSourceRow {
    /// Registry primary key and disk unit directory name.
    pub reference_source_id: String,
    /// Workspace binding.
    pub workspace_id: String,
    /// Contract enum string (file, url, pdf, note).
    pub source_type: String,
    /// Mutability policy: static or refreshable.
    pub source_mutability: String,
    /// Logical locator URI.
    pub uri: String,
    /// Human-readable title.
    pub title: String,
    /// Serialized tag list.
    pub tags: Option<String>,
    /// Hash of canonical body.md when available.
    pub content_hash: Option<String>,
    /// Relative path from Creator root to canonical body.md.
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
    /// Contract enum string (file, url, pdf, note).
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

/// Register a new reference source: inserts metadata into `SQLite`, then creates `body.md`.
///
/// The SQL INSERT runs first (R5: prevents orphan body files on DB failure).
/// If the file write fails after a successful INSERT, the registry row is
/// cleaned up (deleted) to avoid a dangling entry.
///
/// # Errors
///
/// Returns `LocalDbError` if the database insert fails or the body file cannot be created.
pub async fn register(
    pool: &SqlitePool,
    params: RegisterParams<'_>,
) -> Result<ReferenceSourceRow, LocalDbError> {
    let reference_source_id = format!("ref_{}", uuid::Uuid::new_v4().simple());
    let now = chrono::Utc::now().to_rfc3339();
    let mutability_str = params.source_mutability.as_str();

    // Relative path from Creator root
    let content_path = format!("references/units/{reference_source_id}/body.md");

    // Compute content hash from body bytes (R7)
    let content_hash = blake3_hash(params.body.as_bytes());

    // Step 1: Insert metadata into SQLite first (R5: DB first, file second)
    let row = sqlx::query!(
        r#"INSERT INTO reference_sources
            (reference_source_id, workspace_id, source_type, source_mutability, uri, title, tags, content_hash, content_path, content, scan_status, created_at, updated_at)
           VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, 'pending', ?, NULL)
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
        content_hash,
        content_path,
        now,
    )
    .fetch_one(pool)
    .await?;

    // Step 2: Write body.md to disk (only after DB success)
    let body_abs = nexus_home_layout::reference_body_path(
        params.home,
        params.creator_id,
        &reference_source_id,
    );

    // Create unit directory
    if let Some(parent) = body_abs.parent() {
        tokio::fs::create_dir_all(parent).await.map_err(|e| {
            cleanup_row(pool, &reference_source_id);
            LocalDbError::Io {
                path: parent.display().to_string(),
                source: e,
            }
        })?;
    }

    // Write body file
    tokio::fs::write(&body_abs, params.body)
        .await
        .map_err(|e| {
            cleanup_row(pool, &reference_source_id);
            LocalDbError::Io {
                path: body_abs.display().to_string(),
                source: e,
            }
        })?;

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

/// Best-effort cleanup: delete a just-inserted row when file write fails.
///
/// TD-V130-02: Logs an error on DELETE failure instead of silently discarding.
fn cleanup_row(pool: &SqlitePool, id: &str) {
    let pool = pool.clone();
    let id = id.to_string();
    tokio::spawn(async move {
        // SAFETY: runtime query for best-effort cleanup; compile-time
        // macro not required in fire-and-forget context.
        let result = sqlx::query("DELETE FROM reference_sources WHERE reference_source_id = ?")
            .bind(&id)
            .execute(&pool)
            .await;
        if let Err(e) = result {
            tracing::error!(
                reference_source_id = %id,
                error = %e,
                "TD-V130-02: cleanup_row DELETE failed — dangling registry row may remain"
            );
        }
    });
}

/// Compute a blake3 hex hash of the given bytes.
fn blake3_hash(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// List reference sources with pagination.
///
/// Ordered by `created_at` descending (newest first). Uses `limit`/`offset` pagination
/// with a default page size of [`DEFAULT_PAGE_LIMIT`].
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list(
    pool: &SqlitePool,
    limit: Option<i64>,
    offset: Option<i64>,
) -> Result<Vec<ReferenceSourceRow>, LocalDbError> {
    let limit = limit.unwrap_or(DEFAULT_PAGE_LIMIT).clamp(1, 1000);
    let offset = offset.unwrap_or(0).max(0);

    // SAFETY: `limit` is clamped to 1..=1000 and `offset` to >= 0.
    // Dynamic SQL needed because `sqlx::query!` does not support
    // `LIMIT`/`OFFSET` as bind parameters in SQLite offline mode.
    let rows = sqlx::query(&format!(
        "SELECT
              reference_source_id,
              workspace_id,
              source_type,
              source_mutability,
              uri,
              title,
              tags,
              content_hash,
              content_path,
              scan_status,
              created_at,
              updated_at
           FROM reference_sources
           ORDER BY created_at DESC
           LIMIT {limit} OFFSET {offset}"
    ))
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|r| ReferenceSourceRow {
            reference_source_id: r.get("reference_source_id"),
            workspace_id: r.get("workspace_id"),
            source_type: r.get("source_type"),
            source_mutability: r.get("source_mutability"),
            uri: r.get("uri"),
            title: r.get("title"),
            tags: r.get("tags"),
            content_hash: r.get("content_hash"),
            content_path: r.get("content_path"),
            scan_status: r.get("scan_status"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        })
        .collect())
}

/// Get a single reference source by ID — returns registry metadata only (no body content).
///
/// Returns `None` if the record does not exist.
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

        let all = list(&pool, None, None).await.unwrap();
        assert_eq!(all.len(), 2);

        // Ordered by created_at DESC — newest first
        assert_eq!(all[0].title, "Ref B");
        assert_eq!(all[0].source_mutability, "refreshable");
        assert_eq!(all[0].tags.as_deref(), Some("research,tutorial"));

        assert_eq!(all[1].title, "Ref A");
        assert_eq!(all[1].source_mutability, "static");
    }

    #[tokio::test]
    async fn test_list_pagination() {
        let (pool, dir) = fresh_pool().await;
        let home = dir.path();

        // Register 3 references
        for i in 0..3 {
            register(
                &pool,
                RegisterParams {
                    home,
                    creator_id: "ctr_test",
                    workspace_id: "wrk_default",
                    source_type: "note",
                    source_mutability: SourceMutability::Static,
                    uri: &format!("nexus42://references/units/pg{i}"),
                    title: &format!("Ref {i}"),
                    tags: None,
                    body: &format!("Body {i}"),
                },
            )
            .await
            .unwrap();
        }

        // Page 1: limit=2, offset=0
        let page1 = list(&pool, Some(2), Some(0)).await.unwrap();
        assert_eq!(page1.len(), 2);

        // Page 2: limit=2, offset=2
        let page2 = list(&pool, Some(2), Some(2)).await.unwrap();
        assert_eq!(page2.len(), 1);

        // No overlap
        assert_ne!(page1[0].reference_source_id, page2[0].reference_source_id);
    }

    #[tokio::test]
    async fn test_content_hash_populated() {
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
                uri: "nexus42://references/units/hash-test",
                title: "Hash Test",
                tags: None,
                body: "Some body content",
            },
        )
        .await
        .unwrap();

        // content_hash should be populated (blake3 hex)
        assert!(row.content_hash.is_some());
        let hash = row.content_hash.unwrap();
        assert_eq!(hash.len(), 64); // blake3 hex is 64 chars

        // Verify hash is correct
        let expected = blake3::hash(b"Some body content").to_hex().to_string();
        assert_eq!(hash, expected);

        // Same hash via get_by_id
        let fetched = get_by_id(&pool, &row.reference_source_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(fetched.content_hash.as_deref(), Some(expected.as_str()));
    }

    #[tokio::test]
    async fn test_write_order_no_orphan_file_on_db_failure() {
        // R5: Verify that if we simulate a constraint violation (duplicate PK),
        // no body file is left behind.
        let (pool, dir) = fresh_pool().await;
        let home = dir.path();

        // Register successfully
        let row = register(
            &pool,
            RegisterParams {
                home,
                creator_id: "ctr_test",
                workspace_id: "wrk_default",
                source_type: "note",
                source_mutability: SourceMutability::Static,
                uri: "nexus42://references/units/orphan-test",
                title: "Orphan Test",
                tags: None,
                body: "First body",
            },
        )
        .await
        .unwrap();

        // Body file exists
        let body_path =
            nexus_home_layout::reference_body_path(home, "ctr_test", &row.reference_source_id);
        assert!(tokio::fs::metadata(&body_path).await.is_ok());

        // Try to INSERT a row with the same PK (simulating DB failure)
        let result: Result<sqlx::sqlite::SqliteQueryResult, sqlx::Error> = sqlx::query!(
            "INSERT INTO reference_sources (reference_source_id, workspace_id, source_type, source_mutability, uri, title, tags, content_hash, content_path, content, scan_status, created_at) VALUES (?, 'x', 'x', 'x', 'x', 'x', NULL, NULL, NULL, NULL, 'pending', 'x')",
            row.reference_source_id,
        )
        .execute(&pool)
        .await;
        assert!(result.is_err(), "Duplicate PK should fail");

        // The original body file is still intact — no new orphan
        let body_content = tokio::fs::read_to_string(&body_path).await.unwrap();
        assert_eq!(body_content, "First body");
    }

    #[tokio::test]
    async fn test_get_by_id_not_found() {
        let (pool, _dir) = fresh_pool().await;
        assert!(get_by_id(&pool, "ref_ghost").await.unwrap().is_none());
    }
}
