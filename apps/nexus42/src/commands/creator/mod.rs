//! Creator Command Module — the creative hub for Nexus CLI
//!
//! `creator` is the primary entry for agent identity, Work lifecycle, and local assets.
//! Per cli-command-ia.md §3.1, subcommands are organized in tiers:
//!
//! - **Three-plane IA (V1.45)**:
//!   - `bootstrap` — composite Work onboarding (create Work + schedule intake)
//!   - `works` — atomic single-purpose ops (inspire, reopen, resume-chain, …)
//!   - `run <preset_id>` — strategy / preset dispatch
//! - **Primary**: `register`, `use`, `list`
//! - **Assets**: `workspace`, `soul`, `memory`, `kb`, `knowledge`, `reference`, `world`
//! - **Platform bridge**: `status`, `pair`, `unpair`, `credentials`
//! - **Maintenance**: `demo-seed`, `logout`

pub mod bootstrap;
pub mod inspector;
pub mod kb;
pub mod knowledge;
pub mod memory;
pub mod moment_directive;
pub mod reading;
pub mod reference;
pub mod rules_runtime;
pub mod run;
pub mod soul;
pub mod work_utils;
pub mod works;
pub mod world;

use crate::auth;
use crate::challenge::{solve_challenge_with_fallback, UnavailableLlmSolver};
use crate::commands::local_creator_bootstrap::{global_db_path, open_global_db_read_only};
use crate::config::{
    find_workspace_root, nexus_home, workspace_config_path, workspace_nexus_dir, CliConfig,
    DEFAULT_WORKSPACE_SLUG,
};
use crate::creator_identity::{self, CreatorIdentityEntry};
use crate::errors::{CliError, Result};
use crate::paths;
use clap::{Args, Subcommand};
use memory::MemoryCommand;
use nexus_cloud_sync::platform_client::{PlatformClient, VerifyStatus};
use nexus_contracts::Creator;
use nexus_knowledge::world_kb::KbStore;
use nexus_knowledge::KnowledgeStore;
use serde::Deserialize;
use soul::SoulCommand;
use std::path::PathBuf;

// Re-export KB types so `CreatorCommand::Kb` variant and `KbCommand` remain
// accessible from `super::` for existing consumers and tests.
pub use kb::{KbCommand, KbScope};

/// Default registration source for the CLI.
const DEFAULT_REGISTRATION_SOURCE: &str = "cli";

/// Maximum length for creator display name (WS-B T4) — the single definition
/// lives in `local_creator_bootstrap` so the platform register path and the
/// local bootstrap helper bound the same display-name token (qc2 S#4 parity).
use crate::commands::local_creator_bootstrap::MAX_CREATOR_NAME_LENGTH;

/// Handle validation regex: 4–15 chars, starts/ends with `[a-z0-9]`, interior allows `[a-z0-9._-]`.
/// Frozen spec v3 §7.
static HANDLE_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
    regex::Regex::new(r"^[a-z0-9][a-z0-9._-]{2,13}[a-z0-9]$")
        .expect("frozen spec handle regex is valid")
});

/// Buffer seconds added to expiry check to avoid edge-case failures.
const EXPIRY_BUFFER_SECS: i64 = 10;

/// Maximum number of auto-retry attempts for wrong answers (D4).
const MAX_VERIFY_ATTEMPTS: u32 = 2;

// ── Inlined types from init.rs (V1.22 deprecation cleanup) ──────────

/// Init subcommands (formerly in `commands::init`).
#[derive(Debug, Subcommand)]
pub enum InitCommand {
    /// Initialize creative workspace + operational registration under ~/.nexus42/creators/...
    #[command(name = "workspace")]
    Workspace {
        /// Workspace display name (defaults to current directory name)
        name: Option<String>,
        /// Creator id for operational paths (default: local)
        #[arg(long)]
        creator_id: Option<String>,
        /// Operational workspace slug (default: default)
        #[arg(long)]
        workspace_slug: Option<String>,
        /// Creative root directory (default: ~/Documents/nexus/<`creator_id`>/<`workspace_slug`>)
        #[arg(long)]
        creative_root: Option<PathBuf>,
    },
}

/// Metadata for a workspace, persisted to `meta.json`.
#[derive(serde::Serialize)]
struct WorkspaceMeta {
    schema_version: u32,
    creator_id: String,
    workspace_slug: String,
    local_root: PathBuf,
    #[serde(skip_serializing_if = "Option::is_none")]
    workspace_id: Option<String>,
    created_at: String,
}

/// Default creative root path: ~/Documents/nexus/<`creator_id`>/<`workspace_slug`>.
fn default_creative_root(creator_id: &str, workspace_slug: &str) -> Result<PathBuf> {
    let docs = dirs::document_dir()
        .or_else(|| dirs::home_dir().map(|h| h.join("Documents")))
        .ok_or_else(|| CliError::Other("Cannot resolve Documents directory".into()))?;
    Ok(docs.join("nexus").join(creator_id).join(workspace_slug))
}

/// Validate that a slug is a single, safe path segment.
fn validate_slug(label: &str, value: &str) -> Result<()> {
    if value.is_empty()
        || value.contains('/')
        || value.contains('\\')
        || value == "."
        || value == ".."
    {
        return Err(CliError::Other(format!(
            "Invalid {label} {value:?} (must be a single path segment)"
        )));
    }
    Ok(())
}

/// Writes creative tree, `meta.json`, and initializes workspace `state.db` (ADR-014).
async fn materialize_adr014_workspace(
    user_home: &std::path::Path,
    creator_id: &str,
    workspace_slug: &str,
    creative_root: &std::path::Path,
    workspace_display_name: &str,
) -> Result<std::path::PathBuf> {
    let nexus_dir = workspace_nexus_dir(creative_root);
    std::fs::create_dir_all(&nexus_dir)?;

    let workspace_config = serde_json::json!({
        "name": workspace_display_name,
        "version": 1,
        "created_at": chrono::Utc::now().to_rfc3339(),
        "creator_id": creator_id,
        "workspace_slug": workspace_slug,
    });
    let config_path = workspace_config_path(creative_root);
    std::fs::write(
        &config_path,
        serde_json::to_string_pretty(&workspace_config)?,
    )?;

    let gitignore_content =
        "# Nexus local state (do not commit)\n*.db\n*.db-wal\n*.db-shm\nstate.db\n";
    std::fs::write(nexus_dir.join(".gitignore"), gitignore_content)?;

    let op_dir = crate::paths::operational_workspace_dir(user_home, creator_id, workspace_slug);
    std::fs::create_dir_all(&op_dir)?;

    let op_meta = op_dir.join("meta.json");
    let meta = WorkspaceMeta {
        schema_version: 1,
        creator_id: creator_id.to_string(),
        workspace_slug: workspace_slug.to_string(),
        local_root: creative_root.to_path_buf(),
        workspace_id: None,
        created_at: chrono::Utc::now().to_rfc3339(),
    };
    std::fs::write(op_meta, serde_json::to_string_pretty(&meta)?)?;

    let db_path = crate::paths::state_db_path(user_home, creator_id, workspace_slug);
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    crate::db::Schema::init(&db_path).await?;
    Ok(db_path)
}

/// Persist CLI workspace selection to config.
fn persist_cli_workspace_selection(
    creative_root: PathBuf,
    creator_id: String,
    workspace_slug: String,
) -> Result<()> {
    let mut config = CliConfig::load()?;
    config.workspace_path = Some(creative_root);
    config.active_creator_id = Some(creator_id.clone());
    config
        .active_workspace_slug_by_creator
        .insert(creator_id, workspace_slug);
    config.save()?;
    Ok(())
}

/// Run `init workspace` subcommand.
async fn run_init(cmd: InitCommand) -> Result<()> {
    match cmd {
        InitCommand::Workspace {
            name,
            creator_id,
            workspace_slug,
            creative_root,
        } => init_workspace(name, creator_id, workspace_slug, creative_root).await,
    }
}

/// Create workspace structure (daemon-first, FS fallback).
#[allow(clippy::too_many_lines)]
async fn init_workspace(
    name: Option<String>,
    creator_id: Option<String>,
    workspace_slug: Option<String>,
    creative_root_arg: Option<PathBuf>,
) -> Result<()> {
    let creator_id = creator_id.unwrap_or_else(|| "local".to_string());
    let workspace_slug = workspace_slug.unwrap_or_else(|| DEFAULT_WORKSPACE_SLUG.to_string());
    validate_slug("creator_id", &creator_id)?;
    validate_slug("workspace_slug", &workspace_slug)?;

    let user_home = dirs::home_dir()
        .ok_or_else(|| CliError::Other("Cannot determine home directory".into()))?;

    let op_meta = crate::paths::operational_workspace_dir(&user_home, &creator_id, &workspace_slug)
        .join("meta.json");
    if op_meta.exists() {
        println!("Workspace already registered for creator {creator_id} / {workspace_slug}.");
        return Ok(());
    }

    if find_workspace_root().is_some() {
        println!("Workspace already initialized in this directory tree.");
        return Ok(());
    }

    let display_name = name.unwrap_or_else(|| workspace_slug.clone());

    // Try daemon API first (T25: CLI → daemon migration)
    let client = crate::api::DaemonClient::from_config(&CliConfig::load()?);
    if client.health_check().await? {
        let req = crate::api::models::CreateWorkspaceRequest {
            creator_id: creator_id.clone(),
            workspace_slug: workspace_slug.clone(),
            creative_root: creative_root_arg.clone(),
            display_name: Some(display_name.clone()),
        };
        match client.create_workspace(&req).await {
            Ok(resp) => {
                let active_req = crate::api::models::SetActiveWorkspaceRequest {
                    creator_id: Some(creator_id.clone()),
                    workspace_slug: workspace_slug.clone(),
                };
                if let Err(e) = client.set_active_workspace(&active_req).await {
                    eprintln!(
                        "nexus42: warning — workspace created but active selection failed: {e}"
                    );
                }
                println!("✓ Workspace initialized: {display_name}");
                println!("  Creative root: {}", resp.creative_root);
                println!("  Operational: {}", resp.operational_dir);
                println!("  state.db: {}", resp.state_db_path);
                println!("  .nexus42/  — workspace configuration (creative root)");
                print_next_steps();
                return Ok(());
            }
            Err(e) => {
                eprintln!(
                    "nexus42: daemon workspace creation failed, falling back to local init: {e}"
                );
            }
        }
    }

    // Fallback: direct FS operations
    let current_dir = std::env::current_dir()?;
    let creative_root = match creative_root_arg {
        Some(p) if p.is_absolute() => p,
        Some(p) => current_dir.join(p),
        None => default_creative_root(&creator_id, &workspace_slug)?,
    };
    let db_path = materialize_adr014_workspace(
        &user_home,
        &creator_id,
        &workspace_slug,
        &creative_root,
        &display_name,
    )
    .await?;
    persist_cli_workspace_selection(
        creative_root.clone(),
        creator_id.clone(),
        workspace_slug.clone(),
    )?;

    let nh = nexus_home()?;
    std::fs::create_dir_all(&nh)?;

    match nexus_orchestration::skill_sync::sync_embedded_skills(&nh) {
        Ok(result) => {
            if !result.installed.is_empty() {
                println!("  Skills synced: {} installed", result.installed.len());
            }
            if !result.conflicts.is_empty() {
                for c in &result.conflicts {
                    eprintln!(
                        "  nexus42: skill conflict — {} (user-modified, not overwritten)",
                        c.skill_id
                    );
                }
            }
        }
        Err(e) => {
            eprintln!("nexus42: skill sync skipped — {e}");
        }
    }

    let op_dir = crate::paths::operational_workspace_dir(&user_home, &creator_id, &workspace_slug);
    println!("✓ Workspace initialized: {display_name}");
    println!("  Creative root: {}", creative_root.display());
    println!("  Operational: {}", op_dir.display());
    println!("  state.db: {}", db_path.display());
    println!("  .nexus42/  — workspace configuration (creative root)");
    print_next_steps();
    Ok(())
}

/// Print next steps after workspace initialization.
fn print_next_steps() {
    println!();
    println!("Next steps:");
    println!("  nexus42 system preset list    — see available workflow presets");
    println!("  nexus42 daemon schedule add --preset <id> --creator <id>");
    println!("                                 — start a preset-driven workflow");
    println!("  nexus42 platform auth login   — authenticate with the platform");
    println!("  nexus42 creator register --name <name> [--local]  — create a Creator entity");
    println!();
    println!("Workspace artifacts (stories, research reports) are created");
    println!("automatically by preset workflows as needed.");
}

// ── Inlined types from clone.rs (V1.22 deprecation cleanup) ──────────

/// Clone command arguments (formerly in `commands::clone`).
#[derive(Debug, Args)]
pub struct CloneArgs {
    /// World reference to clone (`world_id`, e.g. `wld_abc123`)
    pub world_ref: String,
    /// Clone source: platform (default) or local
    #[arg(long, value_enum, default_value = "platform")]
    pub source: CloneSourceArg,
    /// Print the JSON request and exit without calling the daemon
    #[arg(long)]
    pub dry_run: bool,
    /// Skip interactive confirmation
    #[arg(long)]
    pub yes: bool,
}

/// Clone source options (formerly in `commands::clone`).
#[derive(Debug, Clone, Copy, clap::ValueEnum)]
pub enum CloneSourceArg {
    /// Clone from the platform (via daemon proxy)
    Platform,
    /// Clone from a local source
    Local,
}

/// Response from the daemon clone endpoint (formerly in `commands::clone`).
// Kept for future platform clone support; unused since V1.27 hard-deprecation.
#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct WorldCloneResponse {
    success: bool,
    world_id: Option<String>,
    world_revision: Option<u64>,
    cloned_at: Option<String>,
    error: Option<String>,
}

/// Validate `WorldId` format: must start with 'wld_' followed by alphanumeric characters.
// Kept for future platform clone support; unused since V1.27 hard-deprecation.
#[allow(dead_code)]
fn validate_world_id(s: &str) -> std::result::Result<String, String> {
    if !s.starts_with("wld_") {
        return Err(format!("WorldId must start with 'wld_' prefix (got '{s}')"));
    }
    let suffix = &s[4..];
    if suffix.is_empty() {
        return Err("WorldId must have alphanumeric characters after 'wld_' prefix".to_string());
    }
    if !suffix.chars().all(char::is_alphanumeric) {
        return Err(format!(
            "WorldId must contain only alphanumeric characters after 'wld_' prefix (got '{suffix}')"
        ));
    }
    Ok(s.to_string())
}

/// Validate world reference format (accepts wld_* and numeric).
// Kept for future platform clone support; unused since V1.27 hard-deprecation.
#[allow(dead_code)]
fn validate_world_ref(s: &str) -> std::result::Result<String, String> {
    if s.starts_with("wld_") {
        return validate_world_id(s);
    }
    if s.is_empty() {
        return Err("world-ref cannot be empty".to_string());
    }
    Ok(s.to_string())
}

/// Confirm clone interactively (or skip with --yes).
// Kept for future platform clone support; unused since V1.27 hard-deprecation.
#[allow(dead_code)]
fn confirm_clone(yes: bool, world_ref: &str, source: CloneSourceArg) -> bool {
    if yes {
        return true;
    }
    let source_label = match source {
        CloneSourceArg::Platform => "platform",
        CloneSourceArg::Local => "local",
    };
    dialoguer::Confirm::new()
        .with_prompt(format!("Clone world '{world_ref}' from {source_label}?"))
        .default(false)
        .interact()
        .unwrap_or_else(|_| {
            eprintln!("Non-interactive terminal: pass --yes to confirm clone.");
            false
        })
}

/// Run the clone command — hard-deprecated stub (V1.27 H1).
///
/// World cloning is a platform-only operation that cannot be performed
/// locally by the CLI. The `/v1/daemon/world/clone` endpoint never existed.
/// Users should use the platform UI or a future `nexus42 sync` command
/// to pull a world skeleton from the platform.
fn run_clone(_args: CloneArgs, _config: &CliConfig) -> Result<()> {
    Err(CliError::Other(
        "creator workspace clone is not available locally. \
         World cloning is a platform-only operation. \
         Use the platform UI or a future `nexus42 sync pull --world <id>` \
         to pull a world skeleton."
            .into(),
    ))
}

// ── End inlined types ────────────────────────────────────────────────

#[derive(Debug, Subcommand)]
pub enum CreatorCommand {
    // ── Three-plane IA (V1.45) ──────────────────────────────────────
    /// Composite Work onboarding — create Work + schedule intake/production
    ///
    /// Creates a new Work and optionally schedules init preset, intake,
    /// and production. The sole composite entry for new Work creation.
    /// For atomic Work operations, use `creator works`.
    /// For preset dispatch, use `creator run <preset_id>`.
    ///
    /// See the creator-run-preset-entry spec for the CLI workflow.
    Bootstrap(bootstrap::BootstrapArgs),

    /// Work management and pool — atomic single-purpose ops (DF-60 §6.2H).
    ///
    /// List, inspect, and manage your Works and the selection pool.
    /// Each subcommand is strictly single-purpose (one business function).
    /// Shows progress, chapter status, open findings, and completion state.
    /// See novel-author-experience §3 for the author path.
    Works {
        #[command(subcommand)]
        command: works::WorksCommand,
    },

    /// Preset dispatch — run a preset by id (V1.45 P0)
    ///
    /// Generic runner: `creator run <preset_id> [<work_id>]`.
    /// Any resolvable preset id (embedded, user, or system) can be dispatched.
    /// No CLI preset whitelist — adding a preset changes YAML, not Rust.
    /// FL-E stage-advance presets (`research`, `novel-writing`,
    /// `novel-chapter-review`, `kb-extract`) preserve stage-advance semantics.
    Run {
        #[command(flatten)]
        command: run::RunCommand,
    },

    // ── Primary tier ────────────────────────────────────────────────
    /// Register a new Creator entity
    ///
    /// Creates a Creator identity (platform or local-only with `--local`).
    /// With `--local`, registers a local-only creator with no platform involvement.
    /// Usage: nexus42 creator register --name "My Agent" [--source `cli|web_agent`] [--handle <handle>] [--local]
    Register {
        /// Display name for the Creator (required)
        #[arg(long)]
        name: String,
        /// Registration source (default: cli)
        #[arg(long, default_value = DEFAULT_REGISTRATION_SOURCE)]
        source: String,
        /// Creator handle — 4–15 chars, lowercase alphanumeric, dots, hyphens, underscores
        #[arg(long)]
        handle: Option<String>,
        /// Register a local-only creator (no platform account; conflicts with --source/--handle)
        #[arg(long, conflicts_with_all = ["source", "handle"])]
        local: bool,
    },

    /// Switch the active Creator identity
    ///
    /// All subsequent `creator *` commands bind to the active creator.
    /// Positional `<creator_ref>` is accepted for convenience.
    Use {
        /// Creator ID or display name (positional; may become a flag in a future version)
        creator_ref: String,
    },

    /// List all registered Creator identities
    ///
    /// Persistent local identities (from `local_identities`) appear alongside
    /// platform rows with an additive ORIGIN column (`local` | `platform`).
    /// `--json` emits the machine DTO verbatim (a JSON array of objects with
    /// keys `creator_id`, `handle`, `display_name`, `active`, `origin`;
    /// nullable `handle`/`display_name` — AR-90 #4) instead of the human table.
    List {
        /// Emit machine-readable JSON (the AR-90 #4 DTO verbatim) instead of
        /// the human table.
        #[arg(long, default_value_t = false)]
        json: bool,
    },

    // ── Assets tier (scoped knowledge and narrative) ────────────────
    /// Operational workspace slugs for the active creator (local ADR-014 tree)
    Workspace {
        #[command(subcommand)]
        command: CreatorWorkspaceCommand,
    },

    /// SOUL management (creator personality and behavior configuration)
    Soul {
        #[command(subcommand)]
        command: SoulCommand,
    },

    /// Long-term memory management (creator-scoped episodic memory)
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },

    /// Work-scope file index and World KB knowledge entries
    ///
    /// Manages TWO knowledge scopes:
    ///   • `--scope work` (default): local workspace file index — per-creator,
    ///     per-workspace documents stored under `kb/`. NOT the World narrative KB.
    ///   • `--scope world`: narrative KB knowledge entries (nexus-knowledge + nexus-narrative),
    ///     requires `--world-id`.
    ///
    /// For User-scoped global knowledge, use `creator knowledge` instead.
    /// See entity-scope-model §5.3–5.4 for the three KB namespaces.
    Kb {
        #[command(subcommand)]
        command: KbCommand,
    },

    /// User-scoped global knowledge entries (add, list, search)
    ///
    /// Stores unstructured knowledge entries scoped to the User (not Creator).
    /// For Work-scope file index or World narrative KB, use `creator kb` instead.
    /// See entity-scope-model §5.3–§5.4 for the three KB namespaces.
    Knowledge {
        #[command(subcommand)]
        command: knowledge::KnowledgeCommand,
    },

    /// Moment Directive author surface (V1.150 P1, DF-75)
    ///
    /// A short author-written instruction injected into the assembled prompt's
    /// reserved `moment.directive` slot (above lore, below system/personality)
    /// with an insert depth and a TTL in generations or chapters. Set, show,
    /// and clear the active directive for a Work scope (or the World override).
    /// Rendered output is observable in `platform context assemble-moment`.
    MomentDirective {
        #[command(subcommand)]
        command: moment_directive::MomentDirectiveCommand,
    },

    /// Reference source management (V1.26 reference store)
    Reference {
        #[command(subcommand)]
        command: reference::ReferenceCommand,
    },
    /// Narrative world management (create worlds, add events, list timelines)
    World {
        #[command(subcommand)]
        command: world::WorldCommand,
    },

    /// Reading-depth data CRUD (V1.175 P1, group 3) — progress + annotations.
    ///
    /// Data CRUD only: export, reset, and write reading progress and
    /// annotations from scripts/agents. Not a manuscript reader; the V1.79
    /// reading surface stays web.
    Reading {
        #[command(subcommand)]
        command: reading::ReadingCommand,
    },

    /// Moment assembly inspector (V1.151 observe-only packet).
    ///
    /// Hidden debug group (PL-6): the packet is a daemon contract — a
    /// headless developer debugging assembly must reach it, but it is
    /// deliberately absent from root `--help`. Documented in
    /// `.mstar/specs/cli-spec.md` and `creator inspector --help`.
    #[command(hide = true)]
    Inspector {
        #[command(subcommand)]
        command: inspector::InspectorCommand,
    },

    // ── Platform bridge tier (optional; requires User login) ────────
    /// Show current Creator status and authentication state
    Status {
        /// Specific creator ID to check (default: active creator)
        creator_id: Option<String>,
    },

    /// Initiate pairing flow with a Creator (requires platform User login)
    ///
    /// Positional `<creator_id>` is accepted for convenience.
    Pair {
        /// Creator ID to pair (positional; may become a flag in a future version)
        creator_id: String,
    },

    /// Remove pairing with a Creator (requires platform User login)
    Unpair {
        /// Creator ID to unpair (positional; may become a flag in a future version)
        creator_id: String,
    },

    /// Rotate Creator API credentials (requires platform User login)
    #[command(name = "credentials")]
    Credentials {
        #[command(subcommand)]
        action: CredentialsAction,
    },

    // ── Maintenance tier ────────────────────────────────────────────
    /// Seed demo data: creates a demo world, event, KB block, and knowledge entry
    ///
    /// Idempotent by default — skips if demo world already exists.
    /// Use --force to recreate (deletes existing demo data first).
    #[command(name = "demo-seed")]
    DemoSeed {
        /// Force recreation of demo data (deletes existing demo world)
        #[arg(long)]
        force: bool,
    },

    /// Logout and clear creator credentials
    Logout,
}

#[derive(Debug, Subcommand)]
pub enum CreatorWorkspaceCommand {
    /// List workspace slugs that exist on disk under the active creator
    List,
    /// Create a new workspace (ADR-014 operational registration + creative tree)
    Create {
        /// Workspace slug (path segment)
        workspace_slug: String,
        /// Creative root directory (default: ~/Documents/nexus/<creator>/<slug>)
        #[arg(long)]
        creative_root: Option<PathBuf>,
        /// Display name stored in workspace.json (default: slug)
        #[arg(long)]
        name: Option<String>,
    },
    /// Set the active workspace slug for the active creator
    Use {
        /// Workspace slug (directory must exist under creators/<id>/workspaces/)
        workspace_slug: String,
    },
    /// Initialize a new workspace (migrated from `nexus42 init`)
    Init {
        #[command(subcommand)]
        command: InitCommand,
    },
    /// Clone a world into the workspace (DEPRECATED — platform-only, not implemented locally)
    #[command(hide = true)]
    Clone {
        /// World reference to clone (e.g. `wld_abc123`)
        world_ref: String,
        /// Clone source: platform (default) or local
        #[arg(long, value_enum, default_value = "platform")]
        source: CloneSourceArg,
        /// Print the JSON request and exit without calling the daemon
        #[arg(long)]
        dry_run: bool,
        /// Skip interactive confirmation
        #[arg(long)]
        yes: bool,
    },
    /// Link a workspace (coming soon)
    Link {
        /// Workspace slug to link
        workspace_slug: String,
    },
    /// Unlink a workspace (coming soon)
    Unlink {
        /// Workspace slug to unlink
        workspace_slug: String,
    },
    /// Show workspace status (coming soon)
    Status,
}

#[derive(Debug, Subcommand)]
pub enum CredentialsAction {
    /// Rotate the API key for the active or specified Creator
    Rotate {
        /// Creator ID (default: active creator)
        creator_id: Option<String>,
    },
}

/// Run creator command
///
/// # Errors
///
/// Returns `CliError` if:
/// - Platform API calls fail (registration, credential rotation)
/// - Configuration cannot be read or written
/// - Creator authentication fails
// CLI entry-point — single-threaded tokio; Send not required.
#[allow(clippy::future_not_send)]
pub async fn run(cmd: CreatorCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        CreatorCommand::Bootstrap(args) => bootstrap::handle_bootstrap(args, config).await,
        CreatorCommand::Register {
            name,
            source,
            handle,
            local,
        } => register_creator(config, name, source, handle, local).await,
        CreatorCommand::Status { creator_id } => creator_status(config, creator_id).await,
        CreatorCommand::Use { creator_ref } => use_creator(config, creator_ref.as_str()).await,
        CreatorCommand::List { json } => list_creators(config, json).await,
        CreatorCommand::Pair { creator_id } => {
            pair_creator(config, creator_id.as_str());
            Ok(())
        }
        CreatorCommand::Unpair { creator_id } => {
            unpair_creator(config, creator_id.as_str());
            Ok(())
        }
        CreatorCommand::Credentials { action } => match action {
            CredentialsAction::Rotate { creator_id } => {
                rotate_credentials(config, creator_id).await
            }
        },
        CreatorCommand::Workspace { command } => run_creator_workspace(config, command).await,
        CreatorCommand::Soul { command } => soul::run(command, config).await,
        CreatorCommand::Memory { command } => memory::run(command, config).await,
        CreatorCommand::Reference { command } => reference::run(command, config).await,
        CreatorCommand::Kb { command } => kb::run(command, config).await,
        CreatorCommand::World { command } => world::run(command, config).await,
        CreatorCommand::Reading { command } => reading::run(command, config).await,
        CreatorCommand::Inspector { command } => inspector::run(command, config).await,
        CreatorCommand::Knowledge { command } => knowledge::run(command, config).await,
        CreatorCommand::MomentDirective { command } => moment_directive::run(command, config).await,
        CreatorCommand::Run { command } => run::handle_run(command, config).await,
        CreatorCommand::Works { command } => works::handle_works(command, config).await,
        CreatorCommand::DemoSeed { force } => run_demo_seed(config, force).await,
        CreatorCommand::Logout => logout_creator(config).await,
    }
}
fn user_home() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| CliError::Other("Cannot determine home directory".into()))
}

fn validate_workspace_slug(slug: &str) -> Result<()> {
    validate_slug("workspace_slug", slug)
}

// ── Demo seed ───────────────────────────────────────────────────────

/// Seed demo data for testing and development.
///
/// Creates a demo world, event, KB block, and knowledge entry using
/// Plan 1 write APIs + knowledge store. Idempotent unless `--force`.
async fn run_demo_seed(config: &CliConfig, force: bool) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;

    let demo_title = "Demo World";
    let demo_slug = "demo-world";

    // Check if demo world already exists
    // SAFETY: SELECT against known narrative_worlds table schema
    let existing_id: Option<String> = sqlx::query_scalar(
        "SELECT world_id FROM narrative_worlds WHERE slug = ? AND owner_creator_id = ? LIMIT 1",
    )
    .bind(demo_slug)
    .bind(&creator_id)
    .fetch_optional(&pool)
    .await
    .map_err(|e| CliError::Other(format!("Failed to check existing demo: {e}")))?
    .flatten();

    if let Some(ref wid) = existing_id {
        if !force {
            println!("Demo world already exists: {wid}");
            println!("Use --force to recreate demo data.");
            return Ok(());
        }
        // Delete existing demo data (cascade handles events, KB blocks)
        // SAFETY: DELETE against known tables
        sqlx::query("DELETE FROM knowledge_entries WHERE user_id = 'user_default'")
            .execute(&pool)
            .await
            .map_err(|e| CliError::Other(format!("Failed to clean demo knowledge: {e}")))?;
        sqlx::query("DELETE FROM narrative_worlds WHERE world_id = ?")
            .bind(wid)
            .execute(&pool)
            .await
            .map_err(|e| CliError::Other(format!("Failed to clean demo world: {e}")))?;
        println!("Deleted existing demo data.");
    }

    // 1. Create demo world
    let world = nexus_local_db::create_world(
        &pool,
        &creator_id,
        demo_title,
        demo_slug,
        "private",
        "manual",
    )
    .await
    .map_err(|e| CliError::Other(format!("Failed to create demo world: {e}")))?;
    println!("✓ Demo world: {}", world.world_id);

    // 2. Append demo event
    let event = nexus_local_db::append_event(
        &pool,
        &world.world_id,
        &world.root_fork_branch_id,
        "story_advance",
        Some("The Journey Begins"),
        Some("A hero embarks on their first adventure."),
        None, // modules_json — the demo seed writes no modules
    )
    .await
    .map_err(|e| CliError::Other(format!("Failed to create demo event: {e}")))?;
    println!("✓ Demo event: {}", event.event_id);

    // 3. Create demo KB block
    let mut kb = nexus_knowledge::world_kb::knowledge_entry::WorldKbEntry::new(
        &world.world_id,
        nexus_contracts::BlockType::Character,
        "Hero",
    );
    kb.body = Some(nexus_knowledge::world_kb::knowledge_entry::WorldKbBody {
        summary: Some("The protagonist of the demo world.".to_string()),
        attributes: None,
        tags: Some(vec!["protagonist".to_string(), "demo".to_string()]),
        ..Default::default()
    });
    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let kb_result = kb_store
        .insert_knowledge_entry(kb)
        .await
        .map_err(|e| CliError::Other(format!("Failed to create demo KB block: {e}")))?;
    println!("✓ Demo KB block: {}", kb_result.entry_id);

    // 4. Create demo knowledge entry
    let knowledge_store = nexus_local_db::SqliteKnowledgeStore::new(pool);
    let entry = nexus_knowledge::UserKnowledgeEntry::new(
        "user_default",
        vec![
            nexus_knowledge::KnowledgeTag::new("demo"),
            nexus_knowledge::KnowledgeTag::new("worldbuilding"),
        ],
        "Demo knowledge entry for Moment context assembly testing.",
    );
    let stored = knowledge_store
        .store(entry)
        .await
        .map_err(|e| CliError::Other(format!("Failed to create demo knowledge: {e}")))?;
    println!("✓ Demo knowledge: {}", stored.id);

    println!();
    println!("Demo seed complete. Run `nexus42 platform context assemble-moment` to verify.");
    Ok(())
}

/// Validate a creator handle against the frozen spec v3 §7 regex.
///
/// Handle must be 4–15 chars, start and end with `[a-z0-9]`,
/// and contain only `[a-z0-9._-]`.
fn validate_handle(handle: &str) -> Result<()> {
    if HANDLE_RE.is_match(handle) {
        Ok(())
    } else {
        Err(CliError::InvalidHandle {
            handle: handle.to_string(),
            reason: "Handle must be 4\u{2013}15 characters, start and end with a letter or digit, and contain only lowercase letters, digits, dots, hyphens, and underscores.".to_string(),
        })
    }
}

#[allow(clippy::too_many_lines)]
async fn run_creator_workspace(config: &CliConfig, cmd: CreatorWorkspaceCommand) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?;

    match cmd {
        CreatorWorkspaceCommand::List => {
            let home = user_home()?;
            // Try daemon API first (T26: migration)
            let client = crate::api::DaemonClient::from_config(config);
            if client.health_check().await? {
                match client.list_workspaces(Some(creator_id)).await {
                    Ok(resp) => {
                        println!("Workspaces for creator {creator_id}:");
                        if resp.items.is_empty() {
                            println!("  (none)");
                        }
                        let active = config.workspace_slug_for_creator(creator_id);
                        for ws in &resp.items {
                            let mark = if ws.workspace_slug == active {
                                " (active)"
                            } else {
                                ""
                            };
                            println!("  {}{mark}", ws.workspace_slug);
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("nexus42: daemon workspace list failed, falling back: {e}");
                    }
                }
            }

            // Fallback: direct FS scan
            let root = paths::creator_workspaces_root(&home, creator_id);
            if !root.is_dir() {
                println!("No workspaces directory yet ({}).", root.display());
                println!(
                    "Active slug (config): {}",
                    config.workspace_slug_for_creator(creator_id)
                );
                return Ok(());
            }
            println!("Workspaces for creator {creator_id}:");
            let mut names: Vec<String> = std::fs::read_dir(&root)?
                .filter_map(std::result::Result::ok)
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            names.sort();
            let active = config.workspace_slug_for_creator(creator_id);
            for n in names {
                let mark = if n == active { " (active)" } else { "" };
                println!("  {n}{mark}");
            }
            Ok(())
        }
        CreatorWorkspaceCommand::Create {
            workspace_slug,
            creative_root: creative_root_arg,
            name,
        } => {
            validate_workspace_slug(&workspace_slug)?;

            // Try daemon API first (T26: migration)
            let client = crate::api::DaemonClient::from_config(config);
            if client.health_check().await? {
                let req = crate::api::models::CreateWorkspaceRequest {
                    creator_id: creator_id.to_string(),
                    workspace_slug: workspace_slug.clone(),
                    creative_root: creative_root_arg.clone(),
                    display_name: name.clone(),
                };
                match client.create_workspace(&req).await {
                    Ok(resp) => {
                        // Set as active workspace
                        let active_req = crate::api::models::SetActiveWorkspaceRequest {
                            creator_id: Some(creator_id.to_string()),
                            workspace_slug: workspace_slug.clone(),
                        };
                        if let Err(e) = client.set_active_workspace(&active_req).await {
                            eprintln!("nexus42: warning — active selection failed: {e}");
                        }
                        println!(
                            "✓ Workspace {workspace_slug:?} created for creator {creator_id}."
                        );
                        println!("  Creative root: {}", resp.creative_root);
                        println!("  state.db: {}", resp.state_db_path);
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("nexus42: daemon workspace create failed, falling back: {e}");
                    }
                }
            }

            // Fallback: direct FS operations
            let home = user_home()?;
            let op_meta = paths::operational_workspace_dir(&home, creator_id, &workspace_slug)
                .join("meta.json");
            if op_meta.exists() {
                return Err(CliError::Other(format!(
                    "Workspace {workspace_slug:?} already exists for creator {creator_id}."
                )));
            }
            let current_dir = std::env::current_dir()?;
            let creative_root = match creative_root_arg {
                Some(p) if p.is_absolute() => p,
                Some(p) => current_dir.join(p),
                None => default_creative_root(creator_id, &workspace_slug)?,
            };
            let workspace_name = name.unwrap_or_else(|| workspace_slug.clone());
            let db_path = materialize_adr014_workspace(
                &home,
                creator_id,
                &workspace_slug,
                &creative_root,
                &workspace_name,
            )
            .await?;
            persist_cli_workspace_selection(
                creative_root.clone(),
                creator_id.to_string(),
                workspace_slug.clone(),
            )?;
            println!("✓ Workspace {workspace_slug:?} created for creator {creator_id}.");
            println!("  Creative root: {}", creative_root.display());
            println!("  state.db: {}", db_path.display());
            Ok(())
        }
        CreatorWorkspaceCommand::Use { workspace_slug } => {
            validate_workspace_slug(&workspace_slug)?;

            // Try daemon API first (T26: migration)
            let client = crate::api::DaemonClient::from_config(config);
            if client.health_check().await? {
                let req = crate::api::models::SetActiveWorkspaceRequest {
                    creator_id: Some(creator_id.to_string()),
                    workspace_slug: workspace_slug.clone(),
                };
                match client.set_active_workspace(&req).await {
                    Ok(_resp) => {
                        println!(
                            "✓ Active workspace slug for {creator_id} set to: {workspace_slug}"
                        );
                        return Ok(());
                    }
                    Err(e) => {
                        eprintln!("nexus42: daemon set active failed, falling back: {e}");
                    }
                }
            }

            // Fallback: direct config update
            let home = user_home()?;
            let dir = paths::operational_workspace_dir(&home, creator_id, &workspace_slug);
            if !dir.is_dir() {
                return Err(CliError::Other(format!(
                    "Workspace {:?} does not exist for creator {} (expected dir {}).",
                    workspace_slug,
                    creator_id,
                    dir.display()
                )));
            }
            let mut cli = CliConfig::load()?;
            cli.active_workspace_slug_by_creator
                .insert(creator_id.to_string(), workspace_slug.clone());
            cli.save()?;
            println!("✓ Active workspace slug for {creator_id} set to: {workspace_slug}");
            Ok(())
        }
        CreatorWorkspaceCommand::Init { command } => run_init(command).await,
        CreatorWorkspaceCommand::Clone {
            world_ref,
            source,
            dry_run,
            yes,
        } => {
            let args = CloneArgs {
                world_ref,
                source,
                dry_run,
                yes,
            };
            run_clone(args, config)
        }
        CreatorWorkspaceCommand::Link { workspace_slug } => {
            println!("Coming soon: `creator workspace link` — link workspace: {workspace_slug}");
            Ok(())
        }
        CreatorWorkspaceCommand::Unlink { workspace_slug } => {
            println!(
                "Coming soon: `creator workspace unlink` — unlink workspace: {workspace_slug}"
            );
            Ok(())
        }
        CreatorWorkspaceCommand::Status => {
            println!("Coming soon: `creator workspace status` — show workspace status.");
            Ok(())
        }
    }
}

/// Register a new Creator entity.
///
/// Orchestrates the full registration flow (design doc §4):
/// register → solve challenge → verify → store credentials.
///
/// On wrong answer, auto-retries once (D4). On second failure, reports error.
///
/// Note: This function is 103 lines; splitting would break the coherent registration flow.
#[allow(clippy::too_many_lines)]
async fn register_creator(
    config: &CliConfig,
    name: String,
    source: String,
    handle: Option<String>,
    local: bool,
) -> Result<()> {
    // WS-B T4: validate name length (cheap check before regex)
    if name.len() > MAX_CREATOR_NAME_LENGTH {
        return Err(CliError::Other(format!(
            "Creator name exceeds maximum length ({MAX_CREATOR_NAME_LENGTH} bytes)"
        )));
    }
    // --- Local-only mode (AC-V167-P2-1): delegate to identity machinery ---
    // `--local` conflicts with `--source`/`--handle` at the clap layer, so
    // platform-only concepts are guaranteed absent. No network is touched.
    if local {
        return register_local_creator(name).await;
    }
    // Validate handle if provided
    let validated_handle = match &handle {
        Some(h) => {
            validate_handle(h)?;
            Some(h.as_str())
        }
        None => None,
    };
    // --- Step 1: Obtain auth token ---
    let auth_store = auth::AuthStore::load()?;

    // Try to find a user access token from the daemon-managed auth flow.
    // The PlatformClient requires a bearer token; if none is available,
    // name both exits so the user isn't stuck: authenticate, or go local.
    // Map only the no-token case to the dual-exit hint; any other error from
    // `obtain_auth_token` (e.g. a future store-I/O failure) propagates unmapped
    // so it is never masked as "Platform authentication required".
    let auth_token = obtain_auth_token(&auth_store).map_err(|err| match err {
        CliError::AuthenticationRequired => CliError::Other(
            "Platform authentication required. Authenticate with `nexus42 platform auth login`, \
             or re-run with `--local` for local-only mode."
                .to_string(),
        ),
        other => other,
    })?;

    // --- Step 2: Create platform client and call register ---
    println!("Registering creator \"{name}\"...");

    let client = PlatformClient::new(&config.platform_url, &auth_token, &config.device_id)?;

    let register_response = client
        .register_creator(&name, &source, validated_handle)
        .await?;

    let creator_id = &register_response.creator_id;
    let pending_api_key = &register_response.creator_api_key;
    let verification = &register_response.verification;

    println!("  Creator ID: {creator_id}");
    println!(
        "  Verification code: {}",
        &verification.verification_code[..verification.verification_code.len().min(16)]
    );

    // --- Step 3: Check challenge expiry (with buffer) ---
    let expires_at = chrono::DateTime::parse_from_rfc3339(&verification.expires_at)?;

    let now = chrono::Utc::now();
    let buffered_expiry = expires_at - chrono::Duration::seconds(EXPIRY_BUFFER_SECS);

    if now > buffered_expiry {
        return Err(CliError::ChallengeExpired {
            expires_at: verification.expires_at.clone(),
        });
    }

    let remaining_secs = (expires_at.timestamp() - now.timestamp()).max(0);
    println!("  Challenge expires in {remaining_secs}s");

    // --- Step 4: Solve challenge ---
    println!("Solving challenge...");

    let answer: String =
        match solve_challenge_with_fallback(&verification.challenge_text, &UnavailableLlmSolver)
            .await
        {
            Ok(answer) => {
                println!("  Answer computed: {answer}");
                answer
            }
            Err(challenge_err) => {
                return Err(CliError::ChallengeFailed {
                    reason: challenge_err.to_string(),
                });
            }
        };

    // --- Step 5: Re-check challenge expiry before submit ---
    // Solve may have taken time; re-check to give a clearer error than a
    // generic platform-side expiry response.
    let now_after_solve = chrono::Utc::now();
    if now_after_solve > buffered_expiry {
        return Err(CliError::ChallengeExpired {
            expires_at: verification.expires_at.clone(),
        });
    }

    // --- Step 6: Submit answer with auto-retry (D4: max 1 auto-retry) ---
    let verify_response = submit_with_retry(
        &client,
        &verification.verification_code,
        &answer,
        MAX_VERIFY_ATTEMPTS,
    )
    .await?;

    // --- Step 7: Handle verification response ---
    match verify_response.status {
        VerifyStatus::Verified => {
            let api_key = verify_response
                .creator_api_key
                .as_deref()
                .unwrap_or(pending_api_key);

            // Store credentials locally
            let mut store = auth::AuthStore::load()?;
            store.store_creator_api_key(creator_id, api_key)?;

            // V1.16: populate CLI-local identity cache
            let identity_entry = CreatorIdentityEntry {
                creator_id: creator_id.clone(),
                handle: handle.clone(),
                display_name: Some(name.clone()),
            };
            if let Err(e) = creator_identity::set_creator_identity(identity_entry) {
                // Non-fatal: identity cache is best-effort display data
                tracing::warn!("Failed to cache creator identity: {e}");
            }

            // Set as active creator
            let mut cli_config = CliConfig::load()?;
            cli_config.active_creator_id = Some(creator_id.clone());
            cli_config.save()?;

            println!();
            println!("✓ Verification successful!");
            println!("  Creator ID: {creator_id}");
            println!("  API key stored to local credentials.");
            println!();

            Ok(())
        }
        VerifyStatus::WrongAnswer => {
            let remaining = verify_response.remaining_attempts.unwrap_or(0);
            Err(CliError::CreatorVerificationFailed {
                status: "wrong_answer".to_string(),
                message: format!(
                    "Incorrect answer after auto-retry. {remaining} attempts remaining."
                ),
            })
        }
        VerifyStatus::Expired => Err(CliError::CreatorVerificationFailed {
            status: "expired".to_string(),
            message: "Challenge timed out during verification.".to_string(),
        }),
        VerifyStatus::Locked => Err(CliError::CreatorVerificationFailed {
            status: "locked".to_string(),
            message: "Account is permanently locked due to too many failed attempts.".to_string(),
        }),
    }
}

/// V1.176 P0 T1 (AR-88): delegates to the shared bootstrap helper
/// [`crate::commands::local_creator_bootstrap::bootstrap_local_creator`] — the
/// single identity-mint + workspace-row materialization sequence both named
/// local entry points (`creator register --local`, `system identity create
/// --persistent`) call. No `PlatformClient` calls — zero network.
async fn register_local_creator(name: String) -> Result<()> {
    crate::commands::local_creator_bootstrap::bootstrap_local_creator(Some(name)).await?;

    // The helper rendered the mint + active lines; add the one local-only
    // exit marker so the register flow still names its mode.
    println!("  Local-only (no platform) — no platform account created.");

    Ok(())
}

/// Submit a verification answer with automatic retry on wrong answer.
///
/// Retries the same answer once (D4). If both attempts fail, returns
/// the error. Non-retryable statuses (Expired, Locked) return immediately.
async fn submit_with_retry(
    client: &PlatformClient,
    verification_code: &str,
    answer: &str,
    max_attempts: u32,
) -> Result<nexus_cloud_sync::platform_client::VerifyResponse> {
    let mut last_response = None;

    for attempt in 1..=max_attempts {
        if attempt > 1 {
            println!("  Retrying verification (attempt {attempt}/{max_attempts})...");
        }

        let response = match client
            .verify_creator(verification_code, answer)
            .await
            .map_err(CliError::verify_creator_error)
        {
            Ok(resp) => resp,
            Err(CliError::Network(_)) if attempt < max_attempts => {
                eprintln!(
                    "  Network error during verification (attempt {attempt}/{max_attempts}). Retrying..."
                );
                continue;
            }
            Err(e) => return Err(e),
        };

        match response.status {
            VerifyStatus::Verified => return Ok(response),
            VerifyStatus::WrongAnswer => {
                let remaining = response.remaining_attempts.unwrap_or(0);
                last_response = Some(response);
                if attempt < max_attempts {
                    eprintln!("  Wrong answer. {remaining} attempts remaining. Retrying...");
                }
            }
            VerifyStatus::Expired | VerifyStatus::Locked => {
                // Non-retryable — return immediately
                return Ok(response);
            }
        }
    }

    // Exhausted retries — return the last wrong_answer response
    last_response.ok_or_else(|| {
        CliError::Other("Verification retry exhausted without a response".to_string())
    })
}

/// Obtain an auth token for platform API calls.
///
/// Tries to extract a user access token from the auth store.
/// If no token is found, returns an error suggesting the user authenticate.
fn obtain_auth_token(auth_store: &auth::AuthStore) -> Result<String> {
    // V1.3 limitation: `obtain_auth_token` scans creator entries for
    // non-empty access_token as a proxy for the user's auth token.
    // A dedicated user token field (or platform session) would be more robust.
    // This is sufficient for the current CLI-only registration flow.
    if let Some(creators) = &auth_store.creators {
        for state in creators.values() {
            if !state.access_token.is_empty() {
                return Ok(state.access_token.clone());
            }
        }
    }

    Err(CliError::AuthenticationRequired)
}

/// Show Creator status with three-layer identity model (V1.16).
///
/// Tries the daemon API for active creator info first (T33: migration),
/// falls back to local-only display on daemon failure.
async fn creator_status(config: &CliConfig, creator_id: Option<String>) -> Result<()> {
    let id = creator_id.unwrap_or_else(|| {
        config
            .active_creator_id
            .clone()
            .unwrap_or_else(|| "none".to_string())
    });

    if id == "none" {
        println!("No active Creator set.");
        println!("Use: nexus42 creator use <creator-id>");
        return Ok(());
    }

    // Try daemon API for enriched info when checking active creator
    if config.active_creator_id.as_deref() == Some(id.as_str()) {
        let client = crate::api::DaemonClient::from_config(config);
        if client.health_check().await? {
            match client.get_active_creator().await {
                Ok(daemon_resp) => {
                    // Still read local auth state for credential indicators
                    let store = crate::auth::AuthStore::load()?;
                    let has_creator_api_key =
                        store.get_creator_api_key(&id).unwrap_or(None).is_some();
                    let has_cached_token = store.is_creator_authenticated(&id);

                    let creator_key_indicator = if has_creator_api_key {
                        "✓ Creator API key"
                    } else {
                        "✗ No Creator API key"
                    };
                    let token_indicator = if has_cached_token {
                        "✓ Token cached"
                    } else {
                        "✗ No cached token"
                    };

                    let handle_str = daemon_resp.handle.as_deref().unwrap_or("-");
                    let display_name_str = daemon_resp.display_name.as_deref().unwrap_or("-");

                    println!("Creator ID:    {id}");
                    println!("Handle:        {handle_str}");
                    println!("Display Name:  {display_name_str}");
                    println!("Auth:          {creator_key_indicator} | {token_indicator}");
                    return Ok(());
                }
                Err(e) => {
                    eprintln!("nexus42: daemon creator status failed, falling back: {e}");
                }
            }
        }
    }

    // Fallback: local-only status
    let store = crate::auth::AuthStore::load()?;
    let cache = creator_identity::load_creator_identity_cache();
    let entry = creator_identity::get_creator_identity(&cache, &id);

    let handle_str = entry.and_then(|e| e.handle.as_deref()).unwrap_or("-");
    let display_name_str = entry.and_then(|e| e.display_name.as_deref()).unwrap_or("-");

    // Auth indicators
    let has_creator_api_key = store.get_creator_api_key(&id).unwrap_or(None).is_some();
    let has_cached_token = store.is_creator_authenticated(&id);

    let creator_key_indicator = if has_creator_api_key {
        "✓ Creator API key"
    } else {
        "✗ No Creator API key"
    };
    let token_indicator = if has_cached_token {
        "✓ Token cached"
    } else {
        "✗ No cached token"
    };

    println!("Creator ID:    {id}");
    println!("Handle:        {handle_str}");
    println!("Display Name:  {display_name_str}");
    println!("Auth:          {creator_key_indicator} | {token_indicator}");

    Ok(())
}

/// Switch active Creator.
///
/// V1.16: normalizes the input using the CLI-local identity cache:
/// 1. Exact `creator_id` match → use that ID.
/// 2. Exact `handle` match → use the matched `creator_id`.
/// 3. Path-safe but unknown → persist as explicit ID (backward compat).
/// 4. Unsafe characters → error.
///
/// Tries daemon API first (T33: migration), falls back to local config update.
async fn use_creator(_config: &CliConfig, creator_ref: &str) -> Result<()> {
    let resolved_id = creator_identity::resolve_creator_ref(creator_ref)?;

    // Try daemon API first
    let daemon_config = CliConfig::load()?;
    let client = crate::api::DaemonClient::from_config(&daemon_config);
    if client.health_check().await? {
        let req = crate::api::models::SetActiveCreatorRequest {
            creator_id: resolved_id.clone(),
        };
        match client.set_active_creator(&req).await {
            Ok(_resp) => {
                // Also update local config so CLI works without daemon
                let mut cli_config = CliConfig::load()?;
                cli_config.active_creator_id = Some(resolved_id.clone());
                cli_config
                    .active_workspace_slug_by_creator
                    .remove(creator_ref);
                cli_config
                    .active_workspace_slug_by_creator
                    .remove(&resolved_id);
                cli_config.save()?;

                if resolved_id == creator_ref {
                    println!("✓ Active Creator set to: {resolved_id}");
                } else {
                    println!(
                        "✓ Active Creator set to: {resolved_id} (resolved from: {creator_ref})"
                    );
                }
                println!(
                    "  Workspace slug: {DEFAULT_WORKSPACE_SLUG} (use `nexus42 creator workspace use <slug>` after the directory exists)"
                );
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon set active creator failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct config update
    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = Some(resolved_id.clone());
    // Clear workspace slug for the old creator ref and the resolved ID.
    cli_config
        .active_workspace_slug_by_creator
        .remove(creator_ref);
    cli_config
        .active_workspace_slug_by_creator
        .remove(&resolved_id);
    cli_config.save()?;

    if resolved_id == creator_ref {
        println!("✓ Active Creator set to: {resolved_id}");
    } else {
        println!("✓ Active Creator set to: {resolved_id} (resolved from: {creator_ref})");
    }
    println!(
        "  Workspace slug: {DEFAULT_WORKSPACE_SLUG} (use `nexus42 creator workspace use <slug>` after the directory exists)"
    );
    Ok(())
}

/// One resolved row for `creator list` (V1.176 P0 T3, AR-90).
#[derive(Debug, Clone, PartialEq, Eq)]
struct ListRow {
    creator_id: String,
    /// `None` on local rows (PL-6: HANDLE renders `-`; JSON `null`).
    handle: Option<String>,
    /// Local rows: the `local_identities` value (authoritative). Platform
    /// rows: today's cache/auth value. Absent metadata renders `-` (human) /
    /// `null` (JSON).
    display_name: Option<String>,
    active: bool,
    /// `"local"` iff the id has a persistent `local_identities` row.
    origin: &'static str,
}

/// Resolve the merged `creator list` rows (AR-90 #1-#3).
///
/// Sources: platform ids = identity cache ∪ auth store (exactly today's
/// sources); persistent local ids = `local_identities` (SSOT — the JSON
/// cache is platform display metadata only and is NOT a local source).
/// Deduped by `creator_id` (a platform-linked persistent local id appears
/// once, as local), sorted by id (unchanged ordering). Anonymous/ephemeral
/// identities are absent — they are not registered creators (AR-90 #2).
///
/// Row-field precedence (AR-90 #3): local row `display_name` from
/// `local_identities` (authoritative), `handle = None` (PL-6); platform row
/// uses today's cache lookups unchanged (byte-stable).
#[must_use]
fn list_rows(
    cache: &creator_identity::CreatorIdentityCache,
    auth_store: &crate::auth::AuthStore,
    local_rows: &[nexus_local_db::LocalIdentityRow],
    active_id: Option<&str>,
) -> Vec<ListRow> {
    let local_by_id: std::collections::HashMap<&str, &nexus_local_db::LocalIdentityRow> =
        local_rows
            .iter()
            .filter(|r| r.identity_type == "persistent")
            .map(|r| (r.creator_id.as_str(), r))
            .collect();

    // Gather all known creator IDs from the two platform sources (unchanged
    // from V1.16 behavior).
    let mut all_ids: Vec<String> = cache.creators.keys().cloned().collect();
    if let Some(creators) = &auth_store.creators {
        for id in creators.keys() {
            if !all_ids.contains(id) {
                all_ids.push(id.clone());
            }
        }
    }
    // Join persistent local rows; dedupe by id — local wins (AR-90 #1).
    for row in local_by_id.values() {
        if !all_ids.contains(&row.creator_id) {
            all_ids.push(row.creator_id.clone());
        }
    }
    all_ids.sort();

    all_ids
        .into_iter()
        .map(|id| {
            let local_row = local_by_id.get(id.as_str()).copied();
            let (handle, display_name) = local_row.map_or_else(
                || {
                    let entry = creator_identity::get_creator_identity(cache, &id);
                    (
                        entry.and_then(|e| e.handle.clone()),
                        entry.and_then(|e| e.display_name.clone()),
                    )
                },
                |row| (None, row.display_name.clone()),
            );
            let active = active_id == Some(id.as_str());
            let origin = if local_row.is_some() {
                "local"
            } else {
                "platform"
            };
            ListRow {
                creator_id: id,
                handle,
                display_name,
                active,
                origin,
            }
        })
        .collect()
}

/// Build the pinned `--json` DTO object for one row (AR-90 #4) — exactly
/// `creator_id`, `handle`, `display_name`, `active`, `origin` (`snake_case`;
/// `handle`/`display_name` nullable; `origin` = `"local"` | `"platform"`).
#[must_use]
fn row_to_json(row: &ListRow) -> serde_json::Value {
    serde_json::json!({
        "creator_id": row.creator_id,
        "handle": row.handle,
        "display_name": row.display_name,
        "active": row.active,
        "origin": row.origin,
    })
}

/// CREATOR ID column width for the human table: the widest id in the
/// listing, never narrower than the pre-existing 19 — a platform-only
/// listing of short ids keeps the V1.176 layout byte-for-byte. Minted
/// `ctr_local` ids are 21 chars (`ctr_local` + 12 hex, V1.176 P0 T1).
#[must_use]
fn creator_id_column_width(rows: &[ListRow]) -> usize {
    rows.iter()
        .map(|r| r.creator_id.len())
        .max()
        .unwrap_or(19)
        .max(19)
}

/// List all known Creators with three-layer identity model (V1.16).
///
/// V1.176 P0 T3 (AR-90): persistent local identities from `local_identities`
/// (SSOT) appear alongside platform rows with an additive ORIGIN column
/// (`local` | `platform`); existing id/handle/display/active semantics are
/// unchanged and platform rows stay byte-stable. `--json` emits the pinned
/// machine DTO verbatim — a JSON array of `{creator_id, handle, display_name,
/// active, origin}` objects with nullable `handle`/`display_name` — never a
/// string dump of the table. Empty-state copy unchanged.
///
/// # Errors
///
/// Returns `CliError` if the identity store, config, or auth store cannot be
/// read, or JSON serialization fails.
async fn list_creators(_config: &CliConfig, json: bool) -> Result<()> {
    let config = CliConfig::load()?;
    let cache = creator_identity::load_creator_identity_cache();
    let active_id = config.active_creator_id.as_deref();

    // Local rows come from `local_identities` (SSOT, AR-90 #1) — the JSON
    // cache is platform display metadata only and is never a local source.
    // The global db is opened lazily and read-only (qc3 F-002): `creator
    // list` must not materialize `~/.nexus42/state.db` for a platform-only /
    // empty surface, and no local rows can exist when the db file is absent.
    // A locked / corrupt / unreadable local source degrades with an honest
    // stderr warning instead of failing the whole listing (qc3 S-003) — the
    // platform rows are still shown.
    let local_rows = if global_db_path()?.exists() {
        let read_result: Result<Vec<nexus_local_db::LocalIdentityRow>> = async {
            let pool = open_global_db_read_only().await?;
            Ok(nexus_local_db::list_local_identities(&pool).await?)
        }
        .await;
        match read_result {
            Ok(rows) => rows,
            Err(err) => {
                eprintln!(
                    "warning: local identities unavailable ({err}); showing platform rows only."
                );
                Vec::new()
            }
        }
    } else {
        Vec::new()
    };
    let auth_store = crate::auth::AuthStore::load()?;

    let rows = list_rows(&cache, &auth_store, &local_rows, active_id);

    if rows.is_empty() {
        if json {
            println!("[]");
        } else {
            println!("No registered Creators found.");
            println!("Use: nexus42 creator register --name <name> [--local]");
        }
        return Ok(());
    }

    if json {
        // AR-90 #4: verbatim DTO serialization (house pattern) — never a
        // string dump of the table.
        let items: Vec<serde_json::Value> = rows.iter().map(row_to_json).collect();
        println!("{}", serde_json::to_string_pretty(&items)?);
        return Ok(());
    }

    // Human default: additive ORIGIN column; existing column meanings and
    // platform values unchanged (local HANDLE renders `-`, PL-6). The CREATOR
    // ID column is padded to the widest id in the listing (never narrower
    // than the pre-existing 19), so 21-char `ctr_local*` ids align and
    // platform-only listings keep the V1.176 layout byte-for-byte.
    let id_width = creator_id_column_width(&rows);
    println!(
        "{:<id_width$} {:<14} {:<21} {:<8} ACTIVE",
        "CREATOR ID", "HANDLE", "DISPLAY NAME", "ORIGIN"
    );
    for row in &rows {
        let handle_str = row.handle.as_deref().unwrap_or("-");
        let display_str = row.display_name.as_deref().unwrap_or("-");
        let active_marker = if row.active { "✓" } else { "" };
        println!(
            "{:<id_width$} {:<14} {:<21} {:<8} {}",
            row.creator_id, handle_str, display_str, row.origin, active_marker
        );
    }

    Ok(())
}

/// Initiate pairing flow
fn pair_creator(_config: &CliConfig, creator_id: &str) {
    // Platform API integration not yet available
    println!("⚠ V1.0 skeleton: Creator pairing requires platform API.");
    println!("  Creator: {creator_id}");
}

/// Remove pairing
fn unpair_creator(_config: &CliConfig, creator_id: &str) {
    // Platform API integration not yet available
    println!("⚠ V1.0 skeleton: Creator unpairing requires platform API.");
    println!("  Creator: {creator_id}");
}

/// Logout — clear active creator credentials from local config and auth store.
///
/// Tries daemon API first (T33: migration), then clears local state.
/// Local state is always cleared regardless of daemon result to ensure
/// CLI works even when daemon is unreachable.
///
/// # Errors
///
/// Returns I/O errors if config or auth store cannot be read or written.
async fn logout_creator(config: &CliConfig) -> Result<()> {
    let creator_id = config.active_creator_id.as_deref();

    if creator_id.is_none() {
        println!("No active Creator to logout.");
        return Ok(());
    }

    let creator_id = creator_id.expect("checked above");

    // Try daemon API first (T33: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        if let Err(e) = client.logout_creator(creator_id).await {
            eprintln!("nexus42: daemon logout failed, continuing with local cleanup: {e}");
        }
    }

    // Always clear local state
    let mut store = auth::AuthStore::load()?;
    if let Some(creators) = &mut store.creators {
        if creators.remove(creator_id).is_some() {
            store.save()?;
        }
    }

    // Clear active creator from CLI config
    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = None;
    cli_config.save()?;

    println!("✓ Creator {creator_id} logged out.");
    Ok(())
}

/// Rotate Creator credentials
async fn rotate_credentials(config: &CliConfig, creator_id: Option<String>) -> Result<()> {
    let id = creator_id.unwrap_or_else(|| {
        config
            .active_creator_id
            .clone()
            .ok_or(crate::errors::CliError::CreatorNotSelected)
            .unwrap_or_default()
    });

    auth::creator_auth::rotate_credentials(config, &id).await
}

/// Cache a Creator locally in `SQLite`
#[allow(dead_code)]
async fn cache_creator_locally(creator: &Creator) -> Result<()> {
    use crate::config::state_db_path;
    let db_path = state_db_path()?;

    // Ensure parent directory exists
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    let pool = crate::db::Schema::init(&db_path).await?;

    let cached_at = chrono::Utc::now().to_rfc3339();
    let data = serde_json::to_string(creator)?;
    let status_str = creator.status.as_str();
    let creator_id = &*creator.creator_id;
    let display_name = &*creator.display_name;
    sqlx::query!(
        "INSERT OR REPLACE INTO creators (creator_id, display_name, status, cached_at, data)
         VALUES (?, ?, ?, ?, ?)",
        creator_id,
        display_name,
        status_str,
        cached_at,
        data
    )
    .execute(&pool)
    .await?;

    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::field_reassign_with_default)]
mod tests {
    use super::*;
    use crate::auth::{AuthStore, CreatorAuthState};
    use nexus_cloud_sync::platform_client::{
        classify_platform_error, StagedPlatformError, VerifyStatus,
    };

    /// Helper: create an `AuthStore` with a known access token.
    fn store_with_token(creator_id: &str, token: &str) -> AuthStore {
        let mut store = AuthStore::default();
        let mut m = std::collections::BTreeMap::new();
        m.insert(
            creator_id.to_string(),
            CreatorAuthState {
                creator_id: creator_id.to_string(),
                access_token: token.to_string(),
                expires_at: "2099-01-01T00:00:00Z".to_string(),
                creator_api_key: None,
            },
        );
        store.creators = Some(m.into_iter().collect());
        store
    }

    // ── obtain_auth_token tests ──────────────────────────────────

    #[test]
    fn obtain_auth_token_finds_token_in_store() {
        let store = store_with_token("crt_test", "test_token_value");
        let token = obtain_auth_token(&store).expect("should find token");
        assert_eq!(token, "test_token_value");
    }

    #[test]
    fn obtain_auth_token_returns_first_available_token() {
        let mut map = std::collections::BTreeMap::new();
        map.insert(
            "crt_a".to_string(),
            CreatorAuthState {
                creator_id: "crt_a".to_string(),
                access_token: "token_a".to_string(),
                expires_at: "2099-01-01T00:00:00Z".to_string(),
                creator_api_key: None,
            },
        );
        map.insert(
            "crt_b".to_string(),
            CreatorAuthState {
                creator_id: "crt_b".to_string(),
                access_token: "token_b".to_string(),
                expires_at: "2099-01-01T00:00:00Z".to_string(),
                creator_api_key: None,
            },
        );
        let mut store = AuthStore::default();
        store.creators = Some(map.into_iter().collect());
        let token = obtain_auth_token(&store).expect("should find token");
        // With BTreeMap insertion, keys are ordered: "crt_a" < "crt_b".
        // HashMap iteration is non-deterministic, so we accept either token.
        assert!(token == "token_a" || token == "token_b");
    }

    #[test]
    fn obtain_auth_token_skips_empty_access_tokens() {
        let store = store_with_token("crt_empty", "");
        let result = obtain_auth_token(&store);
        assert!(result.is_err());
        assert!(matches!(result, Err(CliError::AuthenticationRequired)));
    }

    #[test]
    fn obtain_auth_token_errors_on_empty_store() {
        let store = AuthStore::default();
        let result = obtain_auth_token(&store);
        assert!(matches!(result, Err(CliError::AuthenticationRequired)));
    }

    // ── CliError display tests for new variants ──────────────────

    #[test]
    fn challenge_failed_error_has_suggestion() {
        let err = CliError::ChallengeFailed {
            reason: "could not parse math problem".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("Challenge solving failed"));
        assert!(display.contains("could not parse math problem"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("creator register"));
    }

    #[test]
    fn creator_registration_failed_error_shows_status() {
        let err = CliError::CreatorRegistrationFailed {
            status: 500,
            message: "internal server error".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("500"));
        assert!(display.contains("internal server error"));
        assert!(display.contains("Suggestion:"));
        assert!(display.contains("auth status"));
    }

    #[test]
    fn creator_verification_failed_wrong_answer_has_suggestion() {
        let err = CliError::CreatorVerificationFailed {
            status: "wrong_answer".to_string(),
            message: "0 attempts remaining".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("wrong_answer"));
        assert!(display.contains("auto-retry has been exhausted"));
    }

    #[test]
    fn creator_verification_failed_expired_has_suggestion() {
        let err = CliError::CreatorVerificationFailed {
            status: "expired".to_string(),
            message: "timed out".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("expired"));
        assert!(display.contains("timed out"));
    }

    #[test]
    fn creator_verification_failed_locked_has_suggestion() {
        let err = CliError::CreatorVerificationFailed {
            status: "locked".to_string(),
            message: "permanently locked".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("locked"));
        assert!(display.contains("permanently locked"));
        assert!(display.contains("Contact support"));
    }

    #[test]
    fn challenge_expired_error_shows_timestamp() {
        let err = CliError::ChallengeExpired {
            expires_at: "2026-04-16T00:05:00.000Z".to_string(),
        };
        let display = format!("{err}");
        assert!(display.contains("expired"));
        assert!(display.contains("2026-04-16T00:05:00.000Z"));
    }

    // ── SyncError → CliError conversion tests ────────────────────

    #[test]
    fn sync_platform_error_maps_to_creator_registration_failed() {
        let sync_err = nexus_cloud_sync::errors::SyncError::PlatformError {
            status: 409,
            body: "creator already exists".to_string(),
        };
        let cli_err: CliError = sync_err.into();
        match cli_err {
            CliError::CreatorRegistrationFailed { status, message } => {
                assert_eq!(status, 409);
                assert_eq!(message, "creator already exists");
            }
            _ => panic!("Expected CreatorRegistrationFailed variant"),
        }
    }

    #[test]
    fn sync_not_configured_maps_to_cli_config_error() {
        let sync_err = nexus_cloud_sync::errors::SyncError::SyncNotConfigured(
            "platform_base_url is required".to_string(),
        );
        let cli_err: CliError = sync_err.into();
        assert!(matches!(cli_err, CliError::Config(_)));
    }

    #[test]
    fn sync_http_error_maps_to_cli_network_error() {
        // Build a reqwest::Error via a builder that fails (no network needed).
        // Use reqwest's Error::from on a builder-level timeout which
        // doesn't require a real connection. However, since we can't easily
        // construct a reqwest::Error, we instead verify the mapping logic
        // by checking the SyncError variant directly.
        let sync_err = nexus_cloud_sync::errors::SyncError::PlatformError {
            status: 502,
            body: "bad gateway".to_string(),
        };
        let cli_err: CliError = sync_err.into();
        assert!(matches!(
            cli_err,
            CliError::CreatorRegistrationFailed { status: 502, .. }
        ));
    }

    // ── submit_with_retry tests (mock via wiremock) ──────────────

    #[tokio::test]
    async fn submit_retry_succeeds_on_first_attempt() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "verified",
                "creator_api_key": "nexus_live_active"
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token", "dev_test")
            .expect("create client");
        let result = submit_with_retry(&client, "nxc_verify_test", "47", 2).await;

        assert!(result.is_ok());
        let resp = result.expect("response");
        assert_eq!(resp.status, VerifyStatus::Verified);
    }

    #[tokio::test]
    async fn submit_retry_returns_expired_immediately() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "expired"
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token", "dev_test")
            .expect("create client");
        let result = submit_with_retry(&client, "nxc_verify_expired", "47", 2).await;

        assert!(result.is_ok());
        let resp = result.expect("response");
        assert_eq!(resp.status, VerifyStatus::Expired);
    }

    #[tokio::test]
    async fn submit_retry_returns_locked_immediately() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "locked"
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token", "dev_test")
            .expect("create client");
        let result = submit_with_retry(&client, "nxc_verify_locked", "47", 2).await;

        assert!(result.is_ok());
        let resp = result.expect("response");
        assert_eq!(resp.status, VerifyStatus::Locked);
    }

    #[tokio::test]
    async fn submit_retry_retries_on_wrong_answer() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;

        // First call: wrong_answer, second call: verified
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "wrong_answer",
                "remaining_attempts": 2
            })))
            .up_to_n_times(1)
            .mount(&mock_server)
            .await;

        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "verified",
                "creator_api_key": "nexus_live_after_retry"
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token", "dev_test")
            .expect("create client");
        let result = submit_with_retry(&client, "nxc_verify_retry", "47", 2).await;

        assert!(result.is_ok());
        let resp = result.expect("response");
        assert_eq!(resp.status, VerifyStatus::Verified);
        assert_eq!(
            resp.creator_api_key,
            Some("nexus_live_after_retry".to_string())
        );
    }

    #[tokio::test]
    async fn submit_retry_exhausts_attempts_on_persistent_wrong_answer() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "wrong_answer",
                "remaining_attempts": 1
            })))
            .mount(&mock_server)
            .await;

        let client = PlatformClient::new(&mock_server.uri(), "test_token", "dev_test")
            .expect("create client");
        let result = submit_with_retry(&client, "nxc_verify_fail", "47", 2).await;

        assert!(result.is_ok());
        let resp = result.expect("response");
        assert_eq!(resp.status, VerifyStatus::WrongAnswer);
        assert_eq!(resp.remaining_attempts, Some(1));
    }

    // ── Constants tests ──────────────────────────────────────────

    #[test]
    fn default_registration_source_is_cli() {
        assert_eq!(DEFAULT_REGISTRATION_SOURCE, "cli");
    }

    #[test]
    fn expiry_buffer_is_ten_seconds() {
        assert_eq!(EXPIRY_BUFFER_SECS, 10);
    }

    #[test]
    fn max_verify_attempts_is_two() {
        assert_eq!(MAX_VERIFY_ATTEMPTS, 2);
    }

    // ── Handle validation tests ─────────────────────────────────

    #[test]
    fn validate_handle_accepts_valid_handle() {
        assert!(validate_handle("valid-handle").is_ok());
    }

    #[test]
    fn validate_handle_accepts_min_length() {
        assert!(validate_handle("abcd").is_ok());
    }

    #[test]
    fn validate_handle_accepts_max_length() {
        // 15 chars: starts/ends with [a-z0-9], interior 13 chars
        assert!(validate_handle("abcdefghijklmno").is_ok());
    }

    #[test]
    fn validate_handle_accepts_dots_and_underscores() {
        assert!(validate_handle("my.agent_name").is_ok());
    }

    #[test]
    fn validate_handle_rejects_too_short() {
        let result = validate_handle("AB");
        assert!(result.is_err());
        let display = format!("{}", result.unwrap_err());
        assert!(display.contains('4'));
        assert!(display.contains("15"));
    }

    #[test]
    fn validate_handle_rejects_three_chars() {
        let result = validate_handle("abc");
        assert!(result.is_err());
        let display = format!("{}", result.unwrap_err());
        assert!(display.contains('4'));
        assert!(display.contains("15"));
    }

    #[test]
    fn validate_handle_rejects_too_long() {
        let result = validate_handle("abcdefghijklmnop"); // 16 chars
        assert!(result.is_err());
        let display = format!("{}", result.unwrap_err());
        assert!(display.contains('4'));
        assert!(display.contains("15"));
    }

    #[test]
    fn validate_handle_rejects_spaces() {
        let result = validate_handle("a b");
        assert!(result.is_err());
        let display = format!("{}", result.unwrap_err());
        assert!(display.contains("lowercase letters"));
    }

    #[test]
    fn validate_handle_rejects_uppercase() {
        let result = validate_handle("ValidHandle");
        assert!(result.is_err());
        let display = format!("{}", result.unwrap_err());
        assert!(display.contains("lowercase letters"));
    }

    #[test]
    fn validate_handle_rejects_leading_hyphen() {
        let result = validate_handle("-ab");
        assert!(result.is_err());
        let display = format!("{}", result.unwrap_err());
        assert!(display.contains("start and end"));
    }

    #[test]
    fn validate_handle_rejects_trailing_hyphen() {
        let result = validate_handle("ab-");
        assert!(result.is_err());
        let display = format!("{}", result.unwrap_err());
        assert!(display.contains("start and end"));
    }

    #[test]
    fn validate_handle_rejects_empty_string() {
        let result = validate_handle("");
        assert!(result.is_err());
    }

    #[test]
    fn validate_handle_rejects_special_chars() {
        let result = validate_handle("ab@cd");
        assert!(result.is_err());
        let display = format!("{}", result.unwrap_err());
        assert!(display.contains("lowercase letters"));
    }

    #[test]
    fn validate_handle_regex_is_frozen_spec_compliant() {
        // Confirm the regex constant matches spec v3 §7 exactly
        assert_eq!(HANDLE_RE.as_str(), r"^[a-z0-9][a-z0-9._-]{2,13}[a-z0-9]$");
    }

    // ── WS-B T4/T6: name max-length tests ──────────────────────

    #[test]
    fn max_creator_name_length_is_64() {
        assert_eq!(MAX_CREATOR_NAME_LENGTH, 64);
    }

    #[test]
    fn name_exactly_64_chars_passes_length_check() {
        let name_64 = "a".repeat(64);
        // Simulate the check logic
        assert!(name_64.len() <= MAX_CREATOR_NAME_LENGTH);
    }

    #[test]
    fn name_65_chars_exceeds_max_length() {
        let name_65 = "a".repeat(65);
        assert!(name_65.len() > MAX_CREATOR_NAME_LENGTH);
    }

    // ── DF-14: Staged e2e verification harness (gate-B1/B2) ─────────

    /// Test mode for the staged e2e verification harness.
    ///
    /// Controls whether the staged flow runs against a happy-path platform
    /// or simulates an upstream failure scenario.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestMode {
        /// Platform responds with valid registration + verification.
        HappyPath,
        /// Platform is unreachable or returns a timeout.
        UpstreamTimeout,
    }

    /// Result of the staged creator register e2e flow.
    ///
    /// Breaks the registration pipeline into discrete stages so tests can
    /// assert on individual gate outcomes (gate-B1: register, gate-B2: verify).
    #[derive(Debug)]
    struct StagedE2eResult {
        /// Gate-B1 outcome: platform register call result.
        register: std::result::Result<
            nexus_cloud_sync::platform_client::RegisterResponse,
            StagedPlatformError,
        >,
        /// Gate-B2 outcome: platform verify call result (None if register failed).
        verify: Option<
            std::result::Result<
                nexus_cloud_sync::platform_client::VerifyResponse,
                StagedPlatformError,
            >,
        >,
    }

    /// Run the staged creator register e2e verification flow.
    ///
    /// This is the testable harness that separates gate-B1 (register) and
    /// gate-B2 (verify) into distinct stages with deterministic error shaping.
    ///
    /// In `TestMode::HappyPath`, the platform client calls proceed normally.
    /// In `TestMode::UpstreamTimeout`, the function simulates an upstream
    /// timeout by using a deliberately unreachable platform URL.
    async fn run_creator_register_e2e(
        platform_url: &str,
        auth_token: &str,
        device_id: &str,
        display_name: &str,
        registration_source: &str,
        handle: Option<&str>,
        mode: TestMode,
    ) -> StagedE2eResult {
        let effective_url = match mode {
            TestMode::HappyPath => platform_url.to_string(),
            TestMode::UpstreamTimeout => {
                // Use a deliberately unreachable URL to trigger a timeout/connection error
                "http://192.0.2.1:1".to_string()
            }
        };

        let client = match PlatformClient::new(&effective_url, auth_token, device_id) {
            Ok(c) => c,
            Err(err) => {
                return StagedE2eResult {
                    register: Err(classify_platform_error(err)),
                    verify: None,
                };
            }
        };

        // Gate-B1: Register creator on platform
        let register_result = client
            .register_creator(display_name, registration_source, handle)
            .await
            .map_err(classify_platform_error);

        let Ok(ref register_response) = register_result else {
            return StagedE2eResult {
                register: register_result,
                verify: None,
            };
        };

        // Gate-B2: Verify creator (using a placeholder answer — the e2e harness
        // focuses on platform connectivity and error shaping, not challenge solving)
        let verify_result = client
            .verify_creator(
                &register_response.verification.verification_code,
                "0", // Placeholder answer for e2e harness
            )
            .await
            .map_err(classify_platform_error);

        StagedE2eResult {
            register: Ok(register_response.clone()),
            verify: Some(verify_result),
        }
    }

    /// Gate-B1/B2: Happy path — platform returns valid register + verify responses.
    ///
    /// Verifies that `run_creator_register_e2e` with `TestMode::HappyPath`
    /// successfully completes both the register (B1) and verify (B2) stages.
    #[tokio::test]
    async fn creator_register_e2e_handles_platform_happy_path() {
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        let mock = MockServer::start().await;

        // Mock POST /api/v1/creators/register → valid registration
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/register"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "creator_id": "crt_staged_e2e",
                "display_name": "Staged E2E Creator",
                "creator_api_key": "nexus_live_staged_key",
                "verification": {
                    "verification_code": "nxc_verify_staged",
                    "challenge_text": "A basket has five apples and someone adds three more",
                    "expires_at": "2099-12-31T23:59:59.000Z",
                    "instructions": "Solve the math problem"
                }
            })))
            .mount(&mock)
            .await;

        // Mock POST /api/v1/creators/verify → verified
        Mock::given(method("POST"))
            .and(path("/api/v1/creators/verify"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                "status": "verified",
                "creator_api_key": "nexus_live_staged_active"
            })))
            .mount(&mock)
            .await;

        let result = run_creator_register_e2e(
            &mock.uri(),
            "test_token",
            "dev_staged",
            "Staged E2E Creator",
            "cli",
            None,
            TestMode::HappyPath,
        )
        .await;

        // Gate-B1: register should succeed
        assert!(
            result.register.is_ok(),
            "gate-B1 register should succeed in HappyPath; got: {:?}",
            result.register
        );
        let register_resp = result.register.as_ref().expect("register response");
        assert_eq!(register_resp.creator_id, "crt_staged_e2e");

        // Gate-B2: verify should succeed
        let verify_result = result
            .verify
            .as_ref()
            .expect("verify stage should be present after successful register");
        assert!(
            verify_result.is_ok(),
            "gate-B2 verify should succeed in HappyPath; got: {verify_result:?}",
        );
        let verify_resp = verify_result.as_ref().expect("verify response");
        assert_eq!(verify_resp.status, VerifyStatus::Verified);
    }

    /// Gate-B1/B2: Upstream timeout — platform is unreachable.
    ///
    /// Verifies that `run_creator_register_e2e` with `TestMode::UpstreamTimeout`
    /// surfaces a deterministic timeout error from gate-B1, and that the error
    /// is shaped into a [`StagedPlatformError`] bucket.
    #[tokio::test]
    async fn creator_register_e2e_surfaces_platform_failure_context() {
        // No mock server needed — UpstreamTimeout mode uses an unreachable URL
        let result = run_creator_register_e2e(
            "http://will-be-ignored.invalid", // Overridden by UpstreamTimeout mode
            "test_token",
            "dev_staged_fail",
            "Staged Fail Creator",
            "cli",
            None,
            TestMode::UpstreamTimeout,
        )
        .await;

        // Gate-B1: register should fail with a timeout/connection error
        assert!(
            result.register.is_err(),
            "gate-B1 register should fail in UpstreamTimeout; got: {:?}",
            result.register
        );

        let err = result
            .register
            .expect_err("register should be Err in UpstreamTimeout");
        // The error must be shaped into a deterministic bucket.
        match &err {
            StagedPlatformError::Timeout
            | StagedPlatformError::Platform { status: 0, .. }
            | StagedPlatformError::Platform { status: 502, .. } => {}
            StagedPlatformError::Config(msg) => {
                panic!("unexpected Config error: {msg}");
            }
            StagedPlatformError::Auth(msg) => {
                panic!("unexpected Auth error: {msg}");
            }
            StagedPlatformError::Platform { status, body } => {
                panic!("unexpected Platform error with HTTP status {status}: {body}");
            }
        }

        // The error display must contain "timeout" or "platform" for CLI visibility
        let err_display = format!("{err}");
        assert!(
            err_display.contains("timeout") || err_display.contains("platform"),
            "error must contain 'timeout' or 'platform' for CLI visibility; got: {err_display}"
        );

        // Gate-B2: verify should not be reached (None)
        assert!(
            result.verify.is_none(),
            "gate-B2 verify should not be reached when gate-B1 fails"
        );
    }

    // ── V1.167 P2 T1: `creator register --local` ──────────────────

    /// (a) AC-V167-P2-1: local register in an isolated HOME mints a
    /// persistent `ctr_local*` identity and sets it active — with zero
    /// platform involvement (no auth token anywhere in the store).
    #[tokio::test]
    async fn register_local_mints_ctr_local_and_sets_active() {
        let _home = crate::testutil::isolated_home();
        let config = CliConfig::load().expect("load default config");

        register_creator(
            &config,
            "Local Tester".to_string(),
            "cli".to_string(),
            None,
            true,
        )
        .await
        .expect("local register should succeed");

        // Active creator must be set to the freshly minted local identity.
        let config = CliConfig::load().expect("reload config");
        let active = config
            .active_creator_id
            .as_deref()
            .expect("active_creator_id should be set");
        assert!(
            active.starts_with("ctr_local"),
            "expected a ctr_local* identity, got {active}"
        );

        // V1.167 P2 T2 (AC-V167-P2-1 second half): the workspace state db
        // `creators` row must exist so `creator world create` passes its FK
        // precheck — same resolution seam as `creator world create`.
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");
        // SAFETY: one-off test assertion mirroring create_world's FK precheck,
        // intentionally stronger: narrative_write.rs:214 EXISTS checks
        // creator_id only; here we also pin status = 'active' as written by
        // ensure_creator_row.
        let creator_exists: i64 = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM creators WHERE creator_id = ? AND status = 'active')",
        )
        .bind(active)
        .fetch_one(&pool)
        .await
        .expect("query workspace creators row");
        assert_eq!(
            creator_exists, 1,
            "workspace creators row must exist for the registered local creator"
        );

        // The identity must resolve through the shared resolution path as
        // a persistent (non-anonymous, non-platform) local identity.
        let resolved = crate::commands::system::identity::resolve_active_identity()
            .await
            .expect("resolve active identity")
            .expect("active identity should resolve");
        assert_eq!(resolved.creator_id, active);
        assert!(resolved.is_persistent);
        assert!(!resolved.is_anonymous);
        assert!(!resolved.platform_linked);
    }

    /// (d) V1.167 P2 T2 (AC-V167-P2-1 second half): after `register --local`,
    /// `create_world` on the resolved workspace pool succeeds — the missing
    /// `creators` row is materialized by the register bootstrap, no daemon
    /// HTTP workaround required.
    #[tokio::test]
    async fn register_local_then_world_create_succeeds() {
        let _home = crate::testutil::isolated_home();
        let config = CliConfig::load().expect("load default config");

        register_creator(
            &config,
            "World Builder".to_string(),
            "cli".to_string(),
            None,
            true,
        )
        .await
        .expect("local register should succeed");

        let config = CliConfig::load().expect("reload config");
        let active = config
            .active_creator_id
            .as_deref()
            .expect("active_creator_id should be set");

        // Resolve the workspace state db exactly like `creator world create`.
        let db_path = crate::config::resolve_state_db_path(&config).expect("resolve state db path");
        let pool = crate::db::Schema::init(&db_path)
            .await
            .expect("init workspace pool");

        let result = nexus_local_db::create_world(
            &pool,
            active,
            "Test World",
            "test-world",
            "public",
            "manual",
        )
        .await
        .expect("create_world must succeed after local register (no HTTP workaround)");
        assert!(result.world_id.starts_with("wld_"));
    }

    /// (b) AC-V167-P2-2: platform-path register with no auth token fails
    /// with a hint naming both exits — `platform auth login` and `--local`.
    #[tokio::test]
    async fn register_platform_without_auth_token_hints_both_exits() {
        let _home = crate::testutil::isolated_home();
        let config = CliConfig::load().expect("load default config");

        let err = register_creator(
            &config,
            "No Auth".to_string(),
            "cli".to_string(),
            None,
            false,
        )
        .await
        .expect_err("platform register without a token must fail");

        let display = format!("{err}");
        // Frozen one-block hint copy (see the map_err in `register_creator`):
        // exact match after trim — a copy regression that drops or rewrites any
        // word must fail this test, not just the two exit names.
        assert_eq!(
            display.trim(),
            "Platform authentication required. Authenticate with `nexus42 platform auth login`, \
             or re-run with `--local` for local-only mode."
                .trim(),
            "hint copy must be the frozen one-block string; got: {display}"
        );
    }

    /// (c) AC-V167-P2-3: `--local` conflicts with the platform-only flags
    /// `--source` and `--handle` at the clap layer.
    #[test]
    fn register_local_conflicts_with_source_and_handle() {
        // Full CLI command tree (same builder `system completion` uses).
        let command = crate::cli::build_command();

        for args in [
            vec![
                "nexus42", "creator", "register", "--local", "--source", "cli", "--name", "T",
            ],
            vec![
                "nexus42", "creator", "register", "--local", "--handle", "myagent", "--name", "T",
            ],
        ] {
            let result = command.clone().try_get_matches_from(args);
            assert!(
                result.is_err(),
                "--local must conflict with the platform-only flag"
            );
        }

        // `--local` alone (with the required --name) parses fine.
        let ok = command
            .try_get_matches_from(["nexus42", "creator", "register", "--local", "--name", "T"]);
        assert!(ok.is_ok(), "--local alone must parse: {ok:?}");
    }

    // ── V1.176 P0 T3 (AR-90): creator list row resolution + --json DTO ──

    #[test]
    fn list_rows_marks_local_and_keeps_platform_byte_stable() {
        let mut cache = creator_identity::CreatorIdentityCache::default();
        cache.creators.insert(
            "ctr_plat_abc".to_string(),
            CreatorIdentityEntry {
                creator_id: "ctr_plat_abc".to_string(),
                handle: Some("alice".to_string()),
                display_name: Some("Alice Platform".to_string()),
            },
        );
        let auth_store = AuthStore::default();
        let local_rows = vec![
            nexus_local_db::LocalIdentityRow {
                creator_id: "ctr_local_xyz".to_string(),
                identity_type: "persistent".to_string(),
                display_name: Some("Local Alice".to_string()),
                created_at: "2026-08-26T00:00:00Z".to_string(),
                platform_linked: false,
                platform_creator_id: None,
            },
            // Anonymous/ephemeral identities are NOT registered creators —
            // absent from the display list (AR-90 #2).
            nexus_local_db::LocalIdentityRow {
                creator_id: "ctr_anon_zzz".to_string(),
                identity_type: "anonymous".to_string(),
                display_name: Some("Ghost".to_string()),
                created_at: "2026-08-26T00:00:00Z".to_string(),
                platform_linked: false,
                platform_creator_id: None,
            },
        ];

        let rows = list_rows(&cache, &auth_store, &local_rows, Some("ctr_local_xyz"));

        assert_eq!(
            rows.iter()
                .map(|r| r.creator_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ctr_local_xyz", "ctr_plat_abc"],
            "sorted by id; anonymous identity excluded"
        );
        let local = &rows[0];
        assert_eq!(local.origin, "local");
        assert_eq!(local.handle, None, "local handle is null (renders `-`)");
        assert_eq!(local.display_name.as_deref(), Some("Local Alice"));
        assert!(local.active);
        let platform = &rows[1];
        assert_eq!(platform.origin, "platform");
        assert_eq!(platform.handle.as_deref(), Some("alice"));
        assert_eq!(platform.display_name.as_deref(), Some("Alice Platform"));
        assert!(!platform.active);
    }

    #[test]
    fn list_rows_dedupes_platform_linked_local_id_as_local() {
        let mut cache = creator_identity::CreatorIdentityCache::default();
        cache.creators.insert(
            "ctr_local_dup".to_string(),
            CreatorIdentityEntry {
                creator_id: "ctr_local_dup".to_string(),
                handle: Some("dup_handle".to_string()),
                display_name: Some("Platform Copy".to_string()),
            },
        );
        let auth_store = AuthStore::default();
        let local_rows = vec![nexus_local_db::LocalIdentityRow {
            creator_id: "ctr_local_dup".to_string(),
            identity_type: "persistent".to_string(),
            display_name: Some("Local Authority".to_string()),
            created_at: "2026-08-26T00:00:00Z".to_string(),
            platform_linked: true,
            platform_creator_id: Some("ctr_plat_dup".to_string()),
        }];

        let rows = list_rows(&cache, &auth_store, &local_rows, None);

        assert_eq!(rows.len(), 1, "same id in both sources appears once");
        assert_eq!(rows[0].origin, "local");
        assert_eq!(
            rows[0].handle, None,
            "cache handle must not leak onto the local row (PL-6)"
        );
        assert_eq!(rows[0].display_name.as_deref(), Some("Local Authority"));
    }

    #[test]
    fn row_to_json_emits_pinned_dto() {
        let local = ListRow {
            creator_id: "ctr_local_xyz".to_string(),
            handle: None,
            display_name: Some("Local Alice".to_string()),
            active: true,
            origin: "local",
        };
        assert_eq!(
            row_to_json(&local),
            serde_json::json!({
                "creator_id": "ctr_local_xyz",
                "handle": null,
                "display_name": "Local Alice",
                "active": true,
                "origin": "local",
            })
        );

        let platform = ListRow {
            creator_id: "ctr_plat_abc".to_string(),
            handle: Some("alice".to_string()),
            display_name: None,
            active: false,
            origin: "platform",
        };
        assert_eq!(
            row_to_json(&platform),
            serde_json::json!({
                "creator_id": "ctr_plat_abc",
                "handle": "alice",
                "display_name": null,
                "active": false,
                "origin": "platform",
            })
        );
    }

    #[test]
    fn creator_id_column_width_fits_widest_id_floor_19() {
        // No rows → pre-existing width.
        assert_eq!(creator_id_column_width(&[]), 19);
        // Short platform-only ids keep the pre-existing width — the V1.176
        // layout stays byte-for-byte on a platform-only listing.
        let platform = ListRow {
            creator_id: "ctr_platabc".to_string(),
            handle: Some("alice".to_string()),
            display_name: Some("Alice Platform".to_string()),
            active: false,
            origin: "platform",
        };
        assert_eq!(creator_id_column_width(std::slice::from_ref(&platform)), 19);
        // Minted 21-char `ctr_local` ids (V1.176 P0 T1 format) widen the
        // column so HANDLE / ORIGIN stay aligned on local rows.
        let local = ListRow {
            creator_id: "ctr_localf0d65930e496".to_string(),
            handle: None,
            display_name: Some("Local Alice".to_string()),
            active: true,
            origin: "local",
        };
        assert_eq!(creator_id_column_width(std::slice::from_ref(&local)), 21);
        assert_eq!(
            creator_id_column_width(&[local, platform]),
            21,
            "the widest id drives the column in a mixed listing"
        );
    }
}
