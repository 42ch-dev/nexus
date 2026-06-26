//! Chapter content Local API handlers (V1.65 P0).
//!
//! Endpoints under `/v1/local/works/{work_id}/chapters/*` expose the
//! `work_chapters` metadata table and the file-backed outline/body markdown.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug};
use crate::api::path_guard::resolve_guarded_path;
use crate::api::runtime_lock::RuntimeLockGuard;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, Query, State};
use axum::Json;
use nexus_contracts::{
    ChapterBody, ChapterContentQuery, ChapterDetail, ChapterOutline, ChapterProtection,
    ChapterStatus, ChapterSummary, ListChaptersQuery, ListChaptersResponse, PaginationInfo,
    PatchChapterRequest, PutChapterOutlineRequest,
};
use nexus_local_db::work_chapters::{self, PatchChapterParams, WorkChapterRecord};
use nexus_local_db::works;
use std::path::{Path as StdPath, PathBuf};

#[cfg(test)]
use std::sync::atomic::{AtomicBool, Ordering};

#[cfg(test)]
static TEST_UPDATE_OUTLINE_PATH_FAIL: AtomicBool = AtomicBool::new(false);

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Parse a chapter number path parameter.
fn parse_chapter(n: &str) -> Result<i32, NexusApiError> {
    n.parse::<i32>()
        .map_err(|_| NexusApiError::BadRequest {
            code: "invalid_chapter_number".to_string(),
            message: format!("chapter number must be a positive integer, got '{n}'"),
        })
        .and_then(|v| {
            if v < 1 {
                Err(NexusApiError::BadRequest {
                    code: "invalid_chapter_number".to_string(),
                    message: format!("chapter number must be >= 1, got {v}"),
                })
            } else {
                Ok(v)
            }
        })
}

/// Prefix for chapter keyset cursors.
const CHAPTER_CURSOR_PREFIX: &str = "v2:";

/// Decode an opaque chapter-list cursor into the `(volume, chapter)` tuple
/// that the next page must start after.
///
/// `None` decodes to `(1, 0)` so the first page includes all chapters.
fn decode_chapter_cursor(cursor: Option<&String>) -> Result<(i32, i32), NexusApiError> {
    match cursor {
        None => Ok((1, 0)),
        Some(raw) => {
            let stripped = raw.strip_prefix(CHAPTER_CURSOR_PREFIX).ok_or_else(|| {
                NexusApiError::BadRequest {
                    code: "invalid_input".to_string(),
                    message: "invalid chapter_cursor; pass the next_cursor value unchanged"
                        .to_string(),
                }
            })?;
            let mut parts = stripped.splitn(2, ':');
            let volume = parts
                .next()
                .and_then(|s| s.parse::<i32>().ok())
                .filter(|v| *v >= 1)
                .ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".to_string(),
                    message: "invalid chapter_cursor volume".to_string(),
                })?;
            let chapter = parts
                .next()
                .and_then(|s| s.parse::<i32>().ok())
                .filter(|v| *v >= 1)
                .ok_or_else(|| NexusApiError::BadRequest {
                    code: "invalid_input".to_string(),
                    message: "invalid chapter_cursor chapter".to_string(),
                })?;
            Ok((volume, chapter))
        }
    }
}

/// Encode a `(volume, chapter)` tuple into an opaque cursor token.
fn encode_chapter_cursor(volume: i32, chapter: i32) -> String {
    format!("{CHAPTER_CURSOR_PREFIX}{volume}:{chapter}")
}

/// Compute `(next_cursor, has_more)` for a keyset-paginated chapter page.
fn chapter_page_meta(records: &[WorkChapterRecord], limit: u32) -> (Option<String>, bool) {
    let limit_us = usize::try_from(limit).unwrap_or(usize::MAX);
    if records.len() > limit_us {
        let last = records.get(limit_us - 1).expect("limit > 0");
        let next_volume = last.volume.unwrap_or(1);
        let next_cursor = encode_chapter_cursor(next_volume, last.chapter);
        (Some(next_cursor), true)
    } else {
        (None, false)
    }
}

/// Resolve the active workspace root.
fn workspace_root(state: &WorkspaceState) -> Result<PathBuf, NexusApiError> {
    let path_str = state.workspace_path().ok_or(NexusApiError::Uninitialized)?;
    if path_str.is_empty() {
        return Err(NexusApiError::Uninitialized);
    }
    Ok(PathBuf::from(path_str))
}

/// Load the Work row and verify active creator ownership.
async fn load_work(
    state: &WorkspaceState,
    creator_id: &str,
    work_id: &str,
) -> Result<works::WorkRecord, NexusApiError> {
    works::get_work(state.pool(), creator_id, work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("work {work_id}")))
}

/// Compute protection metadata for a chapter based on its status.
fn chapter_protection(status: &str) -> ChapterProtection {
    match status {
        "finalized" => ChapterProtection {
            level: "confirm_structure_edit".to_string(),
            reason: "Chapter is finalized; structural edits require confirmation.".to_string(),
        },
        "published" => ChapterProtection {
            level: "hard_block_delete".to_string(),
            reason: "Chapter is published; structural edits are blocked.".to_string(),
        },
        _ => ChapterProtection {
            level: "none".to_string(),
            reason: "No protection.".to_string(),
        },
    }
}

/// Map a DB record to a `ChapterSummary` contract DTO.
fn to_summary(r: &WorkChapterRecord) -> ChapterSummary {
    ChapterSummary {
        work_id: r.work_id.clone(),
        chapter: i64::from(r.chapter),
        volume: i64::from(r.volume.unwrap_or(1)),
        title: None,
        slug: r.slug.clone(),
        planned_word_count: u64::try_from(r.planned_word_count).unwrap_or(0),
        actual_word_count: r.actual_word_count.map(|v| u64::try_from(v).unwrap_or(0)),
        status: r.status.parse().ok().unwrap_or(ChapterStatus::NotStarted),
        outline_path: r.outline_path.clone(),
        body_path: r.body_path.clone(),
        created_at: r.created_at.clone(),
        updated_at: r.updated_at.clone(),
    }
}

/// Map a DB record to a `ChapterDetail` contract DTO.
fn to_detail(r: &WorkChapterRecord) -> ChapterDetail {
    ChapterDetail {
        work_id: r.work_id.clone(),
        chapter: i64::from(r.chapter),
        volume: i64::from(r.volume.unwrap_or(1)),
        title: None,
        slug: r.slug.clone(),
        planned_word_count: u64::try_from(r.planned_word_count).unwrap_or(0),
        actual_word_count: r.actual_word_count.map(|v| u64::try_from(v).unwrap_or(0)),
        status: r.status.parse().ok().unwrap_or(ChapterStatus::NotStarted),
        outline_path: r.outline_path.clone(),
        body_path: r.body_path.clone(),
        created_at: r.created_at.clone(),
        updated_at: r.updated_at.clone(),
        can_edit_outline: r.outline_path.as_deref().is_some_and(|s| !s.is_empty()),
        can_edit_structure: true,
        body_read_only: true,
        protection: chapter_protection(&r.status),
    }
}

/// Read a text file after path-guard verification.
///
/// Enforces a 10 MiB size cap to prevent unbounded memory reads on
/// unexpectedly large chapter bodies.
async fn read_guarded_file(
    workspace_root: &StdPath,
    rel_path: &str,
    forbidden_code: &str,
    not_found_code: &str,
) -> Result<String, NexusApiError> {
    const CHAPTER_BODY_MAX_BYTES: usize = 10 * 1024 * 1024;

    let path = resolve_guarded_path(workspace_root, rel_path, true).map_err(|e| {
        if matches!(e, NexusApiError::BadRequest { ref code, .. } if code == "chapter_path_forbidden")
        {
            NexusApiError::BadRequest {
                code: forbidden_code.to_string(),
                message: format!("chapter path '{rel_path}' escapes workspace root"),
            }
        } else {
            e
        }
    })?;

    let metadata = tokio::fs::metadata(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            NexusApiError::NotFound(format!("{not_found_code}: file not found at '{rel_path}'"))
        } else {
            NexusApiError::Internal {
                code: "FILE_READ_ERROR".to_string(),
                message: format!("failed to read metadata for '{rel_path}': {e}"),
            }
        }
    })?;

    let max_bytes = u64::try_from(CHAPTER_BODY_MAX_BYTES).unwrap_or(u64::MAX);
    if metadata.len() > max_bytes {
        return Err(NexusApiError::BadRequest {
            code: "chapter_body_too_large".to_string(),
            message: format!(
                "chapter body at '{rel_path}' is {size} bytes, exceeding the maximum of {max} bytes",
                size = metadata.len(),
                max = CHAPTER_BODY_MAX_BYTES
            ),
        });
    }

    match tokio::fs::read_to_string(&path).await {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(NexusApiError::NotFound(
            format!("{not_found_code}: file not found at '{rel_path}'"),
        )),
        Err(e) => Err(NexusApiError::Internal {
            code: "FILE_READ_ERROR".to_string(),
            message: format!("failed to read '{rel_path}': {e}"),
        }),
    }
}

/// Atomically write `content` to `rel_path` under `workspace_root`, creating
/// parent directories as needed. Mirrors `work_chapters::sync_frontmatter_status`.
async fn atomic_write_outline(
    workspace_root: &StdPath,
    rel_path: &str,
    content: &str,
) -> Result<(), NexusApiError> {
    let target = resolve_guarded_path(workspace_root, rel_path, false)?;

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DIRECTORY_CREATE_ERROR".to_string(),
                message: format!("failed to create parent directories for '{rel_path}': {e}"),
            })?;
    }

    let tmp_extension = format!(
        "md.tmp.{}.{}",
        std::process::id(),
        chrono::Utc::now().timestamp_millis()
    );
    let temp_path = target.with_extension(&tmp_extension);

    let write_result = async {
        tokio::fs::write(&temp_path, content).await?;
        let file = tokio::fs::File::open(&temp_path).await?;
        file.sync_all().await?;
        tokio::fs::rename(&temp_path, &target).await?;
        // Durability: fsync the final file after the atomic rename so a crash
        // after rename() returns does not leave the rename unflushed.
        let final_file = tokio::fs::File::open(&target).await?;
        final_file.sync_all().await?;
        // Durability: fsync the parent directory so the renamed entry is
        // committed to disk (QC3-S3).
        if let Some(parent) = target.parent() {
            let dir = tokio::fs::File::open(parent).await?;
            dir.sync_all().await?;
        }
        Ok::<(), std::io::Error>(())
    }
    .await;

    if let Err(e) = write_result {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(NexusApiError::Internal {
            code: "OUTLINE_WRITE_ERROR".to_string(),
            message: format!("failed to write outline to '{rel_path}': {e}"),
        });
    }

    Ok(())
}

/// Validate that a requested chapter status transition is allowed.
fn validate_status_transition(from: &str, to: &str) -> Result<(), NexusApiError> {
    if from == to {
        return Ok(());
    }
    match (from, to) {
        ("not_started", "outlined") => Ok(()),
        _ => Err(NexusApiError::BadRequest {
            code: "chapter_status_transition_invalid".to_string(),
            message: format!(
                "status transition '{from}' -> '{to}' is not allowed through this endpoint"
            ),
        }),
    }
}

// ─── Handlers ───────────────────────────────────────────────────────────────

/// `GET /v1/local/works/{work_id}/chapters` — cursor-paginated chapter summaries.
pub async fn list_chapters(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Query(query): Query<ListChaptersQuery>,
) -> Result<Json<ListChaptersResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    // Verify work exists and belongs to active creator.
    let _work = load_work(&state, &creator_id, &work_id).await?;

    let (cursor_volume, cursor_chapter) = decode_chapter_cursor(query.cursor.as_ref())?;
    // Clamp to [1, 100]: the JSON schema declares `minimum: 1`, but Axum's Query
    // extractor (serde) does not enforce schema constraints, so `?limit=0` would
    // otherwise reach `chapter_page_meta` and underflow `limit_us - 1`, panicking
    // with a 500. Default 50 when absent.
    let limit = u32::try_from(query.limit.unwrap_or(50).clamp(1, 100)).unwrap_or(50);
    let fetch_limit = i64::from(limit.saturating_add(1));

    let status_filter = query.status.as_ref().map(ChapterStatus::as_str);

    let records = work_chapters::list_chapters_paginated(
        state.pool(),
        &work_id,
        status_filter,
        fetch_limit,
        cursor_volume,
        cursor_chapter,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let (next_cursor, has_more) = chapter_page_meta(&records, limit);
    let items: Vec<ChapterSummary> = records
        .into_iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
        .map(|r| to_summary(&r))
        .collect();

    Ok(Json(ListChaptersResponse {
        items,
        pagination: PaginationInfo {
            limit: i64::from(limit),
            next_cursor,
            has_more,
        },
    }))
}

/// `GET /v1/local/works/{work_id}/chapters/{n}` — chapter detail.
pub async fn get_chapter(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
) -> Result<Json<ChapterDetail>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let _work = load_work(&state, &creator_id, &work_id).await?;
    let chapter = parse_chapter(&n)?;
    let volume = i32::try_from(query.volume.unwrap_or(1)).unwrap_or(1);

    let record = work_chapters::get_chapter(state.pool(), &work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("chapter {chapter} volume {volume}")))?;

    Ok(Json(to_detail(&record)))
}

/// `GET /v1/local/works/{work_id}/chapters/{n}/outline` — read outline markdown.
pub async fn get_chapter_outline(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
) -> Result<Json<ChapterOutline>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let _work = load_work(&state, &creator_id, &work_id).await?;
    let chapter = parse_chapter(&n)?;
    let volume = i32::try_from(query.volume.unwrap_or(1)).unwrap_or(1);

    let record = work_chapters::get_chapter(state.pool(), &work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("chapter {chapter} volume {volume}")))?;

    let outline_path = record
        .outline_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            NexusApiError::NotFound(format!(
                "chapter {chapter} volume {volume} has no outline_path"
            ))
        })?;

    let workspace_root = workspace_root(&state)?;
    let content = read_guarded_file(
        &workspace_root,
        outline_path,
        "chapter_outline_path_forbidden",
        "chapter_outline_not_found",
    )
    .await?;

    Ok(Json(ChapterOutline {
        work_id: record.work_id,
        chapter: i64::from(record.chapter),
        volume: i64::from(record.volume.unwrap_or(1)),
        outline_path: outline_path.to_string(),
        content,
        updated_at: record.updated_at,
    }))
}

/// `PUT /v1/local/works/{work_id}/chapters/{n}/outline` — replace outline atomically.
pub async fn put_chapter_outline(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
    Json(req): Json<PutChapterOutlineRequest>,
) -> Result<Json<ChapterOutline>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let work = load_work(&state, &creator_id, &work_id).await?;
    let chapter = parse_chapter(&n)?;
    let volume = i32::try_from(query.volume.unwrap_or(1)).unwrap_or(1);

    let record = work_chapters::get_chapter(state.pool(), &work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("chapter {chapter} volume {volume}")))?;

    let work_ref = work.work_ref.ok_or_else(|| NexusApiError::Internal {
        code: "WORK_REF_MISSING".to_string(),
        message: format!("work {work_id} has no work_ref"),
    })?;

    let outline_path = record
        .outline_path
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Works/{work_ref}/Outlines/chapters/ch{chapter:02}-outline.md"));

    let workspace_root = workspace_root(&state)?;

    // Acquire runtime lock before mutating file + DB.
    let lock = RuntimeLockGuard::acquire(state.pool(), &creator_id, &work_id).await?;

    // Test-only seam: simulate a DB-update failure to verify the file is not
    // written before the metadata is persisted.
    #[cfg(test)]
    if TEST_UPDATE_OUTLINE_PATH_FAIL.load(Ordering::SeqCst) {
        lock.release().await;
        return Err(NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: "simulated update_outline_path failure".to_string(),
        });
    }

    // Persist the metadata first, then write the file. If the DB update fails,
    // no file is created; if the file write fails after the DB commit, the DB
    // points to the intended path and a retry PUT is idempotent.
    let result: Result<(String, String, String), NexusApiError> = async {
        let now = chrono::Utc::now().to_rfc3339();
        work_chapters::update_outline_path(
            state.pool(),
            &work_id,
            chapter,
            volume,
            Some(&outline_path),
            &now,
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
        atomic_write_outline(&workspace_root, &outline_path, &req.content).await?;
        Ok((now, outline_path, req.content))
    }
    .await;

    lock.release().await;
    let (now, outline_path, content) = result?;

    Ok(Json(ChapterOutline {
        work_id: record.work_id,
        chapter: i64::from(record.chapter),
        volume: i64::from(record.volume.unwrap_or(1)),
        outline_path,
        content,
        updated_at: now,
    }))
}

/// `PATCH /v1/local/works/{work_id}/chapters/{n}` — partial structure update.
pub async fn patch_chapter(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
    Json(req): Json<PatchChapterRequest>,
) -> Result<Json<ChapterDetail>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let _work = load_work(&state, &creator_id, &work_id).await?;
    let chapter = parse_chapter(&n)?;
    let volume = i32::try_from(query.volume.unwrap_or(1)).unwrap_or(1);

    let record = work_chapters::get_chapter(state.pool(), &work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("chapter {chapter} volume {volume}")))?;

    // Reject display-only title writes in V1.65.
    if req.title.is_some() {
        return Err(NexusApiError::BadRequest {
            code: "chapter_title_unsupported".to_string(),
            message: "title is display-only in V1.65; use outline frontmatter or slug instead"
                .to_string(),
        });
    }

    // Validate status transition before acquiring lock.
    if let Some(ref target_status) = req.status {
        let target = target_status.as_str();
        validate_status_transition(&record.status, target)?;
    }

    // Structural edit protection for finalized/published chapters.
    let has_structural_edit = req.slug.is_some()
        || req.planned_word_count.is_some()
        || req.volume.is_some()
        || req.status.is_some();

    if has_structural_edit {
        match record.status.as_str() {
            "published" => {
                return Err(NexusApiError::BadRequest {
                    code: "chapter_structure_edit_blocked".to_string(),
                    message: "structural edits to published chapters are blocked".to_string(),
                });
            }
            "finalized" if !req.confirm_structural_edit.unwrap_or(false) => {
                return Err(NexusApiError::BadRequest {
                    code: "chapter_structure_confirmation_required".to_string(),
                    message: "set confirm_structural_edit=true to edit a finalized chapter"
                        .to_string(),
                });
            }
            _ => {}
        }
    }

    // Acquire runtime lock before mutating DB metadata.
    let lock = RuntimeLockGuard::acquire(state.pool(), &creator_id, &work_id).await?;

    let updated: Result<WorkChapterRecord, NexusApiError> = async {
        let patch = PatchChapterParams {
            slug: req.slug,
            planned_word_count: req
                .planned_word_count
                .map(|v| i32::try_from(v).unwrap_or(i32::MAX)),
            volume: req.volume.map(|v| i32::try_from(v).unwrap_or(1)),
            status: req.status.as_ref().map(ToString::to_string),
        };

        let now = chrono::Utc::now().to_rfc3339();
        work_chapters::patch_chapter(state.pool(), &work_id, chapter, volume, &patch, &now)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .then_some(())
            .ok_or_else(|| NexusApiError::NotFound(format!("chapter {chapter} volume {volume}")))?;

        // Re-fetch using the POST-patch volume. If `req.volume` changed it, the
        // row now lives at the new volume; re-fetching with the original query
        // `volume` would miss it and return 404 on a fully-committed write.
        let fetch_volume = patch.volume.unwrap_or(volume);
        work_chapters::get_chapter(state.pool(), &work_id, chapter, fetch_volume)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| {
                NexusApiError::NotFound(format!("chapter {chapter} volume {fetch_volume}"))
            })
    }
    .await;

    lock.release().await;
    Ok(Json(to_detail(&updated?)))
}

/// `GET /v1/local/works/{work_id}/chapters/{n}/body` — read body markdown (read-only).
pub async fn get_chapter_body(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Query(query): Query<ChapterContentQuery>,
) -> Result<Json<ChapterBody>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let _work = load_work(&state, &creator_id, &work_id).await?;
    let chapter = parse_chapter(&n)?;
    let volume = i32::try_from(query.volume.unwrap_or(1)).unwrap_or(1);

    let record = work_chapters::get_chapter(state.pool(), &work_id, chapter, volume)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("chapter {chapter} volume {volume}")))?;

    let body_path = record
        .body_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            NexusApiError::NotFound(format!(
                "chapter {chapter} volume {volume} has no body_path"
            ))
        })?;

    let workspace_root = workspace_root(&state)?;
    let content = read_guarded_file(
        &workspace_root,
        body_path,
        "chapter_body_path_forbidden",
        "chapter_body_not_found",
    )
    .await?;

    Ok(Json(ChapterBody {
        work_id: record.work_id,
        chapter: i64::from(record.chapter),
        volume: i64::from(record.volume.unwrap_or(1)),
        body_path: body_path.to_string(),
        content,
        frontmatter: None,
        read_only: true,
        updated_at: record.updated_at,
    }))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::handlers::works::{create_work, CreateWorkRequest};
    use axum::extract::{Path as AxumPath, Query as AxumQuery, State as AxumState};

    /// RAII guard that enables the simulated `update_outline_path` failure and
    /// disables it when dropped, even if the test panics.
    struct FailpointGuard;

    impl FailpointGuard {
        fn enable() -> Self {
            TEST_UPDATE_OUTLINE_PATH_FAIL.store(true, Ordering::SeqCst);
            Self
        }
    }

    impl Drop for FailpointGuard {
        fn drop(&mut self) {
            TEST_UPDATE_OUTLINE_PATH_FAIL.store(false, Ordering::SeqCst);
        }
    }

    async fn setup_chapter_work() -> (
        crate::workspace::WorkspaceState,
        crate::test_utils::TestTempRoot,
        String,
    ) {
        let (tmp, nexus_home, db_path) = crate::test_utils::create_test_workspace().await;
        let local_root = tmp.path().join("creative");
        std::fs::create_dir_all(&local_root).expect("create local root");
        let state = crate::workspace::WorkspaceState::new_for_testing(
            nexus_home,
            db_path,
            Some(local_root.to_string_lossy().to_string()),
        )
        .await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;

        let req = CreateWorkRequest {
            title: "Test Novel".into(),
            long_term_goal: "Write".into(),
            initial_idea: "Idea".into(),
            world_id: Some("wld_test_world".to_string()),
            story_ref: None,
            primary_preset_id: None,
            client_request_id: None,
            lineage_from_work_id: None,
            set_pool_active: None,
            work_profile: Some("novel".to_string()),
        };
        let (_, resp) = create_work(AxumState(state.clone()), axum::Json(req))
            .await
            .expect("create work");
        let work_id = resp.work_id.clone();

        let patch = nexus_local_db::works::WorkPatch {
            work_ref: Some(Some("test-novel".to_string())),
            ..Default::default()
        };
        let now = chrono::Utc::now().to_rfc3339();
        nexus_local_db::works::patch_work(state.pool(), "test_creator", &work_id, &patch, &now)
            .await
            .expect("patch work_ref");

        let now = chrono::Utc::now().to_rfc3339();
        nexus_local_db::work_chapters::seed_chapters(state.pool(), &work_id, "test-novel", 3, &now)
            .await
            .expect("seed chapters");

        (state, tmp, work_id)
    }

    #[test]
    fn resolve_guarded_path_accepts_inside_and_rejects_escape() {
        let root = tempfile::tempdir().unwrap().path().to_path_buf();
        let nested = root.join("Works/test/Outlines");
        std::fs::create_dir_all(&nested).unwrap();
        let file = nested.join("ch01.md");
        std::fs::write(&file, "x").unwrap();

        assert!(
            resolve_guarded_path(&root, "Works/test/Outlines/ch01.md", true).is_ok(),
            "inside path should be accepted"
        );
        assert!(
            resolve_guarded_path(&root, "../escape.md", true).is_err(),
            "escape path should be rejected: {:?}",
            resolve_guarded_path(&root, "../escape.md", true)
        );
    }

    /// Regression: a sibling directory whose name extends the workspace-root
    /// name (e.g. root `…/creative`, sibling `…/creative-evil`) must NOT pass
    /// the guard via a `..` traversal. A plain string `starts_with` would accept
    /// `…/creative-evil/evil.md` because the string starts with `…/creative`;
    /// `Path::starts_with` compares components and rejects it. Covers both the
    /// read path (`must_exist = true`) and the write path (`must_exist = false`).
    #[test]
    fn resolve_guarded_path_rejects_prefix_confusion_sibling() {
        let base = tempfile::tempdir().unwrap().path().to_path_buf();
        let root = base.join("creative");
        std::fs::create_dir_all(&root).unwrap();
        // Sibling whose name extends the root name.
        let evil_dir = base.join("creative-evil");
        std::fs::create_dir_all(&evil_dir).unwrap();
        std::fs::write(evil_dir.join("evil.md"), "stolen").unwrap();

        // Read path: target resolves into the sibling via `..`.
        assert!(
            resolve_guarded_path(&root, "../creative-evil/evil.md", true).is_err(),
            "prefix-confusion sibling must be rejected on the read path: {:?}",
            resolve_guarded_path(&root, "../creative-evil/evil.md", true)
        );
        // Write path: a creatable target whose nearest-existing parent is the
        // sibling must also be rejected.
        assert!(
            resolve_guarded_path(&root, "../creative-evil/newfile.md", false).is_err(),
            "prefix-confusion sibling must be rejected on the write path: {:?}",
            resolve_guarded_path(&root, "../creative-evil/newfile.md", false)
        );
        // Sanity: a genuine inside-root path still passes (write path, creatable).
        assert!(
            resolve_guarded_path(&root, "Outlines/ch01.md", false).is_ok(),
            "inside-root creatable path should be accepted"
        );
    }

    #[tokio::test]
    async fn list_chapters_returns_summaries() {
        let (state, _tmp, work_id) = setup_chapter_work().await;
        let query = ListChaptersQuery {
            cursor: None,
            limit: None,
            status: None,
        };
        let resp = list_chapters(AxumState(state), AxumPath(work_id), AxumQuery(query))
            .await
            .expect("list chapters");
        assert_eq!(resp.items.len(), 3);
        assert_eq!(resp.pagination.limit, 50);
    }

    /// Regression: `?limit=0` used to reach `chapter_page_meta` and underflow
    /// `limit_us - 1` (panic -> 500). The handler now clamps limit to [1, 100],
    /// so limit=0 becomes 1 and returns a valid page instead of panicking.
    #[tokio::test]
    async fn list_chapters_limit_zero_is_clamped_not_panicked() {
        let (state, _tmp, work_id) = setup_chapter_work().await;
        let query = ListChaptersQuery {
            cursor: None,
            limit: Some(0),
            status: None,
        };
        let resp = list_chapters(AxumState(state), AxumPath(work_id), AxumQuery(query))
            .await
            .expect("limit=0 must be clamped, not panic");
        assert_eq!(resp.pagination.limit, 1);
        assert_eq!(resp.items.len(), 1);
    }

    #[tokio::test]
    async fn list_chapters_keyset_pagination() {
        let (state, _tmp, work_id) = setup_chapter_work().await;

        // First page: limit 2 should return 2 items and a next cursor.
        let first = list_chapters(
            AxumState(state.clone()),
            AxumPath(work_id.clone()),
            AxumQuery(ListChaptersQuery {
                cursor: None,
                limit: Some(2),
                status: None,
            }),
        )
        .await
        .expect("list first page");
        assert_eq!(first.items.len(), 2);
        assert!(first.pagination.has_more);
        let cursor = first
            .pagination
            .next_cursor
            .clone()
            .expect("first page should have next_cursor");
        assert!(
            cursor.starts_with("v2:"),
            "cursor should use v2 keyset encoding"
        );

        // Second page: should return the remaining chapter and no cursor.
        let second = list_chapters(
            AxumState(state),
            AxumPath(work_id),
            AxumQuery(ListChaptersQuery {
                cursor: Some(cursor),
                limit: Some(2),
                status: None,
            }),
        )
        .await
        .expect("list second page");
        assert_eq!(second.items.len(), 1);
        assert!(!second.pagination.has_more);
        assert!(second.pagination.next_cursor.is_none());
    }

    #[tokio::test]
    async fn get_chapter_returns_detail() {
        let (state, _tmp, work_id) = setup_chapter_work().await;
        let resp = get_chapter(
            AxumState(state),
            AxumPath((work_id, "1".to_string())),
            AxumQuery(ChapterContentQuery { volume: None }),
        )
        .await
        .expect("get chapter");
        assert_eq!(resp.chapter, 1);
        assert_eq!(resp.volume, 1);
        assert!(resp.slug.as_deref().is_some_and(|s| s.starts_with("ch01")));
    }

    // Both outline-PUT tests share the global `TEST_UPDATE_OUTLINE_PATH_FAIL`
    // failpoint (one sets it, the other is vulnerable to it). Serialize them so
    // the flag cannot leak across threads under the default multi-threaded test
    // runner — without this, `put_outline_creates_file_and_updates_path` flakes
    // when the DB-failure test's guard is concurrently held.
    #[tokio::test]
    #[serial_test::serial]
    async fn put_outline_creates_file_and_updates_path() {
        let (state, _tmp, work_id) = setup_chapter_work().await;
        let root = state.workspace_path().expect("workspace path");

        let req = PutChapterOutlineRequest {
            content: "# Chapter 1\n\nOutline text.".to_string(),
        };
        let _ = put_chapter_outline(
            AxumState(state.clone()),
            AxumPath((work_id.clone(), "1".to_string())),
            AxumQuery(ChapterContentQuery { volume: None }),
            axum::Json(req),
        )
        .await
        .expect("put outline");

        let file_path = std::path::PathBuf::from(&root)
            .join("Works/test-novel/Outlines/chapters/ch01-outline.md");
        assert!(file_path.exists(), "outline file should be created");

        let resp = get_chapter_outline(
            AxumState(state),
            AxumPath((work_id, "1".to_string())),
            AxumQuery(ChapterContentQuery { volume: None }),
        )
        .await
        .expect("get outline");
        assert!(resp.content.contains("Outline text"));
    }

    #[tokio::test]
    #[serial_test::serial]
    async fn put_outline_db_failure_does_not_write_file() {
        let (state, _tmp, work_id) = setup_chapter_work().await;
        let root = state.workspace_path().expect("workspace path");

        // Enable simulated DB-update failure. The guard disables the failpoint
        // on drop so subsequent tests are unaffected, even if this test panics.
        let _guard = FailpointGuard::enable();

        let req = PutChapterOutlineRequest {
            content: "# Chapter 1\n\nOutline text.".to_string(),
        };
        let result = put_chapter_outline(
            AxumState(state.clone()),
            AxumPath((work_id, "1".to_string())),
            AxumQuery(ChapterContentQuery { volume: None }),
            axum::Json(req),
        )
        .await;

        assert!(result.is_err(), "expected DB failure error");

        let file_path = std::path::PathBuf::from(&root)
            .join("Works/test-novel/Outlines/chapters/ch01-outline.md");
        assert!(
            !file_path.exists(),
            "outline file should not be created when DB update fails"
        );
    }

    #[tokio::test]
    async fn patch_chapter_updates_slug() {
        let (state, _tmp, work_id) = setup_chapter_work().await;
        let req = PatchChapterRequest {
            title: None,
            slug: Some("new-slug".to_string()),
            planned_word_count: None,
            volume: None,
            status: None,
            confirm_structural_edit: None,
            transition_reason: None,
        };
        let resp = patch_chapter(
            AxumState(state),
            AxumPath((work_id, "1".to_string())),
            AxumQuery(ChapterContentQuery { volume: None }),
            axum::Json(req),
        )
        .await
        .expect("patch chapter");
        assert_eq!(resp.slug, Some("new-slug".to_string()));
    }

    /// Regression: `patch_chapter` used to re-fetch the row using the ORIGINAL
    /// query volume after a volume change. The UPDATE committed (row moved to
    /// the new volume) but the re-fetch `WHERE volume = <old>` found nothing and
    /// the handler returned 404 on a successful write. The re-fetch must use the
    /// patched volume.
    #[tokio::test]
    async fn patch_chapter_volume_change_returns_updated_row() {
        let (state, _tmp, work_id) = setup_chapter_work().await;
        let req = PatchChapterRequest {
            title: None,
            slug: None,
            planned_word_count: None,
            volume: Some(2),
            status: None,
            confirm_structural_edit: None,
            transition_reason: None,
        };
        let resp = patch_chapter(
            AxumState(state),
            AxumPath((work_id, "1".to_string())),
            AxumQuery(ChapterContentQuery { volume: None }),
            axum::Json(req),
        )
        .await
        .expect("patch with volume change must not 404 after the write");
        assert_eq!(resp.volume, 2);
    }

    #[tokio::test]
    async fn get_chapter_body_reads_file() {
        let (state, _tmp, work_id) = setup_chapter_work().await;
        let root = state.workspace_path().expect("workspace path");
        let body_path =
            std::path::PathBuf::from(&root).join("Works/test-novel/Stories/ch01-ch01.md");
        std::fs::create_dir_all(body_path.parent().unwrap()).unwrap();
        std::fs::write(&body_path, "body content").unwrap();

        let resp = get_chapter_body(
            AxumState(state),
            AxumPath((work_id, "1".to_string())),
            AxumQuery(ChapterContentQuery { volume: None }),
        )
        .await
        .expect("get body");
        assert_eq!(resp.content, "body content");
        assert!(resp.read_only);
    }

    #[tokio::test]
    async fn get_chapter_body_rejects_oversized_file() {
        // The constant is private to the parent module; access it via the
        // helper below which is also private and uses the same value.
        const TEST_MAX_BYTES: usize = 10 * 1024 * 1024;

        let (state, _tmp, work_id) = setup_chapter_work().await;
        let root = state.workspace_path().expect("workspace path");
        let body_path =
            std::path::PathBuf::from(&root).join("Works/test-novel/Stories/ch01-ch01.md");
        std::fs::create_dir_all(body_path.parent().unwrap()).unwrap();
        let oversized = "x".repeat(TEST_MAX_BYTES + 1);
        std::fs::write(&body_path, oversized).unwrap();

        let result = get_chapter_body(
            AxumState(state),
            AxumPath((work_id, "1".to_string())),
            AxumQuery(ChapterContentQuery { volume: None }),
        )
        .await;

        assert!(result.is_err(), "expected error for oversized body");
        match result {
            Err(NexusApiError::BadRequest { code, .. }) => {
                assert_eq!(code, "chapter_body_too_large");
            }
            Err(other) => panic!("unexpected error: {other:?}"),
            Ok(_) => panic!("expected error"),
        }
    }
}
