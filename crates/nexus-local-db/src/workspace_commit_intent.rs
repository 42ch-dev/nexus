//! Durable workspace commit intents (v1.188 P3).

use crate::LocalDbError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntentState {
    Applying,
    RollingBack,
    Committed,
    RolledBack,
    RecoveryConflict,
}

impl IntentState {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Applying => "applying",
            Self::RollingBack => "rolling_back",
            Self::Committed => "committed",
            Self::RolledBack => "rolled_back",
            Self::RecoveryConflict => "recovery_conflict",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "applying" => Some(Self::Applying),
            "rolling_back" => Some(Self::RollingBack),
            "committed" => Some(Self::Committed),
            "rolled_back" => Some(Self::RolledBack),
            "recovery_conflict" => Some(Self::RecoveryConflict),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentEntryJson {
    pub path: String,
    pub op: String,
    pub pre_hash: Option<String>,
    pub post_hash: Option<String>,
    pub stage_basename: String,
    pub backup_basename: Option<String>,
    pub mode: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct CommitIntentRow {
    pub session_id: String,
    pub workspace_root: String,
    pub revision: String,
    pub request_digest: String,
    pub state: IntentState,
    pub entries: Vec<IntentEntryJson>,
    pub error_category: Option<String>,
}

#[derive(Debug, Clone)]
pub enum ClaimSessionResult {
    Claimed,
    NotFound,
    AlreadyConsumed,
    Expired,
    AlreadyClaimed { revision: String },
    DigestConflict,
}

pub async fn claim_session_and_insert_intent(
    pool: &SqlitePool,
    session_id: &str,
    revision: &str,
    workspace_root: &str,
    request_digest: &str,
    entries_json: &str,
) -> Result<ClaimSessionResult, LocalDbError> {
    let mut tx = pool.begin().await?;

    let existing = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM workspace_commit_intents \
         WHERE session_id = ? AND request_digest = ? AND state = 'committed'",
    )
    .bind(session_id)
    .bind(request_digest)
    .fetch_one(&mut *tx)
    .await?;
    if existing > 0 {
        tx.rollback().await?;
        return Ok(ClaimSessionResult::DigestConflict);
    }

    let claim = sqlx::query(
        "UPDATE workspace_sessions SET claimed_by_revision = ? \
         WHERE session_id = ? AND consumed = 0 \
         AND expires_at > strftime('%Y-%m-%dT%H:%M:%SZ', 'now') \
         AND (claimed_by_revision IS NULL OR claimed_by_revision = '')",
    )
    .bind(revision)
    .bind(session_id)
    .execute(&mut *tx)
    .await?;
    if claim.rows_affected() == 0 {
        tx.rollback().await?;
        let row = crate::workspace_session::get_session(pool, session_id).await?;
        return Ok(match row {
            None => ClaimSessionResult::NotFound,
            Some(s) if s.consumed => ClaimSessionResult::AlreadyConsumed,
            Some(s) if s.claimed_by_revision.as_deref().is_some_and(|r| !r.is_empty()) => {
                ClaimSessionResult::AlreadyClaimed {
                    revision: s.claimed_by_revision.clone().unwrap_or_default(),
                }
            }
            _ => ClaimSessionResult::Expired,
        });
    }

    sqlx::query(
        "INSERT INTO workspace_commit_intents \
         (session_id, workspace_root, revision, request_digest, state, entries_json) \
         VALUES (?, ?, ?, ?, 'applying', ?)",
    )
    .bind(session_id)
    .bind(workspace_root)
    .bind(revision)
    .bind(request_digest)
    .bind(entries_json)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(ClaimSessionResult::Claimed)
}

pub async fn get_committed_intent_by_digest(
    pool: &SqlitePool,
    session_id: &str,
    request_digest: &str,
) -> Result<Option<CommitIntentRow>, LocalDbError> {
    let row = sqlx::query_as::<_, IntentRowRaw>(
        "SELECT session_id, workspace_root, revision, request_digest, state, entries_json, error_category \
         FROM workspace_commit_intents \
         WHERE session_id = ? AND request_digest = ? AND state = 'committed' LIMIT 1",
    )
    .bind(session_id)
    .bind(request_digest)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(IntentRowRaw::into_row))
}

pub async fn get_intent_by_revision(
    pool: &SqlitePool,
    revision: &str,
) -> Result<Option<CommitIntentRow>, LocalDbError> {
    let row = sqlx::query_as::<_, IntentRowRaw>(
        "SELECT session_id, workspace_root, revision, request_digest, state, entries_json, error_category \
         FROM workspace_commit_intents WHERE revision = ?",
    )
    .bind(revision)
    .fetch_optional(pool)
    .await?;
    Ok(row.map(IntentRowRaw::into_row))
}

pub async fn update_intent_state(
    pool: &SqlitePool,
    revision: &str,
    state: IntentState,
    error_category: Option<&str>,
) -> Result<(), LocalDbError> {
    sqlx::query(
        "UPDATE workspace_commit_intents SET state = ?, error_category = ?, \
         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE revision = ?",
    )
    .bind(state.as_str())
    .bind(error_category)
    .bind(revision)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn finalize_committed_intent(
    pool: &SqlitePool,
    revision: &str,
    session_id: &str,
) -> Result<bool, LocalDbError> {
    let mut tx = pool.begin().await?;
    let updated = sqlx::query(
        "UPDATE workspace_commit_intents SET state = 'committed', \
         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') \
         WHERE revision = ? AND state = 'applying'",
    )
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    if updated.rows_affected() == 0 {
        tx.rollback().await?;
        return Ok(false);
    }
    sqlx::query(
        "UPDATE workspace_sessions SET consumed = 1, claimed_by_revision = NULL \
         WHERE session_id = ? AND claimed_by_revision = ?",
    )
    .bind(session_id)
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(true)
}

pub async fn finalize_rolled_back_intent(
    pool: &SqlitePool,
    revision: &str,
    session_id: &str,
) -> Result<(), LocalDbError> {
    let mut tx = pool.begin().await?;
    sqlx::query(
        "UPDATE workspace_commit_intents SET state = 'rolled_back', \
         updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE revision = ?",
    )
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "UPDATE workspace_sessions SET consumed = 1, claimed_by_revision = NULL WHERE session_id = ?",
    )
    .bind(session_id)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

pub async fn release_session_claim(
    pool: &SqlitePool,
    session_id: &str,
    revision: &str,
) -> Result<(), LocalDbError> {
    sqlx::query(
        "UPDATE workspace_sessions SET claimed_by_revision = NULL \
         WHERE session_id = ? AND claimed_by_revision = ? AND consumed = 0",
    )
    .bind(session_id)
    .bind(revision)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn list_unsettled_intents(
    pool: &SqlitePool,
    workspace_root: &str,
) -> Result<Vec<CommitIntentRow>, LocalDbError> {
    let rows = sqlx::query_as::<_, IntentRowRaw>(
        "SELECT session_id, workspace_root, revision, request_digest, state, entries_json, error_category \
         FROM workspace_commit_intents \
         WHERE workspace_root = ? AND state IN ('applying', 'rolling_back', 'recovery_conflict')",
    )
    .bind(workspace_root)
    .fetch_all(pool)
    .await?;
    Ok(rows.into_iter().map(IntentRowRaw::into_row).collect())
}

#[derive(Debug, sqlx::FromRow)]
struct IntentRowRaw {
    session_id: String,
    workspace_root: String,
    revision: String,
    request_digest: String,
    state: String,
    entries_json: String,
    error_category: Option<String>,
}

impl IntentRowRaw {
    fn into_row(self) -> CommitIntentRow {
        let entries: Vec<IntentEntryJson> =
            serde_json::from_str(&self.entries_json).unwrap_or_default();
        CommitIntentRow {
            session_id: self.session_id,
            workspace_root: self.workspace_root,
            revision: self.revision,
            request_digest: self.request_digest,
            state: IntentState::parse(&self.state).unwrap_or(IntentState::RecoveryConflict),
            entries,
            error_category: self.error_category,
        }
    }
}
