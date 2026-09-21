//! Reading-depth data CRUD — `creator reading` (V1.175 P1 Task 1, group 3;
//! direct-core retarget v1.193 P0-T8).
//!
//! Direct-core leaves over the retained typed reading family
//! ([`CoreService::get_reading_progress`], `put_reading_progress`,
//! `delete_reading_progress`, `list_annotations`, `create_annotation`,
//! `patch_annotation`, `delete_annotation`): `progress get|set|clear` +
//! `annotation list|add|patch|remove`. **Data CRUD only** (PL-7) — this is
//! not a manuscript reader / TUI pager; the V1.79 reading surface stays
//! web. Agents and scripts export, reset, and write annotations here.
//!
//! Every leaf runs on the shared direct-call seam ([`crate::core`]): one
//! owner-scoped `CoreService` is opened, the typed request is issued, and the
//! writer is released before anything is rendered. Reading persists in the
//! selected workspace `state.db` — the same storage the retired
//! `GET/PUT/DELETE /v1/daemon/reading/progress` and
//! `GET/POST /v1/daemon/reading/annotations` routes wrote.
//!
//! Conventions: human-readable default output, `--json` emits the typed DTO
//! verbatim (generated contract types only — AR-83 #2/#3); write bodies are
//! typed long flags; core refusals surface through the shared mapper (named
//! `[code]`, non-zero exit). Flag-vocabulary validation (chapter >= 1, scroll
//! bounds, closed color enum, non-empty selected text, `--end > --start`)
//! stays here because it names the flags; the core re-checks the same
//! invariants on its own authority.

use crate::config::CliConfig;
use crate::core::{finish_direct, map_core_error, open_direct_core};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::reading::{
    ReadingAnnotation, ReadingAnnotationCreateRequest, ReadingAnnotationCreateRequestColor,
    ReadingAnnotationCreateRequestSelectedText, ReadingAnnotationListQuery,
    ReadingAnnotationPatchRequest, ReadingAnnotationPatchRequestColor, ReadingProgressQuery,
    ReadingProgressRequest, ReadingProgressResponse,
};
use nexus_core::{CoreService, Principal};
use std::fmt::Write as _;
use std::num::NonZeroU64;

/// Valid annotation highlight colors (V1.89 closed enum, core-validated).
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
        /// Work ID (wrk_...) — the core's canonical work reference.
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
        /// Work ID (wrk_...) — the core's canonical work reference.
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
        /// Work ID (wrk_...) — the core's canonical work reference.
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
        /// Work ID (wrk_...) — the core's canonical work reference.
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
        /// Work ID (wrk_...) — the core's canonical work reference.
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
/// The writer is released by [`finish_direct`] before any line is printed, so
/// a command never reports an outcome its core could not settle.
///
/// # Errors
///
/// Returns `CliError` on invalid input (chapter < 1, scroll out of range,
/// invalid color, empty selected text) or the mapped core refusal (unknown or
/// foreign work/annotation, storage failure), plus any cleanup refusal from
/// [`finish_direct`].
pub async fn run(cmd: ReadingCommand, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match cmd {
            ReadingCommand::Progress { command } => run_progress(&core, &principal, command).await,
            ReadingCommand::Annotation { command } => {
                run_annotation(&core, &principal, command).await
            }
        }
    }
    .await;
    // `None` is a settled verb that prints nothing (`--json` on a delete).
    if let Some(text) = finish_direct(&core, outcome).await? {
        println!("{text}");
    }
    Ok(())
}

async fn run_progress(
    core: &CoreService,
    principal: &Principal,
    cmd: ProgressCommand,
) -> Result<Option<String>> {
    match cmd {
        ProgressCommand::Get {
            work_id,
            chapter,
            json,
        } => progress_get(core, principal, &work_id, chapter, json).await,
        ProgressCommand::Set {
            work_id,
            chapter,
            scroll,
            json,
        } => progress_set(core, principal, &work_id, chapter, scroll, json).await,
        ProgressCommand::Clear {
            work_id,
            chapter,
            json,
        } => progress_clear(core, principal, &work_id, chapter, json).await,
    }
}

async fn run_annotation(
    core: &CoreService,
    principal: &Principal,
    cmd: AnnotationCommand,
) -> Result<Option<String>> {
    match cmd {
        AnnotationCommand::List {
            work_id,
            chapter,
            json,
        } => annotation_list(core, principal, &work_id, chapter, json).await,
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
                core,
                principal,
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
                core,
                principal,
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
        } => annotation_remove(core, principal, &annotation_id, json).await,
    }
}

// ── Shared helpers ────────────────────────────────────────────────────────

/// Parse a 1-based chapter number into the typed `NonZeroU64` key.
///
/// # Errors
///
/// Returns a named `CliError::Other` when `chapter == 0` (the reading
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
/// not one of `yellow|blue|green|pink` (the same vocabulary the core
/// validates).
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
fn render_progress(resp: &ReadingProgressResponse) -> String {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "Reading progress — {} chapter {}",
        resp.work_id, resp.chapter
    );
    let _ = writeln!(
        out,
        "  scroll: {}/{}",
        resp.scroll_progress, SCROLL_PROGRESS_MAX
    );
    let _ = write!(out, "  updated: {}", resp.updated_at);
    out
}

// ── Progress leaves ───────────────────────────────────────────────────────

/// `creator reading progress get` — read persisted scroll progress.
///
/// # Errors
///
/// Returns the mapped core refusal (404 `not_found` for an unknown or foreign
/// work) or a storage failure.
async fn progress_get(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
    chapter: u64,
    json: bool,
) -> Result<Option<String>> {
    let chapter = parse_chapter(chapter)?;
    let resp = core
        .get_reading_progress(
            principal,
            ReadingProgressQuery {
                work_id: work_id.to_string(),
                chapter,
            },
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(render_progress(&resp)))
}

/// `creator reading progress set <work_id> --chapter <n> --scroll <p>` —
/// upsert persisted scroll progress.
///
/// # Errors
///
/// Returns a named `CliError::Other` when `scroll` is outside 0–10000 or
/// `chapter` is 0, and the mapped core refusal (unknown/foreign work,
/// read-only access, storage failure) otherwise.
async fn progress_set(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
    chapter: u64,
    scroll: i64,
    json: bool,
) -> Result<Option<String>> {
    let chapter = parse_chapter(chapter)?;
    validate_scroll(scroll)?;
    let resp = core
        .put_reading_progress(
            principal,
            work_id.to_string(),
            ReadingProgressRequest {
                work_id: work_id.to_string(),
                chapter,
                scroll_progress: scroll,
            },
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(format!(
        "Saved reading progress.\n{}",
        render_progress(&resp)
    )))
}

/// `creator reading progress clear` — delete persisted scroll progress.
///
/// # Errors
///
/// Returns the mapped core refusal (unknown/foreign work, read-only access,
/// storage failure).
async fn progress_clear(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
    chapter: u64,
    json: bool,
) -> Result<Option<String>> {
    let chapter = parse_chapter(chapter)?;
    core.delete_reading_progress(
        principal,
        ReadingProgressQuery {
            work_id: work_id.to_string(),
            chapter,
        },
    )
    .await
    .map_err(map_core_error)?;
    if json {
        return Ok(None);
    }
    Ok(Some(format!(
        "Cleared reading progress for {work_id} chapter {chapter}."
    )))
}

// ── Annotation leaves ─────────────────────────────────────────────────────

/// `creator reading annotation list` — list annotations for a work + chapter.
///
/// # Errors
///
/// Returns the mapped core refusal (unknown/foreign work, storage failure).
async fn annotation_list(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
    chapter: u64,
    json: bool,
) -> Result<Option<String>> {
    let chapter = parse_chapter(chapter)?;
    let resp = core
        .list_annotations(
            principal,
            ReadingAnnotationListQuery {
                work_id: work_id.to_string(),
                chapter,
            },
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    if resp.items.is_empty() {
        return Ok(Some(format!(
            "No annotations for {work_id} chapter {chapter}."
        )));
    }
    let mut out = String::new();
    let _ = writeln!(out, "Annotations for {work_id} chapter {chapter}:");
    let _ = writeln!(
        out,
        "{:<38} {:<7} {:>6} {:>6}  SELECTED",
        "ANNOTATION_ID", "COLOR", "START", "END"
    );
    for item in &resp.items {
        let _ = writeln!(
            out,
            "{:<38} {:<7} {:>6} {:>6}  {}",
            item.annotation_id, item.color, item.start_offset, item.end_offset, *item.selected_text
        );
    }
    let _ = write!(out, "\n{} annotation(s)", resp.items.len());
    Ok(Some(out))
}

/// `creator reading annotation add` — create an annotation.
///
/// # Errors
///
/// Returns a named `CliError::Other` for a zero chapter, an invalid color,
/// empty `--selected-text`, or `--end <= --start`; the mapped core refusal
/// (unknown/foreign work, read-only access, offset/storage validation)
/// otherwise.
#[allow(clippy::too_many_arguments)]
async fn annotation_add(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
    chapter: u64,
    start: u64,
    end: u64,
    selected_text: &str,
    color: &str,
    note: Option<&str>,
    json: bool,
) -> Result<Option<String>> {
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
    let resp = core
        .create_annotation(
            principal,
            ReadingAnnotationCreateRequest {
                work_id: work_id.to_string(),
                chapter,
                color,
                start_offset: start,
                end_offset: end,
                selected_text,
                note: note.map(str::to_string),
            },
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(format!(
        "Created annotation {} ({} {}-{})",
        resp.annotation_id, resp.color, resp.start_offset, resp.end_offset
    )))
}

/// `creator reading annotation patch` — edit an annotation's color / note.
///
/// # Errors
///
/// Returns a named `CliError::Other` for an invalid `--color`; the mapped core
/// refusal (404 `not_found` for an unknown annotation, 403 for a foreign one,
/// storage failure) otherwise.
async fn annotation_patch(
    core: &CoreService,
    principal: &Principal,
    annotation_id: &str,
    color: Option<&str>,
    note: Option<&str>,
    json: bool,
) -> Result<Option<String>> {
    let color = color.map(parse_patch_color).transpose()?;
    let resp: ReadingAnnotation = core
        .patch_annotation(
            principal,
            annotation_id.to_string(),
            ReadingAnnotationPatchRequest {
                color,
                note: note.map(str::to_string),
            },
        )
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(Some(serde_json::to_string_pretty(&resp)?));
    }
    Ok(Some(format!("Updated annotation {annotation_id}.")))
}

/// `creator reading annotation remove` — delete an annotation.
///
/// # Errors
///
/// Returns the mapped core refusal (unknown/foreign annotation, read-only
/// access, storage failure).
async fn annotation_remove(
    core: &CoreService,
    principal: &Principal,
    annotation_id: &str,
    json: bool,
) -> Result<Option<String>> {
    core.delete_annotation(principal, annotation_id.to_string())
        .await
        .map_err(map_core_error)?;
    if json {
        return Ok(None);
    }
    Ok(Some(format!("Removed annotation {annotation_id}.")))
}
