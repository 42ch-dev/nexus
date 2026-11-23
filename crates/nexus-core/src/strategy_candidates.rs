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
use nexus_preset::workflow_ir::{import_preset_bundle, import_preset_manifest, FsAssetSource};
use nexus_workflow_ir::{content_hash, WorkflowDocument, WorkflowDocumentState};
use sqlx::SqlitePool;

#[cfg(feature = "strategy-authoring")]
use nexus_contracts::SimulationTrajectory;
#[cfg(feature = "strategy-authoring")]
use nexus_workflow_simulate::simulate_document;

use crate::error::local_db_err;
use crate::{
    CoreError, CoreResult, CoreService, PresetError, Principal, StrategyCandidateConflictError,
};

/// The canonical candidate document plus the identity derived from it.
#[derive(Debug, Clone, serde::Serialize)]
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
pub fn imported_strategy_document(
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
        Some(dir) => {
            let source = FsAssetSource::new(&dir).map_err(|source| CoreError::InvalidInput {
                field: "strategy".into(),
                reason: format!("open preset bundle `{}`: {source}", dir.display()),
            })?;
            import_preset_bundle(&loaded.manifest, &source).map(|imported| imported.document)
        }
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
pub fn conflict(strategy_id: &str, base_revision: u64, current_revision: u64) -> CoreError {
    CoreError::StrategyCandidateConflict(StrategyCandidateConflictError {
        strategy_id: strategy_id.to_string(),
        base_revision,
        current_revision,
        conflicting_path: "$".to_string(),
        recovery_hint: "Reload the current candidate and explicitly reapply the compose operation."
            .to_string(),
    })
}

async fn load_stored(pool: &SqlitePool, strategy_id: &str) -> CoreResult<Option<(String, i64)>> {
    sqlx::query_as("SELECT document_json, revision FROM strategy_candidates WHERE strategy_id = ?")
        .bind(strategy_id)
        .fetch_optional(pool)
        .await
        .map_err(storage_error)
}

pub async fn current_revision(pool: &SqlitePool, strategy_id: &str) -> CoreResult<u64> {
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

/// The CAS write itself: one conditional statement keyed by `(strategy_id, revision)`, inserting
/// at base 0 and updating otherwise, with the document's embedded revision already rewritten by
/// the caller. A zero-row result is the typed conflict, never a repair or a merge.
///
/// This is the ONE write both [`CoreService::commit_strategy_candidate`] and an import reach, so
/// the two can never disagree about what a stored candidate row means.
async fn write_candidate(
    pool: &SqlitePool,
    strategy_id: &str,
    stored: bool,
    base_revision: u64,
    document: WorkflowDocument,
) -> CoreResult<StrategyCandidate> {
    let document_json = serde_json::to_string(&document)
        .map_err(|error| candidate_internal(format!("document does not encode: {error}")))?;
    let hash = content_hash(&document)
        .map_err(|error| candidate_internal(format!("canonical hash: {error}")))?;
    let revision = i64::try_from(document.revision)
        .map_err(|_| candidate_internal("revision does not fit the row column"))?;
    let base = i64::try_from(base_revision)
        .map_err(|_| candidate_internal("base revision does not fit the row column"))?;
    let now = chrono::Utc::now().to_rfc3339();

    let written = if stored {
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
        .execute(pool)
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
        .execute(pool)
        .await
        .map_err(storage_error)?
        .rows_affected()
    };
    if written == 0 {
        let current = current_revision(pool, strategy_id).await?;
        return Err(conflict(strategy_id, base_revision, current));
    }

    Ok(StrategyCandidate {
        revision: document.revision,
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
            Some((document_json, revision)) => decode_stored(strategy_id, &document_json, revision),
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
            Some((document_json, revision)) => {
                decode_stored(strategy_id, document_json, *revision)?
            }
            None => self.imported_candidate(strategy_id)?,
        };
        if current.revision != base_revision {
            return Err(conflict(strategy_id, base_revision, current.revision));
        }

        let mut document = apply(&current.document)?;
        let new_revision = base_revision
            .checked_add(1)
            .ok_or_else(|| CoreError::InvalidInput {
                field: "base_revision".into(),
                reason: "revision is exhausted".into(),
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
                reason: format!(
                    "a candidate must stay in state `candidate`, got `{}`",
                    document.state
                ),
            });
        }
        document.revision = new_revision;
        write_candidate(
            &self.inner.pool,
            strategy_id,
            stored.is_some(),
            base_revision,
            document,
        )
        .await
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

/// Why a bundle import was refused. Nothing was written when one is returned.
#[cfg(feature = "strategy-authoring")]
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PresetImportRefusal {
    /// The imported document does not pass the shared structural verdict Approve applies, so the
    /// candidate would be one no approval could freeze.
    InvalidDocument {
        /// The shared validator's violations, plus any pin or slot finding that refused it.
        violations: Vec<String>,
    },
    /// The document cannot be walked, so no trajectory exists to hand Approve with the candidate.
    SimulationUnavailable {
        /// The static walk's own refusal.
        reason: String,
    },
    /// The bundle declares an id this host already serves: an embedded preset, or a `_system.` id.
    /// A stored candidate row is read before the preset resolution (`read_strategy_candidate`), so
    /// a row under one of those ids would shadow a preset no partner owns.
    ReservedId {
        /// The id the bundle declared.
        strategy_id: String,
    },
}

#[cfg(feature = "strategy-authoring")]
impl PresetImportRefusal {
    /// The stable lowercase reason a client branches on.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::InvalidDocument { .. } => "invalid_document",
            Self::SimulationUnavailable { .. } => "simulation_unavailable",
            Self::ReservedId { .. } => "reserved_strategy_id",
        }
    }

    /// The refusal envelope's `details`, carrying what refused the bundle.
    #[must_use]
    pub fn details(&self) -> serde_json::Value {
        let mut details = serde_json::Map::new();
        details.insert(
            "code".into(),
            serde_json::Value::from("preset_import_refused"),
        );
        details.insert("reason".into(), serde_json::Value::from(self.reason()));
        details.insert("message".into(), serde_json::Value::from(self.to_string()));
        match self {
            Self::InvalidDocument { violations } => {
                details.insert(
                    "violations".into(),
                    serde_json::Value::from(violations.clone()),
                );
            }
            Self::SimulationUnavailable { reason } => {
                details.insert(
                    "simulation_reason".into(),
                    serde_json::Value::from(reason.clone()),
                );
            }
            Self::ReservedId { strategy_id } => {
                details.insert(
                    "strategy_id".into(),
                    serde_json::Value::from(strategy_id.clone()),
                );
            }
        }
        serde_json::Value::Object(details)
    }

    /// The core error a refused import returns; the adapter renders `details` verbatim.
    #[must_use]
    pub fn into_error(self) -> CoreError {
        CoreError::InputValidation {
            details: self.details(),
        }
    }
}

#[cfg(feature = "strategy-authoring")]
impl std::fmt::Display for PresetImportRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidDocument { violations } => write!(
                f,
                "the imported bundle does not pass shared validation: {}",
                violations.join("; ")
            ),
            Self::SimulationUnavailable { reason } => {
                write!(f, "the imported bundle cannot be simulated: {reason}")
            }
            Self::ReservedId { strategy_id } => write!(
                f,
                "the bundle declares `{strategy_id}`, which the host serves as an embedded or \
                 `_system.` preset; import a user bundle under its own id"
            ),
        }
    }
}

/// A committed import: the revision-1 candidate and the simulation evidence Approve needs.
///
/// The trajectory is the deterministic walk of the SAME bytes that were stored, so a caller can
/// pass it to [`CoreService::approve_strategy`] unchanged; recomputing it elsewhere is how a
/// stored candidate and its approval evidence drift apart.
#[cfg(feature = "strategy-authoring")]
#[derive(Debug, Clone, serde::Serialize)]
pub struct StrategyCandidateImport {
    /// The committed candidate (row revision 1).
    pub candidate: StrategyCandidate,
    /// The deterministic simulation of `candidate.document`.
    pub trajectory: SimulationTrajectory,
}

#[cfg(feature = "strategy-authoring")]
impl CoreService {
    /// Import the external bundle at `bundle_root` as this strategy's revision-1 candidate.
    ///
    /// The bundle is loaded and converted by the ONE adapters that already own those steps
    /// (`load_preset`, `import_preset_bundle`) — no second YAML reader and no copied directory.
    /// The prospective row's bytes are validated and simulated BEFORE the CAS write, so a refusal
    /// leaves the candidate table, the source bundle and the strategy's other rows untouched. The
    /// write is [`write_candidate`] — the same conditional statement compose commits through, at
    /// base revision 0 — so an existing candidate is the typed conflict compose answers with.
    ///
    /// The bundle's `preset.id` names the strategy. An id the host ships as an embedded preset, or
    /// a `_system.` id, is a [`PresetImportRefusal::ReservedId`]: the candidate read prefers a
    /// stored row, so the import would shadow a preset no partner owns. Every other id defines a
    /// new strategy — the candidate is stored and approvable, and the schedule intake runs it once
    /// the bundle is installed as a user preset under `<nexus_home>/presets/<id>/` and an apply
    /// exists (the apply gate reads only user presets).
    ///
    /// # Errors
    ///
    /// [`CoreError::InvalidInput`] with field `bundle` for a bundle that does not load or convert,
    /// [`CoreError::InputValidation`] carrying a [`PresetImportRefusal::details`] envelope for a
    /// document the shared validator refuses, the walk cannot simulate, or an id the host reserves,
    /// [`CoreError::StrategyCandidateConflict`] when the strategy already has a candidate, and the
    /// principal/store errors otherwise.
    pub async fn import_strategy_candidate(
        &self,
        principal: &Principal,
        bundle_root: &Path,
    ) -> CoreResult<StrategyCandidateImport> {
        self.verify_principal(principal)?;
        let loaded = nexus_preset::load_preset(bundle_root, &BuiltinCapabilityCatalog)
            .map_err(|error| bundle_load_error(&error))?;
        let source = FsAssetSource::new(bundle_root).map_err(|source| CoreError::InvalidInput {
            field: "bundle".into(),
            reason: source.to_string(),
        })?;
        let imported = import_preset_bundle(&loaded.manifest, &source).map_err(|error| {
            CoreError::InvalidInput {
                field: "bundle".into(),
                reason: error.to_string(),
            }
        })?;
        let strategy_id = imported.document.id.as_str().to_owned();

        if strategy_id.starts_with("_system.")
            || nexus_preset::list_embedded_presets().contains(&strategy_id)
        {
            return Err(PresetImportRefusal::ReservedId { strategy_id }.into_error());
        }

        let mut document = imported.document;
        document.revision = 1;
        let packages = self.compose_packages()?;
        if let Some(refusal) = crate::approve::structural_refusal(&document, &packages) {
            return Err(import_invalid(refusal).into_error());
        }
        let trajectory = simulate_document(&document).map_err(|error| {
            PresetImportRefusal::SimulationUnavailable {
                reason: error.to_string(),
            }
            .into_error()
        })?;

        let candidate = write_candidate(&self.inner.pool, &strategy_id, false, 0, document).await?;
        Ok(StrategyCandidateImport {
            candidate,
            trajectory,
        })
    }
}

/// The import refusal a shared structural verdict becomes: an explicit invalid-document refusal
/// keeps its violations, and a pin or slot finding (which no YAML bundle can carry today) is
/// reported in the same list rather than dropped.
#[cfg(feature = "strategy-authoring")]
fn import_invalid(refusal: crate::approve::ApproveRefusal) -> PresetImportRefusal {
    match refusal {
        crate::approve::ApproveRefusal::InvalidDocument { violations } => {
            PresetImportRefusal::InvalidDocument { violations }
        }
        other => PresetImportRefusal::InvalidDocument {
            violations: vec![other.to_string()],
        },
    }
}

/// A bundle that does not load is a `bundle` input refusal; the loader's own problems are named
/// instead of only their count, so the caller knows which line of `preset.yaml` to fix.
#[cfg(feature = "strategy-authoring")]
fn bundle_load_error(error: &nexus_preset::PresetLoadError) -> CoreError {
    let reason = match error {
        nexus_preset::PresetLoadError::Validation { problems, .. } => problems
            .iter()
            .map(|problem| format!("{}: {}", problem.path, problem.error))
            .collect::<Vec<_>>()
            .join("; "),
        _ => error.to_string(),
    };
    CoreError::InvalidInput {
        field: "bundle".into(),
        reason,
    }
}
