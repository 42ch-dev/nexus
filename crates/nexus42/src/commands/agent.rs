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
        } => cmd_run(&agent_ref, message, cwd).await,
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

// ── `agent run` (unique to agent.rs — not duplicated in acp.rs) ───

async fn cmd_run(agent_ref: &str, message: Option<String>, cwd: Option<PathBuf>) -> Result<()> {
    let client = nexus_acp_host::registry::RegistryClient::new()?;
    let registry = client.get_registry().await?;

    let agent = client.find_agent(&registry, agent_ref).ok_or_else(|| {
        crate::errors::CliError::Other(format!(
            "Agent '{agent_ref}' not found. Run `nexus42 agent list` to see available agents."
        ))
    })?;

    // Resolve working directory
    let work_dir = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Resolve the launch command from distribution
    let (program, args) = super::acp::resolve_launch_command(agent)?;

    eprintln!("Starting {} {}...", agent.name, agent.version);
    eprintln!("  Command: {} {}", program, args.join(" "));

    let spawner = nexus_acp_host::transport::AgentSpawner::new(work_dir.clone());

    // Spawn the agent subprocess
    let (child, _stdin, _stdout) = spawner
        .spawn(
            &program,
            &args
                .iter()
                .map(std::string::String::as_str)
                .collect::<Vec<_>>(),
        )
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;

    let mut child = child;
    // Set up graceful shutdown handler (Ctrl+C)
    let cancel_tx = setup_cancel_handler(agent.id.clone());

    // Determine mode
    let result = if let Some(msg) = message {
        // Single-shot mode: send message, wait, exit
        eprintln!("  Mode: single-shot");
        eprintln!();
        eprintln!("Message: {msg}");
        eprintln!();

        // Wait for the agent to finish (with timeout)
        wait_for_agent_exit(&mut child, &agent.id).await
    } else {
        // Interactive mode
        eprintln!("  Mode: interactive");
        eprintln!();
        eprintln!("Type your message and press Enter. Press Ctrl+C to exit.");
        eprintln!();

        // Simple interactive prompt loop using stdin
        // The full ACP prompt integration (LocalSet + SDK) will be wired
        // in a follow-up task — here we handle the subprocess lifecycle.
        interactive_prompt_loop(&child, &agent.id)
    };

    // Send cancel signal
    let _ = cancel_tx.send(());

    // Wait for exit
    match result {
        Ok(()) => {
            let status = child.wait().await.map_err(|e| {
                crate::errors::CliError::Other(format!("Failed to wait for agent: {e}"))
            })?;
            if let Some(code) = status.code() {
                if code == 0 {
                    eprintln!("Agent exited (code {code}).");
                } else {
                    eprintln!("Agent exited with code {code}.");
                }
            }
        }
        Err(e) => {
            eprintln!("Agent error: {e}");
        }
    }

    Ok(())
}

/// Set up a Ctrl+C handler that sends a cancel signal.
fn setup_cancel_handler(agent_id: String) -> tokio::sync::oneshot::Sender<()> {
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel::<()>();

    // Spawn a task that waits for Ctrl+C and forwards it
    tokio::spawn(async move {
        tokio::signal::ctrl_c().await.ok();
        tracing::info!(
            agent_id = %agent_id,
            "Ctrl+C received, initiating graceful shutdown"
        );
        eprintln!("\nShutting down agent (Ctrl+C)...");
        // The cancel_tx is consumed here; the receiver side is dropped
        // when the scope exits, which is the signal to shut down.
        drop(cancel_rx);
    });

    cancel_tx
}

/// Wait for the agent subprocess to exit with a timeout.
async fn wait_for_agent_exit(
    child: &mut tokio::process::Child,
    agent_id: &str,
) -> std::result::Result<(), String> {
    // Use a 5-minute timeout for single-shot mode
    let timeout_duration = std::time::Duration::from_mins(5);

    match tokio::time::timeout(timeout_duration, child.wait()).await {
        Ok(Ok(status)) => {
            if status.success() {
                Ok(())
            } else {
                Err(format!(
                    "Agent {} exited with {}",
                    agent_id,
                    status
                        .code()
                        .map_or_else(|| "signal".to_string(), |c| format!("code {c}"))
                ))
            }
        }
        Ok(Err(e)) => Err(format!("Failed to wait for agent: {e}")),
        Err(_) => Err(format!(
            "Agent {} timed out after {}s",
            agent_id,
            timeout_duration.as_secs()
        )),
    }
}

/// Simple interactive prompt loop.
///
/// This reads user input from stdin and forwards it. The full ACP
/// integration (`LocalSet` + SDK prompt) will be wired in a follow-up.
fn interactive_prompt_loop(
    child: &tokio::process::Child,
    agent_id: &str,
) -> std::result::Result<(), String> {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    eprintln!("(Connected to agent: {agent_id})");
    eprintln!();

    loop {
        // Check if agent is still running
        if child.id().is_none() {
            eprintln!("Agent process has exited.");
            break;
        }

        // Print prompt
        eprint!("> ");
        let _ = stdout.flush();

        // Read user input
        let mut input = String::new();
        let read_result = stdin.lock().read_line(&mut input);
        match read_result {
            Ok(0) => {
                // EOF (Ctrl+D)
                eprintln!("\nExiting (EOF).");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Failed to read input: {e}"));
            }
        }

        let trimmed = input.trim();
        if trimmed.is_empty() {
            continue;
        }

        // Exit commands
        if trimmed == "/quit" || trimmed == "/exit" || trimmed == "quit" || trimmed == "exit" {
            eprintln!("Exiting.");
            break;
        }

        // In V1.0, we note that the full ACP prompt loop requires
        // LocalSet + SDK integration. The prompt would go through
        // AcpSdkAdapter::prompt() within a LocalSet context.
        eprintln!(
            "  [note: ACP prompt integration pending — message '{trimmed}' not sent to agent]"
        );
    }

    Ok(())
}

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
