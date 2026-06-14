//! Inspiration items DAO (DF-61 selection pool).
//!
//! Manages the `inspiration_items` table — creator-scoped inspiration
//! items with optional markdown file scaffold under `{workspace}/Pool/Ideas/`.
//!
//! Spec: novel-work-pool.md §3, local-db-schema.md §4.1.5.
//!
//! # Instrumented mutation paths (V1.46 P4 audit)
//!
//! The following `pub fn` mutate the `inspiration_items` table (or perform
//! the atomic promotion that also inserts a Work and a pool entry) and are
//! instrumented with `tracing::info!`:
//!
//! - [`create_inspiration_row`]
//! - [`create_inspiration_with_scaffold`]
//! - [`promote_inspiration`]
//! - [`inspiration_promote_atomic`]
//! - [`archive_inspiration`]
//!
//! Read-only functions (`list_inspiration`, `count_inspiration`,
//! `get_inspiration`) and helpers (`title_to_slug`, `generate_fallback_slug`)
//! are intentionally not traced.

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
/// `limit` defaults to 200 when `None`; capped at 1000.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn list_inspiration(
    pool: &SqlitePool,
    creator_id: &str,
    status_filter: Option<&str>,
    limit: Option<u32>,
    offset: Option<u32>,
) -> Result<Vec<InspirationItem>, LocalDbError> {
    let effective_limit = limit.unwrap_or(200).min(1000);
    let effective_offset = offset.unwrap_or(0);

    let sql = if status_filter.is_some() {
        format!(
            "SELECT {INSPIRATION_COLUMNS} FROM inspiration_items \
             WHERE creator_id = ? AND status = ? \
             ORDER BY created_at DESC \
             LIMIT ? OFFSET ?"
        )
    } else {
        format!(
            "SELECT {INSPIRATION_COLUMNS} FROM inspiration_items \
             WHERE creator_id = ? \
             ORDER BY created_at DESC \
             LIMIT ? OFFSET ?"
        )
    };

    let mut query = sqlx::query(&sql).bind(creator_id);
    if let Some(s) = status_filter {
        query = query.bind(s);
    }
    query = query.bind(effective_limit).bind(effective_offset);

    let rows = query.fetch_all(pool).await?;
    Ok(rows.iter().map(row_to_inspiration_item).collect())
}

/// Count inspiration items for a creator, optionally filtered by status.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails.
pub async fn count_inspiration(
    pool: &SqlitePool,
    creator_id: &str,
    status_filter: Option<&str>,
) -> Result<u32, LocalDbError> {
    let sql = if status_filter.is_some() {
        "SELECT COUNT(*) FROM inspiration_items WHERE creator_id = ? AND status = ?"
    } else {
        "SELECT COUNT(*) FROM inspiration_items WHERE creator_id = ?"
    };

    let mut query = sqlx::query(sql).bind(creator_id);
    if let Some(s) = status_filter {
        query = query.bind(s);
    }

    let count: i64 = query.fetch_one(pool).await?.get(0);
    Ok(u32::try_from(count).unwrap_or(0))
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
/// The MD file is written via tmp+rename for atomicity.
///
/// V1.42 P-last (R-V141P1-13): on slug collision, auto-suffixes with -2, -3, etc.
/// instead of returning an error.
///
/// `workspace_dir` must be the resolved operational workspace directory
/// (per `nexus_home_layout::operational_workspace_dir`). The scaffold is
/// written to `{workspace_dir}/Pool/Ideas/<slug>.md`.
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the file cannot be
/// written after exhausting collision retries.
pub async fn create_inspiration_with_scaffold(
    pool: &SqlitePool,
    item_id: &str,
    creator_id: &str,
    title: &str,
    workspace_dir: &std::path::Path,
    created_at: &str,
) -> Result<InspirationItem, LocalDbError> {
    let base_slug = title_to_slug(title);

    // V1.42 P-last (R-V141P1-13): try base slug, then -2, -3, ... up to 100.
    let mut suffix: u32 = 0;
    let (rel_path, abs_path) = loop {
        let candidate = if suffix == 0 {
            base_slug.clone()
        } else {
            format!("{base_slug}-{suffix}")
        };
        let rp = format!("Pool/Ideas/{candidate}.md");
        let ap = workspace_dir.join(&rp);
        if !ap.exists() {
            break (rp, ap);
        }
        suffix += 1;
        if suffix > 100 {
            return Err(LocalDbError::ConstraintViolation {
                table: "inspiration_items".to_string(),
                constraint: format!(
                    "too many slug collisions for '{base_slug}' (tried up to -100)"
                ),
            });
        }
    };

    // Step 1: Write MD file (tmp + rename)
    let frontmatter = format!("---\ntitle: {title}\nstatus: idea\ncreated_at: {created_at}\n---\n");

    let abs_path_clone = abs_path.clone();
    tokio::task::spawn_blocking(move || -> Result<std::path::PathBuf, LocalDbError> {
        if let Some(parent) = abs_path_clone.parent() {
            std::fs::create_dir_all(parent).map_err(|e| LocalDbError::Io(e.to_string()))?;
        }

        let tmp_path = abs_path_clone.with_extension("md.tmp");
        std::fs::write(&tmp_path, &frontmatter).map_err(|e| LocalDbError::Io(e.to_string()))?;
        std::fs::rename(&tmp_path, &abs_path_clone).map_err(|e| {
            let _ = std::fs::remove_file(&tmp_path);
            LocalDbError::Io(e.to_string())
        })?;

        Ok(abs_path_clone)
    })
    .await
    .map_err(|e| LocalDbError::Io(e.to_string()))??;

    // Step 2: Insert DB row
    match create_inspiration_row(pool, item_id, creator_id, &rel_path, title, created_at).await {
        Ok(item) => Ok(item),
        Err(e) => {
            // Roll back MD file on DB failure
            let rollback_path = workspace_dir.join(&rel_path);
            let _ = tokio::task::spawn_blocking(move || std::fs::remove_file(rollback_path)).await;
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

/// Atomic inspiration promote: Work insert + pool promote + inspiration update
/// in a single transaction. Rolls back all three if any step fails (qc2 W-02).
///
/// # Errors
///
/// Returns `LocalDbError` if any of the three DB operations fails.
pub async fn inspiration_promote_atomic(
    pool: &SqlitePool,
    work_record: &crate::works::WorkRecord,
    creator_id: &str,
    work_id: &str,
    item_id: &str,
    now: &str,
) -> Result<crate::novel_pool_entries::PoolEntry, LocalDbError> {
    use sqlx::Connection;

    let mut conn = pool.acquire().await?;
    let mut tx = conn.begin().await?;

    // Step 1: Insert Work
    // SAFETY: dynamic SQL — compile-time macro not applicable.
    sqlx::query(
        "INSERT INTO works (work_id, creator_id, workspace_slug, status, title, long_term_goal,
         initial_idea, creative_brief, intake_status, world_id, story_ref, inspiration_log,
         primary_preset_id, schedule_ids, created_at, updated_at, current_stage, stage_status,
         work_profile, work_ref, total_planned_chapters, current_chapter,
         auto_chain_enabled, driver_schedule_id, auto_chain_interrupted,
         auto_review_master_on_timeout,
         runtime_lock_holder, runtime_lock_acquired_at, completion_locked_at,
         novel_completion_status, lineage_from_work_id)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, NULL, ?, ?,
                 NULL, NULL, NULL, NULL, NULL)",
    )
    .bind(&work_record.work_id)
    .bind(&work_record.creator_id)
    .bind(&work_record.workspace_slug)
    .bind(&work_record.status)
    .bind(&work_record.title)
    .bind(&work_record.long_term_goal)
    .bind(&work_record.initial_idea)
    .bind(&work_record.creative_brief)
    .bind(&work_record.intake_status)
    .bind(&work_record.world_id)
    .bind(&work_record.story_ref)
    .bind(&work_record.inspiration_log)
    .bind(&work_record.primary_preset_id)
    .bind(&work_record.schedule_ids)
    .bind(&work_record.created_at)
    .bind(&work_record.updated_at)
    .bind(&work_record.current_stage)
    .bind(&work_record.stage_status)
    .bind(&work_record.work_profile)
    .bind(&work_record.work_ref)
    .bind(work_record.total_planned_chapters)
    .bind(work_record.current_chapter)
    .bind(work_record.auto_chain_enabled)
    .bind(work_record.auto_chain_interrupted)
    .bind(work_record.auto_review_master_on_timeout)
    .bind(&work_record.novel_completion_status)
    .bind(&work_record.lineage_from_work_id)
    .execute(&mut *tx)
    .await?;

    // Step 2: Demote prior active pool entry → queued, upsert target → active
    let entry_id = format!("npe_{}", uuid::Uuid::new_v4());
    let work_title: Option<String> =
        sqlx::query_scalar("SELECT title FROM works WHERE work_id = ? AND creator_id = ?")
            .bind(work_id)
            .bind(creator_id)
            .fetch_optional(&mut *tx)
            .await?
            .flatten();
    let title = work_title.unwrap_or_default();

    // Demote prior active
    sqlx::query(
        "UPDATE novel_pool_entries SET status = 'queued', updated_at = ? \
         WHERE creator_id = ? AND status = 'active'",
    )
    .bind(now)
    .bind(creator_id)
    .execute(&mut *tx)
    .await?;

    // Upsert target
    sqlx::query(
        "INSERT INTO novel_pool_entries (entry_id, creator_id, work_id, status, promoted_at, note, title, updated_at) \
         VALUES (?, ?, ?, 'active', ?, NULL, ?, ?) \
         ON CONFLICT(creator_id, work_id) DO UPDATE SET \
           status = 'active', promoted_at = excluded.promoted_at, note = NULL, \
           title = excluded.title, updated_at = excluded.updated_at",
    )
    .bind(&entry_id)
    .bind(creator_id)
    .bind(work_id)
    .bind(now)
    .bind(&title)
    .bind(now)
    .execute(&mut *tx)
    .await?;

    // Step 3: Update inspiration item to promoted
    sqlx::query(
        "UPDATE inspiration_items SET status = 'promoted', promoted_work_id = ?, promoted_at = ? \
         WHERE item_id = ?",
    )
    .bind(work_id)
    .bind(now)
    .bind(item_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    // Fetch the upserted pool entry
    crate::novel_pool_entries::get_pool_entry_by_work(pool, creator_id, work_id)
        .await?
        .ok_or_else(|| LocalDbError::MissingVersionKey {
            key: format!("novel_pool_entries/{work_id}"),
        })
}

/// Archive an inspiration item (set status to `archived`).
///
/// Restricted to the owning `creator_id` — rows belonging to other
/// creators are silently unaffected (0 rows updated → `MissingVersionKey`).
///
/// # Errors
///
/// Returns `LocalDbError` if the database query fails or the item is not found
/// (or does not belong to the given `creator_id`).
pub async fn archive_inspiration(
    pool: &SqlitePool,
    item_id: &str,
    creator_id: &str,
) -> Result<InspirationItem, LocalDbError> {
    // SAFETY: dynamic SQL — compile-time macro not applicable.
    let result = sqlx::query(
        "UPDATE inspiration_items SET status = 'archived' WHERE item_id = ? AND creator_id = ?",
    )
    .bind(item_id)
    .bind(creator_id)
    .execute(pool)
    .await?;

    if result.rows_affected() == 0 {
        return Err(LocalDbError::MissingVersionKey {
            key: format!("inspiration_items/{item_id} (creator {creator_id})"),
        });
    }

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
/// - CJK fallback: if the resulting slug would be "untitled", generate
///   a short ID suffix instead (e.g. `idea-a3f2`)
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
        return generate_fallback_slug();
    }

    // Reject reserved names
    if final_slug == "." || final_slug == ".." {
        return generate_fallback_slug();
    }

    final_slug
}

/// V1.42 P-last (R-V141P1-12): CJK fallback — when the title produces no ASCII
/// slug, generate a short ID suffix instead of "untitled".
fn generate_fallback_slug() -> String {
    // Use a simple timestamp-based short ID (6 hex chars = 24 bits of entropy).
    // Good enough for local-first use; avoids external deps.
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("idea-{:06x}", (now.subsec_nanos() >> 4) & 0x00FF_FFFF)
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
        // V1.42 P-last (R-V141P1-12): pure CJK now produces idea-<hex>
        // instead of "untitled"
        let slug = title_to_slug("灵感和创意");
        assert!(slug.starts_with("idea-"));
        assert!(slug.len() > 5);
        // Mixed: the CJK chars become hyphens, collapsed, trailing stripped
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
        // Empty/whitespace now produces idea-<hex> fallback
        let slug = title_to_slug("");
        assert!(slug.starts_with("idea-"));
        let slug = title_to_slug("   ");
        assert!(slug.starts_with("idea-"));
    }

    #[test]
    fn test_title_to_slug_reserved() {
        let slug = title_to_slug("..");
        assert!(slug.starts_with("idea-"));
    }
}
