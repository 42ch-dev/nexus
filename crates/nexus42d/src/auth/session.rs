//! User Session Management

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Active user session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserSession {
    pub user_id: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: String,
}

impl UserSession {
    /// Check if the session has expired
    pub fn is_expired(&self) -> bool {
        if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&self.expires_at) {
            chrono::Utc::now() > expires
        } else {
            true // Can't parse → assume expired
        }
    }
}
