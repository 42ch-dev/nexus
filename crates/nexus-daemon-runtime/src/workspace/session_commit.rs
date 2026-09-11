//! Recoverable multi-file workspace commit (v1.188 P3).

use std::path::{Path, PathBuf};

use nexus_contracts::local::orchestration::{WorkspaceChangeEntry, WorkspaceChangeOp};
use nexus_local_db as db;
use sha2::{Digest, Sha256};

use super::commit_fs::{
    atomic_create, atomic_delete, atomic_replace, backup_file, cleanup_temp, decode_base64,
    hash_bytes, restore_preimage, write_stage_file, MAX_CHANGES, MAX_FILE_BYTES, MAX_TOTAL_BYTES,
};
use super::session::{
    canonicalize_workspace_root, enforce_path_boundary, SessionError, SessionId,
    WorkspaceSessionManager,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitOutcome {
    pub revision: String,
    pub committed: bool,
}

pub async fn recover_unsettled(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
) -> Result<(), SessionError> {
    let intents = db::list_unsettled_intents(mgr.pool().as_ref(), workspace_root)
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?;
    for intent in intents {
        match intent.state {
            db::IntentState::RecoveryConflict => {
                return Err(SessionError::RecoveryConflict(workspace_root.to_string()));
            }
            db::IntentState::Applying | db::IntentState::RollingBack => {
                rollback_intent(mgr, workspace_root, &intent).await?;
            }
            _ => {}
        }
    }
    Ok(())
}

async fn rollback_intent(
    mgr: &WorkspaceSessionManager,
    workspace_root: &str,
    intent: &db::CommitIntentRow,
) -> Result<(), SessionError> {
    let canonical_root = canonicalize_workspace_root(Path::new(workspace_root)).await?;
    let _guard = mgr.lock_mutation().await;
    db::update_intent_state(
        mgr.pool().as_ref(),
        &intent.revision,
        db::IntentState::RollingBack,
        None,
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;

    let mut temps = Vec::new();
    for entry in &intent.entries {
        let target = canonical_root.join(&entry.path);
        let backup = entry
            .backup_basename
            .as_ref()
            .map(|b| target.with_file_name(b));
        if let Some(ref backup_path) = backup {
            temps.push(backup_path.clone());
        }
        temps.push(target.with_file_name(&entry.stage_basename));
        restore_preimage(&target, backup.as_deref())
            .map_err(|e| SessionError::Io(e.to_string()))?;
    }
    cleanup_temp(&temps);
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
    if changes.len() > MAX_CHANGES {
        return Err(SessionError::ManifestInvalid(format!(
            "too many changes (max {MAX_CHANGES})"
        )));
    }
    let mut total = 0usize;
    let mut paths: Vec<String> = Vec::new();
    for change in changes {
        if change.path.is_empty() || change.path.contains("..") {
            return Err(SessionError::ManifestInvalid(format!(
                "invalid path: {}",
                change.path
            )));
        }
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
                if expected.is_empty() {
                    return Err(SessionError::ManifestInvalid(
                        "expectedHash must be non-empty".into(),
                    ));
                }
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
                if change.expected_hash.as_deref().is_none_or(|h| h.is_empty()) {
                    return Err(SessionError::ManifestInvalid(
                        "delete requires expectedHash".into(),
                    ));
                }
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

pub async fn commit_recoverable(
    mgr: &WorkspaceSessionManager,
    session_id: &SessionId,
    changes: &[WorkspaceChangeEntry],
    workspace_root: &str,
) -> Result<CommitOutcome, SessionError> {
    if mgr.recoverable_config().is_none() {
        return Err(SessionError::Database("recoverable commit not configured".into()));
    }

    recover_unsettled(mgr, workspace_root).await?;
    validate_manifest(changes)?;

    let digest = request_digest(&session_id.to_string(), changes);
    if let Some(existing) = db::get_committed_intent_by_digest(
        mgr.pool().as_ref(),
        &session_id.to_string(),
        &digest,
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?
    {
        return Ok(CommitOutcome {
            revision: existing.revision,
            committed: true,
        });
    }

    if changes.is_empty() {
        mgr.validate_session(session_id).await?;
    } else {
        mgr.validate_contract_manifest(session_id, changes, workspace_root)
            .await?;
    }

    let revision = new_revision();
    let canonical_root = canonicalize_workspace_root(Path::new(workspace_root)).await?;
    let _guard = mgr.lock_mutation().await;

    let claim = db::claim_session_and_insert_intent(
        mgr.pool().as_ref(),
        &session_id.to_string(),
        &revision,
        workspace_root,
        &digest,
        "[]",
    )
    .await
    .map_err(|e| SessionError::Database(e.to_string()))?;

    match claim {
        db::ClaimSessionResult::Claimed => {}
        db::ClaimSessionResult::DigestConflict => {
            if let Some(existing) = db::get_committed_intent_by_digest(
                mgr.pool().as_ref(),
                &session_id.to_string(),
                &digest,
            )
            .await
            .map_err(|e| SessionError::Database(e.to_string()))?
            {
                return Ok(CommitOutcome {
                    revision: existing.revision,
                    committed: true,
                });
            }
            return Err(SessionError::ManifestInvalid("digest conflict".into()));
        }
        db::ClaimSessionResult::AlreadyClaimed { revision: other } => {
            return Err(SessionError::AlreadyCommitted(SessionId(other)));
        }
        db::ClaimSessionResult::NotFound => return Err(SessionError::NotFound(session_id.clone())),
        db::ClaimSessionResult::AlreadyConsumed => {
            return Err(SessionError::AlreadyCommitted(session_id.clone()));
        }
        db::ClaimSessionResult::Expired => return Err(SessionError::Expired(session_id.clone())),
    }

    let mut entries_json: Vec<db::IntentEntryJson> = Vec::new();
    let mut stage_paths: Vec<PathBuf> = Vec::new();
    let mut backup_paths: Vec<PathBuf> = Vec::new();
    let rev_tag = revision.replace("rev_", "");

    for (idx, change) in changes.iter().enumerate() {
        let target = canonical_root.join(&change.path);
        enforce_path_boundary(&target, &canonical_root)?;
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| SessionError::Io(e.to_string()))?;
        }
        let stage_basename = format!(".nexus-stage-{}-{}", rev_tag, idx);
        let stage = target.with_file_name(&stage_basename);
        let backup_basename = if target.exists() {
            Some(format!(".nexus-backup-{}-{}", rev_tag, idx))
        } else {
            None
        };
        let backup = backup_basename
            .as_ref()
            .map(|b| target.with_file_name(b));

        let (pre_hash, post_hash) = match change.op {
            WorkspaceChangeOp::Create => {
                let bytes = decode_base64(change.content_base64.as_deref().unwrap_or(""))
                    .map_err(SessionError::ManifestInvalid)?;
                write_stage_file(&stage, &bytes).map_err(|e| SessionError::Io(e.to_string()))?;
                stage_paths.push(stage.clone());
                (None, Some(hash_bytes(&bytes)))
            }
            WorkspaceChangeOp::Modify => {
                let bytes = decode_base64(change.content_base64.as_deref().unwrap_or(""))
                    .map_err(SessionError::ManifestInvalid)?;
                if let Some(ref backup_path) = backup {
                    backup_file(&target, backup_path)
                        .map_err(|e| SessionError::Io(e.to_string()))?;
                    backup_paths.push(backup_path.clone());
                }
                write_stage_file(&stage, &bytes).map_err(|e| SessionError::Io(e.to_string()))?;
                stage_paths.push(stage.clone());
                (change.expected_hash.clone(), Some(hash_bytes(&bytes)))
            }
            WorkspaceChangeOp::Delete => {
                if let Some(ref backup_path) = backup {
                    backup_file(&target, backup_path)
                        .map_err(|e| SessionError::Io(e.to_string()))?;
                    backup_paths.push(backup_path.clone());
                }
                (change.expected_hash.clone(), None)
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
            mode: None,
        });
    }

    let entries_serialized =
        serde_json::to_string(&entries_json).map_err(|e| SessionError::Database(e.to_string()))?;
    sqlx::query("UPDATE workspace_commit_intents SET entries_json = ? WHERE revision = ?")
        .bind(&entries_serialized)
        .bind(&revision)
        .execute(mgr.pool().as_ref())
        .await
        .map_err(|e| SessionError::Database(e.to_string()))?;

    for (idx, change) in changes.iter().enumerate() {
        let target = canonical_root.join(&change.path);
        let stage = target.with_file_name(&entries_json[idx].stage_basename);
        let result = match change.op {
            WorkspaceChangeOp::Create => atomic_create(&target, &stage),
            WorkspaceChangeOp::Modify => atomic_replace(&target, &stage),
            WorkspaceChangeOp::Delete => atomic_delete(&target),
        };
        if let Err(e) = result {
            let intent = db::CommitIntentRow {
                session_id: session_id.to_string(),
                workspace_root: workspace_root.to_string(),
                revision: revision.clone(),
                request_digest: digest.clone(),
                state: db::IntentState::Applying,
                entries: entries_json.clone(),
                error_category: None,
            };
            let _ = db::update_intent_state(
                mgr.pool().as_ref(),
                &revision,
                db::IntentState::RollingBack,
                Some("apply_failed"),
            )
            .await;
            let _ = rollback_intent(mgr, workspace_root, &intent).await;
            cleanup_temp(&stage_paths);
            cleanup_temp(&backup_paths);
            return Err(SessionError::Io(e.to_string()));
        }
    }

    cleanup_temp(&stage_paths);
    cleanup_temp(&backup_paths);

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
