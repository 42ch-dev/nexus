//! Schedule command — full 13-subcommand surface per WS7 §8.
//!
//! Each subcommand is a thin clap::Args that calls the daemon HTTP endpoint.
//! WS3 shipped `start/status/advance`; WS7 adds the remaining 10.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::{Parser, Subcommand};
use nexus_contracts::local::schedule::http::*;

// Base path for all schedule endpoints
const SCHEDULE_BASE: &str = "/v1/local/orchestration/schedules";

#[derive(Debug, Subcommand)]
pub enum ScheduleCommand {
    /// Create a new schedule
    Add {
        /// Preset ID to run (e.g. `novel-writing`)
        #[arg(long)]
        preset: String,

        /// Creator ID that owns this schedule
        #[arg(long)]
        creator: String,

        /// Seed text for core_context v0
        #[arg(long)]
        seed: Option<String>,

        /// Optional label for this schedule
        #[arg(long)]
        label: Option<String>,

        /// Schedule ID to depend on (must complete first)
        #[arg(long)]
        after: Option<String>,

        /// Allow parallel execution with another schedule
        #[arg(long)]
        parallel_with: Option<String>,

        /// Allow parallel with any sibling schedule
        #[arg(long, conflicts_with = "parallel_with")]
        parallel_any: bool,
    },

    /// Edit core_context of a schedule
    Edit {
        /// Schedule ID to edit
        id: String,

        /// Append text to core_context
        #[arg(long, group = "edit_op")]
        append: Option<String>,

        /// Replace core_context entirely
        #[arg(long, group = "edit_op")]
        replace: Option<String>,

        /// Path to JSON file for struct_merge
        #[arg(long, group = "edit_op")]
        struct_merge_file: Option<String>,

        /// Key path to remove from struct payload
        #[arg(long, group = "edit_op")]
        struct_remove: Option<String>,
    },

    /// Remove a terminal schedule
    Remove {
        /// Schedule ID to remove
        id: String,
    },

    /// List schedules
    List {
        /// Filter by creator ID
        #[arg(long)]
        creator: Option<String>,

        /// Filter by status (pending|running|paused|completed|cancelled|failed)
        #[arg(long)]
        status: Option<String>,
    },

    /// Inspect a schedule's details
    Inspect {
        /// Schedule ID to inspect
        id: String,
    },

    /// Show current core_context content
    Context {
        /// Schedule ID
        id: String,
    },

    /// Show core_context version history
    ContextHistory {
        /// Schedule ID
        id: String,

        /// Include full content for each version
        #[arg(long)]
        show_content: bool,
    },

    /// Start a pending schedule (force Pending→Running)
    Start {
        /// Schedule ID to start
        id: String,
    },

    /// Pause a schedule
    Pause {
        /// Schedule ID to pause
        id: String,
    },

    /// Resume a paused schedule
    Resume {
        /// Schedule ID to resume
        id: String,
    },

    /// Cancel a schedule
    Cancel {
        /// Schedule ID to cancel
        id: String,
    },

    /// Advance a schedule past a manual wait
    Advance {
        /// Schedule ID to advance
        id: String,
    },

    /// Show creator's schedule timeline
    Timeline {
        /// Creator ID
        #[arg(long)]
        creator: String,

        /// Number of days to look back/forward
        #[arg(long, default_value = "7")]
        days: u32,
    },
}

/// Wrapper for parsing `ScheduleCommand` in tests.
#[derive(Debug, Parser)]
#[command(subcommand_required = true, name = "schedule")]
struct ScheduleCli {
    #[command(subcommand)]
    command: ScheduleCommand,
}

/// Run the schedule command.
pub async fn run(cmd: ScheduleCommand, config: &CliConfig) -> Result<()> {
    let client = crate::api::DaemonClient::from_config(config);

    match cmd {
        ScheduleCommand::Add {
            preset,
            creator,
            seed,
            label,
            after,
            parallel_with,
            parallel_any,
        } => add_schedule(&client, &preset, &creator, seed, label, after, parallel_with, parallel_any).await,
        ScheduleCommand::Edit { id, append, replace, struct_merge_file, struct_remove } => {
            edit_schedule(&client, &id, append, replace, struct_merge_file, struct_remove).await
        }
        ScheduleCommand::Remove { id } => remove_schedule(&client, &id).await,
        ScheduleCommand::List { creator, status } => list_schedules(&client, creator, status).await,
        ScheduleCommand::Inspect { id } => inspect_schedule(&client, &id).await,
        ScheduleCommand::Context { id } => get_context(&client, &id).await,
        ScheduleCommand::ContextHistory { id, show_content: _ } => {
            get_context_history(&client, &id).await
        }
        ScheduleCommand::Start { id } => signal_schedule_cmd(&client, &id, "start").await,
        ScheduleCommand::Pause { id } => signal_schedule_cmd(&client, &id, "pause").await,
        ScheduleCommand::Resume { id } => signal_schedule_cmd(&client, &id, "resume").await,
        ScheduleCommand::Cancel { id } => signal_schedule_cmd(&client, &id, "cancel").await,
        ScheduleCommand::Advance { id } => signal_schedule_cmd(&client, &id, "advance").await,
        ScheduleCommand::Timeline { creator, days } => timeline(&client, &creator, days).await,
    }
}

// ---------------------------------------------------------------------------
// Subcommand implementations (thin HTTP wrappers)
// ---------------------------------------------------------------------------

async fn add_schedule(
    client: &crate::api::DaemonClient,
    preset: &str,
    creator: &str,
    seed: Option<String>,
    label: Option<String>,
    after: Option<String>,
    parallel_with: Option<String>,
    parallel_any: bool,
) -> Result<()> {
    let concurrency = if parallel_any {
        Some(ScheduleConcurrencyRequest::ParallelAny)
    } else if let Some(pw_id) = parallel_with {
        Some(ScheduleConcurrencyRequest::ParallelWith {
            schedule_ids: vec![pw_id],
        })
    } else {
        None
    };

    let body = AddScheduleRequest {
        creator_id: creator.to_string(),
        preset_id: preset.to_string(),
        seed,
        label,
        depends_on: after.map(|id| vec![id]),
        concurrency,
    };

    let resp: AddScheduleResponse = client.post(SCHEDULE_BASE, &body).await?;

    println!("schedule_id:    {}", resp.schedule_id);
    println!("status:         {}", resp.status);
    println!("context_version: {}", resp.core_context_version);
    Ok(())
}

async fn edit_schedule(
    client: &crate::api::DaemonClient,
    id: &str,
    append: Option<String>,
    replace: Option<String>,
    struct_merge_file: Option<String>,
    struct_remove: Option<String>,
) -> Result<()> {
    let (op, body, patch, path) = if let Some(text) = append {
        ("append".to_string(), Some(text), None, None)
    } else if let Some(text) = replace {
        ("replace".to_string(), Some(text), None, None)
    } else if let Some(file_path) = struct_merge_file {
        let json_str = std::fs::read_to_string(&file_path)?;
        let patch_val: serde_json::Value = serde_json::from_str(&json_str)?;
        ("struct_merge".to_string(), None, Some(patch_val), None)
    } else if let Some(p) = struct_remove {
        ("struct_remove".to_string(), None, None, Some(p))
    } else {
        // Should not happen due to clap group, but handle gracefully
        return Err(crate::errors::CliError::Other(
            "no edit operation specified; use --append, --replace, --struct-merge-file, or --struct-remove".to_string(),
        ));
    };

    let body_req = EditCoreContextRequest { op, body, patch, path };
    let path = format!("{SCHEDULE_BASE}/{id}/core-context");
    let resp: EditCoreContextResponse = client.patch(&path, &body_req).await?;

    println!("new_version: {}", resp.new_version);
    Ok(())
}

async fn remove_schedule(client: &crate::api::DaemonClient, id: &str) -> Result<()> {
    let path = format!("{SCHEDULE_BASE}/{id}");
    let resp: DeleteScheduleResponse = client.delete(&path).await?;

    if resp.deleted {
        println!("schedule {id} deleted");
    } else {
        println!("schedule {id} not deleted (may not be terminal)");
    }
    Ok(())
}

async fn list_schedules(
    client: &crate::api::DaemonClient,
    creator: Option<String>,
    status: Option<String>,
) -> Result<()> {
    let query = ListSchedulesQuery { creator_id: creator, status };
    // Build query string manually for GET
    let mut path = SCHEDULE_BASE.to_string();
    let mut params = Vec::new();
    if let Some(ref c) = query.creator_id {
        params.push(format!("creator_id={c}"));
    }
    if let Some(ref s) = query.status {
        params.push(format!("status={s}"));
    }
    if !params.is_empty() {
        path = format!("{path}?{}", params.join("&"));
    }

    let resp: ListSchedulesResponse = client.get(&path).await?;

    if resp.schedules.is_empty() {
        println!("No schedules found.");
        return Ok(());
    }

    println!(
        "{:<25} {:<15} {:<12} {:<10} {:<6} {}",
        "SCHEDULE_ID", "CREATOR", "PRESET", "STATUS", "CTX_V", "LABEL"
    );
    println!("{}", "-".repeat(85));
    for s in &resp.schedules {
        let label = s.label.as_deref().unwrap_or("-");
        println!(
            "{:<25} {:<15} {:<12} {:<10} {:<6} {}",
            s.schedule_id,
            s.creator_id,
            s.preset_id,
            s.status,
            s.current_core_context_version,
            label,
        );
    }
    println!("\n{} schedule(s)", resp.schedules.len());
    Ok(())
}

async fn inspect_schedule(client: &crate::api::DaemonClient, id: &str) -> Result<()> {
    let path = format!("{SCHEDULE_BASE}/{id}");
    let resp: InspectScheduleResponse = client.get(&path).await?;

    let s = &resp.schedule;
    println!("schedule_id:    {}", s.schedule_id);
    println!("creator_id:     {}", s.creator_id);
    println!("preset_id:      {}", s.preset_id);
    println!("status:         {}", s.status);
    println!("concurrency:    {}", resp.concurrency_kind);
    println!("ctx_version:    {}", s.current_core_context_version);
    println!("label:          {}", s.label.as_deref().unwrap_or("-"));
    println!("created_at:     {}", s.created_at);
    println!("updated_at:     {}", s.updated_at);
    if !resp.depends_on.is_empty() {
        println!("depends_on:     {}", resp.depends_on.join(", "));
    }
    Ok(())
}

async fn get_context(client: &crate::api::DaemonClient, id: &str) -> Result<()> {
    let path = format!("{SCHEDULE_BASE}/{id}/core-context");
    let resp: CoreContextResponse = client.get(&path).await?;

    println!("version:        {}", resp.version);
    println!("payload_kind:   {}", resp.payload_kind);
    println!("derivation:     {}", resp.derivation_kind);
    println!("created_at:     {}", resp.created_at);
    println!("content:");
    println!("{}", serde_json::to_string_pretty(&resp.content)?);
    Ok(())
}

async fn get_context_history(client: &crate::api::DaemonClient, id: &str) -> Result<()> {
    let path = format!("{SCHEDULE_BASE}/{id}/core-context-history");
    let resp: CoreContextHistoryResponse = client.get(&path).await?;

    if resp.entries.is_empty() {
        println!("No context history for schedule {id}.");
        return Ok(());
    }

    println!(
        "{:<8} {:<12} {:<20} {}",
        "VERSION", "KIND", "CREATED_AT", "CONTENT"
    );
    println!("{}", "-".repeat(60));
    for e in &resp.entries {
        let content = e
            .content
            .as_ref()
            .map(|c| serde_json::to_string(c).unwrap_or_else(|_| "-".to_string()))
            .unwrap_or_else(|| "(meta only)".to_string());
        // Truncate content for readability
        let content_display = if content.len() > 40 {
            format!("{}...", &content[..40])
        } else {
            content
        };
        println!(
            "{:<8} {:<12} {:<20} {}",
            e.version, e.derivation_kind, e.created_at, content_display
        );
    }
    println!("\n{} version(s)", resp.entries.len());
    Ok(())
}

async fn signal_schedule_cmd(
    client: &crate::api::DaemonClient,
    id: &str,
    signal: &str,
) -> Result<()> {
    let path = format!("{SCHEDULE_BASE}/{id}/signal");
    let body = SignalScheduleRequest {
        signal: signal.to_string(),
    };
    let resp: SignalScheduleResponse = client.post(&path, &body).await?;

    println!("schedule {id}: {} → {}", signal, resp.status);
    Ok(())
}

async fn timeline(
    client: &crate::api::DaemonClient,
    creator: &str,
    days: u32,
) -> Result<()> {
    // Use list with creator filter
    let path = format!("{SCHEDULE_BASE}?creator_id={creator}");
    let resp: ListSchedulesResponse = client.get(&path).await?;

    if resp.schedules.is_empty() {
        println!("No schedules for creator {creator}.");
        return Ok(());
    }

    let cutoff = chrono::Utc::now() - chrono::Duration::days(days as i64);

    println!("Creator: {creator} (last {days} days)\n");

    // Sort by created_at desc
    let mut sorted = resp.schedules;
    sorted.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    for s in &sorted {
        let ts = s.created_at.parse::<i64>().unwrap_or(0);
        let dt = chrono::DateTime::from_timestamp(ts, 0)
            .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
            .unwrap_or_else(|| s.created_at.clone());

        let is_recent = ts >= cutoff.timestamp();

        let label = s.label.as_deref().unwrap_or("-");
        let marker = if is_recent { "●" } else { "○" };

        println!(
            "{marker} [{:<10}] {:<25} {}  {} (ctx v{})",
            s.status, s.schedule_id, dt, label, s.current_core_context_version
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_command_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "add",
            "--preset",
            "novel-writing",
            "--creator",
            "c1",
            "--seed",
            "test-seed",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Add { preset, creator, seed, .. } => {
                assert_eq!(preset, "novel-writing");
                assert_eq!(creator, "c1");
                assert_eq!(seed.as_deref(), Some("test-seed"));
            }
            other => panic!("expected Add, got: {other:?}"),
        }
    }

    #[test]
    fn add_with_after_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "add",
            "--preset",
            "novel-writing",
            "--creator",
            "c1",
            "--after",
            "SCH001",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Add { after, .. } => {
                assert_eq!(after, Some("SCH001".to_string()));
            }
            other => panic!("expected Add, got: {other:?}"),
        }
    }

    #[test]
    fn add_with_parallel_any_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "add",
            "--preset",
            "novel-writing",
            "--creator",
            "c1",
            "--parallel-any",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Add { parallel_any, .. } => {
                assert!(parallel_any);
            }
            other => panic!("expected Add, got: {other:?}"),
        }
    }

    #[test]
    fn edit_append_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "edit",
            "SCH001",
            "--append",
            "more text",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Edit { id, append, .. } => {
                assert_eq!(id, "SCH001");
                assert_eq!(append, Some("more text".to_string()));
            }
            other => panic!("expected Edit, got: {other:?}"),
        }
    }

    #[test]
    fn edit_replace_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "edit",
            "SCH001",
            "--replace",
            "new content",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Edit { id, replace, .. } => {
                assert_eq!(id, "SCH001");
                assert_eq!(replace, Some("new content".to_string()));
            }
            other => panic!("expected Edit, got: {other:?}"),
        }
    }

    #[test]
    fn list_command_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "list",
            "--creator",
            "c1",
            "--status",
            "pending",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::List { creator, status } => {
                assert_eq!(creator, Some("c1".to_string()));
                assert_eq!(status, Some("pending".to_string()));
            }
            other => panic!("expected List, got: {other:?}"),
        }
    }

    #[test]
    fn inspect_command_parses() {
        let cmd = ScheduleCli::try_parse_from(["schedule", "inspect", "SCH001"]).unwrap();

        match cmd.command {
            ScheduleCommand::Inspect { id } => {
                assert_eq!(id, "SCH001");
            }
            other => panic!("expected Inspect, got: {other:?}"),
        }
    }

    #[test]
    fn context_command_parses() {
        let cmd = ScheduleCli::try_parse_from(["schedule", "context", "SCH001"]).unwrap();

        match cmd.command {
            ScheduleCommand::Context { id } => {
                assert_eq!(id, "SCH001");
            }
            other => panic!("expected Context, got: {other:?}"),
        }
    }

    #[test]
    fn context_history_command_parses() {
        let cmd =
            ScheduleCli::try_parse_from(["schedule", "context-history", "SCH001", "--show-content"])
                .unwrap();

        match cmd.command {
            ScheduleCommand::ContextHistory { id, show_content } => {
                assert_eq!(id, "SCH001");
                assert!(show_content);
            }
            other => panic!("expected ContextHistory, got: {other:?}"),
        }
    }

    #[test]
    fn start_command_parses() {
        let cmd = ScheduleCli::try_parse_from(["schedule", "start", "SCH001"]).unwrap();

        match cmd.command {
            ScheduleCommand::Start { id } => {
                assert_eq!(id, "SCH001");
            }
            other => panic!("expected Start, got: {other:?}"),
        }
    }

    #[test]
    fn pause_command_parses() {
        let cmd = ScheduleCli::try_parse_from(["schedule", "pause", "SCH001"]).unwrap();

        match cmd.command {
            ScheduleCommand::Pause { id } => {
                assert_eq!(id, "SCH001");
            }
            other => panic!("expected Pause, got: {other:?}"),
        }
    }

    #[test]
    fn resume_command_parses() {
        let cmd = ScheduleCli::try_parse_from(["schedule", "resume", "SCH001"]).unwrap();

        match cmd.command {
            ScheduleCommand::Resume { id } => {
                assert_eq!(id, "SCH001");
            }
            other => panic!("expected Resume, got: {other:?}"),
        }
    }

    #[test]
    fn cancel_command_parses() {
        let cmd = ScheduleCli::try_parse_from(["schedule", "cancel", "SCH001"]).unwrap();

        match cmd.command {
            ScheduleCommand::Cancel { id } => {
                assert_eq!(id, "SCH001");
            }
            other => panic!("expected Cancel, got: {other:?}"),
        }
    }

    #[test]
    fn advance_command_parses() {
        let cmd = ScheduleCli::try_parse_from(["schedule", "advance", "SCH001"]).unwrap();

        match cmd.command {
            ScheduleCommand::Advance { id } => {
                assert_eq!(id, "SCH001");
            }
            other => panic!("expected Advance, got: {other:?}"),
        }
    }

    #[test]
    fn remove_command_parses() {
        let cmd = ScheduleCli::try_parse_from(["schedule", "remove", "SCH001"]).unwrap();

        match cmd.command {
            ScheduleCommand::Remove { id } => {
                assert_eq!(id, "SCH001");
            }
            other => panic!("expected Remove, got: {other:?}"),
        }
    }

    #[test]
    fn timeline_command_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "timeline",
            "--creator",
            "c1",
            "--days",
            "14",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Timeline { creator, days } => {
                assert_eq!(creator, "c1");
                assert_eq!(days, 14);
            }
            other => panic!("expected Timeline, got: {other:?}"),
        }
    }
}
