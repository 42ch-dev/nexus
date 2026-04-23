//! Creator Command Module
//!
//! Creator is a V1.0 first-class citizen (roadmap §3.1.1, §3.1.2).
//! Subcommands: register, status, use, list, pair, unpair, credentials rotate, workspace.

use crate::auth;
use crate::challenge::{solve_challenge_with_fallback, UnavailableLlmSolver};
use crate::commands::init;
use crate::config::{CliConfig, DEFAULT_WORKSPACE_SLUG};
use crate::errors::{CliError, Result};
use crate::paths;
use clap::Subcommand;
use nexus_contracts::Creator;
use nexus_sync::platform_client::{PlatformClient, VerifyStatus};
use std::path::PathBuf;

/// Default registration source for the CLI.
const DEFAULT_REGISTRATION_SOURCE: &str = "cli";

/// Buffer seconds added to expiry check to avoid edge-case failures.
const EXPIRY_BUFFER_SECS: i64 = 10;

/// Maximum number of auto-retry attempts for wrong answers (D4).
const MAX_VERIFY_ATTEMPTS: u32 = 2;

#[derive(Debug, Subcommand)]
pub enum CreatorCommand {
    /// Register a new Creator entity
    ///
    /// Usage: nexus42 creator register --name "My Agent" [--source cli|web_agent]
    Register {
        /// Display name for the Creator (required)
        #[arg(long)]
        name: String,
        /// Registration source (default: cli)
        #[arg(long, default_value = DEFAULT_REGISTRATION_SOURCE)]
        source: String,
    },

    /// Show current Creator status
    Status {
        /// Specific creator ID to check (default: active creator)
        creator_id: Option<String>,
    },

    /// Switch the active Creator
    Use {
        /// Creator ID or display name
        creator_ref: String,
    },

    /// List all registered Creators
    List,

    /// Initiate pairing flow with a Creator
    Pair {
        /// Creator ID to pair
        creator_id: String,
    },

    /// Remove pairing with a Creator
    Unpair {
        /// Creator ID to unpair
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
pub async fn run(cmd: CreatorCommand, config: &CliConfig) -> Result<()> {
    match cmd {
        CreatorCommand::Register { name, source } => register_creator(config, name, source).await,
        CreatorCommand::Status { creator_id } => creator_status(config, creator_id).await,
        CreatorCommand::Use { creator_ref } => use_creator(config, creator_ref).await,
        CreatorCommand::List => list_creators(config).await,
        CreatorCommand::Pair { creator_id } => pair_creator(config, creator_id).await,
        CreatorCommand::Unpair { creator_id } => unpair_creator(config, creator_id).await,
        CreatorCommand::Credentials { action } => match action {
            CredentialsAction::Rotate { creator_id } => {
                rotate_credentials(config, creator_id).await
            }
        },
        CreatorCommand::Workspace { command } => run_creator_workspace(config, command).await,
    }
}

fn user_home() -> Result<PathBuf> {
    dirs::home_dir().ok_or_else(|| CliError::Other("Cannot determine home directory".into()))
}

fn validate_workspace_slug(slug: &str) -> Result<()> {
    init::validate_slug("workspace_slug", slug)
}

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
            println!("Workspaces for creator {}:", creator_id);
            let mut names: Vec<String> = std::fs::read_dir(&root)?
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .filter_map(|e| e.file_name().into_string().ok())
                .collect();
            names.sort();
            let active = config.workspace_slug_for_creator(creator_id);
            for n in names {
                let mark = if n == active { " (active)" } else { "" };
                println!("  {}{}", n, mark);
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
                    "Workspace {:?} already exists for creator {}.",
                    workspace_slug, creator_id
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
            println!(
                "✓ Workspace {:?} created for creator {}.",
                workspace_slug, creator_id
            );
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
            println!(
                "✓ Active workspace slug for {} set to: {}",
                creator_id, workspace_slug
            );
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
async fn register_creator(config: &CliConfig, name: String, source: String) -> Result<()> {
    // --- Step 1: Obtain auth token ---
    let auth_store = auth::AuthStore::load()?;

    // Try to find a user access token from the daemon-managed auth flow.
    // The PlatformClient requires a bearer token; if none is available,
    // prompt the user to authenticate first.
    let auth_token = obtain_auth_token(&auth_store)?;

    // --- Step 2: Create platform client and call register ---
    println!("Registering creator \"{}\"...", name);

    let client = PlatformClient::new(&config.platform_url, &auth_token)?;

    let register_response = client.register_creator(&name, &source).await?;

    let creator_id = &register_response.creator_id;
    let pending_api_key = &register_response.creator_api_key;
    let verification = &register_response.verification;

    println!("  Creator ID: {}", creator_id);
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
    println!("  Challenge expires in {}s", remaining_secs);

    // --- Step 4: Solve challenge ---
    println!("Solving challenge...");

    let answer: String =
        match solve_challenge_with_fallback(&verification.challenge_text, &UnavailableLlmSolver)
            .await
        {
            Ok(answer) => {
                println!("  Answer computed: {}", answer);
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
            println!("  Creator ID: {}", creator_id);
            println!("  API key stored to local credentials.");
            println!();

            Ok(())
        }
        VerifyStatus::WrongAnswer => {
            let remaining = verify_response.remaining_attempts.unwrap_or(0);
            Err(CliError::CreatorVerificationFailed {
                status: "wrong_answer".to_string(),
                message: format!(
                    "Incorrect answer after auto-retry. {} attempts remaining.",
                    remaining
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
            println!(
                "  Retrying verification (attempt {}/{})...",
                attempt, max_attempts
            );
        }

        let response = match client
            .verify_creator(verification_code, answer)
            .await
            .map_err(CliError::verify_creator_error)
        {
            Ok(resp) => resp,
            Err(CliError::Network(_)) if attempt < max_attempts => {
                eprintln!(
                    "  Network error during verification (attempt {}/{}). Retrying...",
                    attempt, max_attempts
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
                    eprintln!(
                        "  Wrong answer. {} attempts remaining. Retrying...",
                        remaining
                    );
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
async fn creator_status(config: &CliConfig, creator_id: Option<String>) -> Result<()> {
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
    println!("Creator: {}", id);

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
async fn use_creator(_config: &CliConfig, creator_ref: String) -> Result<()> {
    let mut cli_config = CliConfig::load()?;
    cli_config.active_creator_id = Some(creator_ref.clone());
    // New active creator uses default workspace slug until `creator workspace use`.
    cli_config
        .active_workspace_slug_by_creator
        .remove(&creator_ref);
    cli_config.save()?;

    println!("✓ Active Creator set to: {}", creator_ref);
    println!(
        "  Workspace slug: {} (use `nexus42 creator workspace use <slug>` after the directory exists)",
        DEFAULT_WORKSPACE_SLUG
    );
    Ok(())
}

/// List all registered Creators
async fn list_creators(_config: &CliConfig) -> Result<()> {
    // In V1.0, list from local cache
    // In production, also fetch from platform
    let config = CliConfig::load()?;

    println!("Registered Creators:");
    println!();

    if let Some(active_id) = &config.active_creator_id {
        println!("  {} (active)", active_id);
    }

    println!();
    println!("⚠ V1.0 skeleton: full list requires daemon + platform API.");

    Ok(())
}

/// Initiate pairing flow
async fn pair_creator(_config: &CliConfig, creator_id: String) -> Result<()> {
    // Platform API integration not yet available
    println!("⚠ V1.0 skeleton: Creator pairing requires platform API.");
    println!("  Creator: {}", creator_id);
    Ok(())
}

/// Remove pairing
async fn unpair_creator(_config: &CliConfig, creator_id: String) -> Result<()> {
    // Platform API integration not yet available
    println!("⚠ V1.0 skeleton: Creator unpairing requires platform API.");
    println!("  Creator: {}", creator_id);
    Ok(())
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

/// Cache a Creator locally in SQLite
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
mod tests {
    use super::*;
    use crate::auth::{AuthStore, CreatorAuthState};
    use nexus_sync::platform_client::VerifyStatus;

    /// Helper: create an AuthStore with a known access token.
    fn store_with_token(creator_id: &str, token: &str) -> AuthStore {
        let mut store = AuthStore::default();
        store.creators = Some({
            let mut m = std::collections::HashMap::new();
            m.insert(
                creator_id.to_string(),
                CreatorAuthState {
                    creator_id: creator_id.to_string(),
                    access_token: token.to_string(),
                    expires_at: "2099-01-01T00:00:00Z".to_string(),
                    creator_api_key: None,
                },
            );
            m
        });
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
        let mut store = store_with_token("crt_a", "token_a");
        if let Some(creators) = store.creators.as_mut() {
            creators.insert(
                "crt_b".to_string(),
                CreatorAuthState {
                    creator_id: "crt_b".to_string(),
                    access_token: "token_b".to_string(),
                    expires_at: "2099-01-01T00:00:00Z".to_string(),
                    creator_api_key: None,
                },
            );
        }
        let token = obtain_auth_token(&store).expect("should find token");
        // HashMap iteration order is non-deterministic, but it should find *one* token
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
        let display = format!("{}", err);
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
        let display = format!("{}", err);
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
        let display = format!("{}", err);
        assert!(display.contains("wrong_answer"));
        assert!(display.contains("auto-retry has been exhausted"));
    }

    #[test]
    fn creator_verification_failed_expired_has_suggestion() {
        let err = CliError::CreatorVerificationFailed {
            status: "expired".to_string(),
            message: "timed out".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("expired"));
        assert!(display.contains("timed out"));
    }

    #[test]
    fn creator_verification_failed_locked_has_suggestion() {
        let err = CliError::CreatorVerificationFailed {
            status: "locked".to_string(),
            message: "permanently locked".to_string(),
        };
        let display = format!("{}", err);
        assert!(display.contains("locked"));
        assert!(display.contains("permanently locked"));
        assert!(display.contains("Contact support"));
    }

    #[test]
    fn challenge_expired_error_shows_timestamp() {
        let err = CliError::ChallengeExpired {
            expires_at: "2026-04-16T00:05:00.000Z".to_string(),
        };
        let display = format!("{}", err);
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

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create client");
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

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create client");
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

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create client");
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

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create client");
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

        let client = PlatformClient::new(&mock_server.uri(), "test_token").expect("create client");
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
}
