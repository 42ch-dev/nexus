//! Work outline, structure and chronology (timeline/foreshadow) operations
//! over the guarded core workspace. The work-level outline markdown carries
//! `outline_revision:` frontmatter for optimistic concurrency control.
//! HTTP envelopes stay in adapters; these projections are owned domain values.

use std::collections::HashMap;
use std::path::Path;

use nexus_contracts::{
    OutlinePatchChapterRequest, OutlinePatchResponse, OutlinePatchStructureRequest,
    TimelinePatchEventRequest, WorkOutline, WorkOutlineForeshadowsItem,
    WorkOutlineTimelineEventsItem, WorkOutlineVolumesItem,
};
use nexus_local_db::work_chapters::{self, PatchChapterParams, WorkChapterRecord};
use nexus_local_db::works;

use crate::content::{fsync_write_atomic, resolve_guarded_path_async, wire_cast, WorkLock};
use crate::{CoreError, CoreResult, CoreService, Principal};

const OUTLINE_FILE_MAX_BYTES: usize = 10 * 1024 * 1024;

/// Outline-family fault carrying the legacy classification (including the
/// structured 409 conflict and 422 validation channels) until the adapter
#[derive(Debug, Clone, thiserror::Error)]
enum OutlineFault {
    #[error("{message}")]
    BadRequest { code: String, message: String },
    #[error("{message}")]
    Internal { code: String, message: String },
    #[error("{0}")]
    NotFound(String),
    #[error("outline conflict: {conflicting_path}")]
    Conflict {
        current_revision: u64,
        node_id: String,
        conflicting_path: String,
        recovery_hint: String,
    },
    #[error("outline validation failed")]
    Validation {
        errors: Vec<String>,
        warnings: Vec<String>,
    },
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl From<OutlineFault> for CoreError {
    fn from(error: OutlineFault) -> Self {
        match error {
            OutlineFault::BadRequest { code, message } => Self::InvalidInput {
                field: code,
                reason: message,
            },
            OutlineFault::Internal { code, message } => Self::Internal {
                category: format!("{code}: {message}"),
            },
            OutlineFault::NotFound(resource) => Self::NotFound { resource },
            OutlineFault::Conflict {
                current_revision,
                node_id,
                conflicting_path,
                recovery_hint,
            } => Self::outline_conflict(current_revision, node_id, conflicting_path, recovery_hint),
            OutlineFault::Validation { errors, warnings } => {
                Self::outline_validation_failed(&errors, &warnings)
            }
            OutlineFault::Core(error) => error,
        }
    }
}

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
    fn to_work_outline(&self, work_id: String) -> Result<WorkOutline, OutlineFault> {
        Ok(WorkOutline {
            work_id,
            outline_revision: self.outline_revision_u64()?,
            volumes: self.volumes.clone(),
            timeline_events: self.timeline_events.clone(),
            foreshadows: self.foreshadows.clone(),
            chapter_titles: self.chapter_titles.clone(),
            updated_at: self.updated_at.clone(),
        })
    }

    /// Return `outline_revision` as `u64` for wire contracts that use unsigned
    /// integers. This is an internal invariant; a negative value is a bug.
    fn outline_revision_u64(&self) -> Result<u64, OutlineFault> {
        u64::try_from(self.outline_revision).map_err(|_| OutlineFault::Internal {
            code: "OUTLINE_REVISION_NEGATIVE".to_string(),
            message: "outline_revision became negative".to_string(),
        })
    }
}

// ─── Shared helpers ─────────────────────────────────────────────────────────

/// Canonical relative path for the work-level outline markdown.
fn outline_rel_path(work_ref: &str) -> String {
    format!("Works/{work_ref}/Outlines/outline.md")
}

/// Resolve the filesystem-safe Work reference.
///
/// Prefer the dedicated `work_ref` column; fall back to `story_ref` so tests
/// and legacy flows that only set `story_ref` can still open the outline file.
fn resolve_work_ref(work: &works::WorkRecord) -> Result<String, OutlineFault> {
    work.work_ref
        .clone()
        .or_else(|| work.story_ref.clone())
        .ok_or_else(|| OutlineFault::Internal {
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
    workspace_root: &Path,
    rel_path: &str,
    chapters: &[WorkChapterRecord],
) -> Result<(OutlineFrontmatter, String), OutlineFault> {
    // Use must_exist=false so a missing outline file is treated as a default
    // frontmatter rather than a path-guard error. The guard still verifies the
    // resolved path would live inside the workspace root.
    let path =
        resolve_guarded_path_async(workspace_root.to_path_buf(), rel_path.to_string(), false)
            .await
            .map_err(|e| match &e {
                CoreError::InvalidInput { field, .. } if field == "chapter_path_forbidden" => {
                    OutlineFault::BadRequest {
                        code: "outline_path_forbidden".to_string(),
                        message: format!("outline path '{rel_path}' escapes workspace root"),
                    }
                }
                _ => OutlineFault::Core(e),
            })?;

    let content = match tokio::fs::read_to_string(&path).await {
        Ok(c) => c,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            let now = chrono::Utc::now().to_rfc3339();
            return Ok((default_frontmatter(&now, chapters), String::new()));
        }
        Err(e) => {
            return Err(OutlineFault::Internal {
                code: "FILE_READ_ERROR".to_string(),
                message: format!("failed to read outline '{rel_path}': {e}"),
            });
        }
    };

    let metadata = tokio::fs::metadata(&path)
        .await
        .map_err(|e| OutlineFault::Internal {
            code: "FILE_READ_ERROR".to_string(),
            message: format!("failed to read outline metadata '{rel_path}': {e}"),
        })?;
    let max_bytes = u64::try_from(OUTLINE_FILE_MAX_BYTES).unwrap_or(u64::MAX);
    if metadata.len() > max_bytes {
        return Err(OutlineFault::BadRequest {
            code: "outline_file_too_large".to_string(),
            message: format!("outline file '{rel_path}' exceeds {OUTLINE_FILE_MAX_BYTES} bytes"),
        });
    }

    let Some((yaml, body)) = split_frontmatter(&content) else {
        let now = chrono::Utc::now().to_rfc3339();
        return Ok((default_frontmatter(&now, chapters), content));
    };

    let frontmatter: OutlineFrontmatter =
        serde_yaml::from_str(&yaml).map_err(|e| OutlineFault::BadRequest {
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
    workspace_root: &Path,
    rel_path: &str,
    frontmatter: &OutlineFrontmatter,
    body: &str,
) -> Result<(), OutlineFault> {
    let target =
        resolve_guarded_path_async(workspace_root.to_path_buf(), rel_path.to_string(), false)
            .await?;

    if let Some(parent) = target.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .map_err(|e| OutlineFault::Internal {
                code: "DIRECTORY_CREATE_ERROR".to_string(),
                message: format!("failed to create outline parent dirs: {e}"),
            })?;
    }

    let yaml = serde_yaml::to_string(frontmatter).map_err(|e| OutlineFault::Internal {
        code: "OUTLINE_SERIALIZE_ERROR".to_string(),
        message: format!("failed to serialize outline frontmatter: {e}"),
    })?;
    let content = format!("---\n{yaml}---\n{body}");

    fsync_write_atomic(target, &content)
        .await
        .map_err(|e| OutlineFault::Internal {
            code: "OUTLINE_WRITE_ERROR".to_string(),
            message: format!("failed to write outline '{rel_path}': {e}"),
        })
}

/// Validate a chapter status transition using the V1.65 lifecycle vocabulary.
fn validate_status_transition(from: &str, to: &str) -> Result<(), OutlineFault> {
    if from == to {
        return Ok(());
    }
    match (from, to) {
        ("not_started", "outlined" | "draft" | "finalized")
        | ("outlined", "draft" | "finalized")
        | ("draft", "finalized") => Ok(()),
        _ => Err(OutlineFault::BadRequest {
            code: "chapter_status_transition_invalid".to_string(),
            message: format!(
                "status transition '{from}' -> '{to}' is not allowed through this endpoint"
            ),
        }),
    }
}

/// Build a successful patch response with optional side effects.
fn patch_ok(
    new_revision: i64,
    side_effects: Vec<String>,
) -> Result<OutlinePatchResponse, OutlineFault> {
    Ok(OutlinePatchResponse {
        new_revision: std::num::NonZeroU64::new(u64::try_from(new_revision).unwrap_or(1))
            .unwrap_or(std::num::NonZeroU64::MIN),
        validation_summary: wire_cast(serde_json::json!({
            "errors": Vec::<String>::new(),
            "warnings": Vec::<String>::new(),
        }))?,
        side_effects,
    })
}

// ─── Outline validation rules (V1.73 β hardening, B1–B4) ────────────────────

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
) -> Result<(), OutlineFault> {
    let len = slug.len();
    if !(1..=MAX_SLUG_LEN).contains(&len) {
        return Err(OutlineFault::Validation {
            errors: vec![format!(
                "slug '{slug}' must be 1..={MAX_SLUG_LEN} characters (got {len})"
            )],
            warnings: vec![],
        });
    }
    if !slug
        .bytes()
        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        return Err(OutlineFault::Validation {
            errors: vec![format!(
                "slug '{slug}' must be kebab-case (lowercase ascii letters, digits, and hyphens only)"
            )],
            warnings: vec![],
        });
    }
    // Uniqueness within the Work (exclude the chapter being patched).
    if chapters
        .iter()
        .any(|r| r.chapter != current_chapter && r.slug.as_deref() == Some(slug))
    {
        return Err(OutlineFault::Validation {
            errors: vec![format!(
                "slug '{slug}' is already used by another chapter in this work"
            )],
            warnings: vec![],
        });
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
/// volume" authoring flow.
fn validate_volume_target(
    frontmatter: &OutlineFrontmatter,
    volume_id: i64,
) -> Result<(), OutlineFault> {
    if volume_id < 1 {
        return Err(OutlineFault::Validation {
            errors: vec![format!("volume_id {volume_id} must be >= 1")],
            warnings: vec![],
        });
    }
    let max_volume = frontmatter
        .volumes
        .iter()
        .map(|v| i64::try_from(u64::from(v.volume_id)).unwrap_or(0))
        .max()
        .unwrap_or(0);
    if volume_id > max_volume + 1 {
        return Err(OutlineFault::Validation {
            errors: vec![format!(
                "volume_id {volume_id} does not exist and is not the next sequential volume \
                 (max existing volume: {max_volume}); create it explicitly before binding chapters"
            )],
            warnings: vec![],
        });
    }
    Ok(())
}

/// Reject structural mutations of a published chapter (B4 — `R-V172P0-QC2-004`).
///
/// Guards `patch_structure` operations (`move_chapter`, `attach_to_volume`)
/// that would mutate a published chapter's containment/ordering. The
/// route-specific `patch_chapter` guard uses the older `BadRequest`
/// channel; this structural guard uses the structured 422 validation channel.
fn ensure_chapter_not_published(
    chapters: &[WorkChapterRecord],
    chapter_id: i64,
) -> Result<(), OutlineFault> {
    if let Some(record) = chapters.iter().find(|r| i64::from(r.chapter) == chapter_id) {
        if record.status == "published" {
            return Err(OutlineFault::Validation {
                errors: vec![format!(
                    "structural edits to published chapter {chapter_id} are blocked"
                )],
                warnings: vec![],
            });
        }
    }
    Ok(())
}

fn ensure_chapter_exists(
    chapters: &[WorkChapterRecord],
    chapter_id: i64,
) -> Result<(), OutlineFault> {
    if chapters.iter().any(|r| i64::from(r.chapter) == chapter_id) {
        Ok(())
    } else {
        Err(OutlineFault::NotFound(format!("chapter {chapter_id}")))
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

impl CoreService {
    /// Canonical work outline + timeline read model.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::NotFound`] for an unknown or foreign Work,
    /// and [`CoreError::Internal`] for storage/contract faults.
    pub async fn work_outline(
        &self,
        principal: &Principal,
        work_id: String,
    ) -> CoreResult<WorkOutline> {
        self.verify_principal(principal)?;
        let work = self.resolve_owned_work(principal, &work_id).await?;
        let root = self.workspace_root(principal)?;
        let outline = get_work_outline(self, &work_id, &work, &root)
            .await
            .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(outline)
    }

    /// Structured outline patch (move/attach chapters, link events) guarded by
    /// `outline_revision` CAS.
    ///
    /// # Errors
    /// As [`CoreService::work_outline`]; additionally
    /// [`CoreError::OutlineConflict`] when `base_revision` is stale,
    /// [`CoreError::OutlineValidation`] for invalid patches, and
    /// [`CoreError::Forbidden`] under read-only core access or a runtime lock.
    pub async fn patch_outline_structure(
        &self,
        principal: &Principal,
        holder: &str,
        work_id: String,
        request: OutlinePatchStructureRequest,
    ) -> CoreResult<OutlinePatchResponse> {
        self.verify_principal(principal)?;
        let work = self.resolve_owned_work(principal, &work_id).await?;
        self.require_work_write()?;
        let root = self.workspace_root(principal)?;
        let response =
            patch_outline_structure(self, principal, holder, &work_id, &work, &root, request)
                .await
                .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(response)
    }

    /// Outline canvas chapter-node patch (metadata + per-chapter outline
    /// prose) guarded by `outline_revision` CAS.
    ///
    /// # Errors
    /// As [`CoreService::patch_outline_structure`].
    pub async fn patch_outline_chapter(
        &self,
        principal: &Principal,
        holder: &str,
        work_id: String,
        chapter_id: String,
        request: OutlinePatchChapterRequest,
    ) -> CoreResult<OutlinePatchResponse> {
        self.verify_principal(principal)?;
        let work = self.resolve_owned_work(principal, &work_id).await?;
        self.require_work_write()?;
        let root = self.workspace_root(principal)?;
        let response = patch_outline_chapter(
            self,
            principal,
            holder,
            &work_id,
            &work,
            &root,
            &chapter_id,
            request,
        )
        .await
        .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(response)
    }

    /// Structured timeline patch (events + foreshadow edges) guarded by
    /// `outline_revision` CAS.
    ///
    /// # Errors
    /// As [`CoreService::patch_outline_structure`].
    pub async fn patch_timeline_event(
        &self,
        principal: &Principal,
        holder: &str,
        work_id: String,
        request: TimelinePatchEventRequest,
    ) -> CoreResult<OutlinePatchResponse> {
        self.verify_principal(principal)?;
        let work = self.resolve_owned_work(principal, &work_id).await?;
        self.require_work_write()?;
        let root = self.workspace_root(principal)?;
        let response =
            patch_timeline_event(self, principal, holder, &work_id, &work, &root, request)
                .await
                .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(response)
    }
}

/// Load the chapter rows used for defaulting and validation.
async fn list_chapters(
    service: &CoreService,
    work_id: &str,
) -> Result<Vec<WorkChapterRecord>, OutlineFault> {
    work_chapters::list_chapters(&service.inner.pool, work_id)
        .await
        .map_err(|e| OutlineFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })
}

async fn get_work_outline(
    service: &CoreService,
    work_id: &str,
    work: &works::WorkRecord,
    workspace_root: &Path,
) -> Result<WorkOutline, OutlineFault> {
    let work_ref = resolve_work_ref(work)?;
    let chapters = list_chapters(service, work_id).await?;
    let rel_path = outline_rel_path(&work_ref);
    let (frontmatter, _body) = read_outline_file(workspace_root, &rel_path, &chapters).await?;
    frontmatter.to_work_outline(work_id.to_string())
}

#[allow(clippy::too_many_lines)]
async fn patch_outline_structure(
    service: &CoreService,
    principal: &Principal,
    holder: &str,
    work_id: &str,
    work: &works::WorkRecord,
    workspace_root: &Path,
    req: OutlinePatchStructureRequest,
) -> Result<OutlinePatchResponse, OutlineFault> {
    if req.work_id != work_id {
        return Err(OutlineFault::BadRequest {
            code: "work_id_mismatch".to_string(),
            message: "request work_id must match URL path".to_string(),
        });
    }

    let work_ref = resolve_work_ref(work)?;
    let rel_path = outline_rel_path(&work_ref);

    // Pre-load chapter rows for defaulting and validation.
    let chapters = list_chapters(service, work_id).await?;

    let initial_frontmatter = read_outline_file(workspace_root, &rel_path, &chapters)
        .await?
        .0;

    let base_revision = i64::try_from(req.base_revision).map_err(|_| OutlineFault::BadRequest {
        code: "base_revision_out_of_range".to_string(),
        message: "base_revision exceeds i64 range".to_string(),
    })?;
    if base_revision != initial_frontmatter.outline_revision {
        return Err(OutlineFault::Conflict {
            current_revision: initial_frontmatter.outline_revision_u64()?,
            node_id: req
                .chapter_id
                .map_or_else(|| work_id.to_string(), |n| n.to_string()),
            conflicting_path: "outline_revision".to_string(),
            recovery_hint: "refetch the work outline and reapply".to_string(),
        });
    }
    let lock =
        WorkLock::acquire(&service.inner.pool, principal.creator_id(), work_id, holder).await?;

    // Re-read both frontmatter and body under lock to close the TOCTOU window
    // for concurrent writers and avoid persisting a stale body snapshot.
    let (mut frontmatter, body) = read_outline_file(workspace_root, &rel_path, &chapters).await?;
    if base_revision != frontmatter.outline_revision {
        lock.release().await;
        return Err(OutlineFault::Conflict {
            current_revision: frontmatter.outline_revision_u64()?,
            node_id: req
                .chapter_id
                .map_or_else(|| work_id.to_string(), |n| n.to_string()),
            conflicting_path: "outline_revision".to_string(),
            recovery_hint: "refetch the work outline and reapply".to_string(),
        });
    }

    let result = apply_structure_patch(service, work_id, &req, &mut frontmatter, &chapters).await;
    if let Err(e) = &result {
        lock.release().await;
        return Err(e.clone());
    }

    let now = chrono::Utc::now().to_rfc3339();
    frontmatter.outline_revision += 1;
    frontmatter.updated_at = now;

    if let Err(e) = atomic_write_outline(workspace_root, &rel_path, &frontmatter, &body).await {
        lock.release().await;
        return Err(e);
    }

    lock.release().await;
    patch_ok(frontmatter.outline_revision, Vec::new())
}

#[allow(clippy::too_many_lines)]
async fn patch_outline_chapter(
    service: &CoreService,
    principal: &Principal,
    holder: &str,
    work_id: &str,
    work: &works::WorkRecord,
    workspace_root: &Path,
    n: &str,
    req: OutlinePatchChapterRequest,
) -> Result<OutlinePatchResponse, OutlineFault> {
    if req.work_id != work_id {
        return Err(OutlineFault::BadRequest {
            code: "work_id_mismatch".to_string(),
            message: "request work_id must match URL path".to_string(),
        });
    }

    let chapter = n.parse::<i32>().map_err(|_| OutlineFault::BadRequest {
        code: "invalid_chapter_number".to_string(),
        message: format!("chapter number must be a positive integer, got '{n}'"),
    })?;
    if chapter < 1 {
        return Err(OutlineFault::BadRequest {
            code: "invalid_chapter_number".to_string(),
            message: format!("chapter number must be >= 1, got {chapter}"),
        });
    }
    if i64::try_from(u64::from(req.chapter_id)).unwrap_or(0) != i64::from(chapter) {
        return Err(OutlineFault::BadRequest {
            code: "chapter_id_mismatch".to_string(),
            message: "request chapter_id must match URL path".to_string(),
        });
    }

    let work_ref = resolve_work_ref(work)?;
    let rel_path = outline_rel_path(&work_ref);

    let record = work_chapters::get_chapter(&service.inner.pool, work_id, chapter, 1)
        .await
        .map_err(|e| OutlineFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| OutlineFault::NotFound(format!("chapter {chapter}")))?;

    let chapters = list_chapters(service, work_id).await?;

    let initial_frontmatter = read_outline_file(workspace_root, &rel_path, &chapters)
        .await?
        .0;

    let base_revision = i64::try_from(req.base_revision).map_err(|_| OutlineFault::BadRequest {
        code: "base_revision_out_of_range".to_string(),
        message: "base_revision exceeds i64 range".to_string(),
    })?;
    if base_revision != initial_frontmatter.outline_revision {
        return Err(OutlineFault::Conflict {
            current_revision: initial_frontmatter.outline_revision_u64()?,
            node_id: chapter.to_string(),
            conflicting_path: "outline_revision".to_string(),
            recovery_hint: "refetch the work outline and reapply".to_string(),
        });
    }

    // Protect published chapters from any canvas outline edit (structural
    // metadata AND prose content — V1.75 extended the guard to `content`).
    if record.status == "published" && has_chapter_structural_edit(&req) {
        return Err(OutlineFault::BadRequest {
            code: "chapter_structure_edit_blocked".to_string(),
            message: "edits to published chapters are blocked".to_string(),
        });
    }

    let lock =
        WorkLock::acquire(&service.inner.pool, principal.creator_id(), work_id, holder).await?;

    // Re-read both frontmatter and body under lock to close the TOCTOU window
    // for concurrent writers and avoid persisting a stale body snapshot.
    let (mut frontmatter, body) = read_outline_file(workspace_root, &rel_path, &chapters).await?;
    if base_revision != frontmatter.outline_revision {
        lock.release().await;
        return Err(OutlineFault::Conflict {
            current_revision: frontmatter.outline_revision_u64()?,
            node_id: chapter.to_string(),
            conflicting_path: "outline_revision".to_string(),
            recovery_hint: "refetch the work outline and reapply".to_string(),
        });
    }

    let result = apply_chapter_patch(
        service,
        workspace_root,
        &work_ref,
        work_id,
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

    if let Err(e) = atomic_write_outline(workspace_root, &rel_path, &frontmatter, &body).await {
        lock.release().await;
        return Err(e);
    }

    lock.release().await;
    patch_ok(frontmatter.outline_revision, Vec::new())
}

#[allow(clippy::too_many_lines)]
async fn patch_timeline_event(
    service: &CoreService,
    principal: &Principal,
    holder: &str,
    work_id: &str,
    work: &works::WorkRecord,
    workspace_root: &Path,
    req: TimelinePatchEventRequest,
) -> Result<OutlinePatchResponse, OutlineFault> {
    if req.work_id != work_id {
        return Err(OutlineFault::BadRequest {
            code: "work_id_mismatch".to_string(),
            message: "request work_id must match URL path".to_string(),
        });
    }

    let work_ref = resolve_work_ref(work)?;
    let rel_path = outline_rel_path(&work_ref);

    let chapters = list_chapters(service, work_id).await?;

    let initial_frontmatter = read_outline_file(workspace_root, &rel_path, &chapters)
        .await?
        .0;

    let base_revision = i64::try_from(req.base_revision).map_err(|_| OutlineFault::BadRequest {
        code: "base_revision_out_of_range".to_string(),
        message: "base_revision exceeds i64 range".to_string(),
    })?;
    if base_revision != initial_frontmatter.outline_revision {
        return Err(OutlineFault::Conflict {
            current_revision: initial_frontmatter.outline_revision_u64()?,
            node_id: req.event_id.clone().unwrap_or_else(|| work_id.to_string()),
            conflicting_path: "outline_revision".to_string(),
            recovery_hint: "refetch the work outline and reapply".to_string(),
        });
    }

    let lock =
        WorkLock::acquire(&service.inner.pool, principal.creator_id(), work_id, holder).await?;

    // Re-read both frontmatter and body under lock to close the TOCTOU window
    // for concurrent writers and avoid persisting a stale body snapshot.
    let (mut frontmatter, body) = read_outline_file(workspace_root, &rel_path, &chapters).await?;
    if base_revision != frontmatter.outline_revision {
        lock.release().await;
        return Err(OutlineFault::Conflict {
            current_revision: frontmatter.outline_revision_u64()?,
            node_id: req.event_id.clone().unwrap_or_else(|| work_id.to_string()),
            conflicting_path: "outline_revision".to_string(),
            recovery_hint: "refetch the work outline and reapply".to_string(),
        });
    }

    let result = apply_timeline_patch(&req, &mut frontmatter, &chapters);
    if let Err(e) = &result {
        lock.release().await;
        return Err(e.clone());
    }

    let now = chrono::Utc::now().to_rfc3339();
    frontmatter.outline_revision += 1;
    frontmatter.updated_at = now;

    if let Err(e) = atomic_write_outline(workspace_root, &rel_path, &frontmatter, &body).await {
        lock.release().await;
        return Err(e);
    }

    lock.release().await;
    patch_ok(frontmatter.outline_revision, Vec::new())
}

// ─── Patch application logic ────────────────────────────────────────────────

async fn apply_structure_patch(
    service: &CoreService,
    work_id: &str,
    req: &OutlinePatchStructureRequest,
    frontmatter: &mut OutlineFrontmatter,
    chapters: &[WorkChapterRecord],
) -> Result<(), OutlineFault> {
    let operation = req.operation.as_str();
    match operation {
        "move_chapter" | "attach_to_volume" => {
            let chapter_id = req.chapter_id.ok_or_else(|| OutlineFault::BadRequest {
                code: "missing_chapter_id".to_string(),
                message: format!("{operation} requires chapter_id"),
            })?;
            let volume_id = req.volume_id.ok_or_else(|| OutlineFault::BadRequest {
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
                i32::try_from(chapter_id_i64).map_err(|_| OutlineFault::BadRequest {
                    code: "invalid_chapter_id".to_string(),
                    message: format!("chapter_id {chapter_id_i64} out of range"),
                })?;
            work_chapters::patch_chapter(
                &service.inner.pool,
                work_id,
                chapter_id_i32,
                1,
                &patch,
                &now,
            )
            .await
            .map_err(|e| OutlineFault::Internal {
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
                .ok_or_else(|| OutlineFault::BadRequest {
                    code: "missing_event_id".to_string(),
                    message: "link_event requires event_id".to_string(),
                })?;
            let target = req
                .target_chapter_id
                .ok_or_else(|| OutlineFault::BadRequest {
                    code: "missing_target_chapter_id".to_string(),
                    message: "link_event requires target_chapter_id".to_string(),
                })?;
            ensure_chapter_exists(chapters, i64::try_from(u64::from(target)).unwrap_or(0))?;

            let event = frontmatter
                .timeline_events
                .iter_mut()
                .find(|e| e.event_id == event_id)
                .ok_or_else(|| OutlineFault::NotFound(format!("event {event_id}")))?;
            event.realizes_chapter_id = Some(target);
            Ok(())
        }
        _ => Err(OutlineFault::BadRequest {
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
    service: &CoreService,
    workspace_root: &Path,
    work_id: &str,
    work_ref: &str,
    record: &WorkChapterRecord,
    content: String,
) -> Result<(), OutlineFault> {
    if content.len() > OUTLINE_FILE_MAX_BYTES {
        return Err(OutlineFault::BadRequest {
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
            &service.inner.pool,
            work_id,
            chapter,
            volume_for_path,
            Some(&outline_path),
            &now,
        )
        .await
        .map_err(|e| OutlineFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?;
    }

    // Atomic write of prose to the per-chapter outline file. This is the
    // plain-content writer (distinct from this module's frontmatter+body
    // `atomic_write_outline`).
    crate::content::atomic_write_text(workspace_root, &outline_path, &content)
        .await
        .map_err(OutlineFault::Core)
}

#[allow(clippy::too_many_lines)]
async fn apply_chapter_patch(
    service: &CoreService,
    workspace_root: &Path,
    work_ref: &str,
    work_id: &str,
    record: &WorkChapterRecord,
    req: &OutlinePatchChapterRequest,
    frontmatter: &mut OutlineFrontmatter,
    chapters: &[WorkChapterRecord],
) -> Result<(), OutlineFault> {
    let chapter = record.chapter;

    if let Some(status) = &req.set.status {
        validate_status_transition(&record.status, &status.to_string())?;
    }

    // V1.73 B1 — validate slug format + Work-wide uniqueness before writing.
    if let Some(slug) = &req.set.slug {
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
            .map_err(|_| OutlineFault::BadRequest {
                code: "planned_word_count_too_large".to_string(),
                message: "planned_word_count exceeds i32 range".to_string(),
            })?,
        volume: req
            .set
            .volume
            .map(|v| i32::try_from(u64::from(v)))
            .transpose()
            .map_err(|_| OutlineFault::BadRequest {
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
        &service.inner.pool,
        work_id,
        chapter,
        record.volume.unwrap_or(1),
        &patch,
        &now,
    )
    .await
    .map_err(|e| OutlineFault::Internal {
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
        let chapters = list_chapters(service, work_id).await?;
        move_chapter_in_frontmatter(frontmatter, i64::from(chapter), new_volume_i64, &chapters);
    }

    // V1.75 A2 — outline-prose content patch (canvas-pivot parity-close).
    //
    // The chapter outline prose lives in the per-chapter markdown file
    // referenced by `work_chapters.outline_path` (NOT the work-level
    // `Outlines/outline.md` body, and NEVER `body_path`). Persist it with the
    // same temp+rename+fsync durability pattern used by the V1.65 PUT route.
    // The work-level `outline_revision` CAS bump happens in the caller after
    // this function returns, so a content write rides the same revision
    // increment as a metadata edit.
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
        persist_chapter_outline_content(
            service,
            workspace_root,
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
) -> Result<(), OutlineFault> {
    match req.operation.as_str() {
        "add_event" => timeline_add_event(req, frontmatter, chapters),
        "remove_event" => timeline_remove_event(req, frontmatter),
        "attach_event_to_chapter" => timeline_attach_event_to_chapter(req, frontmatter, chapters),
        "link_foreshadow" => timeline_link_foreshadow(req, frontmatter),
        "unlink_foreshadow" => timeline_unlink_foreshadow(req, frontmatter),
        operation => Err(OutlineFault::BadRequest {
            code: "invalid_timeline_operation".to_string(),
            message: format!("unsupported timeline operation '{operation}'"),
        }),
    }
}

fn timeline_add_event(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
    chapters: &[WorkChapterRecord],
) -> Result<(), OutlineFault> {
    let title = req.title.clone().ok_or_else(|| OutlineFault::BadRequest {
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
            world_event_id: None,
        });
    Ok(())
}

fn timeline_remove_event(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
) -> Result<(), OutlineFault> {
    let event_id = req
        .event_id
        .as_deref()
        .ok_or_else(|| OutlineFault::BadRequest {
            code: "missing_event_id".to_string(),
            message: "remove_event requires event_id".to_string(),
        })?;
    let before = frontmatter.timeline_events.len();
    frontmatter
        .timeline_events
        .retain(|e| e.event_id != event_id);
    if frontmatter.timeline_events.len() == before {
        return Err(OutlineFault::NotFound(format!("event {event_id}")));
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
) -> Result<(), OutlineFault> {
    let event_id = req
        .event_id
        .as_deref()
        .ok_or_else(|| OutlineFault::BadRequest {
            code: "missing_event_id".to_string(),
            message: "attach_event_to_chapter requires event_id".to_string(),
        })?;
    let target = req
        .target_chapter_id
        .ok_or_else(|| OutlineFault::BadRequest {
            code: "missing_target_chapter_id".to_string(),
            message: "attach_event_to_chapter requires target_chapter_id".to_string(),
        })?;
    ensure_chapter_exists(chapters, i64::try_from(u64::from(target)).unwrap_or(0))?;
    let event = frontmatter
        .timeline_events
        .iter_mut()
        .find(|e| e.event_id == event_id)
        .ok_or_else(|| OutlineFault::NotFound(format!("event {event_id}")))?;
    event.realizes_chapter_id = Some(target);
    Ok(())
}

fn timeline_link_foreshadow(
    req: &TimelinePatchEventRequest,
    frontmatter: &mut OutlineFrontmatter,
) -> Result<(), OutlineFault> {
    let source = req
        .event_id
        .as_deref()
        .ok_or_else(|| OutlineFault::BadRequest {
            code: "missing_event_id".to_string(),
            message: "link_foreshadow requires event_id".to_string(),
        })?;
    let target = req
        .foreshadows_event_id
        .as_deref()
        .ok_or_else(|| OutlineFault::BadRequest {
            code: "missing_foreshadows_event_id".to_string(),
            message: "link_foreshadow requires foreshadows_event_id".to_string(),
        })?;

    // QC2-F002 — a foreshadow edge from an event to itself is nonsensical (an
    // event cannot foreshadow its own realization). Reject at the service
    // level so a bypass of the UI's `!==` guard cannot create a self-loop edge.
    if source == target {
        return Err(OutlineFault::BadRequest {
            code: "self_foreshadow_forbidden".to_string(),
            message: "foreshadow source and target events must differ".to_string(),
        });
    }

    let source_event = frontmatter
        .timeline_events
        .iter()
        .find(|e| e.event_id == source)
        .ok_or_else(|| OutlineFault::NotFound(format!("event {source}")))?;
    let target_event = frontmatter
        .timeline_events
        .iter()
        .find(|e| e.event_id == target)
        .ok_or_else(|| OutlineFault::NotFound(format!("event {target}")))?;

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
            return Err(OutlineFault::Validation {
                errors: vec![format!(
                    "foreshadow source event '{source}' realizes chapter {src}, which is after \
                     target event '{target}' realization chapter {tgt}; the source must be \
                     scheduled at or before the target's realization point"
                )],
                warnings: vec![],
            });
        }
        _ => {
            return Err(OutlineFault::Validation {
                errors: vec![format!(
                    "foreshadow link requires both source and target events to be attached to a \
                     realizing chapter (source '{source}' realizes: {source_chapter:?}, target \
                     '{target}' realizes: {target_chapter:?})"
                )],
                warnings: vec![],
            });
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
) -> Result<(), OutlineFault> {
    let source = req
        .event_id
        .as_deref()
        .ok_or_else(|| OutlineFault::BadRequest {
            code: "missing_event_id".to_string(),
            message: "unlink_foreshadow requires event_id".to_string(),
        })?;
    let target = req
        .foreshadows_event_id
        .as_deref()
        .ok_or_else(|| OutlineFault::BadRequest {
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
        return Err(OutlineFault::NotFound(format!("event {source}")));
    }
    if !frontmatter
        .timeline_events
        .iter()
        .any(|e| e.event_id == target)
    {
        return Err(OutlineFault::NotFound(format!("event {target}")));
    }

    let before = frontmatter.foreshadows.len();
    frontmatter
        .foreshadows
        .retain(|edge| !(edge.source_event_id == source && edge.target_event_id == target));
    if frontmatter.foreshadows.len() == before {
        return Err(OutlineFault::NotFound(format!(
            "foreshadow link {source} → {target}"
        )));
    }
    Ok(())
}
