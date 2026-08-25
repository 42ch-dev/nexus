//! Reading-depth data CRUD — `creator reading` (V1.175 P1 Task 1, group 3).
//!
//! Thin daemon-HTTP leaves over the existing V1.89 reading routes
//! (AR-83 #1 / AR-84 group 3): `progress get|set|clear` +
//! `annotation list|add|patch|remove`. **Data CRUD only** (PL-7) — this is
//! not a manuscript reader / TUI pager; the V1.79 reading surface stays
//! web. Agents and scripts export, reset, and write annotations here.
//!
//! Routes consumed (unchanged, daemon `api/` untouched):
//! `GET/PUT/DELETE /v1/daemon/reading/progress`,
//! `GET/POST /v1/daemon/reading/annotations`,
//! `PATCH/DELETE /v1/daemon/reading/annotations/:annotation_id`.
//!
//! Conventions: human-readable default output, `--json` emits the daemon
//! DTO verbatim (generated contract types only — AR-83 #2/#3); write
//! bodies are typed long flags; daemon error envelopes surface via
//! `DaemonClient::parse_error_response` (named `[code]`, non-zero exit).

use crate::api::DaemonClient;
use crate::commands::creator::work_utils::query_path;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::reading::{
    ReadingAnnotation, ReadingAnnotationCreateRequest, ReadingAnnotationCreateRequestColor,
    ReadingAnnotationCreateRequestSelectedText, ReadingAnnotationListResponse,
    ReadingAnnotationPatchRequest, ReadingAnnotationPatchRequestColor, ReadingProgressRequest,
    ReadingProgressResponse,
};
use std::num::NonZeroU64;

/// Valid annotation highlight colors (V1.89 closed enum, daemon-validated).
const VALID_ANNOTATION_COLORS: [&str; 4] = ["yellow", "blue", "green", "pink"];
/// Scroll-progress unit ceiling: thousandths (0–10000).
const SCROLL_PROGRESS_MAX: i64 = 10_000;

/// `creator reading` subcommands.
#[derive(Debug, Subcommand)]
pub enum ReadingCommand {
    /// Reading progress (persisted scroll position per work + chapter).
    Progress {
        #[command(subcommand)]
        command: ProgressCommand,
    },
    /// Reading annotations / highlights (per work + chapter).
    Annotation {
        #[command(subcommand)]
        command: AnnotationCommand,
    },
}

/// `creator reading progress` verbs.
#[derive(Debug, Subcommand)]
pub enum ProgressCommand {
    /// Get persisted scroll progress for a work + chapter.
    Get {
        /// Work ID (wrk_...) — the daemon's canonical work reference.
        work_id: String,
        /// Chapter number (1-based).
        #[arg(long)]
        chapter: u64,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Upsert persisted scroll progress for a work + chapter.
    Set {
        /// Work ID (wrk_...) — the daemon's canonical work reference.
        work_id: String,
        /// Chapter number (1-based).
        #[arg(long)]
        chapter: u64,
        /// Scroll position in thousandths (0–10000).
        #[arg(long)]
        scroll: i64,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Clear persisted scroll progress for a work + chapter.
    Clear {
        /// Work ID (wrk_...) — the daemon's canonical work reference.
        work_id: String,
        /// Chapter number (1-based).
        #[arg(long)]
        chapter: u64,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator reading annotation` verbs.
#[derive(Debug, Subcommand)]
pub enum AnnotationCommand {
    /// List annotations for a work + chapter.
    List {
        /// Work ID (wrk_...) — the daemon's canonical work reference.
        work_id: String,
        /// Chapter number (1-based).
        #[arg(long)]
        chapter: u64,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Add an annotation to a work + chapter.
    Add {
        /// Work ID (wrk_...) — the daemon's canonical work reference.
        work_id: String,
        /// Chapter number (1-based).
        #[arg(long)]
        chapter: u64,
        /// Start character offset (inclusive) into the chapter body text.
        #[arg(long)]
        start: u64,
        /// End character offset (exclusive); must be strictly greater than `--start`.
        #[arg(long)]
        end: u64,
        /// Selected text being annotated (must be non-empty).
        #[arg(long, value_name = "TEXT")]
        selected_text: String,
        /// Highlight color: yellow | blue | green | pink.
        #[arg(long)]
        color: String,
        /// Optional free-text note attached to the highlight.
        #[arg(long)]
        note: Option<String>,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Patch an existing annotation (color and/or note).
    Patch {
        /// Annotation ID (ann_...).
        annotation_id: String,
        /// New highlight color: yellow | blue | green | pink. At least one
        /// of --color / --note is required.
        #[arg(long, required_unless_present = "note")]
        color: Option<String>,
        /// New note. An empty string clears the note; omitting the flag
        /// leaves the note unchanged. At least one of --color / --note is
        /// required.
        #[arg(long, required_unless_present = "color")]
        note: Option<String>,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Remove an annotation.
    Remove {
        /// Annotation ID (ann_...).
        annotation_id: String,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run a `creator reading` subcommand.
///
/// # Errors
///
/// Returns `CliError` on invalid input (chapter < 1, scroll out of range,
/// invalid color, empty selected text) or any daemon API / network failure.
pub async fn run(cmd: ReadingCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    match cmd {
        ReadingCommand::Progress { command } => run_progress(&client, command).await,
        ReadingCommand::Annotation { command } => run_annotation(&client, command).await,
    }
}

async fn run_progress(client: &DaemonClient, cmd: ProgressCommand) -> Result<()> {
    match cmd {
        ProgressCommand::Get {
            work_id,
            chapter,
            json,
        } => progress_get(client, &work_id, chapter, json).await,
        ProgressCommand::Set {
            work_id,
            chapter,
            scroll,
            json,
        } => progress_set(client, &work_id, chapter, scroll, json).await,
        ProgressCommand::Clear {
            work_id,
            chapter,
            json,
        } => progress_clear(client, &work_id, chapter, json).await,
    }
}

async fn run_annotation(client: &DaemonClient, cmd: AnnotationCommand) -> Result<()> {
    match cmd {
        AnnotationCommand::List {
            work_id,
            chapter,
            json,
        } => annotation_list(client, &work_id, chapter, json).await,
        AnnotationCommand::Add {
            work_id,
            chapter,
            start,
            end,
            selected_text,
            color,
            note,
            json,
        } => {
            annotation_add(
                client,
                &work_id,
                chapter,
                start,
                end,
                &selected_text,
                &color,
                note.as_deref(),
                json,
            )
            .await
        }
        AnnotationCommand::Patch {
            annotation_id,
            color,
            note,
            json,
        } => {
            annotation_patch(
                client,
                &annotation_id,
                color.as_deref(),
                note.as_deref(),
                json,
            )
            .await
        }
        AnnotationCommand::Remove {
            annotation_id,
            json,
        } => annotation_remove(client, &annotation_id, json).await,
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────

/// Parse a 1-based chapter number into the daemon's `NonZeroU64` key.
///
/// # Errors
///
/// Returns a named `CliError::Other` when `chapter == 0` (the daemon
/// contract requires a positive chapter).
fn parse_chapter(chapter: u64) -> Result<NonZeroU64> {
    NonZeroU64::new(chapter).ok_or_else(|| CliError::Other("--chapter must be >= 1".to_string()))
}

/// Validate the scroll-progress range (thousandths, 0–10000).
///
/// # Errors
///
/// Returns a named `CliError::Other` when `scroll` is outside `0..=10000`.
fn validate_scroll(scroll: i64) -> Result<()> {
    if (0..=SCROLL_PROGRESS_MAX).contains(&scroll) {
        Ok(())
    } else {
        Err(CliError::Other(format!(
            "--scroll must be in 0..={SCROLL_PROGRESS_MAX} (thousandths), got {scroll}"
        )))
    }
}

/// Parse a create-request highlight color (V1.89 closed enum).
///
/// # Errors
///
/// Returns a named `CliError::Other` naming the valid set when `color` is
/// not one of `yellow|blue|green|pink` (same vocabulary as the daemon's
/// `invalid_input` validation).
fn parse_create_color(color: &str) -> Result<ReadingAnnotationCreateRequestColor> {
    match color {
        "yellow" => Ok(ReadingAnnotationCreateRequestColor::Yellow),
        "blue" => Ok(ReadingAnnotationCreateRequestColor::Blue),
        "green" => Ok(ReadingAnnotationCreateRequestColor::Green),
        "pink" => Ok(ReadingAnnotationCreateRequestColor::Pink),
        other => Err(CliError::Other(format!(
            "color must be one of {}, got '{other}'",
            VALID_ANNOTATION_COLORS.join(", ")
        ))),
    }
}

/// Parse a patch highlight color against the V1.89 closed enum.
///
/// # Errors
///
/// Returns a named `CliError::Other` naming the valid colors when `color`
/// is not one of `yellow|blue|green|pink`.
fn parse_patch_color(color: &str) -> Result<ReadingAnnotationPatchRequestColor> {
    match color {
        "yellow" => Ok(ReadingAnnotationPatchRequestColor::Yellow),
        "blue" => Ok(ReadingAnnotationPatchRequestColor::Blue),
        "green" => Ok(ReadingAnnotationPatchRequestColor::Green),
        "pink" => Ok(ReadingAnnotationPatchRequestColor::Pink),
        other => Err(CliError::Other(format!(
            "color must be one of {}, got '{other}'",
            VALID_ANNOTATION_COLORS.join(", ")
        ))),
    }
}

/// Render a progress DTO row for human output.
fn render_progress(resp: &ReadingProgressResponse) {
    println!(
        "Reading progress — {} chapter {}",
        resp.work_id, resp.chapter
    );
    println!("  scroll: {}/{}", resp.scroll_progress, SCROLL_PROGRESS_MAX);
    println!("  updated: {}", resp.updated_at);
}

// ── Progress leaves ───────────────────────────────────────────────────────

/// `creator reading progress get` — read persisted scroll progress.
///
/// # Errors
///
/// Returns `CliError` if the daemon rejects the request (404 unknown
/// work, field-level 400s, …) or the network fails.
pub async fn progress_get(
    client: &DaemonClient,
    work_id: &str,
    chapter: u64,
    json: bool,
) -> Result<()> {
    let chapter = parse_chapter(chapter)?;
    let path = query_path(
        "/v1/daemon/reading/progress",
        &[("work_id", work_id), ("chapter", &chapter.to_string())],
    );
    let resp: ReadingProgressResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        render_progress(&resp);
    }
    Ok(())
}

/// `creator reading progress set <work_id> --chapter <n> --scroll <p>` —
/// upsert persisted scroll progress.
///
/// # Errors
///
/// Returns a named `CliError::Other` when `scroll` is outside 0–10000 or
/// `chapter` is 0, or `CliError` for daemon / network failures.
async fn progress_set(
    client: &DaemonClient,
    work_id: &str,
    chapter: u64,
    scroll: i64,
    json: bool,
) -> Result<()> {
    let chapter = parse_chapter(chapter)?;
    validate_scroll(scroll)?;
    let req = ReadingProgressRequest {
        work_id: work_id.to_string(),
        chapter,
        scroll_progress: scroll,
    };
    let resp: ReadingProgressResponse = client.put("/v1/daemon/reading/progress", &req).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Saved reading progress.");
        render_progress(&resp);
    }
    Ok(())
}

/// `creator reading progress clear` — delete persisted scroll progress
/// (daemon returns 204 No Content).
///
/// # Errors
///
/// Returns `CliError` for the daemon / network failures.
async fn progress_clear(
    client: &DaemonClient,
    work_id: &str,
    chapter: u64,
    json: bool,
) -> Result<()> {
    let chapter = parse_chapter(chapter)?;
    let path = query_path(
        "/v1/daemon/reading/progress",
        &[("work_id", work_id), ("chapter", &chapter.to_string())],
    );
    client.delete_no_content(&path).await?;
    if !json {
        println!("Cleared reading progress for {work_id} chapter {chapter}.");
    }
    Ok(())
}

// ── Annotation leaves ─────────────────────────────────────────────────────

/// `creator reading annotation list` — list annotations for a work + chapter.
///
/// # Errors
///
/// Returns `CliError` for the daemon / network failures.
async fn annotation_list(
    client: &DaemonClient,
    work_id: &str,
    chapter: u64,
    json: bool,
) -> Result<()> {
    let chapter = parse_chapter(chapter)?;
    let path = query_path(
        "/v1/daemon/reading/annotations",
        &[("work_id", work_id), ("chapter", &chapter.to_string())],
    );
    let resp: ReadingAnnotationListResponse = client.get(&path).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.items.is_empty() {
        println!("No annotations for {work_id} chapter {chapter}.");
    } else {
        println!("Annotations for {work_id} chapter {chapter}:");
        println!(
            "{:<38} {:<7} {:>6} {:>6}  SELECTED",
            "ANNOTATION_ID", "COLOR", "START", "END"
        );
        for item in &resp.items {
            println!(
                "{:<38} {:<7} {:>6} {:>6}  {}",
                item.annotation_id,
                item.color,
                item.start_offset,
                item.end_offset,
                *item.selected_text
            );
        }
        println!("\n{} annotation(s)", resp.items.len());
    }
    Ok(())
}

/// `creator reading annotation add` — create an annotation.
///
/// # Errors
///
/// Returns a named `CliError::Other` for a zero chapter, an invalid color,
/// empty `--selected-text`, or `--end <= --start`; otherwise daemon /
/// network failures surface as `CliError`.
#[allow(clippy::too_many_arguments)]
async fn annotation_add(
    client: &DaemonClient,
    work_id: &str,
    chapter: u64,
    start: u64,
    end: u64,
    selected_text: &str,
    color: &str,
    note: Option<&str>,
    json: bool,
) -> Result<()> {
    let chapter = parse_chapter(chapter)?;
    let color = parse_create_color(color)?;
    let selected_text: ReadingAnnotationCreateRequestSelectedText = selected_text
        .parse()
        .map_err(|e| CliError::Other(format!("--selected-text: {e}")))?;
    if end <= start {
        return Err(CliError::Other(format!(
            "--end ({end}) must be strictly greater than --start ({start})"
        )));
    }
    let req = ReadingAnnotationCreateRequest {
        work_id: work_id.to_string(),
        chapter,
        color,
        start_offset: start,
        end_offset: end,
        selected_text,
        note: note.map(str::to_string),
    };
    let resp: ReadingAnnotation = client.post("/v1/daemon/reading/annotations", &req).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!(
            "Created annotation {} ({} {}-{})",
            resp.annotation_id, resp.color, resp.start_offset, resp.end_offset
        );
    }
    Ok(())
}

/// `creator reading annotation patch` — edit an annotation's color / note.
///
/// # Errors
///
/// Returns a named `CliError::Other` for an invalid `--color`; daemon /
/// network failures (including 404 unknown annotation) surface as
/// `CliError`.
async fn annotation_patch(
    client: &DaemonClient,
    annotation_id: &str,
    color: Option<&str>,
    note: Option<&str>,
    json: bool,
) -> Result<()> {
    let color = color.map(parse_patch_color).transpose()?;
    let req = ReadingAnnotationPatchRequest {
        color,
        note: note.map(str::to_string),
    };
    let path = format!("/v1/daemon/reading/annotations/{annotation_id}");
    let resp: ReadingAnnotation = client.patch(&path, &req).await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Updated annotation {annotation_id}.");
    }
    Ok(())
}

/// `creator reading annotation remove` — delete an annotation (204).
///
/// # Errors
///
/// Returns `CliError` for the daemon / network failures.
async fn annotation_remove(client: &DaemonClient, annotation_id: &str, json: bool) -> Result<()> {
    let path = format!("/v1/daemon/reading/annotations/{annotation_id}");
    client.delete_no_content(&path).await?;
    if !json {
        println!("Removed annotation {annotation_id}.");
    }
    Ok(())
}
