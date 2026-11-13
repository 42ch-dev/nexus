//! Candidate strategy drafts: one mutable `strategy_candidates` row per strategy.
//!
//! Plan: `.mstar/plans/v239-p3-compose-ops.md` §Candidate storage (Q4) / D17.
//!
//! The row `revision` is the single compare-and-swap authority: a write is one
//! conditional statement keyed by `(strategy_id, revision)`, and the embedded
//! `document.revision` is rewritten to the new row revision in that same
//! statement. A refused CAS writes nothing — its path reads a revision and
//! returns the typed conflict, never a repair or a merge.
//!
//! A strategy with no row is not an error: the canonical read projects the
//! `workflow-ir` import at revision 0 and persists nothing. The first write at
//! `base_revision: 0` inserts revision 1, applying its operation to that
//! imported draft. The canonical content hash is recomputed from the document
//! through `nexus-workflow-ir`; it is never a stored column.

use std::path::Path;

use nexus_preset::capability_catalog::BuiltinCapabilityCatalog;
use nexus_preset::workflow_ir::{import_preset_bundle, import_preset_manifest};
use nexus_workflow_ir::{content_hash, WorkflowDocument, WorkflowDocumentState};
use sqlx::SqlitePool;

use crate::error::local_db_err;
use crate::{
    CoreError, CoreResult, CoreService, PresetError, Principal, StrategyCandidateConflictError,
};

/// The canonical candidate document plus the identity derived from it.
#[derive(Debug, Clone)]
pub struct StrategyCandidate {
    /// Row revision; `0` for the unpersisted import projection.
    pub revision: u64,
    /// The complete document; its embedded `revision` mirrors [`Self::revision`].
    pub document: WorkflowDocument,
    /// Canonical hash recomputed from the document through `nexus-workflow-ir`.
    pub content_hash: String,
}

/// The revision-0 projection of an installed strategy, shared with the
/// `workflow-ir` read route so there is one import adapter, not two.
///
/// # Errors
///
/// Returns the preset-family errors when the id does not resolve or cannot be
/// projected into an IR document.
pub(crate) fn imported_strategy_document(
    nexus_home: &Path,
    strategy_id: &str,
) -> CoreResult<WorkflowDocument> {
    let caps = BuiltinCapabilityCatalog;
    let (_, bundle_dir) =
        crate::presets::locate_preset(nexus_home, strategy_id).map_err(CoreError::from)?;
    let loaded = match nexus_preset::lookup_preset_by_id(strategy_id, nexus_home, &caps) {
        Some(loaded) => loaded,
        None => nexus_preset::resolve_preset(strategy_id, nexus_home, &caps).map_err(|error| {
            PresetError::NotFound(format!("preset '{strategy_id}' not found: {error}"))
        })?,
    };
    match bundle_dir {
        Some(dir) => import_preset_bundle(&loaded.manifest, &dir).map(|imported| imported.document),
        None => import_preset_manifest(&loaded.manifest),
    }
    .map_err(|error| CoreError::InvalidInput {
        field: "strategy".into(),
        reason: error.to_string(),
    })
}

fn candidate_internal(detail: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("strategy candidate: {detail}"),
    }
}

fn storage_error(error: sqlx::Error) -> CoreError {
    local_db_err(error.into())
}

/// The typed stale-base refusal of §Q2. Nothing has been written when it returns.
fn conflict(strategy_id: &str, base_revision: u64, current_revision: u64) -> CoreError {
    CoreError::StrategyCandidateConflict(StrategyCandidateConflictError {
        strategy_id: strategy_id.to_string(),
        base_revision,
        current_revision,
        conflicting_path: "$".to_string(),
        recovery_hint:
            "Reload the current candidate and explicitly reapply the compose operation.".to_string(),
    })
}

async fn load_stored(
    pool: &SqlitePool,
    strategy_id: &str,
) -> CoreResult<Option<(String, i64)>> {
    sqlx::query_as(
        "SELECT document_json, revision FROM strategy_candidates WHERE strategy_id = ?",
    )
    .bind(strategy_id)
    .fetch_optional(pool)
    .await
    .map_err(storage_error)
}

async fn current_revision(pool: &SqlitePool, strategy_id: &str) -> CoreResult<u64> {
    let revision: Option<i64> =
        sqlx::query_scalar("SELECT revision FROM strategy_candidates WHERE strategy_id = ?")
            .bind(strategy_id)
            .fetch_optional(pool)
            .await
            .map_err(storage_error)?;
    let revision = revision.unwrap_or(0);
    u64::try_from(revision)
        .map_err(|_| candidate_internal(format!("stored revision {revision} is negative")))
}

/// Project one stored row. The schema CHECKs already pin the document's id,
/// state and revision to the row; a mismatch here means the row was tampered
/// with outside the store, so this fails rather than serving it.
fn decode_stored(
    strategy_id: &str,
    document_json: &str,
    revision: i64,
) -> CoreResult<StrategyCandidate> {
    let revision = u64::try_from(revision)
        .map_err(|_| candidate_internal(format!("stored revision {revision} is negative")))?;
    let document: WorkflowDocument = serde_json::from_str(document_json)
        .map_err(|error| candidate_internal(format!("stored document does not decode: {error}")))?;
    if document.revision != revision
        || document.id.as_str() != strategy_id
        || document.state != WorkflowDocumentState::Candidate
    {
        return Err(candidate_internal(format!(
            "stored row '{strategy_id}' disagrees with its document"
        )));
    }
    let hash = content_hash(&document)
        .map_err(|error| candidate_internal(format!("canonical hash: {error}")))?;
    Ok(StrategyCandidate {
        revision,
        document,
        content_hash: hash,
    })
}

impl CoreService {
    /// The canonical candidate: the stored row when present, otherwise the
    /// revision-0 import projection (never persisted).
    ///
    /// # Errors
    ///
    /// Returns `CoreError` when the principal is rejected, the strategy does not
    /// resolve to a preset and has no row, or a stored row cannot be projected.
    pub async fn read_strategy_candidate(
        &self,
        principal: &Principal,
        strategy_id: &str,
    ) -> CoreResult<StrategyCandidate> {
        self.verify_principal(principal)?;
        match load_stored(&self.inner.pool, strategy_id).await? {
            Some((document_json, revision)) => {
                decode_stored(strategy_id, &document_json, revision)
            }
            None => self.imported_candidate(strategy_id),
        }
    }

    /// Apply `apply` to the candidate at `base_revision` and commit it as
    /// `base_revision + 1`, or refuse with the typed conflict.
    ///
    /// The write is one statement — an insert at `base_revision: 0`, otherwise a
    /// conditional update — so a stale base or a concurrent writer changes
    /// nothing. `apply` runs before the write: its error leaves the row intact.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::StrategyCandidateConflict`] when the stored revision
    /// is not `base_revision`, the `apply` error unchanged, and `invalid input`
    /// when the produced document's id or state disagrees with the candidate row.
    pub async fn commit_strategy_candidate<F>(
        &self,
        principal: &Principal,
        strategy_id: &str,
        base_revision: u64,
        apply: F,
    ) -> CoreResult<StrategyCandidate>
    where
        F: FnOnce(&WorkflowDocument) -> CoreResult<WorkflowDocument>,
    {
        self.verify_principal(principal)?;
        let stored = load_stored(&self.inner.pool, strategy_id).await?;
        let current = match &stored {
            Some((document_json, revision)) => decode_stored(strategy_id, document_json, *revision)?,
            None => self.imported_candidate(strategy_id)?,
        };
        if current.revision != base_revision {
            return Err(conflict(strategy_id, base_revision, current.revision));
        }

        let mut document = apply(&current.document)?;
        let new_revision = base_revision.checked_add(1).ok_or_else(|| {
            CoreError::InvalidInput {
                field: "base_revision".into(),
                reason: "revision is exhausted".into(),
            }
        })?;
        if document.id.as_str() != strategy_id {
            return Err(CoreError::InvalidInput {
                field: "document.id".into(),
                reason: format!(
                    "document id '{}' does not match candidate '{strategy_id}'",
                    document.id.as_str()
                ),
            });
        }
        if document.state != WorkflowDocumentState::Candidate {
            return Err(CoreError::InvalidInput {
                field: "document.state".into(),
                reason: format!("a candidate must stay in state `candidate`, got `{}`", document.state),
            });
        }
        document.revision = new_revision;
        let document_json = serde_json::to_string(&document)
            .map_err(|error| candidate_internal(format!("document does not encode: {error}")))?;
        let hash = content_hash(&document)
            .map_err(|error| candidate_internal(format!("canonical hash: {error}")))?;
        let revision = i64::try_from(new_revision)
            .map_err(|_| candidate_internal("revision does not fit the row column"))?;
        let base = i64::try_from(base_revision)
            .map_err(|_| candidate_internal("base revision does not fit the row column"))?;
        let now = chrono::Utc::now().to_rfc3339();

        let written = if stored.is_some() {
            sqlx::query(
                "UPDATE strategy_candidates \
                 SET document_json = ?, revision = ?, updated_at = ? \
                 WHERE strategy_id = ? AND revision = ?",
            )
            .bind(&document_json)
            .bind(revision)
            .bind(&now)
            .bind(strategy_id)
            .bind(base)
            .execute(&self.inner.pool)
            .await
            .map_err(storage_error)?
            .rows_affected()
        } else {
            sqlx::query(
                "INSERT INTO strategy_candidates \
                 (strategy_id, document_json, revision, created_at, updated_at) \
                 VALUES (?, ?, ?, ?, ?) ON CONFLICT(strategy_id) DO NOTHING",
            )
            .bind(strategy_id)
            .bind(&document_json)
            .bind(revision)
            .bind(&now)
            .bind(&now)
            .execute(&self.inner.pool)
            .await
            .map_err(storage_error)?
            .rows_affected()
        };
        if written == 0 {
            let current_revision = current_revision(&self.inner.pool, strategy_id).await?;
            return Err(conflict(strategy_id, base_revision, current_revision));
        }

        Ok(StrategyCandidate {
            revision: new_revision,
            document,
            content_hash: hash,
        })
    }

    fn imported_candidate(&self, strategy_id: &str) -> CoreResult<StrategyCandidate> {
        let document = imported_strategy_document(&self.inner.nexus_home, strategy_id)?;
        let hash = content_hash(&document)
            .map_err(|error| candidate_internal(format!("canonical hash: {error}")))?;
        Ok(StrategyCandidate {
            revision: 0,
            document,
            content_hash: hash,
        })
    }
}
