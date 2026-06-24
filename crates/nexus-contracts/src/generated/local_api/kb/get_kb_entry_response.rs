//! `Nexus` `GetKbEntryResponse`
//!
//! `Response` for `GET` /v1/local/kb/entries/{`entry_id`}.
//!
//! `@schema_version` 1
//! `@source` get-kb-entry-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` /v1/local/kb/entries/{`entry_id`}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct GetKbEntryResponse {
    pub entry_id: String,
    pub title: String,
    pub created_at: String,
    pub content: String,
}
