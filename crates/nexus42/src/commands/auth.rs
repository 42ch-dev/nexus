//! Auth Command — User and Creator authentication

use crate::auth;
use crate::config::CliConfig;
use crate::errors::Result;
use clap::Subcommand;
use nexus_domain::runtime_guard;

#[derive(Debug, Subcommand)]
pub enum AuthCommand {
    /// Login via device flow (OAuth)
    Login,

    /// Set access token directly (development/testing)
    Token {
        /// Access token
        access_token: String,
        /// Expiry time in seconds (default: 3600)
        #[arg(long, default_value_t = 3600)]
        expires_in: u64,
        /// User ID (prefix: "usr_")
        #[arg(long)]
        user_id: Option<String>,
    },

    /// Logout and clear credentials
    Logout,

    /// Show authentication status
    Status,
}

/// Run auth command
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
            let uid =
                user_id.unwrap_or_else(|| format!("usr_dev_{}", uuid::Uuid::new_v4()));
            auth::user_auth::login_with_token(config, access_token, uid, expires_in).await
        }
        AuthCommand::Logout => auth::user_auth::logout(config).await,
        AuthCommand::Status => auth::user_auth::status(config).await,
    }
}
