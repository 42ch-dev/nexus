//! Canvas Outline+Timeline Daemon API handlers (V1.72 P0).
//!
//! Endpoints under `/v1/daemon/works/{work_id}/outline/*` and
//! `/v1/daemon/works/{work_id}/timeline/*` expose the Work-level outline
//! structure, chapter metadata, and timeline events. All writes use the
//! `outline_revision:` frontmatter in `Works/<work_ref>/Outlines/outline.md`
//! for optimistic concurrency control.

#![allow(clippy::missing_errors_doc)]

use super::wire_cast;
use crate::api::errors::NexusApiError;
use crate::api::handlers::works::{read_active_creator_id, read_active_workspace_slug};
use crate::api::path_guard::resolve_guarded_path_async;
use crate::api::runtime_lock::RuntimeLockGuard;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::{
    OutlinePatchChapterRequest, OutlinePatchResponse, OutlinePatchStructureRequest,
    TimelinePatchEventRequest, WorkOutline, WorkOutlineForeshadowsItem,
    WorkOutlineTimelineEventsItem, WorkOutlineVolumesItem,
};
use nexus_local_db::work_chapters::{self, PatchChapterParams, WorkChapterRecord};
use nexus_local_db::works;
use std::collections::HashMap;
use std::path::{Path as StdPath, PathBuf};

const OUTLINE_FILE_MAX_BYTES: usize = 10 * 1024 * 1024;

// ─── Internal frontmatter model ─────────────────────────────────────────────

/// In-memory representation of the work outline markdown frontmatter.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
struct OutlineFrontmatter {
    outline_revision: i64,
    volumes: Vec<WorkOutlineVolumesItem>,
    timeline_events: Vec<WorkOutlineTimelineEventsItem>,
    foreshadows: Vec<WorkOutlineForeshadowsItem>,
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
    works::get_work(state.pool_or_uninit()?, creator_id, work_id)
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
    } else {
        let idx = after_open.find("\n---")?;
        if idx + 4 == after_open.len() {
            (idx, 4)
        } else {
            // `---` is not on its own line (e.g. `\n---more`); malformed.
            return None;
        }
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
    let path = resolve_guarded_path_async(
        workspace_root.to_path_buf(),
        rel_path.to_string(),
        false,
    )
    .await
    .map_err(|e| {
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
    let mut ids: Vec<std::num::NonZeroU64> = chapters
        .iter()
        .map(|r| {
            std::num::NonZeroU64::new(u64::try_from(r.chapter).unwrap_or(1))
                .unwrap_or(std::num::NonZeroU64::MIN)
        })
        .collect();
    ids.sort_unstable();
    let volume = WorkOutlineVolumesItem {
        volume_id: std::num::NonZeroU64::MIN,
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
    let target =
        resolve_guarded_path_async(workspace_root.to_path_buf(), rel_path.to_string(), false)
            .await?;

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
        new_revision: std::num::NonZeroU64::new(u64::try_from(new_revision).unwrap_or(1))
            .unwrap_or(std::num::NonZeroU64::MIN),
        validation_summary: wire_cast(serde_json::json!({
            "errors": Vec::<String>::new(),
            "warnings": Vec::<String>::new(),
        })),
        side_effects,
    }
}

// ─── Outline validation rules (V1.73 β hardening, B1–B4) ────────────────────
//
// These rules close the V1.72 carry-over validation gaps. They reject only
// genuinely-invalid inputs through the structured `outline_validation_failed`
// (HTTP 422) channel; existing valid patches continue to pass.

/// Maximum length of a chapter slug (kebab-case identifier).
const MAX_SLUG_LEN: usize = 80;

/// Validate a chapter slug (B1 — `R-V172P0-QC2-001`).
///
/// Rules:
/// - Kebab-case only: ASCII lowercase letters, digits, and hyphens
///   (`^[a-z0-9-]+$`).
/// - Length 1..=80.
/// - Unique within the Work (excluding the chapter currently being patched,
///   so re-asserting an unchanged slug is allowed).
fn validate_chapter_slug(
    slug: &str,
    current_chapter: i32,
    chapters: &[WorkChapterRecord],
) -> Result<(), NexusApiError> {
    let len = slug.len();
    if !(1..=MAX_SLUG_LEN).contains(&len) {
        return Err(NexusApiError::outline_validation_failed(
            &[format!(
                "slug '{slug}' must be 1..={MAX_SLUG_LEN} characters (got {len})"
            )],
            &[],
        ));
    }
    if !slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(NexusApiError::outline_validation_failed(
            &[format!(
                "slug '{slug}' must be kebab-case (lowercase ascii letters, digits, and hyphens only)"
            )],
            &[],
        ));
    }
    // Uniqueness within the Work (exclude the chapter being patched).
    if chapters
        .iter()
        .any(|r| r.chapter != current_chapter && r.slug.as_deref() == Some(slug))
    {
        return Err(NexusApiError::outline_validation_failed(
            &[format!(
                "slug '{slug}' is already used by another chapter in this work"
            )],
            &[],
        ));
    }
    Ok(())
}

/// Validate a volume binding target (B2 — `R-V172P0-QC2-002`).
///
/// The target volume must already exist in the outline, OR be the immediate
/// next sequential volume (`max_existing + 1`) — the only legitimate way a
/// new volume is introduced. Arbitrary out-of-range volumes (e.g. a typo like
/// `999` when only volume 1 exists) are rejected with 422 rather than silently
/// auto-created. This preserves the existing valid "move chapter to the next
/// volume" authoring flow (see `outline_structure_patch_moves_chapter_*`).
fn validate_volume_target(
    frontmatter: &OutlineFrontmatter,
    volume_id: i64,
) -> Result<(), NexusApiError> {
    if volume_id < 1 {
        return Err(NexusApiError::outline_validation_failed(
            &[format!("volume_id {volume_id} must be >= 1")],
            &[],
        ));
    }
    let max_volume = frontmatter
        .volumes
        .iter()
        .map(|v| i64::try_from(u64::from(v.volume_id)).unwrap_or(0))
        .max()
        .unwrap_or(0);
    if volume_id > max_volume + 1 {
        return Err(NexusApiError::outline_validation_failed(
            &[format!(
                "volume_id {volume_id} does not exist and is not the next sequential volume \
                 (max existing volume: {max_volume}); create it explicitly before binding chapters"
            )],
            &[],
        ));
    }
    Ok(())
}

/// Reject structural mutations of a published chapter (B4 — `R-V172P0-QC2-004`).
///
/// Guards `patch_structure` operations (`move_chapter`, `attach_to_volume`)
/// that would mutate a published chapter's containment/ordering. The
/// route-specific `patch_chapter` guard above uses the older `BadRequest`
/// channel; this structural guard uses the structured 422 validation channel.
fn ensure_chapter_not_published(
    chapters: &[WorkChapterRecord],
    chapter_id: i64,
) -> Result<(), NexusApiError> {
    if let Some(record) = chapters.iter().find(|r| i64::from(r.chapter) == chapter_id) {
        if record.status == "published" {
            return Err(NexusApiError::outline_validation_failed(
                &[format!(
                    "structural edits to published chapter {chapter_id} are blocked"
                )],
                &[],
            ));
        }
    }
    Ok(())
}

// V1.73 β hardening closes the four V1.72 MEDIUM validation gaps (slug format,
// volume existence, foreshadow temporal order, published-chapter structural
// guard). Full graph validation (acyclic checks, etc.) remains a future slice.

// ─── Handlers ───────────────────────────────────────────────────────────────

/// `GET /v1/daemon/works/{work_id}/outline` — canonical work outline + timeline.
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

    let chapters = work_chapters::list_chapters(state.pool_or_uninit()?, &work_id)
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

/// `POST /v1/daemon/works/{work_id}/outline/patch` — structured outline patch.
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
    let chapters = work_chapters::list_chapters(state.pool_or_uninit()?, &work_id)
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

    let lock = RuntimeLockGuard::acquire(state.pool_or_uninit()?, &creator_id, &work_id).await?;

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

/// `POST /v1/daemon/works/{work_id}/chapters/{chapter_id}/patch` — outline chapter patch.
///
// `too_many_lines`: this handler intentionally keeps the auth → work load →
// pre-lock read → revision check → `RuntimeLockGuard` acquire → locked re-read
// → validation → atomic persist sequence inline so the TOCTOU + lock-release
// invariants (see `nexus-daemon-runtime/AGENTS.md` Rule 2) are locally
// auditable. V1.73 B1 extended `apply_chapter_patch` with the `chapters` slice
// for slug-uniqueness validation, which pushed it one line over the cap.
#[allow(clippy::too_many_lines)]
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
    if i64::try_from(u64::from(req.chapter_id)).unwrap_or(0) != i64::from(chapter) {
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

    let record = work_chapters::get_chapter(state.pool_or_uninit()?, &work_id, chapter, 1)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| NexusApiError::NotFound(format!("chapter {chapter}")))?;

    let chapters = work_chapters::list_chapters(state.pool_or_uninit()?, &work_id)
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

    // Protect published chapters from any canvas outline edit (structural
    // metadata AND prose content — V1.75 extended the guard to `content`).
    if record.status == "published" && has_chapter_structural_edit(&req) {
        return Err(NexusApiError::BadRequest {
            code: "chapter_structure_edit_blocked".to_string(),
            message: "edits to published chapters are blocked".to_string(),
        });
    }

    let lock = RuntimeLockGuard::acquire(state.pool_or_uninit()?, &creator_id, &work_id).await?;

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

    let result = apply_chapter_patch(
        &state,
        &work_ref,
        &work_id,
        &record,
        &req,
        &mut frontmatter,
        &chapters,
    )
    .await;
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

/// `POST /v1/daemon/works/{work_id}/timeline/patch` — structured timeline patch.
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

    let chapters = work_chapters::list_chapters(state.pool_or_uninit()?, &work_id)
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

    let lock = RuntimeLockGuard::acquire(state.pool_or_uninit()?, &creator_id, &work_id).await?;

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
            let chapter_id_i64 = i64::try_from(u64::from(chapter_id)).unwrap_or(0);
            let volume_id_i64 = i64::try_from(u64::from(volume_id)).unwrap_or(0);

            ensure_chapter_exists(chapters, chapter_id_i64)?;
            // V1.73 B4 — block structural edits to published chapters.
            ensure_chapter_not_published(chapters, chapter_id_i64)?;
            // V1.73 B2 — reject binding to a non-existent / out-of-range volume.
            validate_volume_target(frontmatter, volume_id_i64)?;

            let volume_id_i32 = i32::try_from(volume_id_i64).unwrap_or(1);

            // Update the DB volume binding so `work_chapters` stays SSOT.
            let now = chrono::Utc::now().to_rfc3339();
            let patch = PatchChapterParams {
                volume: Some(volume_id_i32),
                ..Default::default()
            };
            let chapter_id_i32 =
                i32::try_from(chapter_id_i64).map_err(|_| NexusApiError::BadRequest {
                    code: "invalid_chapter_id".to_string(),
                    message: format!("chapter_id {chapter_id_i64} out of range"),
                })?;
            work_chapters::patch_chapter(
                state.pool_or_uninit()?,
                work_id,
                chapter_id_i32,
                1,
                &patch,
                &now,
            )
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;

            // Re-sync the outline volume ordering.
            move_chapter_in_frontmatter(frontmatter, chapter_id_i64, volume_id_i64, chapters);
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
            ensure_chapter_exists(chapters, i64::try_from(u64::from(target)).unwrap_or(0))?;

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
    let chapter_id = std::num::NonZeroU64::new(u64::try_from(chapter_id).unwrap_or(1))
        .unwrap_or(std::num::NonZeroU64::MIN);
    let volume_id = std::num::NonZeroU64::new(u64::try_from(volume_id).unwrap_or(1))
        .unwrap_or(std::num::NonZeroU64::MIN);
    let vol1 = std::num::NonZeroU64::MIN;
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
        frontmatter.volumes.push(WorkOutlineVolumesItem {
            volume_id,
            label: format!("Volume {volume_id}"),
            chapter_ids: vec![chapter_id],
        });
    }

    // Ensure every chapter still appears somewhere; missing ones land in volume 1.
    let mut present: std::collections::HashSet<std::num::NonZeroU64> = frontmatter
        .volumes
        .iter()
        .flat_map(|vol| vol.chapter_ids.clone())
        .collect();

    // Resolve the volume-1 slot once, creating it if absent, to avoid overlapping
    // mutable borrows inside the loop.
    let vol1_idx = if let Some(idx) = frontmatter
        .volumes
        .iter()
        .position(|vol| vol.volume_id == vol1)
    {
        idx
    } else {
        frontmatter.volumes.push(WorkOutlineVolumesItem {
            volume_id: vol1,
            label: "Volume 1".to_string(),
            chapter_ids: Vec::new(),
        });
        frontmatter.volumes.len() - 1
    };

    for record in chapters {
        let id = std::num::NonZeroU64::new(u64::try_from(record.chapter).unwrap_or(1))
            .unwrap_or(std::num::NonZeroU64::MIN);
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

/// Returns true if the patch carries any canvas-editable chapter field. Used
/// by the published-chapter guard to block ALL outline mutations on a published
/// chapter (structural metadata AND prose content) — a published chapter is in
/// its final state. V1.75 extended this to include `content`.
const fn has_chapter_structural_edit(req: &OutlinePatchChapterRequest) -> bool {
    req.set.title.is_some()
        || req.set.slug.is_some()
        || req.set.planned_word_count.is_some()
        || req.set.actual_word_count.is_some()
        || req.set.volume.is_some()
        || req.set.status.is_some()
        || req.set.content.is_some()
}

/// Persist chapter outline prose to its per-chapter file and seed the DB
/// `outline_path` column when it is empty.
///
/// Ordering invariant: the DB `outline_path` is seeded before the file is
/// atomically written. If the file write fails, the column still points at the
/// canonical derived path, and the next read will re-derive and re-seed it.
/// The caller remains responsible for the work-level `Outlines/outline.md`
/// frontmatter + `outline_revision` bump, so the per-chapter content is
/// durably on disk before the work-level revision is advanced.
async fn persist_chapter_outline_content(
    pool: &sqlx::SqlitePool,
    workspace_root: &StdPath,
    work_id: &str,
    work_ref: &str,
    record: &WorkChapterRecord,
    content: String,
) -> Result<(), NexusApiError> {
    if content.len() > OUTLINE_FILE_MAX_BYTES {
        return Err(NexusApiError::BadRequest {
            code: "chapter_outline_content_too_large".to_string(),
            message: format!(
                "chapter outline content is {} bytes, exceeding the maximum of {} bytes",
                content.len(),
                OUTLINE_FILE_MAX_BYTES
            ),
        });
    }

    let chapter = record.chapter;
    let volume_for_path = record.volume.unwrap_or(1);
    let was_empty = record.outline_path.as_deref().is_none_or(str::is_empty);
    let outline_path = record
        .outline_path
        .clone()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| format!("Works/{work_ref}/Outlines/chapters/ch{chapter:02}-outline.md"));

    // If the column was empty, persist the derived path so subsequent reads
    // (V1.65 GET, the canvas inspector) find the file. This mirrors the
    // V1.65 PUT route's seeding behavior.
    if was_empty {
        let now = chrono::Utc::now().to_rfc3339();
        work_chapters::update_outline_path(
            pool,
            work_id,
            chapter,
            volume_for_path,
            Some(&outline_path),
            &now,
        )
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    }

    // Atomic write of prose to the per-chapter outline file. This is the
    // chapters.rs plain-content writer (distinct from this module's
    // frontmatter+body `atomic_write_outline`).
    crate::api::handlers::chapters::atomic_write_outline(workspace_root, &outline_path, &content)
        .await
}

// The validate → DB persist → frontmatter mutate sequence is kept inline so
// the `RuntimeLockGuard` release paths stay locally auditable. The per-chapter
// outline-file write is delegated to `persist_chapter_outline_content`, which
// documents its own body-ownership invariant.
async fn apply_chapter_patch(
    state: &WorkspaceState,
    work_ref: &str,
    work_id: &str,
    record: &WorkChapterRecord,
    req: &OutlinePatchChapterRequest,
    frontmatter: &mut OutlineFrontmatter,
    chapters: &[WorkChapterRecord],
) -> Result<(), NexusApiError> {
    let chapter = record.chapter;

    if let Some(ref status) = req.set.status {
        validate_status_transition(&record.status, &status.to_string())?;
    }

    // V1.73 B1 — validate slug format + Work-wide uniqueness before writing.
    if let Some(ref slug) = req.set.slug {
        validate_chapter_slug(slug, chapter, chapters)?;
    }
    // V1.73 B2 — validate volume binding target before moving the chapter.
    if let Some(volume_id) = req.set.volume {
        validate_volume_target(
            frontmatter,
            i64::try_from(u64::from(volume_id)).unwrap_or(0),
        )?;
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
        volume: req
            .set
            .volume
            .map(|v| i32::try_from(u64::from(v)))
            .transpose()
            .map_err(|_| NexusApiError::BadRequest {
                code: "invalid_volume".to_string(),
                message: "volume exceeds i32 range".to_string(),
            })?,
        status: req
            .set
            .status
            .as_ref()
            .map(std::string::ToString::to_string),
    };

    // Persist slug/wc/volume/status to the chapter SSOT table.
    let now = chrono::Utc::now().to_rfc3339();
    work_chapters::patch_chapter(
        state.pool_or_uninit()?,
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
        let new_volume = req.set.volume.unwrap_or_else(|| {
            std::num::NonZeroU64::new(u64::try_from(record.volume.unwrap_or(1)).unwrap_or(1))
                .unwrap_or(std::num::NonZeroU64::MIN)
        });
        let new_volume_i64 = i64::try_from(u64::from(new_volume)).unwrap_or(1);
        let chapters = work_chapters::list_chapters(state.pool_or_uninit()?, work_id)
            .await
            .map_err(|e| NexusApiError::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?;
        move_chapter_in_frontmatter(frontmatter, i64::from(chapter), new_volume_i64, &chapters);
    }

    // V1.75 A2 — outline-prose content patch (canvas-pivot parity-close).
    //
    // The chapter outline prose lives in the per-chapter markdown file
    // referenced by `work_chapters.outline_path` (NOT the work-level
    // `Outlines/outline.md` body, and NEVER `body_path`). Persist it with the
    // same temp+rename+fsync durability pattern used by the V1.65 PUT route,
    // reusing `chapters::atomic_write_outline`. The work-level
    // `outline_revision` CAS bump happens in the caller after this function
    // returns, so a content write rides the same revision increment as a
    // metadata edit.
    //
    // Body-ownership invariant: this block writes ONLY to `outline_path`. It
    // does not touch `body_path`, the body writer, or `Stories/**`.
    //
    // Two-file write ordering: this helper writes the per-chapter outline file
    // first; the caller then atomically writes the work-level frontmatter and
    // bumps `outline_revision`. The per-chapter content is durable before the
    // work-level revision advances, and a failed work-level write can be
    // retried idempotently.
    if let Some(content) = req.set.content.clone() {
        let workspace_root = workspace_root(state)?;
        persist_chapter_outline_content(
            state.pool_or_uninit()?,
            &workspace_root,
            work_id,
            work_ref,
            record,
            content.to_string(),
        )
        .await?;
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
        "unlink_foreshadow" => timeline_unlink_foreshadow(req, frontmatter),
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
        ensure_chapter_exists(chapters, i64::try_from(u64::from(chapter_id)).unwrap_or(0))?;
    }
    let event_id = format!("evt_{}", uuid::Uuid::new_v4());
    frontmatter
        .timeline_events
        .push(WorkOutlineTimelineEventsItem {
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
    ensure_chapter_exists(chapters, i64::try_from(u64::from(target)).unwrap_or(0))?;
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

    // QC2-F002 — a foreshadow edge from an event to itself is nonsensical (an
    // event cannot foreshadow its own realization). Reject at the daemon level
    // so a bypass of the UI's `!==` guard cannot create a self-loop edge.
    if source == target {
        return Err(NexusApiError::BadRequest {
            code: "self_foreshadow_forbidden".to_string(),
            message: "foreshadow source and target events must differ".to_string(),
        });
    }

    let source_event = frontmatter
        .timeline_events
        .iter()
        .find(|e| e.event_id == source)
        .ok_or_else(|| NexusApiError::NotFound(format!("event {source}")))?;
    let target_event = frontmatter
        .timeline_events
        .iter()
        .find(|e| e.event_id == target)
        .ok_or_else(|| NexusApiError::NotFound(format!("event {target}")))?;

    // V1.73 B3 — enforce source-before-target temporal order. A foreshadow is
    // planted by the source event and realized by the target event, so the
    // source must be scheduled at or before the target's realization point.
    // An event's temporal coordinate is its `realizes_chapter_id`; both ends
    // must carry one to establish an ordering.
    let source_chapter = source_event.realizes_chapter_id;
    let target_chapter = target_event.realizes_chapter_id;
    match (source_chapter, target_chapter) {
        (Some(src), Some(tgt)) if src <= tgt => {}
        (Some(src), Some(tgt)) => {
            return Err(NexusApiError::outline_validation_failed(
                &[format!(
                    "foreshadow source event '{source}' realizes chapter {src}, which is after \
                     target event '{target}' realization chapter {tgt}; the source must be \
                     scheduled at or before the target's realization point"
                )],
                &[],
            ));
        }
        _ => {
            return Err(NexusApiError::outline_validation_failed(
                &[format!(
                    "foreshadow link requires both source and target events to be attached to a \
                     realizing chapter (source '{source}' realizes: {source_chapter:?}, target \
                     '{target}' realizes: {target_chapter:?})"
                )],
                &[],
            ));
        }
    }

    if !frontmatter
        .foreshadows
        .iter()
        .any(|edge| edge.source_event_id == source && edge.target_event_id == target)
    {
        frontmatter.foreshadows.push(WorkOutlineForeshadowsItem {
            source_event_id: source.to_string(),
            target_event_id: target.to_string(),
        });
    }
    Ok(())
}

/// Remove a foreshadow link (source → target) from the outline.
///
/// This is the unlink counterpart to [`timeline_link_foreshadow`]. It requires
/// `event_id` (source) and `foreshadows_event_id` (target); both must match an
/// existing edge exactly. A non-existent edge returns `NotFound` so callers can
/// distinguish "nothing to unlink" from a silent no-op.
fn timeline_unlink_foreshadow(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
) -> Result<(), NexusApiError> {
    let source = req
        .event_id
        .as_deref()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "missing_event_id".to_string(),
            message: "unlink_foreshadow requires event_id".to_string(),
        })?;
    let target = req
        .foreshadows_event_id
        .as_deref()
        .ok_or_else(|| NexusApiError::BadRequest {
            code: "missing_foreshadows_event_id".to_string(),
            message: "unlink_foreshadow requires foreshadows_event_id".to_string(),
        })?;

    // QC2-F001 — verify both events still exist in `timeline_events`, mirroring
    // the link handler's existence checks. An edge can outlive its events if
    // the outline was edited outside the daemon (e.g. manual YAML edit removed
    // an event but left a dangling foreshadow entry). Without this guard the
    // unlink would silently succeed on a structurally invalid outline.
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

    let before = frontmatter.foreshadows.len();
    frontmatter
        .foreshadows
        .retain(|edge| !(edge.source_event_id == source && edge.target_event_id == target));
    if frontmatter.foreshadows.len() == before {
        return Err(NexusApiError::NotFound(format!(
            "foreshadow link {source} → {target}"
        )));
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
        crate::test_utils::seed_test_creator_and_world(state.pool_or_uninit().unwrap()).await;

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
            state.pool_or_uninit().unwrap(),
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

        let chapters = work_chapters::list_chapters(state.pool_or_uninit().unwrap(), &work_id)
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

    /// V1.88 T3 (R-V187-QC3-P001): async path guard accepts an in-bounds
    /// outline file and returns the parsed frontmatter/body.
    #[tokio::test]
    async fn read_outline_file_accepts_in_bounds_path() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace_root = tmp.path();
        let rel_path = "Works/test/Outlines/outline.md";
        let outline_path = workspace_root.join(rel_path);
        std::fs::create_dir_all(outline_path.parent().unwrap()).unwrap();
        std::fs::write(
            &outline_path,
            "---\noutline_revision: 2\nvolumes: []\ntimeline_events: []\nforeshadows: []\nchapter_titles: {}\nupdated_at: \"2024-01-01T00:00:00Z\"\n---\nbody\n",
        )
        .unwrap();

        let (frontmatter, body) = read_outline_file(workspace_root, rel_path, &[])
            .await
            .expect("in-bounds outline should read successfully");
        assert_eq!(frontmatter.outline_revision, 2);
        assert_eq!(body, "body\n");
    }

    /// V1.88 T3 (R-V187-QC3-P001): async path guard rejects a relative path
    /// that escapes the workspace root before any FS access.
    #[tokio::test]
    async fn read_outline_file_rejects_escape_path() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace_root = tmp.path();
        let result = read_outline_file(workspace_root, "../evil.md", &[]).await;
        assert!(result.is_err(), "escape path should be rejected");
        match result {
            Err(NexusApiError::BadRequest { code, .. }) => {
                assert_eq!(code, "outline_path_forbidden");
            }
            Err(other) => panic!("expected outline_path_forbidden BadRequest, got {other:?}"),
            Ok(_) => panic!("expected error"),
        }
    }

    // ── Foreshadow link/unlink unit tests (FB-C1-005, V1.108 P0 T4) ────────

    /// Build a frontmatter with two attached events and a foreshadow edge
    /// between them, for reuse by link/unlink tests.
    fn frontmatter_with_foreshadow() -> OutlineFrontmatter {
        OutlineFrontmatter {
            outline_revision: 1,
            volumes: Vec::new(),
            timeline_events: vec![
                WorkOutlineTimelineEventsItem {
                    event_id: "evt_a".to_string(),
                    title: "Plant".to_string(),
                    description: None,
                    realizes_chapter_id: Some(std::num::NonZeroU64::new(1).unwrap()),
                },
                WorkOutlineTimelineEventsItem {
                    event_id: "evt_b".to_string(),
                    title: "Payoff".to_string(),
                    description: None,
                    realizes_chapter_id: Some(std::num::NonZeroU64::new(2).unwrap()),
                },
            ],
            foreshadows: vec![WorkOutlineForeshadowsItem {
                source_event_id: "evt_a".to_string(),
                target_event_id: "evt_b".to_string(),
            }],
            chapter_titles: HashMap::new(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn unlink_foreshadow_removes_existing_edge() {
        let mut fm = frontmatter_with_foreshadow();
        let req = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "unlink_foreshadow".parse().unwrap(),
            event_id: Some("evt_a".to_string()),
            foreshadows_event_id: Some("evt_b".to_string()),
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        assert_eq!(fm.foreshadows.len(), 1);
        timeline_unlink_foreshadow(&req, &mut fm).expect("unlink should succeed");
        assert!(fm.foreshadows.is_empty(), "edge must be removed");
    }

    #[test]
    fn unlink_foreshadow_returns_not_found_for_missing_edge() {
        let mut fm = frontmatter_with_foreshadow();
        let req = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "unlink_foreshadow".parse().unwrap(),
            event_id: Some("evt_b".to_string()), // reversed direction — does not exist
            foreshadows_event_id: Some("evt_a".to_string()),
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        let result = timeline_unlink_foreshadow(&req, &mut fm);
        assert!(result.is_err(), "non-existent edge must error");
        assert!(matches!(result, Err(NexusApiError::NotFound(_))));
        // The existing edge must be untouched.
        assert_eq!(fm.foreshadows.len(), 1);
    }

    #[test]
    fn unlink_foreshadow_requires_event_id_and_target() {
        let mut fm = frontmatter_with_foreshadow();

        // Missing event_id.
        let no_source = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "unlink_foreshadow".parse().unwrap(),
            foreshadows_event_id: Some("evt_b".to_string()),
            event_id: None,
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        assert!(timeline_unlink_foreshadow(&no_source, &mut fm).is_err());

        // Missing foreshadows_event_id.
        let no_target = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "unlink_foreshadow".parse().unwrap(),
            event_id: Some("evt_a".to_string()),
            foreshadows_event_id: None,
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        assert!(timeline_unlink_foreshadow(&no_target, &mut fm).is_err());
    }

    #[test]
    fn apply_timeline_patch_dispatches_unlink_foreshadow() {
        let mut fm = frontmatter_with_foreshadow();
        let req = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "unlink_foreshadow".parse().unwrap(),
            event_id: Some("evt_a".to_string()),
            foreshadows_event_id: Some("evt_b".to_string()),
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        // apply_timeline_patch routes to the correct handler by operation string.
        apply_timeline_patch(&req, &mut fm, &[]).expect("dispatch should succeed");
        assert!(fm.foreshadows.is_empty());
    }

    #[test]
    fn link_then_unlink_roundtrip() {
        // Start with no foreshadow edges, link one, then unlink it — verifying
        // the round-trip leaves the outline clean.
        let mut fm = OutlineFrontmatter {
            outline_revision: 1,
            volumes: Vec::new(),
            timeline_events: frontmatter_with_foreshadow().timeline_events,
            foreshadows: Vec::new(),
            chapter_titles: HashMap::new(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };

        let link_req = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "link_foreshadow".parse().unwrap(),
            event_id: Some("evt_a".to_string()),
            foreshadows_event_id: Some("evt_b".to_string()),
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        apply_timeline_patch(&link_req, &mut fm, &[]).expect("link should succeed");
        assert_eq!(fm.foreshadows.len(), 1);

        let unlink_req = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "unlink_foreshadow".parse().unwrap(),
            event_id: Some("evt_a".to_string()),
            foreshadows_event_id: Some("evt_b".to_string()),
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        apply_timeline_patch(&unlink_req, &mut fm, &[]).expect("unlink should succeed");
        assert!(fm.foreshadows.is_empty());
    }

    // ── QC2-F002 / QC2-F003 — link self-ref guard + unlink existence checks ─

    /// QC2-F002 — `link_foreshadow` with source == target must return
    /// `BadRequest("self_foreshadow_forbidden")`.
    #[test]
    fn link_foreshadow_rejects_self_reference() {
        let mut fm = frontmatter_with_foreshadow();
        let req = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "link_foreshadow".parse().unwrap(),
            event_id: Some("evt_a".to_string()),
            foreshadows_event_id: Some("evt_a".to_string()),
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        let result = timeline_link_foreshadow(&req, &mut fm);
        match result {
            Err(NexusApiError::BadRequest { code, .. }) => {
                assert_eq!(
                    code, "self_foreshadow_forbidden",
                    "expected self_foreshadow_forbidden code"
                );
            }
            Err(other) => panic!("expected BadRequest self_foreshadow_forbidden, got {other:?}"),
            Ok(()) => panic!("self-referential foreshadow link must be rejected"),
        }
        // No edge should have been added.
        assert_eq!(
            fm.foreshadows.len(),
            1,
            "existing edges must be untouched after a rejected self-ref link"
        );
    }

    /// QC2-F001 — `unlink_foreshadow` must return `NotFound` when either the
    /// source or target event no longer exists in `timeline_events`, even if
    /// the edge is still present in `foreshadows`. This can happen when the
    /// outline was edited outside the daemon (manual YAML edit) and an event
    /// was removed without cleaning up the foreshadow entry.
    #[test]
    fn unlink_foreshadow_returns_not_found_for_missing_event() {
        // Start with a frontmatter that has the foreshadow edge but whose
        // source event has been removed from timeline_events.
        let mut fm = OutlineFrontmatter {
            outline_revision: 1,
            volumes: Vec::new(),
            timeline_events: vec![WorkOutlineTimelineEventsItem {
                event_id: "evt_b".to_string(),
                title: "Payoff".to_string(),
                description: None,
                realizes_chapter_id: Some(std::num::NonZeroU64::new(2).unwrap()),
            }],
            // The edge evt_a → evt_b is still present, but evt_a no longer exists.
            foreshadows: vec![WorkOutlineForeshadowsItem {
                source_event_id: "evt_a".to_string(),
                target_event_id: "evt_b".to_string(),
            }],
            chapter_titles: HashMap::new(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let req = TimelinePatchEventRequest {
            work_id: "wk_test".to_string(),
            base_revision: 1,
            operation: "unlink_foreshadow".parse().unwrap(),
            event_id: Some("evt_a".to_string()),
            foreshadows_event_id: Some("evt_b".to_string()),
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        let result = timeline_unlink_foreshadow(&req, &mut fm);
        assert!(
            result.is_err(),
            "unlink with missing source event must error"
        );
        assert!(
            matches!(result, Err(NexusApiError::NotFound(_))),
            "expected NotFound for missing source event"
        );
        // Edge must be untouched (we didn't remove it because we failed early).
        assert_eq!(fm.foreshadows.len(), 1);

        // Same for missing target event.
        let mut fm2 = OutlineFrontmatter {
            outline_revision: 1,
            volumes: Vec::new(),
            timeline_events: vec![WorkOutlineTimelineEventsItem {
                event_id: "evt_a".to_string(),
                title: "Plant".to_string(),
                description: None,
                realizes_chapter_id: Some(std::num::NonZeroU64::new(1).unwrap()),
            }],
            foreshadows: vec![WorkOutlineForeshadowsItem {
                source_event_id: "evt_a".to_string(),
                target_event_id: "evt_b".to_string(),
            }],
            chapter_titles: HashMap::new(),
            updated_at: "2024-01-01T00:00:00Z".to_string(),
        };
        let result2 = timeline_unlink_foreshadow(&req, &mut fm2);
        assert!(
            matches!(result2, Err(NexusApiError::NotFound(_))),
            "expected NotFound for missing target event"
        );
    }

    /// QC2-F003 — `patch_timeline_event` handler must return `outline_conflict`
    /// (409) when `base_revision` is stale, even for the `unlink_foreshadow`
    /// op. This verifies the revision gate fires before the operation is
    /// dispatched.
    #[tokio::test]
    async fn patch_timeline_event_stale_revision_unlink_returns_conflict() {
        use crate::api::handlers::works::{CreateWorkRequest, PatchWorkRequest};

        let (tmp, nexus_home, db_path, workspace_dir) =
            crate::test_utils::create_initialized_test_workspace().await;
        let state = WorkspaceState::new_for_testing(
            nexus_home,
            db_path,
            Some(workspace_dir.to_string_lossy().to_string()),
        )
        .await;
        crate::test_utils::seed_test_creator_and_world(state.pool_or_uninit().unwrap()).await;

        // Create a work so the outline file path is resolvable.
        let work_id = {
            let req = CreateWorkRequest {
                title: "Stale Rev Test".to_string(),
                long_term_goal: "Test stale revision guard".to_string(),
                initial_idea: "A test".to_string(),
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

        // Set story_ref for deterministic outline path.
        {
            let req = PatchWorkRequest {
                title: None,
                long_term_goal: None,
                creative_brief: None,
                intake_status: None,
                status: None,
                world_id: None,
                story_ref: Some(Some("stale-rev-test".to_string())),
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

        // Write an outline at revision 5 with events + a foreshadow edge.
        let workspace_root = &workspace_dir;
        let rel_path = "Works/stale-rev-test/Outlines/outline.md";
        let outline_path = workspace_root.join(rel_path);
        tokio::fs::create_dir_all(outline_path.parent().unwrap())
            .await
            .expect("create outline dirs");
        tokio::fs::write(
            &outline_path,
            "---\n\
             outline_revision: 5\n\
             volumes: []\n\
             timeline_events:\n\
             \x20 - event_id: evt_a\n\
             \x20   title: Plant\n\
             \x20   realizes_chapter_id: 1\n\
             \x20 - event_id: evt_b\n\
             \x20   title: Payoff\n\
             \x20   realizes_chapter_id: 2\n\
             foreshadows:\n\
             \x20 - source_event_id: evt_a\n\
             \x20   target_event_id: evt_b\n\
             chapter_titles: {}\n\
             updated_at: \"2024-01-01T00:00:00Z\"\n\
             ---\nbody\n",
        )
        .await
        .expect("write outline");

        // Send unlink_foreshadow with a stale base_revision (3 vs current 5).
        let stale_req = TimelinePatchEventRequest {
            work_id: work_id.clone(),
            base_revision: 3, // stale — outline is at revision 5
            operation: "unlink_foreshadow".parse().unwrap(),
            event_id: Some("evt_a".to_string()),
            foreshadows_event_id: Some("evt_b".to_string()),
            description: None,
            realizes_chapter_id: None,
            target_chapter_id: None,
            title: None,
        };
        let result = patch_timeline_event(
            axum::extract::State(state.clone()),
            axum::extract::Path(work_id.clone()),
            axum::Json(stale_req),
        )
        .await;

        match result {
            Err(NexusApiError::OutlineConflict { .. }) => {
                // Expected — the handler must reject the stale revision before
                // dispatching the unlink operation.
            }
            Err(other) => {
                panic!("expected OutlineConflict for stale base_revision, got {other:?}")
            }
            Ok(_) => panic!("stale base_revision must be rejected with outline_conflict"),
        }

        drop(tmp);
    }
}
