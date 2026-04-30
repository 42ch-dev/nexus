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

/// Known top-level keys in the permissions TOML file.
pub const VALID_TOP_LEVEL_KEYS: &[&str] = &["default", "grant", "deny", "agents"];

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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check whether any rules are configured.
    #[must_use]
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Load permission policy from workspace.
    ///
    /// If the policy file doesn't exist, returns a default policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the policy file exists but cannot be read or parsed.
    pub fn load(workspace_root: &Path) -> anyhow::Result<Self> {
        let policy_path = Self::policy_path(workspace_root);
        if !policy_path.exists() {
            return Ok(Self::default());
        }

        let content = std::fs::read_to_string(&policy_path)?;
        let policy: Self = toml::from_str(&content)?;
        Ok(policy)
    }

    /// Save permission policy to workspace using `toml_edit` for round-trip
    /// preservation of comments, formatting, and unknown keys.
    ///
    /// If the policy file already exists, it is parsed as a `toml_edit` document
    /// and updated in place. Otherwise a new document is created.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read, parsed, or written.
    pub fn save_toml_edit(&self, workspace_root: &Path) -> anyhow::Result<()> {
        let mut doc = Self::load_toml_edit(workspace_root)?;

        // Update `default` key
        doc["default"] = toml_edit::value(format!("{:?}", self.default).to_lowercase());

        // Update `grant` keys
        Self::sync_hashmap_to_table(&mut doc, "grant", &self.grant);

        // Update `deny` keys
        Self::sync_hashmap_to_table(&mut doc, "deny", &self.deny);

        // Update per-agent rules
        for (agent_id, rules) in &self.agents {
            if !rules.grant.is_empty() || !rules.deny.is_empty() || !rules.ask.is_empty() {
                Self::ensure_agents_table_doc(&mut doc);
                Self::sync_hashmap_to_table_nested(&mut doc, agent_id, "grant", &rules.grant);
                Self::sync_hashmap_to_table_nested(&mut doc, agent_id, "deny", &rules.deny);
                Self::sync_hashmap_to_table_nested(&mut doc, agent_id, "ask", &rules.ask);
            }
        }

        Self::save_toml_edit_doc(workspace_root, &doc)?;
        Ok(())
    }

    /// Load the permissions TOML as a mutable `toml_edit` document.
    ///
    /// Returns an empty document if the file doesn't exist.
    ///
    /// # Errors
    ///
    /// Returns an error if the file exists but cannot be read or parsed.
    pub fn load_toml_edit(workspace_root: &Path) -> anyhow::Result<toml_edit::DocumentMut> {
        let policy_path = Self::policy_path(workspace_root);
        if policy_path.exists() {
            let content = std::fs::read_to_string(&policy_path)?;
            let doc = content
                .parse::<toml_edit::DocumentMut>()
                .map_err(|e| anyhow::anyhow!("Failed to parse permissions TOML: {e}"))?;
            Ok(doc)
        } else {
            Ok(toml_edit::DocumentMut::new())
        }
    }

    /// Save a `toml_edit` document to the permissions file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be written.
    pub fn save_toml_edit_doc(
        workspace_root: &Path,
        doc: &toml_edit::DocumentMut,
    ) -> anyhow::Result<()> {
        let policy_path = Self::policy_path(workspace_root);
        if let Some(parent) = policy_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&policy_path, doc.to_string())?;
        Ok(())
    }

    /// Ensure the `[agents]` table exists in the document.
    pub fn ensure_agents_table_doc(doc: &mut toml_edit::DocumentMut) {
        if doc.get("agents").is_none() {
            doc["agents"] = toml_edit::Item::Table(toml_edit::Table::new());
        }
    }

    /// Ensure `[agents.<agent>.<action>]` table exists.
    ///
    /// # Panics
    ///
    /// Panics if the `[agents]` table or agent table cannot be accessed after creation.
    pub fn ensure_agent_action_table_doc(
        doc: &mut toml_edit::DocumentMut,
        agent: &str,
        action: &str,
    ) {
        Self::ensure_agents_table_doc(doc);
        let agents = doc["agents"].as_table_mut().expect("agents table");
        if !agents.contains_key(agent) {
            agents.insert(agent, toml_edit::Item::Table(toml_edit::Table::new()));
        }
        let agent_table = agents[agent].as_table_mut().expect("agent table");
        if !agent_table.contains_key(action) {
            agent_table.insert(action, toml_edit::Item::Table(toml_edit::Table::new()));
        }
    }

    /// Set a capability value in `[agents.<agent>.<action>]`.
    pub fn set_agent_capability_doc(
        doc: &mut toml_edit::DocumentMut,
        agent: &str,
        action: &str,
        capability: &str,
        value: bool,
    ) {
        if let Some(agents) = doc.get_mut("agents") {
            if let Some(agent_table) = agents.get_mut(agent) {
                if let Some(action_table) = agent_table.get_mut(action) {
                    action_table[capability] = toml_edit::value(value);
                }
            }
        }
    }

    /// Remove a capability from `[agents.<agent>.<action>]`.
    /// Returns true if the capability existed and was removed.
    pub fn remove_agent_capability_doc(
        doc: &mut toml_edit::DocumentMut,
        agent: &str,
        action: &str,
        capability: &str,
    ) -> bool {
        if let Some(agents) = doc.get_mut("agents") {
            if let Some(agent_table) = agents.get_mut(agent) {
                if let Some(action_table) = agent_table.get_mut(action) {
                    if action_table.get(capability).is_some() {
                        action_table
                            .as_table_like_mut()
                            .map(|t| t.remove(capability));
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Clean up empty action tables and agent entries after removal.
    pub fn clean_empty_agent_tables_doc(doc: &mut toml_edit::DocumentMut, agent: &str) {
        let actions = ["grant", "deny", "ask"];
        if let Some(agents) = doc.get_mut("agents") {
            if let Some(agent_table) = agents.get_mut(agent) {
                for action in &actions {
                    if let Some(action_table) = agent_table.get_mut(*action) {
                        if let Some(table) = action_table.as_table() {
                            if table.is_empty() {
                                if let Some(t) = agent_table.as_table_like_mut() {
                                    t.remove(action);
                                }
                            }
                        }
                    }
                }
                // If the agent entry is now empty, remove it
                if let Some(table) = agent_table.as_table() {
                    if table.is_empty() {
                        agents.as_table_like_mut().map(|t| t.remove(agent));
                    }
                }
            }
            // If the agents table is now empty, remove it
            if let Some(agents_table) = doc["agents"].as_table() {
                if agents_table.is_empty() {
                    doc.remove("agents");
                }
            }
        }
    }

    /// Reset all rules for a specific agent by clearing its action tables
    /// and removing the agent entry. Uses the same cleanup logic as
    /// `clean_empty_agent_tables_doc` but clears all actions first.
    pub fn reset_agent_doc(doc: &mut toml_edit::DocumentMut, agent: &str) {
        let actions = ["grant", "deny", "ask"];
        if let Some(agents) = doc.get_mut("agents") {
            if let Some(agent_table) = agents.get_mut(agent) {
                // Clear all action sub-tables
                if let Some(t) = agent_table.as_table_like_mut() {
                    for action in &actions {
                        t.remove(action);
                    }
                }
            }
            // Remove the agent entry entirely (now empty after clearing actions)
            if let Some(t) = agents.as_table_like_mut() {
                t.remove(agent);
            }
            // Remove agents table if empty
            if let Some(agents_table) = doc["agents"].as_table() {
                if agents_table.is_empty() {
                    doc.remove("agents");
                }
            }
        }
    }

    /// Reset all agent rules by clearing the entire `[agents]` table.
    pub fn reset_all_agents_doc(doc: &mut toml_edit::DocumentMut) {
        doc.remove("agents");
    }

    /// Validate top-level keys in the TOML document against the known schema.
    ///
    /// Returns a list of warning messages for unknown keys.
    #[must_use]
    pub fn validate_toml_keys(doc: &toml_edit::DocumentMut) -> Vec<String> {
        let mut warnings = Vec::new();

        // Check top-level keys
        for (key, _value) in doc.iter() {
            let key_str = key.to_string();
            if !VALID_TOP_LEVEL_KEYS.contains(&key_str.as_str()) {
                warnings.push(format!("Unknown top-level key: '{key_str}'"));
            }
        }

        // Check sub-keys under [agents.<agent>]
        if let Some(agents) = doc.get("agents") {
            if let Some(agents_table) = agents.as_table() {
                for (agent_id, agent_item) in agents_table {
                    let agent_str = agent_id.to_string();
                    if let Some(agent_tbl) = agent_item.as_table() {
                        for (action_key, _) in agent_tbl {
                            let action_str = action_key.to_string();
                            if !["grant", "deny", "ask"].contains(&action_str.as_str()) {
                                warnings.push(format!(
                                    "Unknown key '{action_str}' under agent '{agent_str}'",
                                ));
                            }
                        }
                    }
                }
            }
        }

        warnings
    }

    /// Get the path to the policy file.
    #[must_use]
    pub fn policy_path(workspace_root: &Path) -> PathBuf {
        workspace_root.join(".nexus42").join("permissions.toml")
    }

    /// Sync a `HashMap<String, bool>` into a top-level table in the document.
    ///
    /// Removes keys not in the map, adds keys that are missing, preserves existing formatting.
    fn sync_hashmap_to_table(
        doc: &mut toml_edit::DocumentMut,
        table_key: &str,
        map: &HashMap<String, bool>,
    ) {
        if map.is_empty() {
            doc.remove(table_key);
            return;
        }
        // Ensure the table exists
        if doc.get(table_key).is_none() {
            doc[table_key] = toml_edit::Item::Table(toml_edit::Table::new());
        }
        if let Some(item) = doc.get_mut(table_key) {
            if let Some(table) = item.as_table_mut() {
                // Remove keys not in the map
                let to_remove: Vec<String> = table
                    .iter()
                    .filter_map(|(k, _)| {
                        let key_str = k.to_string();
                        if map.contains_key(&key_str) {
                            None
                        } else {
                            Some(key_str)
                        }
                    })
                    .collect();
                for key in &to_remove {
                    table.remove(key);
                }
                // Add/update keys from the map
                for (k, v) in map {
                    table[k.as_str()] = toml_edit::value(*v);
                }
            }
        }
    }

    /// Sync a `HashMap<String, bool>` into a nested `[agents.<agent>.<action>]` table.
    fn sync_hashmap_to_table_nested(
        doc: &mut toml_edit::DocumentMut,
        agent: &str,
        action: &str,
        map: &HashMap<String, bool>,
    ) {
        if map.is_empty() {
            // Remove the action table if empty
            if let Some(agents) = doc.get_mut("agents") {
                if let Some(agent_table) = agents.get_mut(agent) {
                    if let Some(action_table) = agent_table.get_mut(action) {
                        if let Some(t) = action_table.as_table() {
                            if t.is_empty() {
                                agent_table.as_table_like_mut().map(|at| at.remove(action));
                            }
                        }
                    }
                }
            }
            return;
        }
        Self::ensure_agent_action_table_doc(doc, agent, action);
        if let Some(agents) = doc.get_mut("agents") {
            if let Some(agent_table) = agents.get_mut(agent) {
                if let Some(action_item) = agent_table.get_mut(action) {
                    if let Some(table) = action_item.as_table_mut() {
                        // Remove keys not in the map
                        let to_remove: Vec<String> = table
                            .iter()
                            .filter_map(|(k, _)| {
                                let key_str = k.to_string();
                                if map.contains_key(&key_str) {
                                    None
                                } else {
                                    Some(key_str)
                                }
                            })
                            .collect();
                        for key in &to_remove {
                            table.remove(key);
                        }
                        // Add/update keys from the map
                        for (k, v) in map {
                            table[k.as_str()] = toml_edit::value(*v);
                        }
                    }
                }
            }
        }
    }

    /// Evaluate a permission request against the policy.
    ///
    /// Returns the decision for this permission.
    #[must_use]
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn list_agent_rules(&self, agent_id: &str) -> (Vec<String>, Vec<String>, Vec<String>) {
        self.agents.get(agent_id).map_or_else(
            || (vec![], vec![], vec![]),
            |rules| {
                let granted: Vec<String> = rules.grant.keys().cloned().collect();
                let denied: Vec<String> = rules.deny.keys().cloned().collect();
                let asked: Vec<String> = rules.ask.keys().cloned().collect();
                (granted, denied, asked)
            },
        )
    }

    /// List all agents that have rules configured.
    #[must_use]
    pub fn list_agents(&self) -> Vec<&str> {
        let mut agents: Vec<&str> = self
            .agents
            .keys()
            .map(std::string::String::as_str)
            .collect();
        agents.sort_unstable();
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
        policy
            .save_toml_edit(workspace_root)
            .expect("Failed to save policy");

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

        policy
            .save_toml_edit(workspace_root)
            .expect("Failed to save policy");

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

    #[test]
    fn test_save_toml_edit_preserves_comments() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_root = temp_dir.path();

        // Write a file with a comment
        let path = PermissionPolicy::policy_path(workspace_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create dir failed");
        }
        std::fs::write(&path, "# My important comment\ndefault = \"ask\"\n").expect("write failed");

        let mut policy = PermissionPolicy::new();
        policy.grant_agent("test-agent", "terminal.create");
        policy
            .save_toml_edit(workspace_root)
            .expect("Failed to save policy");

        // Reload raw content and check comment preserved
        let content = std::fs::read_to_string(&path).expect("read failed");
        assert!(content.contains("# My important comment"));
    }

    #[test]
    fn test_save_toml_edit_preserves_unknown_key() {
        let temp_dir = TempDir::new().expect("Failed to create temp dir");
        let workspace_root = temp_dir.path();

        // Write a file with a future unknown key
        let path = PermissionPolicy::policy_path(workspace_root);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create dir failed");
        }
        std::fs::write(&path, "default = \"ask\"\nfuture_feature = \"enabled\"\n")
            .expect("write failed");

        let mut policy = PermissionPolicy::new();
        policy.grant_permission("file_system.read".to_string());
        policy
            .save_toml_edit(workspace_root)
            .expect("Failed to save policy");

        // Reload raw content and check unknown key preserved
        let content = std::fs::read_to_string(&path).expect("read failed");
        assert!(content.contains("future_feature"));
    }

    #[test]
    fn test_validate_toml_keys_known_keys_no_warnings() {
        let toml_str = r#"
default = "ask"

[grant]
"file_system.read" = true

[deny]
"terminal.kill" = true

[agents.my-agent.grant]
"terminal.create" = true
"#;
        let doc: toml_edit::DocumentMut = toml_str.parse().expect("parse failed");
        let warnings = PermissionPolicy::validate_toml_keys(&doc);
        assert!(
            warnings.is_empty(),
            "Expected no warnings, got: {warnings:?}",
        );
    }

    #[test]
    fn test_validate_toml_keys_unknown_top_level_key() {
        let toml_str = r#"
default = "ask"
unknown_key = "value"
"#;
        let doc: toml_edit::DocumentMut = toml_str.parse().expect("parse failed");
        let warnings = PermissionPolicy::validate_toml_keys(&doc);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown_key"));
    }

    #[test]
    fn test_validate_toml_keys_unknown_sub_key_under_agent() {
        let toml_str = r#"
[agents.my-agent]
unknown_action = "value"
"#;
        let doc: toml_edit::DocumentMut = toml_str.parse().expect("parse failed");
        let warnings = PermissionPolicy::validate_toml_keys(&doc);
        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("unknown_action"));
        assert!(warnings[0].contains("my-agent"));
    }
}
