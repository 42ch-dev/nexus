//! `Nexus` `DeleteKbEntryResponse`
//!
//! `Response` for `DELETE` /v1/local/kb/entries/{`entry_id`}.
//!
//! `@schema_version` 1
//! `@source` delete-kb-entry-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `DELETE` /v1/local/kb/entries/{`entry_id`}.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct DeleteKbEntryResponse {
    pub entry_id: String,
    pub deleted: bool,
}
