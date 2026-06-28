//! Canvas Outline+Timeline Local API handlers (V1.72 P0).
//!
//! Endpoints under `/v1/local/works/{work_id}/outline/*` and
//! `/v1/local/works/{work_id}/timeline/*` expose the Work-level outline
//! structure, chapter metadata, and timeline events. All writes use the
//! `outline_revision:` frontmatter in `Works/<work_ref>/Outlines/outline.md`
//! for optimistic concurrency control.

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug};
use crate::api::path_guard::resolve_guarded_path;
use crate::api::runtime_lock::RuntimeLockGuard;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::{
    OutlinePatchChapterRequest, OutlinePatchResponse, OutlinePatchStructureRequest,
    TimelinePatchEventRequest, WorkOutline, WorkOutlineForeshadow, WorkOutlineTimelineEvent,
    WorkOutlineVolume,
};
use nexus_local_db::work_chapters::{self, PatchChapterParams, WorkChapterRecord};
use nexus_local_db::works;
use std::collections::HashMap;
use std::path::{Path as StdPath, PathBuf};

const OUTLINE_FILE_MAX_BYTES: usize = 10 * 1024 * 1024;

// ─── Internal frontmatter model ─────────────────────────────────────────────

/// In-memory representation of the work outline markdown frontmatter.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
struct OutlineFrontmatter {
    outline_revision: i64,
    volumes: Vec<WorkOutlineVolume>,
    timeline_events: Vec<WorkOutlineTimelineEvent>,
    foreshadows: Vec<WorkOutlineForeshadow>,
    chapter_titles: HashMap<String, String>,
    updated_at: String,
}

impl OutlineFrontmatter {
    /// Convert the frontmatter into the public `WorkOutline` contract DTO.
    fn to_work_outline(&self, work_id: String) -> WorkOutline {
        WorkOutline {
            work_id,
            outline_revision: self
                .outline_revision_u64()
                .expect("outline_revision is kept non-negative by the patch handlers"),
            volumes: self.volumes.clone(),
            timeline_events: self.timeline_events.clone(),
            foreshadows: self.foreshadows.clone(),
            chapter_titles: self.chapter_titles.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    /// Return `outline_revision` as `u64` for wire contracts that use unsigned
    /// integers. This is an internal invariant; a negative value is a bug.
    fn outline_revision_u64(&self) -> Result<u64, NexusApiError> {
        u64::try_from(self.outline_revision).map_err(|_| NexusApiError::Internal {
            code: "OUTLINE_REVISION_NEGATIVE".to_string(),
            message: "outline_revision became negative".to_string(),
        })
    }
}

// ─── Shared helpers ─────────────────────────────────────────────────────────

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

/// Canonical relative path for the work-level outline markdown.
fn outline_rel_path(work_ref: &str) -> String {
    format!("Works/{work_ref}/Outlines/outline.md")
}

/// Resolve the filesystem-safe Work reference.
///
/// Prefer the dedicated `work_ref` column; fall back to `story_ref` so tests
/// and legacy flows that only set `story_ref` can still open the outline file.
fn resolve_work_ref(work: &works::WorkRecord) -> Result<String, NexusApiError> {
    work.work_ref
        .clone()
        .or_else(|| work.story_ref.clone())
        .ok_or_else(|| NexusApiError::Internal {
            code: "WORK_REF_MISSING".to_string(),
            message: format!("work {} has no work_ref or story_ref", work.work_id),
        })
}

/// Split a markdown file into its YAML frontmatter block and body.
///
/// Returns `None` when the file does not start with a `---` delimiter.
fn split_frontmatter(content: &str) -> Option<(String, String)> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_open = &trimmed[3..];
    // Find the closing `---` on its own line: it must be followed by `\n` or be
    // at end-of-string. Matching bare `\n---` would also accept substrings like
    // `\n---more` or an unquoted YAML block scalar line starting with `---`,
    // splitting the frontmatter prematurely (R-V172-GREPTILE-004).
    let (end, skip) = if let Some(idx) = after_open.find("\n---\n") {
        (idx, 5)
    } else if let Some(idx) = after_open.find("\n---") {
        if idx + 4 == after_open.len() {
            (idx, 4)
        } else {
            // `---` is not on its own line (e.g. `\n---more`); malformed.
            return None;
        }
    } else {
        return None;
    };
    let yaml = after_open[..end].to_string();
    let body = after_open[end + skip..]
        .trim_start_matches('\n')
        .to_string();
    Some((yaml, body))
}

/// Read the work outline file after path-guard verification.
///
/// If the file is missing or has no frontmatter, a default frontmatter is
/// returned along with the original body (or an empty body when missing).
async fn read_outline_file(
    workspace_root: &StdPath,
    rel_path: &str,
    chapters: &[WorkChapterRecord],
) -> Result<(OutlineFrontmatter, String), NexusApiError> {
    // Use must_exist=false so a missing outline file is treated as a default
    // frontmatter rather than a path-guard error. The guard still verifies the
    // resolved path would live inside the workspace root.
    let path = resolve_guarded_path(workspace_root, rel_path, false).map_err(|e| {
        if matches!(e, NexusApiError::BadRequest { ref code, .. } if code == "chapter_path_forbidden")
        {
            NexusApiError::BadRequest {
                code: "outline_path_forbidden".to_string(),
                message: format!("outline path '{rel_path}' escapes workspace root"),
            }
        } else {
            e
        }
    })?;

    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let now = chrono::Utc::now().to_rfc3339();
            return Ok((default_frontmatter(&now, chapters), String::new()));
        }
        Err(e) => {
            return Err(NexusApiError::Internal {
                code: "FILE_READ_ERROR".to_string(),
                message: format!("failed to read outline '{rel_path}': {e}"),
            });
        }
    };

    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "FILE_READ_ERROR".to_string(),
            message: format!("failed to read outline metadata '{rel_path}': {e}"),
        })?;
    let max_bytes = u64::try_from(OUTLINE_FILE_MAX_BYTES).unwrap_or(u64::MAX);
    if metadata.len() > max_bytes {
        return Err(NexusApiError::BadRequest {
            code: "outline_file_too_large".to_string(),
            message: format!("outline file '{rel_path}' exceeds {OUTLINE_FILE_MAX_BYTES} bytes"),
        });
    }

    let Some((yaml, body)) = split_frontmatter(&content) else {
        let now = chrono::Utc::now().to_rfc3339();
        return Ok((default_frontmatter(&now, chapters), content));
    };

    let frontmatter: OutlineFrontmatter =
        serde_yaml::from_str(&yaml).map_err(|e| NexusApiError::BadRequest {
            code: "outline_frontmatter_invalid".to_string(),
            message: format!("failed to parse outline frontmatter: {e}"),
        })?;

    Ok((frontmatter, body))
}

/// Build a default frontmatter from the current `work_chapters` rows.
fn default_frontmatter(now: &str, chapters: &[WorkChapterRecord]) -> OutlineFrontmatter {
    let mut ids: Vec<i64> = chapters.iter().map(|r| i64::from(r.chapter)).collect();
    ids.sort_unstable();
    let volume = WorkOutlineVolume {
        volume_id: 1,
        label: "Volume 1".to_string(),
        chapter_ids: ids,
    };
    OutlineFrontmatter {
        outline_revision: 0,
        volumes: vec![volume],
        timeline_events: Vec::new(),
        foreshadows: Vec::new(),
        chapter_titles: HashMap::new(),
        updated_at: now.to_string(),
    }
}

/// Atomically write the outline frontmatter + preserved body to disk.
async fn atomic_write_outline(
    workspace_root: &StdPath,
    rel_path: &str,
    frontmatter: &OutlineFrontmatter,
    body: &str,
) -> Result<(), NexusApiError> {
    let target = resolve_guarded_path(workspace_root, rel_path, false)?;

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DIRECTORY_CREATE_ERROR".to_string(),
                message: format!("failed to create outline parent dirs: {e}"),
            })?;
    }

    let yaml = serde_yaml::to_string(frontmatter).map_err(|e| NexusApiError::Internal {
        code: "OUTLINE_SERIALIZE_ERROR".to_string(),
        message: format!("failed to serialize outline frontmatter: {e}"),
    })?;
    let content = format!("---\n{yaml}---\n{body}");

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
        let final_file = tokio::fs::File::open(&target).await?;
        final_file.sync_all().await?;
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
            message: format!("failed to write outline '{rel_path}': {e}"),
        });
    }

    Ok(())
}

/// Validate a chapter status transition using the V1.65 lifecycle vocabulary.
fn validate_status_transition(from: &str, to: &str) -> Result<(), NexusApiError> {
    if from == to {
        return Ok(());
    }
    match (from, to) {
        ("not_started", "outlined" | "draft" | "finalized")
        | ("outlined", "draft" | "finalized")
        | ("draft", "finalized") => Ok(()),
        _ => Err(NexusApiError::BadRequest {
            code: "chapter_status_transition_invalid".to_string(),
            message: format!(
                "status transition '{from}' -> '{to}' is not allowed through this endpoint"
            ),
        }),
    }
}

/// Build a successful patch response with optional side effects.
fn patch_ok(new_revision: i64, side_effects: Vec<String>) -> OutlinePatchResponse {
    OutlinePatchResponse {
        new_revision,
        validation_summary: serde_json::json!({
            "errors": Vec::<String>::new(),
            "warnings": Vec::<String>::new(),
        }),
        side_effects: Some(side_effects),
    }
}

// simplify: V1.72 does not yet implement full graph validation; we guard the
// most critical invariants (id existence, revision, published protection) and
// leave acyclic / foreshadow-order checks for a future slice.

// ─── Handlers ───────────────────────────────────────────────────────────────

/// `GET /v1/local/works/{work_id}/outline` — canonical work outline + timeline.
pub async fn get_work_outline(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
) -> Result<Json<WorkOutline>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let work = load_work(&state, &creator_id, &work_id).await?;
    let work_ref = resolve_work_ref(&work)?;

    let chapters = work_chapters::list_chapters(state.pool(), &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    let workspace_root = workspace_root(&state)?;
    let rel_path = outline_rel_path(&work_ref);
    let (frontmatter, _body) = read_outline_file(&workspace_root, &rel_path, &chapters).await?;

    Ok(Json(frontmatter.to_work_outline(work_id)))
}

/// `POST /v1/local/works/{work_id}/outline/patch` — structured outline patch.
pub async fn patch_outline_structure(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<OutlinePatchStructureRequest>,
) -> Result<Json<OutlinePatchResponse>, NexusApiError> {
    if req.work_id != work_id {
        return Err(NexusApiError::BadRequest {
            code: "work_id_mismatch".to_string(),
            message: "request work_id must match URL path".to_string(),
        });
    }

    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let work = load_work(&state, &creator_id, &work_id).await?;
    let work_ref = resolve_work_ref(&work)?;

    let workspace_root = workspace_root(&state)?;
    let rel_path = outline_rel_path(&work_ref);

    // Pre-load chapter rows for defaulting and validation.
    let chapters = work_chapters::list_chapters(state.pool(), &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    let initial_frontmatter = read_outline_file(&workspace_root, &rel_path, &chapters)
        .await?
        .0;

    let base_revision =
        i64::try_from(req.base_revision).map_err(|_| NexusApiError::BadRequest {
            code: "base_revision_out_of_range".to_string(),
            message: "base_revision exceeds i64 range".to_string(),
        })?;
    if base_revision != initial_frontmatter.outline_revision {
        return Err(NexusApiError::outline_conflict(
            initial_frontmatter.outline_revision_u64()?,
            req.chapter_id
                .map_or_else(|| work_id.clone(), |n| n.to_string()),
            "outline_revision",
            "refetch the work outline and reapply",
        ));
    }

    let lock = RuntimeLockGuard::acquire(state.pool(), &creator_id, &work_id).await?;

    // Re-read both frontmatter and body under lock to close the TOCTOU window
    // for concurrent writers and avoid persisting a stale body snapshot.
    let (mut frontmatter, body) = read_outline_file(&workspace_root, &rel_path, &chapters).await?;
    if base_revision != frontmatter.outline_revision {
        lock.release().await;
        return Err(NexusApiError::outline_conflict(
            frontmatter.outline_revision_u64()?,
            req.chapter_id
                .map_or_else(|| work_id.clone(), |n| n.to_string()),
            "outline_revision",
            "refetch the work outline and reapply",
        ));
    }

    let result = apply_structure_patch(&state, &work_id, &req, &mut frontmatter, &chapters).await;
    if let Err(e) = &result {
        lock.release().await;
        return Err(e.clone());
    }

    let now = chrono::Utc::now().to_rfc3339();
    frontmatter.outline_revision += 1;
    frontmatter.updated_at = now;

    if let Err(e) = atomic_write_outline(&workspace_root, &rel_path, &frontmatter, &body).await {
        lock.release().await;
        return Err(e);
    }

    lock.release().await;
    Ok(Json(patch_ok(frontmatter.outline_revision, Vec::new())))
}

/// `POST /v1/local/works/{work_id}/chapters/{chapter_id}/patch` — outline chapter patch.
pub async fn patch_outline_chapter(
    State(state): State<WorkspaceState>,
    Path((work_id, n)): Path<(String, String)>,
    Json(req): Json<OutlinePatchChapterRequest>,
) -> Result<Json<OutlinePatchResponse>, NexusApiError> {
    if req.work_id != work_id {
        return Err(NexusApiError::BadRequest {
            code: "work_id_mismatch".to_string(),
            message: "request work_id must match URL path".to_string(),
        });
    }

    let chapter = n.parse::<i32>().map_err(|_| NexusApiError::BadRequest {
        code: "invalid_chapter_number".to_string(),
        message: format!("chapter number must be a positive integer, got '{n}'"),
    })?;
    if chapter < 1 {
        return Err(NexusApiError::BadRequest {
            code: "invalid_chapter_number".to_string(),
            message: format!("chapter number must be >= 1, got {chapter}"),
        });
    }
    if req.chapter_id != i64::from(chapter) {
        return Err(NexusApiError::BadRequest {
            code: "chapter_id_mismatch".to_string(),
            message: "request chapter_id must match URL path".to_string(),
        });
    }

    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let work = load_work(&state, &creator_id, &work_id).await?;
    let work_ref = resolve_work_ref(&work)?;

    let workspace_root = workspace_root(&state)?;
    let rel_path = outline_rel_path(&work_ref);

    let record = work_chapters::get_chapter(state.pool(), &work_id, chapter, 1)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("chapter {chapter}")))?;

    let chapters = work_chapters::list_chapters(state.pool(), &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    let initial_frontmatter = read_outline_file(&workspace_root, &rel_path, &chapters)
        .await?
        .0;

    let base_revision =
        i64::try_from(req.base_revision).map_err(|_| NexusApiError::BadRequest {
            code: "base_revision_out_of_range".to_string(),
            message: "base_revision exceeds i64 range".to_string(),
        })?;
    if base_revision != initial_frontmatter.outline_revision {
        return Err(NexusApiError::outline_conflict(
            initial_frontmatter.outline_revision_u64()?,
            chapter.to_string(),
            "outline_revision",
            "refetch the work outline and reapply",
        ));
    }

    // Protect published chapters from any canvas metadata edit in V1.72.
    if record.status == "published" && has_chapter_structural_edit(&req) {
        return Err(NexusApiError::BadRequest {
            code: "chapter_structure_edit_blocked".to_string(),
            message: "structural edits to published chapters are blocked".to_string(),
        });
    }

    let lock = RuntimeLockGuard::acquire(state.pool(), &creator_id, &work_id).await?;

    // Re-read both frontmatter and body under lock to close the TOCTOU window
    // for concurrent writers and avoid persisting a stale body snapshot.
    let (mut frontmatter, body) = read_outline_file(&workspace_root, &rel_path, &chapters).await?;
    if base_revision != frontmatter.outline_revision {
        lock.release().await;
        return Err(NexusApiError::outline_conflict(
            frontmatter.outline_revision_u64()?,
            chapter.to_string(),
            "outline_revision",
            "refetch the work outline and reapply",
        ));
    }

    let result =
        apply_chapter_patch(&state, &work_id, chapter, &record, &req, &mut frontmatter).await;
    if let Err(e) = &result {
        lock.release().await;
        return Err(e.clone());
    }

    let now = chrono::Utc::now().to_rfc3339();
    frontmatter.outline_revision += 1;
    frontmatter.updated_at = now;

    if let Err(e) = atomic_write_outline(&workspace_root, &rel_path, &frontmatter, &body).await {
        lock.release().await;
        return Err(e);
    }

    lock.release().await;
    Ok(Json(patch_ok(frontmatter.outline_revision, Vec::new())))
}

/// `POST /v1/local/works/{work_id}/timeline/patch` — structured timeline patch.
pub async fn patch_timeline_event(
    State(state): State<WorkspaceState>,
    Path(work_id): Path<String>,
    Json(req): Json<TimelinePatchEventRequest>,
) -> Result<Json<OutlinePatchResponse>, NexusApiError> {
    if req.work_id != work_id {
        return Err(NexusApiError::BadRequest {
            code: "work_id_mismatch".to_string(),
            message: "request work_id must match URL path".to_string(),
        });
    }

    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::AuthRequired)?;
    let _workspace_slug = read_active_workspace_slug(state.nexus_home(), &creator_id)
        .ok_or(NexusApiError::AuthRequired)?;

    let work = load_work(&state, &creator_id, &work_id).await?;
    let work_ref = resolve_work_ref(&work)?;

    let workspace_root = workspace_root(&state)?;
    let rel_path = outline_rel_path(&work_ref);

    let chapters = work_chapters::list_chapters(state.pool(), &work_id)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;

    let initial_frontmatter = read_outline_file(&workspace_root, &rel_path, &chapters)
        .await?
        .0;

    let base_revision =
        i64::try_from(req.base_revision).map_err(|_| NexusApiError::BadRequest {
            code: "base_revision_out_of_range".to_string(),
            message: "base_revision exceeds i64 range".to_string(),
        })?;
    if base_revision != initial_frontmatter.outline_revision {
        return Err(NexusApiError::outline_conflict(
            initial_frontmatter.outline_revision_u64()?,
            req.event_id.clone().unwrap_or_else(|| work_id.clone()),
            "outline_revision",
            "refetch the work outline and reapply",
        ));
    }

    let lock = RuntimeLockGuard::acquire(state.pool(), &creator_id, &work_id).await?;

    // Re-read both frontmatter and body under lock to close the TOCTOU window
    // for concurrent writers and avoid persisting a stale body snapshot.
    let (mut frontmatter, body) = read_outline_file(&workspace_root, &rel_path, &chapters).await?;
    if base_revision != frontmatter.outline_revision {
        lock.release().await;
        return Err(NexusApiError::outline_conflict(
            frontmatter.outline_revision_u64()?,
            req.event_id.clone().unwrap_or_else(|| work_id.clone()),
            "outline_revision",
            "refetch the work outline and reapply",
        ));
    }

    let result = apply_timeline_patch(&req, &mut frontmatter, &chapters);
    if let Err(e) = &result {
        lock.release().await;
        return Err(e.clone());
    }

    let now = chrono::Utc::now().to_rfc3339();
    frontmatter.outline_revision += 1;
    frontmatter.updated_at = now;

    if let Err(e) = atomic_write_outline(&workspace_root, &rel_path, &frontmatter, &body).await {
        lock.release().await;
        return Err(e);
    }

    lock.release().await;
    Ok(Json(patch_ok(frontmatter.outline_revision, Vec::new())))
}

// ─── Patch application logic ────────────────────────────────────────────────

async fn apply_structure_patch(
    state: &WorkspaceState,
    work_id: &str,
    req: &OutlinePatchStructureRequest,
    frontmatter: &mut OutlineFrontmatter,
    chapters: &[WorkChapterRecord],
) -> Result<(), NexusApiError> {
    let operation = req.operation.as_str();
    match operation {
        "move_chapter" | "attach_to_volume" => {
            let chapter_id = req.chapter_id.ok_or_else(|| NexusApiError::BadRequest {
                code: "missing_chapter_id".to_string(),
                message: format!("{operation} requires chapter_id"),
            })?;
            let volume_id = req.volume_id.ok_or_else(|| NexusApiError::BadRequest {
                code: "missing_volume_id".to_string(),
                message: format!("{operation} requires volume_id"),
            })?;
            let volume_id_i32 = i32::try_from(volume_id).unwrap_or(1);

            ensure_chapter_exists(chapters, chapter_id)?;

            // Update the DB volume binding so `work_chapters` stays SSOT.
            let now = chrono::Utc::now().to_rfc3339();
            let patch = PatchChapterParams {
                volume: Some(volume_id_i32),
                ..Default::default()
            };
            let chapter_id_i32 =
                i32::try_from(chapter_id).map_err(|_| NexusApiError::BadRequest {
                    code: "invalid_chapter_id".to_string(),
                    message: format!("chapter_id {chapter_id} out of range"),
                })?;
            work_chapters::patch_chapter(state.pool(), work_id, chapter_id_i32, 1, &patch, &now)
                .await
                .map_err(|e| NexusApiError::Internal {
                    code: "DATABASE_ERROR".to_string(),
                    message: e.to_string(),
                })?;

            // Re-sync the outline volume ordering.
            move_chapter_in_frontmatter(frontmatter, chapter_id, volume_id, chapters);
            Ok(())
        }
        "link_event" => {
            let event_id = req
                .event_id
                .as_deref()
                .ok_or_else(|| NexusApiError::BadRequest {
                    code: "missing_event_id".to_string(),
                    message: "link_event requires event_id".to_string(),
                })?;
            let target = req
                .target_chapter_id
                .ok_or_else(|| NexusApiError::BadRequest {
                    code: "missing_target_chapter_id".to_string(),
                    message: "link_event requires target_chapter_id".to_string(),
                })?;
            ensure_chapter_exists(chapters, target)?;

            let event = frontmatter
                .timeline_events
                .iter_mut()
                .find(|e| e.event_id == event_id)
                .ok_or_else(|| NexusApiError::NotFound(format!("event {event_id}")))?;
            event.realizes_chapter_id = Some(target);
            Ok(())
        }
        _ => Err(NexusApiError::BadRequest {
            code: "invalid_outline_operation".to_string(),
            message: format!("unsupported outline operation '{operation}'"),
        }),
    }
}

fn move_chapter_in_frontmatter(
    frontmatter: &mut OutlineFrontmatter,
    chapter_id: i64,
    volume_id: i64,
    chapters: &[WorkChapterRecord],
) {
    // Remove the chapter from all existing volumes.
    for vol in &mut frontmatter.volumes {
        vol.chapter_ids.retain(|id| *id != chapter_id);
    }
    frontmatter
        .volumes
        .retain(|vol| !vol.chapter_ids.is_empty());

    // Append to the target volume, creating it if necessary.
    let target = frontmatter
        .volumes
        .iter_mut()
        .find(|vol| vol.volume_id == volume_id);
    if let Some(vol) = target {
        if !vol.chapter_ids.contains(&chapter_id) {
            vol.chapter_ids.push(chapter_id);
        }
    } else {
        frontmatter.volumes.push(WorkOutlineVolume {
            volume_id,
            label: format!("Volume {volume_id}"),
            chapter_ids: vec![chapter_id],
        });
    }

    // Ensure every chapter still appears somewhere; missing ones land in volume 1.
    let mut present: std::collections::HashSet<i64> = frontmatter
        .volumes
        .iter()
        .flat_map(|vol| vol.chapter_ids.clone())
        .collect();

    // Resolve the volume-1 slot once, creating it if absent, to avoid overlapping
    // mutable borrows inside the loop.
    let vol1_idx = if let Some(idx) = frontmatter
        .volumes
        .iter()
        .position(|vol| vol.volume_id == 1)
    {
        idx
    } else {
        frontmatter.volumes.push(WorkOutlineVolume {
            volume_id: 1,
            label: "Volume 1".to_string(),
            chapter_ids: Vec::new(),
        });
        frontmatter.volumes.len() - 1
    };

    for record in chapters {
        let id = i64::from(record.chapter);
        if present.insert(id) {
            frontmatter.volumes[vol1_idx].chapter_ids.push(id);
        }
    }

    // Drop the volume-1 placeholder if no chapters actually landed there.
    frontmatter
        .volumes
        .retain(|vol| !vol.chapter_ids.is_empty());

    // Sort each volume's chapter list by chapter number for stable ordering.
    for vol in &mut frontmatter.volumes {
        vol.chapter_ids.sort_unstable();
    }
    frontmatter.volumes.sort_by_key(|vol| vol.volume_id);
}

fn ensure_chapter_exists(
    chapters: &[WorkChapterRecord],
    chapter_id: i64,
) -> Result<(), NexusApiError> {
    if chapters.iter().any(|r| i64::from(r.chapter) == chapter_id) {
        Ok(())
    } else {
        Err(NexusApiError::NotFound(format!("chapter {chapter_id}")))
    }
}

const fn has_chapter_structural_edit(req: &OutlinePatchChapterRequest) -> bool {
    req.set.title.is_some()
        || req.set.slug.is_some()
        || req.set.planned_word_count.is_some()
        || req.set.actual_word_count.is_some()
        || req.set.volume.is_some()
        || req.set.status.is_some()
}

async fn apply_chapter_patch(
    state: &WorkspaceState,
    work_id: &str,
    chapter: i32,
    record: &WorkChapterRecord,
    req: &OutlinePatchChapterRequest,
    frontmatter: &mut OutlineFrontmatter,
) -> Result<(), NexusApiError> {
    if let Some(ref status) = req.set.status {
        validate_status_transition(&record.status, status)?;
    }

    let has_volume_change = req.set.volume.is_some();
    let patch = PatchChapterParams {
        slug: req.set.slug.clone(),
        planned_word_count: req
            .set
            .planned_word_count
            .map(i32::try_from)
            .transpose()
            .map_err(|_| NexusApiError::BadRequest {
                code: "planned_word_count_too_large".to_string(),
                message: "planned_word_count exceeds i32 range".to_string(),
            })?,
        volume: req.set.volume.map(i32::try_from).transpose().map_err(|_| {
            NexusApiError::BadRequest {
                code: "invalid_volume".to_string(),
                message: "volume exceeds i32 range".to_string(),
            }
        })?,
        status: req.set.status.clone(),
    };

    // Persist slug/wc/volume/status to the chapter SSOT table.
    let now = chrono::Utc::now().to_rfc3339();
    work_chapters::patch_chapter(
        state.pool(),
        work_id,
        chapter,
        record.volume.unwrap_or(1),
        &patch,
        &now,
    )
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    // Persist the UI-facing title in the outline frontmatter.
    if let Some(title) = req.set.title.clone() {
        frontmatter
            .chapter_titles
            .insert(chapter.to_string(), title);
    }

    // Re-sync volume ordering when the volume binding changed.
    if has_volume_change {
        let new_volume = req
            .set
            .volume
            .unwrap_or_else(|| i64::from(record.volume.unwrap_or(1)));
        let chapters = work_chapters::list_chapters(state.pool(), work_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;
        move_chapter_in_frontmatter(frontmatter, i64::from(chapter), new_volume, &chapters);
    }

    Ok(())
}

fn apply_timeline_patch(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
    chapters: &[WorkChapterRecord],
) -> Result<(), NexusApiError> {
    match req.operation.as_str() {
        "add_event" => timeline_add_event(req, frontmatter, chapters),
        "remove_event" => timeline_remove_event(req, frontmatter),
        "attach_event_to_chapter" => timeline_attach_event_to_chapter(req, frontmatter, chapters),
        "link_foreshadow" => timeline_link_foreshadow(req, frontmatter),
        operation => Err(NexusApiError::BadRequest {
            code: "invalid_timeline_operation".to_string(),
            message: format!("unsupported timeline operation '{operation}'"),
        }),
    }
}

fn timeline_add_event(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
    chapters: &[WorkChapterRecord],
) -> Result<(), NexusApiError> {
    let title = req.title.clone().ok_or_else(|| NexusApiError::BadRequest {
        code: "missing_event_title".to_string(),
        message: "add_event requires title".to_string(),
    })?;
    if let Some(chapter_id) = req.realizes_chapter_id {
        ensure_chapter_exists(chapters, chapter_id)?;
    }
    let event_id = format!("evt_{}", uuid::Uuid::new_v4());
    frontmatter.timeline_events.push(WorkOutlineTimelineEvent {
        event_id,
        title,
        description: req.description.clone(),
        realizes_chapter_id: req.realizes_chapter_id,
    });
    Ok(())
}

fn timeline_remove_event(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
) -> Result<(), NexusApiError> {
    let event_id = req
        .event_id
        .as_deref()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "missing_event_id".to_string(),
            message: "remove_event requires event_id".to_string(),
        })?;
    let before = frontmatter.timeline_events.len();
    frontmatter
        .timeline_events
        .retain(|e| e.event_id != event_id);
    if frontmatter.timeline_events.len() == before {
        return Err(NexusApiError::NotFound(format!("event {event_id}")));
    }
    // Also drop foreshadow edges touching this event.
    frontmatter
        .foreshadows
        .retain(|edge| edge.source_event_id != event_id && edge.target_event_id != event_id);
    Ok(())
}

fn timeline_attach_event_to_chapter(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
    chapters: &[WorkChapterRecord],
) -> Result<(), NexusApiError> {
    let event_id = req
        .event_id
        .as_deref()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "missing_event_id".to_string(),
            message: "attach_event_to_chapter requires event_id".to_string(),
        })?;
    let target = req
        .target_chapter_id
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "missing_target_chapter_id".to_string(),
            message: "attach_event_to_chapter requires target_chapter_id".to_string(),
        })?;
    ensure_chapter_exists(chapters, target)?;
    let event = frontmatter
        .timeline_events
        .iter_mut()
        .find(|e| e.event_id == event_id)
        .ok_or_else(|| NexusApiError::NotFound(format!("event {event_id}")))?;
    event.realizes_chapter_id = Some(target);
    Ok(())
}

fn timeline_link_foreshadow(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
) -> Result<(), NexusApiError> {
    let source = req
        .event_id
        .as_deref()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "missing_event_id".to_string(),
            message: "link_foreshadow requires event_id".to_string(),
        })?;
    let target = req
        .foreshadows_event_id
        .as_deref()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "missing_foreshadows_event_id".to_string(),
            message: "link_foreshadow requires foreshadows_event_id".to_string(),
        })?;
    if !frontmatter
        .timeline_events
        .iter()
        .any(|e| e.event_id == source)
    {
        return Err(NexusApiError::NotFound(format!("event {source}")));
    }
    if !frontmatter
        .timeline_events
        .iter()
        .any(|e| e.event_id == target)
    {
        return Err(NexusApiError::NotFound(format!("event {target}")));
    }
    if !frontmatter
        .foreshadows
        .iter()
        .any(|edge| edge.source_event_id == source && edge.target_event_id == target)
    {
        frontmatter.foreshadows.push(WorkOutlineForeshadow {
            source_event_id: source.to_string(),
            target_event_id: target.to_string(),
        });
    }
    Ok(())
}

// ─── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_frontmatter_parses_delimited_block() {
        let content = "---\noutline_revision: 3\nvolumes: []\ntimeline_events: []\nforeshadows: []\nchapter_titles: {}\nupdated_at: \"2024-01-01T00:00:00Z\"\n---\n# Body\n";
        let (yaml, body) = split_frontmatter(content).unwrap();
        assert!(yaml.contains("outline_revision: 3"));
        assert_eq!(body, "# Body\n");
    }

    #[test]
    fn split_frontmatter_returns_none_without_delimiter() {
        assert!(split_frontmatter("# Just body").is_none());
    }

    /// Regression for R-V172-GREPTILE-004.
    ///
    /// A YAML block scalar line starting with `---` (indented) must not be
    /// mistaken for the closing delimiter. The real closing `---` on its own
    /// line must still be found.
    #[test]
    fn split_frontmatter_rejects_non_delimiter_dashes() {
        // The `  ---` line is inside the `body_intro` block scalar, not a
        // delimiter. The bare `---` line closes the frontmatter.
        let content = "---\ntitle: \"hello\"\nbody_intro: |\n  ---\n  multi-line\n---\nactual body";
        let (yaml, body) = split_frontmatter(content).expect("valid frontmatter should parse");
        assert!(
            yaml.contains("body_intro"),
            "yaml should keep the block scalar: {yaml}"
        );
        assert_eq!(body, "actual body");
    }

    /// Regression for R-V172-GREPTILE-004.
    ///
    /// `\n---more` is not a bare delimiter line and must be rejected rather
    /// than splitting the frontmatter at the inline dashes.
    #[test]
    fn split_frontmatter_rejects_inline_dashes() {
        let content = "---\ntitle: test\n---more\nbody";
        assert!(
            split_frontmatter(content).is_none(),
            "inline `---more` should not match as a delimiter"
        );
    }

    /// Regression for R-V172-GREPTILE-004.
    ///
    /// A closing delimiter at end-of-string (no body, no trailing newline) is
    /// a valid bare delimiter line and must still parse.
    #[test]
    fn split_frontmatter_accepts_trailing_delimiter_without_body() {
        let content = "---\ntitle: test\n---";
        let (yaml, body) = split_frontmatter(content).expect("trailing delimiter should parse");
        assert!(yaml.contains("title: test"));
        assert_eq!(body, "");
    }

    #[test]
    fn validate_status_transition_allows_forward_moves() {
        assert!(validate_status_transition("not_started", "outlined").is_ok());
        assert!(validate_status_transition("outlined", "draft").is_ok());
        assert!(validate_status_transition("draft", "finalized").is_ok());
        assert!(validate_status_transition("not_started", "not_started").is_ok());
    }

    #[test]
    fn validate_status_transition_rejects_reverse_and_published() {
        assert!(validate_status_transition("finalized", "draft").is_err());
        assert!(validate_status_transition("not_started", "published").is_err());
    }

    /// Regression test for R-V172P0-QC3-001.
    ///
    /// Simulates a concurrent writer changing the outline body between the
    /// early (pre-lock) read and the locked re-read. The handler must persist
    /// the body that was present at locked-read time, not the stale snapshot
    /// from the early read.
    #[tokio::test]
    async fn patch_write_uses_body_from_locked_re_read() {
        use crate::api::handlers::works::{CreateWorkRequest, PatchWorkRequest};

        let (tmp, nexus_home, db_path, workspace_dir) =
            crate::test_utils::create_initialized_test_workspace().await;
        let state = WorkspaceState::new_for_testing(
            nexus_home,
            db_path,
            Some(workspace_dir.to_string_lossy().to_string()),
        )
        .await;
        crate::test_utils::seed_test_creator_and_world(state.pool()).await;

        let work_id = {
            let req = CreateWorkRequest {
                title: "Outline Test Novel".to_string(),
                long_term_goal: "Test the outline canvas".to_string(),
                initial_idea: "A test story".to_string(),
                world_id: Some("wld_test_world".to_string()),
                story_ref: None,
                primary_preset_id: None,
                lineage_from_work_id: None,
                client_request_id: None,
                set_pool_active: None,
                work_profile: None,
            };
            let (_status, axum::Json(resp)) = crate::api::handlers::works::create_work(
                axum::extract::State(state.clone()),
                axum::Json(req),
            )
            .await
            .unwrap();
            resp.work_id
        };

        // Set the story_ref so the outline file path is deterministic.
        {
            let req = PatchWorkRequest {
                title: None,
                long_term_goal: None,
                creative_brief: None,
                intake_status: None,
                status: None,
                world_id: None,
                story_ref: Some(Some("outline-test-novel".to_string())),
                primary_preset_id: None,
                current_stage: None,
                stage_status: None,
                force: None,
                auto_review_master_on_timeout: None,
                auto_chain_interrupted: None,
                work_profile: None,
            };
            let _ = crate::api::handlers::works::patch_work(
                axum::extract::State(state.clone()),
                axum::extract::Path(work_id.clone()),
                axum::Json(req),
            )
            .await
            .unwrap();
        }

        // Seed a single chapter so default frontmatter / volume moves work.
        let now = chrono::Utc::now().to_rfc3339();
        nexus_local_db::work_chapters::insert_chapter(
            state.pool(),
            &nexus_local_db::work_chapters::InsertChapterParams {
                work_id: &work_id,
                chapter: 1,
                volume: Some(1),
                slug: Some("ch01"),
                planned_word_count: 4000,
                outline_path: None,
                body_path: None,
                now: &now,
            },
        )
        .await
        .expect("seed chapter");

        let workspace_root = workspace_dir;
        let rel_path = "Works/outline-test-novel/Outlines/outline.md";
        let outline_path = workspace_root.join(rel_path);
        tokio::fs::create_dir_all(outline_path.parent().unwrap())
            .await
            .expect("create outline dirs");

        let stale_body = "stale body\n";
        tokio::fs::write(
            &outline_path,
            format!(
                "---\noutline_revision: 0\nvolumes: []\ntimeline_events: []\nforeshadows: []\nchapter_titles: {{}}\nupdated_at: \"2024-01-01T00:00:00Z\"\n---\n{stale_body}"
            ),
        )
        .await
        .expect("write initial outline");

        let chapters = work_chapters::list_chapters(state.pool(), &work_id)
            .await
            .expect("list chapters");

        // Pre-lock read (old bug would capture this body for the later write).
        let (_initial_frontmatter, _stale_body) =
            read_outline_file(&workspace_root, rel_path, &chapters)
                .await
                .expect("early read");

        // Concurrent writer changes the body before the lock is acquired.
        let fresh_body = "fresh body\n";
        tokio::fs::write(
            &outline_path,
            format!(
                "---\noutline_revision: 0\nvolumes: []\ntimeline_events: []\nforeshadows: []\nchapter_titles: {{}}\nupdated_at: \"2024-01-01T00:00:00Z\"\n---\n{fresh_body}"
            ),
        )
        .await
        .expect("write concurrent outline body");

        // Locked re-read must observe the fresh body; the subsequent write uses it.
        let (mut frontmatter, body) = read_outline_file(&workspace_root, rel_path, &chapters)
            .await
            .expect("locked re-read");
        assert_eq!(body, fresh_body);

        // Apply a minimal mutation and bump the revision exactly as the handler does.
        frontmatter.outline_revision += 1;
        frontmatter.updated_at = chrono::Utc::now().to_rfc3339();
        atomic_write_outline(&workspace_root, rel_path, &frontmatter, &body)
            .await
            .expect("write outline");

        // The file on disk must contain the fresh body, not the stale snapshot.
        let final_content = tokio::fs::read_to_string(&outline_path).await.unwrap();
        assert!(
            final_content.contains(fresh_body),
            "final outline should contain the fresh body; got: {final_content}"
        );
        assert!(
            !final_content.contains(stale_body),
            "final outline should not contain the stale body; got: {final_content}"
        );

        // The revision bump must also have been persisted.
        let (final_frontmatter, _final_body) =
            read_outline_file(&workspace_root, rel_path, &chapters)
                .await
                .expect("final read");
        assert_eq!(final_frontmatter.outline_revision, 1);

        drop(tmp);
    }
}
