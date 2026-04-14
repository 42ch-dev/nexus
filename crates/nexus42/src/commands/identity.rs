//! Identity Command Module
//!
//! Local-only identity management for `local_only` mode.
//! Subcommands: list, create, use, link.
//!
//! Anonymous identities (`ctr_anon*`) are ephemeral.
//! Persistent identities (`ctr_local*`) are stored in SQLite.

use crate::config::{self, CliConfig};
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_domain::local_identity::LocalIdentity;
use nexus_local_db::{
    create_local_identity, get_local_identity, link_to_platform, list_local_identities, RuntimeRole,
};
use rusqlite::Connection;

#[derive(Debug, Subcommand)]
pub enum IdentityCommand {
    /// List all local identities
    List,

    /// Create a new local identity
    Create {
        /// Identity type: anonymous (ephemeral) or persistent (stored in SQLite)
        #[arg(long, value_enum, default_value = "persistent")]
        kind: IdentityKindArg,

        /// Display name for the identity (recommended for persistent)
        #[arg(long)]
        name: Option<String>,
    },

    /// Set the active identity for the current session
    Use {
        /// Creator ID of the identity to activate
        creator_id: String,
    },

    /// Link a local identity to a platform Creator
    Link {
        /// Local creator ID to link
        creator_id: String,
        /// Platform Creator ID to link to
        #[arg(long)]
        platform_id: String,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum IdentityKindArg {
    Anonymous,
    Persistent,
}

/// Run identity command.
pub async fn run(cmd: IdentityCommand, _config: &CliConfig) -> Result<()> {
    match cmd {
        IdentityCommand::List => list_identities(),
        IdentityCommand::Create { kind, name } => create_identity(kind, name),
        IdentityCommand::Use { creator_id } => use_identity(creator_id),
        IdentityCommand::Link {
            creator_id,
            platform_id,
        } => link_identity(creator_id, platform_id),
    }
}

/// Open or create the global identity database at `~/.nexus42/state.db`.
fn open_global_db() -> Result<Connection> {
    let home = config::user_home_dir()?;
    let nexus_dir = home.join(".nexus42");

    // Ensure ~/.nexus42/ exists
    std::fs::create_dir_all(&nexus_dir)?;

    let db_path = nexus_dir.join("state.db");
    let conn = Connection::open(&db_path)?;
    nexus_local_db::init(&conn, RuntimeRole::Cli)?;
    Ok(conn)
}

/// List all local identities.
fn list_identities() -> Result<()> {
    let conn = open_global_db()?;
    let identities = list_local_identities(&conn)?;

    if identities.is_empty() {
        println!("No local identities found.");
        println!("Create one with: nexus42 identity create [--anonymous | --persistent] [--name \"name\"]");
        return Ok(());
    }

    let cli_config = CliConfig::load()?;
    let active_id = cli_config.active_creator_id.as_deref();

    println!("Local Identities:");
    println!();

    for identity in &identities {
        let active_mark = active_id
            .map(|a| {
                if a == identity.creator_id {
                    " (active)"
                } else {
                    ""
                }
            })
            .unwrap_or("");

        let linked_mark = if identity.platform_linked {
            format!(
                " → {}",
                identity.platform_creator_id.as_deref().unwrap_or("?")
            )
        } else {
            String::new()
        };

        let name_display = identity.display_name.as_deref().unwrap_or("(no name)");

        let kind_label = match identity.identity_type.as_str() {
            "anonymous" => "anon",
            "persistent" => "local",
            other => other,
        };

        println!(
            "  {} [{}] {}{}{}",
            identity.creator_id, kind_label, name_display, linked_mark, active_mark
        );
    }

    Ok(())
}

/// Create a new local identity.
fn create_identity(kind: IdentityKindArg, name: Option<String>) -> Result<()> {
    let identity = match kind {
        IdentityKindArg::Anonymous => LocalIdentity::create_anonymous(),
        IdentityKindArg::Persistent => LocalIdentity::create_persistent(name.as_deref()),
    };

    let is_persistent = identity.is_persistent();

    if is_persistent {
        // Persist to SQLite
        let conn = open_global_db()?;
        create_local_identity(
            &conn,
            &identity.creator_id,
            identity.identity_type.as_str(),
            identity.display_name.as_deref(),
            &identity.created_at,
        )?;
    }

    println!(
        "Created {} identity: {}",
        kind_label(&kind),
        identity.creator_id
    );
    if let Some(ref name) = identity.display_name {
        println!("  Name: {}", name);
    }
    if is_persistent {
        println!("  Stored in ~/.nexus42/state.db");
    } else {
        println!("  Ephemeral — data will be lost when this session ends");
        println!("  (run `nexus42 identity create --persistent` for a saved identity)");
    }

    // Auto-set as active
    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = Some(identity.creator_id.clone());
    cli_config.save()?;
    println!("  Set as active identity.");

    Ok(())
}

/// Set the active identity.
fn use_identity(creator_id: String) -> Result<()> {
    // Verify identity exists
    let conn = open_global_db()?;
    let identity = get_local_identity(&conn, &creator_id)?;

    match identity {
        Some(_) => {
            let mut cli_config = CliConfig::load()?;
            cli_config.active_creator_id = Some(creator_id.clone());
            cli_config.save()?;
            println!("Active identity set to: {}", creator_id);
            Ok(())
        }
        None => Err(CliError::Other(format!(
            "Local identity '{}' not found. Run `nexus42 identity list` to see available identities.",
            creator_id
        ))),
    }
}

/// Link a local identity to a platform Creator.
fn link_identity(creator_id: String, platform_id: String) -> Result<()> {
    let conn = open_global_db()?;
    link_to_platform(&conn, &creator_id, &platform_id)?;

    println!("Linked {} to platform creator: {}", creator_id, platform_id);
    Ok(())
}

/// Resolve the active local identity for the current session.
///
/// Resolution order:
/// 1. Check `active_creator_id` from CLI config
/// 2. If set, verify it exists in local_identities (for persistent) or accept as-is (for anonymous)
/// 3. If no identity is set, return `None` (caller should prompt to create one)
///
/// Returns the resolved identity info or `None` if no identity is configured.
#[allow(dead_code)]
pub fn resolve_active_identity() -> Result<Option<ResolvedIdentity>> {
    let cli_config = CliConfig::load()?;
    let creator_id = match &cli_config.active_creator_id {
        Some(id) => id.clone(),
        None => return Ok(None),
    };

    // Check if it's a persistent identity in the DB
    let conn = open_global_db()?;
    let row = get_local_identity(&conn, &creator_id)?;

    match row {
        Some(db_row) => {
            let identity_type = db_row.identity_type.as_str();
            Ok(Some(ResolvedIdentity {
                creator_id,
                identity_type: identity_type.to_string(),
                display_name: db_row.display_name,
                is_anonymous: identity_type == "anonymous",
                is_persistent: identity_type == "persistent",
                platform_linked: db_row.platform_linked,
                platform_creator_id: db_row.platform_creator_id,
            }))
        }
        None => {
            // The active_creator_id might be a platform creator (from previous session)
            // or an anonymous identity that wasn't persisted
            if creator_id.starts_with("ctr_anon") {
                Ok(Some(ResolvedIdentity {
                    creator_id,
                    identity_type: "anonymous".to_string(),
                    display_name: None,
                    is_anonymous: true,
                    is_persistent: false,
                    platform_linked: false,
                    platform_creator_id: None,
                }))
            } else {
                // Might be a platform creator ID; return minimal info
                Ok(Some(ResolvedIdentity {
                    creator_id,
                    identity_type: "platform".to_string(),
                    display_name: None,
                    is_anonymous: false,
                    is_persistent: false,
                    platform_linked: false,
                    platform_creator_id: None,
                }))
            }
        }
    }
}

/// Resolved identity information for the active session.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ResolvedIdentity {
    pub creator_id: String,
    pub identity_type: String,
    pub display_name: Option<String>,
    pub is_anonymous: bool,
    pub is_persistent: bool,
    pub platform_linked: bool,
    pub platform_creator_id: Option<String>,
}

impl ResolvedIdentity {
    /// Check if this is an ephemeral (anonymous) identity.
    #[allow(dead_code)]
    pub fn is_ephemeral(&self) -> bool {
        self.is_anonymous
    }

    /// Warning message for anonymous identities.
    #[allow(dead_code)]
    pub fn ephemeral_warning(&self) -> Option<&'static str> {
        if self.is_anonymous {
            Some("Active identity is anonymous — data will be lost when this session ends. Use `nexus42 identity create --persistent` for a saved identity.")
        } else {
            None
        }
    }
}

fn kind_label(kind: &IdentityKindArg) -> &'static str {
    match kind {
        IdentityKindArg::Anonymous => "anonymous",
        IdentityKindArg::Persistent => "persistent",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kind_label() {
        assert_eq!(kind_label(&IdentityKindArg::Anonymous), "anonymous");
        assert_eq!(kind_label(&IdentityKindArg::Persistent), "persistent");
    }
}
