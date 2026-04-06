//! Creator Session Management

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Creator session (short-lived token from platform)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreatorSession {
    pub creator_id: String,
    pub access_token: String,
    pub expires_at: String,
}

impl CreatorSession {
    /// Check if the session has expired
    pub fn is_expired(&self) -> bool {
        if let Ok(expires) = chrono::DateTime::parse_from_rfc3339(&self.expires_at) {
            chrono::Utc::now() > expires
        } else {
            true
        }
    }
}
