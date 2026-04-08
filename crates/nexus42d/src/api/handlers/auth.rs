//! Auth handlers — status, device authorization, token exchange, logout

use crate::api::auth_middleware::TokenStatusResponse;
use crate::api::errors::NexusApiError;
use crate::auth::token_manager::TokenManager;
use crate::workspace::WorkspaceState;
use axum::{extract::State, http::StatusCode, Json};
use serde::{Deserialize, Serialize};
use tracing::{debug, info};

// ---- Types ----

/// Device authorization request (from CLI)
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthRequest {
    pub client_id: Option<String>,
}

/// Device authorization response
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Token exchange request (from CLI polling)
#[derive(Debug, Deserialize)]
pub struct TokenExchangeRequest {
    pub device_code: String,
}

// ---- Handlers ----

/// GET /v1/local/auth/status
pub async fn status(
    State(state): State<WorkspaceState>,
) -> Result<Json<TokenStatusResponse>, NexusApiError> {
    info!("Handling auth status request");

    let token_manager = TokenManager::new(state.db_pool());
    let token = token_manager.get_valid_token().await?;

    match token {
        Some(t) => {
            let user_id = t.user_id.clone();
            let expires_at = t.expires_at.to_rfc3339();
            let needs_refresh = t.needs_refresh();
            debug!(user_id = %user_id, needs_refresh, "Auth token found");
            info!("Auth status completed");
            Ok(Json(TokenStatusResponse {
                authenticated: true,
                user_id: Some(user_id),
                expires_at: Some(expires_at),
                needs_refresh,
            }))
        }
        None => {
            debug!("No auth token found");
            info!("Auth status completed");
            Ok(Json(TokenStatusResponse {
                authenticated: false,
                user_id: None,
                expires_at: None,
                needs_refresh: false,
            }))
        }
    }
}

/// POST /v1/local/auth/device
///
/// Start device authorization flow. In production, this proxies to the platform's
/// `/oauth/device_authorization` endpoint. For V1.x, returns mock data.
pub async fn device_authorization(
    State(state): State<WorkspaceState>,
    Json(_req): Json<DeviceAuthRequest>,
) -> Result<(StatusCode, Json<DeviceAuthResponse>), NexusApiError> {
    info!("Handling device authorization request");

    let user_code = format!("{:04}-{:04}", rand_int(1000, 9999), rand_int(1000, 9999));
    let device_code = format!("dc_{}", uuid::Uuid::new_v4());

    debug!(user_code = %user_code, "Generated device code session");

    // Store the device code session in SQLite
    let conn = state.db().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("Failed to get database connection: {}", e),
    })?;

    let expires_at = (chrono::Utc::now() + chrono::Duration::minutes(15)).to_rfc3339();
    let verification_uri = "https://auth.42ch.com/activate".to_string();

    // Clone values needed after the move into interact closure
    let response_device_code = device_code.clone();
    let response_user_code = user_code.clone();
    let response_verification_uri = verification_uri.clone();

    conn.interact(move |conn| {
        conn.execute(
            "INSERT OR REPLACE INTO device_code_sessions (device_code, user_code, verification_uri, expires_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            [&device_code, &user_code, &verification_uri, &expires_at, "pending"],
        )
    })
    .await
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("Failed to store device code session: {}", e),
    })?
    .map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: e.to_string(),
    })?;

    info!("Device authorization completed");
    Ok((
        StatusCode::OK,
        Json(DeviceAuthResponse {
            device_code: response_device_code,
            user_code: response_user_code,
            verification_uri: response_verification_uri,
            expires_in: 900,
            interval: 5,
        }),
    ))
}

/// POST /v1/local/auth/token
///
/// Exchange a device code for tokens. In production, proxies to platform's
/// `/oauth/token` endpoint. For V1.x, handles mock verification.
pub async fn exchange_token(
    State(state): State<WorkspaceState>,
    Json(req): Json<TokenExchangeRequest>,
) -> Result<Json<serde_json::Value>, NexusApiError> {
    info!("Handling token exchange request");
    debug!(device_code = %req.device_code, "Looking up device code session");
    let conn = state.db().await.map_err(|e| NexusApiError::Internal {
        code: "DATABASE_ERROR".into(),
        message: format!("Failed to get database connection: {}", e),
    })?;

    // Check the device code session status
    let device_code_owned = req.device_code.clone();
    let session: Option<(String, String)> = conn
        .interact(move |conn| {
            let mut stmt = conn.prepare(
                "SELECT status, user_code FROM device_code_sessions WHERE device_code = ?1",
            )?;
            let rows: Vec<(String, String)> = stmt
                .query_map([&device_code_owned], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
                })?
                .filter_map(|r| r.ok())
                .collect();
            Ok::<Option<(String, String)>, rusqlite::Error>(rows.into_iter().next())
        })
        .await
        .map_err(|e| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: format!("Failed to query device code session: {}", e),
        })?
        .map_err(|e: rusqlite::Error| NexusApiError::Internal {
            code: "DATABASE_ERROR".into(),
            message: e.to_string(),
        })?;

    let (status, user_code) = match session {
        Some(s) => s,
        None => {
            return Ok(Json(serde_json::json!({
                "error": "invalid_grant",
                "error_description": "Device code not found"
            })));
        }
    };

    if status == "expired" {
        return Ok(Json(serde_json::json!({
            "error": "expired_token",
            "error_description": "Device code has expired"
        })));
    }

    if status != "confirmed" {
        return Ok(Json(serde_json::json!({
            "status": "pending",
            "message": format!("Authorization pending. Visit https://auth.42ch.com/activate and enter code: {}", user_code)
        })));
    }

    // Mock tokens for confirmed device code
    let access_token = format!("at_{}", uuid::Uuid::new_v4());
    let refresh_token = format!("rt_{}", uuid::Uuid::new_v4());
    let user_id = format!("usr_mock_{}", &uuid::Uuid::new_v4().to_string()[..8]);
    let expires_in = 3600u64;

    let expires_at = chrono::Utc::now() + chrono::Duration::seconds(expires_in as i64);
    let token_manager = TokenManager::new(state.db_pool());
    token_manager
        .store_tokens(&user_id, &access_token, &refresh_token, expires_at)
        .await?;

    // Clean up device code session
    let dc = req.device_code.clone();
    let _ = conn
        .interact(move |conn| {
            conn.execute(
                "DELETE FROM device_code_sessions WHERE device_code = ?1",
                [&dc],
            )
        })
        .await;

    info!("Token exchange completed");
    Ok(Json(serde_json::json!({
        "access_token": access_token,
        "refresh_token": refresh_token,
        "token_type": "Bearer",
        "expires_in": expires_in,
        "user_id": user_id
    })))
}

/// POST /v1/local/auth/logout
///
/// Clear all stored tokens (logout).
pub async fn logout(
    State(state): State<WorkspaceState>,
) -> Result<Json<serde_json::Value>, NexusApiError> {
    info!("Handling logout request");

    let token_manager = TokenManager::new(state.db_pool());
    token_manager.clear_tokens().await?;

    info!("Logout completed");
    Ok(Json(serde_json::json!({
        "success": true,
        "message": "Logged out successfully"
    })))
}

// ---- Helpers ----

/// Simple pseudo-random integer in range (for mock user codes)
fn rand_int(min: u32, max: u32) -> u32 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("SystemTime before UNIX_EPOCH")
        .as_nanos();
    min + (nanos as u32 % (max - min + 1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rand_int_produces_values_in_range() {
        for _ in 0..100 {
            let val = rand_int(1000, 9999);
            assert!((1000..=9999).contains(&val));
        }
    }
}
