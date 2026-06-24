//! `Nexus` `StaleFindingsResponse`
//!
//! `Response` for `GET` /v1/local/findings/stale.
//!
//! `@schema_version` 1
//! `@source` stale-findings-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/findings/stale.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct StaleFindingsResponse {
    pub open_count: i64,
    pub stale_threshold_seconds: i64,
    pub items: Vec<serde_json::Value>,
}
