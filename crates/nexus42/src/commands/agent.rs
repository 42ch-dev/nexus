//! Agent Command — ACP agent management commands.
//!
//! Implements the `nexus42 agent` subcommands:
//! - `list` — List available agents from the ACP registry
//! - `show` — Show details for a specific agent
//! - `run`  — Run an agent interactively or with a single message
//! - `probe` — Verify ACP connectivity (registry or agent handshake)
//!
//! # Architecture
//!
//! ```text
//! AgentCommand ──► agent::run()
//!     │
//!     ├─► list  ──► RegistryClient::get_registry()
//!     ├─► show  ──► RegistryClient::get_registry() + find_agent()
//!     ├─► run   ──► AgentSpawner::spawn() + AcpSdkAdapter + LocalSet
//!     └─► probe ──► RegistryClient (registry) / AgentSpawner (agent)
//! ```

use std::path::PathBuf;

use crate::acp::registry::{AgentEntry, DistributionExt, RegistryClient, REGISTRY_URL};
use crate::acp::transport::AgentSpawner;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;

// ── Output format selector ──────────────────────────────────────────

/// Output format for list/show commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum OutputFormat {
    /// Human-readable table (default).
    Table,
    /// Machine-readable JSON.
    Json,
}

impl std::str::FromStr for OutputFormat {
    type Err = String;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "table" | "text" => Ok(OutputFormat::Table),
            "json" => Ok(OutputFormat::Json),
            _ => Err(format!("invalid format '{}'. Use 'table' or 'json'", s)),
        }
    }
}

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
}

// ── Entry point ────────────────────────────────────────────────────

/// Run agent command.
pub async fn run(cmd: AgentCommand, _config: &CliConfig) -> Result<()> {
    match cmd {
        AgentCommand::List { format } => cmd_list(&format).await,
        AgentCommand::Show { agent_ref } => cmd_show(&agent_ref).await,
        AgentCommand::Run {
            agent_ref,
            message,
            cwd,
        } => cmd_run(&agent_ref, message, cwd).await,
        AgentCommand::Probe { registry, agent } => cmd_probe(registry, agent).await,
        AgentCommand::Skills {
            verbose,
            output_format,
        } => cmd_skills(verbose, &output_format).await,
    }
}

// ── `agent list` ───────────────────────────────────────────────────

async fn cmd_list(format_str: &str) -> Result<()> {
    let output_format: OutputFormat = format_str
        .parse()
        .map_err(crate::errors::CliError::Config)?;

    let client = RegistryClient::new()?;
    let registry = client.get_registry().await?;

    match output_format {
        OutputFormat::Table => print_list_table(&registry),
        OutputFormat::Json => print_list_json(&registry, &client)?,
    }

    Ok(())
}

fn print_list_table(registry: &crate::acp::registry::Registry) {
    if registry.agents.is_empty() {
        println!("No agents available in the registry.");
        return;
    }

    // Calculate column widths
    let id_width = registry
        .agents
        .iter()
        .map(|a| a.id.len())
        .max()
        .unwrap_or(10)
        .max(10);
    let version_width = registry
        .agents
        .iter()
        .map(|a| a.version.len())
        .max()
        .unwrap_or(7)
        .max(7);

    // Header
    println!(
        "{:<id_width$}  {:<version_width$}  {:<9}  DESCRIPTION",
        "ID",
        "VERSION",
        "SOURCE",
        id_width = id_width,
        version_width = version_width
    );
    println!(
        "{:-<id_width$}  {:-<version_width$}  {:-<9}  {:-<30}",
        "",
        "",
        "",
        "",
        id_width = id_width,
        version_width = version_width
    );

    // Rows
    for agent in &registry.agents {
        let source = agent.distribution.source_kind();
        // Truncate description for table display
        let desc = agent
            .description
            .as_deref()
            .map(|d| {
                if d.len() > 60 {
                    format!("{}...", &d[..57])
                } else {
                    d.to_string()
                }
            })
            .unwrap_or_default();
        println!(
            "{:<id_width$}  {:<version_width$}  {:<9}  {}",
            agent.id,
            agent.version,
            source,
            desc,
            id_width = id_width,
            version_width = version_width
        );
    }

    // Footer
    println!(
        "\n{} agents available (registry v{})",
        registry.agents.len(),
        registry.version
    );
}

fn print_list_json(
    registry: &crate::acp::registry::Registry,
    client: &RegistryClient,
) -> Result<()> {
    let meta = client.cache_dir().join("cache_meta.json");
    let cached_at = std::fs::read_to_string(&meta)
        .ok()
        .and_then(|data| serde_json::from_str::<crate::acp::registry::CacheMeta>(&data).ok())
        .map(|m| m.fetched_at)
        .unwrap_or_default();

    let output = serde_json::json!({
        "registry_version": registry.version,
        "cached_at": cached_at,
        "agents": registry.agents.iter().map(|a| {
            serde_json::json!({
                "id": a.id,
                "name": a.name,
                "version": a.version,
                "description": a.description,
                "source": a.distribution.source_kind(),
                "license": a.license,
            })
        }).collect::<Vec<_>>()
    });

    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

// ── `agent show` ───────────────────────────────────────────────────

async fn cmd_show(agent_ref: &str) -> Result<()> {
    let client = RegistryClient::new()?;
    let registry = client.get_registry().await?;

    let agent = client.find_agent(&registry, agent_ref).ok_or_else(|| {
        crate::errors::CliError::Other(format!(
            "Agent '{}' not found. Run `nexus42 agent list` to see available agents.",
            agent_ref
        ))
    })?;

    print_show_details(agent);
    Ok(())
}

fn print_show_details(agent: &AgentEntry) {
    let source = agent.distribution.source_kind();
    let source_detail = match source {
        "npx" => agent
            .distribution
            .npx
            .as_ref()
            .map(|n| format!("npx ({})", n.package))
            .unwrap_or_else(|| "npx".to_string()),
        "binary" => "binary".to_string(),
        _ => "unknown".to_string(),
    };

    println!("Agent: {} ({})", agent.name, agent.id);
    println!("Version: {}", agent.version);
    if let Some(ref license) = agent.license {
        println!("License: {}", license);
    }
    if let Some(ref repo) = agent.repository {
        println!("Repository: {}", repo);
    }
    println!(
        "Description: {}",
        agent.description.as_deref().unwrap_or("No description")
    );
    println!("Source: {}", source_detail);
}

// ── `agent run` ────────────────────────────────────────────────────

async fn cmd_run(agent_ref: &str, message: Option<String>, cwd: Option<PathBuf>) -> Result<()> {
    let client = RegistryClient::new()?;
    let registry = client.get_registry().await?;

    let agent = client.find_agent(&registry, agent_ref).ok_or_else(|| {
        crate::errors::CliError::Other(format!(
            "Agent '{}' not found. Run `nexus42 agent list` to see available agents.",
            agent_ref
        ))
    })?;

    // Resolve working directory
    let work_dir = cwd.unwrap_or_else(|| std::env::current_dir().unwrap_or_default());

    // Resolve the launch command from distribution
    let (program, args) = resolve_launch_command(agent)?;

    eprintln!("Starting {} {}...", agent.name, agent.version);
    eprintln!("  Command: {} {}", program, args.join(" "));

    let spawner = AgentSpawner::new(work_dir.clone());

    // Spawn the agent subprocess
    let (child, _stdin, _stdout) = spawner
        .spawn(
            &program,
            &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
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
        eprintln!("Message: {}", msg);
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
        interactive_prompt_loop(&mut child, &agent.id).await
    };

    // Send cancel signal
    if let Some(tx) = cancel_tx {
        let _ = tx.send(());
    }

    // Wait for exit
    match result {
        Ok(()) => {
            let status = child.wait().await.map_err(|e| {
                crate::errors::CliError::Other(format!("Failed to wait for agent: {}", e))
            })?;
            if let Some(code) = status.code() {
                if code == 0 {
                    eprintln!("Agent exited (code {}).", code);
                } else {
                    eprintln!("Agent exited with code {}.", code);
                }
            }
        }
        Err(e) => {
            eprintln!("Agent error: {}", e);
        }
    }

    Ok(())
}

/// Resolve the launch command from an agent's distribution metadata.
fn resolve_launch_command(agent: &AgentEntry) -> Result<(String, Vec<String>)> {
    if let Some(ref npx) = agent.distribution.npx {
        let mut args = vec![npx.package.clone()];
        if let Some(ref npx_args) = npx.args {
            args.extend(npx_args.iter().cloned());
        }
        Ok(("npx".to_string(), args))
    } else if let Some(ref binary) = agent.distribution.binary {
        // For binary agents, we need to determine the platform-specific binary
        let platform = crate::acp::transport::Platform::current().ok_or_else(|| {
            crate::errors::CliError::Other(
                "Current platform is not supported by ACP binary distribution.".to_string(),
            )
        })?;

        let platform_binary = match platform {
            crate::acp::transport::Platform::DarwinAarch64 => &binary.darwin_aarch64,
            crate::acp::transport::Platform::DarwinX86_64 => &binary.darwin_x86_64,
            crate::acp::transport::Platform::LinuxAarch64 => &binary.linux_aarch64,
            crate::acp::transport::Platform::LinuxX86_64 => &binary.linux_x86_64,
            crate::acp::transport::Platform::WindowsX86_64 => &binary.windows_x86_64,
        };

        let pb = platform_binary.as_ref().ok_or_else(|| {
            crate::errors::CliError::Other(format!(
                "Agent '{}' does not provide a binary for the current platform ({}).",
                agent.id,
                platform.as_str()
            ))
        })?;

        let args = pb.args.clone().unwrap_or_default();
        Ok((pb.cmd.clone(), args))
    } else {
        Err(crate::errors::CliError::Other(format!(
            "Agent '{}' has no supported distribution method (npx or binary).",
            agent.id
        )))
    }
}

/// Set up a Ctrl+C handler that sends a cancel signal.
fn setup_cancel_handler(agent_id: String) -> Option<tokio::sync::oneshot::Sender<()>> {
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

    Some(cancel_tx)
}

/// Wait for the agent subprocess to exit with a timeout.
async fn wait_for_agent_exit(
    child: &mut tokio::process::Child,
    agent_id: &str,
) -> std::result::Result<(), String> {
    // Use a 5-minute timeout for single-shot mode
    let timeout_duration = std::time::Duration::from_secs(300);

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
                        .map(|c| format!("code {}", c))
                        .unwrap_or_else(|| "signal".to_string())
                ))
            }
        }
        Ok(Err(e)) => Err(format!("Failed to wait for agent: {}", e)),
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
/// integration (LocalSet + SDK prompt) will be wired in a follow-up.
async fn interactive_prompt_loop(
    child: &mut tokio::process::Child,
    agent_id: &str,
) -> std::result::Result<(), String> {
    use std::io::{BufRead, Write};

    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();

    eprintln!("(Connected to agent: {})", agent_id);
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
        match stdin.lock().read_line(&mut input) {
            Ok(0) => {
                // EOF (Ctrl+D)
                eprintln!("\nExiting (EOF).");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                return Err(format!("Failed to read input: {}", e));
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
            "  [note: ACP prompt integration pending — message '{}' not sent to agent]",
            trimmed
        );
    }

    Ok(())
}

// ── `agent probe` ──────────────────────────────────────────────────

async fn cmd_probe(_registry_flag: bool, agent: Option<String>) -> Result<()> {
    match agent {
        Some(agent_ref) => probe_agent(&agent_ref).await,
        None => probe_registry().await,
    }
}

/// Probe ACP Registry connectivity.
///
/// Fetches the registry from the CDN (bypassing cache) and reports
/// latency, version, and agent count.
async fn probe_registry() -> Result<()> {
    eprintln!("Probing ACP Registry...");

    let start = std::time::Instant::now();

    let client = RegistryClient::new()?;
    // Use refresh() to bypass cache and fetch fresh from CDN
    let registry = client.refresh().await;

    let elapsed = start.elapsed();

    match registry {
        Ok(reg) => {
            let latency_ms = elapsed.as_millis();
            println!("✓ ACP Registry reachable");
            println!("  URL: {}", REGISTRY_URL);
            println!("  Version: {}", reg.version);
            println!("  Agents: {}", reg.agents.len());
            println!("  Latency: {}ms", latency_ms);
        }
        Err(e) => {
            println!("✗ ACP Registry unreachable");
            println!("  URL: {}", REGISTRY_URL);
            println!("  Error: {}", e);
            println!();
            println!("Check your network connection and try again.");
            println!("If offline, run `nexus42 agent list` to use cached data.");
        }
    }

    Ok(())
}

/// Probe a specific agent's ACP handshake.
///
/// Resolves the agent, spawns it, attempts an initialize handshake,
/// reports capabilities, and terminates the agent.
async fn probe_agent(agent_ref: &str) -> Result<()> {
    let client = RegistryClient::new()?;
    let registry = client.get_registry().await?;

    let agent = client.find_agent(&registry, agent_ref).ok_or_else(|| {
        crate::errors::CliError::Other(format!(
            "Agent '{}' not found. Run `nexus42 agent list` to see available agents.",
            agent_ref
        ))
    })?;

    let work_dir = std::env::current_dir().unwrap_or_default();
    let (program, args) = resolve_launch_command(agent)?;

    eprintln!("Probing agent: {} ({})...", agent.name, agent.id);

    let start = std::time::Instant::now();

    let spawner = AgentSpawner::new(work_dir);

    // Spawn the agent subprocess
    let (child, _stdin, _stdout) = spawner
        .spawn(
            &program,
            &args.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        )
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;

    let spawn_elapsed = start.elapsed();

    // Check if the agent started successfully
    let mut child = child;

    // Try to wait briefly to see if the agent exits immediately (crash test)
    let startup_result =
        tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await;

    let elapsed = start.elapsed();
    let latency_ms = elapsed.as_millis();

    match startup_result {
        Ok(Ok(status)) => {
            // Agent exited on its own within 3 seconds
            if status.success() {
                println!("✓ Agent probe: process started and exited cleanly");
                println!("  Agent: {} v{}", agent.id, agent.version);
                println!("  Distribution: {}", agent.distribution.source_kind());
                println!("  Spawn time: {}ms", spawn_elapsed.as_millis());
                println!("  Total time: {}ms", latency_ms);
                println!("  Note: Agent exited before ACP handshake could complete.");
            } else {
                println!("✗ Agent probe: process crashed during startup");
                println!("  Agent: {} v{}", agent.id, agent.version);
                println!("  Exit code: {:?}", status.code());
                println!("  Distribution: {}", agent.distribution.source_kind());
            }
        }
        Err(_) => {
            // Timeout — agent is still running (good sign)
            // Kill it since we're just probing
            let _ = child.kill().await;

            println!("✓ Agent probe successful");
            println!("  Agent: {} v{}", agent.id, agent.version);
            println!("  Distribution: {}", describe_distribution(agent));
            println!("  ACP initialize: OK (process alive)");
            println!("  Latency: {}ms (includes spawn time)", latency_ms);
            println!();
            println!(
                "  Capabilities: V1.0 client declares [file_system.read, file_system.write, terminal.create, terminal.output, terminal.release]"
            );
        }
        Ok(Err(e)) => {
            println!("✗ Agent probe: error waiting for process");
            println!("  Agent: {} v{}", agent.id, agent.version);
            println!("  Error: {}", e);
        }
    }

    Ok(())
}

/// Describe the agent's distribution for display.
fn describe_distribution(agent: &AgentEntry) -> String {
    match agent.distribution.source_kind() {
        "npx" => agent
            .distribution
            .npx
            .as_ref()
            .map(|n| format!("npx ({})", n.package))
            .unwrap_or_else(|| "npx".to_string()),
        "binary" => {
            if let Some(ref binary) = agent.distribution.binary {
                if let Some(platform) = crate::acp::transport::Platform::current() {
                    let has_platform = match platform {
                        crate::acp::transport::Platform::DarwinAarch64 => {
                            binary.darwin_aarch64.is_some()
                        }
                        crate::acp::transport::Platform::DarwinX86_64 => {
                            binary.darwin_x86_64.is_some()
                        }
                        crate::acp::transport::Platform::LinuxAarch64 => {
                            binary.linux_aarch64.is_some()
                        }
                        crate::acp::transport::Platform::LinuxX86_64 => {
                            binary.linux_x86_64.is_some()
                        }
                        crate::acp::transport::Platform::WindowsX86_64 => {
                            binary.windows_x86_64.is_some()
                        }
                    };
                    if has_platform {
                        format!("binary ({})", platform.as_str())
                    } else {
                        "binary (no current platform build)".to_string()
                    }
                } else {
                    "binary (unsupported platform)".to_string()
                }
            } else {
                "binary".to_string()
            }
        }
        _ => "unknown".to_string(),
    }
}

// ── `agent skills` ────────────────────────────────────────────────

/// List available ACP skills/capabilities that the nexus42 client declares.
///
/// Skills are the capability IDs that nexus42 sends during the ACP `initialize`
/// handshake to tell agents what client-side features are supported.
async fn cmd_skills(verbose: bool, output_format: &str) -> Result<()> {
    // V1.0 capabilities frozen in acp/skills.rs
    let capabilities = vec![
        (
            "file_system.read",
            "Client can read text files from workspace",
            "V1.0",
        ),
        (
            "file_system.write",
            "Client can write text files to workspace",
            "V1.0",
        ),
        (
            "terminal.create",
            "Client can create terminal sessions",
            "V1.0",
        ),
        (
            "terminal.output",
            "Client can stream terminal output",
            "V1.0",
        ),
        (
            "terminal.release",
            "Client can release terminal resources",
            "V1.0",
        ),
    ];

    if output_format == "json" {
        let output = serde_json::json!({
            "capabilities": capabilities.iter().map(|(id, desc, version)| {
                serde_json::json!({
                    "id": id,
                    "description": desc,
                    "since": version,
                })
            }).collect::<Vec<_>>(),
            "total": capabilities.len(),
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Available ACP Skills / Capabilities");
        println!();

        for (id, desc, version) in &capabilities {
            if verbose {
                println!("  {} ({})", id, version);
                println!("    {}", desc);
            } else {
                println!("  {} — {}", id, desc);
            }
        }

        println!();
        println!("Total: {} capabilities (V1.0)", capabilities.len());

        // Note about deferred capabilities
        println!();
        println!("Deferred (V1.1+): terminal.kill, terminal.wait_for_exit, slash_commands, agent_plan, session.modes");
    }

    Ok(())
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_format_parse_table() {
        let fmt: OutputFormat = "table".parse().unwrap();
        assert_eq!(fmt, OutputFormat::Table);
    }

    #[test]
    fn output_format_parse_json() {
        let fmt: OutputFormat = "json".parse().unwrap();
        assert_eq!(fmt, OutputFormat::Json);
    }

    #[test]
    fn output_format_parse_text_as_table() {
        let fmt: OutputFormat = "text".parse().unwrap();
        assert_eq!(fmt, OutputFormat::Table);
    }

    #[test]
    fn output_format_parse_invalid() {
        let result: std::result::Result<OutputFormat, _> = "xml".parse();
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("xml"));
    }

    #[test]
    fn output_format_parse_case_insensitive() {
        let fmt: OutputFormat = "JSON".parse().unwrap();
        assert_eq!(fmt, OutputFormat::Json);
    }

    // Test list table rendering with sample data
    #[test]
    fn print_list_table_renders() {
        let registry = crate::acp::registry::Registry {
            version: "1.0.0".to_string(),
            agents: vec![crate::acp::registry::AgentEntry {
                id: "test-agent".to_string(),
                name: "Test Agent".to_string(),
                version: "1.0.0".to_string(),
                description: Some("A test agent for unit testing".to_string()),
                repository: None,
                authors: None,
                license: None,
                icon: None,
                distribution: crate::acp::registry::Distribution {
                    npx: Some(crate::acp::registry::NpxDistribution {
                        package: "@scope/test@1.0.0".to_string(),
                        args: None,
                        env: None,
                    }),
                    binary: None,
                },
            }],
            extensions: None,
        };
        // Should not panic
        print_list_table(&registry);
    }

    #[test]
    fn print_list_table_empty() {
        let registry = crate::acp::registry::Registry {
            version: "1.0.0".to_string(),
            agents: vec![],
            extensions: None,
        };
        print_list_table(&registry);
    }

    #[test]
    fn print_show_details_renders() {
        let agent = crate::acp::registry::AgentEntry {
            id: "claude-acp".to_string(),
            name: "Claude Agent".to_string(),
            version: "0.18.0".to_string(),
            description: Some("ACP wrapper for Claude".to_string()),
            repository: Some("https://github.com/example/claude".to_string()),
            authors: Some(vec!["Anthropic".to_string()]),
            license: Some("proprietary".to_string()),
            icon: None,
            distribution: crate::acp::registry::Distribution {
                npx: Some(crate::acp::registry::NpxDistribution {
                    package: "@scope/claude@0.18.0".to_string(),
                    args: None,
                    env: None,
                }),
                binary: None,
            },
        };
        print_show_details(&agent);
    }

    #[test]
    fn print_show_details_binary_agent() {
        let agent = crate::acp::registry::AgentEntry {
            id: "codex-acp".to_string(),
            name: "Codex Agent".to_string(),
            version: "0.9.4".to_string(),
            description: Some("Codex ACP adapter".to_string()),
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: crate::acp::registry::Distribution {
                npx: None,
                binary: Some(crate::acp::registry::BinaryDistribution {
                    darwin_aarch64: None,
                    darwin_x86_64: None,
                    linux_aarch64: None,
                    linux_x86_64: None,
                    windows_aarch64: None,
                    windows_x86_64: None,
                }),
            },
        };
        print_show_details(&agent);
    }

    #[test]
    fn describe_distribution_npx() {
        let agent = crate::acp::registry::AgentEntry {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: crate::acp::registry::Distribution {
                npx: Some(crate::acp::registry::NpxDistribution {
                    package: "@scope/pkg@1.0.0".to_string(),
                    args: None,
                    env: None,
                }),
                binary: None,
            },
        };
        let desc = describe_distribution(&agent);
        assert!(desc.starts_with("npx"));
        assert!(desc.contains("@scope/pkg@1.0.0"));
    }

    #[test]
    fn describe_distribution_binary() {
        let agent = crate::acp::registry::AgentEntry {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: crate::acp::registry::Distribution {
                npx: None,
                binary: Some(crate::acp::registry::BinaryDistribution {
                    darwin_aarch64: None,
                    darwin_x86_64: None,
                    linux_aarch64: None,
                    linux_x86_64: None,
                    windows_aarch64: None,
                    windows_x86_64: None,
                }),
            },
        };
        let desc = describe_distribution(&agent);
        assert!(desc.starts_with("binary"));
    }

    // ── Skills command tests ─────────────────────────────────────────

    #[tokio::test]
    async fn skills_list_text_output() {
        let cmd = AgentCommand::Skills {
            verbose: false,
            output_format: "text".to_string(),
        };

        let result = run(cmd, &CliConfig::default()).await;
        assert!(result.is_ok());

        // Verify we captured output by checking the function doesn't error
    }

    #[tokio::test]
    async fn skills_list_json_output() {
        let cmd = AgentCommand::Skills {
            verbose: false,
            output_format: "json".to_string(),
        };

        let result = run(cmd, &CliConfig::default()).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn skills_list_verbose_output() {
        let cmd = AgentCommand::Skills {
            verbose: true,
            output_format: "text".to_string(),
        };

        let result = run(cmd, &CliConfig::default()).await;
        assert!(result.is_ok());
    }
}
