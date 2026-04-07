//! Permission policy engine for ACP permission requests.
//!
//! This module implements V1.1's configurable permission policy system (ACP-R7),
//! replacing V1.0's auto-grant-all behavior with user-controlled grant/deny rules.
//!
//! # Architecture
//!
//! ```text
//! Agent requests permission
//!         │
//!         ▼
//! PermissionPolicy::evaluate()
//!         │
//!         ├─► Check explicit rules (grant/deny mappings)
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

/// Permission policy configuration loaded from workspace.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PermissionPolicy {
    /// Default policy for permissions without explicit rules.
    #[serde(default)]
    pub default: DefaultPolicy,

    /// Permissions to always grant.
    #[serde(default)]
    pub grant: HashMap<String, bool>,

    /// Permissions to always deny.
    #[serde(default)]
    pub deny: HashMap<String, bool>,
}

impl PermissionPolicy {
    /// Create a new permission policy with default settings.
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
}
