//! Shared World KB handler guards (V1.155 P2 T3, R-V1152P0-002).
//!
//! Active-creator resolution and core principal resolution for the
//! World-scoped KB routes. World ownership/existence is enforced by the core
//! typed guard (`nexus_core` `world_kb::guards`); the daemon renders the
//! retained 403/404 envelopes from the core denial — the daemon runs no
//! ownership SQL of its own.

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;

/// Read the active creator id or return `AuthRequired`.
pub(crate) fn require_creator(state: &WorkspaceState) -> Result<String, NexusApiError> {
    // ONE provenance authority: `WorkspaceState::verified_creator_context`
    // requires an active creator AND a valid active workspace selection — the
    // same predicate the agent-host probe owner reuses, so admission and probe
    // context can never disagree.
    state
        .verified_creator_context()
        .map(|(creator_id, _workspace_slug)| creator_id)
        .ok_or(NexusApiError::AuthRequired)
}

/// Resolve core service + stored principal for thin HTTP adapters.
pub(crate) async fn resolve_core_principal(
    state: &WorkspaceState,
) -> Result<
    (
        std::sync::Arc<nexus_core::CoreService>,
        nexus_core::Principal,
    ),
    NexusApiError,
> {
    let core = state.core_or_uninit().await?;
    let principal = core.active_principal().await.map_err(NexusApiError::from)?;
    Ok((core, principal))
}
