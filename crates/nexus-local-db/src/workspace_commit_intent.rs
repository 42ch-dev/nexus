//! Durable workspace commit intents (v1.188 P3 L2).

use crate::LocalDbError;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;

/// Maximum accepted size of a persisted `entries_json` document, in bytes.
///
/// Enforced on the RAW column value BEFORE `serde_json` deserializes it, so a
/// hostile or corrupt row cannot force unbounded parse work; and enforced
/// again on the serialized document before it is written.
pub const MAX_ENTRIES_JSON_BYTES: usize = 512_000;

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
    pub entries_json_raw: String,
}

impl CommitIntentRow {
    /// True when durable entry metadata was parsed successfully.
    #[must_use]
    pub fn entries_metadata_valid(&self) -> bool {
        serde_json::from_str::<Vec<IntentEntryJson>>(&self.entries_json_raw).is_ok()
    }
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

    if entries_json.len() > MAX_ENTRIES_JSON_BYTES {
        return Err(LocalDbError::ValidationError(format!(
            "intent entries_json exceeds {MAX_ENTRIES_JSON_BYTES} bytes"
        )));
    }

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
    row.map(|r| r.try_into_row()).transpose()
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
    row.map(|r| r.try_into_row()).transpose()
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


/// Remove an intent that never applied and release its claim, atomically.
///
/// Staging failures happen before any target mutation, so there is no durable
/// material to recover: the row is DELETED and the claim CLEARED in one
/// transaction. Neither an orphaned `rolling_back` row nor a claimed-but-
/// abandoned session can survive this call.
pub async fn abort_intent_and_release_claim(
    pool: &SqlitePool,
    revision: &str,
    session_id: &str,
) -> Result<(), LocalDbError> {
    let mut tx = pool.begin().await?;
    sqlx::query("DELETE FROM workspace_commit_intents WHERE revision = ? AND state = 'applying'")
        .bind(revision)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE workspace_sessions SET claimed_by_revision = NULL \
         WHERE session_id = ? AND claimed_by_revision = ? AND consumed = 0",
    )
    .bind(session_id)
    .bind(revision)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(())
}

/// Most recent committed intent for a canonical workspace root.
///
/// Root comparison canonicalizes both sides (the stored value may be a
/// non-canonical alias, e.g. `/var` vs `/private/var`); `rowid DESC` makes
/// the pick deterministic.
pub async fn latest_committed_intent_for_root(
    pool: &SqlitePool,
    workspace_root: &str,
) -> Result<Option<CommitIntentRow>, LocalDbError> {
    let rows = sqlx::query_as::<_, IntentRowRaw>(
        "SELECT session_id, workspace_root, revision, request_digest, state, entries_json, error_category \
         FROM workspace_commit_intents WHERE state = 'committed' ORDER BY rowid DESC",
    )
    .fetch_all(pool)
    .await?;
    let row = rows
        .into_iter()
        .find(|row| workspace_roots_match(&row.workspace_root, workspace_root));
    row.map(IntentRowRaw::try_into_row).transpose()
}

/// Rows for post-settle artifact cleanup, parsed leniently by the caller.
///
/// Committed and rolled-back intents are already durable, so a row whose
/// metadata cannot be parsed must never fail startup: the caller skips what it
/// cannot read.
pub async fn list_settled_intents_for_cleanup(
    pool: &SqlitePool,
) -> Result<Vec<(String, String, String)>, LocalDbError> {
    let rows: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT session_id, workspace_root, entries_json FROM workspace_commit_intents \
         WHERE state IN ('committed', 'rolled_back')",
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn mark_intent_recovery_conflict(
    pool: &SqlitePool,
    revision: &str,
    category: &str,
) -> Result<(), LocalDbError> {
    sqlx::query(
        "UPDATE workspace_commit_intents SET state = 'recovery_conflict', error_category = ?,          updated_at = strftime('%Y-%m-%dT%H:%M:%SZ', 'now') WHERE revision = ?",
    )
    .bind(category)
    .bind(revision)
    .execute(pool)
    .await?;
    Ok(())
}

fn validate_intent_entry(entry: &IntentEntryJson) -> Result<(), LocalDbError> {
    if entry.path.is_empty() || entry.path.len() > 4096 {
        return Err(LocalDbError::ValidationError("invalid intent entry path".into()));
    }
    if entry.path.contains("..") || entry.path.starts_with('/') {
        return Err(LocalDbError::ValidationError("intent entry path must be relative".into()));
    }
    match entry.op.as_str() {
        "create" => {
            if entry.pre_hash.is_some() {
                return Err(LocalDbError::ValidationError("create entry must not have pre_hash".into()));
            }
            if entry.post_hash.as_deref().is_none_or(|h| h.len() != 64) {
                return Err(LocalDbError::ValidationError("create entry requires post_hash".into()));
            }
        }
        "modify" => {
            if entry.pre_hash.as_deref().is_none_or(|h| h.len() != 64)
                || entry.post_hash.as_deref().is_none_or(|h| h.len() != 64)
            {
                return Err(LocalDbError::ValidationError("modify entry requires pre/post hash".into()));
            }
        }
        "delete" => {
            if entry.pre_hash.as_deref().is_none_or(|h| h.len() != 64) || entry.post_hash.is_some() {
                return Err(LocalDbError::ValidationError("delete entry requires pre_hash only".into()));
            }
        }
        other => {
            return Err(LocalDbError::ValidationError(format!("unknown intent op: {other}")));
        }
    }
    for hash in entry.pre_hash.iter().chain(entry.post_hash.iter()) {
        if !hash.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f')) || hash.len() != 64 {
            return Err(LocalDbError::ValidationError("intent hash must be lowercase hex".into()));
        }
    }
    if entry.stage_basename.is_empty()
        || entry.stage_basename.len() > 256
        || !entry.stage_basename.starts_with(".nexus-")
        || entry.stage_basename.contains('/')
    {
        return Err(LocalDbError::ValidationError("invalid stage basename".into()));
    }
    if let Some(ref backup) = entry.backup_basename {
        if backup.is_empty()
            || backup.len() > 256
            || !backup.starts_with(".nexus-")
            || backup.contains('/')
        {
            return Err(LocalDbError::ValidationError("invalid backup basename".into()));
        }
    }
    Ok(())
}

async fn decode_intent_rows(pool: &SqlitePool, rows: Vec<IntentRowRaw>) -> Result<Vec<CommitIntentRow>, LocalDbError> {
    let mut out = Vec::new();
    for raw in rows {
        let revision = raw.revision.clone();
        let workspace_root = raw.workspace_root.clone();
        match raw.try_into_row() {
            Ok(row) => out.push(row),
            Err(_) => {
                mark_intent_recovery_conflict(pool, &revision, "corrupt_entries_json").await?;
                return Err(LocalDbError::CorruptIntent {
                    revision,
                    workspace_root,
                });
            }
        }
    }
    Ok(out)
}

fn canonicalize_workspace_path(path: &str) -> Result<std::path::PathBuf, LocalDbError> {
    std::fs::canonicalize(path).map_err(|e| LocalDbError::ValidationError(e.to_string()))
}

fn workspace_roots_match(stored: &str, workspace_root: &str) -> bool {
    match (
        canonicalize_workspace_path(stored),
        canonicalize_workspace_path(workspace_root),
    ) {
        (Ok(a), Ok(b)) => a == b,
        _ => stored == workspace_root,
    }
}

/// Return the committed request digest for a session, if one exists.
pub async fn get_committed_request_digest(
    pool: &SqlitePool,
    session_id: &str,
) -> Result<Option<String>, LocalDbError> {
    sqlx::query_scalar::<_, String>(
        "SELECT request_digest FROM workspace_commit_intents          WHERE session_id = ? AND state = 'committed' LIMIT 1",
    )
    .bind(session_id)
    .fetch_optional(pool)
    .await
    .map_err(LocalDbError::from)
}

/// Whether any intent is in `recovery_conflict` for the canonical workspace root.
pub async fn workspace_has_recovery_conflict(
    pool: &SqlitePool,
    workspace_root: &str,
) -> Result<bool, LocalDbError> {
    let roots: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT workspace_root FROM workspace_commit_intents          WHERE state = 'recovery_conflict'",
    )
    .fetch_all(pool)
    .await?;
    Ok(roots
        .iter()
        .any(|stored| workspace_roots_match(stored, workspace_root)))
}

pub async fn list_unsettled_intents(
    pool: &SqlitePool,
    workspace_root: &str,
) -> Result<Vec<CommitIntentRow>, LocalDbError> {
    let rows = sqlx::query_as::<_, IntentRowRaw>(
        "SELECT session_id, workspace_root, revision, request_digest, state, entries_json, error_category \
         FROM workspace_commit_intents \
         WHERE state IN ('applying', 'rolling_back', 'recovery_conflict')",
    )
    .fetch_all(pool)
    .await?;
    let rows = rows
        .into_iter()
        .filter(|row| workspace_roots_match(&row.workspace_root, workspace_root))
        .collect();
    decode_intent_rows(pool, rows).await
}

pub async fn list_all_unsettled_intents(pool: &SqlitePool) -> Result<Vec<CommitIntentRow>, LocalDbError> {
    let rows = sqlx::query_as::<_, IntentRowRaw>(
        "SELECT session_id, workspace_root, revision, request_digest, state, entries_json, error_category \
         FROM workspace_commit_intents \
         WHERE state IN ('applying', 'rolling_back', 'recovery_conflict')",
    )
    .fetch_all(pool)
    .await?;
    decode_intent_rows(pool, rows).await
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
    fn try_into_row(self) -> Result<CommitIntentRow, LocalDbError> {
        // T1: bound the RAW column bytes first — never hand an unbounded
        // document to the JSON parser.
        if self.entries_json.len() > MAX_ENTRIES_JSON_BYTES {
            return Err(LocalDbError::ValidationError(format!(
                "intent entries_json exceeds {MAX_ENTRIES_JSON_BYTES} bytes"
            )));
        }
        let entries: Vec<IntentEntryJson> = match serde_json::from_str(&self.entries_json) {
            Ok(v) => v,
            Err(e) => {
                return Err(LocalDbError::Sqlx(sqlx::Error::Decode(Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("corrupt intent entries_json: {e}"),
                )))));
            }
        };
        for entry in &entries {
            validate_intent_entry(entry)?;
        }
        let state = IntentState::parse(&self.state).ok_or_else(|| {
            LocalDbError::ValidationError(format!("unknown intent state: {}", self.state))
        })?;
        Ok(CommitIntentRow {
            session_id: self.session_id,
            workspace_root: self.workspace_root,
            revision: self.revision,
            request_digest: self.request_digest,
            state,
            entries,
            error_category: self.error_category,
            entries_json_raw: self.entries_json,
        })
    }
}
