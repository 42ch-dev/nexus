//! `Nexus` `OutlinePatchStructureRequest`
//!
//! `Request` body for `POST` /v1/local/works/{`work_id`}/outline/patch (`V1`.72). `Mutates` the `Work` outline structure: move a chapter between volumes, attach a chapter to a volume, or link an event to a chapter.
//!
//! `@schema_version` 1
//! `@source` outline-patch-structure-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/works/{`work_id`}/outline/patch (`V1`.72). `Mutates` the `Work` outline structure: move a chapter between volumes, attach a chapter to a volume, or link an event to a chapter.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct OutlinePatchStructureRequest {
    pub work_id: String,
    pub base_revision: u64,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chapter_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_chapter_id: Option<i64>,
}
