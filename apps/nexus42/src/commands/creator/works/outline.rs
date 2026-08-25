//! Outline/chapter/timeline patch leaves — `creator works outline|chapter|timeline`
//! (V1.175 P1 Task 3, group 2).
//!
//! Thin daemon-HTTP leaves over the **existing** V1.72 canvas outline+timeline
//! routes (AR-83 #1 / AR-84 group 2, F-10):
//! - `GET /v1/daemon/works/:work_id/outline`
//! - `POST /v1/daemon/works/:work_id/outline/patch`
//! - `POST /v1/daemon/works/:work_id/chapters/:n/patch` — the **outline node**
//!   patch (chapter metadata exposed on the outline canvas)
//! - `POST /v1/daemon/works/:work_id/timeline/patch`
//!
//! **Route-family guard (AR-84):** `chapter patch` rides the outline **node**
//! route above — NOT the V1.65 chapter-**content** `PATCH
//! /v1/daemon/works/:work_id/chapters/:n` (a different DTO family, not §5
//! remainder). Leaf help names the distinction.
//!
//! All writes are CAS-guarded: every request carries `--base-revision` (the
//! `outline_revision` observed on the last canonical read, e.g. `outline
//! show`). A stale revision returns 409 `outline_conflict`; the CLI error
//! renders all four structured fields — `current_revision`, `node_id`,
//! `conflicting_path`, and `recovery_hint` — via
//! `DaemonClient::parse_error_response` (PL-5). `--help` documents the
//! re-read retry guidance.
//!
//! Conventions: human-readable default output, `--json` emits the daemon
//! DTO verbatim (generated contract types only — AR-83 #2/#3); write bodies
//! are typed long flags; chapter outline prose comes from `--content` or
//! `--content-file <path>`.

use crate::api::DaemonClient;
use crate::commands::creator::work_utils::read_file_bounded;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_contracts::daemon_api::canvas::outline::{
    NexusOutlinePatchChapterSet, NexusOutlinePatchChapterSetContent,
    NexusOutlinePatchChapterSetStatus, OutlinePatchChapterRequest, OutlinePatchResponse,
    OutlinePatchStructureRequest, OutlinePatchStructureRequestOperation, TimelinePatchEventRequest,
    TimelinePatchEventRequestOperation, WorkOutline,
};
use std::num::NonZeroU64;

/// Client-side cap for `--content-file` reads (qc3 S-002). Mirrors the
/// daemon's `OUTLINE_FILE_MAX_BYTES` (`api/handlers/outline.rs`) so an
/// accidentally oversized file is rejected before the full read instead of
/// being materialized and then refused server-side.
const CONTENT_FILE_MAX_BYTES: usize = 10 * 1024 * 1024;

/// `creator works outline` verbs (V1.72 canvas read + structure patch).
#[derive(Debug, Subcommand)]
pub enum OutlineCommand {
    /// Show the canonical work outline + timeline (V1.72 read model).
    ///
    /// Prints the `WorkOutline` DTO: outline revision, volumes, timeline
    /// events, foreshadows, and chapter titles. `--json` emits the DTO
    /// verbatim. The revision printed here is the `--base-revision` to pass
    /// to the patch leaves.
    Show {
        /// Work reference (wrk_...) — the daemon's canonical work id.
        work_ref: String,
        /// Emit machine-readable JSON (the `WorkOutline` DTO verbatim)
        /// instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Patch the outline structure (move a chapter, link an event, or
    /// attach a chapter to a volume).
    ///
    /// CAS-guarded: `--base-revision` must match the outline revision from
    /// `outline show`. On 409 `outline_conflict`, re-read the outline and
    /// reapply with the new revision.
    Patch {
        /// Work reference (wrk_...) — the daemon's canonical work id.
        work_ref: String,
        /// Revision observed on the last canonical read (CAS). On a 409
        /// `outline_conflict`, re-read the outline (`creator works outline
        /// show`) and reapply with the new revision.
        #[arg(long, value_name = "N")]
        base_revision: u64,
        /// Structural operation: `move_chapter`, `link_event`, or
        /// `attach_to_volume`.
        #[arg(long, value_enum)]
        op: OutlineOpArg,
        /// Target chapter number (required for `move_chapter` /
        /// `attach_to_volume`).
        #[arg(long, value_name = "N")]
        chapter: Option<u64>,
        /// Destination volume (required for `move_chapter` /
        /// `attach_to_volume`).
        #[arg(long, value_name = "N")]
        volume: Option<u64>,
        /// Timeline event id (required for `link_event`).
        #[arg(long, value_name = "EVENT_ID")]
        event: Option<String>,
        /// Chapter the event realizes (required for `link_event`).
        #[arg(long, value_name = "N")]
        target_chapter: Option<u64>,
        /// Emit machine-readable JSON (the `OutlinePatchResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator works chapter` verbs (V1.72 outline-node patch).
#[derive(Debug, Subcommand)]
pub enum ChapterCommand {
    /// Patch a chapter's outline-node metadata (title, slug, word counts,
    /// volume, status, or outline prose).
    ///
    /// **Route-family guard:** targets the outline **node** route
    /// `POST /v1/daemon/works/:work_id/chapters/:n/patch` — NOT the V1.65
    /// chapter-**content** `PATCH /v1/daemon/works/:work_id/chapters/:n`
    /// (different DTO family, not covered here).
    ///
    /// CAS-guarded: `--base-revision` must match the outline revision from
    /// `outline show`. On 409 `outline_conflict`, re-read the outline and
    /// reapply with the new revision.
    Patch {
        /// Work reference (wrk_...) — the daemon's canonical work id.
        work_ref: String,
        /// Chapter number (1-based) — the outline node to patch.
        #[arg(long, value_name = "N")]
        n: u64,
        /// Revision observed on the last canonical read (CAS). On a 409
        /// `outline_conflict`, re-read the outline (`creator works outline
        /// show`) and reapply with the new revision.
        #[arg(long, value_name = "N")]
        base_revision: u64,
        /// Display title for the chapter (UI-facing; persisted in the work
        /// outline frontmatter).
        #[arg(long)]
        title: Option<String>,
        /// Filename slug for the chapter (kebab-case, 1..=80 chars, unique
        /// within the work).
        #[arg(long)]
        slug: Option<String>,
        /// Planned word count for the chapter.
        #[arg(long, value_name = "N")]
        planned_word_count: Option<u64>,
        /// Actual word count for the chapter, if known.
        #[arg(long, value_name = "N")]
        actual_word_count: Option<u64>,
        /// Volume binding for the chapter.
        #[arg(long, value_name = "N")]
        volume: Option<u64>,
        /// Lifecycle status: `not_started`, `outlined`, `draft`,
        /// `finalized`, or `published` (published is read/protected).
        #[arg(long, value_enum)]
        status: Option<ChapterStatusArg>,
        /// Chapter outline prose (rich-text outline content). Persisted to
        /// the chapter's `outline_path` markdown file under the same
        /// `outline_revision` CAS; never touches `body_path`.
        #[arg(long, value_name = "TEXT", conflicts_with = "content_file")]
        content: Option<String>,
        /// Read `--content` from a file instead of the flag value.
        #[arg(long, value_name = "PATH", conflicts_with = "content")]
        content_file: Option<String>,
        /// Emit machine-readable JSON (the `OutlinePatchResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `creator works timeline` verbs (V1.72 canvas patch).
#[derive(Debug, Subcommand)]
pub enum TimelineCommand {
    /// Patch the work timeline (V1.72 canvas).
    ///
    /// CAS-guarded: `--base-revision` must match the outline revision from
    /// `outline show`. On 409 `outline_conflict`, re-read the outline and
    /// reapply with the new revision.
    Patch {
        /// Work reference (wrk_...) — the daemon's canonical work id.
        work_ref: String,
        /// Revision observed on the last canonical read (CAS). On a 409
        /// `outline_conflict`, re-read the outline (`creator works outline
        /// show`) and reapply with the new revision.
        #[arg(long, value_name = "N")]
        base_revision: u64,
        /// Timeline operation: `add_event`, `remove_event`,
        /// `attach_event_to_chapter`, `link_foreshadow`, or
        /// `unlink_foreshadow`.
        #[arg(long, value_enum)]
        op: TimelineOpArg,
        /// Identifier of an existing event (required for `remove_event`,
        /// `attach_event_to_chapter`, `link_foreshadow`,
        /// `unlink_foreshadow`).
        #[arg(long, value_name = "EVENT_ID")]
        event: Option<String>,
        /// Human-facing title for a new event (required for `add_event`).
        #[arg(long)]
        title: Option<String>,
        /// Optional longer description for a new event (`add_event`).
        #[arg(long)]
        description: Option<String>,
        /// Chapter number the event realizes (`add_event`,
        /// `attach_event_to_chapter`).
        #[arg(long, value_name = "N")]
        realizes_chapter: Option<u64>,
        /// Chapter number the event attaches to (`attach_event_to_chapter`).
        #[arg(long, value_name = "N")]
        target_chapter: Option<u64>,
        /// Event the source event foreshadows / stops foreshadowing
        /// (`link_foreshadow`, `unlink_foreshadow`).
        #[arg(long, value_name = "EVENT_ID")]
        foreshadows_event: Option<String>,
        /// Emit machine-readable JSON (the `OutlinePatchResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `--op` value for outline structure patches (V1.72 wire vocabulary).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum OutlineOpArg {
    /// Move a chapter to a different volume.
    #[value(name = "move_chapter")]
    MoveChapter,
    /// Link a timeline event to a realizing chapter.
    #[value(name = "link_event")]
    LinkEvent,
    /// Attach a chapter to a volume.
    #[value(name = "attach_to_volume")]
    AttachToVolume,
}

/// `--op` value for timeline patches (V1.72 wire vocabulary).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum TimelineOpArg {
    /// Add a new timeline event.
    #[value(name = "add_event")]
    AddEvent,
    /// Remove an existing timeline event.
    #[value(name = "remove_event")]
    RemoveEvent,
    /// Attach an existing event to a chapter.
    #[value(name = "attach_event_to_chapter")]
    AttachEventToChapter,
    /// Link a foreshadow edge (source → target).
    #[value(name = "link_foreshadow")]
    LinkForeshadow,
    /// Remove a foreshadow edge.
    #[value(name = "unlink_foreshadow")]
    UnlinkForeshadow,
}

/// `--status` value for chapter patches (V1.72 closed lifecycle vocabulary).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum ChapterStatusArg {
    /// Not started.
    #[value(name = "not_started")]
    NotStarted,
    /// Outlined.
    #[value(name = "outlined")]
    Outlined,
    /// Draft.
    #[value(name = "draft")]
    Draft,
    /// Finalized.
    #[value(name = "finalized")]
    Finalized,
    /// Published (read/protected in V1.72 patch routes).
    #[value(name = "published")]
    Published,
}

impl OutlineOpArg {
    const fn to_generated(self) -> OutlinePatchStructureRequestOperation {
        match self {
            Self::MoveChapter => OutlinePatchStructureRequestOperation::MoveChapter,
            Self::LinkEvent => OutlinePatchStructureRequestOperation::LinkEvent,
            Self::AttachToVolume => OutlinePatchStructureRequestOperation::AttachToVolume,
        }
    }
}

impl TimelineOpArg {
    const fn to_generated(self) -> TimelinePatchEventRequestOperation {
        match self {
            Self::AddEvent => TimelinePatchEventRequestOperation::AddEvent,
            Self::RemoveEvent => TimelinePatchEventRequestOperation::RemoveEvent,
            Self::AttachEventToChapter => TimelinePatchEventRequestOperation::AttachEventToChapter,
            Self::LinkForeshadow => TimelinePatchEventRequestOperation::LinkForeshadow,
            Self::UnlinkForeshadow => TimelinePatchEventRequestOperation::UnlinkForeshadow,
        }
    }
}

impl ChapterStatusArg {
    const fn to_generated(self) -> NexusOutlinePatchChapterSetStatus {
        match self {
            Self::NotStarted => NexusOutlinePatchChapterSetStatus::NotStarted,
            Self::Outlined => NexusOutlinePatchChapterSetStatus::Outlined,
            Self::Draft => NexusOutlinePatchChapterSetStatus::Draft,
            Self::Finalized => NexusOutlinePatchChapterSetStatus::Finalized,
            Self::Published => NexusOutlinePatchChapterSetStatus::Published,
        }
    }
}

/// Run a `creator works outline|chapter|timeline` subcommand.
///
/// # Errors
///
/// Returns `CliError` on invalid input (missing required flags for the
/// chosen `--op`, no chapter set fields, unreadable `--content-file`) or
/// any daemon API / network failure (409 `outline_conflict`, 404
/// `not_found`, 422 `outline_validation_failed`, 400 `bad_request` for
/// other 400s — all named, non-zero exit).
pub async fn run(cmd: OutlineCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    match cmd {
        OutlineCommand::Show { work_ref, json } => outline_show(&client, &work_ref, json).await,
        OutlineCommand::Patch {
            work_ref,
            base_revision,
            op,
            chapter,
            volume,
            event,
            target_chapter,
            json,
        } => {
            outline_patch(
                &client,
                &work_ref,
                base_revision,
                op,
                chapter,
                volume,
                event.as_deref(),
                target_chapter,
                json,
            )
            .await
        }
    }
}

/// Run a `creator works chapter` subcommand.
///
/// # Errors
///
/// Returns `CliError` on invalid input (no set fields, unreadable
/// `--content-file`) or any daemon API / network failure (409
/// `outline_conflict`, 404 `not_found`, 422 `outline_validation_failed`,
/// 400 `bad_request` for other 400s — all named, non-zero exit).
pub async fn run_chapter(cmd: ChapterCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    match cmd {
        ChapterCommand::Patch {
            work_ref,
            n,
            base_revision,
            title,
            slug,
            planned_word_count,
            actual_word_count,
            volume,
            status,
            content,
            content_file,
            json,
        } => {
            chapter_patch(
                &client,
                &work_ref,
                n,
                base_revision,
                title.as_deref(),
                slug.as_deref(),
                planned_word_count,
                actual_word_count,
                volume,
                status,
                content.as_deref(),
                content_file.as_deref(),
                json,
            )
            .await
        }
    }
}

/// Run a `creator works timeline` subcommand.
///
/// # Errors
///
/// Returns `CliError` on invalid input (missing required flags for the
/// chosen `--op`) or any daemon API / network failure (409
/// `outline_conflict`, 404 `not_found`, 422 `outline_validation_failed`,
/// 400 `bad_request` for other 400s — all named, non-zero exit).
pub async fn run_timeline(cmd: TimelineCommand, config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);
    match cmd {
        TimelineCommand::Patch {
            work_ref,
            base_revision,
            op,
            event,
            title,
            description,
            realizes_chapter,
            target_chapter,
            foreshadows_event,
            json,
        } => {
            timeline_patch(
                &client,
                &work_ref,
                base_revision,
                op,
                event.as_deref(),
                title.as_deref(),
                description.as_deref(),
                realizes_chapter,
                target_chapter,
                foreshadows_event.as_deref(),
                json,
            )
            .await
        }
    }
}

/// Parse a 1-based chapter/volume number into the daemon's `NonZeroU64` key.
///
/// # Errors
///
/// Returns a named `CliError::Other` when `n == 0` (the daemon contract
/// requires a positive number).
fn parse_positive(n: u64, flag: &str) -> Result<NonZeroU64> {
    NonZeroU64::new(n).ok_or_else(|| CliError::Other(format!("{flag} must be >= 1")))
}

/// `creator works outline show <work_ref>` — read the canonical work outline
/// + timeline (`GET /v1/daemon/works/:work_id/outline`).
///
/// # Errors
///
/// Returns `CliError` for daemon / network failures (404 `not_found` for
/// an unknown work, 400 `bad_request` for other 400s).
async fn outline_show(client: &DaemonClient, work_ref: &str, json: bool) -> Result<()> {
    let outline: WorkOutline = client
        .get(&format!("/v1/daemon/works/{work_ref}/outline"))
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&outline)?);
    } else {
        render_work_outline(&outline);
    }
    Ok(())
}

/// `creator works outline patch <work_ref> --base-revision N --op <op> …` —
/// patch the outline structure (`POST /v1/daemon/works/:work_id/outline/patch`).
///
/// # Errors
///
/// Returns a named `CliError::Other` when a required flag for the chosen
/// `--op` is missing, or `CliError` for daemon / network failures (409
/// `outline_conflict`, 404 `not_found`, 422 `outline_validation_failed`,
/// 400 `bad_request` for other 400s).
#[allow(clippy::too_many_arguments)] // CLI param plumbing — house pattern
async fn outline_patch(
    client: &DaemonClient,
    work_ref: &str,
    base_revision: u64,
    op: OutlineOpArg,
    chapter: Option<u64>,
    volume: Option<u64>,
    event: Option<&str>,
    target_chapter: Option<u64>,
    json: bool,
) -> Result<()> {
    // CLI-side required-flag checks mirror the daemon's field errors so
    // scripts fail fast with named messages (PL-5).
    match op {
        OutlineOpArg::MoveChapter => {
            if chapter.is_none() {
                return Err(CliError::Other(
                    "--chapter is required when --op move_chapter".to_string(),
                ));
            }
            if volume.is_none() {
                return Err(CliError::Other(
                    "--volume is required when --op move_chapter".to_string(),
                ));
            }
        }
        OutlineOpArg::AttachToVolume => {
            if chapter.is_none() {
                return Err(CliError::Other(
                    "--chapter is required when --op attach_to_volume".to_string(),
                ));
            }
            if volume.is_none() {
                return Err(CliError::Other(
                    "--volume is required when --op attach_to_volume".to_string(),
                ));
            }
        }
        OutlineOpArg::LinkEvent => {
            if event.is_none() {
                return Err(CliError::Other(
                    "--event is required when --op link_event".to_string(),
                ));
            }
            if target_chapter.is_none() {
                return Err(CliError::Other(
                    "--target-chapter is required when --op link_event".to_string(),
                ));
            }
        }
    }
    let req = OutlinePatchStructureRequest {
        work_id: work_ref.to_string(),
        base_revision,
        operation: op.to_generated(),
        chapter_id: chapter
            .map(|n| parse_positive(n, "--chapter"))
            .transpose()?,
        volume_id: volume.map(|n| parse_positive(n, "--volume")).transpose()?,
        event_id: event.map(str::to_string),
        target_chapter_id: target_chapter
            .map(|n| parse_positive(n, "--target-chapter"))
            .transpose()?,
    };
    let resp: OutlinePatchResponse = client
        .post(&format!("/v1/daemon/works/{work_ref}/outline/patch"), &req)
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Patched outline structure for Work '{work_ref}'.");
        render_patch_response(&resp);
    }
    Ok(())
}

/// `creator works chapter patch <work_ref> --n <n> --base-revision N …` —
/// patch a chapter's outline-node metadata
/// (`POST /v1/daemon/works/:work_id/chapters/:n/patch` — the outline **node**
/// route, NOT the V1.65 chapter-content PATCH).
///
/// # Errors
///
/// Returns a named `CliError::Other` when no set field is given or
/// `--content-file` cannot be read, or `CliError` for daemon / network
/// failures (409 `outline_conflict`, 404 `not_found`, 422
/// `outline_validation_failed`, 400 `bad_request` for other 400s).
#[allow(clippy::too_many_arguments)] // CLI param plumbing — house pattern
async fn chapter_patch(
    client: &DaemonClient,
    work_ref: &str,
    n: u64,
    base_revision: u64,
    title: Option<&str>,
    slug: Option<&str>,
    planned_word_count: Option<u64>,
    actual_word_count: Option<u64>,
    volume: Option<u64>,
    status: Option<ChapterStatusArg>,
    content: Option<&str>,
    content_file: Option<&str>,
    json: bool,
) -> Result<()> {
    let chapter = parse_positive(n, "--n")?;
    // `--content` / `--content-file` are `conflicts_with` each other at clap
    // parse time, so at most one is set here.
    let content = if let Some(text) = content {
        Some(text.to_string())
    } else if let Some(path) = content_file {
        let text = read_file_bounded(path, CONTENT_FILE_MAX_BYTES, "--content-file")?;
        Some(text)
    } else {
        None
    };
    let set = NexusOutlinePatchChapterSet {
        title: title.map(str::to_string),
        slug: slug.map(str::to_string),
        planned_word_count,
        actual_word_count,
        volume: volume.map(|v| parse_positive(v, "--volume")).transpose()?,
        status: status.map(ChapterStatusArg::to_generated),
        content: content
            .map(|text| {
                text.parse::<NexusOutlinePatchChapterSetContent>()
                    .map_err(|e| CliError::Other(format!("invalid --content: {e}")))
            })
            .transpose()?,
    };
    if set.title.is_none()
        && set.slug.is_none()
        && set.planned_word_count.is_none()
        && set.actual_word_count.is_none()
        && set.volume.is_none()
        && set.status.is_none()
        && set.content.is_none()
    {
        return Err(CliError::Other(
            "provide at least one of --title, --slug, --planned-word-count, \
             --actual-word-count, --volume, --status, --content, or --content-file"
                .to_string(),
        ));
    }
    let req = OutlinePatchChapterRequest {
        work_id: work_ref.to_string(),
        chapter_id: chapter,
        base_revision,
        set,
    };
    let resp: OutlinePatchResponse = client
        .post(
            &format!("/v1/daemon/works/{work_ref}/chapters/{n}/patch"),
            &req,
        )
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Patched chapter {n} outline node for Work '{work_ref}'.");
        render_patch_response(&resp);
    }
    Ok(())
}

/// `creator works timeline patch <work_ref> --base-revision N --op <op> …` —
/// patch the work timeline (`POST /v1/daemon/works/:work_id/timeline/patch`).
///
/// # Errors
///
/// Returns a named `CliError::Other` when a required flag for the chosen
/// `--op` is missing, or `CliError` for daemon / network failures (409
/// `outline_conflict`, 404 `not_found`, 422 `outline_validation_failed`,
/// 400 `bad_request` for other 400s).
#[allow(clippy::too_many_arguments)] // CLI param plumbing — house pattern
async fn timeline_patch(
    client: &DaemonClient,
    work_ref: &str,
    base_revision: u64,
    op: TimelineOpArg,
    event: Option<&str>,
    title: Option<&str>,
    description: Option<&str>,
    realizes_chapter: Option<u64>,
    target_chapter: Option<u64>,
    foreshadows_event: Option<&str>,
    json: bool,
) -> Result<()> {
    // CLI-side required-flag checks mirror the daemon's field errors so
    // scripts fail fast with named messages (PL-5).
    match op {
        TimelineOpArg::AddEvent => {
            if title.is_none() {
                return Err(CliError::Other(
                    "--title is required when --op add_event".to_string(),
                ));
            }
        }
        TimelineOpArg::RemoveEvent => {
            if event.is_none() {
                return Err(CliError::Other(
                    "--event is required when --op remove_event".to_string(),
                ));
            }
        }
        TimelineOpArg::AttachEventToChapter => {
            if event.is_none() {
                return Err(CliError::Other(
                    "--event is required when --op attach_event_to_chapter".to_string(),
                ));
            }
            if target_chapter.is_none() {
                return Err(CliError::Other(
                    "--target-chapter is required when --op attach_event_to_chapter".to_string(),
                ));
            }
        }
        TimelineOpArg::LinkForeshadow => {
            if event.is_none() {
                return Err(CliError::Other(
                    "--event is required when --op link_foreshadow".to_string(),
                ));
            }
            if foreshadows_event.is_none() {
                return Err(CliError::Other(
                    "--foreshadows-event is required when --op link_foreshadow".to_string(),
                ));
            }
        }
        TimelineOpArg::UnlinkForeshadow => {
            if event.is_none() {
                return Err(CliError::Other(
                    "--event is required when --op unlink_foreshadow".to_string(),
                ));
            }
            if foreshadows_event.is_none() {
                return Err(CliError::Other(
                    "--foreshadows-event is required when --op unlink_foreshadow".to_string(),
                ));
            }
        }
    }
    let req = TimelinePatchEventRequest {
        work_id: work_ref.to_string(),
        base_revision,
        operation: op.to_generated(),
        event_id: event.map(str::to_string),
        title: title.map(str::to_string),
        description: description.map(str::to_string),
        realizes_chapter_id: realizes_chapter
            .map(|n| parse_positive(n, "--realizes-chapter"))
            .transpose()?,
        target_chapter_id: target_chapter
            .map(|n| parse_positive(n, "--target-chapter"))
            .transpose()?,
        foreshadows_event_id: foreshadows_event.map(str::to_string),
    };
    let resp: OutlinePatchResponse = client
        .post(&format!("/v1/daemon/works/{work_ref}/timeline/patch"), &req)
        .await?;
    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Patched timeline for Work '{work_ref}'.");
        render_patch_response(&resp);
    }
    Ok(())
}

/// Render a `WorkOutline` for human output.
fn render_work_outline(outline: &WorkOutline) {
    println!(
        "Outline for {} (revision {})",
        outline.work_id, outline.outline_revision
    );
    for vol in &outline.volumes {
        let ids: Vec<String> = vol
            .chapter_ids
            .iter()
            .map(|id| id.get().to_string())
            .collect();
        println!(
            "  Volume {}: {} — chapters [{}]",
            vol.volume_id.get(),
            vol.label,
            ids.join(", ")
        );
    }
    println!("  timeline events: {}", outline.timeline_events.len());
    println!("  foreshadows: {}", outline.foreshadows.len());
    println!("  chapter titles: {}", outline.chapter_titles.len());
    println!("  updated_at: {}", outline.updated_at);
}

/// Render an `OutlinePatchResponse` for human output.
fn render_patch_response(resp: &OutlinePatchResponse) {
    println!("  new_revision: {}", resp.new_revision.get());
    for effect in &resp.side_effects {
        println!("  {effect}");
    }
    if !resp.validation_summary.warnings.is_empty() {
        println!("  warnings:");
        for warning in &resp.validation_summary.warnings {
            println!("    - {warning}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[derive(Parser)]
    struct WorksCli {
        #[command(subcommand)]
        command: OutlineCommand,
    }
    #[derive(Parser)]
    struct ChapterCli {
        #[command(subcommand)]
        command: ChapterCommand,
    }

    #[derive(Parser)]
    struct TimelineCli {
        #[command(subcommand)]
        command: TimelineCommand,
    }

    #[test]
    fn outline_show_parses() {
        let cli = WorksCli::try_parse_from(["nexus42", "show", "wrk_abc", "--json"]).unwrap();
        match cli.command {
            OutlineCommand::Show { work_ref, json } => {
                assert_eq!(work_ref, "wrk_abc");
                assert!(json);
            }
            OutlineCommand::Patch { .. } => panic!("expected show, got patch"),
        }
    }

    #[test]
    fn outline_patch_move_chapter_parses() {
        let cli = WorksCli::try_parse_from([
            "nexus42",
            "patch",
            "wrk_abc",
            "--base-revision",
            "3",
            "--op",
            "move_chapter",
            "--chapter",
            "1",
            "--volume",
            "2",
        ])
        .unwrap();
        match cli.command {
            OutlineCommand::Patch {
                work_ref,
                base_revision,
                op,
                chapter,
                volume,
                event,
                target_chapter,
                json,
            } => {
                assert_eq!(work_ref, "wrk_abc");
                assert_eq!(base_revision, 3);
                assert!(matches!(op, OutlineOpArg::MoveChapter));
                assert_eq!(chapter, Some(1));
                assert_eq!(volume, Some(2));
                assert!(event.is_none());
                assert!(target_chapter.is_none());
                assert!(!json);
            }
            OutlineCommand::Show { .. } => panic!("expected patch, got show"),
        }
    }

    #[test]
    fn outline_patch_link_event_parses() {
        let cli = WorksCli::try_parse_from([
            "nexus42",
            "patch",
            "wrk_abc",
            "--base-revision",
            "1",
            "--op",
            "link_event",
            "--event",
            "evt_1",
            "--target-chapter",
            "2",
        ])
        .unwrap();
        match cli.command {
            OutlineCommand::Patch {
                op,
                event,
                target_chapter,
                ..
            } => {
                assert!(matches!(op, OutlineOpArg::LinkEvent));
                assert_eq!(event.as_deref(), Some("evt_1"));
                assert_eq!(target_chapter, Some(2));
            }
            OutlineCommand::Show { .. } => panic!("expected patch, got show"),
        }
    }

    #[test]
    fn chapter_patch_parses_all_fields() {
        let cli = ChapterCli::try_parse_from([
            "nexus42",
            "patch",
            "wrk_abc",
            "--n",
            "1",
            "--base-revision",
            "2",
            "--title",
            "Chapter One",
            "--slug",
            "ch01",
            "--planned-word-count",
            "4000",
            "--actual-word-count",
            "3500",
            "--volume",
            "1",
            "--status",
            "draft",
            "--content",
            "## Beats",
            "--json",
        ])
        .unwrap();
        match cli.command {
            ChapterCommand::Patch {
                work_ref,
                n,
                base_revision,
                title,
                slug,
                planned_word_count,
                actual_word_count,
                volume,
                status,
                content,
                content_file,
                json,
            } => {
                assert_eq!(work_ref, "wrk_abc");
                assert_eq!(n, 1);
                assert_eq!(base_revision, 2);
                assert_eq!(title.as_deref(), Some("Chapter One"));
                assert_eq!(slug.as_deref(), Some("ch01"));
                assert_eq!(planned_word_count, Some(4000));
                assert_eq!(actual_word_count, Some(3500));
                assert_eq!(volume, Some(1));
                assert!(matches!(status, Some(ChapterStatusArg::Draft)));
                assert_eq!(content.as_deref(), Some("## Beats"));
                assert!(content_file.is_none());
                assert!(json);
            }
        }
    }

    #[test]
    fn timeline_patch_add_event_parses() {
        let cli = TimelineCli::try_parse_from([
            "nexus42",
            "patch",
            "wrk_abc",
            "--base-revision",
            "1",
            "--op",
            "add_event",
            "--title",
            "The storm",
            "--description",
            "A storm hits the harbor",
            "--realizes-chapter",
            "3",
        ])
        .unwrap();
        match cli.command {
            TimelineCommand::Patch {
                work_ref,
                base_revision,
                op,
                title,
                description,
                realizes_chapter,
                ..
            } => {
                assert_eq!(work_ref, "wrk_abc");
                assert_eq!(base_revision, 1);
                assert!(matches!(op, TimelineOpArg::AddEvent));
                assert_eq!(title.as_deref(), Some("The storm"));
                assert_eq!(description.as_deref(), Some("A storm hits the harbor"));
                assert_eq!(realizes_chapter, Some(3));
            }
        }
    }

    #[test]
    fn timeline_patch_link_foreshadow_parses() {
        let cli = TimelineCli::try_parse_from([
            "nexus42",
            "patch",
            "wrk_abc",
            "--base-revision",
            "1",
            "--op",
            "link_foreshadow",
            "--event",
            "evt_a",
            "--foreshadows-event",
            "evt_b",
        ])
        .unwrap();
        match cli.command {
            TimelineCommand::Patch {
                op,
                event,
                foreshadows_event,
                ..
            } => {
                assert!(matches!(op, TimelineOpArg::LinkForeshadow));
                assert_eq!(event.as_deref(), Some("evt_a"));
                assert_eq!(foreshadows_event.as_deref(), Some("evt_b"));
            }
        }
    }
}
