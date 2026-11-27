//! Apply: install an approved snapshot as the strategy's applied identity.
//!
//! Plan: `.mstar/plans/v241-p35-applied-identity.md` §Architecture (Task 2).
//!
//! Apply is the only place an approval becomes the running identity, so each refusal here is a
//! safety property and every one of them happens before a row exists: the requested content hash
//! must name an approval of this strategy, the approval's frozen `document_json` must still hash
//! to the `content_hash` the approval recorded, the pattern bodies that document pins must still
//! hash to the frozen `pins_json` snapshot (the document hash does not cover them), the document
//! must build into an outer graph (the same predicate the schedule intake uses, so an unbuildable
//! identity is refused here instead of at its first run), the strategy must resolve to a user
//! bundle (the only source the intake ever reads an apply from), the strategy must own a schedule
//! (nothing would run otherwise), and the approval must not already be the current apply.
//!
//! `strategy_applies` is append-only and carries no current pointer: the current apply of a
//! strategy is the row with no successor, and a superseded approval is a legal rollback (a new
//! apply naming the previous row). The already-current guard, the supersede link and the insert
//! are therefore ONE statement: it reads the current row through its own subqueries in the same
//! statement that writes, so two concurrent applies cannot both believe they are first and no
//! writer can supersede a row it did not see. Nothing here updates or deletes a row, and
//! `strategy_approvals` is only ever read.
//!
//! [`CoreService::strategy_applied`] is the read side of the same shape: it answers the current
//! apply (that same no-successor row) together with the strategy's newest approval, so the Canvas
//! and the intake ask one question and both get the same answer.
//!
//! [`current_apply`] is that one answer shared as a function, and [`verified_apply_document`] is
//! its load-side counterpart: the schedule intake re-runs the same recomputation before it builds
//! a graph, so bytes edited after apply are a typed refusal instead of a reinterpreted run
//! (v1.241 P3.5, `.mstar/plans/v241-p35-intake-branch.md`). [`is_user_preset`] is that pairing on
//! the other side: the intake consults [`current_apply`] for a user bundle only, so apply refuses
//! any other source rather than record a row nothing reads.

use nexus_orchestration::preset_runtime::{validate_ir_buildability, IrTemplateSource};
use nexus_workflow_ir::{content_hash, WorkflowDocument};
use serde_json::Value;

use crate::compose::{verify_recorded_pins, PinVerificationFailure, ResolvedPatternPin};
use crate::{CoreError, CoreResult, CoreService, Principal};

/// Why an apply was refused. Nothing was written when one is returned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyRefusal {
    /// No approval carries the requested content hash.
    ApprovalNotFound {
        /// The content hash the request named.
        content_hash: String,
    },
    /// The requested content hash belongs to another strategy's approval, so no apply row could
    /// pair it with this strategy.
    ApprovalStrategyMismatch {
        /// The content hash the request named.
        content_hash: String,
        /// The strategy the approval belongs to.
        approval_strategy_id: String,
        /// The strategy the request named.
        strategy_id: String,
    },
    /// The approval's frozen document no longer hashes to the identity the approval recorded, so
    /// installing it would install a changed hash.
    ApprovalHashChanged {
        /// The approval whose bytes moved.
        approval_id: String,
        /// The `content_hash` stored on the approval row.
        stored_hash: String,
        /// The canonical hash of the stored `document_json`.
        recomputed_hash: String,
    },
    /// The strategy does not resolve to a user bundle, so the schedule intake would never read
    /// the apply this request would record.
    StrategyNotUserPreset {
        /// The strategy the request named.
        strategy_id: String,
    },
    /// The strategy owns no schedule, so an applied identity would record nothing that runs.
    StrategyHasNoSchedule {
        /// The strategy the request named.
        strategy_id: String,
    },
    /// The approval is already the current apply; a repeat is the typed conflict. A *superseded*
    /// approval is not covered: re-applying it is a legal rollback.
    AlreadyApplied {
        /// The apply row that already carries this approval.
        apply_id: String,
        /// The approval that is already current.
        approval_id: String,
    },
    /// The approval's document cannot become an outer graph, so applying it would install an
    /// identity the schedule intake refuses to build: the run would fail at its first build.
    DocumentUnbuildable {
        /// The builder's own refusal, verbatim: `IrOuterGraphError::to_string()`.
        builder_reason: String,
    },
    /// A pattern body the approved document pins no longer hashes to the pin the approval froze,
    /// so applying it would run bytes nobody approved.
    PinChanged {
        /// The nest that pins the package.
        nest_id: String,
        /// The pinned package id.
        package_id: String,
        /// The pinned package version.
        package_version: String,
        /// The hash the approval recorded.
        recorded_hash: String,
        /// The hash the registry now serves.
        scanned_hash: String,
    },
    /// The registry does not resolve a pin the approval froze, so there are no bytes to compare.
    PinUnavailable {
        /// The nest that pins the package.
        nest_id: String,
        /// The pinned package id.
        package_id: String,
        /// The pinned package version.
        package_version: String,
        /// The hash the approval recorded.
        recorded_hash: String,
    },
    /// The approval's recorded pin snapshot cannot be relied on: it does not decode, it disagrees
    /// with the document it was frozen beside, or the pattern registry cannot be read.
    PinsUnverifiable {
        /// The approval whose pins could not be verified.
        approval_id: String,
        /// What could not be compared, and why.
        detail: String,
    },
}

/// How the transport renders an [`ApplyRefusal`].
///
/// This is the one statement of the classification: the service's table
/// (`apps/nexus-service/src/apply-refusal-classes.json`) is a projection of it, and
/// `the_service_classification_table_covers_every_variant` asserts the two agree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyRefusalClass {
    /// The state the request named has moved, so retrying this exact request cannot succeed:
    /// the transport answers 409 under the reason's own code.
    Conflict,
    /// The request needs a different approval, preset, schedule or document: the core's
    /// client-error status (422 `invalid_input`).
    ClientError,
}

impl ApplyRefusalClass {
    /// The stable lowercase name the service classification table uses.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conflict => "conflict",
            Self::ClientError => "client_error",
        }
    }
}

impl ApplyRefusal {
    /// The stable lowercase reason a client branches on.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::ApprovalNotFound { .. } => "approval_not_found",
            Self::ApprovalStrategyMismatch { .. } => "approval_strategy_mismatch",
            Self::ApprovalHashChanged { .. } => "approval_hash_changed",
            Self::StrategyNotUserPreset { .. } => "strategy_not_user_preset",
            Self::StrategyHasNoSchedule { .. } => "strategy_has_no_schedule",
            Self::AlreadyApplied { .. } => "already_applied",
            Self::DocumentUnbuildable { .. } => "document_unbuildable",
            Self::PinChanged { .. } => "pin_changed",
            Self::PinUnavailable { .. } => "pin_unavailable",
            Self::PinsUnverifiable { .. } => "approval_pins_unverifiable",
        }
    }

    /// The classification the service transport reads.
    ///
    /// Exhaustive on purpose: a new variant cannot be added without stating its class here, so it
    /// cannot silently inherit the client-error side.
    #[must_use]
    pub const fn class(&self) -> ApplyRefusalClass {
        match self {
            Self::AlreadyApplied { .. } | Self::ApprovalHashChanged { .. } => {
                ApplyRefusalClass::Conflict
            }
            Self::ApprovalNotFound { .. }
            | Self::ApprovalStrategyMismatch { .. }
            | Self::StrategyNotUserPreset { .. }
            | Self::StrategyHasNoSchedule { .. }
            | Self::DocumentUnbuildable { .. }
            | Self::PinChanged { .. }
            | Self::PinUnavailable { .. }
            | Self::PinsUnverifiable { .. } => ApplyRefusalClass::ClientError,
        }
    }

    /// The refusal envelope's `details`, carrying the identity the refusal names.
    #[must_use]
    pub fn details(&self) -> Value {
        let mut details = serde_json::Map::new();
        details.insert("code".to_string(), Value::from("apply_refused"));
        details.insert("reason".to_string(), Value::from(self.reason()));
        details.insert("message".to_string(), Value::from(self.to_string()));
        match self {
            Self::ApprovalNotFound { content_hash } => {
                details.insert("content_hash".into(), Value::from(content_hash.clone()));
            }
            Self::ApprovalStrategyMismatch {
                content_hash,
                approval_strategy_id,
                strategy_id,
            } => {
                details.insert("content_hash".into(), Value::from(content_hash.clone()));
                details.insert("strategy_id".into(), Value::from(strategy_id.clone()));
                details.insert(
                    "approval_strategy_id".into(),
                    Value::from(approval_strategy_id.clone()),
                );
            }
            Self::ApprovalHashChanged {
                approval_id,
                stored_hash,
                recomputed_hash,
            } => {
                details.insert("approval_id".into(), Value::from(approval_id.clone()));
                details.insert(
                    "stored_content_hash".into(),
                    Value::from(stored_hash.clone()),
                );
                details.insert(
                    "recomputed_content_hash".into(),
                    Value::from(recomputed_hash.clone()),
                );
            }
            Self::StrategyNotUserPreset { strategy_id }
            | Self::StrategyHasNoSchedule { strategy_id } => {
                details.insert("strategy_id".into(), Value::from(strategy_id.clone()));
            }
            Self::AlreadyApplied {
                apply_id,
                approval_id,
            } => {
                details.insert("apply_id".into(), Value::from(apply_id.clone()));
                details.insert("approval_id".into(), Value::from(approval_id.clone()));
            }
            Self::DocumentUnbuildable { builder_reason } => {
                details.insert("builder_reason".into(), Value::from(builder_reason.clone()));
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
            Self::PinUnavailable {
                nest_id,
                package_id,
                package_version,
                recorded_hash,
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
            }
            Self::PinsUnverifiable {
                approval_id,
                detail,
            } => {
                details.insert("approval_id".into(), Value::from(approval_id.clone()));
                details.insert("pin_detail".into(), Value::from(detail.clone()));
            }
        }
        Value::Object(details)
    }

    /// The core error a refused apply returns; the adapter renders `details` verbatim.
    #[must_use]
    pub fn into_error(self) -> CoreError {
        CoreError::InputValidation {
            details: self.details(),
        }
    }
}

impl std::fmt::Display for ApplyRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::ApprovalNotFound { content_hash } => write!(
                f,
                "no approval carries content hash `{content_hash}`: approve the candidate before \
                 applying it"
            ),
            Self::ApprovalStrategyMismatch {
                content_hash,
                approval_strategy_id,
                strategy_id,
            } => write!(
                f,
                "content hash `{content_hash}` is approved for strategy `{approval_strategy_id}`, \
                 not `{strategy_id}`: approve it against this strategy before applying it"
            ),
            Self::ApprovalHashChanged {
                approval_id,
                stored_hash,
                recomputed_hash,
            } => write!(
                f,
                "approval `{approval_id}` records content hash `{stored_hash}`, but its frozen \
                 document now hashes to `{recomputed_hash}`: the approval bytes changed after \
                 approval"
            ),
            Self::StrategyNotUserPreset { strategy_id } => write!(
                f,
                "strategy `{strategy_id}` is not a user preset: the schedule intake reads an \
                 applied identity only for a user bundle, so this apply would record nothing that \
                 runs"
            ),
            Self::StrategyHasNoSchedule { strategy_id } => write!(
                f,
                "strategy `{strategy_id}` has no schedule, so applying it would run nothing"
            ),
            Self::AlreadyApplied {
                apply_id,
                approval_id,
            } => write!(
                f,
                "approval `{approval_id}` is already the current apply (`{apply_id}`)"
            ),
            Self::DocumentUnbuildable { builder_reason } => write!(
                f,
                "the approved document cannot be built into a run, so this apply would install \
                 an identity the schedule intake refuses: {builder_reason}"
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
                 `{recorded_hash}`, but the registry now serves `{scanned_hash}`: the approved \
                 bytes moved, so restore the package or approve the region again"
            ),
            Self::PinUnavailable {
                nest_id,
                package_id,
                package_version,
                recorded_hash,
            } => write!(
                f,
                "nest `{nest_id}` pinned `{package_id}@{package_version}` at content hash \
                 `{recorded_hash}`, but the registry has no available package for that pin: \
                 restore the package before applying"
            ),
            Self::PinsUnverifiable {
                approval_id,
                detail,
            } => write!(
                f,
                "the pins approval `{approval_id}` froze cannot be verified: {detail}"
            ),
        }
    }
}

fn apply_internal(detail: impl std::fmt::Display) -> CoreError {
    CoreError::Internal {
        category: format!("strategy apply: {detail}"),
    }
}

/// The identity an apply recorded. The row is immutable, so this is the whole answer.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppliedStrategy {
    pub apply_id: String,
    pub approval_id: String,
    pub content_hash: String,
    pub applied_at: String,
    pub supersedes_apply_id: Option<String>,
}

/// One approval of the strategy, reduced to the keys a client lists and applies by.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StrategyApproval {
    pub approval_id: String,
    pub content_hash: String,
    pub candidate_revision: i64,
    pub approved_at: String,
}

/// Every approval of a strategy, newest first.
///
/// The order is [`CoreService::strategy_applied`]'s total order (`candidate_revision` DESC,
/// `approved_at` DESC, `approval_id` DESC), so `items[0]` is that read's `newest_approval`. This
/// list is what lets a client aim Apply at a superseded approval: the rollback path.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StrategyApprovals {
    pub items: Vec<StrategyApproval>,
}

/// The strategy's applied identity and the newest approval it could move to.
///
/// `candidate_revision` defines approval newness (compass D11); `approved_at` and `approval_id`
/// only make a tied query deterministic. A client compares `newest_approval.approval_id` with
/// `current_apply.approval_id` and never derives newness from either timestamp.
///
/// `applied_document` is the frozen document `current_apply` installed, replayed through
/// [`verified_apply_document`] so its canonical hash is the one the apply recorded;
/// `applied_document_refusal` is why that replay failed. Both are `None` when nothing is
/// applied, and exactly one is `Some` otherwise — "no applied document" and "the applied
/// document does not replay" must never look alike (D16(c)).
#[derive(Debug, Clone, serde::Serialize)]
pub struct StrategyAppliedStatus {
    pub current_apply: Option<AppliedStrategy>,
    pub newest_approval: Option<StrategyApproval>,
    pub applied_document: Option<WorkflowDocument>,
    pub applied_document_refusal: Option<AppliedDocumentRefusal>,
}

/// Why the current apply's frozen document could not be replayed, as the status read reports it.
///
/// `reason` is [`ApplyLoadRefusal::reason`]'s stable `snake_case` wire code — the same code the
/// schedule intake and the recovery path name — so a client switches on it, never on `message`.
#[derive(Debug, Clone, serde::Serialize)]
pub struct AppliedDocumentRefusal {
    pub reason: &'static str,
    pub message: String,
}

impl CoreService {
    /// The applied identity of `strategy_id`, the document it installed and its newest approval,
    /// all read from the append-only rows. The apply side is [`current_apply`] — the same answer
    /// the schedule intake branches on, so the Canvas and the run never disagree about which
    /// apply is live — and the document is the one that apply installed.
    ///
    /// # Errors
    ///
    /// The principal and store errors, and the replay's outer error: an apply naming an approval
    /// row this strategy does not have, or a store failure. That is an error response, never a
    /// body — it is not "never applied" and not `applied_document_refusal`, whose only content is
    /// the typed [`ApplyLoadRefusal`]. A strategy with no apply reports `current_apply: null` and
    /// both document fields `null` without reading a document; a strategy with no approval
    /// reports `newest_approval: null`. Neither is a refusal.
    pub async fn strategy_applied(
        &self,
        principal: &Principal,
        strategy_id: &str,
    ) -> CoreResult<StrategyAppliedStatus> {
        self.verify_principal(principal)?;

        let current_apply = current_apply(&self.inner.pool, strategy_id).await?;

        let (applied_document, applied_document_refusal) = match current_apply.as_ref() {
            None => (None, None),
            Some(apply) => match verified_apply_document(
                &self.inner.pool,
                self.nexus_home(),
                strategy_id,
                apply,
            )
            .await?
            {
                Ok(document) => (Some(document), None),
                Err(refusal) => (
                    None,
                    Some(AppliedDocumentRefusal {
                        reason: refusal.reason(),
                        message: refusal.to_string(),
                    }),
                ),
            },
        };

        let newest_approval = sqlx::query_as::<_, (String, String, i64, String)>(
            "SELECT approval_id, content_hash, candidate_revision, approved_at \
               FROM strategy_approvals \
              WHERE strategy_id = ? \
              ORDER BY candidate_revision DESC, approved_at DESC, approval_id DESC \
              LIMIT 1",
        )
        .bind(strategy_id)
        .fetch_optional(&self.inner.pool)
        .await
        .map_err(|error| crate::error::local_db_err(error.into()))?
        .map(
            |(approval_id, content_hash, candidate_revision, approved_at)| StrategyApproval {
                approval_id,
                content_hash,
                candidate_revision,
                approved_at,
            },
        );

        Ok(StrategyAppliedStatus {
            current_apply,
            newest_approval,
            applied_document,
            applied_document_refusal,
        })
    }

    /// Every approval of `strategy_id`, newest first in the same total order as
    /// [`Self::strategy_applied`]'s `newest_approval` (`items[0]` is that same row).
    ///
    /// This is the read rollback needs: the status read reports only the newest approval, so a
    /// client cannot name a superseded one without it. Nothing is written — applying a superseded
    /// approval appends an apply row and leaves every approval row in place.
    ///
    /// # Errors
    ///
    /// The principal and store errors. A strategy with no approval reports `items: []`; that is
    /// not a refusal.
    pub async fn strategy_approvals(
        &self,
        principal: &Principal,
        strategy_id: &str,
    ) -> CoreResult<StrategyApprovals> {
        self.verify_principal(principal)?;

        let items = sqlx::query_as::<_, (String, String, i64, String)>(
            "SELECT approval_id, content_hash, candidate_revision, approved_at \
               FROM strategy_approvals \
              WHERE strategy_id = ? \
              ORDER BY candidate_revision DESC, approved_at DESC, approval_id DESC",
        )
        .bind(strategy_id)
        .fetch_all(&self.inner.pool)
        .await
        .map_err(|error| crate::error::local_db_err(error.into()))?
        .into_iter()
        .map(
            |(approval_id, content_hash, candidate_revision, approved_at)| StrategyApproval {
                approval_id,
                content_hash,
                candidate_revision,
                approved_at,
            },
        )
        .collect();

        Ok(StrategyApprovals { items })
    }

    /// Apply the approval `approval_content_hash` names, recording it as the strategy's current
    /// applied identity.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InputValidation`] carrying an [`ApplyRefusal::details`] envelope for
    /// a strategy that does not resolve to a user bundle, an unknown content hash, another
    /// strategy's approval, an approval whose document no longer hashes to its identity, an
    /// approved document the builder refuses, a strategy with no schedule, or an approval that is
    /// already current; the principal and store errors otherwise. A refusal writes nothing.
    pub async fn apply_strategy(
        &self,
        principal: &Principal,
        strategy_id: &str,
        approval_content_hash: &str,
    ) -> CoreResult<AppliedStrategy> {
        self.verify_principal(principal)?;

        let user_preset = nexus_preset::resolve_preset(
            strategy_id,
            self.nexus_home(),
            &nexus_preset::capability_catalog::BuiltinCapabilityCatalog,
        )
        .is_ok_and(|loaded| is_user_preset(strategy_id, &loaded));
        if !user_preset {
            return Err(ApplyRefusal::StrategyNotUserPreset {
                strategy_id: strategy_id.to_owned(),
            }
            .into_error());
        }

        let (approval_id, stored_hash) = self
            .verified_approval_identity(strategy_id, approval_content_hash)
            .await?;

        let apply_id = format!("apl_{}", uuid::Uuid::new_v4());
        let applied_at = chrono::Utc::now().to_rfc3339();

        // Both current-row picks order by `applied_at DESC, apply_id DESC` — the same total order
        // [`current_apply`] reports — so a strategy holding two successor-less rows (only an
        // admitted non-core writer can create that) still names one defined row everywhere.
        //
        // The insert and its refusal probe decide in ONE snapshot (I-000469): the insert takes the
        // write lock and the probe reads through the same transaction, so a supersede can never
        // land between them and leave a no-row insert with no refusal to name.
        let mut transaction = self
            .inner
            .pool
            .begin()
            .await
            .map_err(|error| crate::error::local_db_err(error.into()))?;
        let written = sqlx::query_as::<_, (String, Option<String>)>(
            "INSERT INTO strategy_applies \
               (apply_id, strategy_id, approval_id, content_hash, applied_at, applied_by, \
                supersedes_apply_id) \
             SELECT ?, ?, ?, ?, ?, ?, \
                    (SELECT cur.apply_id FROM strategy_applies cur \
                      WHERE cur.strategy_id = ? \
                        AND NOT EXISTS (SELECT 1 FROM strategy_applies later \
                                         WHERE later.supersedes_apply_id = cur.apply_id) \
                      ORDER BY cur.applied_at DESC, cur.apply_id DESC \
                      LIMIT 1) \
             WHERE NOT EXISTS (SELECT 1 FROM strategy_applies cur \
                                WHERE cur.strategy_id = ? \
                                  AND cur.approval_id = ? \
                                  AND NOT EXISTS (SELECT 1 FROM strategy_applies later \
                                                   WHERE later.supersedes_apply_id = cur.apply_id)) \
               AND EXISTS (SELECT 1 FROM creator_schedules WHERE preset_id = ?) \
             RETURNING apply_id, supersedes_apply_id",
        )
        .bind(&apply_id)
        .bind(strategy_id)
        .bind(&approval_id)
        .bind(&stored_hash)
        .bind(&applied_at)
        .bind(principal.creator_id())
        .bind(strategy_id)
        .bind(strategy_id)
        .bind(&approval_id)
        .bind(strategy_id)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| crate::error::local_db_err(error.into()))?;

        let Some((apply_id, supersedes_apply_id)) = written else {
            #[cfg(any(test, feature = "test-hooks"))]
            reach_apply_refusal_seam(strategy_id, &approval_id).await;
            let refusal = self
                .refuse_apply(&mut transaction, strategy_id, &approval_id)
                .await;
            transaction
                .rollback()
                .await
                .map_err(|error| crate::error::local_db_err(error.into()))?;
            return Err(refusal);
        };
        transaction
            .commit()
            .await
            .map_err(|error| crate::error::local_db_err(error.into()))?;

        Ok(AppliedStrategy {
            apply_id,
            approval_id,
            content_hash: stored_hash,
            applied_at,
            supersedes_apply_id,
        })
    }

    /// Resolve the approval `approval_content_hash` names for `strategy_id` and prove the identity
    /// it promises, returning the approval id and the hash the apply row records.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InputValidation`] carrying an [`ApplyRefusal::details`] envelope for an
    /// unknown content hash, another strategy's approval, an approval whose document no longer
    /// hashes to its identity, an approval whose pattern pin moved or cannot be confirmed, an
    /// approved document the builder refuses; the store errors otherwise.
    async fn verified_approval_identity(
        &self,
        strategy_id: &str,
        approval_content_hash: &str,
    ) -> CoreResult<(String, String)> {
        let Some((approval_id, approval_strategy_id, stored_hash, document_json, pins_json)) =
            sqlx::query_as::<_, (String, String, String, String, String)>(
                "SELECT approval_id, strategy_id, content_hash, document_json, pins_json \
                 FROM strategy_approvals \
                 WHERE content_hash = ? \
                 ORDER BY (strategy_id = ?) DESC, approval_id DESC",
            )
            .bind(approval_content_hash)
            .bind(strategy_id)
            .fetch_optional(&self.inner.pool)
            .await
            .map_err(|error| crate::error::local_db_err(error.into()))?
        else {
            return Err(ApplyRefusal::ApprovalNotFound {
                content_hash: approval_content_hash.to_owned(),
            }
            .into_error());
        };
        if approval_strategy_id != strategy_id {
            return Err(ApplyRefusal::ApprovalStrategyMismatch {
                content_hash: stored_hash,
                approval_strategy_id,
                strategy_id: strategy_id.to_owned(),
            }
            .into_error());
        }

        let document: WorkflowDocument = serde_json::from_str(&document_json).map_err(|error| {
            apply_internal(format!(
                "approval `{approval_id}` document does not decode: {error}"
            ))
        })?;
        let recomputed_hash = content_hash(&document)
            .map_err(|error| apply_internal(format!("canonical hash: {error}")))?;
        if recomputed_hash != stored_hash {
            return Err(ApplyRefusal::ApprovalHashChanged {
                approval_id,
                stored_hash,
                recomputed_hash,
            }
            .into_error());
        }

        verify_pins(self.nexus_home(), &document, &pins_json)
            .map_err(|failure| pin_apply_refusal(&approval_id, failure).into_error())?;

        if let Err(reason) = validate_ir_buildability(&document, IrTemplateSource::OwnSource) {
            return Err(ApplyRefusal::DocumentUnbuildable {
                builder_reason: reason.to_string(),
            }
            .into_error());
        }

        Ok((approval_id, stored_hash))
    }

    /// Name why the apply statement wrote no row, reading through that statement's own
    /// transaction: the probe therefore sees the snapshot the guard decided on. The schedule is
    /// checked first because it is the only precondition that lives outside the apply table; with
    /// it in place the sole remaining predicate is the already-current guard, so this probe repeats
    /// that guard's own condition — a successor-less row of this strategy naming this approval, in
    /// the same total order [`current_apply`] uses. A probe that asked only for "the current row"
    /// would name a different apply whenever a non-core writer left two successor-less rows,
    /// turning the typed conflict into an internal fault.
    async fn refuse_apply(
        &self,
        transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
        strategy_id: &str,
        approval_id: &str,
    ) -> CoreError {
        let scheduled: i64 = match sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM creator_schedules WHERE preset_id = ?)",
        )
        .bind(strategy_id)
        .fetch_one(&mut **transaction)
        .await
        {
            Ok(scheduled) => scheduled,
            Err(error) => {
                return apply_internal(format!("schedule probe: {error}"));
            }
        };
        if scheduled == 0 {
            return ApplyRefusal::StrategyHasNoSchedule {
                strategy_id: strategy_id.to_owned(),
            }
            .into_error();
        }
        let current: Option<String> = match sqlx::query_scalar(
            "SELECT cur.apply_id FROM strategy_applies cur \
              WHERE cur.strategy_id = ? \
                AND cur.approval_id = ? \
                AND NOT EXISTS (SELECT 1 FROM strategy_applies later \
                                 WHERE later.supersedes_apply_id = cur.apply_id) \
              ORDER BY cur.applied_at DESC, cur.apply_id DESC \
              LIMIT 1",
        )
        .bind(strategy_id)
        .bind(approval_id)
        .fetch_optional(&mut **transaction)
        .await
        {
            Ok(current) => current,
            Err(error) => return apply_internal(format!("current apply probe: {error}")),
        };
        current.map_or_else(
            || apply_internal("the apply insert wrote no row and no refusal explains it"),
            |apply_id| {
                ApplyRefusal::AlreadyApplied {
                    apply_id,
                    approval_id: approval_id.to_owned(),
                }
                .into_error()
            },
        )
    }
}

/// Test-only seam in the guard→probe window (I-000469).
///
/// An armed apply of the named strategy and approval pauses between its guarded insert and the
/// refusal probe, so a regression test can attempt the supersede that window used to allow. The
/// probe still runs in the insert's transaction, so the attempt must be refused by the store's
/// write lock rather than by the guard. Compiled out of the production build.
#[cfg(any(test, feature = "test-hooks"))]
static APPLY_REFUSAL_SEAM: tokio::sync::Mutex<Option<ApplyRefusalSeam>> =
    tokio::sync::Mutex::const_new(None);

#[cfg(any(test, feature = "test-hooks"))]
struct ApplyRefusalSeam {
    strategy_id: String,
    approval_id: String,
    reached: tokio::sync::oneshot::Sender<()>,
    resume: tokio::sync::oneshot::Receiver<()>,
}

#[cfg(any(test, feature = "test-hooks"))]
async fn reach_apply_refusal_seam(strategy_id: &str, approval_id: &str) {
    let seam = {
        let mut slot = APPLY_REFUSAL_SEAM.lock().await;
        let armed = slot
            .as_ref()
            .is_some_and(|seam| seam.strategy_id == strategy_id && seam.approval_id == approval_id);
        if armed {
            slot.take()
        } else {
            None
        }
    };
    let Some(seam) = seam else {
        return;
    };
    let _ = seam.reached.send(());
    let _ = seam.resume.await;
}

#[cfg(any(test, feature = "test-hooks"))]
impl CoreService {
    /// Arm the apply guard→probe seam for one apply: `reached` resolves when the apply of
    /// `approval_id` on `strategy_id` is paused inside the window, and sending on `resume`
    /// releases it. `#[doc(hidden)]` test seam, compiled out of the production build.
    ///
    /// # Panics
    ///
    /// When the process-global slot still holds an unconsumed armed seam: a parked handshake
    /// would otherwise sit there while another test arms its own, so overlapping seams are a loud
    /// failure instead of a silent steal. A test must therefore key its pair to itself and arm
    /// only once its previous seam was consumed.
    #[doc(hidden)]
    pub async fn arm_apply_refusal_seam(
        &self,
        strategy_id: &str,
        approval_id: &str,
    ) -> (
        tokio::sync::oneshot::Receiver<()>,
        tokio::sync::oneshot::Sender<()>,
    ) {
        let (reached, reached_rx) = tokio::sync::oneshot::channel();
        let (resume_tx, resume) = tokio::sync::oneshot::channel();
        let mut slot = APPLY_REFUSAL_SEAM.lock().await;
        assert!(
            slot.is_none(),
            "the apply guard→probe seam already holds an unconsumed armed handshake: \
             arm one seam at a time and let its apply take it"
        );
        *slot = Some(ApplyRefusalSeam {
            strategy_id: strategy_id.to_owned(),
            approval_id: approval_id.to_owned(),
            reached,
            resume,
        });
        drop(slot);
        (reached_rx, resume_tx)
    }
}

/// Whether `strategy_id` resolved to a user bundle — the only source the schedule intake reads an
/// apply from.
///
/// This is the ONE classification both sides of the apply gate use, so the two cannot disagree
/// about which strategies have an applied identity. Callers pass the id and the preset
/// [`nexus_preset::resolve_preset`] returned for it — never a second classifier: `locate_preset`
/// is embedded-first, so a user bundle that shadows an embedded id reads as `embedded` there.
///
/// A `_system.` id is not a user preset even though its bundle is on disk too (a system bundle
/// resolves to the same `Directory` source); no admitted writer gives one a schedule, and a
/// `_system.` id names no strategy.
#[must_use]
pub fn is_user_preset(strategy_id: &str, loaded: &nexus_preset::LoadedPreset) -> bool {
    nexus_home_layout::strategy_intake::is_user_strategy_bundle(
        strategy_id,
        matches!(
            loaded.source_identity,
            Some(nexus_preset::source_identity::PresetSourceIdentity::Directory { .. })
        ),
    )
}

/// The apply that is live for `strategy_id`: the append-only row with no successor.
///
/// `None` when the strategy has no apply, which is the common case — "nothing applied" is an
/// answer, not a refusal. A second successor-less row is only reachable by an admitted non-core
/// writer; `applied_at` then `apply_id` order the read so that case still has a defined winner.
///
/// This is the ONE definition of "current apply": [`CoreService::strategy_applied`] reports it
/// and the schedule intake branches on it.
///
/// # Errors
///
/// The store errors only.
pub async fn current_apply(
    pool: &sqlx::SqlitePool,
    strategy_id: &str,
) -> CoreResult<Option<AppliedStrategy>> {
    Ok(
        sqlx::query_as::<_, (String, String, String, String, Option<String>)>(
            "SELECT cur.apply_id, cur.approval_id, cur.content_hash, cur.applied_at, \
                cur.supersedes_apply_id \
           FROM strategy_applies cur \
          WHERE cur.strategy_id = ? \
            AND NOT EXISTS (SELECT 1 FROM strategy_applies later \
                             WHERE later.supersedes_apply_id = cur.apply_id) \
          ORDER BY cur.applied_at DESC, cur.apply_id DESC \
          LIMIT 1",
        )
        .bind(strategy_id)
        .fetch_optional(pool)
        .await
        .map_err(|error| crate::error::local_db_err(error.into()))?
        .map(
            |(apply_id, approval_id, content_hash, applied_at, supersedes_apply_id)| {
                AppliedStrategy {
                    apply_id,
                    approval_id,
                    content_hash,
                    applied_at,
                    supersedes_apply_id,
                }
            },
        ),
    )
}

/// The document of `apply`, re-verified against the identity the apply recorded.
///
/// This is the same recomputation [`CoreService::apply_strategy`] performs, run again at load:
/// the approval's frozen bytes are decoded and hashed, and a document that no longer hashes to
/// the apply's recorded `content_hash` is refused instead of quietly reinterpreted. The refusal
/// is returned rather than raised so the caller can name it in its own control-plane error; a
/// row edited after it was applied must never be a 500 and never a silent new graph.
///
/// The frozen pin snapshot is re-compared here too, against `<nexus_home>/patterns` — the same
/// second guard Apply runs — so a pattern body edited after apply is refused at admission and at
/// recovery instead of being run. A document with no recorded pin never reads that root.
///
/// # Errors
///
/// The store errors, and `CoreError::Internal` when the approval row an apply names is absent
/// for this strategy (the foreign key the core maintains; only an admitted non-core writer can
/// reach that state).
///
/// The read is keyed by `approval_id` AND `strategy_id`, the pairing [`CoreService::apply_strategy`]
/// enforces with [`ApplyRefusal::ApprovalStrategyMismatch`]: a replay must never return a foreign
/// strategy's frozen document, even one written by a non-core writer.
pub async fn verified_apply_document(
    pool: &sqlx::SqlitePool,
    nexus_home: &std::path::Path,
    strategy_id: &str,
    apply: &AppliedStrategy,
) -> CoreResult<Result<WorkflowDocument, ApplyLoadRefusal>> {
    let row: Option<(String, String)> = sqlx::query_as(
        "SELECT document_json, pins_json FROM strategy_approvals \
          WHERE approval_id = ? AND strategy_id = ?",
    )
    .bind(&apply.approval_id)
    .bind(strategy_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| crate::error::local_db_err(error.into()))?;
    let Some((document_json, pins_json)) = row else {
        return Err(apply_internal(format!(
            "apply `{}` names approval `{}`, which this strategy has no row for",
            apply.apply_id, apply.approval_id
        )));
    };
    let document = match verify_applied_document(strategy_id, apply, &document_json) {
        Ok(document) => document,
        Err(refusal) => return Ok(Err(refusal)),
    };
    if let Err(failure) = verify_pins(nexus_home, &document, &pins_json) {
        return Ok(Err(pin_load_refusal(strategy_id, apply, &failure)));
    }
    Ok(Ok(document))
}

/// Why an applied identity cannot be replayed. Nothing was executed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyLoadRefusal {
    /// The applied document's frozen bytes no longer hash to the identity the apply recorded.
    DocumentChanged {
        /// The strategy whose apply no longer replays.
        strategy_id: String,
        /// The apply that installed the identity.
        apply_id: String,
        /// The `content_hash` the apply recorded.
        stored_hash: String,
        /// The canonical hash of the stored `document_json`.
        recomputed_hash: String,
    },
    /// The applied document's frozen bytes no longer yield a canonical identity at all: they do
    /// not decode as a workflow document, or their hash cannot be computed.
    DocumentUnverifiable {
        /// The strategy whose apply no longer replays.
        strategy_id: String,
        /// The apply that installed the identity.
        apply_id: String,
        /// What could not be read.
        detail: String,
    },
    /// A pattern body the applied document pins no longer hashes to the pin the apply froze.
    PinChanged {
        /// The strategy whose apply no longer replays.
        strategy_id: String,
        /// The apply that installed the identity.
        apply_id: String,
        /// The nest that pins the package.
        nest_id: String,
        /// The pinned package id.
        package_id: String,
        /// The pinned package version.
        package_version: String,
        /// The hash the apply recorded.
        recorded_hash: String,
        /// The hash the registry now serves.
        scanned_hash: String,
    },
    /// The registry does not resolve a pin the applied document records.
    PinUnavailable {
        /// The strategy whose apply no longer replays.
        strategy_id: String,
        /// The apply that installed the identity.
        apply_id: String,
        /// The nest that pins the package.
        nest_id: String,
        /// The pinned package id.
        package_id: String,
        /// The pinned package version.
        package_version: String,
        /// The hash the apply recorded.
        recorded_hash: String,
    },
    /// The applied identity's recorded pin snapshot cannot be relied on: it does not decode, it
    /// disagrees with the document it was frozen beside, or the pattern registry cannot be read.
    PinsUnverifiable {
        /// The strategy whose apply no longer replays.
        strategy_id: String,
        /// The apply that installed the identity.
        apply_id: String,
        /// What could not be compared, and why.
        detail: String,
    },
}

impl ApplyLoadRefusal {
    /// The stable lowercase wire code the refusal renders as — a coded refusal (HTTP 400 at the
    /// adapter), never an internal fault.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::DocumentChanged { .. } => "applied_document_changed",
            Self::DocumentUnverifiable { .. } => "applied_document_unverifiable",
            Self::PinChanged { .. } => "applied_pin_changed",
            Self::PinUnavailable { .. } => "applied_pin_unavailable",
            Self::PinsUnverifiable { .. } => "applied_pins_unverifiable",
        }
    }

    /// The core error the load refusal returns, naming the strategy and the apply.
    #[must_use]
    pub fn into_error(self) -> CoreError {
        CoreError::Coded {
            code: self.reason().to_string(),
            message: self.to_string(),
        }
    }
}

impl std::fmt::Display for ApplyLoadRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DocumentChanged {
                strategy_id,
                apply_id,
                stored_hash,
                recomputed_hash,
            } => write!(
                f,
                "applied identity of strategy `{strategy_id}` (apply `{apply_id}`) records content \
                 hash `{stored_hash}`, but its frozen document now hashes to \
                 `{recomputed_hash}`: the applied bytes changed after apply"
            ),
            Self::DocumentUnverifiable {
                strategy_id,
                apply_id,
                detail,
            } => write!(
                f,
                "applied identity of strategy `{strategy_id}` (apply `{apply_id}`) cannot be \
                 replayed: {detail}"
            ),
            Self::PinChanged {
                strategy_id,
                apply_id,
                nest_id,
                package_id,
                package_version,
                recorded_hash,
                scanned_hash,
            } => write!(
                f,
                "applied identity of strategy `{strategy_id}` (apply `{apply_id}`) pins \
                 `{package_id}@{package_version}` at nest `{nest_id}` with content hash \
                 `{recorded_hash}`, but the registry now serves `{scanned_hash}`: the pinned \
                 bytes moved after apply"
            ),
            Self::PinUnavailable {
                strategy_id,
                apply_id,
                nest_id,
                package_id,
                package_version,
                recorded_hash,
            } => write!(
                f,
                "applied identity of strategy `{strategy_id}` (apply `{apply_id}`) pins \
                 `{package_id}@{package_version}` at nest `{nest_id}` with content hash \
                 `{recorded_hash}`, but the registry has no available package for that pin"
            ),
            Self::PinsUnverifiable {
                strategy_id,
                apply_id,
                detail,
            } => write!(
                f,
                "applied identity of strategy `{strategy_id}` (apply `{apply_id}`) cannot have \
                 its pins verified: {detail}"
            ),
        }
    }
}

/// Read `pins_json` and re-compare it with the registry, scanning `<nexus_home>/patterns` only
/// when a pin was recorded — a document that pins nothing never touches that root.
// `PinVerificationFailure` carries the whole pin identity (nest, package, both hashes); boxing it
// would only move the same bytes behind a pointer on this one refusal path.
#[allow(clippy::result_large_err)]
fn verify_pins(
    nexus_home: &std::path::Path,
    document: &WorkflowDocument,
    pins_json: &str,
) -> Result<(), PinVerificationFailure> {
    let recorded: Vec<ResolvedPatternPin> = serde_json::from_str(pins_json).map_err(|error| {
        PinVerificationFailure::SnapshotMismatch {
            detail: format!("the recorded pins do not decode: {error}"),
        }
    })?;
    if recorded.is_empty() {
        return verify_recorded_pins(document, &recorded, &[]);
    }
    let patterns_dir = nexus_home.join("patterns");
    let packages =
        nexus_pattern::scan_pattern_packages(&patterns_dir, nexus_workflow_ir::IR_VERSION)
            .map_err(|error| PinVerificationFailure::SnapshotMismatch {
                detail: format!("pattern registry {}: {error}", patterns_dir.display()),
            })?;
    verify_recorded_pins(document, &recorded, &packages)
}

/// The apply-time refusal a pin failure becomes, naming the approval it was read from.
fn pin_apply_refusal(approval_id: &str, failure: PinVerificationFailure) -> ApplyRefusal {
    match failure {
        PinVerificationFailure::Changed {
            nest_id,
            package,
            recorded_hash,
            scanned_hash,
        } => ApplyRefusal::PinChanged {
            nest_id,
            package_id: package.id,
            package_version: package.version,
            recorded_hash,
            scanned_hash,
        },
        PinVerificationFailure::Unavailable {
            nest_id,
            package,
            recorded_hash,
        } => ApplyRefusal::PinUnavailable {
            nest_id,
            package_id: package.id,
            package_version: package.version,
            recorded_hash,
        },
        PinVerificationFailure::SnapshotMismatch { detail } => ApplyRefusal::PinsUnverifiable {
            approval_id: approval_id.to_owned(),
            detail,
        },
    }
}

/// The load-time refusal a pin failure becomes, naming the strategy and the apply.
fn pin_load_refusal(
    strategy_id: &str,
    apply: &AppliedStrategy,
    failure: &PinVerificationFailure,
) -> ApplyLoadRefusal {
    let (strategy_id, apply_id) = (strategy_id.to_owned(), apply.apply_id.clone());
    match failure {
        PinVerificationFailure::Changed {
            nest_id,
            package,
            recorded_hash,
            scanned_hash,
        } => ApplyLoadRefusal::PinChanged {
            strategy_id,
            apply_id,
            nest_id: nest_id.clone(),
            package_id: package.id.clone(),
            package_version: package.version.clone(),
            recorded_hash: recorded_hash.clone(),
            scanned_hash: scanned_hash.clone(),
        },
        PinVerificationFailure::Unavailable {
            nest_id,
            package,
            recorded_hash,
        } => ApplyLoadRefusal::PinUnavailable {
            strategy_id,
            apply_id,
            nest_id: nest_id.clone(),
            package_id: package.id.clone(),
            package_version: package.version.clone(),
            recorded_hash: recorded_hash.clone(),
        },
        PinVerificationFailure::SnapshotMismatch { detail } => ApplyLoadRefusal::PinsUnverifiable {
            strategy_id,
            apply_id,
            detail: detail.clone(),
        },
    }
}

/// Recompute an applied document's identity exactly as apply time did.
// `ApplyLoadRefusal` carries the whole pin identity plus the strategy and apply it belongs to;
// boxing it would change the type threaded through admission and recovery for one terminal path.
#[allow(clippy::result_large_err)]
fn verify_applied_document(
    strategy_id: &str,
    apply: &AppliedStrategy,
    document_json: &str,
) -> Result<WorkflowDocument, ApplyLoadRefusal> {
    let unverifiable = |detail: String| ApplyLoadRefusal::DocumentUnverifiable {
        strategy_id: strategy_id.to_owned(),
        apply_id: apply.apply_id.clone(),
        detail,
    };
    let document: WorkflowDocument =
        serde_json::from_str(document_json).map_err(|error| unverifiable(error.to_string()))?;
    let recomputed_hash =
        content_hash(&document).map_err(|error| unverifiable(error.to_string()))?;
    if recomputed_hash != apply.content_hash {
        return Err(ApplyLoadRefusal::DocumentChanged {
            strategy_id: strategy_id.to_owned(),
            apply_id: apply.apply_id.clone(),
            stored_hash: apply.content_hash.clone(),
            recomputed_hash,
        });
    }
    Ok(document)
}

#[cfg(test)]
mod tests {
    use super::*;

    const IR: &str = r#"{
        "ir_version": 0,
        "id": "ir-load",
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": "load-side replay fixture",
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "plan",
        "terminal": ["done"],
        "nodes": [
            {"id": "plan", "kind": "outer_state", "effect_class": "pure",
             "exit_when": {"kind": "rule"}},
            {"id": "done", "kind": "terminal", "effect_class": "pure"}
        ],
        "edges": [
            {"id": "plan->done", "from": "plan", "to": "done", "kind": "linear"}
        ],
        "nests": [],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "pure",
        "provenance": {"source": "compose"}
    }"#;

    fn applied(content_hash: String) -> AppliedStrategy {
        AppliedStrategy {
            apply_id: "apl_test".to_string(),
            approval_id: "apr_test".to_string(),
            content_hash,
            applied_at: "2026-01-01T00:00:00+00:00".to_string(),
            supersedes_apply_id: None,
        }
    }

    #[test]
    fn an_applied_document_replays_while_its_bytes_still_hash_to_the_apply() {
        let document: WorkflowDocument = serde_json::from_str(IR).unwrap();
        let hash = content_hash(&document).unwrap();

        let replayed = verify_applied_document("ir-load", &applied(hash), IR).unwrap();

        assert_eq!(replayed.id.as_str(), "ir-load");
    }

    #[test]
    fn a_document_edited_after_apply_is_refused_by_strategy_and_apply() {
        let document: WorkflowDocument = serde_json::from_str(IR).unwrap();
        let hash = content_hash(&document).unwrap();
        let mut edited = document;
        edited.version = 7;
        let edited_hash = content_hash(&edited).unwrap();

        let refusal = verify_applied_document(
            "ir-load",
            &applied(hash.clone()),
            &serde_json::to_string(&edited).unwrap(),
        )
        .unwrap_err();

        assert_eq!(refusal.reason(), "applied_document_changed");
        assert_eq!(
            refusal,
            ApplyLoadRefusal::DocumentChanged {
                strategy_id: "ir-load".to_string(),
                apply_id: "apl_test".to_string(),
                stored_hash: hash,
                recomputed_hash: edited_hash,
            }
        );
        let message = refusal.to_string();
        assert!(
            message.contains("ir-load") && message.contains("apl_test"),
            "the refusal names the strategy and the apply: {message}"
        );
        assert!(matches!(
            refusal.into_error(),
            CoreError::Coded { ref code, .. } if code == "applied_document_changed"
        ));
    }

    #[test]
    fn undecodable_applied_bytes_are_refused_rather_than_reinterpreted() {
        let refusal =
            verify_applied_document("ir-load", &applied("blake3:stored".into()), "{not json")
                .unwrap_err();

        assert_eq!(refusal.reason(), "applied_document_unverifiable");
        let message = refusal.to_string();
        assert!(message.contains("ir-load") && message.contains("apl_test"));
    }

    /// One refusal per variant: the variant list the service table has to cover.
    fn one_of_each_refusal() -> Vec<ApplyRefusal> {
        vec![
            ApplyRefusal::ApprovalNotFound {
                content_hash: String::new(),
            },
            ApplyRefusal::ApprovalStrategyMismatch {
                content_hash: String::new(),
                approval_strategy_id: String::new(),
                strategy_id: String::new(),
            },
            ApplyRefusal::ApprovalHashChanged {
                approval_id: String::new(),
                stored_hash: String::new(),
                recomputed_hash: String::new(),
            },
            ApplyRefusal::StrategyNotUserPreset {
                strategy_id: String::new(),
            },
            ApplyRefusal::StrategyHasNoSchedule {
                strategy_id: String::new(),
            },
            ApplyRefusal::AlreadyApplied {
                apply_id: String::new(),
                approval_id: String::new(),
            },
            ApplyRefusal::DocumentUnbuildable {
                builder_reason: String::new(),
            },
            ApplyRefusal::PinChanged {
                nest_id: String::new(),
                package_id: String::new(),
                package_version: String::new(),
                recorded_hash: String::new(),
                scanned_hash: String::new(),
            },
            ApplyRefusal::PinUnavailable {
                nest_id: String::new(),
                package_id: String::new(),
                package_version: String::new(),
                recorded_hash: String::new(),
            },
            ApplyRefusal::PinsUnverifiable {
                approval_id: String::new(),
                detail: String::new(),
            },
        ]
    }

    #[test]
    fn the_service_classification_table_covers_every_variant() {
        let table: serde_json::Map<String, serde_json::Value> = serde_json::from_str(
            &std::fs::read_to_string(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../apps/nexus-service/src/apply-refusal-classes.json"
            ))
            .expect("the service classification table is checked in"),
        )
        .expect("the service classification table decodes");

        let refusals = one_of_each_refusal();
        assert_eq!(
            table.len(),
            refusals.len(),
            "the service table names every ApplyRefusal variant and nothing else"
        );
        for refusal in &refusals {
            assert_eq!(
                table
                    .get(refusal.reason())
                    .and_then(serde_json::Value::as_str),
                Some(refusal.class().as_str()),
                "{}'s service table row states the class `ApplyRefusal::class` assigns",
                refusal.reason()
            );
        }
    }
}
