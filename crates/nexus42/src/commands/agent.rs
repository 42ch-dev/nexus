//! Agent Command — ACP agent management commands (legacy shim).
//!
//! **Deprecated**: Top-level `nexus42 agent` commands have moved to `nexus42 acp`.
//! This module retains the CLI wiring for backward compatibility but delegates
//! all logic to `commands/acp.rs` — single source of truth.
//!
//! Implements the `nexus42 agent` subcommands (all delegated to `acp`):
//! - `list` — List available agents from the ACP registry
//! - `show` — Show details for a specific agent
//! - `run`  — Run an agent interactively or with a single message
//! - `probe` — Verify ACP connectivity (registry or agent handshake)
//! - `skills` — List ACP capabilities
//! - `status` — Show daemon and ACP agent status

// This module is retained for backward compat and test coverage.
// Top-level `nexus42 agent` has moved to `nexus42 acp` (Plan 2).
#![allow(dead_code)]

use std::path::PathBuf;

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

// ── Command definitions ────────────────────────────────────────────

#[derive(Debug, Subcommand)]
pub enum AgentCommand {
    /// List available agents from the ACP registry
    List {
        /// Output format (table or json)
        #[arg(short = 'f', long = "format", default_value = "table")]
        format: String,
    },

    /// Show details for a specific agent
    Show {
        /// Agent reference (partial match on id or name)
        agent_ref: String,
    },

    /// Run an agent interactively or with a single message
    Run {
        /// Agent reference (id or name, partial match supported)
        agent_ref: String,
        /// Send a single message and exit (non-interactive mode)
        #[arg(short, long)]
        message: Option<String>,
        /// Working directory for the agent subprocess
        #[arg(short, long)]
        cwd: Option<PathBuf>,
    },

    /// Verify ACP connectivity
    Probe {
        /// Probe registry connectivity (default when no --agent is given)
        #[arg(long)]
        registry: bool,
        /// Probe a specific agent's ACP handshake
        #[arg(long, name = "AGENT")]
        agent: Option<String>,
    },

    /// List available ACP skills/capabilities
    Skills {
        /// Show detailed information including capability IDs
        #[arg(long, short)]
        verbose: bool,
        /// Output format (text or json)
        #[arg(short = 'o', long = "output", default_value = "text")]
        output_format: String,
    },

    /// Show daemon and agent status
    Status,
}

// ── Entry point ────────────────────────────────────────────────────

/// Run agent command — delegates to `acp` module internals.
///
/// # Errors
///
/// Returns `CliError` if the delegated `acp` command fails.
pub async fn run(cmd: AgentCommand, _config: &CliConfig) -> Result<()> {
    match cmd {
        AgentCommand::List { format } => super::acp::cmd_registry_list(&format).await,
        AgentCommand::Show { agent_ref } => super::acp::cmd_registry_inspect(&agent_ref).await,
        AgentCommand::Run {
            agent_ref,
            message,
            cwd,
        } => {
            eprintln!("Note: `nexus42 agent run` is deprecated. Use `nexus42 acp run` instead.");
            super::acp::cmd_run(&agent_ref, message, cwd).await
        }
        AgentCommand::Probe { registry, agent } => super::acp::cmd_probe(registry, agent).await,
        AgentCommand::Skills {
            verbose,
            output_format,
        } => {
            if output_format == "json" {
                super::acp::cmd_skills_export(&output_format)
            } else {
                super::acp::cmd_skills_export(if verbose { "verbose" } else { "text" })
            }
        }
        AgentCommand::Status => super::acp::cmd_status().await,
    }
}

// ── Helper re-exports for acp::cmd_run ────────────────────────────────
//
// These functions are defined in acp.rs's private `agent` module but are
// needed there. They are re-exported via pub(super) in acp.rs and called
// directly. No additional code needed here.

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn agent_list_delegates_to_acp() {
        // Verify the command delegates without panicking.
        // Will fail on registry fetch but that's expected in test env.
        let cmd = AgentCommand::List {
            format: "table".to_string(),
        };
        let result = run(cmd, &CliConfig::default()).await;
        // May fail if no registry available, but should not panic
        let _ = result;
    }

    #[tokio::test]
    async fn agent_status_delegates_to_acp() {
        let cmd = AgentCommand::Status;
        let result = run(cmd, &CliConfig::default()).await;
        // Status fails when daemon not running — that's expected
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn agent_skills_delegates_to_acp() {
        let cmd = AgentCommand::Skills {
            verbose: false,
            output_format: "text".to_string(),
        };
        let result = run(cmd, &CliConfig::default()).await;
        assert!(result.is_ok());
    }
}
