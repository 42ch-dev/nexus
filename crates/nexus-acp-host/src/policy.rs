//! Permission policy engine for ACP permission requests.
//!
//! This module implements V1.1's configurable permission policy system (ACP-R7),
//! replacing V1.0's auto-grant-all behavior with user-controlled grant/deny rules.
//!
//! V1.6 extends this with per-agent scoped rules via `nexus42 permission` CLI.
//!
//! # Architecture
//!
//! ```text
//! Agent requests permission
//!         │
//!         ▼
//! PermissionPolicy::evaluate()
//!         │
//!         ├─► Check agent-scoped rules (grant/deny/ask per agent)
//!         │
//!         ├─► Check explicit global rules (grant/deny mappings)
//!         │
//!         ├─► If no rule found, use default policy
//!         │       │
//!         │       ├─► Policy::Ask → Interactive prompt
//!         │       ├─► Policy::Grant → Auto-grant
//!         │       └─► Policy::Deny → Auto-deny
//!         │
//!         └─► Return decision
//! ```
//!
//! # Policy File Format
//!
//! Policies are stored in `.nexus42/permissions.toml`:
//!
//! ```toml
//! # Default policy for unknown permissions
//! default = "ask"  # "ask" | "grant" | "deny"
//!
//! # Explicit grant rules
//! [grant]
//! "file_system.read" = true
//! "file_system.write" = true
//!
//! # Explicit deny rules
//! [deny]
//! "terminal.kill" = true
//!
//! # Per-agent rules (V1.6)
//! [agents.my-agent.grant]
//! "terminal.create" = true
//!
//! [agents.my-agent.deny]
//! "terminal.kill" = true
//!
//! [agents.my-agent.ask]
//! "file_system.write" = true
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Permission decision returned by policy evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermissionDecision {
    /// Grant the permission request.
    Grant,
    /// Deny the permission request.
    Deny,
    /// Ask the user interactively.
    Ask,
}

/// Default policy when no explicit rule matches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum DefaultPolicy {
    /// Always ask the user for unknown permissions.
    #[default]
    Ask,
    /// Auto-grant unknown permissions.
    Grant,
    /// Auto-deny unknown permissions.
    Deny,
}

/// Per-agent permission rules (V1.6).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentRules {
    /// Permissions to always grant for this agent.
    #[serde(default)]
    pub grant: HashMap<String, bool>,

    /// Permissions to always deny for this agent.
    #[serde(default)]
    pub deny: HashMap<String, bool>,

    /// Permissions that prompt the user for this agent.
    #[serde(default)]
    pub ask: HashMap<String, bool>,
}

impl AgentRules {
    /// Create a new empty agent rules set.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check whether any rules are configured.
    pub fn is_empty(&self) -> bool {
        self.grant.is_empty() && self.deny.is_empty() && self.ask.is_empty()
    }
}

/// Permission policy configuration loaded from workspace.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionPolicy {
    /// Default policy for permissions without explicit rules.
    #[serde(default)]
    pub default: DefaultPolicy,

    /// Permissions to always grant (global).
    #[serde(default)]
    pub grant: HashMap<String, bool>,

    /// Permissions to always deny (global).
    #[serde(default)]
    pub deny: HashMap<String, bool>,

    /// Per-agent permission rules (V1.6).
    #[serde(default)]
    pub agents: HashMap<String, AgentRules>,
}

impl PermissionPolicy {
    /// Create a new permission policy with default settings.
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load permission policy from workspace.
    ///
    /// If the policy file doesn't exist, returns a default policy.
    pub fn load(workspace_root: &Path) -> anyhow::Result<Self> {
        let policy_path = Self::policy_path(workspace_root);
        if !policy_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&policy_path)?;
        let policy: Self = toml::from_str(&content)?;
        Ok(policy)
    }

    /// Save permission policy to workspace.
    pub fn save(&self, workspace_root: &Path) -> anyhow::Result<()> {
        let policy_path = Self::policy_path(workspace_root);
        if let Some(parent) = policy_path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let content = toml::to_string_pretty(self)?;
        std::fs::write(&policy_path, content)?;
        Ok(())
    }

    /// Get the path to the policy file.
    fn policy_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".nexus42").join("permissions.toml")
    }

    /// Evaluate a permission request against the policy.
    ///
    /// Returns the decision for this permission.
    pub fn evaluate(&self, permission_name: &str) -> PermissionDecision {
        // Check explicit grant rules
        if self.grant.contains_key(permission_name) {
            return PermissionDecision::Grant;
        }

        // Check explicit deny rules
        if self.deny.contains_key(permission_name) {
            return PermissionDecision::Deny;
        }

        // Use default policy
        match self.default {
            DefaultPolicy::Ask => PermissionDecision::Ask,
            DefaultPolicy::Grant => PermissionDecision::Grant,
            DefaultPolicy::Deny => PermissionDecision::Deny,
        }
    }

    /// Grant a permission by adding it to the grant list.
    pub fn grant_permission(&mut self, permission_name: String) {
        self.deny.remove(&permission_name);
        self.grant.insert(permission_name, true);
    }

    /// Deny a permission by adding it to the deny list.
    pub fn deny_permission(&mut self, permission_name: String) {
        self.grant.remove(&permission_name);
        self.deny.insert(permission_name, true);
    }

    /// List all configured permissions.
    pub fn list_permissions(&self) -> (Vec<String>, Vec<String>) {
        let granted: Vec<String> = self.grant.keys().cloned().collect();
        let denied: Vec<String> = self.deny.keys().cloned().collect();
        (granted, denied)
    }

    // -- V1.6 agent-scoped methods --

    /// Set a grant rule for a specific agent and capability.
    /// Removes any existing deny/ask rule for the same agent+capability.
    pub fn grant_agent(&mut self, agent_id: &str, capability: &str) {
        let rules = self.agents.entry(agent_id.to_string()).or_default();
        rules.deny.remove(capability);
        rules.ask.remove(capability);
        rules.grant.insert(capability.to_string(), true);
    }

    /// Set a deny rule for a specific agent and capability.
    /// Removes any existing grant/ask rule for the same agent+capability.
    pub fn deny_agent(&mut self, agent_id: &str, capability: &str) {
        let rules = self.agents.entry(agent_id.to_string()).or_default();
        rules.grant.remove(capability);
        rules.ask.remove(capability);
        rules.deny.insert(capability.to_string(), true);
    }

    /// Set an ask (prompt) rule for a specific agent and capability.
    /// Removes any existing grant/deny rule for the same agent+capability.
    pub fn ask_agent(&mut self, agent_id: &str, capability: &str) {
        let rules = self.agents.entry(agent_id.to_string()).or_default();
        rules.grant.remove(capability);
        rules.deny.remove(capability);
        rules.ask.insert(capability.to_string(), true);
    }

    /// Revoke a specific rule for an agent+capability tuple.
    /// Returns true if a rule was found and removed.
    pub fn revoke_agent(&mut self, agent_id: &str, capability: &str) -> bool {
        if let Some(rules) = self.agents.get_mut(agent_id) {
            let removed = rules.grant.remove(capability).is_some()
                || rules.deny.remove(capability).is_some()
                || rules.ask.remove(capability).is_some();
            if removed && rules.is_empty() {
                self.agents.remove(agent_id);
            }
            return removed;
        }
        false
    }

    /// Reset all rules for a specific agent.
    /// Returns true if the agent had any rules.
    pub fn reset_agent(&mut self, agent_id: &str) -> bool {
        self.agents.remove(agent_id).is_some()
    }

    /// Reset all agent rules (but keep global rules and default policy).
    pub fn reset_all_agents(&mut self) {
        self.agents.clear();
    }

    /// Evaluate a permission request for a specific agent.
    ///
    /// Checks agent-scoped rules first, then global rules, then default policy.
    pub fn evaluate_for_agent(&self, agent_id: &str, capability: &str) -> PermissionDecision {
        // Check agent-scoped rules first
        if let Some(rules) = self.agents.get(agent_id) {
            if rules.grant.contains_key(capability) {
                return PermissionDecision::Grant;
            }
            if rules.deny.contains_key(capability) {
                return PermissionDecision::Deny;
            }
            if rules.ask.contains_key(capability) {
                return PermissionDecision::Ask;
            }
        }

        // Fall through to global evaluation
        self.evaluate(capability)
    }

    /// List all rules for a specific agent.
    /// Returns (granted, denied, asked) capability lists.
    pub fn list_agent_rules(&self, agent_id: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
        if let Some(rules) = self.agents.get(agent_id) {
            let granted: Vec<String> = rules.grant.keys().cloned().collect();
            let denied: Vec<String> = rules.deny.keys().cloned().collect();
            let asked: Vec<String> = rules.ask.keys().cloned().collect();
            (granted, denied, asked)
        } else {
            (vec![], vec![], vec![])
        }
    }

    /// List all agents that have rules configured.
    pub fn list_agents(&self) -> Vec<&str> {
        let mut agents: Vec<&str> = self.agents.keys().map(|s| s.as_str()).collect();
        agents.sort();
        agents
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_policy_is_ask() {
        let policy = PermissionPolicy::new();
        assert_eq!(policy.default, DefaultPolicy::Ask);
    }

    #[test]
    fn test_evaluate_with_default_ask() {
        let policy = PermissionPolicy::new();
        let decision = policy.evaluate("file_system.read");
        assert_eq!(decision, PermissionDecision::Ask);
    }

    #[test]
    fn test_evaluate_with_default_grant() {
        let mut policy = PermissionPolicy::new();
        policy.default = DefaultPolicy::Grant;

        let decision = policy.evaluate("file_system.read");
        assert_eq!(decision, PermissionDecision::Grant);
    }

    #[test]
    fn test_evaluate_with_default_deny() {
        let mut policy = PermissionPolicy::new();
        policy.default = DefaultPolicy::Deny;

        let decision = policy.evaluate("file_system.read");
        assert_eq!(decision, PermissionDecision::Deny);
    }

    #[test]
    fn test_explicit_grant_rule() {
        let mut policy = PermissionPolicy::new();
        policy.grant_permission("file_system.read".to_string());

        let decision = policy.evaluate("file_system.read");
        assert_eq!(decision, PermissionDecision::Grant);
    }

    #[test]
    fn test_explicit_deny_rule() {
        let mut policy = PermissionPolicy::new();
        policy.deny_permission("terminal.kill".to_string());

        let decision = policy.evaluate("terminal.kill");
        assert_eq!(decision, PermissionDecision::Deny);
    }

    #[test]
    fn test_grant_overrides_deny() {
        let mut policy = PermissionPolicy::new();
        policy.deny_permission("file_system.read".to_string());
        policy.grant_permission("file_system.read".to_string());

        let decision = policy.evaluate("file_system.read");
        assert_eq!(decision, PermissionDecision::Grant);
    }

    #[test]
    fn test_deny_overrides_grant() {
        let mut policy = PermissionPolicy::new();
        policy.grant_permission("file_system.read".to_string());
        policy.deny_permission("file_system.read".to_string());

        let decision = policy.evaluate("file_system.read");
        assert_eq!(decision, PermissionDecision::Deny);
    }

    #[test]
    fn test_list_permissions() {
        let mut policy = PermissionPolicy::new();
        policy.grant_permission("file_system.read".to_string());
        policy.grant_permission("file_system.write".to_string());
        policy.deny_permission("terminal.kill".to_string());

        let (granted, denied) = policy.list_permissions();

        assert_eq!(granted.len(), 2);
        assert!(granted.contains(&"file_system.read".to_string()));
        assert!(granted.contains(&"file_system.write".to_string()));

        assert_eq!(denied.len(), 1);
        assert!(denied.contains(&"terminal.kill".to_string()));
    }

    #[test]
    fn test_save_and_load_policy() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_root = temp_dir.path();

        let mut policy = PermissionPolicy::new();
        policy.default = DefaultPolicy::Grant;
        policy.grant_permission("file_system.read".to_string());
        policy.deny_permission("terminal.kill".to_string());

        // Save
        policy.save(workspace_root).expect("Failed to save policy");

        // Load
        let loaded = PermissionPolicy::load(workspace_root).expect("Failed to load policy");

        assert_eq!(loaded.default, DefaultPolicy::Grant);
        assert_eq!(
            loaded.evaluate("file_system.read"),
            PermissionDecision::Grant
        );
        assert_eq!(loaded.evaluate("terminal.kill"), PermissionDecision::Deny);
        assert_eq!(
            loaded.evaluate("unknown.permission"),
            PermissionDecision::Grant
        );
    }

    #[test]
    fn test_load_nonexistent_policy_returns_default() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_root = temp_dir.path();

        let policy = PermissionPolicy::load(workspace_root).expect("Failed to load policy");
        assert_eq!(policy.default, DefaultPolicy::Ask);
        assert!(policy.grant.is_empty());
        assert!(policy.deny.is_empty());
    }

    #[test]
    fn test_toml_serialization() {
        let mut policy = PermissionPolicy::new();
        policy.default = DefaultPolicy::Grant;
        policy.grant_permission("file_system.read".to_string());
        policy.deny_permission("terminal.kill".to_string());

        let toml_str = toml::to_string_pretty(&policy).expect("Failed to serialize");

        assert!(toml_str.contains("default = \"grant\""));
        assert!(toml_str.contains("file_system.read"));
        assert!(toml_str.contains("terminal.kill"));

        // Deserialize back
        let deserialized: PermissionPolicy =
            toml::from_str(&toml_str).expect("Failed to deserialize");

        assert_eq!(deserialized.default, DefaultPolicy::Grant);
        assert_eq!(
            deserialized.evaluate("file_system.read"),
            PermissionDecision::Grant
        );
        assert_eq!(
            deserialized.evaluate("terminal.kill"),
            PermissionDecision::Deny
        );
    }

    // -- V1.6 agent-scoped tests --

    #[test]
    fn test_grant_agent_adds_rule() {
        let mut policy = PermissionPolicy::new();
        policy.grant_agent("test-agent", "terminal.create");

        let (granted, denied, asked) = policy.list_agent_rules("test-agent");
        assert_eq!(granted, vec!["terminal.create"]);
        assert!(denied.is_empty());
        assert!(asked.is_empty());
    }

    #[test]
    fn test_deny_agent_adds_rule() {
        let mut policy = PermissionPolicy::new();
        policy.deny_agent("test-agent", "terminal.kill");

        let (granted, denied, asked) = policy.list_agent_rules("test-agent");
        assert!(granted.is_empty());
        assert_eq!(denied, vec!["terminal.kill"]);
        assert!(asked.is_empty());
    }

    #[test]
    fn test_ask_agent_adds_rule() {
        let mut policy = PermissionPolicy::new();
        policy.ask_agent("test-agent", "file_system.write");

        let (granted, denied, asked) = policy.list_agent_rules("test-agent");
        assert!(granted.is_empty());
        assert!(denied.is_empty());
        assert_eq!(asked, vec!["file_system.write"]);
    }

    #[test]
    fn test_grant_agent_overwrites_deny() {
        let mut policy = PermissionPolicy::new();
        policy.deny_agent("test-agent", "terminal.create");
        policy.grant_agent("test-agent", "terminal.create");

        let (granted, denied, asked) = policy.list_agent_rules("test-agent");
        assert_eq!(granted, vec!["terminal.create"]);
        assert!(denied.is_empty());
        assert!(asked.is_empty());
    }

    #[test]
    fn test_deny_agent_overwrites_grant() {
        let mut policy = PermissionPolicy::new();
        policy.grant_agent("test-agent", "terminal.create");
        policy.deny_agent("test-agent", "terminal.create");

        let (granted, denied, asked) = policy.list_agent_rules("test-agent");
        assert!(granted.is_empty());
        assert_eq!(denied, vec!["terminal.create"]);
        assert!(asked.is_empty());
    }

    #[test]
    fn test_revoke_agent_removes_specific_rule() {
        let mut policy = PermissionPolicy::new();
        policy.grant_agent("test-agent", "terminal.create");
        policy.grant_agent("test-agent", "file_system.read");

        let removed = policy.revoke_agent("test-agent", "terminal.create");
        assert!(removed);

        let (granted, _, _) = policy.list_agent_rules("test-agent");
        assert_eq!(granted, vec!["file_system.read"]);
    }

    #[test]
    fn test_revoke_nonexistent_rule_returns_false() {
        let mut policy = PermissionPolicy::new();
        let removed = policy.revoke_agent("test-agent", "nonexistent");
        assert!(!removed);
    }

    #[test]
    fn test_revoke_last_rule_removes_agent_entry() {
        let mut policy = PermissionPolicy::new();
        policy.grant_agent("test-agent", "terminal.create");

        policy.revoke_agent("test-agent", "terminal.create");
        assert!(policy.list_agents().is_empty());
    }

    #[test]
    fn test_reset_agent_removes_all_rules_for_agent() {
        let mut policy = PermissionPolicy::new();
        policy.grant_agent("test-agent", "terminal.create");
        policy.deny_agent("test-agent", "terminal.kill");
        policy.grant_agent("other-agent", "file_system.read");

        let removed = policy.reset_agent("test-agent");
        assert!(removed);

        assert!(policy.list_agent_rules("test-agent").0.is_empty());
        assert!(!policy.list_agent_rules("other-agent").0.is_empty());
    }

    #[test]
    fn test_reset_all_agents_clears_everything() {
        let mut policy = PermissionPolicy::new();
        policy.grant_agent("agent-a", "cap-a");
        policy.deny_agent("agent-b", "cap-b");

        policy.reset_all_agents();

        assert!(policy.list_agents().is_empty());
    }

    #[test]
    fn test_reset_all_agents_preserves_global_rules() {
        let mut policy = PermissionPolicy::new();
        policy.grant_permission("file_system.read".to_string());
        policy.deny_permission("terminal.kill".to_string());
        policy.grant_agent("test-agent", "terminal.create");

        policy.reset_all_agents();

        assert_eq!(
            policy.evaluate("file_system.read"),
            PermissionDecision::Grant
        );
        assert_eq!(policy.evaluate("terminal.kill"), PermissionDecision::Deny);
    }

    #[test]
    fn test_evaluate_for_agent_checks_agent_rules_first() {
        let mut policy = PermissionPolicy::new();
        policy.default = DefaultPolicy::Deny;
        policy.grant_agent("test-agent", "terminal.create");

        // Agent has grant rule -> grant
        assert_eq!(
            policy.evaluate_for_agent("test-agent", "terminal.create"),
            PermissionDecision::Grant
        );
        // Unknown agent -> falls through to default deny
        assert_eq!(
            policy.evaluate_for_agent("other-agent", "terminal.create"),
            PermissionDecision::Deny
        );
    }

    #[test]
    fn test_evaluate_for_agent_falls_through_to_global_rules() {
        let mut policy = PermissionPolicy::new();
        policy.grant_permission("file_system.read".to_string());
        policy.deny_agent("test-agent", "file_system.read");

        // Agent deny overrides global grant
        assert_eq!(
            policy.evaluate_for_agent("test-agent", "file_system.read"),
            PermissionDecision::Deny
        );
        // Other agent gets global grant
        assert_eq!(
            policy.evaluate_for_agent("other-agent", "file_system.read"),
            PermissionDecision::Grant
        );
    }

    #[test]
    fn test_list_agents_returns_sorted() {
        let mut policy = PermissionPolicy::new();
        policy.grant_agent("charlie", "cap-a");
        policy.grant_agent("alpha", "cap-b");

        let agents = policy.list_agents();
        assert_eq!(agents, vec!["alpha", "charlie"]);
    }

    #[test]
    fn test_list_agent_rules_for_nonexistent_agent() {
        let policy = PermissionPolicy::new();
        let (granted, denied, asked) = policy.list_agent_rules("nonexistent");
        assert!(granted.is_empty());
        assert!(denied.is_empty());
        assert!(asked.is_empty());
    }

    #[test]
    fn test_agent_rules_toml_roundtrip() {
        let mut policy = PermissionPolicy::new();
        policy.grant_agent("test-agent", "terminal.create");
        policy.deny_agent("test-agent", "terminal.kill");
        policy.ask_agent("test-agent", "file_system.write");

        let toml_str = toml::to_string_pretty(&policy).expect("Failed to serialize");

        // Verify key structure
        assert!(toml_str.contains("[agents.test-agent.grant]"));
        assert!(toml_str.contains("terminal.create"));
        assert!(toml_str.contains("[agents.test-agent.deny]"));
        assert!(toml_str.contains("terminal.kill"));
        assert!(toml_str.contains("[agents.test-agent.ask]"));
        assert!(toml_str.contains("file_system.write"));

        // Deserialize and verify
        let loaded: PermissionPolicy = toml::from_str(&toml_str).expect("Failed to deserialize");
        let (granted, denied, asked) = loaded.list_agent_rules("test-agent");
        assert_eq!(granted, vec!["terminal.create"]);
        assert_eq!(denied, vec!["terminal.kill"]);
        assert_eq!(asked, vec!["file_system.write"]);
    }

    #[test]
    fn test_save_and_load_with_agent_rules() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_root = temp_dir.path();

        let mut policy = PermissionPolicy::new();
        policy.grant_agent("test-agent", "terminal.create");
        policy.deny_agent("test-agent", "terminal.kill");

        policy.save(workspace_root).expect("Failed to save policy");

        let loaded = PermissionPolicy::load(workspace_root).expect("Failed to load policy");
        assert_eq!(
            loaded.evaluate_for_agent("test-agent", "terminal.create"),
            PermissionDecision::Grant
        );
        assert_eq!(
            loaded.evaluate_for_agent("test-agent", "terminal.kill"),
            PermissionDecision::Deny
        );
    }

    #[test]
    fn test_agent_rules_is_empty() {
        let mut rules = AgentRules::new();
        assert!(rules.is_empty());

        rules.grant.insert("cap".to_string(), true);
        assert!(!rules.is_empty());
    }
}
