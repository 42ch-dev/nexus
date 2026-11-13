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

impl CoreService {
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
