//! `Nexus` `ListChaptersQuery`
//!
//! `Query` parameters for `GET` /v1/local/works/{`work_id`}/chapters (`V1`.65 `P0`). `Cursor`-based pagination with optional status filter.
//!
//! `@schema_version` 1
//! `@source` list-chapters-query.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::works::chapters::chapter_status::ChapterStatus;

/// `Query` parameters for `GET` /v1/local/works/{`work_id`}/chapters (`V1`.65 `P0`). `Cursor`-based pagination with optional status filter.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ListChaptersQuery {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ChapterStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub limit: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cursor: Option<String>,
}
