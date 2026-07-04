//! `Nexus` `ReadingProgressQuery`
//!
//! `Query` parameters for `GET` /v1/local/reading/progress. `Creator` scope is inferred from the active session.
//!
//! `@schema_version` 1
//! `@source` reading-progress-query.schema.json

use serde::{Deserialize, Serialize};

/// `Query` parameters for `GET` /v1/local/reading/progress. `Creator` scope is inferred from the active session.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ReadingProgressQuery {
    pub work_id: String,
    pub chapter: i64,
}
