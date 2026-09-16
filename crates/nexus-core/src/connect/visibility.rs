//! Per-consumer MCP tool visibility policy (V1.180 P1, RN-OGA-2).
//!
//! The additive seam that lets an operator grant a consumer a `tools/list`
//! subset while `tools/call` still obeys the V1.174 capability-registry
//! spine. Evaluated at the shared MCP serving seam
//! ([`crate::connect::mcp_bridge::McpBridgeHandler`]) BEFORE serving
//! `tools/list` and BEFORE dispatching `tools/call`:
//!
//! - Absent policy ⇒ byte-identical current behavior (every catalog row
//!   visible, every call dispatched to the spine).
//! - Present policy ⇒ `tools/list` is filtered to the operator-configured
//!   subset; a hidden-tool `tools/call` short-circuits at the seam before
//!   `backend.call_tool()`.
//!
//! Visibility is NEVER an authorization grant: dispatch authz stays the
//! spine (Layer 3, AR-69/AR-68). This seam only narrows what a consumer
//! can SEE — the inverse direction of the existing `mcp_catalog_admission`
//! asymmetry (AR-70 §3 / AR-74), which hides tools from the catalog while
//! keeping them spine-dispatchable.
//!
//! Config surface: the daemon-local `PeerToolsConfig.mcp_visibility` list
//! (`~/.nexus42/connect/daemon.json`), read at the two construction sites
//! (Model A stdio child, Model B embedded server). No wire/schema changes.

use std::collections::HashSet;

/// Per-consumer MCP tool visibility policy.
///
/// `None` = absent policy (all visible — byte-identical current behavior);
/// `Some(set)` = the operator-configured visible subset.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VisibilityPolicy {
    visible: Option<HashSet<String>>,
}

impl VisibilityPolicy {
    /// Absent policy: every catalog row visible, every call dispatched to
    /// the spine — byte-identical to the pre-seam behavior.
    #[must_use]
    pub const fn absent() -> Self {
        Self { visible: None }
    }

    /// Present policy from the operator-configured visible subset.
    ///
    /// An empty subset is treated as absent (all visible) — the additive
    /// default keeps existing deployments byte-identical.
    #[must_use]
    pub fn from_visible(ids: impl IntoIterator<Item = String>) -> Self {
        let visible: HashSet<String> = ids.into_iter().collect();
        if visible.is_empty() {
            Self::absent()
        } else {
            Self {
                visible: Some(visible),
            }
        }
    }

    /// Whether a tool id is visible to the consumer.
    #[must_use]
    pub fn is_visible(&self, tool_id: &str) -> bool {
        self.visible
            .as_ref()
            .is_none_or(|visible| visible.contains(tool_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn absent_policy_visits_every_id() {
        let policy = VisibilityPolicy::absent();
        assert!(policy.is_visible("nexus.workspace.info"));
        assert!(policy.is_visible("tools.t5.echo"));
        assert!(policy.is_visible("t6.wcap"));
    }

    #[test]
    fn empty_visible_subset_is_absent() {
        // The additive default: an empty operator list must not flip the
        // surface to deny-all — it stays byte-identical (all visible).
        let policy = VisibilityPolicy::from_visible(Vec::<String>::new());
        assert_eq!(policy, VisibilityPolicy::absent());
        assert!(policy.is_visible("nexus.workspace.info"));
    }

    #[test]
    fn present_policy_visits_only_the_subset() {
        let policy = VisibilityPolicy::from_visible([
            "nexus.workspace.info".to_owned(),
            "tools.t5.echo".to_owned(),
        ]);
        assert!(policy.is_visible("nexus.workspace.info"));
        assert!(policy.is_visible("tools.t5.echo"));
        assert!(!policy.is_visible("tools.t6.echo"), "unlisted id hidden");
        assert!(!policy.is_visible("t6.wcap"), "unlisted id hidden");
    }
}
