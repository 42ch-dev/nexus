//! `Nexus` `CreatorInfo`
//!
//! `Creator` info row from local identity store.
//!
//! `@schema_version` 1
//! `@source` creator-info.schema.json

use serde::{Deserialize, Serialize};

/// `Creator` info row from local identity store.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct CreatorInfo {
    pub creator_id: String,
    pub display_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cached_at: Option<String>,
}
