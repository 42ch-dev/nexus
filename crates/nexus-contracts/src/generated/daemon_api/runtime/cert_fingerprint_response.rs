//! `Nexus` `CertFingerprintResponse`
//!
//! `Response` for `GET` /v1/daemon/runtime/cert-fingerprint. `Returns` the `SHA`-256 fingerprint of the daemon's `TLS` certificate for `TOFU` pinning. `No` authentication required. `When` the daemon is loopback-only and has no `TLS` cert, the fingerprint field is an empty string and `created_at` is absent.
//!
//! `@schema_version` 1
//! `@source` cert-fingerprint-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/daemon/runtime/cert-fingerprint. `Returns` the `SHA`-256 fingerprint of the daemon's `TLS` certificate for `TOFU` pinning. `No` authentication required. `When` the daemon is loopback-only and has no `TLS` cert, the fingerprint field is an empty string and `created_at` is absent.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CertFingerprintResponse {
    pub fingerprint: String,
    pub algorithm: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}
