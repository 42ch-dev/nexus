//! Durable workspace commit authority (v1.190 P3-T2).
//!
//! The workspace session/OCC/intent/recovery protocol moved here from the
//! daemon as ONE module set. It keeps its authority root, digest idempotence,
//! the retained commit owner and the descriptor-relative atomic file apply —
//! nothing about the durable commit contract changed in the move; only its
//! home did.
//!
//! Module map:
//!
//! - [`crate::execution::session`] — the DB-backed
//!   [`WorkspaceSessionManager`](crate::execution::session::WorkspaceSessionManager):
//!   sessions, snapshots, OCC hashing and the recoverable-commit configuration.
//! - [`crate::execution::session_commit`] — the recoverable multi-file commit,
//!   intent/digest idempotence and the crash-recovery classification.
//! - [`crate::execution::commit_fs`] — descriptor-relative atomic mutation
//!   (`openat`/`renameat`/`linkat`/`unlinkat`) with capture-and-verify CAS.
//! - [`crate::execution::authority`] — the cross-process workspace authority
//!   lease guarding the commit root.
//! - [`crate::execution::bounds`] — manifest/body/hash/path bounds.
//! - [`crate::execution::scope`] — scope-coordinate resolution.
//! - [`crate::execution::executor`] — the orchestration `WorkspaceExecutor`
//!   adapter over the shared manager.
//! - [`crate::execution::state_provider`] — `_context.workspace.*` resolution.
//!
//! Type gaps are REPORTED, never filled with a hand-written parallel shape:
//! the wire DTOs for the commit request/response are the P5-owned generated
//! `CoreWorkspaceCommitRequest`/`CoreWorkspaceCommitResponse` pair.

use std::sync::Arc;

use nexus_contracts::local::orchestration::{WorkspaceChangeEntry, WorkspaceChangeOp};
use nexus_contracts::{
    CoreWorkspaceCommitRequest, CoreWorkspaceCommitRequestChangesItem,
    CoreWorkspaceCommitRequestChangesItemOp, CoreWorkspaceCommitResponse,
    CoreWorkspaceCommitResponseRevision,
};

use crate::error::{CoreError, CoreResult};
use crate::execution::session::{SessionError, SessionId, WorkspaceSessionManager};
use crate::execution::session_commit::CommitOutcome;

/// The workspace commit authority one handle owns: the shared session manager
/// plus the active canonical root it was opened against.
///
/// A commit is only valid against the root this authority was opened with, so
/// binding the two together prevents a handle from committing through a root
/// its manager was not admitted for.
#[derive(Clone)]
pub struct WorkspaceCommitAuthority {
    manager: Arc<WorkspaceSessionManager>,
    active_root: String,
}

impl WorkspaceCommitAuthority {
    /// Bind a session manager to the canonical root it commits against.
    #[must_use]
    pub const fn new(manager: Arc<WorkspaceSessionManager>, active_root: String) -> Self {
        Self {
            manager,
            active_root,
        }
    }

    /// The shared session manager (also used by the executor adapter).
    #[must_use]
    pub const fn manager(&self) -> &Arc<WorkspaceSessionManager> {
        &self.manager
    }

    /// The canonical root every commit through this authority is scoped to.
    #[must_use]
    pub fn active_root(&self) -> &str {
        &self.active_root
    }

    /// Commit through this authority's manager and root.
    ///
    /// # Errors
    /// Returns the mapped commit refusal (see [`commit_workspace`]).
    pub async fn commit(
        &self,
        request: CoreWorkspaceCommitRequest,
    ) -> CoreResult<CoreWorkspaceCommitResponse> {
        commit_workspace(&self.manager, request, &self.active_root).await
    }
}

/// Commit a validated change manifest through the durable authority.
///
/// The commit runs on the RETAINED owner: if the awaiting caller is dropped
/// (client disconnect, shutdown), the already-admitted commit still runs to a
/// durable conclusion rather than being abandoned mid-apply. The returned
/// outcome is the owner's real outcome.
///
/// `active_workspace_root` is the canonical root this authority was opened
/// against; the session manager re-checks the session's persisted root
/// against it, so a session from a foreign root cannot commit here.
///
/// # Errors
/// Returns the mapped commit refusal: [`CoreError::Busy`] for a hash/OCC
/// mismatch, an expired/consumed session, an unsettled recovery intent or a
/// foreign workspace root; `InvalidInput` for a manifest/bounds violation; and
/// `Internal` for a storage fault.
pub async fn commit_workspace(
    manager: &Arc<WorkspaceSessionManager>,
    request: CoreWorkspaceCommitRequest,
    active_workspace_root: &str,
) -> CoreResult<CoreWorkspaceCommitResponse> {
    let session_id = SessionId(String::from(request.session_id));
    let changes = request
        .changes
        .into_iter()
        .map(changes_item_to_entry)
        .collect::<Vec<_>>();
    let outcome: CommitOutcome = WorkspaceSessionManager::commit_session_durable_owned(
        Arc::clone(manager),
        session_id,
        changes,
        active_workspace_root.to_string(),
    )
    .await
    .map_err(map_commit_error)?;
    Ok(CoreWorkspaceCommitResponse {
        revision: CoreWorkspaceCommitResponseRevision::try_from(outcome.revision).map_err(
            |err| CoreError::Internal {
                category: format!("commit revision encode: {err}"),
            },
        )?,
        committed: outcome.committed,
    })
}

/// Map a session-layer refusal onto the neutral core taxonomy.
///
/// The taxonomy is schema-derived (`schemas/core/core-error.schema.json`,
/// P5-owned) and carries NO dedicated workspace-commit conflict code, so the
/// OCC/session refusals — which the daemon's retained HTTP path renders as
/// HTTP 409 with full detail by calling the session layer directly — collapse
/// onto [`CoreError::Busy`], the neutral retryable-conflict arm. A malformed
/// manifest or an escaping path is a validation refusal
/// ([`CoreError::InvalidInput`]); a storage fault is `Internal`. The typed
/// entry point therefore never reports a client conflict as a 500.
///
/// The lost conflict *detail* on this new path is a known schema gap (see the
/// task report): expressing it would require a workspace-commit conflict code
/// in `core-error.schema.json`, which only the P5 schema window may add. No
/// parallel handwritten error shape is invented here.
fn map_commit_error(err: SessionError) -> CoreError {
    match err {
        SessionError::NotFound(id) => CoreError::NotFound {
            resource: format!("workspace session {id}"),
        },
        // Conflicts: stale/expired session, OCC hash mismatch, unsettled
        // recovery intent, foreign workspace root.
        SessionError::AlreadyCommitted(_)
        | SessionError::Expired(_)
        | SessionError::HashConflict { .. }
        | SessionError::RecoveryConflict(_)
        | SessionError::ActiveWorkspaceMismatch { .. } => CoreError::Busy,
        SessionError::ManifestInvalid(msg) => CoreError::InvalidInput {
            field: "changes".into(),
            reason: msg,
        },
        SessionError::PathEscape { path, .. } => CoreError::InvalidInput {
            field: "path".into(),
            reason: format!("path not allowed: {path}"),
        },
        SessionError::CorruptSnapshot(msg) => CoreError::Internal {
            category: format!("corrupt workspace snapshot: {msg}"),
        },
        SessionError::Database(msg) | SessionError::Io(msg) | SessionError::Internal(msg) => {
            CoreError::Internal { category: msg }
        }
    }
}

/// One generated manifest entry -> the durable commit manifest entry.
///
/// The generated `contentBase64`/`expectedHash` field names and the local
/// `content_base64`/`expected_hash` fields carry identical wire semantics, and
/// the op enum is the same three-way `create|modify|delete`. This is the single
/// conversion seam, so the commit authority keeps naming ONE manifest type.
///
/// It is a FREE FUNCTION rather than a `From` impl because both sides are
/// external types (both are owned by `nexus-contracts`), and the orphan rule
/// forbids a foreign-trait foreign-type impl here.
fn changes_item_to_entry(item: CoreWorkspaceCommitRequestChangesItem) -> WorkspaceChangeEntry {
    let op = match item.op {
        CoreWorkspaceCommitRequestChangesItemOp::Create => WorkspaceChangeOp::Create,
        CoreWorkspaceCommitRequestChangesItemOp::Modify => WorkspaceChangeOp::Modify,
        CoreWorkspaceCommitRequestChangesItemOp::Delete => WorkspaceChangeOp::Delete,
    };
    WorkspaceChangeEntry {
        path: String::from(item.path),
        op,
        expected_hash: item.expected_hash,
        content_base64: item.content_base64,
    }
}
