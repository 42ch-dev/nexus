//! `Nexus` `PutChapterOutlineRequest`
//!
//! `Request` body for `PUT` /v1/local/works/{`work_id`}/chapters/{n}/outline (`V1`.65 `P0`).
//!
//! `@schema_version` 1
//! `@source` put-chapter-outline-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `PUT` /v1/local/works/{`work_id`}/chapters/{n}/outline (`V1`.65 `P0`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PutChapterOutlineRequest {
    pub content: String,
}
