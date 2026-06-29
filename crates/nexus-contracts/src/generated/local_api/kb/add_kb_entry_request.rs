//! `Nexus` `AddKbEntryRequest`
//!
//! `Request` body for `POST` /v1/local/kb/entries.
//!
//! `@schema_version` 1
//! `@source` add-kb-entry-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/kb/entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct AddKbEntryRequest {
    pub creator_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file_path: Option<String>,
}
