//! Device Flow — OAuth 2.0 Device Authorization Grant support
//!
//! In V1.0, the daemon acts as a proxy for device flow authentication.
//! The actual OAuth flow is handled by the platform; the daemon provides
//! local session management.
//!
//! **TD-10 / production:** `verify_device_code` is a deliberate stub (`Ok(false)`) until real
//! platform device/token endpoints and CI stubs exist. See
//! `.agents/plans/knowledge/device-flow-oauth-scope-v1.md` for scope and deferral rationale.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

/// Device code session stored locally
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeSession {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: String,
    pub interval: u64,
    pub status: String, // "pending" | "confirmed" | "expired"
}

/// Verify a device code (proxy to platform)
pub async fn verify_device_code(_platform_url: &str, _device_code: &str) -> anyhow::Result<bool> {
    // V1.0 skeleton — in production, polls the platform token endpoint
    Ok(false)
}
