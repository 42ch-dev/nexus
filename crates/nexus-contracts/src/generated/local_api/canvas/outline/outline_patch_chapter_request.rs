//! `Nexus` `OutlinePatchChapterRequest`
//!
//! `Request` body for `POST` /v1/local/works/{`work_id`}/chapters/{`chapter_id`}/patch (`V1`.72). `Edits` chapter-level metadata exposed on the outline canvas.
//!
//! `@schema_version` 1
//! `@source` outline-patch-chapter-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/works/{`work_id`}/chapters/{`chapter_id`}/patch (`V1`.72). `Edits` chapter-level metadata exposed on the outline canvas.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OutlinePatchChapterRequest {
    pub work_id: String,
    pub chapter_id: i64,
    pub base_revision: u64,
    pub set: serde_json::Value,
}
