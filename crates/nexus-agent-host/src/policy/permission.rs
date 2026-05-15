//! Permission resolution for ACP and native providers.
//!
//! For ACP providers, delegates to `nexus_acp_host::PermissionPolicy::evaluate_for_agent()`.
//! For native CLI providers, uses host-level risk classification only.
//!
//! Preference order when allowed: session-scoped approval > `allow_always` > `allow_once` > deny.

use crate::capability::model::ProtocolKind;
use crate::capability::risk::ToolRisk;
use crate::config::PolicyConfig;

/// Permission outcome from host-level evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionOutcome {
    /// Allow the operation.
    Allow,
    /// Deny the operation.
    Deny,
    /// Ask the user interactively.
    Ask,
}

impl From<nexus_acp_host::policy::PermissionDecision> for PermissionOutcome {
    fn from(decision: nexus_acp_host::policy::PermissionDecision) -> Self {
        match decision {
            nexus_acp_host::policy::PermissionDecision::Grant => Self::Allow,
            nexus_acp_host::policy::PermissionDecision::Deny => Self::Deny,
            nexus_acp_host::policy::PermissionDecision::Ask => Self::Ask,
        }
    }
}

/// Host-level permission resolver.
///
/// Wraps `nexus_acp_host::PermissionPolicy` for ACP providers and uses local
/// risk classification for native CLI providers.
#[derive(Debug, Clone)]
pub struct HostPermissionResolver {
    /// ACP permission policy (loaded from `.nexus42/permissions.toml`).
    acp_policy: Option<nexus_acp_host::policy::PermissionPolicy>,
    /// Unknown tool risk policy from config.
    unknown_tool_risk: String,
}

impl HostPermissionResolver {
    /// Create a resolver without an ACP policy (native-only mode).
    #[must_use]
    pub fn new_native_only(config: &PolicyConfig) -> Self {
        Self {
            acp_policy: None,
            unknown_tool_risk: config.unknown_tool_risk.clone(),
        }
    }

    /// Create a resolver with an ACP policy loaded from workspace.
    #[must_use]
    pub fn with_acp_policy(
        config: &PolicyConfig,
        acp_policy: nexus_acp_host::policy::PermissionPolicy,
    ) -> Self {
        Self {
            acp_policy: Some(acp_policy),
            unknown_tool_risk: config.unknown_tool_risk.clone(),
        }
    }

    /// Resolve permission for a tool/capability operation.
    ///
    /// - For ACP providers: delegates to `PermissionPolicy::evaluate_for_agent()`.
    /// - For native CLI providers: uses local risk classification.
    #[must_use]
    pub fn resolve(
        &self,
        protocol_kind: ProtocolKind,
        provider_id: &str,
        capability: &str,
        tool_risk: Option<ToolRisk>,
    ) -> PermissionOutcome {
        match protocol_kind {
            ProtocolKind::Acp => self.resolve_acp(provider_id, capability),
            ProtocolKind::NativeCli => self.resolve_native(tool_risk),
        }
    }

    /// Resolve for ACP provider — delegates to `PermissionPolicy`.
    fn resolve_acp(&self, agent_id: &str, capability: &str) -> PermissionOutcome {
        self.acp_policy
            .as_ref()
            .map_or(PermissionOutcome::Deny, |policy| {
                policy.evaluate_for_agent(agent_id, capability).into()
            })
    }

    /// Resolve for native CLI provider — uses local risk classification.
    ///
    /// - Read tools: Allow
    /// - Write tools: Ask
    /// - Destructive tools: Deny
    /// - Unknown tools: follows `unknown_tool_risk` config
    fn resolve_native(&self, tool_risk: Option<ToolRisk>) -> PermissionOutcome {
        match tool_risk {
            Some(ToolRisk::Read) => PermissionOutcome::Allow,
            Some(ToolRisk::Write) => PermissionOutcome::Ask,
            Some(ToolRisk::Destructive) => PermissionOutcome::Deny,
            None => match self.unknown_tool_risk.as_str() {
                "allow" => PermissionOutcome::Allow,
                "ask" => PermissionOutcome::Ask,
                _ => PermissionOutcome::Deny,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_policy_config() -> PolicyConfig {
        PolicyConfig::default()
    }

    #[test]
    fn native_read_is_allowed() {
        let config = default_policy_config();
        let resolver = HostPermissionResolver::new_native_only(&config);
        assert_eq!(
            resolver.resolve(
                ProtocolKind::NativeCli,
                "claude-native",
                "file_read",
                Some(ToolRisk::Read)
            ),
            PermissionOutcome::Allow
        );
    }

    #[test]
    fn native_write_asks() {
        let config = default_policy_config();
        let resolver = HostPermissionResolver::new_native_only(&config);
        assert_eq!(
            resolver.resolve(
                ProtocolKind::NativeCli,
                "claude-native",
                "file_write",
                Some(ToolRisk::Write)
            ),
            PermissionOutcome::Ask
        );
    }

    #[test]
    fn native_destructive_denied() {
        let config = default_policy_config();
        let resolver = HostPermissionResolver::new_native_only(&config);
        assert_eq!(
            resolver.resolve(
                ProtocolKind::NativeCli,
                "claude-native",
                "file_delete",
                Some(ToolRisk::Destructive)
            ),
            PermissionOutcome::Deny
        );
    }

    #[test]
    fn native_unknown_default_deny() {
        let config = default_policy_config();
        let resolver = HostPermissionResolver::new_native_only(&config);
        assert_eq!(
            resolver.resolve(ProtocolKind::NativeCli, "claude-native", "unknown", None),
            PermissionOutcome::Deny
        );
    }

    #[test]
    fn native_unknown_ask_config() {
        let config = PolicyConfig {
            unknown_tool_risk: "ask".to_string(),
            ..PolicyConfig::default()
        };
        let resolver = HostPermissionResolver::new_native_only(&config);
        assert_eq!(
            resolver.resolve(ProtocolKind::NativeCli, "claude-native", "unknown", None),
            PermissionOutcome::Ask
        );
    }

    #[test]
    fn acp_delegates_to_policy() {
        let config = default_policy_config();
        let mut acp_policy = nexus_acp_host::policy::PermissionPolicy::new();
        acp_policy.grant_agent("claude-acp", "terminal.create");

        let resolver = HostPermissionResolver::with_acp_policy(&config, acp_policy);
        assert_eq!(
            resolver.resolve(
                ProtocolKind::Acp,
                "claude-acp",
                "terminal.create",
                Some(ToolRisk::Write)
            ),
            PermissionOutcome::Allow
        );
    }

    #[test]
    fn acp_no_policy_defaults_deny() {
        let config = default_policy_config();
        let resolver = HostPermissionResolver::new_native_only(&config);
        assert_eq!(
            resolver.resolve(
                ProtocolKind::Acp,
                "claude-acp",
                "terminal.create",
                Some(ToolRisk::Write)
            ),
            PermissionOutcome::Deny
        );
    }

    #[test]
    fn acp_unknown_capability_defaults_to_ask() {
        let config = default_policy_config();
        let acp_policy = nexus_acp_host::policy::PermissionPolicy::new(); // default = ask
        let resolver = HostPermissionResolver::with_acp_policy(&config, acp_policy);
        assert_eq!(
            resolver.resolve(
                ProtocolKind::Acp,
                "claude-acp",
                "unknown_tool",
                Some(ToolRisk::Read)
            ),
            PermissionOutcome::Ask
        );
    }

    #[test]
    fn permission_decision_conversion() {
        assert_eq!(
            PermissionOutcome::from(nexus_acp_host::policy::PermissionDecision::Grant),
            PermissionOutcome::Allow
        );
        assert_eq!(
            PermissionOutcome::from(nexus_acp_host::policy::PermissionDecision::Deny),
            PermissionOutcome::Deny
        );
        assert_eq!(
            PermissionOutcome::from(nexus_acp_host::policy::PermissionDecision::Ask),
            PermissionOutcome::Ask
        );
    }
}
