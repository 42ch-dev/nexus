//! `Nexus` `ListKbEntriesResponse`
//!
//! `Response` for `GET` /v1/daemon/kb/entries.
//!
//! `@schema_version` 1
//! `@source` list-kb-entries-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::daemon_api::kb::kb_entry_summary::KbEntrySummary;
use crate::generated::daemon_api::kb::pagination_info::PaginationInfo;

/// `Response` for `GET` /v1/daemon/kb/entries.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListKbEntriesResponse {
    pub items: Vec<KbEntrySummary>,
    pub pagination: PaginationInfo,
}
