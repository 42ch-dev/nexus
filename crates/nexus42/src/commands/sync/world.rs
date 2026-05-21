//! World fork and snapshot — platform-only operations.
//!
//! These operations were previously proxied through the local daemon.
//! As of V1.20 they are no longer available in the local daemon API
//! (platform concern; not local daemon API per delivery compass §3).
//! Users should call the platform API directly.

use crate::errors::{CliError, Result};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum WorldCommand {
    /// Fork a new world from a parent world at a timeline event (platform API)
    Fork,
    /// Request a read-only world snapshot cursor from the platform
    Snapshot,
}

/// Run world subcommands.
///
/// # Errors
///
/// Always returns `CliError` — world fork/snapshot is a platform-only
/// operation no longer proxied through the local daemon.
// async is retained because all callers `.await` this function.
#[allow(clippy::unused_async)]
pub async fn run(_cmd: WorldCommand) -> Result<()> {
    Err(CliError::Config(
        "world fork/snapshot is a platform-only operation; \
         the local daemon no longer proxies these endpoints. \
         Use the platform API directly."
            .into(),
    ))
}
