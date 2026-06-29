//! `Nexus` `ChapterOutline`
//!
//! `Response` body for `GET`/`PUT` /v1/local/works/{`work_id`}/chapters/{n}/outline (`V1`.65 `P0`).
//!
//! `@schema_version` 1
//! `@source` chapter-outline.schema.json

use serde::{Deserialize, Serialize};

/// `Response` body for `GET`/`PUT` /v1/local/works/{`work_id`}/chapters/{n}/outline (`V1`.65 `P0`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChapterOutline {
    pub work_id: String,
    pub chapter: i64,
    pub volume: i64,
    pub outline_path: String,
    pub content: String,
    pub updated_at: String,
}
