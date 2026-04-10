//! Nexus ExploreBrowseRequest
//!
//! Request body for POST /v1/explore/browse — read-only directory-style listing.
//!
//! @schema_version 1
//! @source explore-browse-request.schema.json

use serde::{Deserialize, Serialize};

/// Request body for POST /v1/explore/browse — read-only directory-style listing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ExploreBrowseRequest {
    pub schema_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
}
