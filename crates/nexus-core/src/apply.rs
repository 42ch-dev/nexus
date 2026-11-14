//! Apply: install an approved snapshot as the strategy's applied identity.
//!
//! Plan: `.mstar/plans/v241-p35-applied-identity.md` §Architecture (Task 2).
//!
//! Apply is the only place an approval becomes the running identity, so each refusal here is a
//! safety property and every one of them happens before a row exists: the requested content hash
//! must name an approval of this strategy, the approval's frozen `document_json` must still hash
//! to the `content_hash` the approval recorded, the strategy must own a schedule (nothing would
//! run otherwise), and the approval must not already be the current apply.
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
//! (v1.241 P3.5, `.mstar/plans/v241-p35-intake-branch.md`).

use nexus_workflow_ir::{content_hash, WorkflowDocument};
use serde_json::Value;

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
}

impl ApplyRefusal {
    /// The stable lowercase reason a client branches on.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::ApprovalNotFound { .. } => "approval_not_found",
            Self::ApprovalStrategyMismatch { .. } => "approval_strategy_mismatch",
            Self::ApprovalHashChanged { .. } => "approval_hash_changed",
            Self::StrategyHasNoSchedule { .. } => "strategy_has_no_schedule",
            Self::AlreadyApplied { .. } => "already_applied",
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
            Self::StrategyHasNoSchedule { strategy_id } => {
                details.insert("strategy_id".into(), Value::from(strategy_id.clone()));
            }
            Self::AlreadyApplied {
                apply_id,
                approval_id,
            } => {
                details.insert("apply_id".into(), Value::from(apply_id.clone()));
                details.insert("approval_id".into(), Value::from(approval_id.clone()));
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
#[derive(Debug, Clone, serde::Serialize)]
pub struct StrategyAppliedStatus {
    pub current_apply: Option<AppliedStrategy>,
    pub newest_approval: Option<StrategyApproval>,
}

impl CoreService {
    /// The applied identity of `strategy_id` and its newest approval, both read from the
    /// append-only rows. The apply side is [`current_apply`] — the same answer the schedule
    /// intake branches on, so the Canvas and the run never disagree about which apply is live.
    ///
    /// # Errors
    ///
    /// The principal and store errors. A strategy with no apply reports `current_apply: null`
    /// and a strategy with no approval reports `newest_approval: null`; neither is a refusal.
    pub async fn strategy_applied(
        &self,
        principal: &Principal,
        strategy_id: &str,
    ) -> CoreResult<StrategyAppliedStatus> {
        self.verify_principal(principal)?;

        let current_apply = current_apply(&self.inner.pool, strategy_id).await?;

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
    /// an unknown content hash, another strategy's approval, an approval whose document no longer
    /// hashes to its identity, a strategy with no schedule, or an approval that is already
    /// current; the principal and store errors otherwise. A refusal writes nothing.
    pub async fn apply_strategy(
        &self,
        principal: &Principal,
        strategy_id: &str,
        approval_content_hash: &str,
    ) -> CoreResult<AppliedStrategy> {
        self.verify_principal(principal)?;

        // The approval lookup prefers this strategy's own row: two strategies approving the same
        // content is legal, and the request names the strategy it applies to.
        let Some((approval_id, approval_strategy_id, stored_hash, document_json)) =
            sqlx::query_as::<_, (String, String, String, String)>(
                "SELECT approval_id, strategy_id, content_hash, document_json \
                 FROM strategy_approvals \
                 WHERE content_hash = ? \
                 ORDER BY (strategy_id = ?) DESC",
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

        // The approval identity is recomputed from the frozen bytes, never trusted from the
        // column: an approval whose document moved no longer proves the hash it promises.
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

        let apply_id = format!("apl_{}", uuid::Uuid::new_v4());
        let applied_at = chrono::Utc::now().to_rfc3339();

        // One statement owns every guard that depends on the apply table: the supersede link is
        // the current row as this statement sees it, the already-current refusal is the absence of
        // any successor-less row naming this approval, and the schedule precondition rides in the
        // same statement so an apply cannot land for a strategy that lost its schedule. A
        // superseded approval passes the guard, which is what makes rollback a normal apply.
        let written = sqlx::query_as::<_, (String, Option<String>)>(
            "INSERT INTO strategy_applies \
               (apply_id, strategy_id, approval_id, content_hash, applied_at, applied_by, \
                supersedes_apply_id) \
             SELECT ?, ?, ?, ?, ?, ?, \
                    (SELECT cur.apply_id FROM strategy_applies cur \
                      WHERE cur.strategy_id = ? \
                        AND NOT EXISTS (SELECT 1 FROM strategy_applies later \
                                         WHERE later.supersedes_apply_id = cur.apply_id) \
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
        .fetch_optional(&self.inner.pool)
        .await
        .map_err(|error| crate::error::local_db_err(error.into()))?;

        let Some((apply_id, supersedes_apply_id)) = written else {
            return Err(self.refuse_apply(strategy_id, &approval_id).await);
        };

        Ok(AppliedStrategy {
            apply_id,
            approval_id,
            content_hash: stored_hash,
            applied_at,
            supersedes_apply_id,
        })
    }

    /// Name why the apply statement wrote no row. The schedule is checked first because it is the
    /// only precondition that lives outside the apply table; with it in place the sole remaining
    /// predicate is the already-current guard, so a missing row means this approval is current.
    async fn refuse_apply(&self, strategy_id: &str, approval_id: &str) -> CoreError {
        let scheduled: i64 = match sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM creator_schedules WHERE preset_id = ?)",
        )
        .bind(strategy_id)
        .fetch_one(&self.inner.pool)
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
        let current: Option<(String, String)> = match sqlx::query_as(
            "SELECT cur.apply_id, cur.approval_id FROM strategy_applies cur \
              WHERE cur.strategy_id = ? \
                AND NOT EXISTS (SELECT 1 FROM strategy_applies later \
                                 WHERE later.supersedes_apply_id = cur.apply_id)",
        )
        .bind(strategy_id)
        .fetch_optional(&self.inner.pool)
        .await
        {
            Ok(current) => current,
            Err(error) => return apply_internal(format!("current apply probe: {error}")),
        };
        match current {
            Some((apply_id, current_approval_id)) if current_approval_id == approval_id => {
                ApplyRefusal::AlreadyApplied {
                    apply_id,
                    approval_id: approval_id.to_owned(),
                }
                .into_error()
            }
            _ => apply_internal("the apply insert wrote no row and no refusal explains it"),
        }
    }
}

// ---------------------------------------------------------------------------
// Load-side replay: which apply is live, and whether it still replays.
// ---------------------------------------------------------------------------

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
/// # Errors
///
/// The store errors, and `CoreError::Internal` when the approval row an apply names is absent
/// for this strategy (the foreign key the core maintains; only an admitted non-core writer can
/// reach that state).
///
/// The read is keyed by `approval_id` AND `strategy_id`, the pairing [`CoreService::apply_strategy`]
/// enforces with `ApplyRefusal::ApprovalStrategyMismatch`: a replay must never return a foreign
/// strategy's frozen document, even one written by a non-core writer.
pub async fn verified_apply_document(
    pool: &sqlx::SqlitePool,
    strategy_id: &str,
    apply: &AppliedStrategy,
) -> CoreResult<Result<WorkflowDocument, ApplyLoadRefusal>> {
    let document_json: Option<String> = sqlx::query_scalar(
        "SELECT document_json FROM strategy_approvals WHERE approval_id = ? AND strategy_id = ?",
    )
    .bind(&apply.approval_id)
    .bind(strategy_id)
    .fetch_optional(pool)
    .await
    .map_err(|error| crate::error::local_db_err(error.into()))?;
    let Some(document_json) = document_json else {
        return Err(apply_internal(format!(
            "apply `{}` names approval `{}`, which this strategy has no row for",
            apply.apply_id, apply.approval_id
        )));
    };
    Ok(verify_applied_document(strategy_id, apply, &document_json))
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
}

impl ApplyLoadRefusal {
    /// The stable lowercase wire code the refusal renders as — a coded refusal (HTTP 400 at the
    /// adapter), never an internal fault.
    #[must_use]
    pub const fn reason(&self) -> &'static str {
        match self {
            Self::DocumentChanged { .. } => "applied_document_changed",
            Self::DocumentUnverifiable { .. } => "applied_document_unverifiable",
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
        }
    }
}

/// Recompute an applied document's identity exactly as apply time did.
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
        // Edit the frozen bytes behind the apply's back; only the recomputation can catch it.
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
}
