//! `Nexus` `ErrorResponse`
//!
//! `Canonical` `Local` `API` error detail. `The` daemon wraps this as `{ success: false, error: ErrorResponse, request_id?: string }` on the wire; this schema models the stable, contract-locked `error` detail shared across all `Local` `API` failure paths (`F`-`E1`).
//!
//! `@schema_version` 1
//! `@source` error-response.schema.json

use serde::{Deserialize, Serialize};

/// `Canonical` `Local` `API` error detail. `The` daemon wraps this as `{ success: false, error: ErrorResponse, request_id?: string }` on the wire; this schema models the stable, contract-locked `error` detail shared across all `Local` `API` failure paths (`F`-`E1`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ErrorResponse {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}
