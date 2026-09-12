//! Recoverable multi-file workspace commit (v1.188 P3 L2).

use std::path::Path;
use std::sync::Arc;

use nexus_contracts::local::orchestration::{WorkspaceChangeEntry, WorkspaceChangeOp};
use nexus_local_db as db;
use sha2::{Digest, Sha256};

use super::bounds::{
    validate_encoded_body, validate_entries_json_len, validate_hash_hex, validate_relative_path,
};
use super::commit_fs::{
    decode_base64, hash_bytes, require_parent_exists, ScopeMutation, CREATE_FILE_MODE, MAX_CHANGES,
    MAX_FILE_BYTES, MAX_TOTAL_BYTES,
};
use super::session::{
    canonicalize_workspace_root, enforce_path_boundary, SessionError, SessionId,
    WorkspaceSessionManager,
};

/// Fail the commit when a test has armed this crash point.
///
/// Compiled to a no-op in production: the crash points live in the gated
/// `workspace::test_hooks` seam.
fn test_crash_if(point: &str) -> Result<(), SessionError> {
    #[cfg(any(test, feature = "test-hooks"))]
    if super::test_hooks::crash_point_is(point) {
        return Err(SessionError::Internal(format!("test_crash:{point}")));
    }
    let _ = point;
    Ok(())
}

/// Rendezvous at the owner's admission boundary when a test gate is armed.
async fn test_owner_gate_admitted(session_id: &str) {
    #[cfg(any(test, feature = "test-hooks"))]
    super::test_hooks::owner_gate_admitted(session_id).await;
    let _ = session_id;
}

/// Signal that the retained owner task has settled, when a gate is armed.
fn test_owner_gate_settled(session_id: &str) {
    #[cfg(any(test, feature = "test-hooks"))]
    super::test_hooks::owner_gate_settled(session_id);
    let _ = session_id;
}

#[cfg(any(test, feature = "test-hooks"))]
use super::test_hooks::clear_crash_point;

/// Clear an inherited crash point at owner spawn.
fn clear_inherited_crash_point() {
    #[cfg(any(test, feature = "test-hooks"))]
    clear_crash_point();
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    pub revision: String,
    pub committed: bool,
}

/// Recover unsettled intents, taking the manager's mutation lock for the pass.
///
/// # Errors
///
/// Returns whatever [`recover_unsettled_locked`] reports: a
/// [`SessionError::Database`] failure while listing or settling rows, or the
/// recovery-conflict error for an intent another writer already claimed.
pub async fn recover_unsettled(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
) -> Result<(), SessionError> {
    let _guard = mgr.lock_mutation().await;
    recover_unsettled_locked(mgr, workspace_root).await
}

/// Recover unsettled intents when the caller already holds [`WorkspaceSessionManager::lock_mutation`].
///
/// # Errors
///
/// Returns a [`SessionError::Database`] failure while listing or settling
/// rows, or [`SessionError::RecoveryConflict`] when an intent is owned by
/// another writer.
pub async fn recover_unsettled_locked(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
) -> Result<(), SessionError> {
    let intents = list_unsettled_or_conflict(mgr, workspace_root).await?;
    for intent in intents {
        match intent.state {
            db::IntentState::RecoveryConflict => {
                return Err(SessionError::RecoveryConflict(workspace_root.to_string()));
            }
            db::IntentState::Applying => {
                recover_applying_intent_locked(mgr, workspace_root, &intent).await?;
            }
            db::IntentState::RollingBack => {
                rollback_intent_locked(mgr, workspace_root, &intent).await?;
            }
            _ => {}
        }
    }
    Ok(())
}

async fn list_unsettled_or_conflict(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
) -> Result<Vec<db::CommitIntentRow>, SessionError> {
    match db::list_unsettled_intents(mgr.pool().as_ref(), workspace_root).await {
        Ok(rows) => Ok(rows),
        Err(db::LocalDbError::CorruptIntent { .. }) => {
            Err(SessionError::RecoveryConflict(workspace_root.to_string()))
        }
        Err(e) => Err(SessionError::Database(e.to_string())),
    }
}

async fn persist_recovery_conflict(
    mgr: &WorkspaceSessionManager,
    revision: &str,
    workspace_root: &str,
    category: &str,
) -> Result<(), SessionError> {
    db::update_intent_state(
        mgr.pool().as_ref(),
        revision,
        db::IntentState::RecoveryConflict,
        Some(category),
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;
    Err(SessionError::RecoveryConflict(workspace_root.to_string()))
}

async fn load_recovery_session(
    mgr: &WorkspaceSessionManager,
    intent: &db::CommitIntentRow,
) -> Result<db::WorkspaceSessionRow, SessionError> {
    let row = db::get_session(mgr.pool().as_ref(), &intent.session_id)
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?
        .ok_or_else(|| SessionError::NotFound(SessionId(intent.session_id.clone())))?;
    if row.claimed_by_revision.as_deref() != Some(intent.revision.as_str()) {
        return Err(SessionError::RecoveryConflict(
            intent.workspace_root.clone(),
        ));
    }
    Ok(row)
}

async fn recover_applying_intent_locked(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
    intent: &db::CommitIntentRow,
) -> Result<(), SessionError> {
    if !intent.entries_metadata_valid() {
        return persist_recovery_conflict(mgr, &intent.revision, workspace_root, "corrupt_entries")
            .await;
    }
    let row = match load_recovery_session(mgr, intent).await {
        Ok(r) => r,
        Err(e) => {
            if matches!(e, SessionError::RecoveryConflict(_)) {
                let _ = persist_recovery_conflict(
                    mgr,
                    &intent.revision,
                    workspace_root,
                    "claim_mismatch",
                )
                .await;
            }
            return Err(e);
        }
    };
    let canonical_root = canonicalize_workspace_root(Path::new(workspace_root)).await?;
    let scope = match ScopeMutation::open(&canonical_root, &row.relative_path) {
        Ok(s) => s,
        Err(e) => {
            return persist_recovery_conflict(
                mgr,
                &intent.revision,
                workspace_root,
                "scope_open_failed",
            )
            .await
            .map_err(|_| SessionError::Io(e.to_string()));
        }
    };

    if intent.entries.is_empty() {
        // Staging never persisted its entry metadata (crash between claim and
        // `entries_json` write), so nothing was applied: this intent can only
        // roll back. Reached via `test_crash_if("after_intent")`.
        db::update_intent_state(
            mgr.pool().as_ref(),
            &intent.revision,
            db::IntentState::RollingBack,
            Some("staging_incomplete"),
        )
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?;
        return rollback_intent_locked(mgr, workspace_root, intent).await;
    }

    let mut all_applied = true;
    for entry in &intent.entries {
        let applied = match entry.op.as_str() {
            "create" | "modify" => scope
                .hash_target(&entry.path)
                .is_ok_and(|h| Some(h) == entry.post_hash),
            "delete" => !scope.target_exists(&entry.path).unwrap_or(true),
            _ => false,
        };
        if !applied {
            all_applied = false;
            break;
        }
    }

    if all_applied {
        let finalized = db::finalize_committed_intent(
            mgr.pool().as_ref(),
            &intent.revision,
            &intent.session_id,
        )
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?;
        if !finalized {
            return persist_recovery_conflict(
                mgr,
                &intent.revision,
                workspace_root,
                "finalize_failed",
            )
            .await;
        }
        // The commit is settled; the per-entry stage/backup material is now
        // dead weight and is removed only after the durable settle.
        for entry in &intent.entries {
            scope.cleanup_entry(
                &entry.path,
                &entry.stage_basename,
                entry.backup_basename.as_deref(),
            );
        }
        return Ok(());
    }

    db::update_intent_state(
        mgr.pool().as_ref(),
        &intent.revision,
        db::IntentState::RollingBack,
        Some("applying_incomplete"),
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;
    rollback_intent_locked(mgr, workspace_root, intent).await
}

/// Roll back one intent when the caller already holds [`WorkspaceSessionManager::lock_mutation`].
async fn rollback_intent_locked(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
    intent: &db::CommitIntentRow,
) -> Result<(), SessionError> {
    if !intent.entries_metadata_valid() {
        return persist_recovery_conflict(mgr, &intent.revision, workspace_root, "corrupt_entries")
            .await;
    }

    let row = match load_recovery_session(mgr, intent).await {
        Ok(r) => r,
        Err(e) => {
            if matches!(e, SessionError::RecoveryConflict(_)) {
                let _ = persist_recovery_conflict(
                    mgr,
                    &intent.revision,
                    workspace_root,
                    "claim_mismatch",
                )
                .await;
            }
            return Err(e);
        }
    };

    let canonical_root = canonicalize_workspace_root(Path::new(workspace_root)).await?;
    let scope_dir = super::scope::scope_directory(&canonical_root, &row.relative_path)?;
    let scope = match ScopeMutation::open(&canonical_root, &row.relative_path) {
        Ok(s) => s,
        Err(e) => {
            return persist_recovery_conflict(
                mgr,
                &intent.revision,
                workspace_root,
                "scope_open_failed",
            )
            .await
            .map_err(|_| SessionError::Io(e.to_string()));
        }
    };

    db::update_intent_state(
        mgr.pool().as_ref(),
        &intent.revision,
        db::IntentState::RollingBack,
        None,
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;

    test_crash_if("during_rollback")?;

    // Restore every preimage FIRST, and clean nothing up yet: while the intent
    // is still `rolling_back` the stage/backup artifacts are the evidence a
    // replayed rollback needs, and removing a restored delete target before the
    // rollback is durable is exactly the corruption this ordering prevents.
    for entry in intent.entries.iter().rev() {
        enforce_path_boundary(&scope_dir.join(&entry.path), &canonical_root)?;
        let restore = scope.restore_preimage(
            &entry.path,
            entry.backup_basename.as_deref(),
            &entry.stage_basename,
            entry.pre_hash.as_deref(),
            entry.post_hash.as_deref(),
            entry.mode,
        );
        if let Err(e) = restore {
            let _ = persist_recovery_conflict(
                mgr,
                &intent.revision,
                workspace_root,
                "rollback_third_state",
            )
            .await;
            return Err(SessionError::Io(e.to_string()));
        }
    }

    test_crash_if("after_rollback_restore_before_finalize")?;

    // Durable rolled-back transition BEFORE any artifact removal.
    db::finalize_rolled_back_intent(mgr.pool().as_ref(), &intent.revision, &intent.session_id)
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?;

    // Settled: removal is best-effort. A crash here leaves dead material that
    // the startup sweep reaps WITHOUT replaying the rollback, so recovery can
    // never delete a live restored file.
    for entry in &intent.entries {
        scope.cleanup_entry(
            &entry.path,
            &entry.stage_basename,
            entry.backup_basename.as_deref(),
        );
    }
    Ok(())
}

/// Canonicalize a manifest into fixed, deterministic order (v1.188 P3).
///
/// Each path is lexically normalized (`.` components dropped, separators
/// collapsed) and the entries are SORTED BY NORMALIZED PATH. Both the request
/// digest and the mutation order are derived from this canonical form, so:
///
/// * a retry that merely REORDERS the same entries produces an identical
///   digest and is served as an idempotent replay of the original revision;
/// * the lock/mutation order is fixed, never caller-dependent.
///
/// Duplicates and prefix-overlaps are rejected AFTER normalization (`a.txt`
/// and `./a.txt` are the same target).
///
/// # Errors
///
/// Returns [`SessionError::ManifestInvalid`] for a path that fails
/// [`commit_fs::split_relative`](super::commit_fs::split_relative), for a
/// duplicate normalized path, or for two paths where one is a prefix of the
/// other.
pub fn normalize_changes(
    changes: &[WorkspaceChangeEntry],
) -> Result<Vec<WorkspaceChangeEntry>, SessionError> {
    let mut normalized = Vec::with_capacity(changes.len());
    for change in changes {
        let components = super::commit_fs::split_relative(&change.path)
            .map_err(|e| SessionError::ManifestInvalid(e.to_string()))?;
        let mut cloned = change.clone();
        cloned.path = components.join("/");
        normalized.push(cloned);
    }

    normalized.sort_by(|a, b| a.path.cmp(&b.path));

    for window in normalized.windows(2) {
        let (previous, current) = (&window[0], &window[1]);
        if previous.path == current.path {
            return Err(SessionError::ManifestInvalid(format!(
                "duplicate path after normalization: {}",
                current.path
            )));
        }
        if current.path.starts_with(&format!("{}/", previous.path)) {
            return Err(SessionError::ManifestInvalid(format!(
                "overlapping paths: {} and {}",
                previous.path, current.path
            )));
        }
    }

    Ok(normalized)
}

/// Validate a change manifest's shape before any filesystem work.
///
/// # Errors
///
/// Returns [`SessionError::ManifestInvalid`] when the manifest is empty,
/// exceeds [`commit_fs::MAX_CHANGES`](super::commit_fs::MAX_CHANGES), carries an
/// invalid op or path, or exceeds the per-file/total byte bounds.
pub fn validate_manifest(changes: &[WorkspaceChangeEntry]) -> Result<(), SessionError> {
    if changes.is_empty() {
        return Err(SessionError::ManifestInvalid(
            "changes must not be empty".into(),
        ));
    }
    if changes.len() > MAX_CHANGES {
        return Err(SessionError::ManifestInvalid(format!(
            "too many changes (max {MAX_CHANGES})"
        )));
    }
    let mut total = 0usize;
    let mut paths: Vec<String> = Vec::new();
    for change in changes {
        validate_relative_path(&change.path)?;
        for other in &paths {
            if other.starts_with(&format!("{}/", change.path))
                || change.path.starts_with(&format!("{other}/"))
            {
                return Err(SessionError::ManifestInvalid(format!(
                    "overlapping paths: {} and {}",
                    change.path, other
                )));
            }
        }
        if paths.iter().any(|p| p == &change.path) {
            return Err(SessionError::ManifestInvalid(format!(
                "duplicate path: {}",
                change.path
            )));
        }
        paths.push(change.path.clone());

        match change.op {
            WorkspaceChangeOp::Create => {
                if change.expected_hash.is_some() {
                    return Err(SessionError::ManifestInvalid(
                        "create must not include expectedHash".into(),
                    ));
                }
                let content = change.content_base64.as_deref().ok_or_else(|| {
                    SessionError::ManifestInvalid("create requires contentBase64".into())
                })?;
                validate_encoded_body(content)?;
                let bytes = decode_base64(content).map_err(SessionError::ManifestInvalid)?;
                if bytes.len() > MAX_FILE_BYTES {
                    return Err(SessionError::ManifestInvalid(format!(
                        "file {} exceeds {MAX_FILE_BYTES} bytes",
                        change.path
                    )));
                }
                total += bytes.len();
            }
            WorkspaceChangeOp::Modify => {
                let content = change.content_base64.as_deref().ok_or_else(|| {
                    SessionError::ManifestInvalid("modify requires contentBase64".into())
                })?;
                let expected = change.expected_hash.as_deref().ok_or_else(|| {
                    SessionError::ManifestInvalid("modify requires expectedHash".into())
                })?;
                validate_hash_hex(expected)?;
                validate_encoded_body(content)?;
                let bytes = decode_base64(content).map_err(SessionError::ManifestInvalid)?;
                if bytes.len() > MAX_FILE_BYTES {
                    return Err(SessionError::ManifestInvalid(format!(
                        "file {} exceeds {MAX_FILE_BYTES} bytes",
                        change.path
                    )));
                }
                total += bytes.len();
            }
            WorkspaceChangeOp::Delete => {
                if change.content_base64.is_some() {
                    return Err(SessionError::ManifestInvalid(
                        "delete must not include contentBase64".into(),
                    ));
                }
                let expected = change.expected_hash.as_deref().ok_or_else(|| {
                    SessionError::ManifestInvalid("delete requires expectedHash".into())
                })?;
                validate_hash_hex(expected)?;
            }
        }
    }
    if total > MAX_TOTAL_BYTES {
        return Err(SessionError::ManifestInvalid(format!(
            "total payload exceeds {MAX_TOTAL_BYTES} bytes"
        )));
    }
    Ok(())
}

fn request_digest(session_id: &str, changes: &[WorkspaceChangeEntry]) -> String {
    let payload = serde_json::to_string(changes).unwrap_or_else(|_| "[]".to_string());
    let mut sha = Sha256::new();
    sha.update(session_id.as_bytes());
    sha.update(payload.as_bytes());
    hex::encode(sha.finalize())
}

fn new_revision() -> String {
    format!("rev_{}", uuid::Uuid::new_v4())
}

async fn committed_replay_outcome(
    mgr: &WorkspaceSessionManager,
    session_id: &SessionId,
    digest: &str,
) -> Result<Option<CommitOutcome>, SessionError> {
    let existing =
        db::get_committed_intent_by_digest(mgr.pool().as_ref(), &session_id.to_string(), digest)
            .await
            .map_err(|e| SessionError::Database(e.to_string()))?;
    Ok(existing.map(|row| CommitOutcome {
        revision: row.revision,
        committed: true,
    }))
}

/// Apply one recoverable commit: normalize, validate, CAS-verify each entry at
/// the mutation boundary, persist the durable intent, then settle it.
///
/// # Errors
///
/// Returns [`SessionError::Internal`] when the manager has no recoverable
/// authority, [`SessionError::ManifestInvalid`] for a malformed manifest,
/// [`SessionError::HashConflict`] when a target no longer matches its expected
/// hash at the mutation boundary, [`SessionError::NotFound`] for an unknown
/// session, [`SessionError::Database`] on persistence failure, and
/// [`SessionError::Io`] on filesystem failure.
///
/// # Panics
///
/// Never in practice: the settled-entries JSON is read back with
/// `.expect("entry pushed above")` only after this function pushed that entry.
//
// The body is one ordered commit transaction: normalize -> validate -> capture
// and verify each entry -> persist the intent -> settle. Splitting it into
// helpers would scatter the CAS/settlement ordering that makes a partially
// applied commit recoverable, so the length is deliberate.
#[allow(clippy::too_many_lines)]
pub async fn commit_recoverable(
    mgr: &WorkspaceSessionManager,
    session_id: &SessionId,
    changes: &[WorkspaceChangeEntry],
) -> Result<CommitOutcome, SessionError> {
    if mgr.recoverable_config().is_none() {
        return Err(SessionError::Internal(
            "recoverable workspace authority required".into(),
        ));
    }

    let row = db::get_session(mgr.pool().as_ref(), &session_id.to_string())
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?
        .ok_or_else(|| SessionError::NotFound(session_id.clone()))?;

    // Canonical form first: the digest and the mutation order below are both
    // derived from it, so a reordered-equivalent retry is identical.
    let changes = normalize_changes(changes)?;
    let changes = changes.as_slice();

    validate_manifest(changes)?;
    let digest = request_digest(&session_id.to_string(), changes);

    if let Some(committed_digest) =
        db::get_committed_request_digest(mgr.pool().as_ref(), &session_id.to_string())
            .await
            .map_err(|e| SessionError::Database(e.to_string()))?
    {
        if committed_digest != digest {
            return Err(SessionError::AlreadyCommitted(session_id.clone()));
        }
        if let Some(outcome) = committed_replay_outcome(mgr, session_id, &digest).await? {
            return Ok(outcome);
        }
    }

    if row.consumed {
        return Err(SessionError::AlreadyCommitted(session_id.clone()));
    }

    if !db::is_session_active(mgr.pool().as_ref(), &session_id.to_string())
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?
    {
        return Err(SessionError::Expired(session_id.clone()));
    }

    let canonical_root = canonicalize_workspace_root(Path::new(&row.workspace_root)).await?;
    let workspace_root = canonical_root.to_string_lossy().into_owned();
    let scope_dir = super::scope::scope_directory(&canonical_root, &row.relative_path)?;

    if db::workspace_has_recovery_conflict(mgr.pool().as_ref(), &workspace_root)
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?
    {
        return Err(SessionError::RecoveryConflict(workspace_root));
    }

    recover_unsettled(mgr, &workspace_root).await?;

    mgr.validate_contract_manifest(session_id, changes).await?;

    let revision = new_revision();
    let _guard = mgr.lock_mutation().await;

    let claim = db::claim_session_and_insert_intent(
        mgr.pool().as_ref(),
        &session_id.to_string(),
        &revision,
        &workspace_root,
        &digest,
        "[]",
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;

    match claim {
        db::ClaimSessionResult::Claimed => {}
        db::ClaimSessionResult::DigestConflict => {
            if let Some(outcome) = committed_replay_outcome(mgr, session_id, &digest).await? {
                return Ok(outcome);
            }
            return Err(SessionError::ManifestInvalid("digest conflict".into()));
        }
        db::ClaimSessionResult::AlreadyClaimed { revision: other } => {
            if let Some(outcome) = committed_replay_outcome(mgr, session_id, &digest).await? {
                return Ok(outcome);
            }
            return Err(SessionError::AlreadyCommitted(SessionId(other)));
        }
        db::ClaimSessionResult::NotFound => return Err(SessionError::NotFound(session_id.clone())),
        db::ClaimSessionResult::AlreadyConsumed => {
            if let Some(outcome) = committed_replay_outcome(mgr, session_id, &digest).await? {
                return Ok(outcome);
            }
            return Err(SessionError::AlreadyCommitted(session_id.clone()));
        }
        db::ClaimSessionResult::Expired => return Err(SessionError::Expired(session_id.clone())),
    }

    test_owner_gate_admitted(&session_id.to_string()).await;

    test_crash_if("after_intent")?;

    let scope = ScopeMutation::open(&canonical_root, &row.relative_path)
        .map_err(|e| SessionError::Io(e.to_string()))?;
    let mut entries_json: Vec<db::IntentEntryJson> = Vec::new();
    // Every artifact created for this commit, so any failure path can clean up
    // exactly what it wrote (rel path, stage basename, backup basename).
    let mut staged: Vec<(String, String, Option<String>)> = Vec::new();

    let staging = (|| {
        let rev_tag = revision.replace("rev_", "");
        for (idx, change) in changes.iter().enumerate() {
            let target = super::scope::resolve_in_scope(&scope_dir, &change.path)?;
            enforce_path_boundary(&target, &canonical_root)?;
            require_parent_exists(&target)
                .map_err(|e| SessionError::ManifestInvalid(e.to_string()))?;
            let stage_basename = format!(".nexus-stage-{rev_tag}-{idx}");
            let backup_basename = if scope.target_exists(&change.path).unwrap_or(false) {
                Some(format!(".nexus-backup-{rev_tag}-{idx}"))
            } else {
                None
            };

            let (pre_hash, post_hash, mode) = match change.op {
                WorkspaceChangeOp::Create => {
                    let bytes = decode_base64(change.content_base64.as_deref().unwrap_or(""))
                        .map_err(SessionError::ManifestInvalid)?;
                    scope
                        .write_stage(
                            &change.path,
                            &stage_basename,
                            &bytes,
                            Some(CREATE_FILE_MODE),
                        )
                        .map_err(|e| SessionError::Io(e.to_string()))?;
                    (None, Some(hash_bytes(&bytes)), Some(CREATE_FILE_MODE))
                }
                WorkspaceChangeOp::Modify => {
                    let bytes = decode_base64(change.content_base64.as_deref().unwrap_or(""))
                        .map_err(SessionError::ManifestInvalid)?;
                    let captured_mode = if let Some(ref backup_name) = backup_basename {
                        scope
                            .backup_target(&change.path, backup_name)
                            .map_err(|e| SessionError::Io(e.to_string()))?
                    } else {
                        None
                    };
                    scope
                        .write_stage(&change.path, &stage_basename, &bytes, captured_mode)
                        .map_err(|e| SessionError::Io(e.to_string()))?;
                    (
                        change.expected_hash.clone(),
                        Some(hash_bytes(&bytes)),
                        captured_mode,
                    )
                }
                WorkspaceChangeOp::Delete => {
                    if let Some(ref backup_name) = backup_basename {
                        let mode = scope
                            .backup_target(&change.path, backup_name)
                            .map_err(|e| SessionError::Io(e.to_string()))?;
                        (change.expected_hash.clone(), None, mode)
                    } else {
                        (change.expected_hash.clone(), None, None)
                    }
                }
            };

            entries_json.push(db::IntentEntryJson {
                path: change.path.clone(),
                op: match change.op {
                    WorkspaceChangeOp::Create => "create".to_string(),
                    WorkspaceChangeOp::Modify => "modify".to_string(),
                    WorkspaceChangeOp::Delete => "delete".to_string(),
                },
                pre_hash,
                post_hash,
                stage_basename,
                backup_basename,
                mode,
            });
            let entry = entries_json.last().expect("entry pushed above");
            staged.push((
                change.path.clone(),
                entry.stage_basename.clone(),
                entry.backup_basename.clone(),
            ));
        }
        Ok(())
    })();

    if let Err(e) = staging {
        // Staging happens before any target mutation, so no durable material
        // can need recovery. The intent row is REMOVED and the claim RELEASED
        // in one transaction: neither an orphaned `rolling_back` row nor a
        // claimed-but-abandoned session may outlive this failure.
        if let Err(db_err) = db::abort_intent_and_release_claim(
            mgr.pool().as_ref(),
            &revision,
            &session_id.to_string(),
        )
        .await
        {
            return Err(SessionError::Database(format!(
                "staging_failed ({e}); abort_failed ({db_err})"
            )));
        }
        // Only now that the intent is settled do we drop the staged material.
        cleanup_staged(&scope, &staged);
        return Err(e);
    }

    // Persisting the durable entry metadata is a metadata write that happens
    // AFTER staging. If it fails the commit must settle, not return early with
    // `?`: settle the intent (delete row + release claim in one transaction)
    // and only then drop the staged artifacts.
    let persist = async {
        let entries_serialized = serde_json::to_string(&entries_json)
            .map_err(|e| SessionError::Database(e.to_string()))?;
        validate_entries_json_len(&entries_serialized)?;
        sqlx::query("UPDATE workspace_commit_intents SET entries_json = ? WHERE revision = ?")
            .bind(&entries_serialized)
            .bind(&revision)
            .execute(mgr.pool().as_ref())
            .await
            .map_err(|e| SessionError::Database(e.to_string()))?;
        Ok::<(), SessionError>(())
    }
    .await;

    if let Err(e) = persist {
        if let Err(db_err) = db::abort_intent_and_release_claim(
            mgr.pool().as_ref(),
            &revision,
            &session_id.to_string(),
        )
        .await
        {
            return Err(SessionError::Database(format!(
                "metadata_persist_failed ({e}); abort_failed ({db_err})"
            )));
        }
        cleanup_staged(&scope, &staged);
        return Err(e);
    }

    test_crash_if("after_entries_persisted")?;

    for (idx, change) in changes.iter().enumerate() {
        let meta = &entries_json[idx];
        let result = match change.op {
            WorkspaceChangeOp::Create => scope.atomic_create(&change.path, &meta.stage_basename),
            WorkspaceChangeOp::Modify => scope.atomic_replace_verified(
                &change.path,
                &meta.stage_basename,
                meta.pre_hash.as_deref().unwrap_or(""),
            ),
            WorkspaceChangeOp::Delete => scope.atomic_delete_verified(
                &change.path,
                meta.pre_hash.as_deref().unwrap_or(""),
                &meta.stage_basename,
            ),
        };
        if let Err(e) = result {
            db::update_intent_state(
                mgr.pool().as_ref(),
                &revision,
                db::IntentState::RollingBack,
                Some("apply_failed"),
            )
            .await
            .map_err(|db_err| SessionError::Database(db_err.to_string()))?;

            let intent = db::get_intent_by_revision(mgr.pool().as_ref(), &revision)
                .await
                .map_err(|db_err| SessionError::Database(db_err.to_string()))?;
            let rollback = match intent {
                Some(intent) => rollback_intent_locked(mgr, &workspace_root, &intent).await,
                None => Err(SessionError::Internal(
                    "apply_failed: intent row vanished before rollback".into(),
                )),
            };

            return match rollback {
                // Rollback settled the intent; it removed its own per-entry
                // stage/backup material, so there is nothing left to clean.
                Ok(()) => Err(SessionError::Io(e.to_string())),
                // Rollback could not settle. The conflict evidence (stage and
                // backup files plus the persisted `recovery_conflict` row)
                // MUST survive for operator recovery — never clean it up, and
                // never report success.
                Err(rollback_err) => Err(SessionError::Internal(format!(
                    "apply_failed ({e}); rollback_failed ({rollback_err});                      recovery evidence preserved"
                ))),
            };
        }
        test_crash_if("after_file_apply")?;
    }

    test_crash_if("before_finalize")?;

    // Durable committed transition FIRST. Stage/backup artifacts are the
    // recovery evidence for an unsettled intent, so none of them may be
    // removed while the intent is still `applying`.
    let finalized =
        db::finalize_committed_intent(mgr.pool().as_ref(), &revision, &session_id.to_string())
            .await
            .map_err(|e| SessionError::Database(e.to_string()))?;
    if !finalized {
        return Err(SessionError::AlreadyCommitted(session_id.clone()));
    }

    test_crash_if("after_finalize_before_cleanup")?;

    // The commit is durable; artifact removal is now best-effort. A crash here
    // leaves dead material behind, which the startup sweep reaps without
    // reapplying anything and without raising a conflict.
    cleanup_staged(&scope, &staged);

    tracing::info!(
        session_id = %session_id,
        revision = %revision,
        change_count = changes.len(),
        "Workspace commit durable"
    );

    Ok(CommitOutcome {
        revision,
        committed: true,
    })
}

/// Remove every artifact written by a settled commit attempt.
fn cleanup_staged(scope: &ScopeMutation, staged: &[(String, String, Option<String>)]) {
    for (path, stage, backup) in staged {
        scope.cleanup_entry(path, stage, backup.as_deref());
    }
}

/// Durable commit on an `Arc` manager; retains ownership through caller cancellation.
///
/// # Errors
///
/// Returns whatever [`commit_recoverable`] reports for the manifest.
/// Cancellation of the awaiting caller does not abandon the owned commit; the
/// spawned owner is registered on the manager so a later pass can reap it.
pub async fn commit_recoverable_owned(
    mgr: Arc<WorkspaceSessionManager>,
    session_id: SessionId,
    changes: Vec<WorkspaceChangeEntry>,
) -> Result<CommitOutcome, SessionError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mgr2 = Arc::clone(&mgr);
    let sid = session_id.clone();
    let owner = tokio::spawn(async move {
        clear_inherited_crash_point();
        let result = commit_recoverable(&mgr2, &sid, &changes).await;
        test_owner_gate_settled(&sid.to_string());
        let _ = tx.send(result);
    });
    mgr.register_commit_owner(owner).await;
    rx.await
        .unwrap_or_else(|_| Err(SessionError::Internal("commit owner channel closed".into())))
}

/// Reap stage/backup artifacts left behind by ALREADY-SETTLED intents.
///
/// A crash between a durable settled transition (`committed` or `rolled_back`)
/// and the best-effort artifact cleanup leaves dead material on disk. This
/// sweep only removes that material: it never reapplies a change and never
/// replays a rollback (so it can never overwrite or delete bytes an external
/// writer produced afterwards) and never raises a conflict. Unreadable or
/// unresolvable rows are skipped.
async fn cleanup_settled_artifacts(mgr: &WorkspaceSessionManager) {
    let rows = match db::list_settled_intents_for_cleanup(mgr.pool().as_ref()).await {
        Ok(rows) => rows,
        Err(err) => {
            tracing::warn!(error = %err, "committed-artifact sweep: listing failed");
            return;
        }
    };

    for (session_id, workspace_root, entries_json) in rows {
        // FAIL CLOSED: this document names the files we are about to unlink, so
        // it goes through the full raw-size/entry/path/artifact-basename
        // validation before any filesystem work. A malformed row is skipped
        // whole — nothing is deleted and the evidence survives.
        let entries = match db::validate_cleanup_entries(&entries_json) {
            Ok(entries) => entries,
            Err(err) => {
                tracing::warn!(
                    error = %err,
                    session_id = %session_id,
                    "settled-artifact sweep: refusing cleanup for unvalidated metadata; \
                     evidence preserved"
                );
                continue;
            }
        };
        if entries.is_empty() {
            continue;
        }
        let Some(row) = db::get_session(mgr.pool().as_ref(), &session_id)
            .await
            .ok()
            .flatten()
        else {
            continue;
        };
        let Ok(canonical_root) = canonicalize_workspace_root(Path::new(&workspace_root)).await
        else {
            continue;
        };
        let Ok(scope) = ScopeMutation::open(&canonical_root, &row.relative_path) else {
            continue;
        };
        for entry in &entries {
            scope.cleanup_entry(
                &entry.path,
                &entry.stage_basename,
                entry.backup_basename.as_deref(),
            );
        }
    }
}

/// Startup recovery for every unsettled intent, under the mutation lock.
///
/// # Errors
///
/// Returns [`SessionError::Database`] when listing unsettled intents fails
/// (including a corrupt intent payload), or whatever the per-intent recovery
/// reports.
pub async fn startup_recovery_all(mgr: &WorkspaceSessionManager) -> Result<(), SessionError> {
    let _guard = mgr.lock_mutation().await;
    let intents = match db::list_all_unsettled_intents(mgr.pool().as_ref()).await {
        Ok(rows) => rows,
        Err(db::LocalDbError::CorruptIntent { workspace_root, .. }) => {
            return Err(SessionError::RecoveryConflict(workspace_root));
        }
        Err(e) => return Err(SessionError::Database(e.to_string())),
    };
    for intent in intents {
        if intent.state == db::IntentState::RecoveryConflict {
            return Err(SessionError::RecoveryConflict(
                intent.workspace_root.clone(),
            ));
        }
        if matches!(
            intent.state,
            db::IntentState::Applying | db::IntentState::RollingBack
        ) {
            recover_unsettled_locked(mgr, &intent.workspace_root).await?;
        }
    }

    // Unsettled work is resolved above; only now reap material belonging to
    // commits that already reached the durable committed transition.
    cleanup_settled_artifacts(mgr).await;
    Ok(())
}
