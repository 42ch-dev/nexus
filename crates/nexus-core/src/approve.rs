//! Approve: freeze one validated, simulated candidate content into an immutable snapshot.
//!
//! Plan: `.mstar/plans/v239-p3-approve-route.md` §Candidate and simulation binding (Q4).
//!
//! Approve is the only place a candidate becomes authoritative, so each refusal here is a safety
//! property. The route reads the canonical candidate (the stored row, or the revision-0 import
//! projection), CAS-checks the revision the client simulated, re-runs the shared structural and
//! nest-policy validation, verifies every recorded pattern pin against the registry bytes, and
//! recomputes the deterministic simulation over the same document. The supplied trajectory must
//! equal that recomputation: its `ir_content_hash` is the approval identity, so a different hash is
//! stale evidence and nothing is written.
//!
//! The snapshot is keyed by `(strategy_id, content_hash)`: one content approves once. A repeat
//! approve is the typed 409 refusal, never a second row and never a mutated candidate. The insert
//! itself is the compare-and-swap — it lands only while the candidate still sits at the revision
//! this approval verified — so a compose that commits in the window wins and approve is the typed
//! conflict. The snapshot freezes bytes; it does not apply, bind a schedule or execute.

use nexus_contracts::{ApprovedStrategySnapshot, SimulationTrajectory};
use nexus_orchestration::{simulate_document, CapabilityRegistryHolder, SimulationOptions};
use nexus_workflow_ir::{validate, Violation, WorkflowDocument};
use serde_json::Value;

use crate::compose::{
    document_bodies, document_unbound_slots, resolved_pins, ComposeDiagnostic, ResolvedPatternPin,
};
use crate::strategy_candidates::{conflict, current_revision};
use crate::{CoreError, CoreResult, CoreService, Principal, StrategyAlreadyApprovedError};

/// Why an approval was refused. Nothing was written when one is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApproveRefusal {
    /// The request carried no trajectory, so no simulation evidence exists for the content.
    SimulationMissing {
        /// The candidate's current canonical content hash.
        current_hash: String,
    },
    /// The supplied trajectory was produced for different content: its recorded hash is not the
    /// current document hash.
    SimulationStale {
        /// The trajectory's `ir_content_hash`.
        recorded_hash: String,
        /// The current document's canonical content hash.
        current_hash: String,
    },
    /// The supplied trajectory carries the current hash but is not byte-for-byte what the walk
    /// produces for these bytes.
    SimulationMismatch {
        /// The candidate's canonical content hash.
        content_hash: String,
    },
    /// The static walk cannot produce a trajectory for this document.
    SimulationUnavailable {
        /// The walk's refusal.
        reason: String,
    },
    /// The shared structural or nest-policy validation rejected the document.
    InvalidDocument {
        /// The shared walk's violations, in walk order.
        violations: Vec<String>,
    },
    /// A required slot of a pinned package carries no binding.
    SlotUnbound {
        /// The nest that pins the package.
        nest_id: String,
        /// The pinned package, as `id@version`.
        package: String,
        /// The unbound required slot.
        slot: String,
    },
    /// The registry now serves bytes that do not hash to the pin the document recorded.
    PinChanged {
        /// The nest that pins the package.
        nest_id: String,
        /// The pinned package id.
        package_id: String,
        /// The pinned package version.
        package_version: String,
        /// The hash the document recorded.
        recorded_hash: String,
        /// The hash the registry now serves.
        scanned_hash: String,
    },
}

impl ApproveRefusal {
    /// The stable lowercase reason a client branches on.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::SimulationMissing { .. } => "simulation_missing",
            Self::SimulationStale { .. } => "simulation_stale",
            Self::SimulationMismatch { .. } => "simulation_mismatch",
            Self::SimulationUnavailable { .. } => "simulation_unavailable",
            Self::InvalidDocument { .. } => "invalid_document",
            Self::SlotUnbound { .. } => "slot_unbound",
            Self::PinChanged { .. } => "pin_changed",
        }
    }

    /// The non-mutating validation envelope's `details`, carrying the hashes or package the
    /// refusal names.
    #[must_use]
    pub fn details(&self) -> Value {
        let mut details = serde_json::Map::new();
        details.insert("code".to_string(), Value::from("approval_refused"));
        details.insert("reason".to_string(), Value::from(self.reason()));
        details.insert("message".to_string(), Value::from(self.to_string()));
        match self {
            Self::SimulationMissing { current_hash } => {
                details.insert(
                    "current_content_hash".into(),
                    Value::from(current_hash.clone()),
                );
            }
            Self::SimulationStale {
                recorded_hash,
                current_hash,
            } => {
                details.insert(
                    "recorded_content_hash".into(),
                    Value::from(recorded_hash.clone()),
                );
                details.insert(
                    "current_content_hash".into(),
                    Value::from(current_hash.clone()),
                );
            }
            Self::SimulationMismatch { content_hash } => {
                details.insert(
                    "current_content_hash".into(),
                    Value::from(content_hash.clone()),
                );
            }
            Self::SimulationUnavailable { reason } => {
                details.insert("walk_reason".into(), Value::from(reason.clone()));
            }
            Self::InvalidDocument { violations } => {
                details.insert("violations".into(), Value::from(violations.clone()));
            }
            Self::SlotUnbound {
                nest_id,
                package,
                slot,
            } => {
                details.insert("nest_id".into(), Value::from(nest_id.clone()));
                details.insert("package".into(), Value::from(package.clone()));
                details.insert("slot".into(), Value::from(slot.clone()));
            }
            Self::PinChanged {
                nest_id,
                package_id,
                package_version,
                recorded_hash,
                scanned_hash,
            } => {
                details.insert("nest_id".into(), Value::from(nest_id.clone()));
                details.insert("package_id".into(), Value::from(package_id.clone()));
                details.insert(
                    "package_version".into(),
                    Value::from(package_version.clone()),
                );
                details.insert(
                    "recorded_content_hash".into(),
                    Value::from(recorded_hash.clone()),
                );
                details.insert(
                    "scanned_content_hash".into(),
                    Value::from(scanned_hash.clone()),
                );
            }
        }
        Value::Object(details)
    }

    /// The core error a refused approval returns; the adapter renders `details` verbatim.
    #[must_use]
    pub fn into_error(self) -> CoreError {
        CoreError::InputValidation {
            details: self.details(),
        }
    }

    /// Reuse compose's own findings as approval refusals: the two the walk reports as refusals to
    /// an approval are a slot a pinned pattern requires and a pin whose bytes moved.
    fn from_diagnostic(diagnostic: ComposeDiagnostic) -> Self {
        match diagnostic {
            ComposeDiagnostic::UnboundRequiredSlot {
                nest_id,
                package,
                slot,
            } => Self::SlotUnbound {
                nest_id,
                package: package.to_string(),
                slot,
            },
            ComposeDiagnostic::StalePin {
                nest_id,
                package,
                recorded_hash,
                scanned_hash,
            } => Self::PinChanged {
                nest_id,
                package_id: package.id,
                package_version: package.version,
                recorded_hash,
                scanned_hash,
            },
            other @ ComposeDiagnostic::DroppedSlotBinding { .. } => Self::InvalidDocument {
                violations: vec![other.to_string()],
            },
        }
    }
}

impl std::fmt::Display for ApproveRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SimulationMissing { current_hash } => write!(
                f,
                "no simulation exists for content hash `{current_hash}`: simulate the candidate \
                 before approving it"
            ),
            Self::SimulationStale {
                recorded_hash,
                current_hash,
            } => write!(
                f,
                "the recorded simulation belongs to content hash `{recorded_hash}`, but the \
                 candidate now hashes to `{current_hash}`: simulate the current content again"
            ),
            Self::SimulationMismatch { content_hash } => write!(
                f,
                "the recorded simulation is not the deterministic trajectory of content hash \
                 `{content_hash}`: simulate the current content again"
            ),
            Self::SimulationUnavailable { reason } => {
                write!(f, "the candidate cannot be simulated: {reason}")
            }
            Self::InvalidDocument { violations } => {
                write!(
                    f,
                    "the candidate does not pass shared validation: {}",
                    violations.join("; ")
                )
            }
            Self::SlotUnbound {
                nest_id,
                package,
                slot,
            } => write!(
                f,
                "nest `{nest_id}`: pattern `{package}` requires slot `{slot}`, which is unbound"
            ),
            Self::PinChanged {
                nest_id,
                package_id,
                package_version,
                recorded_hash,
                scanned_hash,
            } => write!(
                f,
                "nest `{nest_id}` pinned `{package_id}@{package_version}` at content hash \
                 `{recorded_hash}`, but the registry now serves `{scanned_hash}`: restore the \
                 package or replace this region before approving"
            ),
        }
    }
}

fn approval_internal(detail: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("strategy approval: {detail}"),
    }
}

/// The snapshot row as it is frozen. Serialized to the generated wire type; every member is the
/// value the approval proved, never a recomputation at read time.
#[derive(serde::Serialize)]
struct FrozenSnapshot<'a> {
    approval_id: &'a str,
    strategy_id: &'a str,
    candidate_revision: u64,
    document: &'a WorkflowDocument,
    content_hash: &'a str,
    trajectory: &'a SimulationTrajectory,
    pins: &'a [ResolvedPatternPin],
    approver: &'a str,
    approved_at: chrono::DateTime<chrono::Utc>,
}

impl CoreService {
    /// Approve the candidate at `base_revision` against `trajectory`, freezing one immutable
    /// snapshot.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::StrategyCandidateConflict`] when the stored revision is not
    /// `base_revision`, [`CoreError::InputValidation`] carrying an [`ApproveRefusal::details`]
    /// envelope for missing/stale simulation evidence, invalid content or a changed pin,
    /// [`CoreError::StrategyAlreadyApproved`] when this content already has a snapshot, and the
    /// principal/store errors otherwise.
    pub async fn approve_strategy(
        &self,
        principal: &Principal,
        strategy_id: &str,
        base_revision: u64,
        trajectory: Option<SimulationTrajectory>,
    ) -> CoreResult<ApprovedStrategySnapshot> {
        self.verify_principal(principal)?;
        let current = self.read_strategy_candidate(principal, strategy_id).await?;
        if current.revision != base_revision {
            return Err(conflict(strategy_id, base_revision, current.revision));
        }
        let trajectory = trajectory.ok_or_else(|| {
            ApproveRefusal::SimulationMissing {
                current_hash: current.content_hash.clone(),
            }
            .into_error()
        })?;

        let packages = self.compose_packages()?;
        let (bodies, diagnostics) = document_bodies(&current.document, &packages);
        let violations: Vec<Violation> = validate(&[&current.document], &bodies)
            .err()
            .unwrap_or_default()
            .into_iter()
            // Outer-state loops are legal for imported presets (the same exception compose makes).
            .filter(|violation| !matches!(violation, Violation::GraphCycle { .. }))
            .collect();
        if !violations.is_empty() {
            return Err(ApproveRefusal::InvalidDocument {
                violations: violations.iter().map(ToString::to_string).collect(),
            }
            .into_error());
        }
        if let Some(unbound) = document_unbound_slots(&current.document, &packages)
            .into_iter()
            .next()
        {
            return Err(ApproveRefusal::from_diagnostic(unbound).into_error());
        }
        // The walk reports every pin whose bytes moved, through the one `verify_pin` comparison.
        // An approval freezes the whole document, so the first such finding refuses it.
        if let Some(stale) = diagnostics
            .into_iter()
            .find(|finding| matches!(finding, ComposeDiagnostic::StalePin { .. }))
        {
            return Err(ApproveRefusal::from_diagnostic(stale).into_error());
        }

        let recomputed = simulate_document(
            &current.document,
            &SimulationOptions {
                capabilities: CapabilityRegistryHolder::new(),
            },
        )
        .map_err(|error| {
            ApproveRefusal::SimulationUnavailable {
                reason: error.to_string(),
            }
            .into_error()
        })?;
        if trajectory.ir_content_hash != current.content_hash {
            return Err(ApproveRefusal::SimulationStale {
                recorded_hash: trajectory.ir_content_hash,
                current_hash: current.content_hash.clone(),
            }
            .into_error());
        }
        let recorded = serde_json::to_value(&trajectory)
            .map_err(|error| approval_internal(format!("recorded trajectory: {error}")))?;
        let expected = serde_json::to_value(&recomputed)
            .map_err(|error| approval_internal(format!("recomputed trajectory: {error}")))?;
        if recorded != expected {
            return Err(ApproveRefusal::SimulationMismatch {
                content_hash: current.content_hash.clone(),
            }
            .into_error());
        }

        let pins = resolved_pins(&current.document, &packages);
        self.write_snapshot(principal, strategy_id, &current, &recomputed, &pins)
            .await
    }

    async fn write_snapshot(
        &self,
        principal: &Principal,
        strategy_id: &str,
        candidate: &crate::StrategyCandidate,
        trajectory: &SimulationTrajectory,
        pins: &[ResolvedPatternPin],
    ) -> CoreResult<ApprovedStrategySnapshot> {
        let approval_id = format!("apr_{}", uuid::Uuid::new_v4());
        let approved_at = chrono::Utc::now();
        let document_json = serde_json::to_string(&candidate.document)
            .map_err(|error| approval_internal(format!("document does not encode: {error}")))?;
        let trajectory_json = serde_json::to_string(trajectory)
            .map_err(|error| approval_internal(format!("trajectory does not encode: {error}")))?;
        let pins_json = serde_json::to_string(pins)
            .map_err(|error| approval_internal(format!("pins do not encode: {error}")))?;
        let revision = i64::try_from(candidate.revision)
            .map_err(|_| approval_internal("revision does not fit the row column"))?;

        // The insert is the CAS: the row lands only while the stored candidate still sits at the
        // revision this approval verified (an absent row is revision 0). A compose that commits in
        // between makes the predicate fail, so it wins and this writer gets the typed conflict.
        let written = sqlx::query(
            "INSERT INTO strategy_approvals \
             (approval_id, strategy_id, content_hash, candidate_revision, document_json, \
              trajectory_json, pins_json, approver, approved_at) \
             SELECT ?, ?, ?, ?, ?, ?, ?, ?, ? \
             WHERE COALESCE((SELECT revision FROM strategy_candidates WHERE strategy_id = ?), 0) = ? \
             ON CONFLICT(strategy_id, content_hash) DO NOTHING",
        )
        .bind(&approval_id)
        .bind(strategy_id)
        .bind(&candidate.content_hash)
        .bind(revision)
        .bind(&document_json)
        .bind(&trajectory_json)
        .bind(&pins_json)
        .bind(principal.creator_id())
        .bind(approved_at.to_rfc3339())
        .bind(strategy_id)
        .bind(revision)
        .execute(&self.inner.pool)
        .await
        .map_err(|error| crate::error::local_db_err(error.into()))?
        .rows_affected();

        if written == 0 {
            let stored = current_revision(&self.inner.pool, strategy_id).await?;
            if stored != candidate.revision {
                return Err(conflict(strategy_id, candidate.revision, stored));
            }
            let existing: Option<(String, i64)> = sqlx::query_as(
                "SELECT approval_id, candidate_revision FROM strategy_approvals \
                 WHERE strategy_id = ? AND content_hash = ?",
            )
            .bind(strategy_id)
            .bind(&candidate.content_hash)
            .fetch_optional(&self.inner.pool)
            .await
            .map_err(|error| crate::error::local_db_err(error.into()))?;
            let Some((approval_id, approved_revision)) = existing else {
                return Err(approval_internal(
                    "the snapshot insert wrote no row and no approval holds this content",
                ));
            };
            let approved_revision = u64::try_from(approved_revision)
                .map_err(|_| approval_internal("the stored approval revision is negative"))?;
            return Err(CoreError::StrategyAlreadyApproved(
                StrategyAlreadyApprovedError {
                    strategy_id: strategy_id.to_string(),
                    content_hash: candidate.content_hash.clone(),
                    approval_id,
                    candidate_revision: approved_revision,
                },
            ));
        }

        let frozen = FrozenSnapshot {
            approval_id: &approval_id,
            strategy_id,
            candidate_revision: candidate.revision,
            document: &candidate.document,
            content_hash: &candidate.content_hash,
            trajectory,
            pins,
            approver: principal.creator_id(),
            approved_at,
        };
        let value = serde_json::to_value(&frozen)
            .map_err(|error| approval_internal(format!("snapshot does not encode: {error}")))?;
        serde_json::from_value(value).map_err(|error| {
            approval_internal(format!("snapshot does not match the wire type: {error}"))
        })
    }
}
