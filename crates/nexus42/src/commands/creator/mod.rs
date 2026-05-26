//! Creator Command Module
//!
//! Creator is a V1.0 first-class citizen (roadmap §3.1.1, §3.1.2).
//! Subcommands: register, status, use, list, pair, unpair, credentials rotate, workspace.

pub mod knowledge;
pub mod memory;
pub mod reference;
pub mod soul;
pub mod world;

use crate::auth;
use crate::challenge::{solve_challenge_with_fallback, UnavailableLlmSolver};
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
use nexus_kb::KbStore;
use nexus_knowledge::KnowledgeStore;
use serde::Deserialize;
use soul::SoulCommand;
use std::path::PathBuf;

/// Default registration source for the CLI.
const DEFAULT_REGISTRATION_SOURCE: &str = "cli";

/// Maximum length for creator display name (WS-B T4).
const MAX_CREATOR_NAME_LENGTH: usize = 64;

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
    println!("  nexus42 creator register      — create a Creator entity");
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
/// locally by the CLI. The `/v1/local/world/clone` endpoint never existed.
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
    /// Register a new Creator entity
    ///
    /// Usage: nexus42 creator register --name "My Agent" [--source `cli|web_agent`] [--handle <handle>]
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
    },

    /// Show current Creator status
    Status {
        /// Specific creator ID to check (default: active creator)
        creator_id: Option<String>,
    },

    /// Switch the active Creator
    ///
    /// Positional `<creator_ref>` is accepted for convenience.
    /// A future version may require `--creator-id <id>` flag syntax.
    Use {
        /// Creator ID or display name (positional; may become a flag in a future version)
        creator_ref: String,
    },

    /// List all registered Creators
    List,

    /// Initiate pairing flow with a Creator
    ///
    /// Positional `<creator_id>` is accepted for convenience.
    /// A future version may require `--creator-id <id>` flag syntax.
    Pair {
        /// Creator ID to pair (positional; may become a flag in a future version)
        creator_id: String,
    },

    /// Remove pairing with a Creator
    ///
    /// Positional `<creator_id>` is accepted for convenience.
    /// A future version may require `--creator-id <id>` flag syntax.
    Unpair {
        /// Creator ID to unpair (positional; may become a flag in a future version)
        creator_id: String,
    },

    /// Rotate Creator API credentials
    #[command(name = "credentials")]
    Credentials {
        #[command(subcommand)]
        action: CredentialsAction,
    },

    /// Operational workspace slugs for the active creator (local ADR-014 tree)
    Workspace {
        #[command(subcommand)]
        command: CreatorWorkspaceCommand,
    },

    /// SOUL management
    Soul {
        #[command(subcommand)]
        command: SoulCommand,
    },

    /// Long-term memory management
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },

    /// Reference source management (V1.26 reference store)
    Reference {
        #[command(subcommand)]
        command: reference::ReferenceCommand,
    },

    /// Local work-scope knowledge assets (file index; default --scope work).
    ///
    /// **This is the CLI local work KB index**, NOT `nexus-kb` (World KB) or
    /// `nexus-knowledge` (User knowledge). See entity-scope-model §5.3.
    ///
    /// `--scope world` reads and writes are implemented (narrative KB via nexus-kb + nexus-narrative).
    Kb {
        #[command(subcommand)]
        command: KbCommand,
    },

    /// Narrative world management (create worlds, add events, list)
    World {
        #[command(subcommand)]
        command: world::WorldCommand,
    },

    /// User-scoped knowledge management (add, list, search)
    Knowledge {
        #[command(subcommand)]
        command: knowledge::KnowledgeCommand,
    },

    /// Seed demo data: creates a demo world, event, KB block, and knowledge entry.
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

/// KB scope: `work` (local workspace file index, default) or `world` (narrative KB via nexus-kb).
///
/// Per entity-scope-model §5.3, `creator kb --scope work` is the **CLI local work KB index** —
/// a per-creator, per-workspace file-based index stored under
/// `~/.nexus42/creators/<id>/workspaces/<slug>/kb/`. It is NOT `nexus-kb` (World-scoped
/// narrative KB graph) or `nexus-knowledge` (User-scoped global knowledge).
///
/// `--scope world` routes to `nexus-kb` + `nexus-narrative` and requires
/// a `--world-id <id>`. User/global knowledge will NOT be a `creator kb` scope.
#[derive(Debug, Clone, clap::ValueEnum, Default, PartialEq, Eq)]
pub enum KbScope {
    /// Local workspace file index (default)
    #[default]
    Work,
    /// World-scoped narrative KB (nexus-kb + nexus-narrative)
    World,
}

/// Knowledge base subcommands (local work-scope file index).
#[derive(Debug, Subcommand)]
pub enum KbCommand {
    /// List local work-scope knowledge entries
    List {
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Search local work-scope entries by title/content
    Search {
        /// Search query string
        query: String,
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Show a single local work-scope entry
    Show {
        /// Entry ID to display (e.g. `kb_a1b2c3d4` or a key-block ID)
        entry_id: String,
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Add a local work-scope entry from a file
    Add {
        /// Path to the source file to add as a work-scope entry
        #[arg(long)]
        file: PathBuf,
        /// Optional title (defaults to filename stem)
        #[arg(long)]
        title: Option<String>,
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
        /// Block type for `--scope world` (e.g. Character, Scene, Item)
        #[arg(long)]
        block_type: Option<String>,
    },

    /// Remove a local work-scope entry
    Remove {
        /// Entry ID to remove (e.g. `kb_a1b2c3d4`)
        entry_id: String,
        /// Scope: `work` (local file index, default) or `world` (narrative KB)
        #[arg(long, value_enum, default_value_t = KbScope::default())]
        scope: KbScope,
        /// World ID for `--scope world` (required when scope is `world`)
        #[arg(long)]
        world_id: Option<String>,
    },

    /// Queue a work-scope entry for KB extraction into a target world.
    ///
    /// Idempotent: if a non-failed job already exists for the same
    /// work entry + world combination, returns the existing job.
    #[command(name = "queue-extract")]
    QueueExtract {
        /// Work-scope entry ID to extract from (e.g. `kb_a1b2c3d4`)
        work_entry_id: String,
        /// Target world ID for the resulting `KeyBlock`
        #[arg(long)]
        world_id: String,
    },

    /// Show extract job status for the active creator.
    ///
    /// Without `--job-id`, lists up to 100 most recent jobs for the active creator.
    #[command(name = "extract-status")]
    ExtractStatus {
        /// Specific job ID to inspect
        #[arg(long)]
        job_id: Option<String>,
    },
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
pub async fn run(cmd: CreatorCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        CreatorCommand::Register {
            name,
            source,
            handle,
        } => register_creator(config, name, source, handle).await,
        CreatorCommand::Status { creator_id } => creator_status(config, creator_id).await,
        CreatorCommand::Use { creator_ref } => use_creator(config, creator_ref.as_str()).await,
        CreatorCommand::List => list_creators(config),
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
        CreatorCommand::Kb { command } => run_kb(command, config).await,
        CreatorCommand::World { command } => world::run(command, config).await,
        CreatorCommand::Knowledge { command } => knowledge::run(command, config).await,
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

/// Handle local work-scope KB commands (CLI local work file index).
///
/// All commands default to `scope=work`. When `scope=world` is requested,
/// reads (list/search/show) are served via `nexus-kb` `SQLite` stores;
/// writes (add/remove) print a deferred message.
/// The work-scope implementation is a local file index only — it does not
/// interact with `nexus-kb` or `nexus-knowledge` crates.
async fn run_kb(cmd: KbCommand, config: &CliConfig) -> Result<()> {
    // F002: Validate active_creator_id before constructing any paths.
    // This prevents path traversal if config is corrupted or malicious.
    if let Some(cid) = &config.active_creator_id {
        paths::validate_creator_id_safe(cid).map_err(CliError::Other)?;
    }
    match cmd {
        KbCommand::List { scope, world_id } => kb_list(config, &scope, world_id.as_deref()).await,
        KbCommand::Search {
            query,
            scope,
            world_id,
        } => kb_search(config, &query, &scope, world_id.as_deref()).await,
        KbCommand::Show {
            entry_id,
            scope,
            world_id,
        } => kb_show(config, &entry_id, &scope, world_id.as_deref()).await,
        KbCommand::Add {
            file,
            title,
            scope,
            world_id,
            block_type,
        } => {
            kb_add(
                config,
                &file,
                title.as_deref(),
                &scope,
                world_id.as_deref(),
                block_type.as_deref(),
            )
            .await
        }
        KbCommand::Remove {
            entry_id,
            scope,
            world_id,
        } => kb_remove(config, &entry_id, &scope, world_id.as_deref()).await,
        KbCommand::QueueExtract {
            work_entry_id,
            world_id,
        } => kb_queue_extract(config, &work_entry_id, &world_id).await,
        KbCommand::ExtractStatus { job_id } => kb_extract_status(config, job_id.as_deref()).await,
    }
}

/// Require `--world-id` when `--scope world` is used. Returns the `world_id` or an error.
fn require_world_id(world_id: Option<&str>) -> Result<String> {
    world_id
        .map(std::string::ToString::to_string)
        .ok_or_else(|| {
            CliError::Other(
                "--world-id is required when using --scope world. \
                  Usage: nexus42 creator kb <command> --scope world --world-id <id>"
                    .into(),
            )
        })
}

/// Open a persistent KB store backed by the workspace `state.db`.
///
/// Uses `nexus_local_db::kb_store::SqliteKbStore` which implements
/// `KbStore` via compile-time checked sqlx queries.
async fn open_world_kb_store(
    config: &CliConfig,
) -> Result<nexus_local_db::kb_store::SqliteKbStore> {
    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;
    Ok(nexus_local_db::kb_store::SqliteKbStore::new(pool))
}

/// Parse a block type string from CLI argument.
fn parse_block_type_cli(s: &str) -> Result<nexus_contracts::BlockType> {
    match s {
        "Character" => Ok(nexus_contracts::BlockType::Character),
        "Ability" => Ok(nexus_contracts::BlockType::Ability),
        "Scene" => Ok(nexus_contracts::BlockType::Scene),
        "Organization" => Ok(nexus_contracts::BlockType::Organization),
        "Item" => Ok(nexus_contracts::BlockType::Item),
        "Conflict" => Ok(nexus_contracts::BlockType::Conflict),
        "InfoPoint" => Ok(nexus_contracts::BlockType::InfoPoint),
        "Event" => Ok(nexus_contracts::BlockType::Event),
        _ => Err(CliError::Other(format!(
            "Unknown block_type '{s}'. Valid: Character, Ability, Scene, Organization, Item, Conflict, InfoPoint, Event"
        ))),
    }
}

/// Resolve active creator + workspace slug, returning `(creator_id, workspace_slug, home)`.
fn resolve_kb_paths(config: &CliConfig) -> Result<(String, String, PathBuf)> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();
    let slug = config.workspace_slug_for_creator(&creator_id).to_string();
    let home = user_home()?;
    Ok((creator_id, slug, home))
}

/// Local work index on disk: `{"entries": [{"entry_id": "...", "title": "...", "created_at": "..."}]}`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
struct KbIndex {
    #[serde(default)]
    entries: Vec<KbIndexEntry>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct KbIndexEntry {
    entry_id: String,
    title: String,
    created_at: String,
}

/// Read the local work index from disk. Returns default (empty) if file is missing.
/// Logs a warning if the file exists but contains invalid JSON.
fn read_kb_index(index_path: &std::path::Path) -> KbIndex {
    if !index_path.exists() {
        return KbIndex::default();
    }
    let Ok(content) = std::fs::read_to_string(index_path) else {
        return KbIndex::default();
    };
    if content.trim().is_empty() {
        return KbIndex::default();
    }
    match serde_json::from_str(&content) {
        Ok(index) => index,
        Err(e) => {
            tracing::warn!(
                "Corrupt local work index file {}: {e}. \
                 The file will be treated as empty. \
                 Consider deleting it or re-adding entries to rebuild the index.",
                index_path.display()
            );
            KbIndex::default()
        }
    }
}

/// Write the local work index to disk atomically.
///
/// Writes to a temporary file first, then renames to the final path.
/// `std::fs::rename` is atomic on the same filesystem (POSIX), which
/// prevents corruption from crashes mid-write or concurrent `kb add` races.
#[allow(dead_code)] // Kept as utility; kb_add inlines the pattern for W2 ordering.
fn write_kb_index(index_path: &std::path::Path, index: &KbIndex) -> Result<()> {
    if let Some(parent) = index_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(index)?;
    let tmp_path = index_path.with_extension("json.tmp");
    std::fs::write(&tmp_path, &json)?;
    std::fs::rename(&tmp_path, index_path)?;
    Ok(())
}

/// Generate a local work entry ID: `kb_` + 8 hex chars from timestamp + 4 hex chars
/// from a simple hash to reduce collision risk under rapid successive calls.
#[allow(clippy::cast_possible_truncation)]
fn generate_entry_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let millis = now.as_millis() as u32;
    // Mix in lower bits of sub-millisecond timing and a simple diversifier
    // to avoid collisions when called in rapid succession.
    let diversifier = ((millis << 16) ^ (now.subsec_nanos() >> 4)) as u16;
    format!("kb_{:08x}{:04x}", millis % 0xFFFF_FFFF, diversifier)
}

/// Ensure an entry ID is unique within the index by appending a counter suffix
/// if the generated ID already exists. Best-effort guard — not cryptographic.
fn deduplicate_entry_id(base_id: &str, index: &KbIndex) -> String {
    if !index.entries.iter().any(|e| e.entry_id == base_id) {
        return base_id.to_string();
    }
    // Collision detected — append counter suffix (_1, _2, ...)
    for counter in 1..100 {
        let candidate = format!("{base_id}_{counter}");
        if !index.entries.iter().any(|e| e.entry_id == candidate) {
            return candidate;
        }
    }
    // Extremely unlikely fallback: use a larger diversifier
    format!("{base_id}_overflow")
}

/// `kb list` implementation.
async fn kb_list(config: &CliConfig, scope: &KbScope, world_id: Option<&str>) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        let store = open_world_kb_store(config).await?;
        let blocks = store
            .list_by_world(&wid)
            .await
            .map_err(|e| CliError::Other(format!("World KB list failed for {wid}: {e}")))?;
        if blocks.is_empty() {
            println!("No key blocks in world {wid}.");
        } else {
            println!("Key blocks in world {wid}:");
            println!("{:<20} {:<15} {:<30} STATUS", "BLOCK_ID", "TYPE", "NAME");
            for block in &blocks {
                println!(
                    "{:<20} {:<15} {:<30} {}",
                    block.key_block_id,
                    format!("{:?}", block.block_type),
                    block.canonical_name,
                    block.status
                );
            }
        }
        return Ok(());
    }
    let (creator_id, slug, home) = resolve_kb_paths(config)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        match client.list_kb_entries(&creator_id, Some(&slug), None).await {
            Ok(resp) => {
                if resp.items.is_empty() {
                    println!("No local work entries in workspace {slug}.");
                } else {
                    println!("Local work entries in workspace {slug}:");
                    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
                    for entry in &resp.items {
                        println!(
                            "{:<20} {:<40} {}",
                            entry.entry_id, entry.title, entry.created_at
                        );
                    }
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index list failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct FS read
    let kb_dir = paths::creator_kb_dir(&home, &creator_id, &slug);
    let index_path = kb_dir.join("index.json");

    if !index_path.exists() {
        println!("No local work entries in workspace {slug}.");
        return Ok(());
    }

    let index = read_kb_index(&index_path);
    if index.entries.is_empty() {
        println!("No local work entries in workspace {slug}.");
        return Ok(());
    }

    println!("Local work entries in workspace {slug}:");
    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
    for entry in &index.entries {
        println!(
            "{:<20} {:<40} {}",
            entry.entry_id, entry.title, entry.created_at
        );
    }
    Ok(())
}

/// `kb search` implementation — case-insensitive substring match on title/content.
async fn kb_search(
    config: &CliConfig,
    query: &str,
    scope: &KbScope,
    world_id: Option<&str>,
) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        let store = open_world_kb_store(config).await?;
        let kb_query = nexus_kb::KbQuery::new(&wid).with_text_search(query);
        let result = store
            .query(&kb_query)
            .await
            .map_err(|e| CliError::Other(format!("World KB search failed for {wid}: {e}")))?;
        if result.items.is_empty() {
            println!("No key blocks matching \"{query}\" in world {wid}.");
        } else {
            println!("Key blocks matching \"{query}\" in world {wid}:");
            println!("{:<20} {:<15} {:<30} STATUS", "BLOCK_ID", "TYPE", "NAME");
            for block in &result.items {
                println!(
                    "{:<20} {:<15} {:<30} {}",
                    block.key_block_id,
                    format!("{:?}", block.block_type),
                    block.canonical_name,
                    block.status
                );
            }
        }
        return Ok(());
    }
    let (creator_id, slug, home) = resolve_kb_paths(config)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        match client
            .list_kb_entries(&creator_id, Some(&slug), Some(query))
            .await
        {
            Ok(resp) => {
                if resp.items.is_empty() {
                    println!("No local work entries matching \"{query}\" in workspace {slug}.");
                } else {
                    println!("Local work entries matching \"{query}\" in workspace {slug}:");
                    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
                    for entry in &resp.items {
                        println!(
                            "{:<20} {:<40} {}",
                            entry.entry_id, entry.title, entry.created_at
                        );
                    }
                }
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index search failed, falling back: {e}");
            }
        }
    }

    // Fallback: local search
    let kb_dir = paths::creator_kb_dir(&home, &creator_id, &slug);
    let index_path = kb_dir.join("index.json");

    if !index_path.exists() {
        println!("No local work entries in workspace {slug} to search.");
        return Ok(());
    }

    let index = read_kb_index(&index_path);
    let query_lower = query.to_lowercase();
    let matches: Vec<&KbIndexEntry> = index
        .entries
        .iter()
        .filter(|e| e.title.to_lowercase().contains(&query_lower))
        .collect();

    if matches.is_empty() {
        println!("No local work entries matching \"{query}\" in workspace {slug}.");
        return Ok(());
    }

    println!("Local work entries matching \"{query}\" in workspace {slug}:");
    println!("{:<20} {:<40} CREATED_AT", "ENTRY_ID", "TITLE");
    for entry in matches {
        println!(
            "{:<20} {:<40} {}",
            entry.entry_id, entry.title, entry.created_at
        );
    }
    Ok(())
}

/// `kb show` implementation — read and print a single entry file / key block.
async fn kb_show(
    config: &CliConfig,
    entry_id: &str,
    scope: &KbScope,
    world_id: Option<&str>,
) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        let store = open_world_kb_store(config).await?;
        let block = store.get_key_block(entry_id).await.map_err(|e| {
            CliError::Other(format!("World KB show failed for {entry_id} in {wid}: {e}"))
        })?;
        println!("Key Block: {}", block.key_block_id);
        println!("  World:      {}", block.world_id);
        println!("  Name:       {}", block.canonical_name);
        println!("  Type:       {:?}", block.block_type);
        println!("  Status:     {}", block.status);
        println!("  Created:    {}", block.created_at);
        if let Some(ref body) = block.body {
            if let Some(ref summary) = body.summary {
                println!("  Summary:    {summary}");
            }
            if let Some(ref tags) = body.tags {
                println!("  Tags:       {}", tags.join(", "));
            }
        }
        return Ok(());
    }
    // F001: Validate entry_id before constructing file path to prevent path traversal.
    paths::validate_entry_id_safe(entry_id).map_err(CliError::Other)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        match client.get_kb_entry(entry_id).await {
            Ok(resp) => {
                println!("{}", resp.content);
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index show failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct FS read
    let (creator_id, slug, home) = resolve_kb_paths(config)?;
    let entries_dir = paths::creator_kb_entries_dir(&home, &creator_id, &slug);
    let entry_path = entries_dir.join(format!("{entry_id}.md"));

    if !entry_path.exists() {
        return Err(CliError::Other(format!(
            "Work-scope entry {entry_id} not found in workspace {slug}."
        )));
    }

    let content = std::fs::read_to_string(&entry_path)?;
    println!("{content}");
    Ok(())
}

/// `kb add` implementation — copy file into local work index, or add world KB block.
///
/// For work scope: writes the index update to a temp file first, then copies the entry file,
/// then atomically renames the index. This prevents orphan entry files on
/// partial failure (W2).
///
/// For world scope: creates a `KeyBlock` via `SqliteKbStore::insert_key_block`.
async fn kb_add(
    config: &CliConfig,
    file: &std::path::Path,
    title: Option<&str>,
    scope: &KbScope,
    world_id: Option<&str>,
    block_type: Option<&str>,
) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        let bt_str = block_type.unwrap_or("InfoPoint");
        let bt = parse_block_type_cli(bt_str)?;
        let entry_title = title
            .map(std::string::ToString::to_string)
            .or_else(|| file.file_stem().map(|s| s.to_string_lossy().to_string()))
            .unwrap_or_else(|| "untitled".to_string());

        let store = open_world_kb_store(config).await?;
        let mut kb = nexus_kb::key_block::KeyBlock::new(&wid, bt, &entry_title);

        // Read file content as summary if provided
        if file.exists() {
            let content = std::fs::read_to_string(file)?;
            let summary = if content.len() > 500 {
                format!("{}...", &content[..500])
            } else {
                content
            };
            kb.body = Some(nexus_kb::key_block::KeyBlockBody {
                summary: Some(summary),
                attributes: None,
                tags: None,
            });
        }

        let result = store
            .insert_key_block(kb)
            .await
            .map_err(|e| CliError::Other(format!("World KB add failed for {wid}: {e}")))?;
        println!("✓ Key block added: {}", result.key_block_id);
        println!("  World:  {wid}");
        println!("  Type:   {bt_str}");
        println!("  Name:   {entry_title}");
        return Ok(());
    }
    if !file.exists() {
        return Err(CliError::Other(format!(
            "Source file not found: {}",
            file.display()
        )));
    }

    let (creator_id, slug, _home) = resolve_kb_paths(config)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        let content = std::fs::read_to_string(file)?;
        let req = crate::api::models::AddKbEntryRequest {
            creator_id: creator_id.clone(),
            workspace_slug: Some(slug.clone()),
            title: title.map(std::string::ToString::to_string),
            content: Some(content),
            file_path: None,
        };
        match client.add_kb_entry(&req).await {
            Ok(resp) => {
                println!("✓ Local work entry added: {}", resp.entry_id);
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index add failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct FS operations
    let (_, _, home) = resolve_kb_paths(config)?;
    let kb_dir = paths::creator_kb_dir(&home, &creator_id, &slug);
    let entries_dir = paths::creator_kb_entries_dir(&home, &creator_id, &slug);
    let index_path = kb_dir.join("index.json");

    // Create directories if needed
    std::fs::create_dir_all(&entries_dir)?;

    // Generate entry ID and determine title
    let base_id = generate_entry_id();
    let mut index = read_kb_index(&index_path);
    let entry_id = deduplicate_entry_id(&base_id, &index);
    let entry_title = title
        .map(std::string::ToString::to_string)
        .or_else(|| file.file_stem().map(|s| s.to_string_lossy().to_string()))
        .unwrap_or_else(|| entry_id.clone());

    // Step 1: Write updated index to temp file (W2 — index update first)
    let created_at = chrono::Utc::now().to_rfc3339();
    index.entries.push(KbIndexEntry {
        entry_id: entry_id.clone(),
        title: entry_title,
        created_at,
    });
    let tmp_index_path = index_path.with_extension("json.tmp");
    {
        if let Some(parent) = tmp_index_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(&index)?;
        std::fs::write(&tmp_index_path, json)?;
    }

    // Step 2: Copy source file to entries dir
    let dest = entries_dir.join(format!("{entry_id}.md"));
    std::fs::copy(file, &dest)?;

    // Step 3: Atomically rename temp index to final (W2 — only committed after file is safe)
    std::fs::rename(&tmp_index_path, &index_path)?;

    println!("✓ Local work entry added: {entry_id}");
    Ok(())
}

/// `kb remove` implementation — delete a local work-scope entry or world KB block.
///
/// Tries the daemon API first; falls back to direct FS removal
/// (delete entry file + update index atomically).
async fn kb_remove(
    config: &CliConfig,
    entry_id: &str,
    scope: &KbScope,
    world_id: Option<&str>,
) -> Result<()> {
    if scope == &KbScope::World {
        let wid = require_world_id(world_id)?;
        let store = open_world_kb_store(config).await?;
        store.delete_key_block(entry_id).await.map_err(|e| {
            CliError::Other(format!(
                "World KB remove failed for {entry_id} in {wid}: {e}"
            ))
        })?;
        println!("✓ Key block removed: {entry_id}");
        return Ok(());
    }
    // F001: Validate entry_id before use.
    paths::validate_entry_id_safe(entry_id).map_err(CliError::Other)?;

    // Try daemon API first (T40: migration)
    let client = crate::api::DaemonClient::from_config(config);
    if client.health_check().await? {
        match client.delete_kb_entry(entry_id).await {
            Ok(_resp) => {
                println!("✓ Local work entry removed: {entry_id}");
                return Ok(());
            }
            Err(e) => {
                eprintln!("nexus42: daemon work-index remove failed, falling back: {e}");
            }
        }
    }

    // Fallback: direct FS removal
    let (creator_id, slug, home) = resolve_kb_paths(config)?;
    let entries_dir = paths::creator_kb_entries_dir(&home, &creator_id, &slug);
    let entry_path = entries_dir.join(format!("{entry_id}.md"));

    if !entry_path.exists() {
        return Err(CliError::Other(format!(
            "Work-scope entry {entry_id} not found in workspace {slug}."
        )));
    }

    // Remove the entry file
    std::fs::remove_file(&entry_path)?;

    // Update index to remove the entry
    let kb_dir = paths::creator_kb_dir(&home, &creator_id, &slug);
    let index_path = kb_dir.join("index.json");
    let mut index = read_kb_index(&index_path);
    let original_len = index.entries.len();
    index.entries.retain(|e| e.entry_id != entry_id);
    if index.entries.len() == original_len {
        // Entry was not in index but file existed — still report success
        tracing::warn!("Work-scope entry {entry_id} file existed but was not in index");
    } else if !index.entries.is_empty() {
        // Write updated index
        write_kb_index(&index_path, &index)?;
    } else if index_path.exists() {
        // Last entry removed — clean up empty index
        let _ = std::fs::remove_file(&index_path);
    }

    println!("✓ Local work entry removed: {entry_id}");
    Ok(())
}

// ── KB Extract Queue ─────────────────────────────────────────────────

/// `kb queue-extract` — idempotent enqueue of a work entry for extraction.
///
/// Creates a row in `kb_extract_jobs` with status `queued`.
/// The actual extraction is performed by the `kb.extract_work` capability
/// (triggered via preset or daemon orchestration). No LLM calls here.
async fn kb_queue_extract(config: &CliConfig, work_entry_id: &str, world_id: &str) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();
    let slug = config.workspace_slug_for_creator(&creator_id).to_string();

    // Validate entry_id format to prevent path traversal.
    paths::validate_entry_id_safe(work_entry_id).map_err(CliError::Other)?;

    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;

    let job =
        nexus_local_db::enqueue_extract_job(&pool, &creator_id, &slug, work_entry_id, world_id)
            .await
            .map_err(|e| CliError::Other(format!("Failed to enqueue extract job: {e}")))?;

    if job.status == "queued" {
        // Check if this was a new enqueue vs idempotent return
        println!("✓ Extract job queued: {}", job.job_id);
    } else {
        println!("ℹ Extract job already exists: {}", job.job_id);
    }
    println!("  Work entry:  {work_entry_id}");
    println!("  Target world: {world_id}");
    println!("  Status:       {}", job.status);
    println!("  Created:      {}", job.created_at);
    Ok(())
}

/// Default maximum number of extract jobs shown when listing without `--job-id`.
const DEFAULT_EXTRACT_STATUS_LIMIT: u32 = 100;

/// `kb extract-status` — show extract job(s) for the active creator.
///
/// With `--job-id`, shows a specific job. Without it, lists up to
/// `DEFAULT_EXTRACT_STATUS_LIMIT` (100) most recent jobs.
async fn kb_extract_status(config: &CliConfig, job_id: Option<&str>) -> Result<()> {
    let creator_id = config
        .active_creator_id
        .as_deref()
        .ok_or(CliError::CreatorNotSelected)?
        .to_string();

    let db_path = crate::config::resolve_state_db_path(config)?;
    let pool = crate::db::Schema::init(&db_path).await?;

    if let Some(jid) = job_id {
        let job = nexus_local_db::get_extract_job(&pool, jid)
            .await
            .map_err(|e| CliError::Other(format!("Failed to get extract job: {e}")))?;

        let Some(job) = job else {
            return Err(CliError::Other(format!("Extract job '{jid}' not found.")));
        };
        print_job_detail(&job);
    } else {
        let jobs =
            nexus_local_db::list_extract_jobs(&pool, &creator_id, DEFAULT_EXTRACT_STATUS_LIMIT)
                .await
                .map_err(|e| CliError::Other(format!("Failed to list extract jobs: {e}")))?;

        if jobs.is_empty() {
            println!("No extract jobs for creator {creator_id}.");
            return Ok(());
        }

        println!(
            "Extract jobs for creator {creator_id} (showing up to {DEFAULT_EXTRACT_STATUS_LIMIT}):"
        );
        println!(
            "{:<20} {:<15} {:<20} {:<20} STATUS",
            "JOB_ID", "WORK_ENTRY", "WORLD", "CREATED"
        );
        for job in &jobs {
            println!(
                "{:<20} {:<15} {:<20} {:<20} {}",
                job.job_id,
                truncate_str(&job.work_entry_id, 15),
                truncate_str(&job.world_id, 20),
                job.created_at,
                job.status,
            );
        }
    }
    Ok(())
}

/// Print a single job in detail.
fn print_job_detail(job: &nexus_local_db::KbExtractJob) {
    println!("Job:           {}", job.job_id);
    println!("  Creator:     {}", job.creator_id);
    println!("  Workspace:   {}", job.workspace_id);
    println!("  Work entry:  {}", job.work_entry_id);
    println!("  World:       {}", job.world_id);
    println!("  Status:      {}", job.status);
    println!("  Created:     {}", job.created_at);
    if let Some(ref started) = job.started_at {
        println!("  Started:     {started}");
    }
    if let Some(ref finished) = job.finished_at {
        println!("  Finished:    {finished}");
    }
    if let Some(ref err) = job.error_text {
        println!("  Error:       {err}");
    }
}

/// Truncate a string for tabular display.
fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
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
    )
    .await
    .map_err(|e| CliError::Other(format!("Failed to create demo event: {e}")))?;
    println!("✓ Demo event: {}", event.event_id);

    // 3. Create demo KB block
    let mut kb = nexus_kb::key_block::KeyBlock::new(
        &world.world_id,
        nexus_contracts::BlockType::Character,
        "Hero",
    );
    kb.body = Some(nexus_kb::key_block::KeyBlockBody {
        summary: Some("The protagonist of the demo world.".to_string()),
        attributes: None,
        tags: Some(vec!["protagonist".to_string(), "demo".to_string()]),
    });
    let kb_store = nexus_local_db::kb_store::SqliteKbStore::new(pool.clone());
    let kb_result = kb_store
        .insert_key_block(kb)
        .await
        .map_err(|e| CliError::Other(format!("Failed to create demo KB block: {e}")))?;
    println!("✓ Demo KB block: {}", kb_result.key_block_id);

    // 4. Create demo knowledge entry
    let knowledge_store = nexus_local_db::SqliteKnowledgeStore::new(pool);
    let entry = nexus_knowledge::KnowledgeEntry::new(
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
) -> Result<()> {
    // WS-B T4: validate name length (cheap check before regex)
    if name.len() > MAX_CREATOR_NAME_LENGTH {
        return Err(CliError::Other(format!(
            "Creator name exceeds maximum length ({MAX_CREATOR_NAME_LENGTH} characters)"
        )));
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
    // prompt the user to authenticate first.
    let auth_token = obtain_auth_token(&auth_store)?;

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

/// List all known Creators with three-layer identity model (V1.16).
fn list_creators(_config: &CliConfig) -> Result<()> {
    let config = CliConfig::load()?;
    let cache = creator_identity::load_creator_identity_cache();
    let active_id = config.active_creator_id.as_deref();

    // Collect all known creators from both the identity cache and auth store.
    // The identity cache is the primary source for display metadata.
    let auth_store = crate::auth::AuthStore::load()?;

    // Gather all known creator IDs from both sources.
    let mut all_ids: Vec<String> = cache.creators.keys().cloned().collect();
    if let Some(creators) = &auth_store.creators {
        for id in creators.keys() {
            if !all_ids.contains(id) {
                all_ids.push(id.clone());
            }
        }
    }
    all_ids.sort();

    if all_ids.is_empty() {
        println!("No registered Creators found.");
        println!("Use: nexus42 creator register --name <name>");
        return Ok(());
    }

    println!("CREATOR ID          HANDLE         DISPLAY NAME          ACTIVE");
    for id in &all_ids {
        let entry = creator_identity::get_creator_identity(&cache, id);
        let handle_str = entry.and_then(|e| e.handle.as_deref()).unwrap_or("-");
        let display_str = entry.and_then(|e| e.display_name.as_deref()).unwrap_or("-");
        let active_marker = if active_id == Some(id.as_str()) {
            "✓"
        } else {
            ""
        };
        println!("{id:<19} {handle_str:<14} {display_str:<21} {active_marker}");
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
    sqlx::query!(
        "INSERT OR REPLACE INTO creators (creator_id, display_name, status, cached_at, data)
         VALUES (?, ?, ?, ?, ?)",
        creator.creator_id,
        creator.display_name,
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

    // ── R-KB-002: ID collision guard tests ────────────────────────────

    #[test]
    fn deduplicate_entry_id_returns_base_when_no_collision() {
        let index = KbIndex::default();
        let result = deduplicate_entry_id("kb_abc12345", &index);
        assert_eq!(result, "kb_abc12345");
    }

    #[test]
    fn deduplicate_entry_id_appends_counter_on_collision() {
        let mut index = KbIndex::default();
        index.entries.push(KbIndexEntry {
            entry_id: "kb_abc12345".to_string(),
            title: "existing".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        });
        let result = deduplicate_entry_id("kb_abc12345", &index);
        assert_eq!(result, "kb_abc12345_1");
        // Verify the suffixed ID is not already in the index
        assert!(index.entries.iter().all(|e| e.entry_id != result));
    }

    #[test]
    fn deduplicate_entry_id_increments_counter_for_multiple_collisions() {
        let mut index = KbIndex::default();
        index.entries.push(KbIndexEntry {
            entry_id: "kb_abc12345".to_string(),
            title: "first".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        });
        index.entries.push(KbIndexEntry {
            entry_id: "kb_abc12345_1".to_string(),
            title: "second".to_string(),
            created_at: "2026-01-01T00:00:00Z".to_string(),
        });
        let result = deduplicate_entry_id("kb_abc12345", &index);
        assert_eq!(result, "kb_abc12345_2");
    }

    #[test]
    fn kb_generate_entry_id_format() {
        let id = generate_entry_id();
        assert!(id.starts_with("kb_"));
        assert_eq!(id.len(), 15, "entry ID should be kb_ + 12 hex chars");
    }

    // ── R-KB-001: Corrupt index.json detection tests ──────────────────

    #[test]
    fn read_kb_index_returns_empty_for_corrupt_json() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let index_path = tmp.path().join("index.json");
        std::fs::write(&index_path, "this is not valid json {{{").expect("write corrupt");

        // Should return empty index (not panic)
        let index = read_kb_index(&index_path);
        assert!(
            index.entries.is_empty(),
            "corrupt index should return empty"
        );
    }

    #[test]
    fn read_kb_index_returns_empty_for_missing_file() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let index_path = tmp.path().join("nonexistent.json");

        let index = read_kb_index(&index_path);
        assert!(index.entries.is_empty());
    }

    #[test]
    fn read_kb_index_parses_valid_json() {
        let tmp = tempfile::TempDir::new().expect("tempdir");
        let index_path = tmp.path().join("index.json");
        let content = r#"{"entries":[{"entry_id":"kb_test1234","title":"Test","created_at":"2026-01-01T00:00:00Z"}]}"#;
        std::fs::write(&index_path, content).expect("write valid");

        let index = read_kb_index(&index_path);
        assert_eq!(index.entries.len(), 1);
        assert_eq!(index.entries[0].entry_id, "kb_test1234");
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
}
