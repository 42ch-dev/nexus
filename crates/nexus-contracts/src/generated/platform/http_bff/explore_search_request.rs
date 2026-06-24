//! `Nexus` `ExploreSearchRequest`
//!
//! `Request` body for `POST` /v1/explore/search — read-only full-text style query.
//!
//! `@schema_version` 1
//! `@source` explore-search-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/explore/search — read-only full-text style query.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreSearchRequest {
    pub schema_version: u32,
    pub query: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
}
