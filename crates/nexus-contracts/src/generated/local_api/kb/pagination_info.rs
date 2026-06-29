//! `Nexus` `PaginationInfo`
//!
//! `Cursor`-based pagination metadata.
//!
//! `@schema_version` 1
//! `@source` pagination-info.schema.json

use serde::{Deserialize, Serialize};

/// `Cursor`-based pagination metadata.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PaginationInfo {
    pub limit: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_cursor: Option<String>,
    pub has_more: bool,
}
