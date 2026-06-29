//! `Nexus` `ListChaptersResponse`
//!
//! `Response` for `GET` /v1/local/works/{`work_id`}/chapters (`V1`.65 `P0`). `Cursor`-based pagination over `ChapterSummary` rows. `Uses` `items` key per `F`-`P3`.
//!
//! `@schema_version` 1
//! `@source` list-chapters-response.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::works::chapters::chapter_summary::ChapterSummary;
use crate::generated::local_api::kb::pagination_info::PaginationInfo;

/// `Response` for `GET` /v1/local/works/{`work_id`}/chapters (`V1`.65 `P0`). `Cursor`-based pagination over `ChapterSummary` rows. `Uses` `items` key per `F`-`P3`.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ListChaptersResponse {
    pub items: Vec<ChapterSummary>,
    pub pagination: PaginationInfo,
}
