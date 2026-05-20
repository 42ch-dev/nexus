//! Creator registration and management handlers (V1.20 Batch 5, T27–T32).
//!
//! Endpoints:
//! - `POST /v1/local/creators/registrations` — initiate platform registration
//! - `POST /v1/local/creators/registrations/{code}:verify` — submit challenge answer
//! - `GET /v1/local/creators/{creator_id}` — creator status/detail
//! - `PUT /v1/local/creators/active` — set active creator
//! - `GET /v1/local/creators/active` — get active creator
//! - `POST /v1/local/creators/{id}:logout` — clear credentials

#![allow(clippy::missing_errors_doc)]

use crate::api::errors::NexusApiError;
use crate::workspace::WorkspaceState;
use axum::extract::{Path, State};
use axum::Json;
use nexus_home_layout::validate_creator_id_safe;
use serde::{Deserialize, Serialize};
use tracing::info;

// ─── Request / Response types ──────────────────────────────────────────────

#[derive(Debug, Deserialize)]
pub struct InitiateRegistrationRequest {
    pub name: String,
    #[serde(default = "default_source")]
    pub source: String,
    pub handle: Option<String>,
}

fn default_source() -> String {
    "cli".to_string()
}

#[derive(Debug, Serialize)]
pub struct RegistrationChallenge {
    pub creator_id: String,
    pub verification_code: String,
    pub challenge_text: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
pub struct VerifyRegistrationRequest {
    pub answer: String,
}

#[derive(Debug, Serialize)]
pub struct VerifyRegistrationResponse {
    pub creator_id: String,
    pub verified: bool,
    pub message: String,
}

#[derive(Debug, Serialize)]
pub struct CreatorDetail {
    pub creator_id: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
    pub has_api_key: bool,
    pub has_cached_token: bool,
    pub is_active: bool,
}

#[derive(Debug, Serialize)]
pub struct ListCreatorsResponse {
    pub creators: Vec<CreatorDetail>,
}

#[derive(Debug, Deserialize)]
pub struct SetActiveCreatorRequest {
    pub creator_id: String,
}

#[derive(Debug, Serialize)]
pub struct ActiveCreatorResponse {
    pub creator_id: String,
    pub handle: Option<String>,
    pub display_name: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct SetActiveCreatorResponse {
    pub creator_id: String,
}

#[derive(Debug, Serialize)]
pub struct LogoutResponse {
    pub creator_id: String,
    pub cleared: bool,
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Read the CLI config from `nexus_home`.
fn read_cli_config(nexus_home: &std::path::Path) -> Result<toml::Value, NexusApiError> {
    let config_path = nexus_home.join("config.toml");
    if !config_path.exists() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    let content = std::fs::read_to_string(&config_path).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_READ_ERROR".into(),
        message: e.to_string(),
    })?;
    if content.trim().is_empty() {
        return Ok(toml::Value::Table(toml::map::Map::new()));
    }
    toml::from_str(&content).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_PARSE_ERROR".into(),
        message: e.to_string(),
    })
}

/// Write CLI config to `nexus_home`.
fn write_cli_config(
    nexus_home: &std::path::Path,
    config: &toml::Value,
) -> Result<(), NexusApiError> {
    let config_path = nexus_home.join("config.toml");
    let toml_str = toml::to_string_pretty(config).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_SERIALIZE_ERROR".into(),
        message: e.to_string(),
    })?;
    std::fs::write(&config_path, toml_str).map_err(|e| NexusApiError::Internal {
        code: "CONFIG_WRITE_ERROR".into(),
        message: e.to_string(),
    })
}

/// Read active `creator_id` from CLI config.
fn read_active_creator_id(nexus_home: &std::path::Path) -> Option<String> {
    let config = read_cli_config(nexus_home).ok()?;
    config
        .get("active_creator_id")
        .and_then(|v| v.as_str())
        .map(std::string::ToString::to_string)
}

/// Set active `creator_id` in CLI config.
fn set_active_creator_id(
    nexus_home: &std::path::Path,
    creator_id: &str,
) -> Result<(), NexusApiError> {
    validate_creator_id_safe(creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    let mut config = read_cli_config(nexus_home)?;
    let table = config
        .as_table_mut()
        .ok_or_else(|| NexusApiError::Internal {
            code: "CONFIG_ERROR".into(),
            message: "config root is not a table".to_string(),
        })?;

    table.insert(
        "active_creator_id".to_string(),
        toml::Value::String(creator_id.to_string()),
    );

    write_cli_config(nexus_home, &config)
}

/// Load the creator identity cache from the daemon-readable location.
fn load_identity_cache() -> serde_json::Value {
    let Some(home) = dirs::home_dir() else {
        return serde_json::Value::Null;
    };
    let cache_path = home.join(".nexus42").join("creator_identity_cache.json");
    if !cache_path.exists() {
        return serde_json::Value::Null;
    }
    let Ok(content) = std::fs::read_to_string(&cache_path) else {
        return serde_json::Value::Null;
    };
    serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
}

/// Get identity cache entry for a creator.
fn get_identity_entry(cache: &serde_json::Value, creator_id: &str) -> Option<IdentityEntry> {
    let creators = cache.get("creators")?.as_object()?;
    let entry = creators.get(creator_id)?;
    Some(IdentityEntry {
        handle: entry
            .get("handle")
            .and_then(|v| v.as_str())
            .map(String::from),
        display_name: entry
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(String::from),
    })
}

#[derive(Clone)]
struct IdentityEntry {
    handle: Option<String>,
    display_name: Option<String>,
}

/// Load the auth store to check credentials.
fn load_auth_store() -> serde_json::Value {
    let Some(home) = dirs::home_dir() else {
        return serde_json::Value::Null;
    };
    // Auth store lives at ~/.nexus42/auth.json
    let auth_path = home.join(".nexus42").join("auth.json");
    if !auth_path.exists() {
        return serde_json::Value::Null;
    }
    let Ok(content) = std::fs::read_to_string(&auth_path) else {
        return serde_json::Value::Null;
    };
    serde_json::from_str(&content).unwrap_or(serde_json::Value::Null)
}

/// Check if a creator has an API key stored.
fn has_creator_api_key(auth_store: &serde_json::Value, creator_id: &str) -> bool {
    auth_store
        .get("creators")
        .and_then(|c| c.get(creator_id))
        .and_then(|e| e.get("creator_api_key"))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// Check if a creator has a cached access token.
fn has_cached_token(auth_store: &serde_json::Value, creator_id: &str) -> bool {
    auth_store
        .get("creators")
        .and_then(|c| c.get(creator_id))
        .and_then(|e| e.get("access_token"))
        .and_then(|v| v.as_str())
        .is_some_and(|s| !s.is_empty())
}

/// Remove a creator's credentials from the auth store.
fn clear_creator_credentials(creator_id: &str) -> Result<bool, NexusApiError> {
    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;
    let auth_path = home.join(".nexus42").join("auth.json");
    if !auth_path.exists() {
        return Ok(false);
    }

    let content = std::fs::read_to_string(&auth_path).map_err(|e| NexusApiError::Internal {
        code: "AUTH_READ_ERROR".into(),
        message: e.to_string(),
    })?;
    let mut store: serde_json::Value =
        serde_json::from_str(&content).map_err(|e| NexusApiError::Internal {
            code: "AUTH_PARSE_ERROR".into(),
            message: e.to_string(),
        })?;

    let removed = store
        .get_mut("creators")
        .and_then(|c| c.as_object_mut())
        .is_some_and(|creators| creators.remove(creator_id).is_some());

    if removed {
        let json = serde_json::to_string_pretty(&store).map_err(|e| NexusApiError::Internal {
            code: "AUTH_SERIALIZE_ERROR".into(),
            message: e.to_string(),
        })?;
        std::fs::write(&auth_path, json).map_err(|e| NexusApiError::Internal {
            code: "AUTH_WRITE_ERROR".into(),
            message: e.to_string(),
        })?;
    }

    Ok(removed)
}

// ─── Handler helpers (extracted to keep handlers under 100 lines) ─────────

/// Validate the `name` and optional `handle` fields of a registration request.
fn validate_registration_fields(req: &InitiateRegistrationRequest) -> Result<(), NexusApiError> {
    if req.name.is_empty() {
        return Err(NexusApiError::InvalidInput {
            field: "name".to_string(),
            reason: "must not be empty".to_string(),
        });
    }
    if req.name.len() > 64 {
        return Err(NexusApiError::InvalidInput {
            field: "name".to_string(),
            reason: "must not exceed 64 characters".to_string(),
        });
    }

    if let Some(ref handle) = req.handle {
        static HANDLE_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
            regex::Regex::new(r"^[a-z0-9][a-z0-9._-]{2,13}[a-z0-9]$")
                .expect("handle regex is valid")
        });
        if !HANDLE_RE.is_match(handle) {
            return Err(NexusApiError::InvalidInput {
                field: "handle".to_string(),
                reason: "must be 4–15 chars, start/end with a-z0-9, interior allows a-z0-9._-"
                    .to_string(),
            });
        }
    }
    Ok(())
}

/// Read platform auth token from the auth store.
fn read_platform_auth_token() -> Result<String, NexusApiError> {
    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;
    let auth_path = home.join(".nexus42").join("auth.json");
    let auth_content =
        std::fs::read_to_string(&auth_path).map_err(|e| NexusApiError::Internal {
            code: "AUTH_READ_ERROR".into(),
            message: format!("Cannot read auth store: {e}"),
        })?;
    let auth_store: serde_json::Value =
        serde_json::from_str(&auth_content).map_err(|e| NexusApiError::Internal {
            code: "AUTH_PARSE_ERROR".into(),
            message: format!("Cannot parse auth store: {e}"),
        })?;

    auth_store
        .get("creators")
        .and_then(|c| c.as_object())
        .and_then(|obj| {
            obj.values().find_map(|v| {
                v.get("access_token")
                    .and_then(|t| t.as_str())
                    .filter(|s| !s.is_empty())
            })
        })
        .map(String::from)
        .ok_or(NexusApiError::AuthRequired)
}

/// Build a `PlatformClient` using CLI config and the given auth token.
fn build_platform_client(
    nexus_home: &std::path::Path,
    auth_token: &str,
) -> Result<nexus_cloud_sync::platform_client::PlatformClient, NexusApiError> {
    let cli_config = read_cli_config(nexus_home)?;
    let platform_url = cli_config
        .get("platform_url")
        .and_then(|v| v.as_str())
        .unwrap_or("https://api.nexus42.com");
    let device_id = cli_config
        .get("device_id")
        .and_then(|v| v.as_str())
        .unwrap_or("daemon");

    nexus_cloud_sync::platform_client::PlatformClient::new(platform_url, auth_token, device_id)
        .map_err(|e| NexusApiError::Internal {
            code: "PLATFORM_CLIENT_ERROR".into(),
            message: e.to_string(),
        })
}

/// Persist pending registration data to disk for the verify step.
fn save_pending_registration(
    nexus_home: &std::path::Path,
    creator_id: &str,
    pending_api_key: &str,
    name: &str,
    handle: Option<&String>,
    verification_code: &str,
) -> Result<(), NexusApiError> {
    let pending_path = nexus_home.join("pending_registration.json");
    let pending_data = serde_json::json!({
        "creator_id": creator_id,
        "pending_api_key": pending_api_key,
        "name": name,
        "handle": handle,
        "verification_code": verification_code,
    });
    std::fs::write(
        &pending_path,
        serde_json::to_string_pretty(&pending_data).map_err(|e| NexusApiError::Internal {
            code: "JSON_ERROR".into(),
            message: e.to_string(),
        })?,
    )
    .map_err(|e| NexusApiError::Internal {
        code: "FILE_WRITE_ERROR".into(),
        message: e.to_string(),
    })
}

/// Store creator credentials in the auth store after verification.
fn store_creator_credentials(
    auth_path: &std::path::Path,
    auth_content: &str,
    creator_id: &str,
    api_key: &str,
) -> Result<(), NexusApiError> {
    let mut store: serde_json::Value = serde_json::from_str(auth_content).unwrap_or_default();
    if store.is_null() || store.as_object_mut().is_none() {
        store = serde_json::json!({});
    }
    let store_obj = store.as_object_mut().expect("guaranteed to be object");
    let creators = store_obj
        .entry("creators")
        .or_insert_with(|| serde_json::json!({}));
    creators
        .as_object_mut()
        .expect("guaranteed to be object")
        .insert(
            creator_id.to_string(),
            serde_json::json!({
                "creator_id": creator_id,
                "creator_api_key": api_key,
                "access_token": "",
                "expires_at": "",
            }),
        );
    std::fs::write(
        auth_path,
        serde_json::to_string_pretty(&store).map_err(|e| NexusApiError::Internal {
            code: "AUTH_SERIALIZE_ERROR".into(),
            message: e.to_string(),
        })?,
    )
    .map_err(|e| NexusApiError::Internal {
        code: "AUTH_WRITE_ERROR".into(),
        message: e.to_string(),
    })
}

/// Update the identity cache with a newly verified creator.
fn update_identity_cache(
    home: &std::path::Path,
    creator_id: &str,
    name: &str,
    handle: Option<&str>,
) -> Result<(), NexusApiError> {
    let cache_path = home.join(".nexus42").join("creator_identity_cache.json");
    let mut cache: serde_json::Value = if cache_path.exists() {
        std::fs::read_to_string(&cache_path)
            .ok()
            .and_then(|c| serde_json::from_str(&c).ok())
            .unwrap_or_else(|| serde_json::json!({}))
    } else {
        serde_json::json!({"creators": {}})
    };
    if cache.get("creators").is_none() {
        cache = serde_json::json!({"creators": {}});
    }
    if let Some(creators) = cache.get_mut("creators").and_then(|c| c.as_object_mut()) {
        creators.insert(
            creator_id.to_string(),
            serde_json::json!({
                "creator_id": creator_id,
                "handle": handle,
                "display_name": name,
            }),
        );
    }
    if let Some(parent) = cache_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| NexusApiError::Internal {
            code: "DIR_CREATE_ERROR".into(),
            message: e.to_string(),
        })?;
    }
    std::fs::write(
        &cache_path,
        serde_json::to_string_pretty(&cache).map_err(|e| NexusApiError::Internal {
            code: "CACHE_SERIALIZE_ERROR".into(),
            message: e.to_string(),
        })?,
    )
    .map_err(|e| NexusApiError::Internal {
        code: "CACHE_WRITE_ERROR".into(),
        message: e.to_string(),
    })
}

// ─── Handlers ──────────────────────────────────────────────────────────────

/// `POST /v1/local/creators/registrations` — initiate platform registration (T27)
///
/// Initiates platform creator registration and returns challenge metadata.
/// The CLI/web UI displays the challenge; the daemon owns platform calls.
pub async fn initiate_registration(
    State(state): State<WorkspaceState>,
    Json(req): Json<InitiateRegistrationRequest>,
) -> Result<Json<RegistrationChallenge>, NexusApiError> {
    info!(
        name = %req.name,
        source = %req.source,
        "Initiating creator registration"
    );

    validate_registration_fields(&req)?;

    let auth_token = read_platform_auth_token()?;
    let client = build_platform_client(state.nexus_home(), &auth_token)?;

    let handle_ref = req.handle.as_deref();
    let response = client
        .register_creator(&req.name, &req.source, handle_ref)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "REGISTRATION_FAILED".into(),
            message: format!("Platform registration failed: {e}"),
        })?;

    let creator_id = response.creator_id.clone();
    let pending_key = response.creator_api_key.clone();
    let verification = response.verification;

    save_pending_registration(
        state.nexus_home(),
        &creator_id,
        &pending_key,
        &req.name,
        req.handle.as_ref(),
        &verification.verification_code,
    )?;

    Ok(Json(RegistrationChallenge {
        creator_id,
        verification_code: verification.verification_code,
        challenge_text: verification.challenge_text,
        expires_at: verification.expires_at,
    }))
}

/// `POST /v1/local/creators/registrations/{code}:verify` — submit challenge answer (T28)
///
/// Submits the challenge answer to the platform. On success, stores
/// creator credentials locally and sets the creator as active.
pub async fn verify_registration(
    State(state): State<WorkspaceState>,
    Path(code): Path<String>,
    Json(req): Json<VerifyRegistrationRequest>,
) -> Result<Json<VerifyRegistrationResponse>, NexusApiError> {
    info!(code = %code, "Verifying creator registration");

    let (pending, pending_path) = load_pending_registration(state.nexus_home(), &code)?;
    let creator_id = pending
        .get("creator_id")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let pending_api_key = pending
        .get("pending_api_key")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();

    // Submit verification to platform
    let auth_token = read_platform_auth_token()?;
    let client = build_platform_client(state.nexus_home(), &auth_token)?;
    let verify_response = client
        .verify_creator(&code, &req.answer)
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "VERIFICATION_FAILED".into(),
            message: format!("Platform verification failed: {e}"),
        })?;

    match verify_response.status {
        nexus_cloud_sync::platform_client::VerifyStatus::Verified => {
            finalize_verified_registration(
                &state,
                &pending,
                &creator_id,
                &pending_api_key,
                verify_response.creator_api_key.as_ref(),
            )?;

            Ok(Json(VerifyRegistrationResponse {
                creator_id,
                verified: true,
                message: "Creator registered and set as active".to_string(),
            }))
        }
        nexus_cloud_sync::platform_client::VerifyStatus::WrongAnswer => {
            Ok(Json(VerifyRegistrationResponse {
                creator_id,
                verified: false,
                message: format!(
                    "Incorrect answer. {} attempts remaining.",
                    verify_response.remaining_attempts.unwrap_or(0)
                ),
            }))
        }
        nexus_cloud_sync::platform_client::VerifyStatus::Expired => {
            let _ = std::fs::remove_file(&pending_path);
            Err(NexusApiError::InvalidInput {
                field: "answer".to_string(),
                reason: "Challenge has expired. Start a new registration.".to_string(),
            })
        }
        nexus_cloud_sync::platform_client::VerifyStatus::Locked => {
            let _ = std::fs::remove_file(&pending_path);
            Err(NexusApiError::Forbidden {
                resource: "creator_registration".to_string(),
                reason: "Account is permanently locked due to too many failed attempts".to_string(),
            })
        }
    }
}

/// Load and validate the pending registration file for the given verification code.
fn load_pending_registration(
    nexus_home: &std::path::Path,
    code: &str,
) -> Result<(serde_json::Value, std::path::PathBuf), NexusApiError> {
    let pending_path = nexus_home.join("pending_registration.json");
    if !pending_path.exists() {
        return Err(NexusApiError::NotFound(
            "No pending registration found. Start with POST /creators/registrations first."
                .to_string(),
        ));
    }

    let pending_content =
        std::fs::read_to_string(&pending_path).map_err(|e| NexusApiError::Internal {
            code: "FILE_READ_ERROR".into(),
            message: e.to_string(),
        })?;
    let pending: serde_json::Value =
        serde_json::from_str(&pending_content).map_err(|e| NexusApiError::Internal {
            code: "JSON_PARSE_ERROR".into(),
            message: e.to_string(),
        })?;

    let stored_code = pending
        .get("verification_code")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if stored_code != code {
        return Err(NexusApiError::InvalidInput {
            field: "verification_code".to_string(),
            reason: "code does not match the pending registration".to_string(),
        });
    }

    Ok((pending, pending_path))
}

/// Complete a verified registration: store credentials, update identity cache, set active.
fn finalize_verified_registration(
    state: &WorkspaceState,
    pending: &serde_json::Value,
    creator_id: &str,
    pending_api_key: &str,
    response_api_key: Option<&String>,
) -> Result<(), NexusApiError> {
    let api_key = response_api_key.map_or(pending_api_key, String::as_str);

    // Store credentials in auth store
    let home = dirs::home_dir().ok_or_else(|| NexusApiError::Internal {
        code: "HOME_DIR_ERROR".into(),
        message: "Cannot determine home directory".to_string(),
    })?;
    let auth_path = home.join(".nexus42").join("auth.json");
    let auth_content = std::fs::read_to_string(&auth_path).unwrap_or_default();
    store_creator_credentials(&auth_path, &auth_content, creator_id, api_key)?;

    // Update identity cache
    let name = pending.get("name").and_then(|v| v.as_str()).unwrap_or("");
    let handle = pending.get("handle").and_then(|v| v.as_str());
    update_identity_cache(&home, creator_id, name, handle)?;

    // Set as active creator
    set_active_creator_id(state.nexus_home(), creator_id)?;

    // Clean up pending registration
    let pending_path = state.nexus_home().join("pending_registration.json");
    let _ = std::fs::remove_file(&pending_path);

    info!(creator_id = %creator_id, "Creator registration verified");
    Ok(())
}

/// `GET /v1/local/creators/{creator_id}` — creator status/detail (T29)
pub async fn get_creator(
    State(state): State<WorkspaceState>,
    Path(creator_id): Path<String>,
) -> Result<Json<CreatorDetail>, NexusApiError> {
    info!(creator_id = %creator_id, "Getting creator detail");

    validate_creator_id_safe(&creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    let cache = load_identity_cache();
    let entry = get_identity_entry(&cache, &creator_id);
    let auth_store = load_auth_store();
    let active_id = read_active_creator_id(state.nexus_home());

    Ok(Json(CreatorDetail {
        creator_id: creator_id.clone(),
        handle: entry.as_ref().and_then(|e| e.handle.clone()),
        display_name: entry.as_ref().and_then(|e| e.display_name.clone()),
        has_api_key: has_creator_api_key(&auth_store, &creator_id),
        has_cached_token: has_cached_token(&auth_store, &creator_id),
        is_active: active_id.as_deref() == Some(creator_id.as_str()),
    }))
}

/// `PUT /v1/local/creators/active` — set active creator (T30)
pub async fn set_active_creator(
    State(state): State<WorkspaceState>,
    Json(req): Json<SetActiveCreatorRequest>,
) -> Result<Json<SetActiveCreatorResponse>, NexusApiError> {
    info!(creator_id = %req.creator_id, "Setting active creator");

    validate_creator_id_safe(&req.creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    // Verify the creator has credentials stored
    let auth_store = load_auth_store();
    let cache = load_identity_cache();

    // Check if creator exists in either auth store or identity cache
    let in_auth = auth_store
        .get("creators")
        .and_then(|c| c.as_object())
        .is_some_and(|obj| obj.contains_key(&req.creator_id));
    let in_cache = get_identity_entry(&cache, &req.creator_id).is_some();

    if !in_auth && !in_cache {
        return Err(NexusApiError::NotFound(format!(
            "Creator {} not found. Register first.",
            req.creator_id
        )));
    }

    set_active_creator_id(state.nexus_home(), &req.creator_id)?;

    // Clear workspace slug for this creator in config (reset to default)
    let mut config = read_cli_config(state.nexus_home())?;
    if let Some(table) = config.as_table_mut() {
        if let Some(slug_table) = table.get_mut("active_workspace_slug_by_creator") {
            if let Some(slugs) = slug_table.as_table_mut() {
                slugs.remove(&req.creator_id);
            }
        }
        write_cli_config(state.nexus_home(), &config)?;
    }

    Ok(Json(SetActiveCreatorResponse {
        creator_id: req.creator_id,
    }))
}

/// `GET /v1/local/creators/active` — get active creator (T31)
pub async fn get_active_creator(
    State(state): State<WorkspaceState>,
) -> Result<Json<ActiveCreatorResponse>, NexusApiError> {
    let creator_id =
        read_active_creator_id(state.nexus_home()).ok_or(NexusApiError::Uninitialized)?;

    let cache = load_identity_cache();
    let entry = get_identity_entry(&cache, &creator_id);

    Ok(Json(ActiveCreatorResponse {
        creator_id,
        handle: entry.as_ref().and_then(|e| e.handle.clone()),
        display_name: entry.and_then(|e| e.display_name),
    }))
}

/// `POST /v1/local/creators/{id}:logout` — clear credentials (T32)
pub async fn logout_creator(
    State(state): State<WorkspaceState>,
    Path(creator_id): Path<String>,
) -> Result<Json<LogoutResponse>, NexusApiError> {
    info!(creator_id = %creator_id, "Logging out creator");

    validate_creator_id_safe(&creator_id).map_err(|reason| NexusApiError::InvalidInput {
        field: "creator_id".to_string(),
        reason,
    })?;

    let cleared = clear_creator_credentials(&creator_id)?;

    // If this was the active creator, clear the active selection
    if let Some(active) = read_active_creator_id(state.nexus_home()) {
        if active == creator_id {
            let mut config = read_cli_config(state.nexus_home())?;
            if let Some(table) = config.as_table_mut() {
                table.remove("active_creator_id");
                write_cli_config(state.nexus_home(), &config)?;
            }
        }
    }

    Ok(Json(LogoutResponse {
        creator_id,
        cleared,
    }))
}

// ─── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_creator_id_rejects_traversal() {
        assert!(validate_creator_id_safe("../etc").is_err());
    }

    #[test]
    fn validate_creator_id_accepts_valid() {
        assert!(validate_creator_id_safe("crt_abc123").is_ok());
    }

    #[tokio::test]
    async fn get_active_without_creator_returns_uninitialized() {
        let tmp = tempfile::TempDir::new().expect("temp dir");
        let nexus_home = tmp.path().join(".nexus42");
        std::fs::create_dir_all(&nexus_home).expect("create");

        let db_path = nexus_home.join("state.db");
        let pool = nexus_local_db::open_pool(&db_path).await.expect("pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("migrate");
        nexus_local_db::seed_versions(&pool).await.expect("seed");

        let state =
            crate::workspace::WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

        let result = get_active_creator(State(state)).await;
        assert!(result.is_err());
        match result.unwrap_err() {
            NexusApiError::Uninitialized => {}
            other => panic!("Expected Uninitialized, got: {other}"),
        }
    }
}
