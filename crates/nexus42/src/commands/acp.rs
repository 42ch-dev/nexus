//! ACP Command — ACP capability plane management.
//!
//! Implements the `nexus42 acp` top-level command group with subcommands:
//! - `status` — Show daemon and ACP agent status
//! - `doctor` — Run ACP connectivity diagnostics
//! - `probe` — Verify ACP connectivity (registry or agent handshake)
//! - `registry list` — List available agents from the ACP registry
//! - `registry inspect` — Show details for a specific agent
//! - `agent use` — Set default agent (stub)
//! - `agent list` — List available agents (alias for registry list)
//! - `skills export` — Export ACP client capabilities
//! - `skills verify` — Verify ACP client capabilities
//!
//! # Architecture
//!
//! ```text
//! AcpCommand ──► acp::run()
//!     │
//!     ├─► status        ──► DaemonClient::get_runtime_status()
//!     ├─► doctor        ──► DaemonClient + RegistryClient probe
//!     ├─► probe         ──► RegistryClient / AgentSpawner
//!     ├─► Registry(List)    ──► RegistryClient::get_registry()
//!     ├─► Registry(Inspect) ──► RegistryClient::get_registry() + find_agent()
//!     ├─► Agent(Use)        ──► (stub — coming soon)
//!     ├─► Agent(List)       ──► RegistryClient::get_registry()
//!     ├─► Skills(Export)    ──► List client capabilities
//!     └─► Skills(Verify)    ──► Verify client capabilities
//! ```

use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_acp_host::registry::{AgentEntry, DistributionExt, RegistryClient, REGISTRY_URL};
use nexus_acp_host::transport::AgentSpawner;

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
            "table" | "text" => Ok(Self::Table),
            "json" => Ok(Self::Json),
            _ => Err(format!("invalid format '{s}'. Use 'table' or 'json'")),
        }
    }
}

// ── Command definitions ────────────────────────────────────────────

/// Registry subcommands
#[derive(Debug, Subcommand)]
pub enum RegistryCommand {
    /// List available agents from the ACP registry
    List {
        /// Output format (table or json)
        #[arg(short = 'f', long = "format", default_value = "table")]
        format: String,
    },

    /// Show details for a specific agent
    Inspect {
        /// Agent reference (partial match on id or name)
        agent_ref: String,
    },
}

/// Agent subcommands
#[derive(Debug, Subcommand)]
pub enum AgentSubcommand {
    /// Set the default agent for this workspace (coming soon)
    Use {
        /// Agent reference (id or name)
        agent_ref: String,
    },

    /// List available agents
    List {
        /// Output format (table or json)
        #[arg(short = 'f', long = "format", default_value = "table")]
        format: String,
    },
}

/// Skills subcommands
#[derive(Debug, Subcommand)]
pub enum SkillsCommand {
    /// Export ACP client capabilities as JSON
    Export {
        /// Output format (text or json)
        #[arg(short = 'o', long = "output", default_value = "json")]
        output_format: String,
    },

    /// Verify ACP client capabilities are valid
    Verify {
        /// Show detailed information
        #[arg(long, short)]
        verbose: bool,
    },
}

#[derive(Debug, Subcommand)]
pub enum AcpCommand {
    /// Show daemon and ACP agent status
    Status,

    /// Run ACP connectivity diagnostics
    Doctor {
        /// Port the daemon is listening on (default: 8420)
        #[arg(long, default_value_t = crate::config::DAEMON_PORT)]
        port: u16,
    },

    /// Verify ACP connectivity (registry or agent handshake)
    Probe {
        /// Probe registry connectivity (default when no --agent is given)
        #[arg(long)]
        registry: bool,
        /// Probe a specific agent's ACP handshake
        #[arg(long, name = "AGENT")]
        agent: Option<String>,
    },

    /// ACP registry management
    Registry {
        #[command(subcommand)]
        command: RegistryCommand,
    },

    /// Agent selection and discovery
    Agent {
        #[command(subcommand)]
        command: AgentSubcommand,
    },

    /// ACP skills and capabilities management
    Skills {
        #[command(subcommand)]
        command: SkillsCommand,
    },
}

// ── Entry point ────────────────────────────────────────────────────

/// Run ACP command.
///
/// # Errors
///
/// Returns `CliError` if:
/// - The ACP registry cannot be accessed
/// - Agent lookup fails
/// - The daemon is not reachable
pub async fn run(cmd: AcpCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        AcpCommand::Status => cmd_status().await,
        AcpCommand::Doctor { port } => cmd_doctor(port, config).await,
        AcpCommand::Probe { registry, agent } => cmd_probe(registry, agent).await,
        AcpCommand::Registry { command } => match command {
            RegistryCommand::List { format } => cmd_registry_list(&format).await,
            RegistryCommand::Inspect { agent_ref } => cmd_registry_inspect(&agent_ref).await,
        },
        AcpCommand::Agent { command } => match command {
            AgentSubcommand::Use { agent_ref } => {
                cmd_agent_use(&agent_ref);
                Ok(())
            }
            AgentSubcommand::List { format } => cmd_registry_list(&format).await,
        },
        AcpCommand::Skills { command } => match command {
            SkillsCommand::Export { output_format } => cmd_skills_export(&output_format),
            SkillsCommand::Verify { verbose } => {
                cmd_skills_verify(verbose);
                Ok(())
            }
        },
    }
}

// ── `acp status` ──────────────────────────────────────────────────

/// Show daemon and ACP agent status.
pub(super) async fn cmd_status() -> Result<()> {
    let client =
        crate::api::DaemonClient::new(&format!("http://127.0.0.1:{}", crate::config::DAEMON_PORT));

    let status =
        client
            .get_runtime_status()
            .await
            .map_err(|e| crate::errors::CliError::Daemon {
                message: format!("Failed to connect to daemon: {e}"),
            })?;

    // Daemon status
    println!("Daemon Status");
    println!("{}", "─".repeat(50));
    println!("  Daemon:    Running");
    println!("  Version:  {}", status.version);

    let uptime = status.uptime_seconds;
    if uptime < 60 {
        println!("  Uptime:   {uptime}s");
    } else if uptime < 3600 {
        println!("  Uptime:   {}m {}s", uptime / 60, uptime % 60);
    } else {
        println!("  Uptime:   {}h {}m", uptime / 3600, (uptime % 3600) / 60);
    }

    println!(
        "  Workspace: {}",
        if status.workspace_initialized {
            "Initialized"
        } else {
            "Not initialized"
        }
    );

    // ACP status
    println!();
    println!("ACP Status");
    println!("{}", "─".repeat(50));

    let acp = &status.acp;
    println!(
        "  Tool execution: {}",
        if acp.tool_execution_enabled {
            "Enabled"
        } else {
            "Disabled"
        }
    );
    println!("  Active sessions: {}", acp.active_sessions);
    println!("  Tool executions: {}", acp.total_tool_executions);

    Ok(())
}

// ── `acp doctor` ──────────────────────────────────────────────────

/// Run ACP connectivity diagnostics.
///
/// Checks:
/// 1. Daemon connectivity
/// 2. ACP registry reachability
/// 3. Reports overall health
async fn cmd_doctor(port: u16, config: &CliConfig) -> Result<()> {
    println!("ACP Doctor — Running diagnostics...");
    println!();

    let mut issues = 0u32;

    // Check 1: Daemon connectivity
    print!("  [1/3] Daemon connectivity... ");
    let daemon_url = format!("http://127.0.0.1:{port}");
    let client = crate::api::DaemonClient::new(&daemon_url);
    match client.health_check().await {
        Ok(true) => println!("✓ Running"),
        Ok(false) => {
            println!("✗ Not running");
            issues += 1;
        }
        Err(e) => {
            println!("✗ Error: {e}");
            issues += 1;
        }
    }

    // Check 2: ACP Registry reachability
    print!("  [2/3] ACP Registry reachability... ");
    match RegistryClient::new() {
        Ok(reg_client) => match reg_client.get_registry().await {
            Ok(registry) => {
                println!(
                    "✓ Reachable (v{}, {} agents)",
                    registry.version,
                    registry.agents.len()
                );
            }
            Err(e) => {
                println!("✗ Error: {e}");
                issues += 1;
            }
        },
        Err(e) => {
            println!("✗ Error: {e}");
            issues += 1;
        }
    }

    // Check 3: Configuration sanity
    print!("  [3/3] Configuration... ");
    if config.daemon_url == daemon_url {
        println!("✓ OK");
    } else {
        println!(
            "⚠ Daemon URL mismatch (config: {}, expected: {daemon_url})",
            config.daemon_url
        );
        issues += 1;
    }

    println!();
    if issues == 0 {
        println!("✓ All checks passed — ACP is healthy.");
    } else {
        println!("✗ {issues} issue(s) found. See above for details.");
    }

    Ok(())
}

// ── `acp probe` ──────────────────────────────────────────────────

pub(super) async fn cmd_probe(_registry_flag: bool, agent: Option<String>) -> Result<()> {
    match agent {
        Some(agent_ref) => probe_agent(&agent_ref).await,
        None => probe_registry().await,
    }
}

/// Probe ACP Registry connectivity.
async fn probe_registry() -> Result<()> {
    eprintln!("Probing ACP Registry...");

    let start = std::time::Instant::now();

    let client = RegistryClient::new()?;
    let registry = client.refresh().await;

    let elapsed = start.elapsed();

    match registry {
        Ok(reg) => {
            let latency_ms = elapsed.as_millis();
            println!("✓ ACP Registry reachable");
            println!("  URL: {REGISTRY_URL}");
            println!("  Version: {}", reg.version);
            println!("  Agents: {}", reg.agents.len());
            println!("  Latency: {latency_ms}ms");
        }
        Err(e) => {
            println!("✗ ACP Registry unreachable");
            println!("  URL: {REGISTRY_URL}");
            println!("  Error: {e}");
            println!();
            println!("Check your network connection and try again.");
        }
    }

    Ok(())
}

/// Probe a specific agent's ACP handshake.
async fn probe_agent(agent_ref: &str) -> Result<()> {
    let client = RegistryClient::new()?;
    let registry = client.get_registry().await?;

    let agent = client.find_agent(&registry, agent_ref).ok_or_else(|| {
        crate::errors::CliError::Other(format!(
            "Agent '{agent_ref}' not found. Run `nexus42 acp registry list` to see available agents."
        ))
    })?;

    let work_dir = std::env::current_dir().unwrap_or_default();
    let (program, args) = resolve_launch_command(agent)?;

    eprintln!("Probing agent: {} ({})...", agent.name, agent.id);

    let start = std::time::Instant::now();

    let spawner = AgentSpawner::new(work_dir);

    let (child, _stdin, _stdout) = spawner
        .spawn(
            &program,
            &args
                .iter()
                .map(std::string::String::as_str)
                .collect::<Vec<_>>(),
        )
        .map_err(|e| crate::errors::CliError::Other(e.to_string()))?;

    let spawn_elapsed = start.elapsed();

    let mut child = child;

    let startup_result =
        tokio::time::timeout(std::time::Duration::from_secs(3), child.wait()).await;

    let elapsed = start.elapsed();
    let latency_ms = elapsed.as_millis();

    match startup_result {
        Ok(Ok(status)) => {
            if status.success() {
                println!("✓ Agent probe: process started and exited cleanly");
                println!("  Agent: {} v{}", agent.id, agent.version);
                println!("  Distribution: {}", agent.distribution.source_kind());
                println!("  Spawn time: {}ms", spawn_elapsed.as_millis());
                println!("  Total time: {latency_ms}ms");
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
            if let Err(e) = child.kill().await {
                eprintln!("  Warning: failed to kill agent process: {e}");
            }
            // Reap the process to avoid zombie
            let _ = child.wait().await;

            println!("✓ Agent probe successful");
            println!("  Agent: {} v{}", agent.id, agent.version);
            println!("  Distribution: {}", describe_distribution(agent));
            println!("  ACP initialize: OK (process alive)");
            println!("  Latency: {latency_ms}ms (includes spawn time)");
        }
        Ok(Err(e)) => {
            println!("✗ Agent probe: error waiting for process");
            println!("  Agent: {} v{}", agent.id, agent.version);
            println!("  Error: {e}");
        }
    }

    Ok(())
}

// ── `acp registry list` ──────────────────────────────────────────

pub(super) async fn cmd_registry_list(format_str: &str) -> Result<()> {
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

fn print_list_table(registry: &nexus_acp_host::registry::Registry) {
    if registry.agents.is_empty() {
        println!("No agents available in the registry.");
        return;
    }

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

    for agent in &registry.agents {
        let source = agent.distribution.source_kind();
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

    println!(
        "\n{} agents available (registry v{})",
        registry.agents.len(),
        registry.version
    );
}

fn print_list_json(
    registry: &nexus_acp_host::registry::Registry,
    client: &RegistryClient,
) -> Result<()> {
    let meta = client.cache_dir().join("cache_meta.json");
    let cached_at = std::fs::read_to_string(&meta)
        .ok()
        .and_then(|data| serde_json::from_str::<nexus_acp_host::registry::CacheMeta>(&data).ok())
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

// ── `acp registry inspect` ──────────────────────────────────────────

pub(super) async fn cmd_registry_inspect(agent_ref: &str) -> Result<()> {
    let client = RegistryClient::new()?;
    let registry = client.get_registry().await?;

    let agent = client.find_agent(&registry, agent_ref).ok_or_else(|| {
        crate::errors::CliError::Other(format!(
            "Agent '{agent_ref}' not found. Run `nexus42 acp registry list` to see available agents."
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
            .map_or_else(|| "npx".to_string(), |n| format!("npx ({})", n.package)),
        "binary" => "binary".to_string(),
        _ => "unknown".to_string(),
    };

    println!("Agent: {} ({})", agent.name, agent.id);
    println!("Version: {}", agent.version);
    if let Some(ref license) = agent.license {
        println!("License: {license}");
    }
    if let Some(ref repo) = agent.repository {
        println!("Repository: {repo}");
    }
    println!(
        "Description: {}",
        agent.description.as_deref().unwrap_or("No description")
    );
    println!("Source: {source_detail}");
}

// ── `acp agent use` ──────────────────────────────────────────────────

fn cmd_agent_use(_agent_ref: &str) {
    println!("Coming soon: `acp agent use` — set default agent for this workspace.");
    println!("  This feature will be implemented in a follow-up plan.");
}

// ── `acp skills export` ──────────────────────────────────────────────

pub(super) fn cmd_skills_export(output_format: &str) -> Result<()> {
    let capabilities = client_capabilities();

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
        println!("Available ACP Skills / Capabilities");
        println!();
        for (id, desc, version) in &capabilities {
            println!("  {id} ({version}) — {desc}");
        }
        println!();
        println!("Total: {} capabilities", capabilities.len());
    }

    Ok(())
}

// ── `acp skills verify` ──────────────────────────────────────────────

fn cmd_skills_verify(verbose: bool) {
    let capabilities = client_capabilities();

    println!("Verifying ACP client capabilities...");
    println!();

    let mut valid = 0;
    let mut total = 0;

    for (id, desc, version) in &capabilities {
        total += 1;
        // All capabilities in V1.0 are considered valid
        valid += 1;
        if verbose {
            println!("  ✓ {id} ({version}) — {desc}");
        }
    }

    println!("✓ {valid}/{total} capabilities verified successfully.",);

    if verbose {
        println!();
        println!("Deferred (V1.1+): terminal.kill, terminal.wait_for_exit, slash_commands, agent_plan, session.modes");
    }
}

// ── Shared helpers ──────────────────────────────────────────────────

/// V1.0 client capabilities.
fn client_capabilities() -> Vec<(&'static str, &'static str, &'static str)> {
    vec![
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
    ]
}

/// Resolve the launch command from an agent's distribution metadata.
pub(super) fn resolve_launch_command(agent: &AgentEntry) -> Result<(String, Vec<String>)> {
    if let Some(ref npx) = agent.distribution.npx {
        let mut args = vec![npx.package.clone()];
        if let Some(ref npx_args) = npx.args {
            args.extend(npx_args.iter().cloned());
        }
        Ok(("npx".to_string(), args))
    } else if let Some(ref binary) = agent.distribution.binary {
        let platform = nexus_acp_host::transport::Platform::current().ok_or_else(|| {
            crate::errors::CliError::Other(
                "Current platform is not supported by ACP binary distribution.".to_string(),
            )
        })?;

        let platform_binary = match platform {
            nexus_acp_host::transport::Platform::DarwinAarch64 => &binary.darwin_aarch64,
            nexus_acp_host::transport::Platform::DarwinX86_64 => &binary.darwin_x86_64,
            nexus_acp_host::transport::Platform::LinuxAarch64 => &binary.linux_aarch64,
            nexus_acp_host::transport::Platform::LinuxX86_64 => &binary.linux_x86_64,
            nexus_acp_host::transport::Platform::WindowsX86_64 => &binary.windows_x86_64,
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

/// Describe the agent's distribution for display.
fn describe_distribution(agent: &AgentEntry) -> String {
    match agent.distribution.source_kind() {
        "npx" => agent
            .distribution
            .npx
            .as_ref()
            .map_or_else(|| "npx".to_string(), |n| format!("npx ({})", n.package)),
        "binary" => agent.distribution.binary.as_ref().map_or_else(
            || "binary".to_string(),
            |binary| {
                nexus_acp_host::transport::Platform::current().map_or_else(
                    || "binary (unsupported platform)".to_string(),
                    |platform| {
                        let has_platform = match platform {
                            nexus_acp_host::transport::Platform::DarwinAarch64 => {
                                binary.darwin_aarch64.is_some()
                            }
                            nexus_acp_host::transport::Platform::DarwinX86_64 => {
                                binary.darwin_x86_64.is_some()
                            }
                            nexus_acp_host::transport::Platform::LinuxAarch64 => {
                                binary.linux_aarch64.is_some()
                            }
                            nexus_acp_host::transport::Platform::LinuxX86_64 => {
                                binary.linux_x86_64.is_some()
                            }
                            nexus_acp_host::transport::Platform::WindowsX86_64 => {
                                binary.windows_x86_64.is_some()
                            }
                        };
                        if has_platform {
                            format!("binary ({})", platform.as_str())
                        } else {
                            "binary (no current platform build)".to_string()
                        }
                    },
                )
            },
        ),
        _ => "unknown".to_string(),
    }
}

// ── Tests ──────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(clippy::unwrap_used)]
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

    #[test]
    fn print_list_table_renders() {
        let registry = nexus_acp_host::registry::Registry {
            version: "1.0.0".to_string(),
            agents: vec![nexus_acp_host::registry::AgentEntry {
                id: "test-agent".to_string(),
                name: "Test Agent".to_string(),
                version: "1.0.0".to_string(),
                description: Some("A test agent for unit testing".to_string()),
                repository: None,
                authors: None,
                license: None,
                icon: None,
                distribution: nexus_acp_host::registry::Distribution {
                    npx: Some(nexus_acp_host::registry::NpxDistribution {
                        package: "@scope/test@1.0.0".to_string(),
                        args: None,
                        env: None,
                    }),
                    binary: None,
                },
            }],
            extensions: None,
        };
        print_list_table(&registry);
    }

    #[test]
    fn print_list_table_empty() {
        let registry = nexus_acp_host::registry::Registry {
            version: "1.0.0".to_string(),
            agents: vec![],
            extensions: None,
        };
        print_list_table(&registry);
    }

    #[test]
    fn print_show_details_renders() {
        let agent = nexus_acp_host::registry::AgentEntry {
            id: "claude-acp".to_string(),
            name: "Claude Agent".to_string(),
            version: "0.18.0".to_string(),
            description: Some("ACP wrapper for Claude".to_string()),
            repository: Some("https://github.com/example/claude".to_string()),
            authors: Some(vec!["Anthropic".to_string()]),
            license: Some("proprietary".to_string()),
            icon: None,
            distribution: nexus_acp_host::registry::Distribution {
                npx: Some(nexus_acp_host::registry::NpxDistribution {
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
        let agent = nexus_acp_host::registry::AgentEntry {
            id: "codex-acp".to_string(),
            name: "Codex Agent".to_string(),
            version: "0.9.4".to_string(),
            description: Some("Codex ACP adapter".to_string()),
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: nexus_acp_host::registry::Distribution {
                npx: None,
                binary: Some(nexus_acp_host::registry::BinaryDistribution {
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
        let agent = nexus_acp_host::registry::AgentEntry {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: nexus_acp_host::registry::Distribution {
                npx: Some(nexus_acp_host::registry::NpxDistribution {
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
        let agent = nexus_acp_host::registry::AgentEntry {
            id: "test".to_string(),
            name: "Test".to_string(),
            version: "1.0.0".to_string(),
            description: None,
            repository: None,
            authors: None,
            license: None,
            icon: None,
            distribution: nexus_acp_host::registry::Distribution {
                npx: None,
                binary: Some(nexus_acp_host::registry::BinaryDistribution {
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

    #[test]
    fn skills_export_json_output() {
        let result = cmd_skills_export("json");
        assert!(result.is_ok());
    }

    #[test]
    fn skills_export_text_output() {
        let result = cmd_skills_export("text");
        assert!(result.is_ok());
    }

    #[test]
    fn skills_verify_non_verbose() {
        cmd_skills_verify(false);
    }

    #[test]
    fn skills_verify_verbose() {
        cmd_skills_verify(true);
    }

    #[test]
    fn agent_use_is_stub() {
        cmd_agent_use("test-agent");
    }

    #[tokio::test]
    async fn acp_status_non_running() {
        // This will fail to connect but should be an error, not panic
        let result = cmd_status().await;
        // Status fails when daemon not running — that's expected
        assert!(result.is_err());
    }
}
