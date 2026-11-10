//! Work-entry extract queue over the guarded core workspace
//! (`creator world kb queue-extract` / `extract-status`; core producer added by
//! v1.207 P1 item 6, operator entrance rebound by v231).
//! Extracted from the legacy daemon `kb.rs` handlers. This is NOT the World
//! narrative KB graph (`nexus-kb` / P0 `world_kb`).

use nexus_home_layout::sanitize_entry_id;
use nexus_local_db::kb_extract_job::KbExtractJob;

use crate::error::db_err;
use crate::{CoreError, CoreResult, CoreService, Principal};

/// Knowledge-family fault carrying the legacy classification until the
/// adapter boundary. `InvalidInput` keeps the legacy 400 mapping.
#[derive(Debug, thiserror::Error)]
enum KnowledgeFault {
    #[error("invalid input: {reason}")]
    InvalidInput { field: String, reason: String },
    /// Foreign-creator entry access; carries the `kb_owner:` prefix so the
    /// adapter can re-emit the legacy `Forbidden { resource, reason }` pair.
    #[error("forbidden: {0}")]
    ForeignEntry(String),
    #[error(transparent)]
    Core(#[from] CoreError),
}

impl From<KnowledgeFault> for CoreError {
    fn from(error: KnowledgeFault) -> Self {
        match error {
            KnowledgeFault::InvalidInput { field, reason } => Self::InvalidInput { field, reason },
            KnowledgeFault::ForeignEntry(resource) => Self::Forbidden { resource },
            KnowledgeFault::Core(error) => error,
        }
    }
}

fn invalid_input(field: &str, reason: impl Into<String>) -> KnowledgeFault {
    KnowledgeFault::InvalidInput {
        field: field.to_string(),
        reason: reason.into(),
    }
}

// ─── Work-entry extract queue (`creator world kb queue-extract`) ──────────

/// Input for [`CoreService::queue_kb_extract`].
///
/// The creator and the workspace binding are derived from the verified
/// principal (and the admission the open already took), so a caller cannot
/// enqueue into a workspace it is not admitted to.
#[derive(Debug, Clone)]
pub struct QueueKbExtractParams {
    /// Idempotency label for the job.
    ///
    /// The extractor reads the Work artifact named by [`Self::work_id`] /
    /// [`Self::chapter`]; this label only keys the job, so it never resolves
    /// to a work-scope file-index entry.
    pub work_entry_id: String,
    /// Target world ID for the resulting `KnowledgeEntryRecord`.
    pub world_id: String,
    /// Source work ID (parent of the chapter artifact).
    pub work_id: Option<String>,
    /// Chapter number sugar: `Some(n)` selects the chapter artifact locator.
    pub chapter: Option<i32>,
}

/// The artifact-locator triple `(source_kind, source_locator, profile_hint)`
/// the `--chapter` sugar implies.
type ArtifactLocator = (Option<String>, Option<String>, Option<String>);

/// The artifact-locator triple implied by the `--chapter N` sugar.
///
/// A chapter below 1 names no chapter, so the producer refuses it rather than
/// enqueueing a job whose locator cannot resolve. The refusal sentence is the
/// legacy CLI wording, kept verbatim so the transport that reaches the same
/// core failure reads the same message.
fn chapter_locator(chapter: Option<i32>) -> Result<ArtifactLocator, KnowledgeFault> {
    let Some(ch) = chapter else {
        return Ok((None, None, None));
    };
    if ch < 1 {
        return Err(invalid_input("chapter", "Chapter number must be >= 1"));
    }
    Ok((
        Some("work_chapter".to_string()),
        Some(format!("chapter:{ch:02}")),
        Some("novel".to_string()),
    ))
}

impl CoreService {
    // ── Work-entry extract queue (`creator world kb queue-extract` / `extract-status`) ──

    /// Enqueue a work-scope entry for extraction into a target world.
    ///
    /// Creates a row in `kb_extract_jobs` with status `queued`; the actual
    /// extraction is performed by the `kb.extract_work` capability, so no LLM
    /// call happens here. Idempotent on the non-failed
    /// `(creator, work_entry_id, world_id)` key the store enforces, so a
    /// re-queue returns the existing job unchanged.
    ///
    /// The producer owns the entry-id sanitization, the `--chapter` locator
    /// sugar and the creator/workspace binding, so no caller can enqueue a
    /// half-shaped job or write into a workspace it was not admitted to.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::Forbidden`] under read-only core access,
    /// [`CoreError::InvalidInput`] for an unsafe `work_entry_id` or a
    /// `chapter` below 1, and the storage carrier when the insert fails.
    pub async fn queue_kb_extract(
        &self,
        principal: &Principal,
        params: QueueKbExtractParams,
    ) -> CoreResult<KbExtractJob> {
        self.verify_principal(principal)?;
        self.require_work_write()?;

        let entry_id = sanitize_entry_id(&params.work_entry_id)
            .map_err(|reason| invalid_input("work_entry_id", reason))?;
        let (source_kind, source_locator, profile_hint) = chapter_locator(params.chapter)?;

        nexus_local_db::enqueue_extract_job_with_artifact(
            &self.inner.pool,
            principal.creator_id(),
            principal.workspace_slug(),
            entry_id,
            &params.world_id,
            source_kind.as_deref(),
            source_locator.as_deref(),
            profile_hint.as_deref(),
            params.work_id.as_deref(),
        )
        .await
        .map_err(|e| db_err(&e))
    }

    /// List the admitted creator's extract jobs, newest first, bounded by
    /// `limit`.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification and the storage carrier when the read fails.
    pub async fn list_kb_extract_jobs(
        &self,
        principal: &Principal,
        limit: u32,
    ) -> CoreResult<Vec<KbExtractJob>> {
        self.verify_principal(principal)?;
        nexus_local_db::list_extract_jobs(&self.inner.pool, principal.creator_id(), limit)
            .await
            .map_err(|e| db_err(&e))
    }

    /// Fetch one extract job by ID for the admitted creator; `None` when no
    /// such job exists.
    ///
    /// A job that exists but belongs to another creator is refused with
    /// [`CoreError::Forbidden`] — the same `kb_owner:` classification the
    /// work-index reads use — rather than reported as absent, so ownership
    /// stays classified the same way on every read.
    ///
    /// # Errors
    /// As [`CoreService::list_kb_extract_jobs`] plus [`CoreError::Forbidden`]
    /// for a foreign job.
    pub async fn get_kb_extract_job(
        &self,
        principal: &Principal,
        job_id: &str,
    ) -> CoreResult<Option<KbExtractJob>> {
        self.verify_principal(principal)?;
        let job = nexus_local_db::get_extract_job(&self.inner.pool, job_id)
            .await
            .map_err(|e| db_err(&e))?;
        match job {
            Some(job) if job.creator_id != principal.creator_id() => {
                Err(KnowledgeFault::ForeignEntry(format!("kb_owner:extract job {job_id}")).into())
            }
            other => Ok(other),
        }
    }
}

