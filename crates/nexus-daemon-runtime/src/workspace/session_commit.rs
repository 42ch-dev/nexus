//! Recoverable multi-file workspace commit (v1.188 P3 L2).

use std::path::Path;
use std::sync::{Arc, Mutex};

use nexus_contracts::local::orchestration::{WorkspaceChangeEntry, WorkspaceChangeOp};
use nexus_local_db as db;
use sha2::{Digest, Sha256};

use super::bounds::{
    validate_encoded_body, validate_entries_json_len, validate_hash_hex, validate_relative_path,
};
use super::commit_fs::{
    decode_base64, hash_bytes, require_parent_exists, ScopeMutation, CREATE_FILE_MODE,
    MAX_CHANGES, MAX_FILE_BYTES, MAX_TOTAL_BYTES,
};
use super::session::{
    canonicalize_workspace_root, enforce_path_boundary, SessionError, SessionId,
    WorkspaceSessionManager,
};

/// Test-only crash barrier labels (integration tests simulate interruption).
static TEST_CRASH_POINT: Mutex<Option<&'static str>> = Mutex::new(None);

/// Set the next commit crash point (`None` disables simulation).
pub fn set_test_crash_point(point: Option<&'static str>) {
    *TEST_CRASH_POINT.lock().expect("crash point lock") = point;
}

fn test_crash_if(point: &str) -> Result<(), SessionError> {
    if *TEST_CRASH_POINT.lock().expect("crash point lock") == Some(point) {
        return Err(SessionError::Internal(format!("test_crash:{point}")));
    }
    Ok(())
}

/// Test-only rendezvous at the commit owner's admission boundary (v1.188 P3).
///
/// Lets a test prove caller-cancellation safety DETERMINISTICALLY: the owner
/// signals `admitted` once it holds the claim, the test cancels the awaiting
/// caller, then releases `proceed` and awaits `settled`. No sleeps, no races.
#[derive(Debug, Default)]
pub struct OwnerGate {
    /// Signalled when the owner has been admitted (claim held).
    pub admitted: Arc<tokio::sync::Notify>,
    /// The owner waits for this before continuing past admission.
    pub proceed: Arc<tokio::sync::Notify>,
    /// Signalled after the owner task settles.
    pub settled: Arc<tokio::sync::Notify>,
}

static TEST_OWNER_GATE: Mutex<Option<Arc<OwnerGate>>> = Mutex::new(None);

/// Install (or clear) the test owner gate.
pub fn set_test_owner_gate(gate: Option<Arc<OwnerGate>>) {
    *TEST_OWNER_GATE.lock().expect("owner gate lock") = gate;
}

fn current_owner_gate() -> Option<Arc<OwnerGate>> {
    TEST_OWNER_GATE
        .lock()
        .expect("owner gate lock")
        .as_ref()
        .map(Arc::clone)
}

async fn test_owner_gate_admitted() {
    if let Some(gate) = current_owner_gate() {
        gate.admitted.notify_one();
        gate.proceed.notified().await;
    }
}

fn test_owner_gate_settled() {
    if let Some(gate) = current_owner_gate() {
        gate.settled.notify_one();
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    pub revision: String,
    pub committed: bool,
}

pub async fn recover_unsettled(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
) -> Result<(), SessionError> {
    let _guard = mgr.lock_mutation().await;
    recover_unsettled_locked(mgr, workspace_root).await
}

/// Recover unsettled intents when the caller already holds [`WorkspaceSessionManager::lock_mutation`].
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
        return Err(SessionError::RecoveryConflict(intent.workspace_root.clone()));
    }
    Ok(row)
}

async fn recover_applying_intent_locked(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
    intent: &db::CommitIntentRow,
) -> Result<(), SessionError> {
    if !intent.entries_metadata_valid() {
        return persist_recovery_conflict(mgr, &intent.revision, workspace_root, "corrupt_entries").await;
    }
    let row = match load_recovery_session(mgr, intent).await {
        Ok(r) => r,
        Err(e) => {
            if matches!(e, SessionError::RecoveryConflict(_)) {
                let _ = persist_recovery_conflict(mgr, &intent.revision, workspace_root, "claim_mismatch").await;
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
                .map(|h| Some(h) == entry.post_hash)
                .unwrap_or(false),
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
        return persist_recovery_conflict(mgr, &intent.revision, workspace_root, "corrupt_entries").await;
    }

    let row = match load_recovery_session(mgr, intent).await {
        Ok(r) => r,
        Err(e) => {
            if matches!(e, SessionError::RecoveryConflict(_)) {
                let _ = persist_recovery_conflict(mgr, &intent.revision, workspace_root, "claim_mismatch").await;
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
        scope.cleanup_entry(
            &entry.path,
            &entry.stage_basename,
            entry.backup_basename.as_deref(),
        );
    }

    db::finalize_rolled_back_intent(
        mgr.pool().as_ref(),
        &intent.revision,
        &intent.session_id,
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;
    Ok(())
}

pub fn validate_manifest(changes: &[WorkspaceChangeEntry]) -> Result<(), SessionError> {
    if changes.is_empty() {
        return Err(SessionError::ManifestInvalid("changes must not be empty".into()));
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
                || change.path.starts_with(&format!("{}/", other))
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
    let existing = db::get_committed_intent_by_digest(
        mgr.pool().as_ref(),
        &session_id.to_string(),
        digest,
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;
    Ok(existing.map(|row| CommitOutcome {
        revision: row.revision,
        committed: true,
    }))
}

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

    validate_manifest(changes)?;
    let digest = request_digest(&session_id.to_string(), changes);

    if let Some(committed_digest) = db::get_committed_request_digest(
        mgr.pool().as_ref(),
        &session_id.to_string(),
    )
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

    test_owner_gate_admitted().await;

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
            require_parent_exists(&target).map_err(|e| SessionError::ManifestInvalid(e.to_string()))?;
            let stage_basename = format!(".nexus-stage-{}-{}", rev_tag, idx);
            let backup_basename = if scope.target_exists(&change.path).unwrap_or(false) {
                Some(format!(".nexus-backup-{}-{}", rev_tag, idx))
            } else {
                None
            };

            let (pre_hash, post_hash, mode) = match change.op {
                WorkspaceChangeOp::Create => {
                    let bytes = decode_base64(change.content_base64.as_deref().unwrap_or(""))
                        .map_err(SessionError::ManifestInvalid)?;
                    scope
                        .write_stage(&change.path, &stage_basename, &bytes, Some(CREATE_FILE_MODE))
                        .map_err(|e| SessionError::Io(e.to_string()))?;
                    (None, Some(hash_bytes(&bytes)), Some(CREATE_FILE_MODE))
                }
                WorkspaceChangeOp::Modify => {
                    let bytes = decode_base64(change.content_base64.as_deref().unwrap_or(""))
                        .map_err(SessionError::ManifestInvalid)?;
                    let captured_mode = if let Some(ref backup_name) = backup_basename {
                        let mode = scope
                            .backup_target(&change.path, backup_name)
                            .map_err(|e| SessionError::Io(e.to_string()))?;
                        mode
                    } else {
                        None
                    };
                    scope
                        .write_stage(&change.path, &stage_basename, &bytes, captured_mode)
                        .map_err(|e| SessionError::Io(e.to_string()))?;
                    (change.expected_hash.clone(), Some(hash_bytes(&bytes)), captured_mode)
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

    cleanup_staged(&scope, &staged);

    let finalized = db::finalize_committed_intent(
        mgr.pool().as_ref(),
        &revision,
        &session_id.to_string(),
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;
    if !finalized {
        return Err(SessionError::AlreadyCommitted(session_id.clone()));
    }

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
pub async fn commit_recoverable_owned(
    mgr: Arc<WorkspaceSessionManager>,
    session_id: SessionId,
    changes: Vec<WorkspaceChangeEntry>,
) -> Result<CommitOutcome, SessionError> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let mgr2 = Arc::clone(&mgr);
    let sid = session_id.clone();
    let owner = tokio::spawn(async move {
        set_test_crash_point(None);
        let result = commit_recoverable(&mgr2, &sid, &changes).await;
        test_owner_gate_settled();
        let _ = tx.send(result);
    });
    mgr.register_commit_owner(owner).await;
    match rx.await {
        Ok(result) => result,
        Err(_) => Err(SessionError::Internal("commit owner channel closed".into())),
    }
}

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
            return Err(SessionError::RecoveryConflict(intent.workspace_root.clone()));
        }
        if matches!(intent.state, db::IntentState::Applying | db::IntentState::RollingBack) {
            recover_unsettled_locked(mgr, &intent.workspace_root).await?;
        }
    }
    Ok(())
}
