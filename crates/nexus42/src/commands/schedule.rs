//! Schedule command — minimal WS3 subset.
//!
//! WS7 grows this to 13 subcommands + schedule HTTP endpoints.
//! WS3 smoke only: start returns session id, status prints current state,
//! advance resumes from WaitForInput.

use crate::config::CliConfig;
use crate::errors::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Subcommand)]
pub enum ScheduleCommand {
    /// Start a new preset session
    Start {
        /// Preset ID to run (e.g. `novel-writing`)
        preset: String,

        /// Creator ID that owns this session
        #[arg(long)]
        creator: String,

        /// Optional seed text for preset input
        #[arg(long)]
        seed: Option<String>,
    },

    /// Show current status of a session
    Status {
        /// Session ID to query
        session_id: String,
    },

    /// Advance a session past a manual wait
    Advance {
        /// Session ID to advance
        session_id: String,
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
        ScheduleCommand::Start {
            preset,
            creator,
            seed,
        } => start_session(&client, &preset, &creator, seed.as_deref()).await,
        ScheduleCommand::Status { session_id } => show_status(&client, &session_id).await,
        ScheduleCommand::Advance { session_id } => {
            advance_session(&client, &session_id).await
        }
    }
}

/// POST /v1/local/orchestration/sessions → create session, print session ID.
async fn start_session(
    client: &crate::api::DaemonClient,
    preset: &str,
    creator: &str,
    seed: Option<&str>,
) -> Result<()> {
    use nexus_contracts::local::orchestration::http::CreateSessionRequest;

    let body = CreateSessionRequest {
        preset_id: preset.to_string(),
        creator_id: creator.to_string(),
        seed: seed.map(|s| s.to_string()),
    };

    let resp: nexus_contracts::local::orchestration::http::CreateSessionResponse =
        client.post("/v1/local/orchestration/sessions", &body).await?;

    println!("{}", resp.session_id);
    Ok(())
}

/// GET /v1/local/orchestration/sessions/{session_id} → print status.
async fn show_status(client: &crate::api::DaemonClient, session_id: &str) -> Result<()> {
    use nexus_contracts::local::orchestration::http::GetSessionResponse;

    let path = format!("/v1/local/orchestration/sessions/{session_id}");
    let resp: GetSessionResponse = client.get(&path).await?;

    println!("session:  {}", resp.session.session_id);
    println!("preset:   {}", resp.session.preset_id);
    println!("creator:  {}", resp.session.creator_id);
    println!("status:   {}", resp.session.status);
    if let Some(task) = resp.session.current_task_id {
        println!("task:     {task}");
    }
    Ok(())
}

/// POST /v1/local/orchestration/sessions/{session_id}/signal → advance.
async fn advance_session(
    client: &crate::api::DaemonClient,
    session_id: &str,
) -> Result<()> {
    use nexus_contracts::local::orchestration::http::SignalSessionRequest;

    let path = format!("/v1/local/orchestration/sessions/{session_id}/signal");
    let body = SignalSessionRequest {
        signal: "advance".to_string(),
    };

    client.post_raw(&path, &body).await?;
    println!("Session {session_id} advanced");
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_command_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "start",
            "novel-writing",
            "--creator",
            "c1",
            "--seed",
            "test-seed",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Start {
                preset,
                creator,
                seed,
            } => {
                assert_eq!(preset, "novel-writing");
                assert_eq!(creator, "c1");
                assert_eq!(seed.as_deref(), Some("test-seed"));
            }
            other => panic!("expected Start, got: {other:?}"),
        }
    }

    #[test]
    fn status_command_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "status",
            "sess_abc123",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Status { session_id } => {
                assert_eq!(session_id, "sess_abc123");
            }
            other => panic!("expected Status, got: {other:?}"),
        }
    }

    #[test]
    fn advance_command_parses() {
        let cmd = ScheduleCli::try_parse_from([
            "schedule",
            "advance",
            "sess_abc123",
        ])
        .unwrap();

        match cmd.command {
            ScheduleCommand::Advance { session_id } => {
                assert_eq!(session_id, "sess_abc123");
            }
            other => panic!("expected Advance, got: {other:?}"),
        }
    }

    #[test]
    fn start_without_seed_parses() {
        let cmd =
            ScheduleCli::try_parse_from(["schedule", "start", "novel-writing", "--creator", "c1"])
                .unwrap();

        match cmd.command {
            ScheduleCommand::Start { seed, .. } => {
                assert!(seed.is_none());
            }
            other => panic!("expected Start, got: {other:?}"),
        }
    }
}
