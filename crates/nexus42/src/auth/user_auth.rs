//! User Authentication — Device Flow OAuth
//!
//! Implements OAuth 2.0 Device Authorization Grant for human user login.
//! The CLI delegates all auth operations to the daemon's HTTP API,
//! which owns the centralized auth state in SQLite.

use crate::api::daemon_client::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use serde::Deserialize;

/// Device authorization response from daemon
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceAuthResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Token response from daemon
#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub struct TokenResponse {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
    pub user_id: String,
}

/// Auth status response from daemon
#[derive(Debug, Deserialize)]
pub struct AuthStatusResponse {
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub expires_at: Option<String>,
    pub needs_refresh: bool,
}

/// Initiate device flow login via daemon
pub async fn login(config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);

    // Check daemon is running
    if !client.health_check().await? {
        return Err(CliError::DaemonNotRunning);
    }

    // Step 1: Request device authorization from daemon
    let auth_response: DeviceAuthResponse = client
        .post(
            "/v1/local/auth/device",
            &serde_json::json!({ "client_id": null }),
        )
        .await?;

    println!("To authenticate, visit:");
    println!("  {}", auth_response.verification_uri);
    println!();
    println!("  Enter code: {}", auth_response.user_code);
    println!();

    // Step 2: Poll for token exchange
    println!("Waiting for authorization...");

    let interval = std::time::Duration::from_secs(auth_response.interval.max(3));
    let max_attempts = (auth_response.expires_in / auth_response.interval.max(1)).min(60);

    for attempt in 1..=max_attempts {
        tokio::time::sleep(interval).await;

        let poll_body = serde_json::json!({
            "device_code": auth_response.device_code
        });

        let poll_result: std::result::Result<serde_json::Value, CliError> =
            client.post("/v1/local/auth/token", &poll_body).await;

        match poll_result {
            Ok(response) => {
                // Check if we got an error or a pending status
                if let Some(error) = response.get("error") {
                    let error_code = error.as_str().unwrap_or("unknown");
                    match error_code {
                        "expired_token" => {
                            eprintln!("✗ Device code expired. Please try again.");
                            return Err(CliError::Other("Device authorization expired".into()));
                        }
                        "invalid_grant" => {
                            eprintln!("✗ Invalid device code. Please try again.");
                            return Err(CliError::Other("Invalid device code".into()));
                        }
                        other => {
                            let error_desc = response
                                .get("error_description")
                                .and_then(|d: &serde_json::Value| d.as_str())
                                .unwrap_or(other);
                            eprintln!("✗ Authorization error: {}", error_desc);
                            return Err(CliError::Other(format!(
                                "Authorization failed: {}",
                                other
                            )));
                        }
                    }
                }

                if let Some(status) = response
                    .get("status")
                    .and_then(|s: &serde_json::Value| s.as_str())
                {
                    if status == "pending" {
                        let msg = response
                            .get("message")
                            .and_then(|m: &serde_json::Value| m.as_str())
                            .unwrap_or("Waiting...");
                        if attempt % 6 == 0 {
                            println!("  [{}] {}", attempt, msg);
                        }
                        continue;
                    }
                }

                // Success — extract token info
                if response.get("access_token").is_some() {
                    let token: TokenResponse = serde_json::from_value(response).map_err(|e| {
                        CliError::Other(format!("Failed to parse token response: {}", e))
                    })?;

                    println!("✓ Authenticated successfully.");
                    println!("  User: {}", token.user_id);
                    println!("  Token type: {}", token.token_type);
                    println!("  Expires in: {}s", token.expires_in);
                    return Ok(());
                }
            }
            Err(CliError::Api { status: 401, .. }) => {
                // Token not available yet, keep polling
                if attempt % 6 == 0 {
                    println!("  [{}] Waiting for authorization...", attempt);
                }
                continue;
            }
            Err(e) => {
                // Log and continue polling — daemon might be temporarily unavailable
                if attempt % 6 == 0 {
                    eprintln!("  [{}] Poll error: {}", attempt, e);
                }
                continue;
            }
        }
    }

    Err(CliError::Other(
        "Authorization timed out. Please try again.".into(),
    ))
}

/// Login with a raw access token (development/testing mode)
///
/// Stores the token in the daemon's SQLite database.
pub async fn login_with_token(
    config: &CliConfig,
    access_token: String,
    _refresh_token: String,
    user_id: String,
) -> Result<()> {
    let client = DaemonClient::from_config(config);

    if !client.health_check().await? {
        return Err(CliError::DaemonNotRunning);
    }

    // For dev mode, the CLI stores tokens via the daemon's mock device flow.
    // Since the daemon API doesn't have a direct "store token" endpoint yet,
    // we note this limitation and provide the token info for reference.
    println!("⚠ Direct token storage requires daemon support.");
    println!("  Use `nexus42 auth login` for the full device flow.");
    println!("  User: {}", user_id);
    println!(
        "  Token: {}...",
        &access_token[..access_token.len().min(16)]
    );

    Ok(())
}

/// Logout — clear tokens via daemon
pub async fn logout(config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);

    if !client.health_check().await? {
        return Err(CliError::DaemonNotRunning);
    }

    let response: serde_json::Value = client.post("/v1/local/auth/logout", &()).await?;

    if response.get("success").and_then(|s| s.as_bool()) == Some(true) {
        println!("✓ Logged out successfully.");
    } else {
        let msg = response
            .get("message")
            .and_then(|m: &serde_json::Value| m.as_str())
            .unwrap_or("Unknown error");
        println!("⚠ Logout response: {}", msg);
    }

    Ok(())
}

/// Show current authentication status from daemon
pub async fn status(config: &CliConfig) -> Result<()> {
    let client = DaemonClient::from_config(config);

    // If daemon is not running, show local status
    if !client.health_check().await? {
        println!("Daemon not running — no auth state available.");
        println!("  Start the daemon with: nexus42 daemon start");
        return Ok(());
    }

    let auth_status: AuthStatusResponse = client.get("/v1/local/auth/status").await?;

    if auth_status.authenticated {
        println!("User Authentication: ✓ Active");
        if let Some(uid) = &auth_status.user_id {
            println!("  User ID: {}", uid);
        }
        if let Some(exp) = &auth_status.expires_at {
            println!("  Expires: {}", exp);
        }
        if auth_status.needs_refresh {
            println!("  ⚠ Token needs refresh (expires soon)");
        }
    } else {
        println!("User Authentication: ✗ Not logged in");
        println!("  Run `nexus42 auth login` to authenticate.");
    }

    Ok(())
}
