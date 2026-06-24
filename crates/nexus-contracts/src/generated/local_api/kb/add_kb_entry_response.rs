//! `Nexus` `AddKbEntryResponse`
//!
//! `Response` for `POST` /v1/local/kb/entries.
//!
//! `@schema_version` 1
//! `@source` add-kb-entry-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `POST` /v1/local/kb/entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct AddKbEntryResponse {
    pub entry_id: String,
    pub title: String,
}
