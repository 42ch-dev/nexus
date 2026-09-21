//! `creator world` subcommand — create worlds, add events, list/show worlds,
//! manage World KB knowledge entries, and author structured check rules.
//!
//! Lifecycle (`create` / `list` / `show` / `findings list`), the World-rule
//! family and the fork family run on the direct `nexus-core` seam
//! ([`crate::core`]): one typed `CoreService` call per leaf — no daemon HTTP,
//! no `DaemonClient`, no second SQL owner. The core keeps the World
//! ownership guards, so a foreign World is refused on the direct path exactly
//! as it was over the removed route.
//!
//! `creator world event-add` keeps its existing guarded local narrative
//! writer (`nexus_local_db::narrative_write`): the World event-append
//! operation has no `CoreService` method, and inventing one is out of
//! contract.
//!
//! World KB author surface (`creator world kb list/show/edit/delete`) lives in
//! the [`kb`] submodule (V1.50 T-B P0).
//!
//! Structured-rule author surface (`creator world rule add|list|deactivate`)
//! lives in the [`rule`] submodule (V1.166 PD-1 / AR-2 / AR-3, DR-64).
//!
//! v1.193 P2-T13: the `basic-cli`/`legacy-cli` router is gone — this module
//! is the single retained `creator world` implementation.

pub mod fork;
pub mod kb;
pub mod rule;

use crate::config::CliConfig;
use crate::core::{
    finish_direct, map_core_error, open_direct_core, require_materialized_workspace,
};
use crate::errors::Result;
use clap::Subcommand;
use nexus_contracts::{CreateWorldRequest, CreateWorldRequestTitle};

/// World subcommands.
#[derive(Debug, Subcommand)]
pub enum WorldCommand {
    /// Create a new narrative world
    ///
    /// The title is the whole input: the core World create seam validates it
    /// (1-200 characters after trim), derives the slug from it and creates the
    /// World `private` / `manual` — the lifecycle the removed daemon route
    /// produced. The old `--name` compatibility spelling and the
    /// `--slug`/`--visibility`/`--time-policy`/`--description` extras are
    /// gone: the typed seam carries none of them, and accepting a flag no
    /// owner honours would be a silent no-op.
    Create {
        /// World title
        #[arg(long)]
        title: String,
    },

    /// Add a timeline event to a world
    #[command(name = "event-add")]
    EventAdd {
        /// World ID (required, e.g. `wld_abc123`)
        #[arg(long)]
        world_id: String,
        /// Branch ID (defaults to world's root branch)
        #[arg(long)]
        branch_id: Option<String>,
        /// Event type (default: `story_advance`)
        #[arg(long, default_value = "story_advance")]
        event_type: String,
        /// Event title
        #[arg(long)]
        title: Option<String>,
        /// Event summary
        #[arg(long)]
        summary: Option<String>,
        /// Observer entry ID recorded in `modules.observation` (repeatable;
        /// e.g. `kb_ana`). Any observer starts the observation module.
        #[arg(long)]
        observer: Vec<String>,
        /// Observation access as a JSON object string (optional; e.g.
        /// `{"line_of_sight":true}`). Providing `--access` without any
        /// `--observer` records `observers: []` (explicit nobody).
        #[arg(long)]
        access: Option<String>,
    },

    /// List all worlds in the active workspace
    List,

    /// Show details for a single world
    Show {
        /// World ID (e.g. `wld_abc123`)
        world_id: String,
    },

    /// World KB key-block author surface (list/show/edit/delete).
    ///
    /// Per entity-scope-model §5.5, `creator world kb` is the canonical author
    /// CLI for inspecting and editing World-scoped `KnowledgeEntryRecord` rows. This is a
    /// separate surface from `creator kb --scope world` (the legacy ingest path).
    Kb {
        #[command(subcommand)]
        command: kb::WorldKbCommand,
    },
    /// Structured-rule author surface (add/list/deactivate) — V1.166 PD-1.
    ///
    /// The CLI is the only write path for `spoke_rules` rows and the
    /// CLI-only validation gate for the AR-2 constraint carrier.
    Rule {
        #[command(subcommand)]
        command: rule::RuleCommand,
    },

    /// Timeline fork surface — `create` (one `CoreService::create_fork` call)
    /// + `list` (projection of the core timeline-events read) — V1.175 P1
    /// group 5.
    Fork {
        #[command(subcommand)]
        command: fork::ForkCommand,
    },

    /// World-attached check findings read surface (V1.175 P1 group 8, AR-87).
    ///
    /// **Read-only by design** (V1.165): world findings are advisory and
    /// `list_world_findings` is a core read, so this surface has no write
    /// owner at all. Triage writes ride the work-findings PATCH
    /// (`creator works findings set-status`).
    Findings {
        #[command(subcommand)]
        command: FindingsCommand,
    },
}

/// `creator world findings` subcommands (V1.175 P1 group 8, AR-87).
///
/// Read surface only — world findings are advisory and read through the core
/// (V1.165): there is no world-findings write owner at all; triage writes ride
/// the work-findings PATCH (`creator works findings set-status`).
#[derive(Debug, Subcommand)]
pub enum FindingsCommand {
    /// List world-attached check findings
    /// (`CoreService::list_world_findings`, V1.165 read surface — read-only
    /// by design, AR-87 #1).
    List {
        /// World ID (wld_...).
        #[arg(long, value_name = "WORLD_ID")]
        world_id: String,
        /// Emit machine-readable JSON (the `WorldFindingsListResponse`
        /// DTO verbatim) instead of human text.
        #[arg(long, default_value_t = false)]
        json: bool,
    },
}

/// Run a world subcommand.
///
/// # Errors
///
/// Returns `CliError` if the database is unavailable, the active creator is
/// not set, or any write/query operation fails.
pub async fn run(cmd: WorldCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        WorldCommand::Create { title } => run_create(config, &title).await,
        WorldCommand::EventAdd {
            world_id,
            branch_id,
            event_type,
            title,
            summary,
            observer,
            access,
        } => {
            run_event_add(
                config,
                &world_id,
                branch_id.as_deref(),
                &event_type,
                title.as_deref(),
                summary.as_deref(),
                &observer,
                access.as_deref(),
            )
            .await
        }
        WorldCommand::List => run_list(config).await,
        WorldCommand::Show { world_id } => run_show(config, &world_id).await,
        WorldCommand::Fork { command } => fork::run(command, config).await,
        WorldCommand::Kb { command } => kb::run(command, config).await,
        WorldCommand::Rule { command } => rule::run(command, config).await,
        WorldCommand::Findings { command } => match command {
            FindingsCommand::List { world_id, json } => {
                world_findings_list(&world_id, json, config).await
            }
        },
    }
}

/// Open a DB pool for the active workspace (initializing the schema).
///
/// The seam's admission pre-flight runs before the pool open: `Schema::init`
/// migrates — and therefore creates — the selected workspace `state.db`, so a
/// selection that names no materialized workspace must be refused instead of
/// having one created for it. Every caller of this pool therefore owes (and now
/// inherits) that refusal, including the retained local `world event-add`
/// narrative writer and the `creator world kb` local leaves.
///
/// # Errors
/// Returns [`CliError`] when the state DB path cannot be resolved from the
/// config, the selection names no materialized workspace, or schema
/// initialization fails.
pub async fn open_workspace_pool(config: &CliConfig) -> Result<sqlx::SqlitePool> {
    require_materialized_workspace(config)?;
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(pool)
}

/// Get the active creator ID or error.
///
/// # Errors
/// Returns [`CliError::CreatorNotSelected`] when no creator is active in the
/// config.
pub fn active_creator_id(config: &CliConfig) -> Result<String> {
    config
        .active_creator_id
        .clone()
        .ok_or(crate::errors::CliError::CreatorNotSelected)
}

/// Run `creator world create` through the typed core World seam.
///
/// The core validates the title, derives the slug and owns the write; this
/// leaf only renders the created World. The direct writer is closed on both
/// paths by [`finish_direct`].
///
/// # Errors
///
/// Returns [`CliError`] when the direct-writer core cannot be opened, the
/// active creator/workspace is unset, the title fails the core's 1-200
/// character validation, or the close cannot be settled.
async fn run_create(config: &CliConfig, title: &str) -> Result<()> {
    let request = CreateWorldRequest {
        title: CreateWorldRequestTitle::try_from(title).map_err(|e| {
            crate::errors::CliError::Other(format!("invalid input (title): {e}"))
        })?,
    };

    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.create_world(&principal, request)
            .await
            .map_err(map_core_error)
    }
    .await;
    let created = finish_direct(&core, outcome).await?;

    println!("✓ World created: {}", created.world_id);
    println!("  Status:    {}", created.status);
    Ok(())
}

/// Run `creator world event-add`.
#[allow(clippy::too_many_arguments)]
// ^ justification: mirrors run_event_add's flat event-add field surface;
// grouping the AR-5 observation flags into a struct would add indirection
// for the two new callers (CLI + tests).
async fn run_event_add(
    config: &CliConfig,
    world_id: &str,
    branch_id: Option<&str>,
    event_type: &str,
    title: Option<&str>,
    summary: Option<&str>,
    observers: &[String],
    access: Option<&str>,
) -> Result<()> {
    let pool = open_workspace_pool(config).await?;

    // Build `modules.observation` from the AR-5 flags (None = unrecorded).
    let modules_json = build_observation_modules(observers, access)?;

    // If no branch_id specified, look up the world's root_fork_branch_id
    let branch_id_resolved = if let Some(bid) = branch_id {
        bid.to_string()
    } else {
        // SAFETY: SELECT against known narrative_worlds table schema
        sqlx::query_scalar(
            "SELECT root_fork_branch_id FROM narrative_worlds WHERE world_id = ?",
        )
        .bind(world_id)
        .fetch_optional(&pool)
        .await
        .map_err(|e| crate::errors::CliError::Other(format!("Failed to query world: {e}")))?
        .flatten()
        .ok_or_else(|| {
            crate::errors::CliError::Other(format!(
                "World '{world_id}' not found or has no root branch. Use --world-id with a valid world ID."
            ))
        })?
    };

    let result = nexus_local_db::append_event(
        &pool,
        world_id,
        &branch_id_resolved,
        event_type,
        title,
        summary,
        modules_json.as_deref(),
    )
    .await
    .map_err(|e| crate::errors::CliError::Other(format!("Failed to append event: {e}")))?;

    println!("✓ Event added: {}", result.event_id);
    println!("  World:     {world_id}");
    println!("  Branch:    {branch_id_resolved}");
    println!("  Sequence:  {}", result.sequence_no);
    if let Some(t) = title {
        println!("  Title:     {t}");
    }
    if let Some(s) = summary {
        println!("  Summary:   {s}");
    }
    Ok(())
}

/// Build the `modules.observation` JSON payload from `event-add` observation
/// flags (AR-5 lock — tri-state).
///
/// - No observation flags → `None`: the appended event stores
///   `modules_json = NULL` (absent / unrecorded).
/// - ≥1 `--observer` → `{"observation": {"observers": [...], "access": {...}?}}`.
/// - Zero `--observer` **with** `--access` → `observers: []` + access
///   (explicit nobody — the PD-9 empty state is authorable).
///
/// Malformed input is rejected here, at the CLI boundary (not at the DB):
/// `--access` must parse as a JSON **object**, and observer values must be
/// non-empty strings.
fn build_observation_modules(observers: &[String], access: Option<&str>) -> Result<Option<String>> {
    if observers.is_empty() && access.is_none() {
        return Ok(None);
    }

    for obs in observers {
        if obs.trim().is_empty() {
            return Err(crate::errors::CliError::Other(
                "--observer values must be non-empty strings".to_string(),
            ));
        }
    }

    let mut observation = serde_json::Map::new();
    observation.insert("observers".to_string(), serde_json::json!(observers));

    if let Some(access_str) = access {
        let access_value: serde_json::Value = serde_json::from_str(access_str).map_err(|e| {
            crate::errors::CliError::Other(format!(
                "--access must be a JSON object string (parse error: {e})"
            ))
        })?;
        if !access_value.is_object() {
            return Err(crate::errors::CliError::Other(
                "--access must be a JSON object string (got a non-object value)".to_string(),
            ));
        }
        observation.insert("access".to_string(), access_value);
    }

    let modules = serde_json::json!({ "observation": observation });
    serde_json::to_string(&modules).map(Some).map_err(|e| {
        crate::errors::CliError::Other(format!("failed to serialize observation modules: {e}"))
    })
}

/// Run `creator world list`.
///
/// # Errors
///
/// Returns [`CliError`] when the direct-writer core cannot be opened, the
/// active creator/workspace is unset, the core refuses the read, or the close
/// cannot be settled.
async fn run_list(config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.list_worlds(&principal).await.map_err(map_core_error)
    }
    .await;
    let worlds = finish_direct(&core, outcome).await?;

    if worlds.is_empty() {
        println!("No worlds found in the active workspace.");
        return Ok(());
    }

    println!(
        "{:<40} {:<25} {:<12} CREATED_AT",
        "WORLD_ID", "TITLE", "STATUS"
    );
    for world in &worlds {
        println!(
            "{:<40} {:<25} {:<12} {}",
            world.world_id, world.title, world.status, world.created_at
        );
    }
    Ok(())
}

/// Run `creator world show`.
///
/// The core read returns the same `WorldState` projection the list arm
/// renders; an unknown id is the core's `not_found` (404) family.
///
/// # Errors
///
/// Returns [`CliError`] when the direct-writer core cannot be opened, the
/// active creator/workspace is unset, the World is unknown, or the close
/// cannot be settled.
async fn run_show(config: &CliConfig, world_id: &str) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.get_world(&principal, world_id.to_string())
            .await
            .map_err(map_core_error)
    }
    .await;
    let world = finish_direct(&core, outcome).await?;

    println!("WORLD_ID:     {}", world.world_id);
    println!("Title:        {}", world.title);
    println!("Slug:         {}", world.slug);
    println!("Status:       {}", world.status);
    if let Some(rev) = world.canon_revision {
        println!("Canon rev:    {rev}");
    }
    if let Some(head) = &world.current_timeline_head_id {
        println!("Timeline head: {head}");
    }
    if let Some(tp) = &world.current_time_pointer {
        println!("Time pointer: {tp}");
    }
    println!("Created:      {}", world.created_at);
    Ok(())
}

/// `creator world findings list --world-id <id> [--json]` — list
/// world-attached check findings through the typed core read (V1.165 read
/// surface — GET-only by design, AR-87 #1).
///
/// # Errors
///
/// Returns [`CliError`] when the direct-writer core cannot be opened, the
/// active creator/workspace is unset, the World is unknown (404), the caller
/// does not own it (403), or the close cannot be settled.
async fn world_findings_list(world_id: &str, json: bool, config: &CliConfig) -> Result<()> {
    let core = open_direct_core(config).await?;
    let outcome = async {
        let principal = core.active_principal().await.map_err(map_core_error)?;
        core.list_world_findings(&principal, world_id.to_string())
            .await
            .map_err(map_core_error)
    }
    .await;
    let resp = finish_direct(&core, outcome).await?;

    if json {
        println!("{}", serde_json::to_string_pretty(&resp)?);
        return Ok(());
    }
    if resp.findings.is_empty() {
        println!("No world findings for world '{world_id}'.");
        return Ok(());
    }
    println!("World findings for world '{world_id}':\n");
    println!(
        "{:<36} {:<10} {:<10} TITLE",
        "FINDING_ID", "STATUS", "SEVERITY"
    );
    println!("{}", "-".repeat(90));
    for f in &resp.findings {
        println!(
            "{:<36} {:<10} {:<10} {}",
            f.finding_id, f.status, f.severity, f.title
        );
    }
    if resp.truncated {
        println!("\n(truncated — more findings exist beyond the server cap)");
    }
    println!("\n{} finding(s)", resp.findings.len());
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use clap::Parser;

    /// Minimal CLI struct for hermetic parsing tests of `creator world`.
    #[derive(Parser)]
    struct WorldCli {
        #[command(subcommand)]
        command: WorldCommand,
    }

    // ── AR-5: event-add observation flag parsing ──────────────────────

    #[test]
    fn event_add_parses_observer_and_access_flags() {
        let cli = WorldCli::try_parse_from([
            "nexus42",
            "event-add",
            "--world-id",
            "wld_abc123",
            "--observer",
            "kb_ana",
            "--observer",
            "kb_guard",
            "--access",
            r#"{"line_of_sight":true}"#,
        ])
        .expect("event-add with observation flags should parse");
        match cli.command {
            WorldCommand::EventAdd {
                observer,
                access,
                world_id,
                ..
            } => {
                assert_eq!(world_id, "wld_abc123");
                assert_eq!(
                    observer,
                    vec!["kb_ana".to_string(), "kb_guard".to_string()],
                    "--observer is repeatable and preserves order"
                );
                assert_eq!(access.as_deref(), Some(r#"{"line_of_sight":true}"#));
            }
            _ => panic!("expected EventAdd variant"),
        }
    }

    #[test]
    fn event_add_parses_without_observation_flags() {
        let cli = WorldCli::try_parse_from(["nexus42", "event-add", "--world-id", "wld_abc123"])
            .expect("event-add without observation flags should parse");
        match cli.command {
            WorldCommand::EventAdd {
                observer, access, ..
            } => {
                assert!(observer.is_empty(), "no --observer → empty list");
                assert!(access.is_none(), "no --access → None");
            }
            _ => panic!("expected EventAdd variant"),
        }
    }

    // ── AR-5: observation modules builder (tri-state) ─────────────────

    #[test]
    fn build_observation_modules_none_without_flags() {
        // (b) no observation flags → None → modules_json stays NULL at rest.
        assert!(
            build_observation_modules(&[], None).unwrap().is_none(),
            "no observation flags must yield no modules payload"
        );
    }

    #[test]
    fn build_observation_modules_observer_and_access() {
        // (a) observer + access → observation object with both keys.
        let modules = build_observation_modules(
            &["kb_ana".to_string()],
            Some(r#"{"line_of_sight":true,"hearing_range":true}"#),
        )
        .unwrap()
        .expect("observer flags must produce a modules payload");
        let value: serde_json::Value = serde_json::from_str(&modules).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "observation": {
                    "observers": ["kb_ana"],
                    "access": {"line_of_sight": true, "hearing_range": true}
                }
            })
        );
    }

    #[test]
    fn build_observation_modules_multiple_observers_preserve_order() {
        let modules =
            build_observation_modules(&["kb_ana".to_string(), "kb_guard".to_string()], None)
                .unwrap()
                .expect("observers without access must still produce a payload");
        let value: serde_json::Value = serde_json::from_str(&modules).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "observation": {"observers": ["kb_ana", "kb_guard"]}
            })
        );
    }

    #[test]
    fn build_observation_modules_zero_observers_with_access() {
        // (c) explicit nobody: observers: [] + access (PD-9 empty state).
        let modules = build_observation_modules(&[], Some(r#"{"line_of_sight":true}"#))
            .unwrap()
            .expect("access without observers must produce a payload");
        let value: serde_json::Value = serde_json::from_str(&modules).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "observation": {
                    "observers": [],
                    "access": {"line_of_sight": true}
                }
            })
        );
    }

    #[test]
    fn build_observation_modules_rejects_non_object_access() {
        // (d) non-object --access rejected at the CLI boundary.
        for bad in [r"[1,2,3]", r#""just a string""#, "42", "null"] {
            let err = build_observation_modules(&["kb_ana".to_string()], Some(bad)).unwrap_err();
            assert!(
                err.to_string().contains("--access must be a JSON object"),
                "expected object rejection for {bad}, got: {err}"
            );
        }
    }

    #[test]
    fn build_observation_modules_rejects_invalid_json_access() {
        let err = build_observation_modules(&[], Some("{not json")).unwrap_err();
        assert!(
            err.to_string()
                .contains("--access must be a JSON object string"),
            "got: {err}"
        );
    }

    #[test]
    fn build_observation_modules_rejects_empty_observer() {
        // (d) empty/whitespace observer values rejected.
        for bad in ["", "   "] {
            let err = build_observation_modules(&[bad.to_string()], None).unwrap_err();
            assert!(
                err.to_string()
                    .contains("--observer values must be non-empty strings"),
                "expected empty-observer rejection for {bad:?}, got: {err}"
            );
        }
    }
}
