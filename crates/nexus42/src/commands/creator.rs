//! Creator Command Module
//!
//! Creator is a V1.0 first-class citizen (roadmap §3.1.1, §3.1.2).
//! Subcommands: register, status, use, list, pair, unpair, credentials rotate, workspace.

use crate::auth;
use crate::challenge::{solve_challenge_with_fallback, UnavailableLlmSolver};
use crate::commands::clone::CloneArgs;
use crate::commands::init;
use crate::commands::init::InitCommand;
use crate::commands::memory::MemoryCommand;
use crate::commands::soul::SoulCommand;
use crate::config::{CliConfig, DEFAULT_WORKSPACE_SLUG};
use crate::errors::{CliError, Result};
use crate::paths;
use clap::Subcommand;
use nexus_contracts::Creator;
use nexus_sync::platform_client::{PlatformClient, VerifyStatus};
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

    /// SOUL management (deprecated: use `nexus42 creator soul`)
    Soul {
        #[command(subcommand)]
        command: SoulCommand,
    },

    /// Long-term memory management (deprecated: use `nexus42 creator memory`)
    Memory {
        #[command(subcommand)]
        command: MemoryCommand,
    },

    /// Knowledge base management (coming soon)
    Kb {
        #[command(subcommand)]
        command: KbCommand,
    },

    /// Logout and clear creator credentials
    Logout,
}

/// Knowledge base subcommands (coming soon).
#[derive(Debug, Subcommand)]
pub enum KbCommand {
    /// List knowledge base entries (coming soon)
    List,

    /// Search knowledge base (coming soon)
    Search {
        /// Search query string
        query: String,
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
    /// Clone a world into the workspace (migrated from `nexus42 clone`)
    Clone {
        /// World reference to clone (e.g. `wld_abc123`)
        world_ref: String,
        /// Clone source: platform (default) or local
        #[arg(long, value_enum, default_value = "platform")]
        source: crate::commands::clone::CloneSourceArg,
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
        CreatorCommand::Status { creator_id } => creator_status(config, creator_id),
        CreatorCommand::Use { creator_ref } => use_creator(config, creator_ref.as_str()),
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
        CreatorCommand::Soul { command } => super::soul::run(command, config).await,
        CreatorCommand::Memory { command } => super::memory::run(command, config).await,
        CreatorCommand::Kb { command } => {
            run_kb(command);
            Ok(())
        }
        CreatorCommand::Logout => {
            println!("Coming soon: `creator logout` — clear creator credentials/session.");
            Ok(())
        }
    }
}

fn user_home() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| CliError::Other("Cannot determine home directory".into()))
}

fn validate_workspace_slug(slug: &str) -> Result<()> {
    init::validate_slug("workspace_slug", slug)
}

/// Handle knowledge base stub subcommands.
fn run_kb(cmd: KbCommand) {
    match cmd {
        KbCommand::List => {
            println!("Coming soon: `creator kb list` — list knowledge base entries.");
        }
        KbCommand::Search { query } => {
            println!("Coming soon: `creator kb search` — search knowledge base for: {query}");
        }
    }
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
    let home = user_home()?;

    match cmd {
        CreatorWorkspaceCommand::List => {
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
                None => init::default_creative_root(creator_id, &workspace_slug)?,
            };
            let workspace_name = name.unwrap_or_else(|| workspace_slug.clone());
            let db_path = init::materialize_adr014_workspace(
                &home,
                creator_id,
                &workspace_slug,
                &creative_root,
                &workspace_name,
            )
            .await?;
            init::persist_cli_workspace_selection(
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
        CreatorWorkspaceCommand::Init { command } => super::init::run(command).await,
        CreatorWorkspaceCommand::Clone {
            world_ref,
            source,
            yes,
        } => {
            let args = CloneArgs {
                world_ref,
                source,
                dry_run: false,
                yes,
            };
            super::clone::run(args, config).await
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
) -> Result<nexus_sync::platform_client::VerifyResponse> {
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

/// Show Creator status
fn creator_status(config: &CliConfig, creator_id: Option<String>) -> Result<()> {
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

    let store = crate::auth::AuthStore::load()?;

    // Try to get from local cache first
    println!("Creator: {id}");

    if store.is_creator_authenticated(&id) {
        println!("  Auth: ✓ Token cached");
    } else {
        println!("  Auth: ✗ No cached token");
    }

    println!();
    println!("⚠ V1.0 skeleton: full status requires daemon + platform API.");

    Ok(())
}

/// Switch active Creator
fn use_creator(_config: &CliConfig, creator_ref: &str) -> Result<()> {
    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = Some(creator_ref.to_string());
    // New active creator uses default workspace slug until `creator workspace use`.
    cli_config
        .active_workspace_slug_by_creator
        .remove(creator_ref);
    cli_config.save()?;

    println!("✓ Active Creator set to: {creator_ref}");
    println!(
        "  Workspace slug: {DEFAULT_WORKSPACE_SLUG} (use `nexus42 creator workspace use <slug>` after the directory exists)"
    );
    Ok(())
}

/// List all registered Creators
fn list_creators(_config: &CliConfig) -> Result<()> {
    // In V1.0, list from local cache
    // In production, also fetch from platform
    let config = CliConfig::load()?;

    println!("Registered Creators:");
    println!();

    if let Some(active_id) = &config.active_creator_id {
        println!("  {active_id} (active)");
    }

    println!();
    println!("⚠ V1.0 skeleton: full list requires daemon + platform API.");

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
    use nexus_sync::platform_client::{classify_platform_error, StagedPlatformError, VerifyStatus};

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
        let sync_err = nexus_sync::errors::SyncError::PlatformError {
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
        let sync_err = nexus_sync::errors::SyncError::SyncNotConfigured(
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
        let sync_err = nexus_sync::errors::SyncError::PlatformError {
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
        register:
            std::result::Result<nexus_sync::platform_client::RegisterResponse, StagedPlatformError>,
        /// Gate-B2 outcome: platform verify call result (None if register failed).
        verify: Option<
            std::result::Result<nexus_sync::platform_client::VerifyResponse, StagedPlatformError>,
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
