//! `Nexus` `LogoutResponse`
//!
//! `Response` for `POST` /v1/local/creators/logout.
//!
//! `@schema_version` 1
//! `@source` logout-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/creators/logout.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct LogoutResponse {
    pub creator_id: String,
    pub cleared: bool,
}
