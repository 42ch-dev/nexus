//! `nexus42 creator works` — atomic Work operations (DF-60 §6.2H, DF-61).
//!
//! `creator works` is the atomic plane: one business function per subcommand.
//!
//! V1.45 P2 migrated the atomic ops off the old `creator run` runner
//! (`inspire` ← `run continue`, `reopen` ← `run resume --reopen`,
//! `reconcile-chapters` ← `run reconcile-chapters`). v1.193 P2-T1 then removed
//! the remaining execution entrances — `works intake`, `works resume-chain` and
//! the hard-reject `works start` / `works create` stubs — together with the
//! incomplete Creator runner; every arm below is a retained atomic operation.

use crate::errors::Result;
use clap::Subcommand;

use crate::config::CliConfig;
// v1.193 P0-T7: every retained Work arm — selection, pool, inspiration,
// governance and findings — runs on the typed core seam ([`crate::core`]).
use crate::core::{finish_direct, map_core_error, open_direct_core};
// V1.42 P-last (R-V141P0-06): completion-lock file path check
use nexus_home_layout;
// Schema-owned wire shape for the stale enrichment of `works status --json`
// (the core report is not a wire type).
use nexus_contracts::daemon_api::findings::{
    StaleFindingEntry as StaleFindingWire, StaleFindingsResponse as StaleFindingsWire,
};
// Schema-owned wire shapes for the `--json` output of the pool reads; the core
// carriers are not wire types (they still hold the stored `creator_id`).
use nexus_contracts::{
    AppendInspirationRequest, ListWorksQuery, ReleaseCompletionLockRequest,
    WorkInspirationAddResponse, WorkInspirationListResponse, WorkPoolListResponse,
};
use nexus_core::{
    AddInspirationRequest, ArchiveInspirationRequest, ArchivePoolRequest, CoreService,
    ListFindingsQuery, ListInspirationQuery, ListPoolQuery, Principal, PromoteInspirationRequest,
    PromotePoolRequest, ReconcileDryRunQuery, StaleFindingsResponse as CoreStaleFindingsResponse,
    WorkReconcileReport,
};

pub mod chronology;
pub mod cron;
pub mod outline;

/// Work management subcommands (DF-60 §6.2H).
#[derive(Debug, Subcommand)]
pub enum WorksCommand {
    /// List all Works for the active creator.
    ///
    /// Migrated from `creator run list` (V1.41).
    List {
        /// Filter by status
        #[arg(long)]
        status: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Show details of a single Work.
    ///
    /// When `<work_id>` is omitted, resolves the pool `active` Work.
    /// Migrated from `creator run status` (V1.41).
    Status {
        /// Work ID (wrk_...). Omit to use pool active Work.
        work_id: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Set pool `active` row → CLI default `work_id` (DF-60 §1.1).
    ///
    /// Does NOT pause other Works. Retained Work commands that take an
    /// optional `work_id` — `status`, `inspire`, `reopen`,
    /// `reconcile-chapters`, `findings list`, `rules reset` — fall back to
    /// this Work when it is omitted.
    Use {
        /// Work ID (wrk_...) to set as active
        work_id: String,
    },
    /// Manage the completion lock for a Work (DF-60 §3.2).
    CompletionLock {
        #[command(subcommand)]
        command: CompletionLockCommand,
    },
    /// Manage the selection pool — promote, archive, list entries (DF-61).
    Pool {
        #[command(subcommand)]
        action: PoolAction,
    },

    // ── V1.45 P2: atomic Work operations migrated from `creator run` ──
    /// Append inspiration / direction to an existing Work (V1.45 P2).
    ///
    /// Pure side-input lane: POSTs an inspiration note to the Work's
    /// inspiration log. Does NOT create a schedule or enqueue a preset.
    /// Migrated from `creator run continue` (drops the unimplemented `--preset`).
    Inspire {
        /// Work ID (wrk_...). Omit to use pool active Work.
        work_id: Option<String>,
        /// New inspiration / direction note
        #[arg(long)]
        note: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Reopen a completed Work for further writing (V1.45 P2).
    ///
    /// Releases the Work's completion lock through the typed core: the DB
    /// `completion_locked_at` is cleared (SSOT), `novel_completion_status`
    /// becomes `reopened`, the derived `.completion-lock.json` is removed, and
    /// the required `--reason` is recorded. A Work that is not
    /// completion-locked is refused.
    /// Migrated from `creator run resume --reopen`.
    Reopen {
        /// Work ID (wrk_...). Omit to use pool active Work.
        work_id: Option<String>,
        /// Audit reason for reopening (required, audit-logged)
        #[arg(long)]
        reason: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    /// Rebuild `work_chapters` from filesystem (V1.45 P2).
    ///
    /// Scans the Work's `Stories/` directory and creates or updates
    /// `work_chapters` rows to match the files on disk.
    /// Migrated from `creator run reconcile-chapters`.
    ///
    /// V1.49 P2 (R-V148P4-W2): `--dry-run` previews the `ReconcileReport`
    /// without filesystem/DB writes; `--yes` skips the confirmation prompt
    /// on the mutating path (mirrors `works rules reset` safety flags).
    ReconcileChapters {
        /// Work ID (wrk_...). Omit to use pool active Work.
        work_id: Option<String>,
        /// Preview the reconcile as a `ReconcileReport` without writing.
        ///
        /// No filesystem or DB rows are modified, no runtime lock is acquired,
        /// and no confirmation prompt is shown. Takes precedence over `--yes`.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Skip the confirmation prompt and mutate immediately.
        ///
        /// By default (when stderr/stdin is a TTY) the reconcile asks for
        /// confirmation before mutating `work_chapters` and chapter
        /// frontmatter. Pass `--yes` (or `-y`) to proceed non-interactively;
        /// use `--dry-run` to preview the changes without writing. Mirrors
        /// `apt-get -y` / `pacman --noconfirm`.
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        yes: bool,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    // ── V1.48 P2: findings + rules (Layer 2 AGENTS.md) ──────────────
    /// Finding-level operations (accept rule suggestions, future prune / …).
    ///
    /// V1.48 P2 introduces the `accept` subcommand which appends a finding's
    /// `rule_suggestion` to the Work's `AGENTS.md` Layer 2 file.
    Findings {
        #[command(subcommand)]
        command: FindingsCommand,
    },

    /// Layer 2 rules file operations for a Work (`Works/<work_ref>/AGENTS.md`).
    ///
    /// V1.48 P2 introduces the `reset` subcommand which restores the
    /// default `AGENTS.md` scaffold.
    Rules {
        #[command(subcommand)]
        command: RulesCommand,
    },

    // ── V1.50 T-A P0: per-Work cron configuration ──────────────────────
    /// Manage per-Work cron configuration for novel-writing staggering
    /// (V1.50 §3). Foundation: set/show/list the `schedule_json` column.
    Cron {
        #[command(subcommand)]
        command: cron::CronCommand,
    },

    // ── V1.50 T-A P3: per-Work auto-chronology ─────────────────────────
    /// Manage per-Work auto-chronology (volume auto-advance on finish)
    /// (V1.50 §2.2). `set` toggles the opt-in flag; `show` renders state;
    /// `advance` manually overrides to a target volume.
    Chronology {
        #[command(subcommand)]
        command: chronology::ChronologyCommand,
    },
    // ── V1.175 P1 Task 3: outline/chapter/timeline patch leaves (group 2) ──
    /// Show / patch the work outline (V1.72 canvas read + structure patch).
    ///
    /// Thin daemon-HTTP leaves over the existing canvas routes (AR-84 group
    /// 2). All writes are CAS-guarded with `--base-revision`; a stale
    /// revision returns 409 `outline_conflict` (current revision, node,
    /// conflicting path, recovery hint). Re-read the outline (`outline
    /// show`) and reapply with the new revision.
    Outline {
        #[command(subcommand)]
        command: outline::OutlineCommand,
    },
    /// Patch a chapter's outline-node metadata (V1.72 outline canvas).
    ///
    /// **Route-family guard:** targets the outline **node** route
    /// `POST /v1/daemon/works/:work_id/chapters/:n/patch` — NOT the V1.65
    /// chapter-**content** `PATCH /v1/daemon/works/:work_id/chapters/:n`
    /// (different DTO family, not covered here). CAS-guarded with
    /// `--base-revision`; a stale revision returns 409 `outline_conflict`
    /// (re-read the outline and reapply).
    Chapter {
        #[command(subcommand)]
        command: outline::ChapterCommand,
    },
    /// Patch the work timeline (V1.72 canvas): add/remove events, attach to
    /// chapters, and link/unlink foreshadow edges.
    ///
    /// CAS-guarded with `--base-revision`; a stale revision returns 409
    /// `outline_conflict` (re-read the outline and reapply).
    Timeline {
        #[command(subcommand)]
        command: outline::TimelineCommand,
    },
}

/// Completion lock subcommands.
#[derive(Debug, Subcommand)]
pub enum CompletionLockCommand {
    /// Release the Work's completion lock and reopen it for further writing.
    ///
    /// One core operation: the DB lock is cleared (SSOT),
    /// `novel_completion_status` becomes `reopened` and the derived
    /// `.completion-lock.json` is removed. `creator works reopen --reason "…"`
    /// runs the same release with an explicit audit reason.
    Release {
        /// Work ID (wrk_...) to release the completion lock for
        work_id: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Findings subcommands (V1.48 P2).
#[derive(Debug, Subcommand)]
pub enum FindingsCommand {
    /// Accept a finding's `rule_suggestion` and append it to the Work's
    /// `AGENTS.md` Layer 2 file (V1.48 P2, overlay §3.2).
    ///
    /// Loads the finding by ID (creator-scoped), validates that
    /// `rule_suggestion` is non-empty, appends an audit-friendly entry
    /// under `## Accepted rule suggestions` in
    /// `Works/<work_ref>/AGENTS.md` (idempotent on `finding_id`), and
    /// marks the finding `status=resolved`.
    Accept {
        /// Finding ID (fnd_...) to accept.
        finding_id: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Prune `resolved` findings older than the retention window (V1.49 P3,
    /// `novel-writing/quality-loop.md` §9.4).
    ///
    /// Deletes `resolved` findings whose `updated_at` is older than the
    /// retention window (default 90 days). `open` and `wont_fix` findings are
    /// never touched. Pass `--dry-run` to preview the count without deleting.
    Prune {
        /// Retention window in days (default 90 = `RETENTION_DEFAULT_DAYS`).
        #[arg(long, default_value_t = nexus_local_db::RETENTION_DEFAULT_DAYS)]
        older_than_days: i64,
        /// Preview the count that WOULD be pruned without deleting.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Emit machine-readable JSON instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    // ── V1.175 P1 Task 4: findings triage leaves (group 8, AR-87) ──────
    /// List findings for a Work (`GET /v1/daemon/works/:work_id/findings`).
    ///
    /// Optional `--status` / `--severity` filters. `--json` emits the
    /// `ListFindingsResponse` DTO verbatim.
    List {
        /// Work reference (wrk_...). Omit to use pool active Work.
        work_id: Option<String>,
        /// Filter by status (single value or comma-separated list, e.g.
        /// `open,triaged`).
        #[arg(long)]
        status: Option<String>,
        /// Filter by severity (`info`, `minor`, `major`, `blocker`).
        #[arg(long)]
        severity: Option<String>,
        /// Emit machine-readable JSON (the `ListFindingsResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Set a finding's status through the work-findings PATCH route
    /// (`PATCH /v1/daemon/works/:work_id/findings/:finding_id`).
    ///
    /// One generic verb over one route (AR-87 #3 — no `triage|resolve`
    /// sugar). `--status` accepts exactly the closed lifecycle vocabulary:
    /// `open`, `triaged`, `in_review`, `resolved`, `wont_fix`, `duplicate`.
    /// Legal transitions:
    ///   `open` → `triaged` | `in_review` | `resolved` | `wont_fix` | `duplicate`
    ///   `triaged` → `in_review` | `resolved` | `wont_fix` | `duplicate`
    ///   `in_review` → `resolved` | `wont_fix` | `duplicate`
    ///   `resolved` / `wont_fix` / `duplicate` are terminal
    /// `from == to` is rejected. An illegal transition returns 422
    /// `invalid_transition` naming `from → to`.
    SetStatus {
        /// Finding ID (fnd_...) to update.
        finding_id: String,
        /// Work reference (wrk_...) the finding belongs to.
        #[arg(long, value_name = "WORK_REF")]
        work: String,
        /// New status (closed lifecycle vocabulary above).
        #[arg(long, value_enum)]
        status: FindingStatusArg,
        /// New routing hint (`write`, `brainstorm`, `none`, `master`).
        #[arg(long)]
        target_executor: Option<String>,
        /// Emit machine-readable JSON (the `FindingDetailResponse` DTO
        /// verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// `--status` value for `creator works findings set-status` (V1.49 F6
/// closed lifecycle vocabulary, AR-87).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum FindingStatusArg {
    Open,
    Triaged,
    /// In review.
    #[value(name = "in_review")]
    InReview,
    Resolved,
    /// Won't fix.
    #[value(name = "wont_fix")]
    WontFix,
    Duplicate,
}

impl FindingStatusArg {
    /// Wire token (`snake_case`) for the PATCH body.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Triaged => "triaged",
            Self::InReview => "in_review",
            Self::Resolved => "resolved",
            Self::WontFix => "wont_fix",
            Self::Duplicate => "duplicate",
        }
    }
}

/// Layer 2 rules subcommands (V1.48 P2).
#[derive(Debug, Subcommand)]
pub enum RulesCommand {
    /// Reset the Work's `AGENTS.md` to the default scaffold (V1.48 P2,
    /// overlay §4).
    ///
    /// Overwrites `Works/<work_ref>/AGENTS.md` with the embedded default
    /// scaffold. Does NOT delete the Work or any chapter artifacts.
    /// Use when the file has drifted and you want to start fresh.
    ///
    /// Safety flags (V1.48 P2-fix1):
    ///
    /// - By default the command prints a unified diff of what would be
    ///   discarded and prompts for confirmation before overwriting.
    /// - `--dry-run` prints the diff and exits WITHOUT writing (preview).
    /// - `--yes` (or `-y`) skips the confirmation prompt and writes
    ///   immediately, intended for scripted use (matches the `apt-get -y` /
    ///   `pacman --noconfirm` convention).
    /// - `--dry-run` takes precedence over `--yes`.
    Reset {
        /// Work ID (wrk_...). Omit to use pool active Work.
        work_id: Option<String>,
        /// Preview the reset as a unified diff without writing.
        ///
        /// No file is modified and no confirmation prompt is shown. Takes
        /// precedence over `--yes`.
        #[arg(long, default_value_t = false)]
        dry_run: bool,
        /// Skip the confirmation prompt and write immediately.
        ///
        /// By default the reset prints a diff and asks for confirmation before
        /// overwriting `AGENTS.md`. Pass `--yes` (or `-y`) to proceed
        /// non-interactively. Mirrors `apt-get -y` / `pacman --noconfirm`.
        #[arg(long = "yes", short = 'y', default_value_t = false)]
        yes: bool,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Selection pool subcommands (DF-61).
#[derive(Debug, Subcommand)]
pub enum PoolAction {
    /// List pool entries for the active creator.
    List {
        /// Filter by status (active, queued, completed, archived)
        #[arg(long)]
        status: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Promote a Work to `active` in the pool (demotes prior active).
    Promote {
        /// Work ID (wrk_...) to promote
        work_id: String,
        /// Also set as CLI default via `works use`
        #[arg(long, default_value_t = false)]
        set_default: bool,
    },
    /// Archive a pool entry.
    Archive {
        /// Pool entry ID (npe_...) to archive
        entry_id: String,
    },
    /// Manage the inspiration pool (DF-61 §4).
    Inspiration {
        #[command(subcommand)]
        action: InspirationAction,
    },
}

/// Inspiration pool subcommands (DF-61 §4).
///
/// Pool-level inspiration items (DB SSOT in `inspiration_items` table);
/// distinct from per-Work `works.inspiration_log`.
#[derive(Debug, Subcommand)]
pub enum InspirationAction {
    /// Add a new inspiration item (creates MD scaffold + DB row).
    ///
    /// Pool-level item; distinct from per-Work `works.inspiration_log`.
    Add {
        /// Title for the inspiration item
        title: String,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List inspiration items.
    ///
    /// Pool-level items; distinct from per-Work `works.inspiration_log`.
    List {
        /// Filter by status (idea, promoted, archived)
        #[arg(long)]
        status: Option<String>,
        /// Emit machine-readable JSON instead of human text
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Promote an inspiration item — creates a Work + pool row.
    ///
    /// Pool-level item; distinct from per-Work `works.inspiration_log`.
    Promote {
        /// Inspiration item ID (npi_...) to promote
        item_id: String,
        /// Optional idea override for the new Work's ``initial_idea``
        #[arg(long)]
        idea: Option<String>,
        /// Also set as CLI default via `works use`
        #[arg(long, default_value_t = false)]
        set_default: bool,
    },
    /// Archive an inspiration item.
    ///
    /// Pool-level item; distinct from per-Work `works.inspiration_log`.
    Archive {
        /// Inspiration item ID (npi_...) to archive
        item_id: String,
    },
}

/// Dispatch `creator works` subcommands.
///
/// # Errors
///
/// Returns the typed core refusal for every arm — the whole group runs the
/// direct core seam, so no arm reports a daemon transport error.
pub async fn handle_works(cmd: WorksCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        WorksCommand::List { status, json } => handle_list(config, status, json).await,
        WorksCommand::Status { work_id, json } => handle_status(config, work_id, json).await,
        WorksCommand::Use { work_id } => handle_use(config, &work_id).await,
        WorksCommand::CompletionLock { command } => handle_completion_lock(config, command).await,
        WorksCommand::Pool { action } => handle_pool(config, action).await,
        WorksCommand::Inspire {
            work_id,
            note,
            json,
        } => handle_inspire(config, work_id, &note, json).await,
        WorksCommand::Reopen {
            work_id,
            reason,
            json,
        } => handle_reopen(config, work_id, &reason, json).await,
        WorksCommand::ReconcileChapters {
            work_id,
            dry_run,
            yes,
            json,
        } => handle_reconcile_chapters(config, work_id, dry_run, yes, json).await,
        WorksCommand::Findings { command } => {
            super::rules_runtime::handle_findings(config, command).await
        }
        WorksCommand::Rules { command } => {
            super::rules_runtime::handle_rules(config, command).await
        }
        WorksCommand::Cron { command } => cron::handle_cron(command, config).await,
        WorksCommand::Chronology { command } => {
            chronology::handle_chronology(command, config).await
        }
        WorksCommand::Outline { command } => outline::run(command, config).await,
        WorksCommand::Chapter { command } => outline::run_chapter(command, config).await,
        WorksCommand::Timeline { command } => outline::run_timeline(command, config).await,
    }
}

/// Handle `creator works list` — the active creator's Works page.
///
/// Reads the typed core producer ([`CoreService::list_works`]) with the
/// retained query defaults: only the `--status` filter is set, so the core's
/// own cursor/limit/default-sort policy applies unchanged.
///
/// # Errors
///
/// Returns the typed core refusal (no selected creator/workspace, malformed
/// status filter, storage failure) and any cleanup refusal from
/// [`finish_direct`].
async fn handle_list(config: &CliConfig, status: Option<String>, json: bool) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.list_works(
            &principal,
            ListWorksQuery {
                status,
                ..ListWorksQuery::default()
            },
        )
        .await
        .map_err(map_core_error)
    }
    .await;
    let resp = finish_direct(&core, outcome).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else if resp.items.is_empty() {
        println!("No works found.");
    } else {
        println!(
            "{:<36} {:30} {:12} {:12} LOCK UPDATED",
            "WORK_ID", "TITLE", "STATUS", "INTAKE"
        );
        for w in &resp.items {
            let id = w.work_id.as_str();
            let title = w.title.as_str();
            let ws = w.status.as_str();
            let intake = w.intake_status.as_str();
            let updated = w.updated_at.as_str();
            let lock_icon = if w.completion_locked_at.is_some() {
                "🔒"
            } else {
                " "
            };
            let display_title = truncate_with_ellipsis(title, 28);
            println!("{id:<36} {display_title:30} {ws:12} {intake:12} {lock_icon}   {updated}");
        }
        println!("\n{} work(s)", resp.items.len());
    }

    Ok(())
}

/// Resolve the omitted Work through a two-step selection over the core.
///
/// Every arm that accepts an omitted `<work_id>` (status, inspire, reopen,
/// reconcile-chapters, and the findings/rules leaves in
/// [`super::rules_runtime`]) resolves it here, against both stores the
/// explicit entrances write:
///
/// 1. the selection pool `active` entry — `novel_pool_entries`, the store
///    `works use` and every pool promotion write. A promoted Work keeps
///    `works.status = 'draft'`, so this step is the only one that finds it
///    (R-V1193-P0T5-OMITTED-ID-POOL-ACTIVE);
/// 2. the `works.status = active` selection, for a Work that is active in
///    `works` without a pool `active` entry. The two domains are independent,
///    so an empty pool page does not mean "no active Work".
///
/// Each step is the same bounded `limit = 1` read of the same producer family
/// ([`CoreService::list_work_pool`], then [`CoreService::list_works`]), and
/// both end in the same refusal text.
///
/// # Errors
///
/// Returns [`crate::errors::CliError::Config`] when neither step resolves a
/// Work, and the mapped core error when a bounded query fails.
pub(crate) async fn active_work_id_core(
    core: &CoreService,
    principal: &Principal,
) -> Result<String> {
    let pool_page = core
        .list_work_pool(
            principal,
            ListPoolQuery {
                status: Some("active".to_string()),
                limit: Some(1),
                offset: Some(0),
            },
        )
        .await
        .map_err(map_core_error)?;
    if let Some(entry) = pool_page.entries.first() {
        return Ok(entry.work_id.clone());
    }
    let works_page = core
        .list_works(
            principal,
            ListWorksQuery {
                status: Some("active".to_string()),
                limit: Some(1),
                ..ListWorksQuery::default()
            },
        )
        .await
        .map_err(map_core_error)?;
    works_page
        .items
        .first()
        .map(|w| w.work_id.clone())
        .ok_or_else(|| {
            crate::errors::CliError::Config(
                "No active Work found. Specify <work_id> or run \
             `nexus42 creator works use <work_id>`."
                    .to_string(),
            )
        })
}

// Migrated from run.rs — preserved status display logic with DF-60 extensions.
//
// v1.193 P0-T7: the Work *and* its findings/stale enrichment come from the
// typed core (`get_work` plus the findings family, with the active-Work
// selection resolved by `active_work_id_core`). Every read happens inside the
// admitted writer, before [`finish_direct`] releases it, so nothing is printed
// ahead of a settled close. A non-novel `--json` status makes neither read.
#[allow(clippy::too_many_lines)]
async fn handle_status(config: &CliConfig, work_id: Option<String>, json: bool) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let resolved = match work_id {
            Some(id) => id,
            None => active_work_id_core(&core, &principal).await?,
        };
        let work = core
            .get_work(&principal, resolved)
            .await
            .map_err(map_core_error)?;
        // Findings/stale enrichment is novel-only (Grill #6/#8; spec §4.1): a
        // generic work stays findings-free, while the human path renders the
        // creator-global stale banner for every profile.
        let novel = work.work_profile.as_deref() == Some("novel");
        let (open_findings, stale) =
            fetch_status_enrichment(&core, &principal, &work.work_id, novel, !json).await;
        Ok((work, open_findings, stale))
    }
    .await;
    let (work, open_findings, stale) = finish_direct(&core, outcome).await?;

    // `WorkDetails` derives `Serialize` as the retained tool/Work wire shape
    // (it replaced the daemon's field-for-field copy), and every reader below
    // treats an absent nullable key and an explicit `null` identically.
    let resolved_id = work.work_id.clone();
    let resp = serde_json::to_value(&work)?;

    if json {
        // V1.46 P0 (T1+T2): novel-only findings enrichment (Grill #6/#8; spec §4.1).
        // Generic / non-novel works stay findings-free (novel-only gate).
        let is_novel =
            resp.get("work_profile").and_then(serde_json::Value::as_str) == Some("novel");
        let findings = if is_novel {
            open_findings.as_slice()
        } else {
            None
        };
        let mut output = enrich_status_json(resp, findings, stale.as_ref());

        // V1.51 T-B P0: add lock_holder field (best-effort, reads from filesystem).
        if let Some(lock_holder) = read_lock_holder_json(&output) {
            if let Some(obj) = output.as_object_mut() {
                obj.insert("lock_holder".to_string(), lock_holder);
            }
        }

        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        // V1.39 P4 T3: stale findings banner — best-effort, never
        // fails the status command.
        //
        // R-V146P0-QC3-S3: rendered from the enrichment read taken inside the
        // admitted writer (`fetch_stale_findings`), so the banner never costs a
        // second read and a degraded stale read still leaves the status output
        // intact.
        if let Some(stale) = stale.as_ref() {
            let stale_count = stale
                .get("stale_count")
                .and_then(serde_json::Value::as_u64)
                .unwrap_or(0);
            let threshold_secs = stale
                .get("threshold_seconds")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(96 * 60 * 60);
            if let Some(banner) = format_stale_banner(stale_count, threshold_secs) {
                println!("{banner}");
                println!();
            }
        }

        let work_status = resp
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("(not set)");
        let title = resp
            .get("title")
            .and_then(|v| v.as_str())
            .unwrap_or("(untitled)");
        let work_profile = resp.get("work_profile").and_then(|v| v.as_str());
        let work_ref = resp
            .get("work_ref")
            .and_then(|v| v.as_str())
            .unwrap_or("(no ref)");
        let intake_status = resp
            .get("intake_status")
            .and_then(|v| v.as_str())
            .unwrap_or("(not set)");
        let current_chapter = resp
            .get("current_chapter")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let total_planned = resp
            .get("total_planned_chapters")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let chapters = resp.get("chapters").and_then(|v| v.as_array());

        // V1.38 P0 (T8): per-chapter status UX per spec §8.1.
        // For novel profile works, show chapter-centric output.
        if let (Some("novel"), Some(ch_list)) = (work_profile, chapters) {
            let finalized_count = ch_list
                .iter()
                .filter(|c| c.get("status").and_then(|v| v.as_str()) == Some("finalized"))
                .count();
            let total = ch_list.len();

            let profile_tag = " (novel)".to_string();

            // V1.43 P2 (T2): the open-findings summary for spec §4 row 3 comes
            // from the enrichment read taken inside the admitted writer.
            if work_status == "completed" {
                let updated_at = resp
                    .get("updated_at")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(unknown)");
                // V1.43 (P1 §3 remediation — work completed): cite quickstart §6.
                println!("═══════════════════════════════════════════════════════");
                println!("  \"{title}\" — Work {resolved_id}{profile_tag}");
                println!("  COMPLETED at {updated_at}");
                println!("  {total}/{total} chapters finalized.");
                println!("  No further novel-writing schedules will be enqueued.");
                println!();
                // V1.43 P2: findings summary in completed view (spec §4 row 3).
                print_findings_summary(&open_findings);
                // V1.47 P1: normalize user-facing copy — spec name, not repo path.
                println!("  This Work is complete; see novel-author-experience §3");
                println!("═══════════════════════════════════════════════════════");
            } else {
                // Header
                println!("Work: {resolved_id} — {title}{profile_tag}");
                println!("work_ref: {work_ref}");
                println!("intake: {intake_status}");
                println!("progress: {finalized_count} / {total} chapters finalized");
                println!("current_chapter: {current_chapter}");
                println!("total_planned_chapters: {total_planned}");

                // V1.39 T7: auto-chain checkpoint fields
                let auto_chain = resp
                    .get("auto_chain_enabled")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(true);
                let driver = resp
                    .get("driver_schedule_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or("none");
                let interrupted = resp
                    .get("auto_chain_interrupted")
                    .and_then(serde_json::Value::as_bool)
                    .unwrap_or(false);

                println!("auto_chain_enabled: {auto_chain}");
                println!("driver_schedule_id: {driver}");
                if interrupted {
                    // v1.193 P2-T1: the `creator works resume-chain` entrance was
                    // removed with the incomplete Creator runner; the flag stays
                    // reported, but no CLI remediation command is advertised.
                    println!("auto_chain_interrupted: true");
                }

                // V1.41: completion lock fields (DF-60 §6.2H)
                if let Some(completion_status) =
                    resp.get("novel_completion_status").and_then(|v| v.as_str())
                {
                    println!("completion_status: {completion_status}");
                }
                if let Some(locked_at) = resp.get("completion_locked_at").and_then(|v| v.as_str()) {
                    println!("completion_locked_at: {locked_at}");
                    // V1.42 P-last (R-V141P0-06): missing-file hint.
                    print_completion_lock_hint(work_ref, &resolved_id);
                }
                if let Some(lock_holder) = resp.get("runtime_lock_holder").and_then(|v| v.as_str())
                {
                    println!("runtime_lock_holder: {lock_holder}");
                }

                // V1.43 P2: findings summary (spec §4 row 3).
                print_findings_summary(&open_findings);

                // Per-chapter table
                // V1.46 P2 (Grill #9): pass work_id so on-disk path hints can
                // render a `works reconcile-chapters` remediation command.
                print_chapter_table(ch_list, &resolved_id);
            }
        } else {
            // Non-novel or generic work display
            println!("Work: {resolved_id} — {title}");
            println!("status: {work_status}");
            println!("work_ref: {work_ref}");
            println!("intake: {intake_status}");

            // Show all remaining key-value pairs
            let skip_keys = [
                "work_id",
                "title",
                "status",
                "work_ref",
                "intake_status",
                "chapters",
                "work_profile",
            ];
            if let Some(obj) = resp.as_object() {
                for (key, val) in obj {
                    if skip_keys.contains(&key.as_str()) {
                        continue;
                    }
                    if val.is_null() {
                        continue;
                    }
                    let label = key.replace('_', " ");
                    let val = if val.is_string() {
                        val.as_str().unwrap_or("(invalid)").to_string()
                    } else {
                        format!("{val}")
                    };
                    println!("{label:>20}: {val}");
                }
            }
        }
    }

    Ok(())
}

/// Handle `creator works use` — set the pool `active` row for a Work (DF-60 §1.1).
///
/// The Work is read first so an unknown id is refused before any write (the
/// retired adapter's GET-then-POST order), then
/// [`CoreService::select_work`] demotes the current `active` entry and
/// promotes this one.
///
/// # Errors
///
/// Returns the typed core refusal (unknown Work, no selected creator/workspace,
/// storage failure) and any cleanup refusal from [`finish_direct`].
async fn handle_use(config: &CliConfig, work_id: &str) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let work = core
            .get_work(&principal, work_id.to_string())
            .await
            .map_err(map_core_error)?;
        core.select_work(&principal, work_id.to_string())
            .await
            .map_err(map_core_error)?;
        Ok(work)
    }
    .await;
    let work = finish_direct(&core, outcome).await?;

    println!("Active Work set to {work_id} ({})", work.title);

    Ok(())
}

// ── V1.45 P2: atomic Work operations ──────────────────────────────────

/// Handle `creator works inspire` — append an inspiration note to the Work's
/// own `inspiration_log` (V1.45 P2).
///
/// Pure side-input: [`CoreService::append_work_inspiration`] appends to the
/// Work's `inspiration_log` without creating a schedule, and never touches the
/// pool-level inspiration store (`works pool inspiration …`, DB SSOT in
/// `inspiration_items`) — the two stores are distinct. Migrated from
/// `creator run continue` (drops `--preset`).
///
/// # Errors
///
/// Returns the typed core refusal (unknown Work, an active auto-chain driver, a
/// held runtime lock, no selected creator/workspace) and any cleanup refusal
/// from [`finish_direct`].
async fn handle_inspire(
    config: &CliConfig,
    work_id: Option<String>,
    note: &str,
    json: bool,
) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let resolved_id = match work_id {
            Some(id) => id,
            None => active_work_id_core(&core, &principal).await?,
        };
        let resp = core
            .append_work_inspiration(
                &principal,
                resolved_id.clone(),
                // `cli:<kind>:<uuid>` runtime-lock holder; the daemon adapter
                // passed `http` for its own surface.
                "inspire",
                AppendInspirationRequest {
                    note: note.to_string(),
                },
            )
            .await
            .map_err(map_core_error)?;
        Ok((resolved_id, resp))
    }
    .await;
    let (resolved_id, resp) = finish_direct(&core, outcome).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
    } else {
        println!("Inspiration appended to {resolved_id}");
    }

    Ok(())
}

/// Handle `creator works reopen` — reopen a completed Work (V1.45 P2).
///
/// Runs the core's completion-lock release
/// ([`CoreService::release_work_completion_lock`]): the DB
/// `completion_locked_at` is cleared (SSOT), `novel_completion_status` becomes
/// `reopened`, the derived `.completion-lock.json` is removed, and the required
/// `--reason` is recorded. The core refuses a Work that is not
/// completion-locked (`not_locked`) instead of accepting a no-op patch.
/// Migrated from `creator run resume --reopen`.
///
/// The retired `--extend-chapters` flag is gone with the transport it rode: no
/// retained request carries `total_planned_chapters` (neither the core
/// [`WorkPatchRequest`] nor [`ReleaseCompletionLockRequest`]), and the daemon's
/// own `PatchWorkRequest` had no such field, so the flag never reached a
/// writer.
///
/// # Errors
///
/// Returns [`crate::errors::CliError::Config`] for an over-long or
/// control-character `--reason`, and the typed core refusal (unknown Work, a
/// Work that is not completion-locked, no selected creator/workspace, a held
/// runtime lock).
async fn handle_reopen(
    config: &CliConfig,
    work_id: Option<String>,
    reason: &str,
    json: bool,
) -> Result<()> {
    // W-5: Cap and sanitize reason
    if reason.len() > 512 {
        return Err(crate::errors::CliError::Config(format!(
            "--reason exceeds maximum length (512 chars); got {} chars",
            reason.len()
        )));
    }
    if reason.contains('\x1b') || reason.chars().any(|c| c.is_control() && c != '\n') {
        return Err(crate::errors::CliError::Config(
            "--reason contains ANSI escape sequences or control characters".to_string(),
        ));
    }

    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let resolved_id = match work_id {
            Some(id) => id,
            None => active_work_id_core(&core, &principal).await?,
        };
        let work = core
            .release_work_completion_lock(
                &principal,
                resolved_id.clone(),
                ReleaseCompletionLockRequest {
                    reason: reason.to_string(),
                },
            )
            .await
            .map_err(map_core_error)?;
        Ok((resolved_id, work))
    }
    .await;
    let (resolved_id, work) = finish_direct(&core, outcome).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&work)?);
    } else {
        println!("Work {resolved_id} reopened for further writing.\nReason: {reason}");
    }

    Ok(())
}

/// Handle `creator works reconcile-chapters` — rebuild `work_chapters`
/// (V1.45 P2; V1.49 P2 adds `--dry-run` / `--yes`).
///
/// Scans the Work's `Stories/` directory and syncs `work_chapters` rows.
/// Migrated from `creator run reconcile-chapters`.
///
/// # Flag policy (V1.49 P2, R-V148P4-W2; overlay §8.2)
///
/// Mirrors `works rules reset` safety flags:
/// - `--dry-run`: compute the `ReconcileReport` only; **no** filesystem/DB
///   writes; **no** confirmation prompt; takes precedence over `--yes`.
/// - `--yes` (or `-y`): skip the confirmation prompt and mutate immediately.
/// - Default (neither flag): when stdin is a TTY, prompt before mutating;
///   when stdin is not a TTY, require `--yes` (error otherwise) so scripted
///   use cannot accidentally mutate without consent.
///
/// The core takes `dry_run` in its own request and skips the runtime lock and
/// every filesystem/DB write when it is set; the mutating path is unchanged
/// when `--dry-run` is absent.
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] on the typed core refusal, work
/// resolution failure, or non-interactive use without `--yes`.
async fn handle_reconcile_chapters(
    config: &CliConfig,
    work_id: Option<String>,
    dry_run: bool,
    yes: bool,
    json: bool,
) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        let resolved_id = match work_id {
            Some(id) => id,
            None => active_work_id_core(&core, &principal).await?,
        };

        // `--dry-run`: preview only, never write, never prompt. The core
        // computes the report without the runtime lock and without any
        // filesystem/DB write (overlay §8.2).
        if dry_run {
            let report = reconcile(&core, &principal, &resolved_id, true).await?;
            return Ok((resolved_id, ReconcileOutcome::Reconciled(report, true)));
        }

        // Mutating path: confirm unless `--yes` (mirror `works rules reset`).
        if !yes {
            if json {
                // Machine-readable mode cannot host an interactive prompt;
                // report that confirmation is required and write nothing.
                return Ok((
                    resolved_id.clone(),
                    ReconcileOutcome::ConfirmationRequired(serde_json::json!({
                        "work_id": resolved_id,
                        "reconciled": false,
                        "confirmation_required": true,
                        "hint": "pass --yes to proceed non-interactively, or --dry-run to preview",
                    })),
                ));
            }
            if !confirm_reconcile_interactive(&resolved_id)? {
                return Ok((resolved_id, ReconcileOutcome::Declined));
            }
        }

        let report = reconcile(&core, &principal, &resolved_id, false).await?;
        Ok((resolved_id, ReconcileOutcome::Reconciled(report, false)))
    }
    .await;
    let (resolved_id, outcome) = finish_direct(&core, outcome).await?;

    match outcome {
        ReconcileOutcome::ConfirmationRequired(body) => {
            println!("{}", serde_json::to_string_pretty(&body)?);
        }
        ReconcileOutcome::Declined => {
            println!("• Reconcile declined; work_chapters left unchanged.");
        }
        ReconcileOutcome::Reconciled(report, is_dry_run) => {
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&reconcile_report_wire(&report))?
                );
            } else {
                print_reconcile_report(&resolved_id, &report, is_dry_run);
            }
        }
    }

    Ok(())
}

/// What a `works reconcile-chapters` invocation decided, once its flags were
/// read inside the admitted writer.
///
/// The CLI renders every arm **after** [`finish_direct`] settles the writer, so
/// the decision travels out of the block instead of printing inside it.
enum ReconcileOutcome {
    /// `--json` without `--yes`: a machine consumer cannot confirm, so nothing
    /// was written and the body reports that confirmation is required.
    ConfirmationRequired(serde_json::Value),
    /// The interactive prompt was declined: nothing was written.
    Declined,
    /// A reconcile was previewed (`--dry-run`) or applied; `true` labels the
    /// human report as a dry run.
    Reconciled(WorkReconcileReport, bool),
}

/// Run the core reconcile for one Work.
///
/// [`CoreService::reconcile_work_chapters`] owns the existence check, the
/// runtime lock (mutating path only) and the report; `holder_kind` labels the
/// minted `cli:<kind>:<uuid>` lock holder, exactly as the daemon adapter passed
/// its own `http`.
///
/// # Errors
///
/// Returns the mapped core error (unknown Work, a held runtime lock, storage
/// failure).
async fn reconcile(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
    dry_run: bool,
) -> Result<WorkReconcileReport> {
    core.reconcile_work_chapters(
        principal,
        work_id.to_string(),
        "reconcile",
        ReconcileDryRunQuery {
            dry_run: Some(dry_run),
        },
    )
    .await
    .map_err(map_core_error)
}

/// Project the core reconcile report onto the retained wire shape.
///
/// `nexus_local_db::work_chapters::ReconcileReport` is what the daemon route
/// returned verbatim; the direct-core `--json` output keeps those field names
/// and types.
const fn reconcile_report_wire(
    report: &WorkReconcileReport,
) -> nexus_local_db::work_chapters::ReconcileReport {
    nexus_local_db::work_chapters::ReconcileReport {
        created: report.created,
        updated: report.updated,
        resynced: report.resynced,
        preserved: report.preserved,
    }
}

/// Render a `ReconcileReport` (created / updated / resynced / preserved) for
/// the human path. `is_dry_run` toggles the leading label.
fn print_reconcile_report(resolved_id: &str, report: &WorkReconcileReport, is_dry_run: bool) {
    let WorkReconcileReport {
        created,
        updated,
        resynced,
        preserved,
    } = *report;
    let label = if is_dry_run {
        "Dry run — no files modified. Reconcile preview"
    } else {
        "Reconcile complete"
    };
    println!("{label} for Work {resolved_id}:");
    println!("  Created:   {created}");
    println!("  Updated:   {updated}");
    println!("  Resynced:  {resynced}");
    println!("  Preserved: {preserved}");
}

/// Human-mode confirmation before a mutating reconcile (V1.49 P2).
///
/// Mirrors `rules_runtime::confirm_reset_interactive`. Returns `Ok(true)` when
/// the user confirms, `Ok(false)` when they decline. Errors when stdin is not a
/// terminal — callers should pass `--yes` for non-interactive use.
///
/// # Errors
///
/// Returns [`crate::errors::CliError`] when stdin is non-interactive.
fn confirm_reconcile_interactive(resolved_id: &str) -> Result<bool> {
    use std::io::IsTerminal;

    if !std::io::stdin().is_terminal() {
        return Err(crate::errors::CliError::Config(format!(
            "Reconciling work_chapters for Work {resolved_id} requires confirmation \
             but stdin is not a terminal. Pass --yes to proceed, or --dry-run to preview."
        )));
    }
    let confirmed = dialoguer::Confirm::new()
        .with_prompt(format!(
            "Reconcile work_chapters from filesystem for Work {resolved_id}? \
             This may create/update chapter rows and rewrite chapter frontmatter."
        ))
        .default(false)
        .show_default(true)
        .interact_opt()
        .map_err(|e| crate::errors::CliError::Other(format!("confirmation prompt failed: {e}")))?;
    Ok(confirmed == Some(true))
}

/// Audit reason recorded when `completion-lock release` performs the release:
/// the verb carries no `--reason` of its own, so the audit entry names the
/// command that released the lock. `works reopen --reason "…"` runs the same
/// producer with the caller's own text.
const COMPLETION_LOCK_RELEASE_REASON: &str =
    "released via `nexus42 creator works completion-lock release`";

/// Handle `creator works completion-lock`.
///
/// `release` runs the core's completion-lock release
/// ([`CoreService::release_work_completion_lock`]) — the single typed producer
/// for this governance action. It clears the DB `completion_locked_at` (SSOT),
/// sets `novel_completion_status = reopened` and deletes the derived
/// `.completion-lock.json`, so the Work is immediately writable again; the
/// core refuses a Work that is not completion-locked.
///
/// # Errors
///
/// Returns the typed core refusal (unknown Work, a Work that is not
/// completion-locked, no selected creator/workspace, storage failure) and any
/// cleanup refusal from [`finish_direct`].
async fn handle_completion_lock(config: &CliConfig, cmd: CompletionLockCommand) -> Result<()> {
    match cmd {
        CompletionLockCommand::Release { work_id, json } => {
            let core = open_direct_core(config).await?;
            let outcome = async {
                let principal = core.active_principal().await.map_err(map_core_error)?;
                core.release_work_completion_lock(
                    &principal,
                    work_id.clone(),
                    ReleaseCompletionLockRequest {
                        reason: COMPLETION_LOCK_RELEASE_REASON.to_string(),
                    },
                )
                .await
                .map_err(map_core_error)
            }
            .await;
            let work = finish_direct(&core, outcome).await?;

            if json {
                println!("{}", serde_json::to_string_pretty(&work)?);
            } else {
                println!("Completion lock released for Work {work_id}.");
            }
        }
    }

    Ok(())
}

// ── Selection pool handlers (DF-61) ────────────────────────────────────

/// Handle `creator works pool` — the selection pool and the pool-level
/// inspiration store, each on its own typed core producer.
///
/// The four pool-inspiration methods
/// ([`CoreService::add_work_inspiration`], `list_work_inspiration`,
/// `promote_work_inspiration`, `archive_work_inspiration`) address the
/// `inspiration_items` store and are distinct from the per-Work
/// `works.inspiration_log` that [`handle_inspire`] appends to through
/// [`CoreService::append_work_inspiration`]. The promotion itself stays the
/// core's single atomic transaction (Work create + pool promote + item
/// update) — this module never splits it into separate writes.
///
/// # Errors
///
/// Returns the typed core refusal (unknown Work/entry/item, an item that is
/// not `idea`, a cross-creator item, no selected creator/workspace) and any
/// cleanup refusal from [`finish_direct`].
async fn handle_pool(config: &CliConfig, action: PoolAction) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        match action {
            PoolAction::List { status, json } => {
                handle_pool_list(&core, &principal, status, json).await
            }
            PoolAction::Promote {
                work_id,
                set_default,
            } => handle_pool_promote(&core, &principal, &work_id, set_default).await,
            PoolAction::Archive { entry_id } => {
                handle_pool_archive(&core, &principal, &entry_id).await
            }
            PoolAction::Inspiration { action } => {
                handle_inspiration(&core, &principal, action).await
            }
        }
    }
    .await;
    // The leaves return their report; nothing reaches stdout until the shared
    // seam released the writer, so a promoted/archived entry is never reported
    // ahead of a close that did not settle.
    if let Some(text) = finish_direct(&core, outcome).await? {
        println!("{text}");
    }
    Ok(())
}

async fn handle_pool_list(
    core: &CoreService,
    principal: &Principal,
    status: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .list_work_pool(
            principal,
            ListPoolQuery {
                status,
                limit: None,
                offset: None,
            },
        )
        .await
        .map_err(map_core_error)?;

    if json {
        let entries: Vec<PoolListEntry> = resp.entries.into_iter().map(pool_entry_wire).collect();
        let wire = WorkPoolListResponse {
            entries,
            total: u64::from(resp.total),
            limit: u64::from(resp.limit),
            offset: u64::from(resp.offset),
        };
        return Ok(Some(serde_json::to_string_pretty(&wire)?));
    }
    if resp.entries.is_empty() {
        return Ok(Some("No pool entries found.".to_string()));
    }
    let mut lines = vec![format!(
        "{:<36} {:36} {:12} {:30} PROMOTED",
        "ENTRY_ID", "WORK_ID", "STATUS", "TITLE"
    )];
    for e in &resp.entries {
        let eid = e.entry_id.as_str();
        let wid = if e.work_id.is_empty() {
            "(none)"
        } else {
            e.work_id.as_str()
        };
        let st = e.status.as_str();
        let title = e.title.as_str();
        let promoted = e.promoted_at.as_str();
        let display_title = truncate_with_ellipsis(title, 28);
        lines.push(format!(
            "{eid:<36} {wid:<36} {st:<12} {display_title:<30} {promoted}"
        ));
    }
    lines.push(format!("\n{} pool entry/entries", resp.entries.len()));
    Ok(Some(lines.join("\n")))
}

async fn handle_pool_promote(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
    set_default: bool,
) -> Result<Option<String>> {
    let entry = core
        .promote_work_pool_entry(
            principal,
            PromotePoolRequest {
                work_id: work_id.to_string(),
                set_default: Some(set_default),
            },
        )
        .await
        .map_err(map_core_error)?;

    let mut lines = vec![format!(
        "Promoted {work_id} to active (entry {})",
        entry.entry_id
    )];

    if set_default {
        // `works use` semantics: the pool `active` row is the CLI default. The
        // retired adapter issued this selection as a second control request;
        // the core promotes to `active` on both paths.
        core.select_work(principal, work_id.to_string())
            .await
            .map_err(map_core_error)?;
        lines.push("Also set as CLI default work.".to_string());
    }

    Ok(Some(lines.join("\n")))
}

async fn handle_pool_archive(
    core: &CoreService,
    principal: &Principal,
    entry_id: &str,
) -> Result<Option<String>> {
    let entry = core
        .archive_work_pool_entry(
            principal,
            ArchivePoolRequest {
                entry_id: entry_id.to_string(),
            },
        )
        .await
        .map_err(map_core_error)?;

    Ok(Some(format!("Entry {entry_id} → {}", entry.status)))
}

// ── Inspiration pool handlers (DF-61 §4) ───────────────────────────────

async fn handle_inspiration(
    core: &CoreService,
    principal: &Principal,
    action: InspirationAction,
) -> Result<Option<String>> {
    match action {
        InspirationAction::Add { title, json } => {
            handle_inspiration_add(core, principal, &title, json).await
        }
        InspirationAction::List { status, json } => {
            handle_inspiration_list(core, principal, status, json).await
        }
        InspirationAction::Promote {
            item_id,
            idea,
            set_default,
        } => handle_inspiration_promote(core, principal, &item_id, idea, set_default).await,
        InspirationAction::Archive { item_id } => {
            handle_inspiration_archive(core, principal, &item_id).await
        }
    }
}

async fn handle_inspiration_add(
    core: &CoreService,
    principal: &Principal,
    title: &str,
    json: bool,
) -> Result<Option<String>> {
    let added = core
        .add_work_inspiration(
            principal,
            AddInspirationRequest {
                title: title.to_string(),
            },
        )
        .await
        .map_err(map_core_error)?;

    Ok(Some(if json {
        let wire = WorkInspirationAddResponse {
            item_id: added.item_id,
            rel_path: added.rel_path,
        };
        serde_json::to_string_pretty(&wire)?
    } else {
        format!(
            "Inspiration added: {}\n  scaffold: {}",
            added.item_id, added.rel_path
        )
    }))
}

async fn handle_inspiration_list(
    core: &CoreService,
    principal: &Principal,
    status: Option<String>,
    json: bool,
) -> Result<Option<String>> {
    let resp = core
        .list_work_inspiration(
            principal,
            ListInspirationQuery {
                status,
                limit: None,
                offset: None,
            },
        )
        .await
        .map_err(map_core_error)?;

    if json {
        let items: Vec<InspirationListItem> =
            resp.items.into_iter().map(inspiration_item_wire).collect();
        let wire = WorkInspirationListResponse {
            items,
            total: u64::from(resp.total),
            limit: u64::from(resp.limit),
            offset: u64::from(resp.offset),
        };
        return Ok(Some(serde_json::to_string_pretty(&wire)?));
    }
    if resp.items.is_empty() {
        return Ok(Some("No inspiration items found.".to_string()));
    }
    let mut lines = vec![format!(
        "{:<36} {:40} {:12} {:30} CREATED",
        "ITEM_ID", "TITLE", "STATUS", "REL_PATH"
    )];
    for i in &resp.items {
        let iid = i.item_id.as_str();
        let title = i.title.as_str();
        let st = i.status.as_str();
        let rp = i.rel_path.as_str();
        let created = i.created_at.as_str();
        let display_title = truncate_with_ellipsis(title, 38);
        let display_rp = if rp.len() > 28 {
            format!("{}…", &rp[..28])
        } else {
            rp.to_string()
        };
        lines.push(format!(
            "{iid:<36} {display_title:40} {st:<12} {display_rp:<30} {created}"
        ));
    }
    lines.push(format!("\n{} inspiration item(s)", resp.items.len()));
    Ok(Some(lines.join("\n")))
}

async fn handle_inspiration_promote(
    core: &CoreService,
    principal: &Principal,
    item_id: &str,
    idea: Option<String>,
    set_default: bool,
) -> Result<Option<String>> {
    let promoted = core
        .promote_work_inspiration(
            principal,
            PromoteInspirationRequest {
                item_id: item_id.to_string(),
                idea,
                set_default: Some(set_default),
            },
        )
        .await
        .map_err(map_core_error)?;

    let mut lines = vec![format!(
        "Inspiration {item_id} promoted → Work {} (pool entry {})",
        promoted.work_id, promoted.pool_entry_id
    )];

    if set_default {
        // The atomic promotion already wrote the new Work as the pool `active`
        // row; this is the retained `works use` selection on its own request.
        core.select_work(principal, promoted.work_id.clone())
            .await
            .map_err(map_core_error)?;
        lines.push("Also set as CLI default work.".to_string());
    }

    Ok(Some(lines.join("\n")))
}

async fn handle_inspiration_archive(
    core: &CoreService,
    principal: &Principal,
    item_id: &str,
) -> Result<Option<String>> {
    core.archive_work_inspiration(
        principal,
        ArchiveInspirationRequest {
            item_id: item_id.to_string(),
        },
    )
    .await
    .map_err(map_core_error)?;

    Ok(Some(format!("Inspiration item {item_id} archived.")))
}

/// Schema-owned pool-list element (`--json` only).
type PoolListEntry =
    nexus_contracts::generated::core::works::work_pool_list_response::WorkPoolEntry;

/// Schema-owned inspiration-list element (`--json` only).
type InspirationListItem =
    nexus_contracts::generated::core::works::work_inspiration_list_response::WorkInspirationItem;

/// Project a core pool entry onto the wire shape the pool list serves.
///
/// The stored `creator_id` never reaches the output (R-V141P1-11 — local-first,
/// always the active creator), exactly as the retired daemon adapter and the
/// native pool-list projection omit it.
fn pool_entry_wire(entry: nexus_core::WorkPoolEntry) -> PoolListEntry {
    PoolListEntry {
        entry_id: entry.entry_id,
        work_id: entry.work_id,
        status: entry.status,
        title: entry.title,
        promoted_at: entry.promoted_at,
        note: entry.note,
    }
}

/// Project a core pool-inspiration item onto the wire shape the pool
/// inspiration list serves (stored `creator_id` intentionally not serialized).
fn inspiration_item_wire(item: nexus_core::WorkInspirationItem) -> InspirationListItem {
    InspirationListItem {
        item_id: item.item_id,
        rel_path: item.rel_path,
        title: item.title,
        status: item.status,
        promoted_work_id: item.promoted_work_id,
        created_at: item.created_at,
        promoted_at: item.promoted_at,
    }
}

// ── Shared display helpers (V1.42 P-last R-V141P0-02 dedup) ───────────

/// Hard cap on the number of open findings the status enrichment reads.
///
/// The cap is also the truncation marker: exactly this many rows means more
/// may exist beyond the read page.
const FINDINGS_FETCH_LIMIT: usize = 50;

/// Stale-findings threshold for the status banner: 96h.
///
/// The retired daemon adapter resolved this from
/// `NEXUS_DAEMON_STALE_FINDINGS_THRESHOLD_SECS` (a daemon-runtime concern that
/// stays there); the direct CLI reads the same default the old banner fallback
/// used.
const STALE_FINDINGS_THRESHOLD_SECS: i64 = 96 * 60 * 60;

/// Result of reading a Work's open findings.
///
/// Distinguishes "successfully read 0 findings" from "read failed", so the
/// display layer can print distinct messages.
enum FindingsResult {
    /// Findings were read successfully.
    Fetched(Vec<serde_json::Value>),
    /// The read did not return findings (storage error, unknown Work, …).
    Unavailable,
}

impl FindingsResult {
    /// The read slice, or `None` when the read failed (graceful degradation).
    fn as_slice(&self) -> Option<&[serde_json::Value]> {
        match self {
            Self::Fetched(items) => Some(items),
            Self::Unavailable => None,
        }
    }
}

/// Read the Work's open findings — best-effort, `Unavailable` on failure.
///
/// V1.43 P2 (T2): used by `handle_status` to satisfy spec §4 row 3
/// ("Are there open findings? Count + severity summary"). The rows are the
/// findings list-API element shape verbatim, so the `--json` status carries the
/// same array the list leaf serves.
///
/// R-V146P0-QC3-S2: observe the silent degradation path — a failed read must
/// not vanish into `Unavailable` without a trace.
async fn fetch_open_findings(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
) -> FindingsResult {
    let read = core
        .list_findings(
            principal,
            work_id.to_string(),
            ListFindingsQuery {
                status: Some("open".to_string()),
                limit: Some(u32::try_from(FINDINGS_FETCH_LIMIT).unwrap_or(u32::MAX)),
                ..ListFindingsQuery::default()
            },
        )
        .await;
    match read {
        Ok(resp) => {
            if let Ok(serde_json::Value::Array(items)) = serde_json::to_value(&resp.items) {
                FindingsResult::Fetched(items)
            } else {
                // A serialization failure is the same class as a failed read: the
                // status command degrades instead of reporting a findings list it
                // cannot render.
                tracing::warn!(
                    work_id = %work_id,
                    "open findings read could not be rendered; degrading to Unavailable"
                );
                FindingsResult::Unavailable
            }
        }
        Err(error) => {
            tracing::warn!(
                work_id = %work_id,
                error = %error,
                "open findings read failed; degrading to Unavailable"
            );
            FindingsResult::Unavailable
        }
    }
}

/// Read the creator-global stale-findings summary — best-effort, `None` on
/// failure (parity with the human stale banner).
///
/// The returned value is the wire shape `GET /v1/daemon/findings/stale`
/// served, so the `--json` status keeps its key set.
async fn fetch_stale_findings(
    core: &CoreService,
    principal: &Principal,
) -> Option<serde_json::Value> {
    let report = core
        .list_stale_findings(principal, STALE_FINDINGS_THRESHOLD_SECS)
        .await
        .map_err(|error| {
            // R-V146P0-QC3-S2: observe the silent swallow — a failed stale
            // read must not vanish into `None` without a trace.
            tracing::warn!(
                error = %error,
                "stale findings read failed; degrading to None"
            );
        })
        .ok()?;
    serde_json::to_value(stale_findings_wire(report)).ok()
}

/// Project the core stale report onto the wire shape the retired stale route
/// served (same keys and types, so `findings_stale` is unchanged).
fn stale_findings_wire(report: CoreStaleFindingsResponse) -> StaleFindingsWire {
    StaleFindingsWire {
        stale_count: report.stale_count,
        threshold_seconds: report.threshold_seconds,
        now_epoch: report.now_epoch,
        findings: report
            .findings
            .into_iter()
            .map(|entry| StaleFindingWire {
                finding_id: entry.finding_id,
                work_id: entry.work_id,
                severity: entry.severity,
                created_at: entry.created_at,
                age_seconds: u64::try_from(entry.age_seconds).unwrap_or(0),
            })
            .collect(),
    }
}

/// Read the findings/stale enrichment for `creator works status`.
///
/// `novel` gates both reads (Grill #6 — the enrichment is novel-only); `human`
/// additionally gates the stale read, which the human path renders as a banner
/// for every profile. Both reads are independent and overlap via
/// `tokio::join!` (the retained qc3 F-001 policy that the status hot path must
/// not stack the two reads' latencies), and each degrades on its own.
async fn fetch_status_enrichment(
    core: &CoreService,
    principal: &Principal,
    work_id: &str,
    novel: bool,
    human: bool,
) -> (FindingsResult, Option<serde_json::Value>) {
    let findings = async {
        if novel {
            fetch_open_findings(core, principal, work_id).await
        } else {
            FindingsResult::Fetched(Vec::new())
        }
    };
    let stale = async {
        if novel || human {
            fetch_stale_findings(core, principal).await
        } else {
            None
        }
    };
    tokio::join!(findings, stale)
}

/// Format the human-path stale-findings banner line (R-V146P0-QC3-S3).
///
/// Pure over `(stale_count, threshold_seconds)`. Returns `Some(banner)` only
/// when `stale_count > 0` (no banner noise when nothing is stale); `None`
/// otherwise so the caller skips printing. Extracted from the inline banner
/// block so the rendering + zero-suppression is hermetically unit-testable,
/// and so both the threshold math and the spec citation live in one place.
fn format_stale_banner(stale_count: u64, threshold_seconds: i64) -> Option<String> {
    if stale_count == 0 {
        return None;
    }
    let threshold_hours = threshold_seconds / 3600;
    // V1.47 P1: normalize user-facing copy — spec name, not repo path.
    // V1.46 P1 (spec hygiene): cite spec, not deleted quickstart.
    Some(format!(
        "⏰ {stale_count} finding(s) stale (>{threshold_hours}h) — \
         address open findings or run a review pass; \
         see novel-author-experience §4"
    ))
}

/// V1.46 P0: enrich the Work payload with novel-only findings.
///
/// For `work_profile=novel` only (Grill #6), inserts a root-level `findings`
/// array matching the findings list-API element shape verbatim (spec §4.1),
/// and an optional `findings_stale` object when the 96h master-review stale
/// banner would show (human parity). Generic / non-novel works are returned
/// unchanged (novel-only gate).
///
/// `findings`: `Some(slice)` when the findings read succeeded (possibly empty);
///             `None` when it failed — `findings` is then omitted for graceful
///             degradation (mirrors the human "unavailable" path), since
///             fabricating an empty array would mask a storage fault.
///             When `slice.len() == FINDINGS_FETCH_LIMIT`, a `findings_truncated`
///             boolean is also inserted so JSON consumers can detect that more
///             open findings may exist beyond the read page (qc3 F-003).
///
/// `stale`: the stale-findings report; `findings_stale` is inserted only when
///          its `stale_count` is greater than zero.
fn enrich_status_json(
    mut resp: serde_json::Value,
    findings: Option<&[serde_json::Value]>,
    stale: Option<&serde_json::Value>,
) -> serde_json::Value {
    let is_novel = resp.get("work_profile").and_then(serde_json::Value::as_str) == Some("novel");
    if !is_novel {
        return resp;
    }
    let Some(obj) = resp.as_object_mut() else {
        return resp;
    };
    if let Some(arr) = findings {
        let is_truncated = arr.len() == FINDINGS_FETCH_LIMIT;
        obj.insert(
            "findings".to_string(),
            serde_json::Value::Array(arr.to_vec()),
        );
        // qc3 F-003: surface the 50-item fetch cap so JSON consumers can
        // distinguish "exactly 50 open findings" from "50+ open findings".
        // Omitted when not at the cap (consumers treat absence as not-truncated).
        if is_truncated {
            obj.insert(
                "findings_truncated".to_string(),
                serde_json::Value::Bool(true),
            );
        }
    }
    if let Some(stale_obj) = stale {
        let stale_count = stale_obj
            .get("stale_count")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0);
        if stale_count > 0 {
            obj.insert("findings_stale".to_string(), stale_obj.clone());
        }
    }
    resp
}

/// Parsed open-findings summary for display formatting.
///
/// Extracted as a struct to enable hermetic unit testing of
/// `format_findings_summary` without daemon client dependency.
#[derive(Debug, Default)]
struct FindingsSummary {
    /// Total open finding count.
    open_count: usize,
    /// Whether the count is truncated (server returned exactly the limit).
    is_truncated: bool,
    /// Highest severity among open findings (ordered: blocker > major > minor > info).
    highest_severity: Option<String>,
    /// Per-severity counts for the summary line.
    severity_counts: Vec<(String, usize)>,
    /// Top findings (up to 5) with title, severity, and routing hint.
    top_findings: Vec<(String, String, String)>,
}

impl FindingsSummary {
    /// Parse from the JSON array returned by the findings list endpoint.
    ///
    /// `is_truncated` should be `true` when the server returned exactly
    /// `FINDINGS_FETCH_LIMIT` rows, indicating there may be more findings
    /// beyond the fetched page.
    fn from_findings_json(findings: &[serde_json::Value], is_truncated: bool) -> Self {
        if findings.is_empty() {
            return Self::default();
        }

        let open_count = findings.len();

        // Severity priority order (highest first).
        let severity_order = ["blocker", "major", "minor", "info"];
        let mut severity_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut highest_severity: Option<String> = None;

        for f in findings {
            let sev = f
                .get("severity")
                .and_then(|v| v.as_str())
                .unwrap_or("info")
                .to_string();
            *severity_counts.entry(sev.clone()).or_insert(0) += 1;

            // Track highest severity.
            let current_rank = severity_order.iter().position(|s| *s == sev);
            let highest_rank = highest_severity
                .as_ref()
                .and_then(|h| severity_order.iter().position(|s| *s == h));
            if current_rank.is_none_or(|c| highest_rank.is_none_or(|h| c < h)) {
                highest_severity = Some(sev);
            }
        }

        // Sort severity counts by priority order.
        let mut severity_vec: Vec<(String, usize)> = severity_counts.into_iter().collect();
        severity_vec.sort_by(|a, b| {
            let ra = severity_order.iter().position(|s| *s == a.0);
            let rb = severity_order.iter().position(|s| *s == b.0);
            ra.cmp(&rb)
        });

        // Top 5 findings with (title, severity, routing_hint).
        let top_findings = findings
            .iter()
            .take(5)
            .map(|f| {
                let title = f
                    .get("title")
                    .and_then(|v| v.as_str())
                    .unwrap_or("(untitled)")
                    .to_string();
                let sev = f
                    .get("severity")
                    .and_then(|v| v.as_str())
                    .unwrap_or("?")
                    .to_string();
                let hint = f
                    .get("routing_hint")
                    .and_then(|v| v.as_str())
                    .unwrap_or("→ none")
                    .to_string();
                (title, sev, hint)
            })
            .collect();

        Self {
            open_count,
            is_truncated,
            highest_severity,
            severity_counts: severity_vec,
            top_findings,
        }
    }
}

/// Format and print the open-findings summary block for `creator works status`.
///
/// Per spec §4 row 3: "Count + severity summary; link to review preset name."
/// Per cli-spec §7.1: clear, non-jargon formatting.
///
/// - `FindingsResult::Fetched(vec)` with empty vec → one line: "none open"
///   (v1.193 P2-T1: the `creator run novel-review-master` suggestion went with
///   the removed runner — no CLI preset dispatch entrance is claimed)
/// - `FindingsResult::Unavailable` → "findings: unavailable (daemon error)"
fn print_findings_summary(result: &FindingsResult) {
    // R-V146P0-QC1-S2: formatting lives in the pure `format_findings_summary_lines`
    // helper so the test helper `capture_findings_output` cannot drift from
    // production. This wrapper only prints each rendered line.
    for line in format_findings_summary_lines(result) {
        println!("{line}");
    }
}

/// Render the open-findings summary as a vector of display lines (pure).
///
/// Extracted from `print_findings_summary` (R-V146P0-QC1-S2) as the single
/// formatting path shared by the production printer and the test helper
/// `capture_findings_output` (previously the helper mirrored the production
/// logic, risking silent drift).
///
/// - `FindingsResult::Unavailable` → one-line `["findings: unavailable (daemon error)"]`
/// - `FindingsResult::Fetched([])` → one line: "none open"
/// - `FindingsResult::Fetched([...])` → summary line + top findings (sanitized)
fn format_findings_summary_lines(result: &FindingsResult) -> Vec<String> {
    let findings = match result {
        FindingsResult::Unavailable => {
            return vec!["findings: unavailable (daemon error)".to_string()];
        }
        FindingsResult::Fetched(vec) => vec,
    };

    let is_truncated = findings.len() == FINDINGS_FETCH_LIMIT;
    let summary = FindingsSummary::from_findings_json(findings, is_truncated);
    if summary.open_count == 0 {
        // V1.46 P0 (Grill #7) suggested a master-decision pass here; v1.193
        // P2-T1 removed the `creator run` entrance it named, and no CLI preset
        // dispatch replaced it, so the empty case reports the state alone.
        return vec!["findings: none open".to_string()];
    }

    // Summary line: "findings: 3 open (1 blocker, 1 major, 1 info)"
    // Truncated: "findings: 50+ open (...)"
    let count_display = if summary.is_truncated {
        format!("{}+", summary.open_count)
    } else {
        format!("{}", summary.open_count)
    };
    let sev_parts: Vec<String> = summary
        .severity_counts
        .iter()
        .map(|(sev, count)| format!("{count} {sev}"))
        .collect();
    let sev_summary = sev_parts.join(", ");
    let highest_tag = summary
        .highest_severity
        .as_ref()
        .map_or(String::new(), |h| format!(" — highest: {h}"));

    let mut lines = vec![format!(
        "findings: {count_display} open ({sev_summary}){highest_tag}"
    )];

    // Top findings with routing hints (sanitized).
    for (i, (title, sev, hint)) in summary.top_findings.iter().enumerate() {
        let safe_title = sanitize_for_terminal(title);
        let safe_hint = sanitize_for_terminal(hint);
        let display_title = truncate_with_ellipsis(&safe_title, 48);
        lines.push(format!(
            "  #{} [{sev}] \"{display_title}\" {safe_hint}",
            i + 1
        ));
    }
    lines
}

/// V1.46 P2 QC fix W-001: maximum number of chapters that receive
/// per-row on-disk path hints in `print_chapter_table`. Bounds the
/// synchronous `Path::exists()` syscall cost on large works (100+
/// chapters). Chapters beyond this cap are summarized in a single
/// line; the per-chapter `exists()` behavior itself (Grill #9) is
/// preserved for the first `CHAPTER_PATH_HINT_CAP` rows.
const CHAPTER_PATH_HINT_CAP: usize = 50;

/// Print per-chapter status table for novel works.
///
/// V1.46 P2 (Grill #9; R-V139P5-N1): best-effort on-disk check of each
/// chapter's configured `body_path` / `outline_path`. When a configured
/// path is missing on disk, a ⚠ marker plus a `works reconcile-chapters`
/// remediation hint is printed below the row. The daemon reconcile pass
/// remains authoritative; this is a CLI-only surfacing hint. All
/// filesystem errors are swallowed (best-effort) and never fail the status
/// command.
///
/// V1.46 P2 QC fix W-001: per-chapter `exists()` hints are capped at
/// `CHAPTER_PATH_HINT_CAP` to prevent tail latency on large works; a
/// summary line covers chapters beyond the cap.
fn print_chapter_table(chapters: &[serde_json::Value], work_id: &str) {
    // Resolve the operational workspace dir once (best-effort). When this
    // cannot be resolved (no active creator, no home dir, etc.), on-disk
    // hints are silently skipped — the table still renders normally.
    let ws_dir = operational_workspace_dir_from_config();

    println!();
    println!(
        "{:<5} {:<30} {:<14} {:<14}",
        "CH", "TITLE", "STATUS", "UPDATED"
    );

    // V1.46 P2 QC fix W-001: bound per-chapter `Path::exists()` hints at
    // `CHAPTER_PATH_HINT_CAP` to prevent synchronous-filesystem-syscall
    // tail latency on large works (100+ chapters on slower storage).
    // Chapters beyond the cap are summarized in a single line below the
    // table; the per-chapter `exists()` behavior (Grill #9) is preserved
    // for the first `hint_cap` rows.
    let total_chapters = chapters.len();
    let hint_cap = total_chapters.min(CHAPTER_PATH_HINT_CAP);

    // tracing span records chapter count + effective cap; the >100ms
    // threshold log below surfaces slow loops without spamming fast SSDs.
    let hint_loop_elapsed_ms = {
        let span = tracing::info_span!("chapter_path_hints", total_chapters, capped = hint_cap,);
        let _enter = span.enter();
        let start = std::time::Instant::now();

        for (idx, ch) in chapters.iter().enumerate() {
            let num = ch
                .get("chapter_number")
                .and_then(serde_json::Value::as_i64)
                .unwrap_or(0);
            let ch_title = ch
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("(untitled)");
            let ch_status = ch.get("status").and_then(|v| v.as_str()).unwrap_or("?");
            let ch_updated = ch.get("updated_at").and_then(|v| v.as_str()).unwrap_or("?");
            let display_title = truncate_with_ellipsis(ch_title, 28);
            println!("{num:<5} {display_title:<30} {ch_status:<14} {ch_updated:<14}");

            // V1.46 P2 (Grill #9): on-disk path hint, best-effort; capped
            // by CHAPTER_PATH_HINT_CAP (W-001). Chapters beyond `hint_cap`
            // still render their row above, but skip the exists() check.
            if idx < hint_cap {
                if let Some(ref dir) = ws_dir {
                    if let Some(reason) = chapter_path_missing_hint(ch, dir) {
                        let safe_work_id = sanitize_for_terminal(work_id);
                        println!(
                            "  ⚠ {reason} — run: nexus42 creator works reconcile-chapters {safe_work_id}"
                        );
                    }
                }
            }
        }

        start.elapsed().as_millis()
    };

    if hint_loop_elapsed_ms > 100 {
        tracing::info!(
            elapsed_ms = hint_loop_elapsed_ms,
            chapters_checked = hint_cap,
            "chapter path hint loop took >100ms",
        );
    }

    // V1.46 P2 QC fix W-001: summary line for chapters beyond the cap.
    // Emitted only when the workspace was resolved (otherwise the entire
    // hint feature was silently skipped and a summary would mislead the
    // user into thinking paths were checked).
    if ws_dir.is_some() {
        if let Some(summary) =
            chapter_path_hint_skipped_summary(total_chapters.saturating_sub(hint_cap))
        {
            println!("  {summary}");
        }
    }
}

/// V1.46 P2 (Grill #9): resolve the operational workspace directory from
/// the active CLI config. Returns `None` on any failure (best-effort) —
/// callers must treat `None` as "skip on-disk hints".
fn operational_workspace_dir_from_config() -> Option<std::path::PathBuf> {
    let cfg = crate::config::CliConfig::load().ok()?;
    let creator_id = cfg.active_creator_id.as_ref()?;
    let ws_slug = cfg.active_workspace_slug_by_creator.get(creator_id)?;
    let home = dirs::home_dir()?;
    Some(nexus_home_layout::operational_workspace_dir(
        &home, creator_id, ws_slug,
    ))
}

/// V1.48 P2: crate-public re-export so `rules_runtime` can resolve the
/// operational workspace dir for `AGENTS.md` file operations. Same
/// semantics as [`operational_workspace_dir_from_config`] (best-effort).
pub(crate) fn operational_workspace_dir_from_config_public() -> Option<std::path::PathBuf> {
    operational_workspace_dir_from_config()
}

/// V1.51 T-B P0: read the lock holder info from the filesystem and return
/// it as a JSON value for `creator works status --json`. Returns `None` if
/// the `work_ref` is missing, the workspace directory cannot be resolved, or
/// the lock file doesn't exist / is unreadable.
fn read_lock_holder_json(work_resp: &serde_json::Value) -> Option<serde_json::Value> {
    let work_ref = work_resp.get("work_ref")?.as_str()?;
    let ws_dir = operational_workspace_dir_from_config()?;
    let work_dir = ws_dir.join("Works").join(work_ref);

    #[cfg(unix)]
    {
        let info = nexus_local_db::file_lock::read_lock_holder_info(&work_dir)?;
        let mut obj = serde_json::Map::new();
        obj.insert(
            "pid".to_string(),
            serde_json::Value::Number(info.pid.into()),
        );
        obj.insert(
            "holder_name".to_string(),
            serde_json::Value::String(info.holder_name),
        );
        obj.insert(
            "expires_at_ms".to_string(),
            serde_json::Value::Number(info.expires_at_ms.into()),
        );
        if info.stale {
            obj.insert("stale".to_string(), serde_json::Value::Bool(true));
        }
        Some(serde_json::Value::Object(obj))
    }
    #[cfg(not(unix))]
    {
        let _ = work_dir;
        None
    }
}

/// V1.46 P2 (Grill #9; R-V139P5-N1): best-effort check of a chapter's
/// configured `body_path` / `outline_path` against the filesystem.
///
/// Returns `Some(reason)` when at least one configured path is missing on
/// disk; `None` when both paths exist, when neither is configured, or when
/// the workspace cannot be resolved. `Path::exists()` semantics swallow
/// permission/IO errors as `false`, which is the desired best-effort
/// behavior (a missing file and an unreadable file both warrant reconcile).
///
/// Pure over `(chapter JSON, ws_dir)` — hermetically testable with a
/// tempdir for `ws_dir`.
fn chapter_path_missing_hint(ch: &serde_json::Value, ws_dir: &std::path::Path) -> Option<String> {
    let body = ch.get("body_path").and_then(serde_json::Value::as_str);
    let outline = ch.get("outline_path").and_then(serde_json::Value::as_str);
    let body_missing = body.is_some_and(|p| !ws_dir.join(p).exists());
    let outline_missing = outline.is_some_and(|p| !ws_dir.join(p).exists());
    if !body_missing && !outline_missing {
        return None;
    }
    let mut parts = Vec::new();
    if body_missing {
        parts.push("body_path");
    }
    if outline_missing {
        parts.push("outline_path");
    }
    Some(format!("{} missing on disk", parts.join(", ")))
}

/// V1.46 P2 QC fix W-001: pure helper that formats the `"+ N more
/// (paths not checked)"` summary line for chapters beyond the hint cap.
/// Returns `None` when `skipped == 0` so the caller can skip rendering.
///
/// Pure over `skipped` — unit-tested independently of `print_chapter_table`.
fn chapter_path_hint_skipped_summary(skipped: usize) -> Option<String> {
    (skipped > 0).then(|| format!("+ {skipped} more (paths not checked)"))
}

/// V1.42 P-last (R-V141P0-06): best-effort on-disk completion-lock file check.
///
/// DB `completion_locked_at` is the authoritative lock state. The
/// `.completion-lock.json` file is a derived artifact. When the file is
/// missing, surface a hint to the user.
fn print_completion_lock_hint(work_ref: &str, work_id: &str) {
    if work_ref.starts_with('(') {
        return;
    }
    // R-V146P2-QC1-S2: route through the shared `operational_workspace_dir_from_config`
    // helper instead of re-implementing the config → creator_id → workspace_slug →
    // home → operational_workspace_dir lookup inline. The helper has identical
    // best-effort semantics (None on any resolution failure); the prior ad-hoc
    // block was a verbatim duplicate of that resolution chain.
    let Some(ws_dir) = operational_workspace_dir_from_config() else {
        return;
    };
    let lock_path = ws_dir
        .join("Works")
        .join(work_ref)
        .join(".completion-lock.json");
    if !lock_path.exists() {
        println!("⚠ completion-lock file missing (DB says locked but file not found)");
        // V1.45 P2: hint updated from `run reconcile-chapters` to `works reconcile-chapters`.
        println!("  Run: nexus42 creator works reconcile-chapters {work_id}");
    }
}

/// Truncate a string to `max_len` characters, appending `…` if truncated.
fn truncate_with_ellipsis(s: &str, max_len: usize) -> String {
    if s.len() > max_len {
        format!("{}…", &s[..max_len])
    } else {
        s.to_string()
    }
}

/// Strip ASCII control characters and ANSI escape sequences from a string
/// to prevent terminal display corruption from user-supplied data.
///
/// Preserves printable ASCII, Unicode, `\n`, and `\t`. Strips:
/// - ASCII control chars 0x00–0x1F (except `\n` 0x0A and `\t` 0x09) and 0x7F (DEL)
/// - ANSI CSI sequences (`ESC [ ... letter`)
fn sanitize_for_terminal(s: &str) -> String {
    // Phase 1: strip ANSI CSI sequences (ESC [ <params> <letter>).
    let ansi_re = regex::Regex::new(r"\x1B\[[0-9;]*[a-zA-Z]").unwrap_or_else(|e| {
        // The pattern is a compile-time constant; panic is unreachable.
        unreachable!("invalid ANSI regex pattern: {e}")
    });
    let stripped = ansi_re.replace_all(s, "");

    // Phase 2: remove remaining ASCII control chars (keep \n, \t, and printable).
    stripped
        .chars()
        .filter(|&c| {
            if c == '\n' || c == '\t' {
                return true;
            }
            let code = c as u32;
            // Allow printable chars: space (0x20) and above, excluding DEL (0x7F).
            // Below 0x20 are control chars — filter them out.
            code >= 0x20 && code != 0x7F
        })
        .collect()
}

// ── Tests (V1.43 P2 T4) ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn finding_json(severity: &str, title: &str, routing_hint: &str) -> serde_json::Value {
        serde_json::json!({
            "finding_id": format!("fnd_test_{}", title.len()),
            "severity": severity,
            "title": title,
            "routing_hint": routing_hint,
            "status": "open",
        })
    }

    /// R-V146P0-QC2-S1: build a finding element with the FULL list-API shape
    /// (every field the findings list endpoint returns), not the minimal
    /// `finding_json` subset. Used to assert `enrich_status_json` preserves
    /// the element verbatim (full shape fidelity, not just a few fields).
    fn full_finding_json() -> serde_json::Value {
        serde_json::json!({
            "finding_id": "fnd_01H8XK9Q2VTestFidelityFullShape",
            "work_id": "wrk_full_shape",
            "chapter": 7,
            "severity": "blocker",
            "status": "open",
            "title": "Continuity error in chapter 7",
            "description": "Character name changes between paragraphs 3 and 9.",
            "routing_hint": "→ write",
            "target_executor": "write",
            "creator_id": "ctr_full_shape",
            "kind": "continuity",
            "rule_suggestion": "Track character names per chapter.",
            "created_at": 1_718_000_000,
            "updated_at": 1_718_000_123,
        })
    }

    // ── FindingsSummary parsing tests ────────────────────────────────────

    #[test]
    fn findings_summary_empty() {
        let summary = FindingsSummary::from_findings_json(&[], false);
        assert_eq!(summary.open_count, 0);
        assert!(!summary.is_truncated);
        assert!(summary.highest_severity.is_none());
        assert!(summary.severity_counts.is_empty());
        assert!(summary.top_findings.is_empty());
    }

    #[test]
    fn findings_summary_single_finding() {
        let findings = vec![finding_json("major", "Plot hole", "→ write")];
        let summary = FindingsSummary::from_findings_json(&findings, false);
        assert_eq!(summary.open_count, 1);
        assert!(!summary.is_truncated);
        assert_eq!(summary.highest_severity.as_deref(), Some("major"));
        assert_eq!(summary.severity_counts, vec![("major".to_string(), 1)]);
        assert_eq!(summary.top_findings.len(), 1);
        assert_eq!(summary.top_findings[0].0, "Plot hole");
    }

    #[test]
    fn findings_summary_mixed_severities() {
        let findings = vec![
            finding_json("info", "Style note", "→ none"),
            finding_json("blocker", "Continuity error", "→ write"),
            finding_json("minor", "Typo", "→ none"),
            finding_json("major", "Plot hole", "→ brainstorm"),
        ];
        let summary = FindingsSummary::from_findings_json(&findings, false);
        assert_eq!(summary.open_count, 4);
        assert_eq!(summary.highest_severity.as_deref(), Some("blocker"));
        // Sorted by severity priority (blocker first).
        assert_eq!(summary.severity_counts[0].0, "blocker");
        assert_eq!(summary.severity_counts[0].1, 1);
        assert_eq!(summary.severity_counts[1].0, "major");
        assert_eq!(summary.severity_counts[1].1, 1);
    }

    #[test]
    fn findings_summary_top_five_cap() {
        let findings: Vec<serde_json::Value> = (0..8)
            .map(|i| finding_json("info", &format!("Finding {i}"), "→ none"))
            .collect();
        let summary = FindingsSummary::from_findings_json(&findings, false);
        assert_eq!(summary.open_count, 8);
        assert_eq!(summary.top_findings.len(), 5);
    }

    #[test]
    fn findings_summary_truncated_flag() {
        let findings: Vec<serde_json::Value> = (0..FINDINGS_FETCH_LIMIT)
            .map(|i| finding_json("info", &format!("Finding {i}"), "→ none"))
            .collect();
        let summary = FindingsSummary::from_findings_json(&findings, true);
        assert_eq!(summary.open_count, FINDINGS_FETCH_LIMIT);
        assert!(summary.is_truncated);
    }

    // ── print_findings_summary display tests ─────────────────────────────

    fn capture_findings_output(findings: &[serde_json::Value]) -> String {
        // R-V146P0-QC1-S2: delegate to the shared production formatter so the
        // test helper cannot drift from `print_findings_summary`. The slice is
        // wrapped into `FindingsResult::Fetched`; the `Unavailable` branch is
        // covered directly by `format_findings_summary_lines` unit tests.
        let result = FindingsResult::Fetched(findings.to_vec());
        format_findings_summary_lines(&result).join("\n")
    }

    #[test]
    fn display_no_open_findings() {
        let output = capture_findings_output(&[]);
        assert!(output.contains("findings: none open"));
        // v1.193 P2-T1: the removed `creator run` runner is not advertised.
        assert!(!output.contains("creator run"));
        assert!(!output.contains("highest"));
    }

    // -----------------------------------------------------------------------
    // R-V146P0-QC1-S2: shared formatter covers the Unavailable branch + parity
    // -----------------------------------------------------------------------

    #[test]
    fn format_findings_summary_lines_unavailable_branch() {
        // R-V146P0-QC1-S2: the shared `format_findings_summary_lines` owns the
        // Unavailable branch (previously only reachable via the production
        // `print_findings_summary`, never via the test helper). Pins the
        // one-line degradation output.
        let lines = format_findings_summary_lines(&FindingsResult::Unavailable);
        assert_eq!(lines.len(), 1, "Unavailable renders exactly one line");
        assert_eq!(lines[0], "findings: unavailable (daemon error)");
    }

    #[test]
    fn format_findings_summary_lines_parity_with_capture_helper() {
        // R-V146P0-QC1-S2: the test helper `capture_findings_output` now
        // delegates to the shared formatter, so its output must equal
        // `format_findings_summary_lines(...).join("\n")` byte-for-byte.
        // A representative populated case proves no formatting drifted.
        let findings = vec![
            finding_json("blocker", "Continuity error", "→ write"),
            finding_json("major", "Plot hole", "→ brainstorm"),
            finding_json("minor", "Typo", "→ none"),
        ];
        let via_helper = capture_findings_output(&findings);
        let result = FindingsResult::Fetched(findings);
        let via_shared = format_findings_summary_lines(&result).join("\n");
        assert_eq!(
            via_helper, via_shared,
            "helper must delegate to shared formatter"
        );
        // Sanity: the populated summary surfaces the count + a top finding.
        assert!(via_shared.contains("findings: 3 open"));
        assert!(via_shared.contains("1 blocker"));
        assert!(via_shared.contains("Continuity error"));
    }

    #[test]
    fn display_findings_with_severity_summary() {
        let findings = vec![
            finding_json("blocker", "Continuity error", "→ write"),
            finding_json("minor", "Style issue", "→ none"),
        ];
        let output = capture_findings_output(&findings);
        assert!(output.contains("findings: 2 open"));
        assert!(output.contains("1 blocker"));
        assert!(output.contains("1 minor"));
        assert!(output.contains("highest: blocker"));
        assert!(output.contains("#1 [blocker] \"Continuity error\" → write"));
        assert!(output.contains("#2 [minor] \"Style issue\" → none"));
        // V1.46 P0 (Grill #7): per-finding hint only; no blanket footer.
        assert!(
            !output.contains("novel-chapter-review"),
            "blanket novel-chapter-review footer removed"
        );
        assert!(
            !output.contains("quickstart"),
            "quickstart reference removed from findings summary"
        );
    }

    #[test]
    fn display_findings_completed_work_shows_summary() {
        // Verify that the findings summary format works for the completed
        // path too — same formatting, just inserted before the "complete" message.
        let findings = vec![finding_json("info", "Nice-to-have", "→ none")];
        let output = capture_findings_output(&findings);
        assert!(output.contains("findings: 1 open"));
        assert!(output.contains("1 info"));
        assert!(output.contains("highest: info"));
    }

    /// V1.47 P1 regression guard (AC4): V1.46 per-finding `routing_hint`
    /// behavior must be unchanged by the gate-remediation copy sweep.
    /// Verifies that each finding row surfaces its own `routing_hint` and
    /// that the summary does **not** inject a blanket footer pointing only
    /// at `novel-chapter-review` (Grill #7 design).
    #[test]
    fn v146_routing_hint_behavior_unchanged() {
        let findings = vec![
            finding_json("blocker", "Continuity error", "→ write"),
            finding_json("major", "Pacing drag", "→ outline"),
            finding_json("minor", "Typo", "→ copyedit"),
        ];
        let output = capture_findings_output(&findings);
        // Each per-finding hint appears verbatim in the output.
        assert!(
            output.contains("→ write"),
            "per-finding routing_hint '→ write' must appear: {output}"
        );
        assert!(
            output.contains("→ outline"),
            "per-finding routing_hint '→ outline' must appear: {output}"
        );
        assert!(
            output.contains("→ copyedit"),
            "per-finding routing_hint '→ copyedit' must appear: {output}"
        );
        // No blanket novel-chapter-review footer (Grill #7).
        assert!(
            !output.contains("novel-chapter-review"),
            "no blanket novel-chapter-review footer (Grill #7): {output}"
        );
    }

    // ── Completion display tests ─────────────────────────────────────────

    #[test]
    fn completion_shows_zero_open_findings() {
        // When no findings exist, the summary line should say "none open".
        let output = capture_findings_output(&[]);
        assert!(output.contains("findings: none open"));
        assert!(!output.contains("creator run"));
    }

    // ── Truncation tests ─────────────────────────────────────────────────

    #[test]
    fn truncate_with_ellipsis_short() {
        assert_eq!(truncate_with_ellipsis("hello", 10), "hello");
    }

    #[test]
    fn truncate_with_ellipsis_long() {
        assert_eq!(truncate_with_ellipsis("hello world", 5), "hello…");
    }

    // ── Truncated findings (50+) display test ────────────────────────────

    #[test]
    fn display_truncated_findings_shows_plus_indicator() {
        let findings: Vec<serde_json::Value> = (0..FINDINGS_FETCH_LIMIT)
            .map(|i| finding_json("info", &format!("Finding {i}"), "→ none"))
            .collect();
        let output = capture_findings_output(&findings);
        assert!(
            output.contains(&format!("findings: {FINDINGS_FETCH_LIMIT}+ open")),
            "expected '50+ open' indicator in output: {output}"
        );
        // Should NOT show bare "50 open" (without +).
        assert!(
            !output.contains(&format!("findings: {FINDINGS_FETCH_LIMIT} open")),
            "should not show exact count without '+' when truncated"
        );
    }

    // ── V1.46 P0 enrich_status_json tests (novel-only gate + JSON contract) ──

    fn novel_work_resp() -> serde_json::Value {
        serde_json::json!({
            "work_id": "wrk_novel_1",
            "title": "Test Novel",
            "work_profile": "novel",
            "status": "writing",
            "current_chapter": 3,
        })
    }

    fn generic_work_resp() -> serde_json::Value {
        serde_json::json!({
            "work_id": "wrk_generic_1",
            "title": "Generic Work",
            "work_profile": "generic",
            "status": "active",
        })
    }

    #[test]
    fn enrich_novel_with_findings_inserts_array() {
        let findings = vec![
            finding_json("major", "Plot hole", "→ write"),
            finding_json("minor", "Typo", "→ none"),
        ];
        let out = enrich_status_json(novel_work_resp(), Some(findings.as_slice()), None);
        let arr = out
            .get("findings")
            .and_then(|v| v.as_array())
            .expect("findings[] present for novel work");
        assert_eq!(arr.len(), 2);
        // Same element shape as list API (verbatim).
        assert_eq!(
            arr[0].get("severity").and_then(|v| v.as_str()),
            Some("major")
        );
        assert_eq!(
            arr[0].get("routing_hint").and_then(|v| v.as_str()),
            Some("→ write")
        );
        // Daemon work fields preserved.
        assert_eq!(
            out.get("title").and_then(|v| v.as_str()),
            Some("Test Novel")
        );
        assert_eq!(
            out.get("current_chapter")
                .and_then(serde_json::Value::as_i64),
            Some(3)
        );
    }

    #[test]
    fn enrich_novel_preserves_full_finding_element_shape_verbatim() {
        // R-V146P0-QC2-S1: the existing enrich tests used the minimal
        // `finding_json` helper and only spot-checked `severity` /
        // `routing_hint`. Assert full element-shape fidelity: a finding
        // carrying every list-API field must survive `enrich_status_json`
        // byte-for-byte (verbatim round-trip), proving no field is dropped,
        // renamed, or coerced. Guards against a future enrich impl that
        // re-serializes a subset of fields.
        let full = full_finding_json();
        let findings = vec![full.clone()];
        let out = enrich_status_json(novel_work_resp(), Some(findings.as_slice()), None);
        let arr = out
            .get("findings")
            .and_then(|v| v.as_array())
            .expect("findings[] present");
        assert_eq!(arr.len(), 1, "exactly the one full-shape finding");
        // Verbatim equality: the enriched element equals the input element.
        assert_eq!(
            arr[0], full,
            "enrich must preserve the full finding element verbatim (no field drop/rename/coerce)"
        );
        // Pin a handful of the previously-unasserted fields explicitly so a
        // regression message points at the right field.
        assert_eq!(
            arr[0].get("chapter").and_then(serde_json::Value::as_i64),
            Some(7)
        );
        assert_eq!(
            arr[0].get("description").and_then(|v| v.as_str()),
            Some("Character name changes between paragraphs 3 and 9.")
        );
        assert_eq!(
            arr[0].get("kind").and_then(|v| v.as_str()),
            Some("continuity")
        );
        assert_eq!(
            arr[0].get("rule_suggestion").and_then(|v| v.as_str()),
            Some("Track character names per chapter.")
        );
        assert_eq!(
            arr[0].get("created_at").and_then(serde_json::Value::as_i64),
            Some(1_718_000_000)
        );
    }

    #[test]
    fn enrich_novel_empty_findings_inserts_empty_array() {
        let out = enrich_status_json(novel_work_resp(), Some(&[]), None);
        let arr = out
            .get("findings")
            .and_then(|v| v.as_array())
            .expect("findings[] present (empty) for novel work");
        assert!(arr.is_empty());
    }

    #[test]
    fn enrich_novel_unavailable_findings_omits_key() {
        // When the findings endpoint is unreachable (None), omit findings[]
        // rather than fabricating an empty array (graceful degradation).
        let out = enrich_status_json(novel_work_resp(), None, None);
        assert!(
            out.get("findings").is_none(),
            "findings key omitted when unavailable"
        );
        // qc3 F-003: truncation marker must also be absent when findings
        // were not fetched at all.
        assert!(
            out.get("findings_truncated").is_none(),
            "findings_truncated omitted when findings unavailable"
        );
    }

    // ── qc3 F-003: findings_truncated marker tests ───────────────────────

    #[test]
    fn enrich_findings_truncated_marker_set_when_at_limit() {
        // When the daemon returns exactly FINDINGS_FETCH_LIMIT (50) rows,
        // there may be more open findings beyond the fetched page. Surface
        // a `findings_truncated: true` flag so JSON consumers can detect
        // the cap (qc3 F-003).
        let findings: Vec<serde_json::Value> = (0..FINDINGS_FETCH_LIMIT)
            .map(|i| finding_json("info", &format!("Finding {i}"), "→ none"))
            .collect();
        let out = enrich_status_json(novel_work_resp(), Some(findings.as_slice()), None);
        assert_eq!(
            out.get("findings_truncated")
                .and_then(serde_json::Value::as_bool),
            Some(true),
            "findings_truncated must be true when findings.len() == FINDINGS_FETCH_LIMIT"
        );
    }

    #[test]
    fn enrich_findings_truncated_omitted_when_below_limit() {
        // Below the cap, the marker is omitted (not false) so consumers can
        // distinguish "truncation known to be false" from "not applicable".
        let findings = vec![
            finding_json("major", "Plot hole", "→ write"),
            finding_json("minor", "Typo", "→ none"),
        ];
        let out = enrich_status_json(novel_work_resp(), Some(findings.as_slice()), None);
        assert!(
            out.get("findings_truncated").is_none(),
            "findings_truncated omitted when findings.len() < FINDINGS_FETCH_LIMIT"
        );
    }

    #[test]
    fn enrich_findings_truncated_omitted_when_empty() {
        // Empty findings (fetched successfully, none open) — not truncated.
        let out = enrich_status_json(novel_work_resp(), Some(&[]), None);
        assert!(
            out.get("findings_truncated").is_none(),
            "findings_truncated omitted when findings is empty"
        );
    }

    #[test]
    fn enrich_generic_work_omits_findings_gate() {
        // Novel-only gate (Grill #6): generic works never get findings.
        let findings = vec![finding_json("major", "Plot hole", "→ write")];
        let out = enrich_status_json(generic_work_resp(), Some(findings.as_slice()), None);
        assert!(
            out.get("findings").is_none(),
            "generic work must not include findings"
        );
        assert!(out.get("findings_stale").is_none());
    }

    #[test]
    fn enrich_missing_work_profile_omits_findings() {
        // A work with no work_profile field is treated as non-novel.
        let resp = serde_json::json!({ "work_id": "wrk_x", "title": "Mystery" });
        let findings = vec![finding_json("info", "x", "→ none")];
        let out = enrich_status_json(resp, Some(findings.as_slice()), None);
        assert!(out.get("findings").is_none());
    }

    #[test]
    fn enrich_novel_stale_inserts_findings_stale() {
        let stale = serde_json::json!({ "stale_count": 3, "threshold_seconds": 345_600 });
        let out = enrich_status_json(novel_work_resp(), None, Some(&stale));
        let stale_out = out
            .get("findings_stale")
            .expect("findings_stale present when stale_count > 0");
        assert_eq!(
            stale_out
                .get("stale_count")
                .and_then(serde_json::Value::as_u64),
            Some(3)
        );
    }

    #[test]
    fn enrich_novel_zero_stale_omits_findings_stale() {
        let stale = serde_json::json!({ "stale_count": 0, "threshold_seconds": 345_600 });
        let out = enrich_status_json(novel_work_resp(), None, Some(&stale));
        assert!(
            out.get("findings_stale").is_none(),
            "findings_stale omitted when stale_count is 0"
        );
    }

    #[test]
    fn enrich_preserves_daemon_work_fields() {
        // Daemon GET work payload fields must be unchanged (spec §4.1).
        let resp = serde_json::json!({
            "work_id": "wrk_full",
            "title": "Full",
            "work_profile": "novel",
            "status": "writing",
            "chapters": [{"chapter_number": 1, "status": "finalized"}],
        });
        let out = enrich_status_json(resp, Some(&[]), None);
        assert_eq!(
            out.get("work_id").and_then(|v| v.as_str()),
            Some("wrk_full")
        );
        assert_eq!(
            out.get("work_profile").and_then(|v| v.as_str()),
            Some("novel")
        );
        assert!(out.get("chapters").and_then(|v| v.as_array()).is_some());
    }

    // ── R-V146P0-QC3-S3: human-path stale banner rendering ──────────────

    #[test]
    fn format_stale_banner_none_when_zero() {
        // R-V146P0-QC3-S3: zero stale findings must NOT emit a banner
        // (no empty sentinel noise). The human status path now routes through
        // `format_stale_banner`, so the zero-suppression is unit-testable.
        assert_eq!(format_stale_banner(0, 96 * 60 * 60), None);
    }

    #[test]
    fn format_stale_banner_some_when_stale_count_positive() {
        // R-V146P0-QC3-S3: a positive stale count renders the banner with the
        // computed whole-hour threshold and the spec citation.
        let banner = format_stale_banner(7, 96 * 60 * 60).expect("banner for stale_count>0");
        assert!(
            banner.contains("7 finding(s) stale"),
            "count rendered: {banner}"
        );
        assert!(
            banner.contains(">96h"),
            "threshold hours rendered: {banner}"
        );
        assert!(
            banner.contains("novel-author-experience §4"),
            "spec citation preserved: {banner}"
        );
    }

    #[test]
    fn format_stale_banner_computes_hours_from_seconds() {
        // R-V146P0-QC3-S3: threshold_seconds is converted to whole hours
        // (integer division). 345600s = 96h; 7200s = 2h.
        assert!(format_stale_banner(1, 345_600).unwrap().contains(">96h"));
        assert!(format_stale_banner(1, 7_200).unwrap().contains(">2h"));
    }

    // ── sanitize_for_terminal tests ──────────────────────────────────────

    #[test]
    fn sanitize_for_terminal_strips_escape_codes() {
        let input = "\x1b[31mRed Text\x1b[0m normal";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "Red Text normal");
    }

    #[test]
    fn sanitize_for_terminal_preserves_unicode() {
        let input = "你好世界 🌍 こんにちは";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, input);
    }

    #[test]
    fn sanitize_for_terminal_strips_control_chars() {
        let input = "hello\x00world\x07bell\x1Fus";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "helloworldbellus");
    }

    #[test]
    fn sanitize_for_terminal_strips_del() {
        // DEL (0x7F) should be removed
        let input = "before\x7Fafter";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "beforeafter");
    }

    #[test]
    fn sanitize_for_terminal_preserves_newline_and_tab() {
        let input = "line1\nline2\ttab";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "line1\nline2\ttab");
    }

    #[test]
    fn sanitize_for_terminal_strips_clear_screen() {
        // \x1b[2J is "clear screen"
        let input = "good\x1b[2Jbad";
        let sanitized = sanitize_for_terminal(input);
        assert_eq!(sanitized, "goodbad");
    }

    // ── R-V146P2-QC1-S2: workspace-dir resolution dedup ─────────────────

    #[test]
    fn print_completion_lock_hint_no_ops_when_work_ref_is_placeholder() {
        // R-V146P2-QC1-S2: a placeholder work_ref (starting with "(") must
        // short-circuit before any workspace-dir resolution. Asserts the
        // early-return branch is preserved after routing through the shared
        // `operational_workspace_dir_from_config` helper.
        // Best-effort: cannot panic regardless of config state.
        print_completion_lock_hint("(no ref)", "wrk_test");
    }

    #[test]
    fn print_completion_lock_hint_no_ops_when_workspace_unresolvable() {
        // R-V146P2-QC1-S2: with a real-looking work_ref but no resolvable
        // active creator/workspace (the default test-env state), the shared
        // helper returns None and the hint is skipped without panicking.
        // Pins that the deduplicated resolution path degrades gracefully.
        print_completion_lock_hint("MYNOVEL", "wrk_test");
    }

    // ── FindingsResult unavailable display test ──────────────────────────

    #[test]
    fn display_unavailable_findings() {
        // When findings are unavailable, the output should say "unavailable"
        // not "none open". This tests the logic that print_findings_summary uses.
        let result = FindingsResult::Unavailable;
        let output = match result {
            FindingsResult::Unavailable => "findings: unavailable (daemon error)".to_string(),
            FindingsResult::Fetched(vec) => {
                let is_truncated = vec.len() == FINDINGS_FETCH_LIMIT;
                let summary = FindingsSummary::from_findings_json(&vec, is_truncated);
                if summary.open_count == 0 {
                    "findings: none open".to_string()
                } else {
                    format!("findings: {} open", summary.open_count)
                }
            }
        };
        assert!(output.contains("unavailable"));
        assert!(!output.contains("none open"));
    }

    // ── V1.45 P2: CLI parsing tests for migrated works subcommands ──────

    use clap::Parser;

    /// Minimal CLI struct for hermetic parsing tests of `creator works`.
    #[derive(Parser)]
    struct WorksCli {
        #[command(subcommand)]
        command: WorksCommand,
    }

    #[test]
    fn works_inspire_parses_with_note() {
        let cli = WorksCli::try_parse_from(["nexus42", "inspire", "--note", "New plot twist idea"])
            .expect("works inspire --note should parse");
        match cli.command {
            WorksCommand::Inspire {
                work_id,
                note,
                json: _,
            } => {
                assert!(work_id.is_none(), "work_id should be optional");
                assert_eq!(note, "New plot twist idea");
            }
            _ => panic!("expected Inspire variant"),
        }
    }

    #[test]
    fn works_inspire_parses_with_work_id_and_note() {
        let cli = WorksCli::try_parse_from([
            "nexus42",
            "inspire",
            "wrk_abc123",
            "--note",
            "Character motivation",
        ])
        .expect("works inspire <work_id> --note should parse");
        match cli.command {
            WorksCommand::Inspire {
                work_id,
                note,
                json: _,
            } => {
                assert_eq!(work_id.as_deref(), Some("wrk_abc123"));
                assert_eq!(note, "Character motivation");
            }
            _ => panic!("expected Inspire variant"),
        }
    }

    #[test]
    fn works_inspire_requires_note() {
        let result = WorksCli::try_parse_from(["nexus42", "inspire", "wrk_123"]);
        assert!(result.is_err(), "works inspire without --note should fail");
    }

    #[test]
    fn works_reopen_parses_with_reason() {
        let cli = WorksCli::try_parse_from([
            "nexus42",
            "reopen",
            "wrk_test",
            "--reason",
            "User requested more chapters",
        ])
        .expect("works reopen <work_id> --reason should parse");
        match cli.command {
            WorksCommand::Reopen {
                work_id,
                reason,
                json: _,
            } => {
                assert_eq!(work_id.as_deref(), Some("wrk_test"));
                assert_eq!(reason, "User requested more chapters");
            }
            _ => panic!("expected Reopen variant"),
        }
    }

    #[test]
    fn works_reopen_rejects_removed_extend_chapters_flag() {
        // The flag rode the retired daemon patch body; no retained request
        // carries `total_planned_chapters`, so the parser must refuse it
        // instead of silently dropping it.
        let result = WorksCli::try_parse_from([
            "nexus42",
            "reopen",
            "--reason",
            "Extend story",
            "--extend-chapters",
            "30",
        ]);
        assert!(
            result.is_err(),
            "works reopen --extend-chapters must no longer parse"
        );
    }

    #[test]
    fn works_reopen_requires_reason() {
        let result = WorksCli::try_parse_from(["nexus42", "reopen", "wrk_test"]);
        assert!(result.is_err(), "works reopen without --reason should fail");
    }

    #[test]
    fn works_reconcile_chapters_parses() {
        let cli = WorksCli::try_parse_from(["nexus42", "reconcile-chapters"])
            .expect("works reconcile-chapters should parse");
        match cli.command {
            WorksCommand::ReconcileChapters {
                work_id,
                dry_run,
                yes,
                json: _,
            } => {
                assert!(work_id.is_none(), "work_id should be optional");
                assert!(!dry_run, "dry_run should default to false");
                assert!(!yes, "yes should default to false");
            }
            _ => panic!("expected ReconcileChapters variant"),
        }
    }

    #[test]
    fn works_reconcile_chapters_parses_with_work_id() {
        let cli = WorksCli::try_parse_from(["nexus42", "reconcile-chapters", "wrk_abc"])
            .expect("works reconcile-chapters <work_id> should parse");
        match cli.command {
            WorksCommand::ReconcileChapters {
                work_id,
                dry_run: _,
                yes: _,
                json: _,
            } => {
                assert_eq!(work_id.as_deref(), Some("wrk_abc"));
            }
            _ => panic!("expected ReconcileChapters variant"),
        }
    }

    /// V1.49 P2 (R-V148P4-W2): `--dry-run` and `--yes` flags parse correctly.
    #[test]
    fn works_reconcile_chapters_parses_dry_run_and_yes_flags() {
        let cli =
            WorksCli::try_parse_from(["nexus42", "reconcile-chapters", "wrk_abc", "--dry-run"])
                .expect("works reconcile-chapters --dry-run should parse");
        match cli.command {
            WorksCommand::ReconcileChapters {
                work_id,
                dry_run,
                yes: _,
                json: _,
            } => {
                assert_eq!(work_id.as_deref(), Some("wrk_abc"));
                assert!(dry_run, "dry_run flag must be true");
            }
            _ => panic!("expected ReconcileChapters variant"),
        }

        let cli = WorksCli::try_parse_from(["nexus42", "reconcile-chapters", "-y"])
            .expect("works reconcile-chapters -y (short form) should parse");
        match cli.command {
            WorksCommand::ReconcileChapters {
                dry_run: _, yes, ..
            } => {
                assert!(yes, "yes flag must be true via the -y short form");
            }
            _ => panic!("expected ReconcileChapters variant"),
        }
    }

    // ── V1.46 P2 (Grill #9): on-disk chapter path hint tests ──────────────

    fn chapter_with_paths(body: Option<&str>, outline: Option<&str>) -> serde_json::Value {
        let mut obj = serde_json::Map::new();
        obj.insert(
            "chapter_number".to_string(),
            serde_json::Value::Number(1.into()),
        );
        obj.insert(
            "title".to_string(),
            serde_json::Value::String("Intro".into()),
        );
        obj.insert(
            "status".to_string(),
            serde_json::Value::String("writing".into()),
        );
        if let Some(b) = body {
            obj.insert("body_path".to_string(), serde_json::Value::String(b.into()));
        }
        if let Some(o) = outline {
            obj.insert(
                "outline_path".to_string(),
                serde_json::Value::String(o.into()),
            );
        }
        serde_json::Value::Object(obj)
    }

    #[test]
    fn chapter_path_missing_hint_body_missing_on_disk() {
        // body_path configured but file does not exist → hint should fire
        // and mention body_path.
        let dir = tempfile::tempdir().expect("tempdir");
        let ch = chapter_with_paths(Some("Works/my-novel/Stories/ch01-intro.md"), None);
        let hint = chapter_path_missing_hint(&ch, dir.path());
        let hint = hint.expect("hint present when body_path missing on disk");
        assert!(
            hint.contains("body_path"),
            "hint should mention body_path: {hint}"
        );
        assert!(hint.contains("missing on disk"));
    }

    #[test]
    fn chapter_path_missing_hint_outline_missing_on_disk() {
        // outline_path configured but file does not exist → hint fires.
        let dir = tempfile::tempdir().expect("tempdir");
        let ch = chapter_with_paths(
            None,
            Some("Works/my-novel/Outlines/chapters/ch01-outline.md"),
        );
        let hint = chapter_path_missing_hint(&ch, dir.path());
        let hint = hint.expect("hint present when outline_path missing on disk");
        assert!(
            hint.contains("outline_path"),
            "hint should mention outline_path: {hint}"
        );
    }

    #[test]
    fn chapter_path_missing_hint_both_present_no_hint() {
        // Both paths configured AND present on disk → no hint (None).
        let dir = tempfile::tempdir().expect("tempdir");
        // Create the files so exists() returns true.
        let body_rel = "Works/my-novel/Stories/ch01.md";
        let outline_rel = "Works/my-novel/Outlines/chapters/ch01-outline.md";
        std::fs::create_dir_all(dir.path().join("Works/my-novel/Stories"))
            .expect("mkdir body parent");
        std::fs::create_dir_all(dir.path().join("Works/my-novel/Outlines/chapters"))
            .expect("mkdir outline parent");
        std::fs::write(dir.path().join(body_rel), "body").expect("write body");
        std::fs::write(dir.path().join(outline_rel), "outline").expect("write outline");

        let ch = chapter_with_paths(Some(body_rel), Some(outline_rel));
        let hint = chapter_path_missing_hint(&ch, dir.path());
        assert!(
            hint.is_none(),
            "no hint when both configured paths exist on disk (got {hint:?})"
        );
    }

    #[test]
    fn chapter_path_missing_hint_no_paths_configured_no_hint() {
        // Neither body_path nor outline_path in the JSON → None (nothing to
        // check; daemon has not assigned file paths yet).
        let dir = tempfile::tempdir().expect("tempdir");
        let ch = chapter_with_paths(None, None);
        let hint = chapter_path_missing_hint(&ch, dir.path());
        assert!(
            hint.is_none(),
            "no hint when neither path is configured (got {hint:?})"
        );
    }

    #[test]
    fn chapter_path_missing_hint_both_missing_mentions_both() {
        // Both configured, neither exists → hint mentions both fields.
        let dir = tempfile::tempdir().expect("tempdir");
        let ch = chapter_with_paths(
            Some("Works/x/Stories/ch01.md"),
            Some("Works/x/Outlines/chapters/ch01-outline.md"),
        );
        let hint =
            chapter_path_missing_hint(&ch, dir.path()).expect("hint present when both missing");
        assert!(
            hint.contains("body_path"),
            "hint should mention body_path: {hint}"
        );
        assert!(
            hint.contains("outline_path"),
            "hint should mention outline_path: {hint}"
        );
    }

    #[test]
    fn chapter_path_missing_hint_exists_failure_is_silent() {
        // Best-effort contract: `Path::exists()` returns false (rather than
        // panicking) for unreadable / permission-denied paths. The hint
        // surfaces "missing on disk" for such cases too — reconcile is the
        // correct remediation regardless. This test pins the "swallow"
        // behavior: a path pointing into a tempdir that was just removed
        // still yields Some (treated as missing), never panics.
        let dir = tempfile::tempdir().expect("tempdir");
        let dir_path = dir.path().to_path_buf();
        let ch = chapter_with_paths(Some("nonexistent/ch01.md"), None);
        // Drop the tempdir handle but keep the path; the files never existed.
        drop(dir);
        let hint = chapter_path_missing_hint(&ch, &dir_path);
        // After drop the tempdir may still exist on disk (cleanup is
        // best-effort), but the inner file definitely does not → Some.
        assert!(
            hint.is_some(),
            "missing file surfaces as Some (best-effort, never panics)"
        );
    }

    // ── V1.46 P2 QC fix W-001: chapter hint cap + summary tests ──────────

    #[test]
    fn chapter_path_hint_skipped_summary_format() {
        // Format contract for the "+ N more (paths not checked)" line.
        assert_eq!(
            chapter_path_hint_skipped_summary(1).as_deref(),
            Some("+ 1 more (paths not checked)"),
        );
        assert_eq!(
            chapter_path_hint_skipped_summary(10).as_deref(),
            Some("+ 10 more (paths not checked)"),
        );
        // Zero skipped → no summary line (caller must not render one).
        assert!(
            chapter_path_hint_skipped_summary(0).is_none(),
            "no summary when skipped == 0"
        );
    }

    #[test]
    fn chapter_path_hint_cap_only_first_50_chapters_get_hints() {
        // 60-chapter work, all with missing body_path. Without the cap,
        // every chapter would emit a ⚠ hint (and incur a synchronous
        // `exists()` syscall). The cap bounds the inspected set at
        // `CHAPTER_PATH_HINT_CAP` and summarizes the remainder.
        let dir = tempfile::tempdir().expect("tempdir");
        let chapters: Vec<serde_json::Value> = (1..=60)
            .map(|i| chapter_with_paths(Some(&format!("Works/x/Stories/ch{i:02}.md")), None))
            .collect();
        assert_eq!(chapters.len(), 60);

        // Mirror print_chapter_table's cap math.
        let hint_cap = chapters.len().min(CHAPTER_PATH_HINT_CAP);
        assert_eq!(
            hint_cap, CHAPTER_PATH_HINT_CAP,
            "60-chapter work hits the cap"
        );

        // Only the first `hint_cap` chapters are inspected for path hints.
        let capped_hint_count = chapters[..hint_cap]
            .iter()
            .filter_map(|ch| chapter_path_missing_hint(ch, dir.path()))
            .count();
        assert_eq!(
            capped_hint_count, 50,
            "all 50 capped chapters have missing body_path → 50 hints"
        );

        // Prove the cap is actually bounding something real: the skipped
        // chapters WOULD have generated hints if the cap weren't in place.
        let would_have_hinted = chapters[hint_cap..]
            .iter()
            .filter_map(|ch| chapter_path_missing_hint(ch, dir.path()))
            .count();
        assert_eq!(
            would_have_hinted, 10,
            "skipped chapters would have hinted without the cap"
        );

        // Summary line covers exactly the skipped count.
        let skipped = chapters.len().saturating_sub(hint_cap);
        assert_eq!(skipped, 10);
        let summary =
            chapter_path_hint_skipped_summary(skipped).expect("summary present when skipped > 0");
        assert_eq!(summary, "+ 10 more (paths not checked)");
    }

    #[test]
    fn chapter_path_hint_cap_not_triggered_under_50() {
        // A 10-chapter work is well under the cap: no chapter is skipped,
        // no summary line is rendered, and the existing per-chapter
        // behavior is fully preserved.
        let chapters: Vec<serde_json::Value> = (1..=10)
            .map(|i| chapter_with_paths(Some(&format!("Works/x/Stories/ch{i:02}.md")), None))
            .collect();
        let hint_cap = chapters.len().min(CHAPTER_PATH_HINT_CAP);
        assert_eq!(hint_cap, 10, "10-chapter work does not hit the cap");

        let skipped = chapters.len().saturating_sub(hint_cap);
        assert_eq!(skipped, 0);
        assert!(
            chapter_path_hint_skipped_summary(skipped).is_none(),
            "no summary when skipped == 0"
        );
    }

    // ── V1.48 P2: CLI parsing for findings + rules subcommands ────────

    #[test]
    fn works_findings_accept_parses_with_finding_id() {
        let cli = WorksCli::try_parse_from(["nexus42", "findings", "accept", "fnd_01HMV8KX"])
            .expect("works findings accept <finding_id> should parse");
        match cli.command {
            WorksCommand::Findings {
                command:
                    FindingsCommand::Accept {
                        finding_id,
                        json: _,
                    },
            } => {
                assert_eq!(finding_id, "fnd_01HMV8KX");
            }
            _ => panic!("expected Findings::Accept variant"),
        }
    }

    #[test]
    fn works_findings_accept_supports_json_flag() {
        let cli =
            WorksCli::try_parse_from(["nexus42", "findings", "accept", "fnd_01HMV8KX", "--json"])
                .expect("works findings accept <finding_id> --json should parse");
        match cli.command {
            WorksCommand::Findings {
                command: FindingsCommand::Accept { finding_id, json },
            } => {
                assert_eq!(finding_id, "fnd_01HMV8KX");
                assert!(json, "--json should set json=true");
            }
            _ => panic!("expected Findings::Accept variant"),
        }
    }

    // ── V1.48 P2 T4: rules reset CLI parsing ──────────────────────────

    #[test]
    fn works_rules_reset_parses_without_work_id() {
        let cli = WorksCli::try_parse_from(["nexus42", "rules", "reset"])
            .expect("works rules reset (no work_id) should parse");
        match cli.command {
            WorksCommand::Rules {
                command:
                    RulesCommand::Reset {
                        work_id,
                        dry_run,
                        yes,
                        json: _,
                    },
            } => {
                assert!(work_id.is_none(), "work_id should default to None");
                assert!(!dry_run, "dry_run should default to false");
                assert!(!yes, "yes should default to false");
            }
            _ => panic!("expected Rules::Reset variant"),
        }
    }

    #[test]
    fn works_rules_reset_parses_with_work_id_and_json() {
        let cli = WorksCli::try_parse_from(["nexus42", "rules", "reset", "wrk_abc", "--json"])
            .expect("works rules reset <work_id> --json should parse");
        match cli.command {
            WorksCommand::Rules {
                command:
                    RulesCommand::Reset {
                        work_id,
                        dry_run,
                        yes,
                        json,
                    },
            } => {
                assert_eq!(work_id.as_deref(), Some("wrk_abc"));
                assert!(!dry_run, "dry_run should default to false");
                assert!(!yes, "yes should default to false");
                assert!(json, "--json should set json=true");
            }
            _ => panic!("expected Rules::Reset variant"),
        }
    }

    // ── V1.48 P2-fix1: --dry-run / --yes flag parsing ─────────────────

    #[test]
    fn works_rules_reset_supports_dry_run_flag() {
        let cli = WorksCli::try_parse_from(["nexus42", "rules", "reset", "--dry-run"])
            .expect("works rules reset --dry-run should parse");
        match cli.command {
            WorksCommand::Rules {
                command: RulesCommand::Reset { dry_run, .. },
            } => {
                assert!(dry_run, "--dry-run should set dry_run=true");
            }
            _ => panic!("expected Rules::Reset variant"),
        }
    }

    #[test]
    fn works_rules_reset_supports_yes_long_and_short_flags() {
        let long = WorksCli::try_parse_from(["nexus42", "rules", "reset", "--yes"])
            .expect("works rules reset --yes should parse");
        match long.command {
            WorksCommand::Rules {
                command: RulesCommand::Reset { yes, .. },
            } => assert!(yes, "--yes should set yes=true"),
            _ => panic!("expected Rules::Reset variant"),
        }

        let short = WorksCli::try_parse_from(["nexus42", "rules", "reset", "-y"])
            .expect("works rules reset -y should parse");
        match short.command {
            WorksCommand::Rules {
                command: RulesCommand::Reset { yes, .. },
            } => assert!(yes, "-y should set yes=true"),
            _ => panic!("expected Rules::Reset variant"),
        }
    }

    #[test]
    fn works_rules_reset_combines_dry_run_yes_and_json() {
        let cli = WorksCli::try_parse_from([
            "nexus42",
            "rules",
            "reset",
            "wrk_xyz",
            "--dry-run",
            "--yes",
            "--json",
        ])
        .expect("works rules reset <work_id> --dry-run --yes --json should parse");
        match cli.command {
            WorksCommand::Rules {
                command:
                    RulesCommand::Reset {
                        work_id,
                        dry_run,
                        yes,
                        json,
                    },
            } => {
                assert_eq!(work_id.as_deref(), Some("wrk_xyz"));
                assert!(dry_run && yes && json, "all three flags should be true");
            }
            _ => panic!("expected Rules::Reset variant"),
        }
    }
}
