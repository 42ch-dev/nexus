//! Inspector debug group — `creator inspector` (V1.175 P1 Task 1, group 6).
//!
//! **Hidden group** (clap `hide = true`, PL-6): the packet is a daemon
//! contract; a headless developer debugging assembly must reach it, but it
//! is deliberately absent from root `--help`. It is documented in
//! `.mstar/specs/cli-spec.md` and in `creator inspector --help`.
//!
//! Thin daemon-HTTP leaf over the existing V1.151 observe-only route
//! `POST /v1/daemon/inspector/moment` (AR-83 #1 / AR-84 group 6). The
//! route **observes** `assemble_moment` output only — no writes. The
//! `moment-directive` route is **not** in the §5 remainder — no leaf here
//! (the existing `creator moment-directive` command tree covers it).

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::inspector::{
    moment_inspect_request::MomentInspectRequest,
    moment_inspect_request::MomentInspectRequestGenerationStage,
    moment_inspect_response::MomentInspectResponse,
};

/// Generation stages accepted by `--stage` (V1.151 enum, verbatim).
const GENERATION_STAGES: [&str; 8] = [
    "intake",
    "research",
    "produce",
    "review",
    "persist",
    "work_maintenance",
    "system_maintenance",
    "unspecified",
];

/// `creator inspector` subcommands.
#[derive(Debug, Subcommand)]
pub enum InspectorCommand {
    /// Assemble and print the moment inspector packet for an owned World.
    ///
    /// Observe-only: never writes, never burns directive TTL (read-only
    /// directive store on the daemon). The `moment-directive` route is
    /// intentionally not covered here (not §5 remainder).
    Moment {
        /// World ID (wld_...).
        world_id: String,
        /// Optional Work ID (wrk_...) — when given, the Work's binding
        /// must agree with the World.
        #[arg(long)]
        work: Option<String>,
        /// Generation stage assignment: `intake` | `research` | `produce` |
        /// `review` | `persist` | `work_maintenance` | `system_maintenance` |
        /// `unspecified`.
        #[arg(long)]
        stage: Option<String>,
        /// Emit machine-readable JSON (the `MomentInspectResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run a `creator inspector` subcommand.
///
/// # Errors
///
/// Returns a named `CliError::Other` for an unknown `--stage`, or
/// `CliError` for daemon / network failures (403 foreign world, 400
/// work→world binding mismatch, …).
pub async fn run(cmd: InspectorCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    match cmd {
        InspectorCommand::Moment {
            world_id,
            work,
            stage,
            json,
        } => moment(&client, &world_id, work.as_deref(), stage.as_deref(), json).await,
    }
}

/// Parse a generation-stage string against the V1.151 enum.
///
/// # Errors
///
/// Returns a named `CliError::Other` naming the valid stages when `stage`
/// is unknown.
fn parse_stage(stage: &str) -> Result<MomentInspectRequestGenerationStage> {
    match stage {
        "intake" => Ok(MomentInspectRequestGenerationStage::Intake),
        "research" => Ok(MomentInspectRequestGenerationStage::Research),
        "produce" => Ok(MomentInspectRequestGenerationStage::Produce),
        "review" => Ok(MomentInspectRequestGenerationStage::Review),
        "persist" => Ok(MomentInspectRequestGenerationStage::Persist),
        "work_maintenance" => Ok(MomentInspectRequestGenerationStage::WorkMaintenance),
        "system_maintenance" => Ok(MomentInspectRequestGenerationStage::SystemMaintenance),
        "unspecified" => Ok(MomentInspectRequestGenerationStage::Unspecified),
        other => Err(CliError::Other(format!(
            "invalid --stage '{other}'; expected one of {}",
            GENERATION_STAGES.join(" | ")
        ))),
    }
}

/// `creator inspector moment <world_id> [--work …] [--stage …]` — print
/// the moment inspector packet for an owned World.
///
/// # Errors
///
/// Returns a named `CliError::Other` for an unknown `--stage`, or
/// `CliError` for daemon / network failures (403 foreign world, 400
/// work→world binding mismatch, …).
async fn moment(
    client: &DaemonClient,
    world_id: &str,
    work: Option<&str>,
    stage: Option<&str>,
    json: bool,
) -> Result<()> {
    let generation_stage = stage.map(parse_stage).transpose()?;
    let req = MomentInspectRequest {
        world_id: world_id.to_string(),
        work_id: work.map(str::to_string),
        generation_stage,
    };
    let resp: MomentInspectResponse = client.post("/v1/daemon/inspector/moment", &req).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        render_packet(world_id, &resp);
    }
    Ok(())
}

/// Render a compact human summary of the inspector packet.
fn render_packet(world_id: &str, resp: &MomentInspectResponse) {
    println!("Inspector moment — {world_id}");
    println!(
        "  budget: primary={} hop={} cap={} remaining={}",
        resp.budget.primary_tokens_est,
        resp.budget.hop_tokens_est,
        resp.budget
            .cap
            .map_or_else(|| "none".to_string(), |c| c.to_string()),
        resp.budget
            .remaining
            .map_or_else(|| "none".to_string(), |r| r.to_string()),
    );
    println!(
        "  modules: placement={} activation_trace={}",
        resp.modules.placement.len(),
        resp.modules.activation_trace.len()
    );
    println!("  slot_map: {} slot(s)", resp.slot_map.len());
    println!(
        "  directive: status={} scope={}",
        resp.moment_directive.status,
        resp.moment_directive
            .scope
            .clone()
            .unwrap_or_else(|| "none".to_string())
    );
}
