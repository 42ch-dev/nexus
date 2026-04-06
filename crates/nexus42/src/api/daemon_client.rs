//! Daemon HTTP Client
//!
//! Communicates with the nexus42d daemon via the Local API (HTTP JSON on port 8420).

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use serde::{de::DeserializeOwned, Serialize};

/// Structured error response from the daemon API
#[derive(Debug, serde::Deserialize)]
struct DaemonErrorResponse {
    #[allow(dead_code)]
    success: bool,
    #[serde(default)]
    error: Option<DaemonErrorDetail>,
}

#[derive(Debug, serde::Deserialize)]
struct DaemonErrorDetail {
    code: String,
    message: String,
}

/// Client for the nexus42d Local API
#[derive(Debug, Clone)]
pub struct DaemonClient {
    base_url: String,
    http: reqwest::Client,
}

impl DaemonClient {
    /// Create a new daemon client from config
    pub fn from_config(config: &CliConfig) -> Self {
        Self::new(&config.daemon_url)
    }

    /// Create a new daemon client with a custom base URL
    pub fn new(base_url: &str) -> Self {
        Self {
            base_url: base_url.to_string(),
            http: reqwest::Client::new(),
        }
    }

    /// Check if the daemon is running and healthy
    pub async fn health_check(&self) -> Result<bool> {
        let url = format!("{}/v1/local/runtime/health", self.base_url);
        match self.http.get(&url).send().await {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Send a GET request
    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.get(&url).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(status, resp).await);
        }

        let data: T = resp.json().await?;
        Ok(data)
    }

    /// Send a POST request with JSON body
    #[allow(dead_code)] // For upcoming sync / local API commands
    pub async fn post<T: DeserializeOwned, B: Serialize>(&self, path: &str, body: &B) -> Result<T> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.post(&url).json(body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(status, resp).await);
        }

        let data: T = resp.json().await?;
        Ok(data)
    }

    /// Send a POST request with JSON body, returning raw response
    #[allow(dead_code)] // For upcoming sync / local API commands
    pub async fn post_raw<B: Serialize>(&self, path: &str, body: &B) -> Result<serde_json::Value> {
        let url = format!("{}{}", self.base_url, path);
        let resp = self.http.post(&url).json(body).send().await?;

        if !resp.status().is_success() {
            let status = resp.status().as_u16();
            return Err(Self::parse_error_response(status, resp).await);
        }

        let data: serde_json::Value = resp.json().await?;
        Ok(data)
    }

    /// Parse an error response from the daemon, attempting structured parsing first
    /// and falling back to raw body text for backward compatibility.
    async fn parse_error_response(status: u16, resp: reqwest::Response) -> CliError {
        let body = resp.text().await.unwrap_or_default();

        // Try structured error parsing first
        if let Ok(parsed) = serde_json::from_str::<DaemonErrorResponse>(&body) {
            if let Some(detail) = parsed.error {
                return CliError::Api {
                    status,
                    message: format!("[{}] {}", detail.code, detail.message),
                };
            }
        }

        // Fallback to raw body (backward compatible with old daemon versions)
        CliError::Api {
            status,
            message: body,
        }
    }
}
