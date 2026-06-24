//! `Nexus` `ActiveCreatorResponse`
//!
//! `Response` for `GET` /v1/local/creators/active.
//!
//! `@schema_version` 1
//! `@source` active-creator-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/creators/active.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ActiveCreatorResponse {
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
}
