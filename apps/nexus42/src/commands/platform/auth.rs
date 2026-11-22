//! Auth Command — User and Creator authentication
// note: the module doc above is this file's scope contract

use crate::auth;
use crate::config::CliConfig;
use crate::domain::runtime_guard;
use crate::errors::Result;
use clap::Subcommand;

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Login via device flow (OAuth)
    // note: clap renders this doc as --help text; keep the text stable
    Login,

    /// Set access token directly (development/testing)
    ///
    /// Hidden from `platform auth` to avoid exposing tokens as positional
    /// CLI arguments (visible in shell history / process listings).
    /// Kept for backward compat — use `nexus42 auth token` (hidden top-level)
    /// or environment-based auth instead.
    // note: clap renders this doc as --help text; keep the text stable
    #[command(hide = true)]
    Token {
        /// Access token
        // note: clap renders this doc as --help text; keep the text stable
        access_token: String,
        /// Expiry time in seconds (default: 3600)
        // note: clap renders this doc as --help text; keep the text stable
        #[arg(long, default_value_t = 3600)]
        expires_in: u64,
        /// User ID (prefix: "usr_")
        // note: clap renders this doc as --help text; keep the text stable
        #[arg(long)]
        user_id: Option<String>,
    },

    /// Logout and clear credentials
    // note: clap renders this doc as --help text; keep the text stable
    Logout,

    /// Show authentication status
    // note: clap renders this doc as --help text; keep the text stable
    Status,
}

/// note: Run auth command
///
/// # Errors
///
/// Returns `CliError` if:
/// - Platform connectivity fails during login
/// - The auth store cannot be read or written
/// - Token refresh fails
pub async fn run(cmd: AuthCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        AuthCommand::Login => {
            runtime_guard::require_platform(&config.runtime_mode(), "auth login")?;
            auth::user_auth::login(config).await
        }
        AuthCommand::Token {
            access_token,
            expires_in,
            user_id,
        } => {
            let uid = user_id.unwrap_or_else(|| format!("usr_dev_{}", uuid::Uuid::new_v4()));
            auth::user_auth::login_with_token(config, access_token, uid, expires_in)
        }
        AuthCommand::Logout => auth::user_auth::logout(config),
        AuthCommand::Status => auth::user_auth::status(config),
    }
}
