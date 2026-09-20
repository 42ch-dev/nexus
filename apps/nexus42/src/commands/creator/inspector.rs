//! Inspector debug group — `creator inspector` (V1.175 P1 Task 1, group 6;
//! direct-core retarget v1.193 P0-T8).
//!
//! **Hidden group** (clap `hide = true`, PL-6): the packet is a core
//! contract; a headless developer debugging assembly must reach it, but it
//! is deliberately absent from root `--help`. It is documented in
//! `.mstar/specs/cli-spec.md` and in `creator inspector --help`.
//!
//! Direct-core leaf over the retained typed seam
//! ([`CoreService::inspect_moment`], `context.rs`). The call **observes**
//! `assemble_moment` output only — the core assembles through a read-only
//! directive store, so no write, TTL burn or scene anchor touches storage;
//! the packet's `moment_directive` section is status/metadata only and never
//! carries the directive body (AC-I3). The `moment-directive` route is
//! **not** in the §5 remainder — no leaf here (the existing
//! `creator moment-directive` command tree covers it).

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::inspector::{
    moment_inspect_request::MomentInspectRequest,
    moment_inspect_request::MomentInspectRequestGenerationStage,
    moment_inspect_response::MomentInspectResponse,
};
use std::fmt::Write as _;

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
    /// Observe-only: the core assembles over a read-only directive store —
    /// it never writes and never burns directive TTL. The `moment-directive`
    /// route is intentionally not covered here (not §5 remainder).
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
/// The writer is released by [`finish_direct`] before any line is printed.
///
/// # Errors
///
/// Returns a named `CliError::Other` for an unknown `--stage`, or the mapped
/// core refusal (403 foreign world, 400 work→world binding mismatch, storage
/// failure), plus any cleanup refusal from [`finish_direct`].
pub async fn run(cmd: InspectorCommand, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match cmd {
            InspectorCommand::Moment {
                world_id,
                work,
                stage,
                json,
            } => moment(&core, &principal, &world_id, work.as_deref(), stage.as_deref(), json).await,
        }
    }
    .await;
    println!("{}", finish_direct(&core, outcome).await?);
    Ok(())
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
/// Returns a named `CliError::Other` for an unknown `--stage`, or the mapped
/// core refusal (403 foreign world, 400 work→world binding mismatch, storage
/// failure).
async fn moment(
    core: &nexus_core::CoreService,
    principal: &nexus_core::Principal,
    world_id: &str,
    work: Option<&str>,
    stage: Option<&str>,
    json: bool,
) -> Result<String> {
    let generation_stage = stage.map(parse_stage).transpose()?;
    let req = MomentInspectRequest {
        world_id: world_id.to_string(),
        work_id: work.map(str::to_string),
        generation_stage,
    };
    let resp = core
        .inspect_moment(principal, req)
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(serde_json::to_string_pretty(&resp)?);
    }
    Ok(render_packet(world_id, &resp))
}

/// Render a compact human summary of the inspector packet.
fn render_packet(world_id: &str, resp: &MomentInspectResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(out, "Inspector moment — {world_id}");
    let _ = writeln!(
        out,
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
    let _ = writeln!(
        out,
        "  modules: placement={} activation_trace={}",
        resp.modules.placement.len(),
        resp.modules.activation_trace.len()
    );
    let _ = writeln!(out, "  slot_map: {} slot(s)", resp.slot_map.len());
    let _ = write!(
        out,
        "  directive: status={} scope={}",
        resp.moment_directive.status,
        resp.moment_directive
            .scope
            .clone()
            .unwrap_or_else(|| "none".to_string())
    );
    out
}
