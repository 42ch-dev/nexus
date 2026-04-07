//! Session Command — ACP session persistence management.
//!
//! Implements the `nexus42 session` subcommands:
//! - `list` — List persisted ACP sessions
//! - `show` — Show details for a specific session
//! - `delete` — Delete a session
//!
//! # Architecture
//!
//! ```text
//! SessionCommand ──► session::run()
//!     │
//!     ├─► list   ──► SessionManager::load_sessions() + cleanup_expired()
//!     ├─► show   ──► SessionManager::load_sessions() + find by ID
//!     └─► delete ──► SessionManager::delete_session()
//! ```

use crate::acp::{SessionEntry, SessionManager};
use crate::config::CliConfig;
use crate::errors::Result;
use chrono::{DateTime, Utc};
use clap::Subcommand;

// ── Command definitions ────────────────────────────────────────────

#[derive(Debug, Subcommand)]
pub enum SessionCommand {
    /// List persisted ACP sessions
    List {
        /// Show all sessions including expired ones (don't auto-cleanup)
        #[arg(long)]
        all: bool,
    },

    /// Show details for a specific session
    Show {
        /// Session ID (or partial match)
        session_id: String,
    },

    /// Delete a session
    Delete {
        /// Session ID to delete
        session_id: String,
    },
}

// ── Command runner ──────────────────────────────────────────────────

/// Run a session command.
pub async fn run(command: SessionCommand, _config: &CliConfig) -> Result<()> {
    let sessions_file = SessionManager::default_sessions_file();
    let manager = SessionManager::new(sessions_file);

    match command {
        SessionCommand::List { all } => run_list(&manager, all),
        SessionCommand::Show { session_id } => run_show(&manager, &session_id),
        SessionCommand::Delete { session_id } => run_delete(&manager, &session_id),
    }
}

// ── Individual command implementations ──────────────────────────────

fn run_list(manager: &SessionManager, show_all: bool) -> Result<()> {
    // Auto-cleanup expired sessions unless --all flag is set
    if !show_all {
        let removed = manager.cleanup_expired()?;
        if !removed.is_empty() {
            eprintln!("Cleaned up {} expired session(s)", removed.len());
        }
    }

    let sessions = manager.load_sessions()?;

    if sessions.is_empty() {
        println!("No sessions found.");
        return Ok(());
    }

    // Print table header
    println!(
        "{:<40} {:<15} {:<30} {:<20}",
        "SESSION ID", "AGENT", "WORKSPACE", "LAST USED"
    );
    println!("{}", "-".repeat(105));

    // Print sessions sorted by last_used_at (most recent first)
    let mut sorted_sessions = sessions;
    sorted_sessions.sort_by(|a, b| b.last_used_at.cmp(&a.last_used_at));

    for session in sorted_sessions {
        let session_id_str = session.session_id.0.as_ref();
        let session_id_display = if session_id_str.len() > 38 {
            format!("{}...", &session_id_str[..35])
        } else {
            session_id_str.to_string()
        };

        let workspace_display = if session.workspace_hint.to_string_lossy().len() > 28 {
            format!("{}...", &session.workspace_hint.to_string_lossy()[..25])
        } else {
            session.workspace_hint.to_string_lossy().to_string()
        };

        let age = format_age(session.last_used_at);

        println!(
            "{:<40} {:<15} {:<30} {:<20}",
            session_id_display, session.agent_id, workspace_display, age
        );
    }

    Ok(())
}

fn run_show(manager: &SessionManager, session_id: &str) -> Result<()> {
    let sessions = manager.load_sessions()?;

    // Find session by exact or partial match
    let session = sessions
        .iter()
        .find(|s| {
            s.session_id.0.as_ref() == session_id || s.session_id.0.as_ref().starts_with(session_id)
        })
        .ok_or_else(|| {
            crate::errors::CliError::Other(format!("Session not found: {}", session_id))
        })?;

    print_session_details(session);

    Ok(())
}

fn run_delete(manager: &SessionManager, session_id: &str) -> Result<()> {
    // Find the session first to show what we're deleting
    let sessions = manager.load_sessions()?;
    let session = sessions.iter().find(|s| {
        s.session_id.0.as_ref() == session_id || s.session_id.0.as_ref().starts_with(session_id)
    });

    let session = match session {
        Some(s) => s,
        None => {
            eprintln!("Session not found: {}", session_id);
            return Err(crate::errors::CliError::Other(format!(
                "Session not found: {}",
                session_id
            )));
        }
    };

    let actual_session_id = session.session_id.clone();

    // Delete the session
    let deleted = manager.delete_session(&actual_session_id)?;

    match deleted {
        Some(session) => {
            println!("Deleted session:");
            print_session_details(&session);
        }
        None => {
            eprintln!("Session not found: {}", session_id);
        }
    }

    Ok(())
}

// ── Helper functions ────────────────────────────────────────────────

/// Print detailed session information.
fn print_session_details(session: &SessionEntry) {
    println!("Session ID:    {}", session.session_id.0.as_ref());
    println!("Agent:         {}", session.agent_id);
    println!("Workspace:     {}", session.workspace_hint.display());
    println!("Created:       {}", format_datetime(session.created_at));
    println!(
        "Last Used:     {} ({})",
        format_datetime(session.last_used_at),
        format_age(session.last_used_at)
    );
}

/// Format a datetime for display.
fn format_datetime(dt: DateTime<Utc>) -> String {
    dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

/// Format the age of a session (e.g., "2 hours ago", "1 day ago").
fn format_age(dt: DateTime<Utc>) -> String {
    let now = Utc::now();
    let duration = now.signed_duration_since(dt);

    if duration.num_minutes() < 1 {
        "just now".to_string()
    } else if duration.num_minutes() < 60 {
        format!("{} minutes ago", duration.num_minutes())
    } else if duration.num_hours() < 24 {
        format!("{} hours ago", duration.num_hours())
    } else {
        format!("{} days ago", duration.num_days())
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn format_age_just_now() {
        let dt = Utc::now() - chrono::Duration::seconds(30);
        assert_eq!(format_age(dt), "just now");
    }

    #[test]
    fn format_age_minutes() {
        let dt = Utc::now() - chrono::Duration::minutes(5);
        assert_eq!(format_age(dt), "5 minutes ago");
    }

    #[test]
    fn format_age_hours() {
        let dt = Utc::now() - chrono::Duration::hours(3);
        assert_eq!(format_age(dt), "3 hours ago");
    }

    #[test]
    fn format_age_days() {
        let dt = Utc::now() - chrono::Duration::days(2);
        assert_eq!(format_age(dt), "2 days ago");
    }

    #[test]
    fn format_datetime_output() {
        let dt: DateTime<Utc> = "2026-04-08T15:30:00Z".parse().unwrap();
        let formatted = format_datetime(dt);
        assert!(formatted.contains("2026-04-08"));
        assert!(formatted.contains("15:30:00"));
    }
}
