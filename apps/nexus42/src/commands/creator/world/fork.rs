//! World fork surface — `creator world fork create|list`
//! (V1.175 P1 Task 1, group 5).
//!
//! Direct-core leaves (v1.193 P0-T3 — the daemon HTTP leaves are gone):
//! - `fork create` → `CoreService::create_fork` (V1.162 P1 T2 capability).
//!   The fork-point event's branch is derived from the existing core
//!   timeline-events read (the event's own branch is the parent branch by
//!   construction — the capability validates exactly that) unless
//!   `--parent-branch` is given explicitly.
//! - `fork list` → **pure projection** of the existing core
//!   timeline-events read (`event_type=fork_created&status=canon`, + optional
//!   `branch_id` per F-14): canon `fork_created` markers → `{branch_id,
//!   parent_branch_id, forked_from_event_id, label}` from
//!   `extensions.fork_lineage` (V1.162 carrier B — no fork-list route exists
//!   by design; **no new read**).
//!
//! # Branch-scoped lineage (V1.162 carrier B contract)
//!
//! Lineage is stored **per branch on the marker event** (the V1.162 plan:
//! "point-lookup lineage per branch, not list-all-forks"). The timeline-events
//! read always reads a single branch (defaulting to the World's current
//! branch — `root_fork_branch_id`); the marker of a fork branch lives on that
//! fork branch, so the World's root-branch read carries no marker.
//! `fork list` therefore lists the marker(s) of ONE branch: the World's
//! current branch by default (exactly the AR-84 pinned query) or `--branch
//! <id>` for a fork branch. `fork create` prints the new `branch_id`; pass it
//! to `--branch` to read the new fork's lineage.
//!
//! Error surface (PL-5, non-zero exit): the core's typed refusals — 403 for a
//! foreign World, 404 for an unknown World, `invalid input (fork_point)` for a
//! fork point that is not on the parent branch.

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::timeline::list_timeline_events_response::{
    ListTimelineEventsResponse, TimelineEventInfo,
};
use nexus_contracts::daemon_api::worlds::{
    CreateForkRequest, CreateForkRequestLabel, CreateForkResponse,
};
use nexus_core::{CoreService, CoreTimelineEventsQuery, Principal};
use serde::Serialize;

/// Max timeline page size honored by the events read.
const MAX_PAGE_LIMIT: u32 = 100;

/// `creator world fork` subcommands.
#[derive(Debug, Subcommand)]
pub enum ForkCommand {
    /// Create a timeline fork from a fork-point event.
    Create {
        /// World ID (wld_...).
        world_id: String,
        /// Fork-point event ID (the canon event the new branch diverges from).
        #[arg(long, value_name = "EVENT_ID")]
        fork_point: String,
        /// Label for the new branch (defaults to `fork`).
        #[arg(long)]
        label: Option<String>,
        /// Parent branch ID override. Normally derived from the fork-point
        /// event's branch; pass when the derivation cannot find the event.
        #[arg(long, value_name = "BRANCH_ID")]
        parent_branch: Option<String>,
        /// Emit machine-readable JSON (the `CreateForkResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List fork markers of one branch (timeline-events projection).
    ///
    /// Lineage is branch-scoped (V1.162 carrier B): a fork branch carries
    /// exactly one canon `fork_created` marker; the World's current
    /// (root) branch carries none. Reads the World's current branch by
    /// default; pass `--branch` (e.g. the `branch_id` printed by
    /// `fork create`) to read a fork branch's marker.
    List {
        /// World ID (wld_...).
        world_id: String,
        /// Branch ID to read the fork marker of (default: the World's
        /// current branch — the read's own default).
        #[arg(long, value_name = "BRANCH_ID")]
        branch: Option<String>,
        /// Emit machine-readable JSON (projected markers) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run a `creator world fork` subcommand.
///
/// Opens the direct-writer core, resolves the admitted principal, and closes
/// the writer before this command reports — on success and on failure.
///
/// # Errors
///
/// Returns `CliError` for the active creator/workspace being unset, the core's
/// refusal (403 foreign World, 404 unknown World, `invalid input (fork_point)`),
/// or a named `CliError::Other` when the fork-point event cannot be resolved
/// for parent-branch derivation.
pub async fn run(cmd: ForkCommand, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match cmd {
            ForkCommand::Create {
                world_id,
                fork_point,
                label,
                parent_branch,
                json,
            } => {
                fork_create(
                    &core,
                    &principal,
                    &world_id,
                    &fork_point,
                    label.as_deref(),
                    parent_branch.as_deref(),
                    json,
                )
                .await
            }
            ForkCommand::List {
                world_id,
                branch,
                json,
            } => fork_list(&core, &principal, &world_id, branch.as_deref(), json).await,
        }
    }
    .await;
    // The leaves return their report; nothing reaches stdout until the shared
    // seam released the writer, so a close that did not settle is never
    // reported as a created/listed fork.
    if let Some(text) = finish_direct(&core, outcome).await? {
        println!("{text}");
    }
    Ok(())
}

/// Read one bounded timeline-events page for an owned World through the core.
///
/// # Errors
///
/// Returns the core's typed refusal: 404 for an unknown World, 403 when the
/// active creator does not own it, `invalid input` for a malformed cursor or
/// status filter, and a storage error otherwise.
async fn timeline_page(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    event_type: Option<&str>,
    status: Option<&str>,
    branch: Option<&str>,
) -> Result<ListTimelineEventsResponse> {
    core.list_timeline_events(
        principal,
        world_id.to_string(),
        CoreTimelineEventsQuery {
            branch_id: branch.map(str::to_string),
            status: status.map(str::to_string),
            event_type: event_type.map(str::to_string),
            limit: Some(MAX_PAGE_LIMIT),
            cursor: None,
        },
    )
    .await
    .map_err(map_core_error)
}

/// Resolve the parent branch for a fork-point event.
///
/// The fork-point event must live on the parent branch (the core capability
/// validates exactly that), so the event's own `branch_id` is the parent
/// branch by construction. Looked up through the existing timeline-events read
/// (canon page first, then provisional) — a pure projection, never a new read.
///
/// # Errors
///
/// Returns a named `CliError::Other` when the fork-point event is not found on
/// the first canon or provisional page, with `--parent-branch` remediation.
async fn resolve_parent_branch(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    fork_point: &str,
    explicit: Option<&str>,
) -> Result<String> {
    if let Some(branch) = explicit {
        return Ok(branch.to_string());
    }
    for status in ["canon", "provisional"] {
        let page = timeline_page(core, principal, world_id, None, Some(status), None).await?;
        if let Some(branch_id) = page
            .items
            .iter()
            .find(|evt| evt.id == fork_point)
            .map(|evt| evt.branch_id.clone())
        {
            return Ok(branch_id);
        }
    }
    Err(CliError::Other(format!(
        "fork-point event '{fork_point}' not found in the timeline of world '{world_id}' \
         (canon/provisional first page; the timeline read is single-branch). Pass \
         --parent-branch <branch-id> to target a specific branch explicitly."
    )))
}

/// `creator world fork create <world_id> --fork-point <event_id>` —
/// create a new timeline fork through the core.
///
/// Returns the human/`--json` report `run` prints once the writer settled.
///
/// # Errors
///
/// Returns a named `CliError::Other` when the fork-point cannot be resolved, or
/// the core's typed refusal (403 foreign World, `invalid input (fork_point)`).
async fn fork_create(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    fork_point: &str,
    label: Option<&str>,
    parent_branch: Option<&str>,
    json: bool,
) -> Result<Option<String>> {
    let parent_branch_id =
        resolve_parent_branch(core, principal, world_id, fork_point, parent_branch).await?;
    let label = label
        .map(|l| {
            l.parse::<CreateForkRequestLabel>()
                .map_err(|e| CliError::Other(format!("--label: {e}")))
        })
        .transpose()?;
    let request = CreateForkRequest {
        forked_from_event_id: fork_point.to_string(),
        parent_branch_id,
        label,
    };
    let resp: CreateForkResponse = core
        .create_fork(principal, world_id.to_string(), request)
        .await
        .map_err(map_core_error)?;
    Ok(Some(if json {
        serde_json::to_string_pretty(&resp)?
    } else {
        format!(
            "Fork created:\n  branch_id:        {}\n  parent_branch_id: {}\n  \
             fork-point:       {}\n  created_at:       {}",
            resp.branch_id, resp.parent_branch_id, resp.forked_from_event_id, resp.created_at
        )
    }))
}

/// A projected fork marker (V1.162 carrier B lineage).
#[derive(Debug, Serialize)]
struct ForkMarker {
    /// The fork branch (`fbk_...`).
    branch_id: String,
    /// The branch the fork diverged from.
    parent_branch_id: String,
    /// The fork-point event on the parent branch.
    forked_from_event_id: String,
    /// Fork label (defaults to `fork`).
    label: String,
}

/// Project one canon `fork_created` marker into [`ForkMarker`].
///
/// Markers without a parseable `extensions.fork_lineage` are skipped
/// (they are not forks in the lineage sense).
fn fork_marker(evt: &TimelineEventInfo) -> Option<ForkMarker> {
    let lineage = evt.extensions.as_ref()?.get("fork_lineage")?;
    Some(ForkMarker {
        branch_id: evt.branch_id.clone(),
        parent_branch_id: lineage.get("parent_branch_id")?.as_str()?.to_string(),
        forked_from_event_id: lineage.get("forked_from_event_id")?.as_str()?.to_string(),
        label: lineage.get("label")?.as_str()?.to_string(),
    })
}

/// `creator world fork list <world_id> [--branch <branch_id>]` — project
/// canon `fork_created` markers into
/// `{branch_id, parent_branch_id, forked_from_event_id, label}`.
///
/// Pure projection of the existing timeline-events read (V1.162 carrier B
/// — no fork-list route exists by design; **no new read**). The read targets a
/// single branch: the World's current branch by default (the AR-84 pinned
/// query verbatim) or `--branch` when given. Returns the report `run` prints
/// once the writer settled.
///
/// # Errors
///
/// Returns the core's typed refusal (404 unknown World, 403 foreign World) or
/// `CliError` if the JSON serialization fails.
async fn fork_list(
    core: &CoreService,
    principal: &Principal,
    world_id: &str,
    branch: Option<&str>,
    json: bool,
) -> Result<Option<String>> {
    let page = timeline_page(
        core,
        principal,
        world_id,
        Some("fork_created"),
        Some("canon"),
        branch,
    )
    .await?;
    let markers: Vec<ForkMarker> = page.items.iter().filter_map(fork_marker).collect();

    Ok(Some(if json {
        serde_json::to_string_pretty(&markers)?
    } else if markers.is_empty() {
        if branch.is_some() {
            format!(
                "No fork marker on branch {} of {world_id}.",
                branch.unwrap_or("")
            )
        } else {
            "No fork marker on the world's current branch (root branches carry no \
             marker — V1.162 carrier B is branch-scoped). Pass --branch <branch-id> \
             (the id printed by `fork create`) to read a fork branch's lineage."
                .to_string()
        }
    } else {
        let mut lines = vec![
            format!("Fork markers for {world_id}:"),
            format!(
                "{:<20} {:<20} {:<20} LABEL",
                "BRANCH_ID", "PARENT_BRANCH", "FORK-POINT"
            ),
        ];
        for marker in &markers {
            lines.push(format!(
                "{:<20} {:<20} {:<20} {}",
                marker.branch_id,
                marker.parent_branch_id,
                marker.forked_from_event_id,
                marker.label
            ));
        }
        lines.push(format!("\n{} fork marker(s)", markers.len()));
        lines.join("\n")
    }))
}
