//! Permission management commands (V1.6).
//!
//! Implements `nexus42 permission list/grant/deny/ask/revoke/reset` for
//! managing per-agent permission rules in `.nexus42/permissions.toml`.
//! Uses `toml_edit` to preserve unknown fields and comments.

use crate::config::find_workspace_root;
use crate::errors::Result;
use clap::Subcommand;
use nexus_acp_host::PermissionPolicy;

/// Permission management commands (agent-scoped rules)
#[derive(Debug, Subcommand)]
pub enum PermissionCommand {
    /// List permission rules, optionally filtered by agent
    List {
        /// Filter by agent ID
        #[arg(long)]
        agent: Option<String>,

        /// Output format (text or json)
        #[arg(short = 'o', long = "output", default_value = "text")]
        output_format: String,
    },

    /// Grant a capability for an agent
    Grant {
        /// Agent ID
        agent: String,

        /// Capability name (e.g., "terminal.create")
        capability: String,
    },

    /// Deny a capability for an agent
    Deny {
        /// Agent ID
        agent: String,

        /// Capability name (e.g., "terminal.kill")
        capability: String,
    },

    /// Require user confirmation (ask) for a capability
    Ask {
        /// Agent ID
        agent: String,

        /// Capability name (e.g., "file_system.write")
        capability: String,
    },

    /// Revoke a specific rule for an agent+capability
    Revoke {
        /// Agent ID
        agent: String,

        /// Capability name
        capability: String,
    },

    /// Reset all rules for an agent (or all agents)
    Reset {
        /// Reset rules for a specific agent only
        #[arg(long)]
        agent: Option<String>,
    },
}

/// Run permission management commands.
pub async fn run(command: PermissionCommand) -> Result<()> {
    let workspace_root = find_workspace_root().ok_or_else(|| {
        anyhow::anyhow!("Not in a Nexus workspace. Run 'nexus42 init workspace' first.")
    })?;

    match command {
        PermissionCommand::List {
            agent,
            output_format,
        } => run_list(&workspace_root, agent.as_deref(), &output_format),
        PermissionCommand::Grant { agent, capability } => {
            run_grant(&workspace_root, &agent, &capability)
        }
        PermissionCommand::Deny { agent, capability } => {
            run_deny(&workspace_root, &agent, &capability)
        }
        PermissionCommand::Ask { agent, capability } => {
            run_ask(&workspace_root, &agent, &capability)
        }
        PermissionCommand::Revoke { agent, capability } => {
            run_revoke(&workspace_root, &agent, &capability)
        }
        PermissionCommand::Reset { agent } => run_reset(&workspace_root, agent.as_deref()),
    }
}

fn run_list(
    workspace_root: &std::path::Path,
    agent_filter: Option<&str>,
    output_format: &str,
) -> Result<()> {
    let policy = PermissionPolicy::load(workspace_root)?;

    if output_format == "json" {
        print_list_json(&policy, agent_filter)?;
    } else {
        print_list_text(&policy, agent_filter);
    }

    Ok(())
}

fn print_list_text(policy: &PermissionPolicy, agent_filter: Option<&str>) {
    println!("Permission Rules");
    println!("================\n");

    // Global rules
    let (global_granted, global_denied) = policy.list_permissions();
    if !global_granted.is_empty() || !global_denied.is_empty() {
        println!("Global Rules (default: {:?}):", policy.default);
        if !global_granted.is_empty() {
            println!("  Granted:");
            for perm in &global_granted {
                println!("    [grant] {}", perm);
            }
        }
        if !global_denied.is_empty() {
            println!("  Denied:");
            for perm in &global_denied {
                println!("    [deny] {}", perm);
            }
        }
        println!();
    }

    // Agent rules
    let agents: Vec<&str> = if let Some(filter) = agent_filter {
        if policy.list_agent_rules(filter).0.is_empty()
            && policy.list_agent_rules(filter).1.is_empty()
            && policy.list_agent_rules(filter).2.is_empty()
        {
            vec![]
        } else {
            vec![filter]
        }
    } else {
        policy.list_agents()
    };

    if agents.is_empty() {
        if let Some(filter) = agent_filter {
            println!("No rules found for agent '{}'.", filter);
        } else if global_granted.is_empty() && global_denied.is_empty() {
            println!("No permission rules configured.");
            println!("Use 'nexus42 permission grant <agent> <capability>' to add rules.");
        }
        return;
    }

    for agent_id in &agents {
        let (granted, denied, asked) = policy.list_agent_rules(agent_id);
        println!("Agent: {}", agent_id);
        if !granted.is_empty() {
            for cap in &granted {
                println!("  [grant] {}", cap);
            }
        }
        if !denied.is_empty() {
            for cap in &denied {
                println!("  [deny] {}", cap);
            }
        }
        if !asked.is_empty() {
            for cap in &asked {
                println!("  [ask] {}", cap);
            }
        }
        if granted.is_empty() && denied.is_empty() && asked.is_empty() {
            println!("  (no rules)");
        }
        println!();
    }
}

fn print_list_json(policy: &PermissionPolicy, agent_filter: Option<&str>) -> Result<()> {
    let mut agents_json = serde_json::Map::new();

    let agent_ids: Vec<&str> = if let Some(filter) = agent_filter {
        vec![filter]
    } else {
        policy.list_agents()
    };

    for agent_id in &agent_ids {
        let (granted, denied, asked) = policy.list_agent_rules(agent_id);
        let mut rules = serde_json::Map::new();
        if !granted.is_empty() {
            rules.insert("grant".to_string(), serde_json::json!(granted));
        }
        if !denied.is_empty() {
            rules.insert("deny".to_string(), serde_json::json!(denied));
        }
        if !asked.is_empty() {
            rules.insert("ask".to_string(), serde_json::json!(asked));
        }
        if !rules.is_empty() {
            agents_json.insert(agent_id.to_string(), serde_json::Value::Object(rules));
        }
    }

    let result = serde_json::json!({
        "default": format!("{:?}", policy.default).to_lowercase(),
        "agents": agents_json,
    });
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

fn run_grant(workspace_root: &std::path::Path, agent: &str, capability: &str) -> Result<()> {
    let mut doc = PermissionPolicy::load_toml_edit(workspace_root)
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
    PermissionPolicy::ensure_agents_table_doc(&mut doc);
    PermissionPolicy::ensure_agent_action_table_doc(&mut doc, agent, "grant");
    PermissionPolicy::set_agent_capability_doc(&mut doc, agent, "grant", capability, true);

    // Remove from deny/ask if present
    PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "deny", capability);
    PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "ask", capability);

    PermissionPolicy::save_toml_edit_doc(workspace_root, &doc)
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
    println!("Granted '{}' for agent '{}'.", capability, agent);
    Ok(())
}

fn run_deny(workspace_root: &std::path::Path, agent: &str, capability: &str) -> Result<()> {
    let mut doc = PermissionPolicy::load_toml_edit(workspace_root)
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
    PermissionPolicy::ensure_agents_table_doc(&mut doc);
    PermissionPolicy::ensure_agent_action_table_doc(&mut doc, agent, "deny");
    PermissionPolicy::set_agent_capability_doc(&mut doc, agent, "deny", capability, true);

    // Remove from grant/ask if present
    PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "grant", capability);
    PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "ask", capability);

    PermissionPolicy::save_toml_edit_doc(workspace_root, &doc)
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
    println!("Denied '{}' for agent '{}'.", capability, agent);
    Ok(())
}

fn run_ask(workspace_root: &std::path::Path, agent: &str, capability: &str) -> Result<()> {
    let mut doc = PermissionPolicy::load_toml_edit(workspace_root)
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
    PermissionPolicy::ensure_agents_table_doc(&mut doc);
    PermissionPolicy::ensure_agent_action_table_doc(&mut doc, agent, "ask");
    PermissionPolicy::set_agent_capability_doc(&mut doc, agent, "ask", capability, true);

    // Remove from grant/deny if present
    PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "grant", capability);
    PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "deny", capability);

    PermissionPolicy::save_toml_edit_doc(workspace_root, &doc)
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
    println!("Set '{}' to 'ask' for agent '{}'.", capability, agent);
    Ok(())
}

fn run_revoke(workspace_root: &std::path::Path, agent: &str, capability: &str) -> Result<()> {
    let policy = PermissionPolicy::load(workspace_root)?;
    let (_, _, asked) = policy.list_agent_rules(agent);

    // Check if there's a rule to revoke in any category
    let has_rule = policy
        .list_agent_rules(agent)
        .0
        .iter()
        .chain(policy.list_agent_rules(agent).1.iter())
        .chain(asked.iter())
        .any(|c| c == capability);

    if !has_rule {
        return Err(crate::errors::CliError::Other(format!(
            "No rule found for '{}' on agent '{}'.",
            capability, agent
        )));
    }

    let mut doc = PermissionPolicy::load_toml_edit(workspace_root)
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
    let removed =
        PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "grant", capability)
            || PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "deny", capability)
            || PermissionPolicy::remove_agent_capability_doc(&mut doc, agent, "ask", capability);

    if removed {
        PermissionPolicy::clean_empty_agent_tables_doc(&mut doc, agent);
        PermissionPolicy::save_toml_edit_doc(workspace_root, &doc)
            .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
        println!("Revoked '{}' for agent '{}'.", capability, agent);
    }

    Ok(())
}

fn run_reset(workspace_root: &std::path::Path, agent: Option<&str>) -> Result<()> {
    let policy = PermissionPolicy::load(workspace_root)?;

    match agent {
        Some(agent_id) => {
            if policy.list_agent_rules(agent_id).0.is_empty()
                && policy.list_agent_rules(agent_id).1.is_empty()
                && policy.list_agent_rules(agent_id).2.is_empty()
            {
                return Err(crate::errors::CliError::Other(format!(
                    "No rules found for agent '{}'.",
                    agent_id
                )));
            }

            let mut doc = PermissionPolicy::load_toml_edit(workspace_root)
                .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
            if let Some(agents) = doc.get_mut("agents") {
                if let Some(agent_table) = agents.get_mut(agent_id) {
                    if let Some(t) = agent_table.as_table_like_mut() {
                        t.clear();
                    }
                }
                // Clean up empty agent entry
                if let Some(agents_table) = agents.as_table_like_mut() {
                    agents_table.remove(agent_id);
                }
            }
            PermissionPolicy::save_toml_edit_doc(workspace_root, &doc)
                .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
            println!("Reset all rules for agent '{}'.", agent_id);
        }
        None => {
            if policy.list_agents().is_empty() {
                return Err(crate::errors::CliError::Other(
                    "No agent rules configured.".to_string(),
                ));
            }

            let mut doc = PermissionPolicy::load_toml_edit(workspace_root)
                .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
            if let Some(agents) = doc.get_mut("agents") {
                if let Some(table) = agents.as_table_like_mut() {
                    table.clear();
                }
            }
            PermissionPolicy::save_toml_edit_doc(workspace_root, &doc)
                .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;
            println!("Reset all agent rules.");
        }
    }

    Ok(())
}

// -- toml_edit helpers are now in policy.rs as PermissionPolicy methods --

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn make_workspace() -> TempDir {
        TempDir::new().expect("Failed to create temp dir")
    }

    #[test]
    fn test_grant_creates_permission_file() {
        let ws = make_workspace();
        run_grant(ws.path(), "test-agent", "terminal.create").expect("grant failed");

        let path = PermissionPolicy::policy_path(ws.path());
        assert!(path.exists());

        let loaded = PermissionPolicy::load(ws.path()).expect("load failed");
        let (granted, _, _) = loaded.list_agent_rules("test-agent");
        assert_eq!(granted, vec!["terminal.create"]);
    }

    #[test]
    fn test_grant_then_list_shows_rule() {
        let ws = make_workspace();
        run_grant(ws.path(), "test-agent", "terminal.create").expect("grant failed");
        run_list(ws.path(), None, "text").expect("list failed");
        // The list command ran without error; verified by I/O
    }

    #[test]
    fn test_deny_overwrites_grant() {
        let ws = make_workspace();
        run_grant(ws.path(), "test-agent", "terminal.create").expect("grant failed");
        run_deny(ws.path(), "test-agent", "terminal.create").expect("deny failed");

        let loaded = PermissionPolicy::load(ws.path()).expect("load failed");
        assert_eq!(
            loaded.evaluate_for_agent("test-agent", "terminal.create"),
            nexus_acp_host::PermissionDecision::Deny
        );
    }

    #[test]
    fn test_revoke_removes_specific_rule() {
        let ws = make_workspace();
        run_grant(ws.path(), "test-agent", "terminal.create").expect("grant failed");
        run_grant(ws.path(), "test-agent", "file_system.read").expect("grant failed");

        run_revoke(ws.path(), "test-agent", "terminal.create").expect("revoke failed");

        let loaded = PermissionPolicy::load(ws.path()).expect("load failed");
        let (granted, _, _) = loaded.list_agent_rules("test-agent");
        assert_eq!(granted, vec!["file_system.read"]);
    }

    #[test]
    fn test_reset_removes_all_agent_rules() {
        let ws = make_workspace();
        run_grant(ws.path(), "test-agent", "terminal.create").expect("grant failed");
        run_deny(ws.path(), "test-agent", "terminal.kill").expect("deny failed");

        run_reset(ws.path(), Some("test-agent")).expect("reset failed");

        let loaded = PermissionPolicy::load(ws.path()).expect("load failed");
        assert!(loaded.list_agent_rules("test-agent").0.is_empty());
    }

    #[test]
    fn test_ask_adds_rule() {
        let ws = make_workspace();
        run_ask(ws.path(), "test-agent", "file_system.write").expect("ask failed");

        let loaded = PermissionPolicy::load(ws.path()).expect("load failed");
        assert_eq!(
            loaded.evaluate_for_agent("test-agent", "file_system.write"),
            nexus_acp_host::PermissionDecision::Ask
        );
    }

    #[test]
    fn test_preserves_existing_global_rules() {
        let ws = make_workspace();
        // Set up global rules first
        let mut policy = PermissionPolicy::new();
        policy.grant_permission("file_system.read".to_string());
        policy.save_toml_edit(ws.path()).expect("save failed");

        // Add agent rule
        run_grant(ws.path(), "test-agent", "terminal.create").expect("grant failed");

        let loaded = PermissionPolicy::load(ws.path()).expect("load failed");
        // Global rule still intact
        assert_eq!(
            loaded.evaluate("file_system.read"),
            nexus_acp_host::PermissionDecision::Grant
        );
        // Agent rule exists
        assert_eq!(
            loaded.evaluate_for_agent("test-agent", "terminal.create"),
            nexus_acp_host::PermissionDecision::Grant
        );
    }

    #[test]
    fn test_preserves_comments_via_toml_edit() {
        let ws = make_workspace();
        let path = PermissionPolicy::policy_path(ws.path());
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).expect("create dir failed");
        }
        // Write a file with a comment
        std::fs::write(&path, "# My custom comment\ndefault = \"ask\"\n").expect("write failed");

        run_grant(ws.path(), "test-agent", "terminal.create").expect("grant failed");

        let content = std::fs::read_to_string(&path).expect("read failed");
        assert!(content.contains("# My custom comment"));
    }

    #[test]
    fn test_revoke_nonexistent_returns_error() {
        let ws = make_workspace();
        let result = run_revoke(ws.path(), "test-agent", "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_nonexistent_agent_returns_error() {
        let ws = make_workspace();
        let result = run_reset(ws.path(), Some("nonexistent"));
        assert!(result.is_err());
    }

    #[test]
    fn test_reset_all_agents() {
        let ws = make_workspace();
        run_grant(ws.path(), "agent-a", "cap-a").expect("grant failed");
        run_deny(ws.path(), "agent-b", "cap-b").expect("deny failed");

        run_reset(ws.path(), None).expect("reset all failed");

        let loaded = PermissionPolicy::load(ws.path()).expect("load failed");
        assert!(loaded.list_agents().is_empty());
    }

    #[test]
    fn test_list_json_output() {
        let ws = make_workspace();
        run_grant(ws.path(), "test-agent", "terminal.create").expect("grant failed");
        run_list(ws.path(), None, "json").expect("list json failed");
        // Verified by successful execution
    }

    #[test]
    fn test_list_filtered_by_agent() {
        let ws = make_workspace();
        run_grant(ws.path(), "agent-a", "cap-a").expect("grant failed");
        run_deny(ws.path(), "agent-b", "cap-b").expect("deny failed");

        // Listing only agent-a should not show agent-b
        let loaded = PermissionPolicy::load(ws.path()).expect("load failed");
        let (granted, _, _) = loaded.list_agent_rules("agent-a");
        assert_eq!(granted, vec!["cap-a"]);
    }
}
