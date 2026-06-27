//! Desktop shell commands for nexus42.
//!
//! These commands operate on the Tauri desktop shell in `apps/desktop`. They
//! are intentionally thin wrappers around `pnpm`/`cargo tauri` so the build
//! pipeline stays reproducible and env-gated.

use crate::errors::{CliError, Result};
use clap::Subcommand;
use std::path::PathBuf;
use std::process::Stdio;

/// Desktop shell subcommands.
#[derive(Debug, Subcommand)]
pub enum DesktopCommand {
    /// Build the Tauri desktop bundle.
    ///
    /// Code signing is opt-in via `--sign-identity` or the
    /// `APPLE_SIGNING_IDENTITY` environment variable. If neither is set the
    /// bundle is built unsigned (the default for local development).
    Bundle {
        /// Apple Developer ID signing identity (e.g. "Developer ID Application: ...").
        ///
        /// Falls back to the `APPLE_SIGNING_IDENTITY` environment variable.
        #[arg(long)]
        sign_identity: Option<String>,
    },
}

/// Run a desktop shell command.
///
/// # Errors
///
/// Returns a [`CliError::Io`] if the underlying `pnpm`/`cargo tauri` build
/// fails, or a [`CliError::Config`] if the repository root cannot be resolved.
pub async fn run(command: DesktopCommand) -> Result<()> {
    match command {
        DesktopCommand::Bundle { sign_identity } => bundle_desktop(sign_identity).await,
    }
}

/// Build the desktop Tauri bundle, optionally signing with the provided
/// Apple Developer ID identity.
///
/// # Errors
///
/// Returns a [`CliError::Io`] if the bundle command cannot be spawned or exits
/// with a non-zero status, or a [`CliError::Config`] if the repository root
/// cannot be resolved.
async fn bundle_desktop(sign_identity: Option<String>) -> Result<()> {
    let sign_identity = sign_identity
        .or_else(|| std::env::var("APPLE_SIGNING_IDENTITY").ok())
        .filter(|s| !s.is_empty());
    let repo_root = repo_root()?;

    match sign_identity.as_deref() {
        Some(identity) if !identity.is_empty() => {
            eprintln!("nexus42 desktop bundle: signing with identity '{identity}'");
        }
        _ => {
            eprintln!(
                "nexus42 desktop bundle: no signing identity provided; building unsigned bundle"
            );
        }
    }

    let mut cmd = std::process::Command::new("pnpm");
    cmd.arg("--filter")
        .arg("desktop")
        .arg("build")
        .current_dir(&repo_root)
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    if let Some(identity) = sign_identity {
        cmd.env("APPLE_SIGNING_IDENTITY", identity);
    }

    let status = tokio::task::spawn_blocking(move || cmd.status())
        .await
        .map_err(|e| CliError::Io(std::io::Error::other(e)))?
        .map_err(CliError::Io)?;

    if !status.success() {
        return Err(CliError::Io(std::io::Error::other(format!(
            "desktop bundle build failed with status {status}"
        ))));
    }

    println!("Desktop bundle built successfully.");
    Ok(())
}

/// Resolve the repository root from `CARGO_MANIFEST_DIR`.
///
/// `apps/nexus42` lives two levels below the repository root.
fn repo_root() -> Result<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .and_then(|p| p.parent())
        .map(PathBuf::from)
        .ok_or_else(|| {
            CliError::Config(
                "could not resolve repository root from CARGO_MANIFEST_DIR".to_string(),
            )
        })
}
