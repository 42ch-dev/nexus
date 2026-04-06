//! Daemon HTTP Client
//!
//! Communicates with the nexus42d daemon via the Local API (HTTP JSON on port 8420).

use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use serde::{de::DeserializeOwned, Serialize};

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
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::Api {
                status,
                message: body,
            });
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
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::Api {
                status,
                message: body,
            });
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
            let body = resp.text().await.unwrap_or_default();
            return Err(CliError::Api {
                status,
                message: body,
            });
        }

        let data: serde_json::Value = resp.json().await?;
        Ok(data)
    }
}
