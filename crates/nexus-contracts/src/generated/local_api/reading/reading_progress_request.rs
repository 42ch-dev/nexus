//! `Nexus` `ReadingProgressRequest`
//!
//! `Request` body for `PUT` /v1/local/reading/progress. `Upserts` persisted scroll position per (creator, work, chapter). `Creator` scope is inferred from the active session.
//!
//! `@schema_version` 1
//! `@source` reading-progress-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `PUT` /v1/local/reading/progress. `Upserts` persisted scroll position per (creator, work, chapter). `Creator` scope is inferred from the active session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReadingProgressRequest {
    pub work_id: String,
    pub chapter: i64,
    pub scroll_progress: u64,
}
