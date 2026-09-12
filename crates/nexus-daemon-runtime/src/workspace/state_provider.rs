//! Live workspace state for preset expression evaluation (v1.188 P3).
//!
//! Preset states may branch on `_context.workspace.*`. The wiring here binds
//! that object to the SAME shared [`WorkspaceSessionManager`] the commit
//! executor writes through, so a conditional edge observes real durable
//! workspace state rather than a synthetic placeholder.

use std::sync::Arc;

use async_trait::async_trait;
use nexus_orchestration::capability::WorkspaceStateProvider;

use super::session::WorkspaceSessionManager;

/// Resolves workspace state from a daemon workspace session authority.
pub struct DaemonWorkspaceStateProvider {
    session_manager: Arc<WorkspaceSessionManager>,
    canonical_workspace_root: String,
}

impl DaemonWorkspaceStateProvider {
    /// Bind the provider to one shared manager and workspace root.
    #[must_use]
    pub fn new(
        session_manager: Arc<WorkspaceSessionManager>,
        canonical_workspace_root: String,
    ) -> Self {
        Self {
            session_manager,
            canonical_workspace_root,
        }
    }
}

#[async_trait]
impl WorkspaceStateProvider for DaemonWorkspaceStateProvider {
    async fn workspace_state(&self) -> Option<serde_json::Value> {
        let intent = nexus_local_db::latest_committed_intent_for_root(
            self.session_manager.pool().as_ref(),
            &self.canonical_workspace_root,
        )
        .await
        .map_err(|err| {
            tracing::warn!(
                error = %err,
                "workspace state lookup failed; no workspace context injected"
            );
        })
        .ok()
        .flatten()?;

        Some(serde_json::json!({
            "session_id": intent.session_id,
            "committed": true,
            "revision": intent.revision,
            "change_count": intent.entries.len(),
            "workspace_root": intent.workspace_root,
        }))
    }
}
