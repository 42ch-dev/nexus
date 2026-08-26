//! Memory management commands.
//!
//! CRUD operations for long-term memories, review pipeline,
//! and fragment management.

use crate::api::daemon_client::DaemonClient;
use crate::config;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_contracts::daemon_api::memory::ReviewResponse;
use nexus_creator_memory::long_term_memory::LongTermMemory;
use nexus_creator_memory::memory_io;
use std::io::Write;
use std::str::FromStr;

#[derive(Debug, Subcommand)]
pub enum MemoryCommand {
    /// List all long-term memories for current creator
    List,

    /// Create a new long-term memory
    Create {
        /// Memory slug (filename, path-safe)
        slug: String,

        /// Memory kind (`story_summary`, `research_material`, etc.)
        #[arg(long, default_value = "custom")]
        kind: String,

        /// Initial content (if empty, opens editor)
        #[arg(long)]
        content: Option<String>,
    },

    /// Show a specific memory
    Show { slug: String },

    /// Edit an existing memory (opens in editor)
    Edit { slug: String },

    /// Delete a memory
    Delete {
        slug: String,

        /// Skip confirmation
        #[arg(long)]
        force: bool,
    },

    /// Trigger review of pending queue (drains while `has_more`; cap 100 calls)
    Review {
        /// Emit machine-readable JSON (cumulative drain report) instead of
        /// human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// List memory fragments (requires daemon)
    Fragments {
        /// Emit machine-readable JSON (the `ListMemoryFragmentsResponse`
        /// DTO verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// List pending review entries for current creator (requires daemon)
    PendingList {
        /// Emit machine-readable JSON (the `ListPendingReviewsResponse`
        /// DTO verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Pending-review queue operations (requires daemon)
    Pending {
        #[command(subcommand)]
        command: PendingCommand,
    },

    /// Show details of a pending review entry (requires daemon)
    PendingShow {
        /// Pending review ID to show
        pending_id: String,
        /// Emit machine-readable JSON (the `PendingReviewInfo` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Dismiss a pending review entry without promoting (requires daemon)
    PendingDismiss {
        /// Pending review ID to dismiss
        pending_id: String,
        /// Emit machine-readable JSON (the `DeletePendingReviewResponse`
        /// DTO verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator memory pending` subcommands (AR-86).
#[derive(Debug, Subcommand)]
pub enum PendingCommand {
    /// Count pending review entries for current creator (requires daemon)
    Count {
        /// Emit machine-readable JSON (the `CountPendingReviewsResponse`
        /// DTO verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run memory command.
///
/// # Errors
///
/// Returns an error if:
/// - No active creator is set
/// - Database operations fail
/// - File I/O operations fail
pub async fn run(command: MemoryCommand, config: &CliConfig) -> Result<()> {
    let creator_id = config.active_creator_id.as_deref().ok_or_else(|| {
        crate::errors::CliError::Other(
            "No active creator set. Run `nexus42 system identity use <id>` first.".to_string(),
        )
    })?;

    match command {
        MemoryCommand::List => list(config, creator_id),
        MemoryCommand::Create {
            slug,
            kind,
            content,
        } => create(config, creator_id, &slug, &kind, content),
        MemoryCommand::Show { slug } => show(config, creator_id, &slug),
        MemoryCommand::Edit { slug } => edit(config, creator_id, &slug),
        MemoryCommand::Delete { slug, force } => delete(config, creator_id, &slug, force),
        MemoryCommand::Review { json } => review(config, creator_id, json).await,
        MemoryCommand::Fragments { json } => fragments(config, creator_id, json).await,
        MemoryCommand::PendingList { json } => pending_list(config, creator_id, json).await,
        MemoryCommand::Pending { command } => match command {
            PendingCommand::Count { json } => pending_count(config, creator_id, json).await,
        },
        MemoryCommand::PendingShow { pending_id, json } => {
            pending_show(config, creator_id, &pending_id, json).await
        }
        MemoryCommand::PendingDismiss { pending_id, json } => {
            pending_dismiss(config, creator_id, &pending_id, json).await
        }
    }
}

fn list(_config: &CliConfig, creator_id: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let slugs = memory_io::list_memories(&home, creator_id)?;

    if slugs.is_empty() {
        println!("No long-term memories for creator '{creator_id}'.");
        return Ok(());
    }

    println!("Long-term memories for creator '{creator_id}':\n");

    // Header
    println!("{:<30} {:<20} UPDATED_AT", "SLUG", "KIND");
    println!("{}", "-".repeat(80));

    for slug in &slugs {
        match memory_io::load_memory(&home, creator_id, slug) {
            Ok(mem) => {
                let kind = &mem.frontmatter.memory_kind;
                let updated = &mem.frontmatter.updated_at;
                println!("{slug:<30} {kind:<20} {updated}");
            }
            Err(_) => {
                println!("{:<30} {:<20} ", slug, "(unreadable)");
            }
        }
    }

    println!("\n{} memories", slugs.len());
    Ok(())
}

fn create(
    _config: &CliConfig,
    creator_id: &str,
    slug: &str,
    kind: &str,
    content: Option<String>,
) -> Result<()> {
    let home = config::user_home_dir()?;

    // Validate slug
    if !nexus_creator_memory::long_term_memory::slug_is_safe(slug) {
        return Err(crate::errors::CliError::Other(format!(
            "Invalid slug '{slug}': must not contain '..', '/', '\\\\', or control characters."
        )));
    }

    // Check if memory already exists
    if memory_io::load_memory(&home, creator_id, slug).is_ok() {
        return Err(crate::errors::CliError::Other(format!(
            "Memory '{slug}' already exists for creator '{creator_id}'. Use `memory edit {slug}` to modify it."
        )));
    }

    // Validate kind
    if nexus_creator_memory::memory_item::MemoryKind::from_str(kind).is_err() {
        return Err(crate::errors::CliError::Other(format!(
            "Invalid memory kind '{}'. Valid kinds: {}",
            kind,
            nexus_creator_memory::memory_item::MemoryKind::all_as_strings().join(", ")
        )));
    }

    // Get content
    let body = match content {
        Some(c) => c,
        None => open_editor_temp("Memory content", "")?,
    };

    let mut memory = LongTermMemory::new(kind);
    memory.set_body(&body);
    memory_io::save_memory(&home, creator_id, slug, &memory)?;

    println!("Memory '{slug}' created for creator '{creator_id}'.");
    println!("  Kind: {kind}");
    println!(
        "  Path: {}",
        memory_io::memory_path(&home, creator_id, slug).display()
    );
    Ok(())
}

fn show(_config: &CliConfig, creator_id: &str, slug: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let memory = memory_io::load_memory(&home, creator_id, slug)?;

    // Display frontmatter
    println!("slug: {slug}");
    println!("memory_id: {}", memory.frontmatter.memory_id);
    println!("memory_kind: {}", memory.frontmatter.memory_kind);
    println!("updated_at: {}", memory.frontmatter.updated_at);
    if !memory.frontmatter.source_session_ids.is_empty() {
        println!(
            "source_sessions: {}",
            memory.frontmatter.source_session_ids.join(", ")
        );
    }
    println!();

    // Display body
    println!("{}", memory.body);
    Ok(())
}

fn edit(_config: &CliConfig, creator_id: &str, slug: &str) -> Result<()> {
    let home = config::user_home_dir()?;
    let mut memory = memory_io::load_memory(&home, creator_id, slug)?;

    let new_body = open_editor_temp("Memory content", &memory.body)?;
    memory.set_body(&new_body);
    memory_io::save_memory(&home, creator_id, slug, &memory)?;

    println!("Memory '{slug}' updated.");
    Ok(())
}

fn delete(_config: &CliConfig, creator_id: &str, slug: &str, force: bool) -> Result<()> {
    let home = config::user_home_dir()?;

    // Verify memory exists
    memory_io::load_memory(&home, creator_id, slug)?;

    if !force {
        // S-005: Confirm deletion. Empty input (just pressing Enter) = cancel.
        println!("Delete memory '{slug}' for creator '{creator_id}'? [y/N]");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        let trimmed = input.trim();
        if trimmed.is_empty() {
            // User pressed Enter without typing anything — treat as cancel
            println!("Aborted (empty input).");
            return Ok(());
        }
        if !trimmed.eq_ignore_ascii_case("y") && !trimmed.eq_ignore_ascii_case("yes") {
            println!("Aborted.");
            return Ok(());
        }
    }

    memory_io::delete_memory(&home, creator_id, slug)?;
    println!("Memory '{slug}' deleted.");
    Ok(())
}

/// Hard cap on `POST /memory/review` drain iterations (AR-86 — matches the
/// web drain contract's bounded loop; the server processes ≤50 rows per call).
const REVIEW_DRAIN_MAX_CALLS: u32 = 100;

/// Cumulative drain outcome of [`drain_review_queue`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct DrainOutcome {
    promoted: i64,
    fragmented: i64,
    dropped: i64,
    processed: i64,
    has_more: bool,
    /// True when the loop stopped on a zero-progress call (server inspected
    /// no rows but still reported `has_more`) rather than finishing the
    /// queue or exhausting the call cap.
    stopped_zero_progress: bool,
    /// True when the loop exhausted [`REVIEW_DRAIN_MAX_CALLS`] with
    /// `has_more` still true (cap exhaustion, not zero progress).
    cap_exhausted: bool,
}

/// Fold one review response into the cumulative drain outcome.
///
/// Returns `true` when the loop should keep draining (`has_more` still true,
/// the call made progress, and the call cap is not exhausted); `false` when
/// the loop must stop (queue drained, zero-progress guard fired, or cap hit).
fn fold_review_response(outcome: &mut DrainOutcome, result: &ReviewResponse, call: u32) -> bool {
    outcome.promoted += result.promoted;
    outcome.fragmented += result.fragmented;
    outcome.dropped += result.dropped;
    outcome.processed += result.processed.unwrap_or(0);
    outcome.has_more = result.has_more.unwrap_or(false);
    if !outcome.has_more {
        return false;
    }
    // Zero-progress guard: a call that inspected no rows but still reports
    // has_more would loop forever on an unprocessable head row.
    if result.processed.unwrap_or(0) == 0 {
        outcome.stopped_zero_progress = true;
        return false;
    }
    call + 1 < REVIEW_DRAIN_MAX_CALLS
}

/// Drain the pending-review queue.
///
/// Loops `fetch` while the daemon reports `has_more == true`, accumulating a
/// cumulative `promoted/fragmented/dropped` report (AR-86 / F-16 — matches
/// the web `useReviewMemory` drain contract). Bounded by
/// [`REVIEW_DRAIN_MAX_CALLS`]; a zero-progress call (server inspected no
/// rows but still reports `has_more`) breaks the loop so an unprocessable
/// head row cannot spin forever.
async fn drain_review_queue(client: &DaemonClient, creator_id: &str) -> Result<DrainOutcome> {
    let mut outcome = DrainOutcome {
        promoted: 0,
        fragmented: 0,
        dropped: 0,
        processed: 0,
        has_more: false,
        stopped_zero_progress: false,
        cap_exhausted: false,
    };
    for call in 0..REVIEW_DRAIN_MAX_CALLS {
        let result = client.review_pending_memories(creator_id).await?;
        if !fold_review_response(&mut outcome, &result, call) {
            break;
        }
    }
    outcome.cap_exhausted = outcome.has_more && !outcome.stopped_zero_progress;
    Ok(outcome)
}

/// Project the cumulative drain outcome onto the `--json` envelope.
///
/// The stop-reason flags (`stopped_zero_progress` / `cap_exhausted`) let a
/// scripted drain retry tell an unprocessable head row from call-cap
/// exhaustion — the two states must serialize differently (qc2 W-1).
#[must_use]
fn review_json(outcome: DrainOutcome) -> serde_json::Value {
    serde_json::json!({
        "promoted": outcome.promoted,
        "fragmented": outcome.fragmented,
        "dropped": outcome.dropped,
        "processed": outcome.processed,
        "has_more": outcome.has_more,
        "stopped_zero_progress": outcome.stopped_zero_progress,
        "cap_exhausted": outcome.cap_exhausted,
    })
}

/// Project the drain outcome onto the human-output lines.
///
/// The empty-queue message is only printed when the queue is genuinely empty
/// (`processed == 0 && !has_more`); a zero-progress stop with `has_more`
/// must still surface the stuck-queue note + retry guidance (Bugbot Medium —
/// the prior `promoted + fragmented + dropped == 0` gate hid that signal).
fn review_human_lines(outcome: &DrainOutcome) -> Vec<String> {
    if outcome.processed == 0 && !outcome.has_more {
        return vec!["No pending memories to review.".to_string()];
    }
    let mut lines = vec![format!(
        "Review completed: promoted={}, fragmented={}, dropped={}",
        outcome.promoted, outcome.fragmented, outcome.dropped
    )];
    if outcome.stopped_zero_progress {
        lines.push(
            "Note: a review call made zero progress but the daemon still reported \
             `has_more`; the queue may contain an unprocessable head row. Re-run \
             `creator memory review` to retry."
                .to_string(),
        );
    } else if outcome.has_more {
        lines.push(format!(
            "Note: the queue was not fully drained within {REVIEW_DRAIN_MAX_CALLS} calls; \
             re-run `creator memory review` to continue."
        ));
    }
    lines
}

/// `creator memory review [--json]` — drain the pending-review queue.
///
/// See [`drain_review_queue`] for the drain contract.
async fn review(config: &CliConfig, creator_id: &str, json: bool) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let outcome = drain_review_queue(&client, creator_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&review_json(outcome))?);
        return Ok(());
    }
    for line in review_human_lines(&outcome) {
        println!("{line}");
    }
    Ok(())
}

async fn fragments(config: &CliConfig, creator_id: &str, json: bool) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let result = client.list_memory_fragments(creator_id).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    let rows = &result.fragments;

    if rows.is_empty() {
        println!("No memory fragments found.");
        return Ok(());
    }

    println!("Memory fragments:\n");
    println!("{:<30} {:<20} SUMMARY", "FRAGMENT_ID", "");
    println!("{}", "-".repeat(80));

    for f in rows {
        println!("{:<30} {}", f.fragment_id, f.summary);
    }

    println!("\n{} fragments", rows.len());
    Ok(())
}

async fn pending_list(config: &CliConfig, creator_id: &str, json: bool) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let result = client.list_pending_reviews(creator_id, None).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.items.is_empty() {
        println!("No pending reviews for creator '{creator_id}'.");
        return Ok(());
    }

    println!("Pending reviews for creator '{creator_id}':\n");
    println!(
        "{:<30} {:<15} {:<15} CREATED_AT",
        "PENDING_ID", "TASK_KIND", "SESSION_ID"
    );
    println!("{}", "-".repeat(100));

    for r in &result.items {
        // Truncate long IDs for display
        let pending_short = if r.pending_id.len() > 28 {
            format!("{}…", &r.pending_id[..25])
        } else {
            r.pending_id.clone()
        };
        let session_short = if r.session_id.len() > 13 {
            format!("{}…", &r.session_id[..10])
        } else {
            r.session_id.clone()
        };
        println!(
            "{:<30} {:<15} {:<15} {}",
            pending_short, r.task_kind, session_short, r.created_at
        );
    }

    println!("\n{} pending reviews", result.items.len());
    Ok(())
}

/// `creator memory pending count [--json]` — count pending review entries
/// (`GET /v1/daemon/memory/pending-review/count`, AR-86).
async fn pending_count(config: &CliConfig, creator_id: &str, json: bool) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let result = client.count_pending_reviews(creator_id).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!(
            "{} pending review(s) for creator '{creator_id}'.",
            result.count
        );
    }
    Ok(())
}

async fn pending_show(
    config: &CliConfig,
    creator_id: &str,
    pending_id: &str,
    json: bool,
) -> Result<()> {
    let client = DaemonClient::from_config(config);
    // The list endpoint is cursor-paginated at 50 rows/page; a creator with
    // more pending reviews can bury the requested ID past page 1. Walk
    // `pagination.next_cursor` until the ID is found or the pages are
    // exhausted (bounded loop — same cap convention as the review drain).
    let mut cursor: Option<String> = None;
    let entry = loop {
        let result = client
            .list_pending_reviews(creator_id, cursor.as_deref())
            .await?;
        if let Some(entry) = result
            .items
            .into_iter()
            .find(|r| r.pending_id == pending_id)
        {
            break entry;
        }
        match result.pagination.next_cursor {
            Some(next) if cursor.as_deref() != Some(next.as_str()) => cursor = Some(next),
            // No next page, or a cursor that no longer advances — the ID is
            // not on any page.
            _ => {
                return Err(crate::errors::CliError::Other(format!(
                    "Pending review '{pending_id}' not found for creator '{creator_id}'."
                )));
            }
        }
    };

    if json {
        println!("{}", serde_json::to_string_pretty(&entry)?);
        return Ok(());
    }

    println!("pending_id: {}", entry.pending_id);
    println!("session_id: {}", entry.session_id);
    println!("creator_id: {}", entry.creator_id);
    if let Some(wid) = &entry.world_id {
        println!("world_id: {wid}");
    }
    println!("task_kind: {}", entry.task_kind);
    println!("created_at: {}", entry.created_at);
    println!();
    println!("raw_digest:");
    println!("{}", entry.raw_digest);
    Ok(())
}

async fn pending_dismiss(
    config: &CliConfig,
    creator_id: &str,
    pending_id: &str,
    json: bool,
) -> Result<()> {
    let client = DaemonClient::from_config(config);
    let result = client
        .dismiss_pending_review(pending_id, creator_id)
        .await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
        return Ok(());
    }

    if result.success {
        println!("Pending review '{pending_id}' dismissed.");
    } else {
        println!("Dismiss did not succeed for '{pending_id}'.");
    }
    Ok(())
}

/// Open a temporary file in the user's $EDITOR, return the edited content.
///
/// Uses `tempfile::NamedTempFile` for automatic cleanup on drop (W-004),
/// preventing temp file leaks if the process crashes or the editor exits
/// abnormally.
fn open_editor_temp(prefix: &str, initial_content: &str) -> Result<String> {
    let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());

    let safe_prefix = prefix.to_lowercase().replace(' ', "-");
    let file_name = format!(
        "nexus42-{}-{}.md",
        safe_prefix,
        uuid::Uuid::new_v4().simple()
    );

    // Use tempfile::NamedTempFile for automatic cleanup on drop (W-004).
    // The file persists long enough for the editor to read it, but is
    // automatically deleted when the NamedTempFile goes out of scope.
    let mut temp_file = tempfile::NamedTempFile::with_prefix(file_name)
        .map_err(|e| crate::errors::CliError::Other(format!("Failed to create temp file: {e}")))?;
    temp_file
        .write_all(initial_content.as_bytes())
        .map_err(|e| crate::errors::CliError::Other(format!("Failed to write temp file: {e}")))?;

    let temp_path = temp_file.path().to_path_buf();

    let status = std::process::Command::new(&editor)
        .arg(&temp_path)
        .status()
        .map_err(|e| {
            crate::errors::CliError::Other(format!("Failed to open editor {editor}: {e}"))
        })?;

    if !status.success() {
        // NamedTempFile auto-deletes on drop — no manual cleanup needed
        return Err(crate::errors::CliError::Other(format!(
            "Editor {editor} exited with non-zero status."
        )));
    }

    let content = std::fs::read_to_string(&temp_path)?;
    // NamedTempFile auto-deletes on drop — no manual cleanup needed
    Ok(content)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_command_enum_exists() {
        let _ = MemoryCommand::List;
        let _ = MemoryCommand::Create {
            slug: "test".to_string(),
            kind: "custom".to_string(),
            content: None,
        };
        let _ = MemoryCommand::Show {
            slug: "test".to_string(),
        };
        let _ = MemoryCommand::Edit {
            slug: "test".to_string(),
        };
        let _ = MemoryCommand::Delete {
            slug: "test".to_string(),
            force: false,
        };
        let _ = MemoryCommand::Review { json: false };
        let _ = MemoryCommand::Fragments { json: false };
        let _ = MemoryCommand::PendingList { json: false };
        let _ = MemoryCommand::Pending {
            command: PendingCommand::Count { json: false },
        };
        let _ = MemoryCommand::PendingShow {
            pending_id: "pending_test".to_string(),
            json: false,
        };
    }

    // ── review drain loop (AR-86) ───────────────────────────────────────

    fn review_response(promoted: i64, processed: i64, has_more: bool) -> ReviewResponse {
        ReviewResponse {
            promoted,
            fragmented: 0,
            dropped: 0,
            has_more: Some(has_more),
            processed: Some(processed),
        }
    }

    #[test]
    fn fold_stops_on_zero_progress_with_has_more() {
        // A call that inspects no rows but still reports `has_more` must
        // stop the loop (an unprocessable head row cannot spin forever)
        // and be reported as a zero-progress stop, not cap exhaustion.
        let mut outcome = DrainOutcome {
            promoted: 0,
            fragmented: 0,
            dropped: 0,
            processed: 0,
            has_more: false,
            stopped_zero_progress: false,
            cap_exhausted: false,
        };
        assert!(!fold_review_response(
            &mut outcome,
            &review_response(0, 0, true),
            0
        ));
        assert!(outcome.stopped_zero_progress);
        assert!(!outcome.cap_exhausted);
        assert_eq!(outcome.processed, 0);
        assert!(outcome.has_more);
    }

    #[test]
    fn fold_continues_while_progress_and_has_more() {
        // A productive call with `has_more` keeps the loop going; the
        // cumulative counters accumulate across folds.
        let mut outcome = DrainOutcome {
            promoted: 0,
            fragmented: 0,
            dropped: 0,
            processed: 0,
            has_more: false,
            stopped_zero_progress: false,
            cap_exhausted: false,
        };
        assert!(fold_review_response(
            &mut outcome,
            &review_response(1, 1, true),
            0
        ));
        assert_eq!(outcome.promoted, 1);
        assert_eq!(outcome.processed, 1);
        assert!(!outcome.stopped_zero_progress);
        assert!(outcome.has_more);

        // `has_more = false` closes the drain.
        assert!(!fold_review_response(
            &mut outcome,
            &review_response(0, 0, false),
            1
        ));
        assert!(!outcome.has_more);
        assert_eq!(outcome.promoted, 1);
        assert_eq!(outcome.processed, 1);
    }

    #[test]
    fn fold_stops_at_call_cap() {
        // The cap check is inside the fold: the final call under the cap
        // must not ask for another iteration (call 99 is the last allowed
        // call — `call + 1 < 100` is false on the 100th call index 99).
        let mut outcome = DrainOutcome {
            promoted: 0,
            fragmented: 0,
            dropped: 0,
            processed: 0,
            has_more: false,
            stopped_zero_progress: false,
            cap_exhausted: false,
        };
        assert!(fold_review_response(
            &mut outcome,
            &review_response(1, 1, true),
            98
        ));
        assert!(!fold_review_response(
            &mut outcome,
            &review_response(1, 1, true),
            99
        ));
        assert!(outcome.has_more);
        assert!(!outcome.stopped_zero_progress);
    }

    #[test]
    fn review_json_serializes_stop_reasons_differently() {
        // qc2 W-1: a scripted `--json` drain retry must be able to tell an
        // unprocessable head row (zero-progress stop) from call-cap
        // exhaustion — the two terminal causes serialize to distinct
        // envelopes.
        let mut zero_progress = DrainOutcome {
            promoted: 0,
            fragmented: 0,
            dropped: 0,
            processed: 0,
            has_more: true,
            stopped_zero_progress: true,
            cap_exhausted: false,
        };
        assert!(!fold_review_response(
            &mut zero_progress,
            &review_response(0, 0, true),
            0
        ));

        let mut cap_hit = DrainOutcome {
            promoted: 0,
            fragmented: 0,
            dropped: 0,
            processed: 0,
            has_more: false,
            stopped_zero_progress: false,
            cap_exhausted: false,
        };
        assert!(!fold_review_response(
            &mut cap_hit,
            &review_response(1, 1, false),
            0
        ));
        cap_hit.has_more = true;
        cap_hit.cap_exhausted = cap_hit.has_more && !cap_hit.stopped_zero_progress;

        let zero_json = review_json(zero_progress);
        let cap_json = review_json(cap_hit);

        assert_eq!(zero_json["stopped_zero_progress"], true);
        assert_eq!(zero_json["cap_exhausted"], false);
        assert_eq!(cap_json["stopped_zero_progress"], false);
        assert_eq!(cap_json["cap_exhausted"], true);
        assert_ne!(zero_json, cap_json);
    }

    fn drain_outcome(
        promoted: i64,
        processed: i64,
        has_more: bool,
        stopped_zero_progress: bool,
    ) -> DrainOutcome {
        DrainOutcome {
            promoted,
            fragmented: 0,
            dropped: 0,
            processed,
            has_more,
            stopped_zero_progress,
            cap_exhausted: has_more && !stopped_zero_progress,
        }
    }

    #[test]
    fn review_human_lines_empty_message_only_when_queue_genuinely_empty() {
        // Bugbot Medium: `promoted + fragmented + dropped == 0` is not the
        // empty-queue signal — a zero-progress stop must still surface the
        // stuck-queue note + retry guidance instead of "No pending memories".
        let lines = review_human_lines(&drain_outcome(0, 0, false, false));
        assert_eq!(lines, ["No pending memories to review."]);

        let lines = review_human_lines(&drain_outcome(0, 0, true, true));
        assert_eq!(
            lines.len(),
            2,
            "stuck-queue note must not be hidden: {lines:?}"
        );
        assert!(!lines[0].contains("No pending memories"), "{lines:?}");
        assert!(
            lines[1].contains("zero progress") && lines[1].contains("retry"),
            "stuck-queue note missing: {lines:?}"
        );

        // Progress with `has_more` (cap-exhaustion stop) keeps the drain
        // summary + continue note.
        let lines = review_human_lines(&drain_outcome(3, 50, true, false));
        assert!(lines[0].contains("promoted=3"), "{lines:?}");
        assert!(lines[1].contains("not fully drained"), "{lines:?}");
    }
}
