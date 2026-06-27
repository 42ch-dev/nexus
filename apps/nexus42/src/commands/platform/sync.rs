//! Platform Sync Command — Canonical sync surface under `platform`.
//!
//! Delegates to the existing `commands::sync` module handlers.
//! This is the V1.35 canonical location; the top-level `sync` group
//! is deprecated and forwards here with a stderr warning.

use crate::commands::sync::{self, SyncCommand};
use crate::config::CliConfig;
use crate::errors::Result;

/// Run platform sync command by delegating to the top-level sync handlers.
///
/// # Errors
///
/// Returns `CliError` if the delegated sync command fails.
pub async fn run(cmd: SyncCommand, config: &CliConfig) -> Result<()> {
    sync::run(cmd, config).await
}
