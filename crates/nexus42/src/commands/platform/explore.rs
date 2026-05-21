//! Explore browse/search — platform-only operations.
//!
//! These operations were previously proxied through the local daemon.
//! As of V1.20 they are no longer available in the local daemon API
//! (platform concern; not local daemon API per delivery compass §3).
//! Users should call the platform API directly.

use crate::errors::{CliError, Result};
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum ExploreCommand {
    /// Directory-style listing (platform API)
    Browse,
    /// Full-text style query (platform API)
    Search,
}

/// Run explore subcommands.
///
/// # Errors
///
/// Always returns `CliError` — explore browse/search is a platform-only
/// operation no longer proxied through the local daemon.
// async is retained because all callers `.await` this function.
#[allow(clippy::unused_async)]
pub async fn run(_cmd: ExploreCommand) -> Result<()> {
    Err(CliError::Config(
        "explore browse/search is a platform-only operation; \
         the local daemon no longer proxies these endpoints. \
         Use the platform API directly."
            .into(),
    ))
}
