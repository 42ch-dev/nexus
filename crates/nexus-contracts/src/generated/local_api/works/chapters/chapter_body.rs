//! `Nexus` `ChapterBody`
//!
//! `Response` body for `GET` /v1/local/works/{`work_id`}/chapters/{n}/body (`V1`.65 `P0`). `Body` is read-only through this surface.
//!
//! `@schema_version` 1
//! `@source` chapter-body.schema.json

use serde::{Deserialize, Serialize};

/// `Response` body for `GET` /v1/local/works/{`work_id`}/chapters/{n}/body (`V1`.65 `P0`). `Body` is read-only through this surface.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct ChapterBody {
    pub work_id: String,
    pub chapter: i64,
    pub volume: i64,
    pub body_path: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<serde_json::Value>,
    pub read_only: bool,
    pub updated_at: String,
}
