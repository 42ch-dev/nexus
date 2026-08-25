//! World fork surface — `creator world fork create|list`
//! (V1.175 P1 Task 1, group 5).
//!
//! Thin daemon-HTTP leaves (AR-83 #1 / AR-84 group 5):
//! - `fork create` → existing `POST /v1/daemon/worlds/:world_id/forks`
//!   (V1.162 P1 T2). The fork-point event's branch is derived from the
//!   existing timeline-events read (the event's own branch is the parent
//!   branch by construction — the daemon capability validates exactly
//!   that) unless `--parent-branch` is given explicitly.
//! - `fork list` → **pure projection** of the existing
//!   `GET /v1/daemon/worlds/:world_id/timeline/events` read
//!   (`event_type=fork_created&status=canon`, + optional `branch_id` per
//!   F-14): canon `fork_created` markers → `{branch_id, parent_branch_id,
//!   forked_from_event_id, label}` from `extensions.fork_lineage`
//!   (V1.162 carrier B — no fork-list route exists by design; **no new
//!   read route**).
//!
//! # Branch-scoped lineage (V1.162 carrier B contract)
//!
//! Lineage is stored **per branch on the marker event** (V1.162 plan:
//! "point-lookup lineage per branch, not list-all-forks"). The timeline-
//! events route always reads a single branch (defaulting to the World's
//! current branch — `root_fork_branch_id`); the marker of a fork branch
//! lives on that fork branch, so the World's root-branch read carries no
//! marker. `fork list` therefore lists the marker(s) of ONE branch:
//! the World's current branch by default (exactly the AR-84 pinned query)
//! or `--branch <id>` for a fork branch. `fork create` prints the new
//! `branch_id`; pass it to `--branch` to read the new fork's lineage.
//!
//! Error surface: 403 foreign world and 422 bad fork-point surface as
//! named daemon errors via `DaemonClient::parse_error_response` (non-zero
//! exit, PL-5).

use crate::api::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::timeline::list_timeline_events_response::{
    ListTimelineEventsResponse, TimelineEventInfo,
};
use nexus_contracts::daemon_api::worlds::{
    CreateForkRequest, CreateForkRequestLabel, CreateForkResponse,
};
use serde::Serialize;

/// Max timeline page size honored by the events route.
const MAX_PAGE_LIMIT: &str = "100";

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
        /// current branch — the route's own default).
        #[arg(long, value_name = "BRANCH_ID")]
        branch: Option<String>,
        /// Emit machine-readable JSON (projected markers) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run a `creator world fork` subcommand.
///
/// # Errors
///
/// Returns `CliError` for daemon / network failures, or a named
/// `CliError::Other` when the fork-point event cannot be resolved for
/// parent-branch derivation.
pub async fn run(cmd: ForkCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    match cmd {
        ForkCommand::Create {
            world_id,
            fork_point,
            label,
            parent_branch,
            json,
        } => {
            fork_create(
                &client,
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
        } => fork_list(&client, &world_id, branch.as_deref(), json).await,
    }
}

/// Build a daemon path with URL-encoded query pairs (house pattern:
/// `works/mod.rs::handle_list`).
fn query_path(base: &str, pairs: &[(&str, &str)]) -> String {
    let mut url = url::Url::parse("http://localhost").expect("valid base");
    url.set_path(base);
    for (key, value) in pairs {
        url.query_pairs_mut().append_pair(key, value);
    }
    let q = url.query().unwrap_or("");
    format!("{base}?{q}")
}

/// Resolve the parent branch for a fork-point event.
///
/// The fork-point event must live on the parent branch (the daemon
/// capability validates exactly that), so the event's own `branch_id` is
/// the parent branch by construction. Looked up through the existing
/// timeline-events read (canon page first, then provisional) — a pure
/// projection, never a new route.
///
/// # Errors
///
/// Returns a named `CliError::Other` when the fork-point event is not
/// found on the first canon or provisional page, with `--parent-branch`
/// remediation.
async fn resolve_parent_branch(
    client: &DaemonClient,
    world_id: &str,
    fork_point: &str,
    explicit: Option<&str>,
) -> Result<String> {
    if let Some(branch) = explicit {
        return Ok(branch.to_string());
    }
    for status in ["canon", "provisional"] {
        let path = query_path(
            &format!("/v1/daemon/worlds/{world_id}/timeline/events"),
            &[("limit", MAX_PAGE_LIMIT), ("status", status)],
        );
        let page: ListTimelineEventsResponse = client.get(&path).await?;
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
/// create a new timeline fork (V1.162 P1 T2 route).
///
/// # Errors
///
/// Returns a named `CliError::Other` when the fork-point cannot be
/// resolved, or `CliError` for daemon / network failures (403 foreign
/// world, 422 bad fork-point, …).
async fn fork_create(
    client: &DaemonClient,
    world_id: &str,
    fork_point: &str,
    label: Option<&str>,
    parent_branch: Option<&str>,
    json: bool,
) -> Result<()> {
    let parent_branch_id =
        resolve_parent_branch(client, world_id, fork_point, parent_branch).await?;
    let label = label
        .map(|l| {
            l.parse::<CreateForkRequestLabel>()
                .map_err(|e| CliError::Other(format!("--label: {e}")))
        })
        .transpose()?;
    let req = CreateForkRequest {
        forked_from_event_id: fork_point.to_string(),
        parent_branch_id,
        label,
    };
    let resp: CreateForkResponse = client
        .post(&format!("/v1/daemon/worlds/{world_id}/forks"), &req)
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Fork created:");
        println!("  branch_id:        {}", resp.branch_id);
        println!("  parent_branch_id: {}", resp.parent_branch_id);
        println!("  fork-point:       {}", resp.forked_from_event_id);
        println!("  created_at:       {}", resp.created_at);
    }
    Ok(())
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
/// — no fork-list route exists by design; **no new read route**). The
/// route reads a single branch: the World's current branch by default
/// (the AR-84 pinned query verbatim) or `--branch` when given.
///
/// # Errors
///
/// Returns `CliError` for daemon / network failures.
async fn fork_list(
    client: &DaemonClient,
    world_id: &str,
    branch: Option<&str>,
    json: bool,
) -> Result<()> {
    let mut pairs: Vec<(&str, &str)> = vec![
        ("event_type", "fork_created"),
        ("status", "canon"),
        ("limit", MAX_PAGE_LIMIT),
    ];
    if let Some(b) = branch {
        pairs.push(("branch_id", b));
    }
    let path = query_path(
        &format!("/v1/daemon/worlds/{world_id}/timeline/events"),
        &pairs,
    );
    let page: ListTimelineEventsResponse = client.get(&path).await?;
    let markers: Vec<ForkMarker> = page.items.iter().filter_map(fork_marker).collect();

    if json {
        println!("{}", serde_json::to_string_pretty(&markers)?);
    } else if markers.is_empty() {
        if branch.is_some() {
            println!(
                "No fork marker on branch {} of {world_id}.",
                branch.unwrap_or("")
            );
        } else {
            println!(
                "No fork marker on the world's current branch (root branches carry no \
                 marker — V1.162 carrier B is branch-scoped). Pass --branch <branch-id> \
                 (the id printed by `fork create`) to read a fork branch's lineage."
            );
        }
    } else {
        println!("Fork markers for {world_id}:");
        println!(
            "{:<20} {:<20} {:<20} LABEL",
            "BRANCH_ID", "PARENT_BRANCH", "FORK-POINT"
        );
        for marker in &markers {
            println!(
                "{:<20} {:<20} {:<20} {}",
                marker.branch_id,
                marker.parent_branch_id,
                marker.forked_from_event_id,
                marker.label
            );
        }
        println!("\n{} fork marker(s)", markers.len());
    }
    Ok(())
}
