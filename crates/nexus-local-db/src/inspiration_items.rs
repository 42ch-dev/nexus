//! Inspiration items DAO (DF-61 selection pool).
//!
//! Manages the `inspiration_items` table — creator-scoped inspiration
//! items with optional markdown file scaffold under `{workspace}/Pool/Ideas/`.
//!
//! Spec: novel-work-pool.md §3, local-db-schema.md §4.1.5.

use sqlx::{Row, SqlitePool};

use crate::error::LocalDbError;

/// Column list for all SELECT queries on `inspiration_items`.
pub const INSPIRATION_COLUMNS: &str = "\
    item_id, creator_id, rel_path, title, status, promoted_work_id, created_at, promoted_at";

/// Inspiration item record — mirrors DB row.
#[derive(Debug, Clone, serde::Serialize)]
pub struct InspirationItem {
    /// Unique identifier (`npi_` prefix).
    pub item_id: String,
    /// Owning creator.
    pub creator_id: String,
    /// Relative path to markdown file under workspace root.
    pub rel_path: String,
    /// Display title.
    pub title: String,
    /// Status: `idea` | `promoted` | `archived`.
    pub status: String,
    /// Work ID assigned on promotion.
    pub promoted_work_id: Option<String>,
    /// Creation timestamp (ISO-8601).
    pub created_at: String,
    /// Promotion timestamp (ISO-8601).
    pub promoted_at: Option<String>,
}

/// Map a sqlx row to [`InspirationItem`].
#[must_use]
pub fn row_to_inspiration_item(r: &sqlx::sqlite::SqliteRow) -> InspirationItem {
    InspirationItem {
        item_id: r.get("item_id"),
        creator_id: r.get("creator_id"),
        rel_path: r.get("rel_path"),
        title: r.get("title"),
        status: r.get("status"),
        promoted_work_id: r.get("promoted_work_id"),
        created_at: r.get("created_at"),
        promoted_at: r.get("promoted_at"),
    }
}

/// List inspiration items for a creator, optionally filtered by status.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_inspiration(
    pool: &SqlitePool,
    creator_id: &str,
    status_filter: Option<&str>,
) -> Result<Vec<InspirationItem>, LocalDbError> {
    let sql = if status_filter.is_some() {
        format!(
            "SELECT {INSPIRATION_COLUMNS} FROM inspiration_items \
             WHERE creator_id = ? AND status = ? \
             ORDER BY created_at DESC"
        )
    } else {
        format!(
            "SELECT {INSPIRATION_COLUMNS} FROM inspiration_items \
             WHERE creator_id = ? \
             ORDER BY created_at DESC"
        )
    };

    let mut query = sqlx::query(&sql).bind(creator_id);
    if let Some(s) = status_filter {
        query = query.bind(s);
    }

    let rows = query.fetch_all(pool).await?;
    Ok(rows.iter().map(row_to_inspiration_item).collect())
}

/// Create a new inspiration item — inserts DB row only.
///
/// The caller is responsible for creating the markdown file scaffold atomically.
/// Use [`create_inspiration_with_scaffold`] for the atomic version that also
/// writes the MD file.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or a unique constraint is violated.
pub async fn create_inspiration_row(
    pool: &SqlitePool,
    item_id: &str,
    creator_id: &str,
    rel_path: &str,
    title: &str,
    created_at: &str,
) -> Result<InspirationItem, LocalDbError> {
    // SAFETY: dynamic SQL — compile-time macro not applicable.
    sqlx::query(
        "INSERT INTO inspiration_items (item_id, creator_id, rel_path, title, status, promoted_work_id, created_at, promoted_at) \
         VALUES (?, ?, ?, ?, 'idea', NULL, ?, NULL)",
    )
    .bind(item_id)
    .bind(creator_id)
    .bind(rel_path)
    .bind(title)
    .bind(created_at)
    .execute(pool)
    .await?;

    get_inspiration(pool, item_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("inspiration_items/{item_id}"),
        })
}

/// Create an inspiration item atomically: DB row + markdown file scaffold.
///
/// The MD file is written via tmp+rename for atomicity. If the MD file
/// already exists, returns an error without modifying the DB.
///
/// `workspace_dir` must be the resolved operational workspace directory
/// (per `nexus_home_layout::operational_workspace_dir`). The scaffold is
/// written to `{workspace_dir}/Pool/Ideas/<slug>.md`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails, the file cannot be
/// written, or the file already exists.
pub async fn create_inspiration_with_scaffold(
    pool: &SqlitePool,
    item_id: &str,
    creator_id: &str,
    title: &str,
    workspace_dir: &std::path::Path,
    created_at: &str,
) -> Result<InspirationItem, LocalDbError> {
    let slug = title_to_slug(title);
    let rel_path = format!("Pool/Ideas/{slug}.md");

    // Step 1: Write MD file (tmp + rename) — fail if exists
    let abs_path = workspace_dir.join(&rel_path);

    // Ensure parent directory exists
    if let Some(parent) = abs_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| LocalDbError::Io(e.to_string()))?;
    }

    if abs_path.exists() {
        return Err(LocalDbError::ConstraintViolation {
            table: "inspiration_items".to_string(),
            constraint: format!(
                "inspiration file already exists at {rel_path} — use a different title or archive the existing one"
            ),
        });
    }

    let frontmatter = format!("---\ntitle: {title}\nstatus: idea\ncreated_at: {created_at}\n---\n");

    // Write via tmp + rename for atomicity
    let tmp_path = abs_path.with_extension("md.tmp");
    std::fs::write(&tmp_path, &frontmatter).map_err(|e| LocalDbError::Io(e.to_string()))?;
    std::fs::rename(&tmp_path, &abs_path).map_err(|e| {
        // Clean up tmp file on rename failure
        let _ = std::fs::remove_file(&tmp_path);
        LocalDbError::Io(e.to_string())
    })?;

    // Step 2: Insert DB row
    match create_inspiration_row(pool, item_id, creator_id, &rel_path, title, created_at).await {
        Ok(item) => Ok(item),
        Err(e) => {
            // Roll back MD file on DB failure
            let _ = std::fs::remove_file(&abs_path);
            Err(e)
        }
    }
}

/// Promote an inspiration item: update status to `promoted` and record the `work_id`.
///
/// This does NOT create the Work or pool row — the caller (daemon handler)
/// is responsible for that. This function only records the promotion in the
/// `inspiration_items` table.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the item is not found.
pub async fn promote_inspiration(
    pool: &SqlitePool,
    item_id: &str,
    promoted_work_id: &str,
    promoted_at: &str,
) -> Result<InspirationItem, LocalDbError> {
    // SAFETY: dynamic SQL — compile-time macro not applicable.
    sqlx::query(
        "UPDATE inspiration_items SET status = 'promoted', promoted_work_id = ?, promoted_at = ? \
         WHERE item_id = ?",
    )
    .bind(promoted_work_id)
    .bind(promoted_at)
    .bind(item_id)
    .execute(pool)
    .await?;

    get_inspiration(pool, item_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("inspiration_items/{item_id}"),
        })
}

/// Archive an inspiration item (set status to `archived`).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the item is not found.
pub async fn archive_inspiration(
    pool: &SqlitePool,
    item_id: &str,
) -> Result<InspirationItem, LocalDbError> {
    // SAFETY: dynamic SQL — compile-time macro not applicable.
    sqlx::query("UPDATE inspiration_items SET status = 'archived' WHERE item_id = ?")
        .bind(item_id)
        .execute(pool)
        .await?;

    get_inspiration(pool, item_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("inspiration_items/{item_id}"),
        })
}

/// Get a single inspiration item by ID.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn get_inspiration(
    pool: &SqlitePool,
    item_id: &str,
) -> Result<Option<InspirationItem>, LocalDbError> {
    let row = sqlx::query(&format!(
        "SELECT {INSPIRATION_COLUMNS} FROM inspiration_items WHERE item_id = ?"
    ))
    .bind(item_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.as_ref().map(row_to_inspiration_item))
}

/// Convert a human title to a filesystem-safe slug.
///
/// Rules:
/// - Lowercase ASCII only (non-ASCII → hyphen)
/// - Hyphens for spaces
/// - Truncate to 64 characters
/// - Reject empty or reserved slugs
#[must_use]
pub fn title_to_slug(title: &str) -> String {
    let slug: String = title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c
            } else if c == ' ' || c == '-' || c == '_' {
                '-'
            } else {
                // Non-ASCII → hyphen
                '-'
            }
        })
        .collect();

    // Collapse consecutive hyphens
    let mut result = String::with_capacity(slug.len());
    let mut prev_hyphen = false;
    for c in slug.chars() {
        if c == '-' {
            if !prev_hyphen {
                result.push('-');
                prev_hyphen = true;
            }
        } else {
            result.push(c);
            prev_hyphen = false;
        }
    }

    // Strip leading/trailing hyphens
    let trimmed = result.trim_matches('-');

    // Truncate to 64 chars
    let truncated: String = trimmed.chars().take(64).collect();
    let final_slug = truncated.trim_matches('-').to_string();

    // Reject empty
    if final_slug.is_empty() {
        return "untitled".to_string();
    }

    // Reject reserved names
    if final_slug == "." || final_slug == ".." {
        return "untitled".to_string();
    }

    final_slug
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_title_to_slug_basic() {
        assert_eq!(title_to_slug("Cyberpunk Heist"), "cyberpunk-heist");
    }

    #[test]
    fn test_title_to_slug_chinese() {
        // Non-ASCII → hyphen, collapsed
        assert_eq!(title_to_slug("灵感和创意"), "untitled");
        // Mixed: the CJK chars become hyphens, collapsed, stripped → empty → untitled
        // But a better test:
        let slug = title_to_slug("My 灵感 Idea");
        // "my-idea" (CJK → hyphens collapsed, trailing stripped)
        assert!(slug.contains("my"));
        assert!(slug.contains("idea"));
    }

    #[test]
    fn test_title_to_slug_truncation() {
        let long = "a".repeat(100);
        let slug = title_to_slug(&long);
        assert!(slug.len() <= 64);
    }

    #[test]
    fn test_title_to_slug_empty() {
        assert_eq!(title_to_slug(""), "untitled");
        assert_eq!(title_to_slug("   "), "untitled");
    }

    #[test]
    fn test_title_to_slug_reserved() {
        assert_eq!(title_to_slug(".."), "untitled");
    }
}
