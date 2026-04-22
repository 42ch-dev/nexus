//! Policy management commands for ACP permissions.
//!
//! Implements `nexus42 policy grant/deny/list` commands for managing
//! permission policies (ACP-R7).

use crate::config::find_workspace_root;
use crate::errors::Result;
use clap::Subcommand;
use nexus_acp_host::{DefaultPolicy, PermissionPolicy};
use std::path::PathBuf;

/// Policy management commands
#[derive(Debug, Subcommand)]
pub enum PolicyCommand {
    /// Grant a permission for ACP agents
    Grant {
        /// Permission name (e.g., "file_system.read")
        permission: String,
    },

    /// Deny a permission for ACP agents
    Deny {
        /// Permission name (e.g., "terminal.kill")
        permission: String,
    },

    /// List all configured permissions
    List {
        /// Output format (text or json)
        #[arg(short = 'o', long = "output", default_value = "text")]
        output_format: String,
    },

    /// Set the default policy for unknown permissions
    Default {
        /// Default policy: "ask", "grant", or "deny"
        policy: String,
    },

    /// Show current policy configuration
    Show,
}

/// Run policy management commands.
pub async fn run(command: PolicyCommand) -> Result<()> {
    let workspace_root = find_workspace_root().ok_or_else(|| {
        anyhow::anyhow!("Not in a Nexus workspace. Run 'nexus42 init workspace' first.")
    })?;

    let mut policy = PermissionPolicy::load(&workspace_root)?;

    match command {
        PolicyCommand::Grant { permission } => {
            policy.grant_permission(permission.clone());
            policy.save_toml_edit(&workspace_root)?;
            println!("✓ Granted permission: {}", permission);
        }

        PolicyCommand::Deny { permission } => {
            policy.deny_permission(permission.clone());
            policy.save_toml_edit(&workspace_root)?;
            println!("✓ Denied permission: {}", permission);
        }

        PolicyCommand::List { output_format } => {
            let (granted, denied) = policy.list_permissions();

            if output_format == "json" {
                let result = serde_json::json!({
                    "granted": granted,
                    "denied": denied,
                    "default": format!("{:?}", policy.default).to_lowercase(),
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
            } else {
                println!("Permission Policy Configuration");
                println!("================================\n");

                println!("Default Policy: {:?}\n", policy.default);

                if !granted.is_empty() {
                    println!("Granted Permissions:");
                    for perm in &granted {
                        println!("  ✓ {}", perm);
                    }
                    println!();
                }

                if !denied.is_empty() {
                    println!("Denied Permissions:");
                    for perm in &denied {
                        println!("  ✗ {}", perm);
                    }
                    println!();
                }

                if granted.is_empty() && denied.is_empty() {
                    println!("No explicit permissions configured.");
                    println!(
                        "All permissions will use the default policy: {:?}\n",
                        policy.default
                    );
                }
            }
        }

        PolicyCommand::Default {
            policy: policy_name,
        } => {
            let default_policy = match policy_name.to_lowercase().as_str() {
                "ask" => DefaultPolicy::Ask,
                "grant" => DefaultPolicy::Grant,
                "deny" => DefaultPolicy::Deny,
                _ => {
                    return Err(crate::errors::CliError::Other(format!(
                        "Invalid default policy '{}'. Must be 'ask', 'grant', or 'deny'.",
                        policy_name
                    )))
                }
            };

            policy.default = default_policy;
            policy.save_toml_edit(&workspace_root)?;
            println!("✓ Default policy set to: {:?}", default_policy);
        }

        PolicyCommand::Show => {
            let policy_path = get_policy_path(&workspace_root);
            println!("Policy file: {}", policy_path.display());
            println!();

            if policy_path.exists() {
                let content = std::fs::read_to_string(&policy_path)?;
                println!("{}", content);
            } else {
                println!("No policy file found. Using default configuration:");
                println!("default = \"{:?}\"\n", policy.default);
                println!("Run 'nexus42 policy grant <permission>' or 'nexus42 policy deny <permission>' to configure.");
            }
        }
    }

    Ok(())
}

// Helper to expose policy_path publicly
fn get_policy_path(workspace_root: &std::path::Path) -> PathBuf {
    workspace_root.join(".nexus42").join("permissions.toml")
}
