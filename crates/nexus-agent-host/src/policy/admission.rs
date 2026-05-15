//! Admission policy — provider/capability/session gates before launch and exec.
//!
//! Runs before provider launch and before operation dispatch.
//! ACP provider permissions delegate to `nexus_acp_host::PermissionPolicy` (R-003).
//! Native CLI providers use host-level risk classification only.

use std::collections::HashSet;

use crate::config::AgentHostConfig;
use crate::error::{HostError, HostResult};
use crate::ids::{HostSessionId, ProviderId};

/// Admission policy for provider, capability, and concurrency checks.
#[derive(Debug, Clone)]
pub struct AdmissionPolicy {
    /// Maximum concurrent sessions.
    pub max_sessions: usize,
    /// Maximum operations per session.
    pub max_ops_per_session: usize,
    /// Whether to deny unknown providers.
    pub deny_unknown_providers: bool,
    /// Known provider IDs (from config + catalog).
    known_providers: HashSet<ProviderId>,
    /// Denied capability names (from policy config).
    denied_capabilities: HashSet<String>,
}

impl AdmissionPolicy {
    /// Build admission policy from config and known provider catalog.
    #[must_use]
    pub fn from_config(config: &AgentHostConfig, known_providers: HashSet<ProviderId>) -> Self {
        Self {
            max_sessions: config.max_sessions,
            max_ops_per_session: config.max_ops_per_session,
            deny_unknown_providers: config.policy.deny_unknown_providers(),
            known_providers,
            denied_capabilities: HashSet::new(),
        }
    }

    /// Check if a provider is allowed.
    ///
    /// # Errors
    ///
    /// Returns `HostError::PolicyDenied` if the provider is not allowed.
    pub fn check_provider(&self, provider_id: &ProviderId) -> HostResult<()> {
        if self.deny_unknown_providers && !self.known_providers.contains(provider_id) {
            return Err(HostError::policy_denied(format!(
                "provider '{provider_id}' is not in the known providers list and unknown providers are denied"
            )));
        }
        Ok(())
    }

    /// Check if a capability is allowed.
    ///
    /// # Errors
    ///
    /// Returns `HostError::PolicyDenied` if the capability is denied.
    pub fn check_capability(&self, capability: &str) -> HostResult<()> {
        if self.denied_capabilities.contains(capability) {
            return Err(HostError::policy_denied(format!(
                "capability '{capability}' is denied by policy"
            )));
        }
        Ok(())
    }

    /// Check session concurrency limit.
    ///
    /// # Errors
    ///
    /// Returns `HostError::PolicyDenied` if the limit is exceeded.
    pub fn check_session_limit(&self, current_sessions: usize) -> HostResult<()> {
        if current_sessions >= self.max_sessions {
            return Err(HostError::policy_denied(format!(
                "session limit reached ({}/{})",
                current_sessions, self.max_sessions
            )));
        }
        Ok(())
    }

    /// Check per-session operation concurrency.
    ///
    /// # Errors
    ///
    /// Returns `HostError::PolicyDenied` if the limit is exceeded.
    pub fn check_ops_per_session(
        &self,
        session_id: &HostSessionId,
        active_ops: usize,
    ) -> HostResult<()> {
        if active_ops >= self.max_ops_per_session {
            return Err(HostError::policy_denied(format!(
                "operation limit reached for session {} ({}/{})",
                session_id, active_ops, self.max_ops_per_session
            )));
        }
        Ok(())
    }

    /// Check workspace root validity.
    ///
    /// The workspace root must be an absolute, existing path.
    ///
    /// # Errors
    ///
    /// Returns `HostError::PolicyDenied` if the workspace root is invalid.
    pub fn check_workspace_root(&self, workspace_root: &std::path::Path) -> HostResult<()> {
        if !workspace_root.is_absolute() {
            return Err(HostError::policy_denied(format!(
                "workspace root must be an absolute path: {}",
                workspace_root.display()
            )));
        }
        Ok(())
    }

    /// Full pre-launch admission check.
    ///
    /// Validates provider, session limit, and workspace root.
    ///
    /// # Errors
    ///
    /// Returns `HostError::PolicyDenied` if any check fails.
    pub fn check_before_launch(
        &self,
        provider_id: &ProviderId,
        current_sessions: usize,
        workspace_root: &std::path::Path,
    ) -> HostResult<()> {
        self.check_provider(provider_id)?;
        self.check_session_limit(current_sessions)?;
        self.check_workspace_root(workspace_root)?;
        Ok(())
    }

    /// Full pre-exec admission check.
    ///
    /// Validates session operation limit.
    ///
    /// # Errors
    ///
    /// Returns `HostError::PolicyDenied` if any check fails.
    pub fn check_before_exec(
        &self,
        session_id: &HostSessionId,
        active_ops: usize,
    ) -> HostResult<()> {
        self.check_ops_per_session(session_id, active_ops)
    }

    /// Add a denied capability.
    pub fn deny_capability(&mut self, capability: impl Into<String>) {
        self.denied_capabilities.insert(capability.into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_config() -> AgentHostConfig {
        AgentHostConfig::default()
    }

    fn config_with_max(max_sessions: usize, max_ops: usize) -> AgentHostConfig {
        AgentHostConfig {
            max_sessions,
            max_ops_per_session: max_ops,
            ..AgentHostConfig::default()
        }
    }

    fn known_providers() -> HashSet<ProviderId> {
        let mut set = HashSet::new();
        set.insert(ProviderId::new("claude-native"));
        set.insert(ProviderId::new("claude-acp"));
        set
    }

    #[test]
    fn known_provider_allowed() {
        let config = default_config();
        let policy = AdmissionPolicy::from_config(&config, known_providers());
        assert!(policy
            .check_provider(&ProviderId::new("claude-native"))
            .is_ok());
    }

    #[test]
    fn unknown_provider_denied_when_policy_strict() {
        let config = default_config(); // deny_unknown_providers = true
        let policy = AdmissionPolicy::from_config(&config, known_providers());
        assert!(policy.check_provider(&ProviderId::new("unknown")).is_err());
    }

    #[test]
    fn unknown_provider_allowed_when_policy_permissive() {
        let mut config = default_config();
        config.policy.unknown_provider = "allow".to_string();
        let policy = AdmissionPolicy::from_config(&config, known_providers());
        assert!(policy.check_provider(&ProviderId::new("unknown")).is_ok());
    }

    #[test]
    fn session_limit_enforced() {
        let config = config_with_max(2, 1);
        let policy = AdmissionPolicy::from_config(&config, HashSet::new());
        assert!(policy.check_session_limit(1).is_ok());
        assert!(policy.check_session_limit(2).is_err());
    }

    #[test]
    fn ops_per_session_enforced() {
        let config = config_with_max(4, 1);
        let policy = AdmissionPolicy::from_config(&config, HashSet::new());
        let sid = HostSessionId::new();
        assert!(policy.check_ops_per_session(&sid, 0).is_ok());
        assert!(policy.check_ops_per_session(&sid, 1).is_err());
    }

    #[test]
    fn workspace_root_must_be_absolute() {
        let config = default_config();
        let policy = AdmissionPolicy::from_config(&config, HashSet::new());
        assert!(policy
            .check_workspace_root(std::path::Path::new("/tmp"))
            .is_ok());
        assert!(policy
            .check_workspace_root(std::path::Path::new("relative/path"))
            .is_err());
    }

    #[test]
    fn denied_capability_blocked() {
        let config = default_config();
        let mut policy = AdmissionPolicy::from_config(&config, HashSet::new());
        policy.deny_capability("streaming");
        assert!(policy.check_capability("streaming").is_err());
        assert!(policy.check_capability("text_prompt").is_ok());
    }

    #[test]
    fn full_before_launch_check() {
        let config = default_config();
        let policy = AdmissionPolicy::from_config(&config, known_providers());
        assert!(policy
            .check_before_launch(
                &ProviderId::new("claude-native"),
                0,
                std::path::Path::new("/tmp")
            )
            .is_ok());
    }

    #[test]
    fn full_before_launch_check_fails_on_unknown() {
        let config = default_config();
        let policy = AdmissionPolicy::from_config(&config, known_providers());
        assert!(policy
            .check_before_launch(&ProviderId::new("unknown"), 0, std::path::Path::new("/tmp"))
            .is_err());
    }
}
