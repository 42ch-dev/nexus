//! DAO for reading progress and annotations (V1.89).

use chrono::{DateTime, Utc};
use sqlx::{Pool, Sqlite};

use crate::LocalDbError;

/// Upsert scroll progress for a single chapter.
///
/// `progress` is a 0-10000 integer. Returns the server-generated `updated_at`.
///
/// # Errors
///
/// Returns [`LocalDbError::ValidationError`] if `progress` is outside 0–10000,
/// or [`LocalDbError::Sqlx`] on database failure.
pub async fn upsert_reading_progress(
    pool: &Pool<Sqlite>,
    creator_id: &str,
    work_id: &str,
    chapter: i64,
    progress: i64,
) -> Result<DateTime<Utc>, LocalDbError> {
    if !(0..=10_000).contains(&progress) {
        return Err(LocalDbError::ValidationError(format!(
            "scroll_progress must be between 0 and 10000, got {progress}"
        )));
    }

    let now = Utc::now();
    let updated_at = sqlx::query_scalar!(
        r#"
        INSERT INTO reading_progress (creator_id, work_id, chapter, scroll_progress, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5)
        ON CONFLICT(creator_id, work_id, chapter)
        DO UPDATE SET scroll_progress = excluded.scroll_progress, updated_at = excluded.updated_at
        RETURNING updated_at as "updated_at: DateTime<Utc>"
        "#,
        creator_id,
        work_id,
        chapter,
        progress,
        now
    )
    .fetch_one(pool)
    .await
    .map_err(LocalDbError::Sqlx)?;

    Ok(updated_at)
}

/// Get scroll progress for a chapter. Returns `0` and `None` for `updated_at`
/// when no row exists.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn get_reading_progress(
    pool: &Pool<Sqlite>,
    creator_id: &str,
    work_id: &str,
    chapter: i64,
) -> Result<(i64, Option<DateTime<Utc>>), LocalDbError> {
    let row = sqlx::query_as!(
        ReadingProgressRow,
        r#"
        SELECT scroll_progress as "scroll_progress: _", updated_at as "updated_at: _"
        FROM reading_progress
        WHERE creator_id = ?1 AND work_id = ?2 AND chapter = ?3
        "#,
        creator_id,
        work_id,
        chapter
    )
    .fetch_optional(pool)
    .await
    .map_err(LocalDbError::Sqlx)?;

    match row {
        Some(ReadingProgressRow {
            scroll_progress: Some(progress),
            updated_at,
        }) => Ok((progress, updated_at)),
        Some(ReadingProgressRow {
            scroll_progress: None,
            ..
        })
        | None => Ok((0, None)),
    }
}

/// Delete progress for a chapter.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn delete_reading_progress(
    pool: &Pool<Sqlite>,
    creator_id: &str,
    work_id: &str,
    chapter: i64,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        r"
        DELETE FROM reading_progress
        WHERE creator_id = ?1 AND work_id = ?2 AND chapter = ?3
        ",
        creator_id,
        work_id,
        chapter
    )
    .execute(pool)
    .await
    .map_err(LocalDbError::Sqlx)?;

    Ok(())
}

/// List annotations for a chapter in ascending creation order.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn list_annotations(
    pool: &Pool<Sqlite>,
    creator_id: &str,
    work_id: &str,
    chapter: i64,
) -> Result<Vec<AnnotationRow>, LocalDbError> {
    let rows = sqlx::query_as!(
        AnnotationRow,
        r#"
        SELECT
            annotation_id as "annotation_id!",
            creator_id as "creator_id!",
            work_id as "work_id!",
            chapter as "chapter!",
            start_offset as "start_offset!",
            end_offset as "end_offset!",
            selected_text as "selected_text!",
            color as "color!",
            note,
            created_at as "created_at: _",
            updated_at as "updated_at: _"
        FROM reading_annotations
        WHERE creator_id = ?1 AND work_id = ?2 AND chapter = ?3
        ORDER BY created_at ASC
        "#,
        creator_id,
        work_id,
        chapter
    )
    .fetch_all(pool)
    .await
    .map_err(LocalDbError::Sqlx)?;

    Ok(rows)
}

/// Get a single annotation by ID.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn get_annotation(
    pool: &Pool<Sqlite>,
    annotation_id: &str,
) -> Result<Option<AnnotationRow>, LocalDbError> {
    let row = sqlx::query_as!(
        AnnotationRow,
        r#"
        SELECT
            annotation_id as "annotation_id!",
            creator_id as "creator_id!",
            work_id as "work_id!",
            chapter as "chapter!",
            start_offset as "start_offset!",
            end_offset as "end_offset!",
            selected_text as "selected_text!",
            color as "color!",
            note,
            created_at as "created_at: _",
            updated_at as "updated_at: _"
        FROM reading_annotations
        WHERE annotation_id = ?1
        "#,
        annotation_id
    )
    .fetch_optional(pool)
    .await
    .map_err(LocalDbError::Sqlx)?;

    Ok(row)
}

/// Insert a new annotation.
///
/// # Errors
///
/// Returns [`LocalDbError::ValidationError`] if `end_offset` is not greater
/// than `start_offset`, or [`LocalDbError::Sqlx`] on database failure.
// Allowed: one parameter per `reading_annotations` column so callers remain
// explicit and the DAO stays schema-aligned without a builder type for MVP.
#[allow(clippy::too_many_arguments)]
pub async fn create_annotation(
    pool: &Pool<Sqlite>,
    creator_id: &str,
    work_id: &str,
    chapter: i64,
    annotation_id: &str,
    start_offset: i64,
    end_offset: i64,
    selected_text: &str,
    color: &str,
    note: Option<&str>,
) -> Result<AnnotationRow, LocalDbError> {
    if end_offset <= start_offset {
        return Err(LocalDbError::ValidationError(format!(
            "end_offset ({end_offset}) must be greater than start_offset ({start_offset})"
        )));
    }

    let now = Utc::now();
    let row = sqlx::query_as!(
        AnnotationRow,
        r#"
        INSERT INTO reading_annotations
            (annotation_id, creator_id, work_id, chapter, start_offset, end_offset, selected_text, color, note, created_at, updated_at)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        RETURNING
            annotation_id as "annotation_id!",
            creator_id as "creator_id!",
            work_id as "work_id!",
            chapter as "chapter!",
            start_offset as "start_offset!",
            end_offset as "end_offset!",
            selected_text as "selected_text!",
            color as "color!",
            note,
            created_at as "created_at: _",
            updated_at as "updated_at: _"
        "#,
        annotation_id,
        creator_id,
        work_id,
        chapter,
        start_offset,
        end_offset,
        selected_text,
        color,
        note,
        now,
        now
    )
    .fetch_one(pool)
    .await
    .map_err(LocalDbError::Sqlx)?;

    Ok(row)
}

/// Update an existing annotation.
///
/// # Errors
///
/// Returns [`LocalDbError::ValidationError`] if the resulting offsets would be
/// invalid, or [`LocalDbError::Sqlx`] on database failure.
pub async fn update_annotation(
    pool: &Pool<Sqlite>,
    annotation_id: &str,
    color: Option<&str>,
    note: Option<Option<&str>>,
) -> Result<Option<AnnotationRow>, LocalDbError> {
    let Some(existing) = get_annotation(pool, annotation_id).await? else {
        return Ok(None);
    };

    let new_color = color.unwrap_or(&existing.color);
    let new_note = match note {
        Some(n) => n,
        None => existing.note.as_deref(),
    };

    let now = Utc::now();
    let row = sqlx::query_as!(
        AnnotationRow,
        r#"
        UPDATE reading_annotations
        SET color = ?1,
            note = ?2,
            updated_at = ?3
        WHERE annotation_id = ?4
        RETURNING
            annotation_id as "annotation_id!",
            creator_id as "creator_id!",
            work_id as "work_id!",
            chapter as "chapter!",
            start_offset as "start_offset!",
            end_offset as "end_offset!",
            selected_text as "selected_text!",
            color as "color!",
            note,
            created_at as "created_at: _",
            updated_at as "updated_at: _"
        "#,
        new_color,
        new_note,
        now,
        annotation_id
    )
    .fetch_one(pool)
    .await
    .map_err(LocalDbError::Sqlx)?;

    Ok(Some(row))
}

/// Delete an annotation by ID.
///
/// # Errors
///
/// Returns [`LocalDbError::Sqlx`] on database failure.
pub async fn delete_annotation(
    pool: &Pool<Sqlite>,
    annotation_id: &str,
) -> Result<(), LocalDbError> {
    sqlx::query!(
        "DELETE FROM reading_annotations WHERE annotation_id = ?1",
        annotation_id
    )
    .execute(pool)
    .await
    .map_err(LocalDbError::Sqlx)?;

    Ok(())
}

/// Database row for a reading-progress lookup.
#[derive(Debug, Clone, sqlx::FromRow)]
struct ReadingProgressRow {
    scroll_progress: Option<i64>,
    updated_at: Option<DateTime<Utc>>,
}

/// Database row for an annotation.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct AnnotationRow {
    pub annotation_id: String,
    pub creator_id: String,
    pub work_id: String,
    pub chapter: i64,
    pub start_offset: i64,
    pub end_offset: i64,
    pub selected_text: String,
    pub color: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::works;
    use sqlx::sqlite::SqlitePoolOptions;

    async fn setup_pool() -> Pool<Sqlite> {
        let pool = SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("connect");
        // Route through the central runner: a raw `sqlx::migrate!().run()`
        // bypasses the FK-suspension scoping and the post-migration
        // foreign_key_check that crate::run_migrations guarantees.
        crate::run_migrations(&pool).await.expect("migrate");
        pool
    }

    async fn seed_work(pool: &Pool<Sqlite>, creator_id: &str, work_id: &str) {
        let now = chrono::Utc::now().to_rfc3339();
        let record = works::WorkRecord {
            work_id: work_id.to_string(),
            creator_id: creator_id.to_string(),
            workspace_slug: "ws".to_string(),
            status: "draft".to_string(),
            title: "Test Work".to_string(),
            long_term_goal: "goal".to_string(),
            initial_idea: "idea".to_string(),
            creative_brief: None,
            intake_status: "pending".to_string(),
            world_id: None,
            story_ref: None,
            inspiration_log: "[]".to_string(),
            primary_preset_id: "novel-writing".to_string(),
            schedule_ids: "[]".to_string(),
            created_at: now.clone(),
            updated_at: now,
            current_stage: "intake".to_string(),
            stage_status: "pending".to_string(),
            work_profile: Some("novel".to_string()),
            work_ref: None,
            total_planned_chapters: None,
            current_chapter: 1,
            auto_chain_enabled: false,
            driver_schedule_id: None,
            auto_chain_interrupted: false,
            auto_review_master_on_timeout: false,
            runtime_lock_holder: None,
            runtime_lock_acquired_at: None,
            completion_locked_at: None,
            novel_completion_status: None,
            lineage_from_work_id: None,
        };
        works::create_work(pool, &record).await.expect("seed work");
    }

    #[tokio::test]
    async fn test_progress_crud() {
        let pool = setup_pool().await;
        let creator_id = "creator1";
        let work_id = "work1";
        seed_work(&pool, creator_id, work_id).await;

        let updated = upsert_reading_progress(&pool, creator_id, work_id, 1, 5000)
            .await
            .expect("upsert");
        let (progress, ts) = get_reading_progress(&pool, creator_id, work_id, 1)
            .await
            .expect("get");
        assert_eq!(progress, 5000);
        assert_eq!(ts, Some(updated));

        let updated2 = upsert_reading_progress(&pool, creator_id, work_id, 1, 7500)
            .await
            .expect("upsert2");
        let (progress2, ts2) = get_reading_progress(&pool, creator_id, work_id, 1)
            .await
            .expect("get2");
        assert_eq!(progress2, 7500);
        assert!(ts2 >= Some(updated));
        assert_eq!(ts2, Some(updated2));

        delete_reading_progress(&pool, creator_id, work_id, 1)
            .await
            .expect("delete");
        let (progress3, ts3) = get_reading_progress(&pool, creator_id, work_id, 1)
            .await
            .expect("get3");
        assert_eq!(progress3, 0);
        assert_eq!(ts3, None);
    }

    #[tokio::test]
    async fn test_progress_validation() {
        let pool = setup_pool().await;
        seed_work(&pool, "c", "w").await;
        let err = upsert_reading_progress(&pool, "c", "w", 1, -1)
            .await
            .expect_err("should fail");
        assert!(matches!(err, LocalDbError::ValidationError(_)));

        let err = upsert_reading_progress(&pool, "c", "w", 1, 10001)
            .await
            .expect_err("should fail");
        assert!(matches!(err, LocalDbError::ValidationError(_)));
    }

    #[tokio::test]
    async fn test_annotation_crud() {
        let pool = setup_pool().await;
        let creator_id = "creator1";
        let work_id = "work1";
        seed_work(&pool, creator_id, work_id).await;

        let row = create_annotation(
            &pool,
            creator_id,
            work_id,
            1,
            "ann_1",
            10,
            20,
            "selected text",
            "yellow",
            Some("a note"),
        )
        .await
        .expect("create");
        assert_eq!(row.annotation_id, "ann_1");
        assert_eq!(row.selected_text, "selected text");

        let list = list_annotations(&pool, creator_id, work_id, 1)
            .await
            .expect("list");
        assert_eq!(list.len(), 1);

        let updated = update_annotation(&pool, "ann_1", Some("blue"), Some(None))
            .await
            .expect("update")
            .expect("exists");
        assert_eq!(updated.color, "blue");
        assert_eq!(updated.note, None);

        delete_annotation(&pool, "ann_1").await.expect("delete");
        let list2 = list_annotations(&pool, creator_id, work_id, 1)
            .await
            .expect("list2");
        assert!(list2.is_empty());
    }

    #[tokio::test]
    async fn test_annotation_offset_validation() {
        let pool = setup_pool().await;
        seed_work(&pool, "c", "w").await;
        let err = create_annotation(
            &pool, "c", "w", 1, "ann_bad", 20, 10, "text", "yellow", None,
        )
        .await
        .expect_err("should fail");
        assert!(matches!(err, LocalDbError::ValidationError(_)));
    }
}
