//! Work chapter content (manuscript body, per-chapter outline prose and
//! chapter metadata) over the guarded core workspace.
//! HTTP envelopes stay in adapters; these projections are owned domain values.

use std::num::NonZeroU64;
use std::path::{Path, PathBuf};

use nexus_contracts::{
    ChapterBody, ChapterContentQuery, ChapterDetail, ChapterOutline, ChapterProtection,
    ChapterStatus, ChapterSummary, ListChaptersQuery, ListChaptersResponse, PaginationInfo,
    PatchChapterRequest,
};
use nexus_local_db::work_chapters::{self, PatchChapterParams, WorkChapterRecord};

use crate::{CoreError, CoreResult, CoreService, Principal};

/// Core-owned chapter content query (path arguments stay explicit method
/// parameters). Wire parity with `nexus_contracts::ChapterContentQuery`:
/// a present `volume` must be non-zero.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CoreChapterContentQuery {
    pub volume: Option<NonZeroU64>,
}

impl From<ChapterContentQuery> for CoreChapterContentQuery {
    fn from(query: ChapterContentQuery) -> Self {
        Self {
            volume: query.volume,
        }
    }
}

/// Content-family fault carrying the legacy classification until the adapter
/// boundary (same carrier scheme as the Work family).
#[derive(Debug, thiserror::Error)]
enum ContentFault {
    #[error("{message}")]
    BadRequest { code: String, message: String },
    #[error("{message}")]
    Internal { code: String, message: String },
    #[error("{0}")]
    NotFound(String),
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl From<ContentFault> for CoreError {
    fn from(error: ContentFault) -> Self {
        match error {
            ContentFault::BadRequest { code, message } => Self::InvalidInput {
                field: code,
                reason: message,
            },
            // The legacy internal classification (DATABASE_ERROR, CONTRACT_ERROR,
            // FILE_READ_ERROR, …) rides verbatim as `<CODE>: <message>`; the
            // daemon content adapter re-emits the code instead of collapsing it
            // to the shared `CORE_ERROR` shape.
            ContentFault::Internal { code, message } => Self::Internal {
                category: format!("{code}: {message}"),
            },
            ContentFault::NotFound(resource) => Self::NotFound { resource },
            ContentFault::Core(error) => error,
        }
    }
}

/// Wire-equivalent typify copy conversion (see `works::wire_cast`); the legacy
/// `CONTRACT_ERROR` code rides the shared internal category verbatim.
pub(crate) fn wire_cast<T: serde::de::DeserializeOwned, S: serde::Serialize>(
    value: S,
) -> Result<T, CoreError> {
    serde_json::to_value(value)
        .and_then(serde_json::from_value)
        .map_err(|error| CoreError::Internal {
            category: format!("CONTRACT_ERROR: {error}"),
        })
}

// ─── Guarded filesystem primitives (shared by content + outline) ───────────

/// Resolve a relative path under the workspace root and enforce the
/// W-002-style path guard: the resolved absolute path must remain inside
/// the canonical workspace root.
///
/// `must_exist` controls whether the target itself must exist (read paths) or
/// whether a missing-but-creatable target is allowed (write paths). For write
/// paths, the helper walks up to the nearest existing parent so that missing
/// intermediate directories are accepted as long as they would be created
/// inside the root.
///
/// # TOCTOU note
///
/// There is a small race window between canonicalizing the workspace root and
/// canonicalizing the requested path: a local attacker with filesystem access
/// could replace either path during that window. Per the V1.86 trust-boundary
/// spec, this is "racy-correct" rather than "racy-incorrect" for the
/// single-user local context: the practical risk is bounded by that threat
/// model, while adversarial multi-user FS access is out of scope
/// (`R-V166-QC2-TOCTOU`).
///
/// Returns [`CoreError::InvalidInput`] whose `field` carries the legacy
/// `CHAPTER_PATH_*` code (`chapter_path_empty`, `chapter_path_unresolvable`,
/// `chapter_path_forbidden`) when the path is empty, cannot be resolved, or
/// escapes the workspace root. Callers may substitute domain-specific codes.
///
/// This is the single W-002 authority: the daemon `api/path_guard.rs` and the
/// host-tool write paths delegate here and map the error at their own
/// boundary, so a security fix cannot diverge between surfaces.
pub fn resolve_guarded_path(
    workspace_root: &Path,
    rel_path: &str,
    must_exist: bool,
) -> Result<PathBuf, CoreError> {
    let forbidden = |rel_path: &str| CoreError::InvalidInput {
        field: "chapter_path_forbidden".to_string(),
        reason: format!("chapter path '{rel_path}' escapes workspace root"),
    };
    if rel_path.is_empty() {
        return Err(CoreError::InvalidInput {
            field: "chapter_path_empty".to_string(),
            reason: "chapter path is empty".to_string(),
        });
    }

    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());

    let joined = canonical_root.join(rel_path);

    if must_exist {
        let canonical_target = joined.canonicalize().map_err(|e| CoreError::InvalidInput {
            field: "chapter_path_unresolvable".to_string(),
            reason: format!("cannot resolve chapter path '{rel_path}': {e}"),
        })?;
        // Component-wise comparison (Path::starts_with). A plain string prefix
        // match would let `/home/user-data/evil.md` slip past a `/home/user`
        // root because the string starts with "/home/user".
        if !canonical_target.starts_with(&canonical_root) {
            return Err(forbidden(rel_path));
        }
        Ok(canonical_target)
    } else {
        // For creatable targets, normalize the joined path and verify it stays
        // within the workspace root. We walk up to the nearest existing parent
        // so that missing intermediate directories are still allowed as long
        // as they would be created inside the root.
        let mut probe = joined.as_path();
        loop {
            if let Ok(canonical) = probe.canonicalize() {
                // Component-wise comparison (Path::starts_with) — see read branch.
                if !canonical.starts_with(&canonical_root) {
                    return Err(forbidden(rel_path));
                }
                return Ok(joined);
            }
            match probe.parent() {
                Some(parent) => probe = parent,
                None => {
                    return Err(CoreError::InvalidInput {
                        field: "chapter_path_forbidden".to_string(),
                        reason: format!(
                            "chapter path '{rel_path}' has no parent inside workspace root"
                        ),
                    });
                }
            }
        }
    }
}

/// Async wrapper around [`resolve_guarded_path`] that runs the blocking
/// `std::fs::canonicalize` syscalls on the tokio blocking pool. Shared by the
/// core content/outline services and the daemon surface (single W-002
/// authority); a spawn failure carries the legacy `PATH_GUARD_PANIC` code.
pub async fn resolve_guarded_path_async(
    workspace_root: PathBuf,
    rel_path: String,
    must_exist: bool,
) -> Result<PathBuf, CoreError> {
    tokio::task::spawn_blocking(move || {
        resolve_guarded_path(&workspace_root, &rel_path, must_exist)
    })
    .await
    .map_err(|e| CoreError::Internal {
        category: format!("PATH_GUARD_PANIC: path guard task panicked: {e}"),
    })?
}

/// Create the parent directories of a guarded target. Callers map the
/// I/O failure to their legacy `DIRECTORY_CREATE_ERROR` message.
pub(crate) async fn create_parent_dirs(parent: &Path) -> std::io::Result<()> {
    tokio::fs::create_dir_all(parent).await
}

/// Durability mechanics shared by every chapter/outline markdown write:
/// temp file + fsync + atomic rename + final-file fsync + parent-dir fsync.
/// The caller owns path-guard resolution, parent-directory creation and the
/// legacy error-code mapping; the temp file is cleaned up on failure.
pub(crate) async fn fsync_write_atomic(target: PathBuf, content: &str) -> std::io::Result<()> {
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
        Ok(())
    }
    .await;

    if let Err(e) = write_result {
        let _ = tokio::fs::remove_file(&temp_path).await;
        return Err(e);
    }
    Ok(())
}

/// Per-Work advisory runtime lock (single-writer authoring contract, DF-60 §4)
/// shared by the content and outline services. The low-level acquire/release
/// primitives live in `nexus_local_db::runtime_lock`; this guard only carries
/// the legacy locked/DATABASE_ERROR classification.
///
/// `holder_kind` on [`WorkLock::acquire`] is the caller label riding the
/// `cli:<kind>:<uuid>` holder format. The daemon content surface passes
/// `http` so the observable 423 `Locked.reason` keeps the legacy
/// `cli:http:<uuid>` holder string.
pub(crate) struct WorkLock {
    pool: sqlx::SqlitePool,
    creator_id: String,
    work_id: String,
    holder: String,
}

impl WorkLock {
    pub(crate) async fn acquire(
        pool: &sqlx::SqlitePool,
        creator_id: &str,
        work_id: &str,
        holder_kind: &str,
    ) -> Result<Self, CoreError> {
        let holder = nexus_local_db::cli_holder(holder_kind);
        let acquired = nexus_local_db::acquire_runtime_lock(
            pool,
            creator_id,
            work_id,
            &holder,
            nexus_local_db::ttl_from_env(),
            true,
        )
        .await
        .map_err(|e| CoreError::Internal {
            category: format!("DATABASE_ERROR: runtime_lock acquire failed: {e}"),
        })?;
        match acquired {
            nexus_local_db::AcquireResult::Acquired { .. } => Ok(Self {
                pool: pool.clone(),
                creator_id: creator_id.into(),
                work_id: work_id.into(),
                holder,
            }),
            nexus_local_db::AcquireResult::Locked {
                holder: existing, ..
            } => Err(CoreError::Forbidden {
                resource: format!(
                    "work_locked:work {work_id} is locked by '{existing}'; \
                         wait for release or check 'creator works status'"
                ),
            }),
        }
    }

    pub(crate) async fn release(self) {
        if let Err(error) = nexus_local_db::release_runtime_lock(
            &self.pool,
            &self.creator_id,
            &self.work_id,
            &self.holder,
        )
        .await
        {
            tracing::warn!(work_id = %self.work_id, %error, "runtime lock release failed");
        }
    }
}

// ─── Chapter content domain helpers ────────────────────────────────────────

/// Parse a chapter number path parameter.
fn parse_chapter(n: &str) -> Result<i32, ContentFault> {
    n.parse::<i32>()
        .map_err(|_| ContentFault::BadRequest {
            code: "invalid_chapter_number".to_string(),
            message: format!("chapter number must be a positive integer, got '{n}'"),
        })
        .and_then(|v| {
            if v < 1 {
                Err(ContentFault::BadRequest {
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
fn decode_chapter_cursor(cursor: Option<&String>) -> Result<(i32, i32), ContentFault> {
    let invalid = |message: &str| ContentFault::BadRequest {
        code: "invalid_input".to_string(),
        message: message.to_string(),
    };
    match cursor {
        None => Ok((1, 0)),
        Some(raw) => {
            let stripped = raw.strip_prefix(CHAPTER_CURSOR_PREFIX).ok_or_else(|| {
                invalid("invalid chapter_cursor; pass the next_cursor value unchanged")
            })?;
            let mut parts = stripped.splitn(2, ':');
            let volume = parts
                .next()
                .and_then(|s| s.parse::<i32>().ok())
                .filter(|v| *v >= 1)
                .ok_or_else(|| invalid("invalid chapter_cursor volume"))?;
            let chapter = parts
                .next()
                .and_then(|s| s.parse::<i32>().ok())
                .filter(|v| *v >= 1)
                .ok_or_else(|| invalid("invalid chapter_cursor chapter"))?;
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

/// Compute protection metadata for a chapter based on its status.
fn chapter_protection(status: &str) -> Result<ChapterProtection, ContentFault> {
    let (level, reason) = match status {
        "finalized" => (
            "confirm_structure_edit",
            "Chapter is finalized; structural edits require confirmation.",
        ),
        "published" => (
            "hard_block_delete",
            "Chapter is published; structural edits are blocked.",
        ),
        _ => ("none", "No protection."),
    };
    Ok(ChapterProtection {
        level: wire_cast(level.to_string())?,
        reason: reason.to_string(),
    })
}

/// Map a DB record to a `ChapterSummary` contract DTO.
fn to_summary(r: &WorkChapterRecord) -> Result<ChapterSummary, ContentFault> {
    Ok(ChapterSummary {
        work_id: r.work_id.clone(),
        chapter: nz(i64::from(r.chapter)),
        volume: nz(i64::from(r.volume.unwrap_or(1))),
        title: None,
        slug: r.slug.clone(),
        planned_word_count: u64::try_from(r.planned_word_count).unwrap_or(0),
        actual_word_count: r.actual_word_count.map(|v| u64::try_from(v).unwrap_or(0)),
        status: wire_cast(r.status.parse().ok().unwrap_or(ChapterStatus::NotStarted))?,
        outline_path: r.outline_path.clone(),
        body_path: r.body_path.clone(),
        created_at: r.created_at.clone(),
        updated_at: r.updated_at.clone(),
    })
}

/// Map a DB record to a `ChapterDetail` contract DTO.
///
/// `workspace_root` is used to probe whether the stored `outline_path` is still
/// inside the active workspace; if the root is unavailable, the outline is
/// reported as non-editable rather than failing the whole detail request.
fn to_detail(
    r: &WorkChapterRecord,
    workspace_root: Option<&Path>,
) -> Result<ChapterDetail, ContentFault> {
    let can_edit_outline = r
        .outline_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .and_then(|path| workspace_root.map(|root| resolve_guarded_path(root, path, false).is_ok()))
        .unwrap_or(false);

    Ok(ChapterDetail {
        work_id: r.work_id.clone(),
        chapter: nz(i64::from(r.chapter)),
        volume: nz(i64::from(r.volume.unwrap_or(1))),
        title: None,
        slug: r.slug.clone(),
        planned_word_count: u64::try_from(r.planned_word_count).unwrap_or(0),
        actual_word_count: r.actual_word_count.map(|v| u64::try_from(v).unwrap_or(0)),
        status: wire_cast(r.status.parse().ok().unwrap_or(ChapterStatus::NotStarted))?,
        outline_path: r.outline_path.clone(),
        body_path: r.body_path.clone(),
        created_at: r.created_at.clone(),
        updated_at: r.updated_at.clone(),
        can_edit_outline,
        can_edit_structure: true,
        body_read_only: true,
        protection: wire_cast(chapter_protection(&r.status)?)?,
    })
}

/// `i64 → NonZeroU64` with the legacy clamp-to-1 fallback for degenerate rows.
fn nz(value: i64) -> NonZeroU64 {
    NonZeroU64::new(u64::try_from(value).unwrap_or(1)).unwrap_or(NonZeroU64::MIN)
}

/// Read a text file after path-guard verification.
///
/// Enforces a 10 MiB size cap to prevent unbounded memory reads on
async fn read_guarded_file(
    workspace_root: &Path,
    rel_path: &str,
    forbidden_code: &str,
    not_found_code: &str,
) -> Result<String, ContentFault> {
    const CHAPTER_BODY_MAX_BYTES: usize = 10 * 1024 * 1024;

    let path = resolve_guarded_path_async(workspace_root.to_path_buf(), rel_path.to_string(), true)
        .await
        .map_err(|e| match &e {
            CoreError::InvalidInput { field, .. } if field == "chapter_path_forbidden" => {
                ContentFault::BadRequest {
                    code: forbidden_code.to_string(),
                    message: format!("chapter path '{rel_path}' escapes workspace root"),
                }
            }
            _ => ContentFault::Core(e),
        })?;

    let metadata = tokio::fs::metadata(&path).await.map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            ContentFault::NotFound(format!("{not_found_code}: file not found at '{rel_path}'"))
        } else {
            ContentFault::Internal {
                code: "FILE_READ_ERROR".to_string(),
                message: format!("failed to read metadata for '{rel_path}': {e}"),
            }
        }
    })?;

    let max_bytes = u64::try_from(CHAPTER_BODY_MAX_BYTES).unwrap_or(u64::MAX);
    if metadata.len() > max_bytes {
        return Err(ContentFault::BadRequest {
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
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Err(ContentFault::NotFound(format!(
            "{not_found_code}: file not found at '{rel_path}'"
        ))),
        Err(e) => Err(ContentFault::Internal {
            code: "FILE_READ_ERROR".to_string(),
            message: format!("failed to read '{rel_path}': {e}"),
        }),
    }
}

/// Atomically write plain `content` to `rel_path` under `workspace_root`,
/// creating parent directories as needed (temp+rename+file fsync+dir fsync).
///
/// Persist per-chapter outline prose to the `outline_path` markdown file with
/// the same durability pattern as every chapter-file write path.
pub(crate) async fn atomic_write_text(
    workspace_root: &Path,
    rel_path: &str,
    content: &str,
) -> Result<(), CoreError> {
    let target =
        resolve_guarded_path_async(workspace_root.to_path_buf(), rel_path.to_string(), false)
            .await?;

    if let Some(parent) = target.parent() {
        create_parent_dirs(parent)
            .await
            .map_err(|e| CoreError::Internal {
                category: format!(
                "DIRECTORY_CREATE_ERROR: failed to create parent directories for '{rel_path}': {e}"
            ),
            })?;
    }

    fsync_write_atomic(target, content)
        .await
        .map_err(|e| CoreError::Internal {
            category: format!("OUTLINE_WRITE_ERROR: failed to write outline to '{rel_path}': {e}"),
        })
}

/// Validate that a requested chapter status transition is allowed.
fn validate_status_transition(from: &str, to: &str) -> Result<(), ContentFault> {
    if from == to {
        return Ok(());
    }
    match (from, to) {
        ("not_started", "outlined") => Ok(()),
        _ => Err(ContentFault::BadRequest {
            code: "chapter_status_transition_invalid".to_string(),
            message: format!(
                "status transition '{from}' -> '{to}' is not allowed through this endpoint"
            ),
        }),
    }
}

impl CoreService {
    /// Best-effort workspace root for editability probes (`None` when unset).
    pub(crate) fn optional_workspace_root(
        &self,
        principal: &Principal,
    ) -> Result<Option<PathBuf>, CoreError> {
        Ok(self
            .work_workspace_path(principal)?
            .filter(|s| !s.is_empty())
            .map(PathBuf::from))
    }

    /// Required workspace root; `Uninitialized` mirrors the pre-extraction
    /// handler behavior when no active workspace root is configured.
    pub(crate) fn workspace_root(&self, principal: &Principal) -> Result<PathBuf, CoreError> {
        self.optional_workspace_root(principal)?
            .ok_or(CoreError::Uninitialized)
    }

    /// Cursor-paginated chapter summaries for a Work.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::NotFound`] for an unknown or foreign Work,
    /// [`CoreError::InvalidInput`] for a malformed cursor, and
    /// [`CoreError::Internal`] (legacy `DATABASE_ERROR`/`CONTRACT_ERROR`
    /// carriers) for storage or contract faults.
    pub async fn list_chapters(
        &self,
        principal: &Principal,
        work_id: String,
        query: ListChaptersQuery,
    ) -> CoreResult<ListChaptersResponse> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        let response = list_chapters(self, &work_id, query)
            .await
            .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(response)
    }

    /// Chapter detail (metadata + protection + editability probe).
    ///
    /// # Errors
    /// As [`CoreService::list_chapters`]; additionally
    /// [`CoreError::InvalidInput`] for a non-positive chapter number.
    pub async fn chapter_detail(
        &self,
        principal: &Principal,
        work_id: String,
        chapter_id: String,
        query: CoreChapterContentQuery,
    ) -> CoreResult<ChapterDetail> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        let root = self.optional_workspace_root(principal)?;
        let detail = chapter_detail(self, &work_id, &chapter_id, query, root.as_deref())
            .await
            .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(detail)
    }

    /// Read the per-chapter outline markdown.
    ///
    /// # Errors
    /// As [`CoreService::chapter_detail`]; additionally [`CoreError::NotFound`]
    /// when the chapter has no `outline_path` or the file is missing, and
    /// [`CoreError::InvalidInput`] with `chapter_outline_path_forbidden` when
    /// the stored path escapes the workspace root.
    pub async fn chapter_outline(
        &self,
        principal: &Principal,
        work_id: String,
        chapter_id: String,
        query: CoreChapterContentQuery,
    ) -> CoreResult<ChapterOutline> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        let root = self.workspace_root(principal)?;
        let outline = chapter_outline(self, &work_id, &chapter_id, query, &root)
            .await
            .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(outline)
    }

    /// Read the chapter body markdown (read-only).
    ///
    /// # Errors
    /// As [`CoreService::chapter_detail`]; additionally [`CoreError::NotFound`]
    /// when the chapter has no `body_path` or the file is missing,
    /// [`CoreError::InvalidInput`] with `chapter_body_path_forbidden` when the
    /// stored path escapes the workspace root, and `chapter_body_too_large`
    /// beyond the 10 MiB cap.
    pub async fn chapter_body(
        &self,
        principal: &Principal,
        work_id: String,
        chapter_id: String,
        query: CoreChapterContentQuery,
    ) -> CoreResult<ChapterBody> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        let root = self.workspace_root(principal)?;
        let body = chapter_body(self, &work_id, &chapter_id, query, &root)
            .await
            .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(body)
    }

    /// Partial structure update of a chapter (slug/word count/volume/status).
    ///
    /// `holder` is the per-Work runtime-lock caller label (see
    /// [`WorkLock::acquire`]); the daemon content surface passes `http`.
    ///
    /// # Errors
    /// As [`CoreService::chapter_detail`]; additionally
    /// [`CoreError::Forbidden`] under read-only core access or when the Work
    /// is runtime-locked, and [`CoreError::InvalidInput`] with the legacy
    /// `chapter_title_unsupported` / `chapter_status_transition_invalid` /
    /// `chapter_structure_edit_blocked` /
    /// `chapter_structure_confirmation_required` codes.
    pub async fn patch_chapter(
        &self,
        principal: &Principal,
        holder: &str,
        work_id: String,
        chapter_id: String,
        query: CoreChapterContentQuery,
        request: PatchChapterRequest,
    ) -> CoreResult<ChapterDetail> {
        self.verify_principal(principal)?;
        self.resolve_owned_work(principal, &work_id).await?;
        self.require_work_write()?;
        let root = self.optional_workspace_root(principal)?;
        let detail = patch_chapter(
            self,
            principal,
            holder,
            &work_id,
            &chapter_id,
            query,
            request,
            root.as_deref(),
        )
        .await
        .map_err(CoreError::from)?;
        self.verify_principal(principal)?;
        Ok(detail)
    }
}

async fn list_chapters(
    service: &CoreService,
    work_id: &str,
    query: ListChaptersQuery,
) -> Result<ListChaptersResponse, ContentFault> {
    let pool = &service.inner.pool;

    let (cursor_volume, cursor_chapter) = decode_chapter_cursor(query.cursor.as_ref())?;
    // Clamp to [1, 100]: the generated `NonZeroU64` limit type enforces the
    // schema `minimum: 1` at deserialization (`?limit=0` rejects as 400
    // instead of reaching `chapter_page_meta`), so only the upper bound is
    // clamped here. Default 50 when absent.
    let limit = u32::try_from(query.limit.map_or(50, |l| l.get().min(100))).unwrap_or(50);
    let fetch_limit = i64::from(limit.saturating_add(1));

    let status_filter = query
        .status
        .as_ref()
        .map(nexus_contracts::list_chapters_query::NexusChapterStatus::as_str);

    let records = work_chapters::list_chapters_paginated(
        pool,
        work_id,
        status_filter,
        fetch_limit,
        cursor_volume,
        cursor_chapter,
    )
    .await
    .map_err(|e| ContentFault::Internal {
        code: "DATABASE_ERROR".to_string(),
        message: e.to_string(),
    })?;

    let (next_cursor, has_more) = chapter_page_meta(&records, limit);
    let mut items = Vec::new();
    for r in records
        .iter()
        .take(usize::try_from(limit).unwrap_or(usize::MAX))
    {
        items.push(to_summary(r)?);
    }

    Ok(ListChaptersResponse {
        items: wire_cast(items)?,
        pagination: wire_cast(PaginationInfo {
            limit: i64::from(limit),
            next_cursor,
            has_more,
        })?,
    })
}

async fn load_chapter(
    service: &CoreService,
    work_id: &str,
    chapter: i32,
    volume: i32,
) -> Result<WorkChapterRecord, ContentFault> {
    work_chapters::get_chapter(&service.inner.pool, work_id, chapter, volume)
        .await
        .map_err(|e| ContentFault::Internal {
            code: "DATABASE_ERROR".to_string(),
            message: e.to_string(),
        })?
        .ok_or_else(|| ContentFault::NotFound(format!("chapter {chapter} volume {volume}")))
}

fn chapter_volume(query: CoreChapterContentQuery) -> i32 {
    query
        .volume
        .and_then(|v| i32::try_from(u64::from(v)).ok())
        .unwrap_or(1)
}

async fn chapter_detail(
    service: &CoreService,
    work_id: &str,
    chapter_id: &str,
    query: CoreChapterContentQuery,
    root: Option<&Path>,
) -> Result<ChapterDetail, ContentFault> {
    let chapter = parse_chapter(chapter_id)?;
    let volume = chapter_volume(query);
    let record = load_chapter(service, work_id, chapter, volume).await?;
    Ok(to_detail(&record, root)?)
}

async fn chapter_outline(
    service: &CoreService,
    work_id: &str,
    chapter_id: &str,
    query: CoreChapterContentQuery,
    workspace_root: &Path,
) -> Result<ChapterOutline, ContentFault> {
    let chapter = parse_chapter(chapter_id)?;
    let volume = chapter_volume(query);
    let record = load_chapter(service, work_id, chapter, volume).await?;

    let outline_path = record
        .outline_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ContentFault::NotFound(format!(
                "chapter {chapter} volume {volume} has no outline_path"
            ))
        })?;

    let content = read_guarded_file(
        workspace_root,
        outline_path,
        "chapter_outline_path_forbidden",
        "chapter_outline_not_found",
    )
    .await?;

    Ok(ChapterOutline {
        work_id: record.work_id,
        chapter: nz(i64::from(record.chapter)),
        volume: nz(i64::from(record.volume.unwrap_or(1))),
        outline_path: outline_path.to_string(),
        content,
        updated_at: record.updated_at,
    })
}

async fn chapter_body(
    service: &CoreService,
    work_id: &str,
    chapter_id: &str,
    query: CoreChapterContentQuery,
    workspace_root: &Path,
) -> Result<ChapterBody, ContentFault> {
    let chapter = parse_chapter(chapter_id)?;
    let volume = chapter_volume(query);
    let record = load_chapter(service, work_id, chapter, volume).await?;

    let body_path = record
        .body_path
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            ContentFault::NotFound(format!(
                "chapter {chapter} volume {volume} has no body_path"
            ))
        })?;

    let content = read_guarded_file(
        workspace_root,
        body_path,
        "chapter_body_path_forbidden",
        "chapter_body_not_found",
    )
    .await?;

    Ok(ChapterBody {
        work_id: record.work_id,
        chapter: nz(i64::from(record.chapter)),
        volume: nz(i64::from(record.volume.unwrap_or(1))),
        body_path: body_path.to_string(),
        content,
        frontmatter: serde_json::Map::new(),
        read_only: true,
        updated_at: record.updated_at,
    })
}

#[allow(clippy::too_many_lines)]
async fn patch_chapter(
    service: &CoreService,
    principal: &Principal,
    holder: &str,
    work_id: &str,
    chapter_id: &str,
    query: CoreChapterContentQuery,
    req: PatchChapterRequest,
    root: Option<&Path>,
) -> Result<ChapterDetail, ContentFault> {
    let pool = &service.inner.pool;
    let creator_id = principal.creator_id();
    let chapter = parse_chapter(chapter_id)?;
    let volume = chapter_volume(query);

    let record = load_chapter(service, work_id, chapter, volume).await?;

    // Reject display-only title writes in V1.65. This is field validation, not
    // a preset gate, so use BadRequest (HTTP 400, code `bad_request`) rather
    // than PresetGatesFailed (HTTP 422, `preset_gates_failed`) — the previous
    // observable contract and the semantic meaning of both error categories.
    if req.title.is_some() {
        return Err(ContentFault::BadRequest {
            code: "chapter_title_unsupported".to_string(),
            message: "title is display-only in V1.65; use outline frontmatter or slug instead"
                .to_string(),
        });
    }

    // Validate status transition before acquiring lock.
    if let Some(target_status) = &req.status {
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
                return Err(ContentFault::BadRequest {
                    code: "chapter_structure_edit_blocked".to_string(),
                    message: "structural edits to published chapters are blocked".to_string(),
                });
            }
            "finalized" if !req.confirm_structural_edit.unwrap_or(false) => {
                return Err(ContentFault::BadRequest {
                    code: "chapter_structure_confirmation_required".to_string(),
                    message: "set confirm_structural_edit=true to edit a finalized chapter"
                        .to_string(),
                });
            }
            _ => {}
        }
    }
    // Acquire runtime lock before mutating DB metadata.
    let lock = WorkLock::acquire(pool, creator_id, work_id, holder).await?;

    let updated: Result<WorkChapterRecord, ContentFault> = async {
        let patch = PatchChapterParams {
            slug: req.slug.map(|s| s.to_string()),
            planned_word_count: req
                .planned_word_count
                .map(|v| i32::try_from(v).unwrap_or(i32::MAX)),
            volume: req.volume.map(|v| i32::try_from(u64::from(v)).unwrap_or(1)),
            status: req.status.as_ref().map(ToString::to_string),
        };

        let now = chrono::Utc::now().to_rfc3339();
        work_chapters::patch_chapter(pool, work_id, chapter, volume, &patch, &now)
            .await
            .map_err(|e| ContentFault::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .then_some(())
            .ok_or_else(|| ContentFault::NotFound(format!("chapter {chapter} volume {volume}")))?;

        // Re-fetch using the POST-patch volume. If `req.volume` changed it, the
        // row now lives at the new volume; re-fetching with the original query
        // `volume` would miss it and return 404 on a fully-committed write.
        let fetch_volume = patch.volume.unwrap_or(volume);
        work_chapters::get_chapter(pool, work_id, chapter, fetch_volume)
            .await
            .map_err(|e| ContentFault::Internal {
                code: "DATABASE_ERROR".to_string(),
                message: e.to_string(),
            })?
            .ok_or_else(|| {
                ContentFault::NotFound(format!("chapter {chapter} volume {fetch_volume}"))
            })
    }
    .await;

    lock.release().await;
    let record = updated?;
    Ok(to_detail(&record, root)?)
}
