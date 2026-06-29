//! `Nexus` `CreatorDetail`
//!
//! `Response` for `GET` /v1/local/creators/{`creator_id`}.
//!
//! `@schema_version` 1
//! `@source` creator-detail.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/creators/{`creator_id`}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct CreatorDetail {
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub handle: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub has_api_key: bool,
    pub has_cached_token: bool,
    pub is_active: bool,
}
