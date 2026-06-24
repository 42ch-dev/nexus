//! `Nexus` `ListWorksResponse`
//!
//! `Response` for `GET` /v1/local/works.
//!
//! `@schema_version` 1
//! `@source` list-works-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::works::work_summary::WorkSummary;

/// `Response` for `GET` /v1/local/works.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListWorksResponse {
    pub works: Vec<WorkSummary>,
    pub total: i64,
}
