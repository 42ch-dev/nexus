//! `Nexus` `ReadingProgressResponse`
//!
//! `Response` for `GET` and `PUT` /v1/daemon/reading/progress. `Returns` the persisted scroll position for the current creator on the requested (work, chapter). `If` no progress has been saved, `scroll_progress` defaults to 0 with a server-generated `updated_at`.
//!
//! `@schema_version` 1
//! `@source` reading-progress-response.schema.json

use serde::{Deserialize, Serialize};

/// `Response` for `GET` and `PUT` /v1/daemon/reading/progress. `Returns` the persisted scroll position for the current creator on the requested (work, chapter). `If` no progress has been saved, `scroll_progress` defaults to 0 with a server-generated `updated_at`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReadingProgressResponse {
    pub work_id: String,
    pub chapter: i64,
    pub scroll_progress: u64,
    pub updated_at: String,
}
