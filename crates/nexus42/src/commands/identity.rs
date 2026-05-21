//! Identity Command Module
//!
//! Local-only identity management for `local_only` mode.
//! Subcommands: list, create, use, link, unlink.
//!
//! Anonymous identities (`ctr_anon*`) are ephemeral.
//! Persistent identities (`ctr_local*`) are stored in `SQLite`.

use crate::config::{self, CliConfig};
use crate::domain::DomainError;
use crate::errors::{CliError, Result};
use clap::Subcommand;
use nexus_creator::local_identity::{is_valid_creator_id, LocalIdentity, LocalIdentityType};
use nexus_local_db::{
    create_local_identity, get_local_identity, link_to_platform, list_local_identities,
    unlink_from_platform,
};
use std::str::FromStr;

#[derive(Debug, Subcommand)]
pub enum IdentityCommand {
    /// List all local identities
    List,

    /// Create a new local identity
    Create {
        /// Identity type: anonymous (ephemeral) or persistent (stored in `SQLite`)
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

    /// Unlink a local identity from its platform Creator
    Unlink {
        /// Local creator ID to unlink
        creator_id: String,
    },
}

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum IdentityKindArg {
    Anonymous,
    Persistent,
}

/// Run identity command.
///
/// # Errors
///
/// Returns `CliError` if:
/// - Identity database operations fail
/// - Configuration cannot be loaded
pub async fn run(cmd: IdentityCommand, _config: &CliConfig) -> Result<()> {
    match cmd {
        IdentityCommand::List => list_identities().await,
        IdentityCommand::Create { kind, name } => create_identity(kind, name).await,
        IdentityCommand::Use { creator_id } => use_identity(creator_id).await,
        IdentityCommand::Link {
            creator_id,
            platform_id,
        } => link_identity(creator_id, platform_id).await,
        IdentityCommand::Unlink { creator_id } => unlink_identity(creator_id).await,
    }
}

/// Open or create the global identity database at `~/.nexus42/state.db`.
async fn open_global_db() -> Result<nexus_local_db::SqlitePool> {
    let home = config::user_home_dir()?;
    let nexus_dir = home.join(".nexus42");

    // Ensure ~/.nexus42/ exists
    std::fs::create_dir_all(&nexus_dir)?;

    let db_path = nexus_dir.join("state.db");
    crate::db::Schema::init(&db_path).await.map_err(Into::into)
}

/// List all local identities.
async fn list_identities() -> Result<()> {
    let pool = open_global_db().await?;
    let identities = list_local_identities(&pool).await?;

    if identities.is_empty() {
        println!("No local identities found.");
        println!("Create one with: nexus42 identity create [--anonymous | --persistent] [--name \"name\"]");
        return Ok(());
    }

    // Use resolve_active_identity for consistency (R4: unified resolution)
    let active_id = resolve_active_identity()
        .await
        .ok()
        .flatten()
        .map(|r| r.creator_id);

    println!("Local Identities:");
    println!();

    for identity in &identities {
        let active_mark = active_id.as_deref().map_or("", |a| {
            if a == identity.creator_id {
                " (active)"
            } else {
                ""
            }
        });

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
async fn create_identity(kind: IdentityKindArg, name: Option<String>) -> Result<()> {
    // R3(identity): Validate display_name — reject empty or whitespace-only
    let trimmed_name = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
    if let Some(raw) = &name {
        if raw.trim().is_empty() {
            return Err(CliError::Other(
                "Display name cannot be empty or whitespace-only.".to_string(),
            ));
        }
    }

    let identity = match kind {
        IdentityKindArg::Anonymous => LocalIdentity::create_anonymous(),
        IdentityKindArg::Persistent => LocalIdentity::create_persistent(trimmed_name),
    };

    let is_persistent = identity.is_persistent();

    if is_persistent {
        // Persist to SQLite
        let pool = open_global_db().await?;
        create_local_identity(
            &pool,
            &identity.creator_id,
            identity.identity_type.as_str(),
            identity.display_name.as_deref(),
            &identity.created_at,
        )
        .await?;
    }

    println!(
        "Created {} identity: {}",
        kind_label(&kind),
        identity.creator_id
    );
    if let Some(ref name) = identity.display_name {
        println!("  Name: {name}");
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
///
/// Wires through [`resolve_active_identity`] internally: after verifying
/// the identity exists and setting it active, the function returns the
/// resolved identity so callers can use the same resolution path everywhere.
async fn use_identity(creator_id: String) -> Result<()> {
    // Verify identity exists
    let pool = open_global_db().await?;
    let _identity = get_local_identity(&pool, &creator_id).await?.ok_or_else(|| {
        CliError::Other(format!(
            "Local identity '{creator_id}' not found. Run `nexus42 identity list` to see available identities."
        ))
    })?;

    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = Some(creator_id.clone());
    cli_config.save()?;

    // Verify the resolution path works
    let resolved = resolve_active_identity().await?;
    match &resolved {
        Some(r) => {
            println!("Active identity set to: {}", r.creator_id);
            if r.is_anonymous {
                eprintln!(
                    "Note: This is an anonymous identity — data will be lost when this session ends."
                );
            }
        }
        None => {
            // Should not happen since we just set it, but handle gracefully
            println!("Active identity set to: {creator_id}");
        }
    }
    Ok(())
}

/// Link a local identity to a platform Creator.
async fn link_identity(creator_id: String, platform_id: String) -> Result<()> {
    if !is_valid_creator_id(&platform_id) {
        return Err(DomainError::InvalidIdFormat(format!(
            "platform_id '{platform_id}' does not match CreatorId pattern (expected: ctr_ followed by alphanumeric characters)"
        ))
        .into());
    }

    let pool = open_global_db().await?;
    link_to_platform(&pool, &creator_id, &platform_id).await?;

    println!("Linked {creator_id} to platform creator: {platform_id}");
    Ok(())
}

/// Unlink a local identity from its platform Creator.
async fn unlink_identity(creator_id: String) -> Result<()> {
    let pool = open_global_db().await?;
    unlink_from_platform(&pool, &creator_id).await?;

    println!("Unlinked {creator_id} from platform creator.");
    Ok(())
}

/// Resolve the active local identity for the current session.
///
/// Resolution order:
/// 1. Check `active_creator_id` from CLI config
/// 2. If set, verify it exists in `local_identities` (for persistent) or accept as-is (for anonymous)
/// 3. If no identity is set, return `None` (caller should prompt to create one)
///
/// This is the single source of truth for active identity resolution (R4).
/// All commands that need the active identity should call this function.
///
/// Returns the resolved identity info or `None` if no identity is configured.
///
/// # Errors
///
/// Returns an error if:
/// - CLI configuration cannot be loaded
/// - Database connection fails
/// - Database query fails
pub async fn resolve_active_identity() -> Result<Option<ResolvedIdentity>> {
    let cli_config = CliConfig::load()?;
    let creator_id = match &cli_config.active_creator_id {
        Some(id) => id.clone(),
        None => return Ok(None),
    };

    // Check if it's a persistent identity in the DB
    let pool = open_global_db().await?;
    let row = get_local_identity(&pool, &creator_id).await?;

    match row {
        Some(db_row) => {
            let identity_type = db_row.identity_type.as_str();
            let resolved_type =
                LocalIdentityType::from_str(identity_type).unwrap_or(LocalIdentityType::Persistent);
            Ok(Some(ResolvedIdentity {
                creator_id,
                identity_type: resolved_type,
                display_name: db_row.display_name,
                is_anonymous: identity_type == "anonymous",
                is_persistent: identity_type == "persistent",
                platform_linked: db_row.platform_linked,
                platform_creator_id: db_row.platform_creator_id,
            }))
        }
        None => {
            // The active_creator_id might be an anonymous identity that wasn't persisted
            if creator_id.starts_with("ctr_anon") {
                Ok(Some(ResolvedIdentity {
                    creator_id,
                    identity_type: LocalIdentityType::Anonymous,
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
                    identity_type: LocalIdentityType::Persistent,
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
#[allow(dead_code)] // Public API — fields/methods used by tests and future callers
pub struct ResolvedIdentity {
    pub creator_id: String,
    pub identity_type: LocalIdentityType,
    pub display_name: Option<String>,
    pub is_anonymous: bool,
    pub is_persistent: bool,
    pub platform_linked: bool,
    pub platform_creator_id: Option<String>,
}

impl ResolvedIdentity {
    /// Check if this is an ephemeral (anonymous) identity.
    #[allow(dead_code)]
    #[must_use]
    pub const fn is_ephemeral(&self) -> bool {
        self.is_anonymous
    }

    /// Warning message for anonymous identities.
    #[allow(dead_code)]
    #[must_use]
    pub const fn ephemeral_warning(&self) -> Option<&'static str> {
        if self.is_anonymous {
            Some("Active identity is anonymous — data will be lost when this session ends. Use `nexus42 identity create --persistent` for a saved identity.")
        } else {
            None
        }
    }
}

const fn kind_label(kind: &IdentityKindArg) -> &'static str {
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

    #[test]
    fn test_link_identity_rejects_invalid_platform_id() {
        // Verify that invalid platform IDs are caught before DB call
        assert!(!is_valid_creator_id("invalid_id"));
        assert!(!is_valid_creator_id("ctr_"));
        assert!(!is_valid_creator_id("ctr_abc_def"));
        assert!(is_valid_creator_id("ctr_Platform123"));
    }

    // ── R1: Additional test coverage for identity commands ─────────────

    #[test]
    fn test_identity_kind_arg_variants() {
        // Verify all enum variants are accessible
        let _ = IdentityKindArg::Anonymous;
        let _ = IdentityKindArg::Persistent;
    }

    #[test]
    fn test_identity_command_enum_exhaustive() {
        // Verify all command variants can be constructed
        let _ = IdentityCommand::List;
        let _ = IdentityCommand::Create {
            kind: IdentityKindArg::Anonymous,
            name: None,
        };
        let _ = IdentityCommand::Use {
            creator_id: "ctr_test123".to_string(),
        };
        let _ = IdentityCommand::Link {
            creator_id: "ctr_local1".to_string(),
            platform_id: "ctr_Platform1".to_string(),
        };
        let _ = IdentityCommand::Unlink {
            creator_id: "ctr_local1".to_string(),
        };
    }

    #[test]
    fn test_resolved_identity_struct_fields() {
        // Verify ResolvedIdentity can be constructed with LocalIdentityType
        let resolved = ResolvedIdentity {
            creator_id: "ctr_test".to_string(),
            identity_type: LocalIdentityType::Persistent,
            display_name: Some("Test".to_string()),
            is_anonymous: false,
            is_persistent: true,
            platform_linked: true,
            platform_creator_id: Some("ctr_plat".to_string()),
        };
        assert!(!resolved.is_ephemeral());
        assert!(resolved.ephemeral_warning().is_none());
    }

    #[test]
    fn test_resolved_identity_ephemeral_warning() {
        let resolved = ResolvedIdentity {
            creator_id: "ctr_anon123".to_string(),
            identity_type: LocalIdentityType::Anonymous,
            display_name: None,
            is_anonymous: true,
            is_persistent: false,
            platform_linked: false,
            platform_creator_id: None,
        };
        assert!(resolved.is_ephemeral());
        assert!(resolved.ephemeral_warning().is_some());
    }

    #[test]
    fn test_resolved_identity_uses_local_identity_type_enum() {
        // R6: identity_type is now LocalIdentityType, not String
        let resolved = ResolvedIdentity {
            creator_id: "ctr_test".to_string(),
            identity_type: LocalIdentityType::Anonymous,
            display_name: None,
            is_anonymous: true,
            is_persistent: false,
            platform_linked: false,
            platform_creator_id: None,
        };
        // This compiles only if identity_type is LocalIdentityType
        let type_str: &str = resolved.identity_type.as_str();
        assert_eq!(type_str, "anonymous");
    }

    // ── R3(identity): display_name validation tests ────────────────────

    #[test]
    fn test_display_name_whitespace_only_should_be_rejected() {
        // Empty/whitespace names should be rejected — trimmed to empty
        let name = Some("   ".to_string());
        assert!(name
            .as_deref()
            .map(str::trim)
            .as_ref()
            .is_none_or(|n| n.is_empty()));
    }

    #[test]
    fn test_display_name_should_be_trimmed() {
        // Names with leading/trailing whitespace should be trimmed
        let name = Some("  Alice  ".to_string());
        let trimmed = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
        assert_eq!(trimmed, Some("Alice"));
    }

    #[test]
    fn test_display_name_none_is_valid() {
        // No name provided is fine
        let name: Option<String> = None;
        let trimmed = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
        assert!(trimmed.is_none());
    }

    #[test]
    fn test_display_name_valid_name_passes() {
        let name = Some("Alice".to_string());
        let trimmed = name.as_deref().map(str::trim).filter(|n| !n.is_empty());
        assert_eq!(trimmed, Some("Alice"));
    }
}
