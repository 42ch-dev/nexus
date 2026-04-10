//! Migrate legacy on-disk layout to ADR-014 (creator / workspace operational tree).

use crate::commands::init::{
    materialize_adr014_workspace, persist_cli_workspace_selection, validate_slug,
};
use crate::config::user_home_dir;
use crate::errors::{CliError, Result};
use crate::paths;
use clap::Subcommand;
use std::path::PathBuf;

#[derive(Debug, Subcommand)]
pub enum MigrateCommand {
    /// Move legacy `$HOME/.nexus42/state.db` into `creators/<id>/workspaces/<slug>/state.db`
    #[command(name = "local-fs")]
    LocalFs {
        #[arg(long)]
        creator_id: String,
        #[arg(long, default_value = "default")]
        workspace_slug: String,
        /// Creative project root (created if missing)
        #[arg(long)]
        local_root: PathBuf,
        /// Confirm migration (required for non-interactive use)
        #[arg(long)]
        yes: bool,
    },
}

pub async fn run(cmd: MigrateCommand) -> Result<()> {
    match cmd {
        MigrateCommand::LocalFs {
            creator_id,
            workspace_slug,
            local_root,
            yes,
        } => migrate_local_fs(creator_id, workspace_slug, local_root, yes),
    }
}

fn migrate_local_fs(
    creator_id: String,
    workspace_slug: String,
    mut local_root: PathBuf,
    yes: bool,
) -> Result<()> {
    if !yes {
        return Err(CliError::Other(
            "Refusing to migrate without `--yes` (this renames the legacy SQLite file).".into(),
        ));
    }
    validate_slug("creator_id", &creator_id)?;
    validate_slug("workspace_slug", &workspace_slug)?;

    let user_home = user_home_dir()?;
    let legacy = paths::legacy_flat_state_db_path(&user_home);
    if !legacy.is_file() {
        return Err(CliError::Other(format!(
            "No legacy database at {}.",
            legacy.display()
        )));
    }

    let current = std::env::current_dir().map_err(CliError::Io)?;
    if !local_root.is_absolute() {
        local_root = current.join(local_root);
    }
    std::fs::create_dir_all(&local_root).map_err(CliError::Io)?;

    let op_meta = paths::operational_workspace_dir(&user_home, &creator_id, &workspace_slug)
        .join("meta.json");
    let target_db = paths::state_db_path(&user_home, &creator_id, &workspace_slug);
    if op_meta.exists() || target_db.exists() {
        return Err(CliError::Other(format!(
            "Target workspace already exists ({}). Remove it or pick another slug.",
            target_db.display()
        )));
    }

    let workspace_name = workspace_slug.clone();
    let db_path = materialize_adr014_workspace(
        &user_home,
        &creator_id,
        &workspace_slug,
        &local_root,
        &workspace_name,
    )?;

    std::fs::remove_file(&db_path).map_err(CliError::Io)?;
    std::fs::copy(&legacy, &db_path).map_err(CliError::Io)?;

    let backup = legacy.with_extension("db.pre-adr014-migrated");
    if backup.exists() {
        std::fs::remove_file(&backup).map_err(CliError::Io)?;
    }
    std::fs::rename(&legacy, &backup).map_err(CliError::Io)?;

    persist_cli_workspace_selection(
        local_root.clone(),
        creator_id.clone(),
        workspace_slug.clone(),
    )?;

    println!("✓ Migrated legacy state.db to ADR-014 layout.");
    println!("  Archived legacy file to: {}", backup.display());
    println!("  state.db: {}", db_path.display());
    println!("  Creative root: {}", local_root.display());
    Ok(())
}
