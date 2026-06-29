//! `Nexus` `ListKbEntriesQuery`
//!
//! `Query` parameters for `GET` /v1/local/kb/entries.
//!
//! `@schema_version` 1
//! `@source` list-kb-entries-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/kb/entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListKbEntriesQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}
