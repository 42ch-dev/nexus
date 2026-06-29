//! `Nexus` `PatchChapterRequest`
//!
//! `Request` body for `PATCH` /v1/local/works/{`work_id`}/chapters/{n} (`V1`.65 `P0`). `All` fields optional. `title` is rejected because it is display-only until `P0` materializes a title column.
//!
//! `@schema_version` 1
//! `@source` patch-chapter-request.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::works::chapters::chapter_status::ChapterStatus;

/// `Request` body for `PATCH` /v1/local/works/{`work_id`}/chapters/{n} (`V1`.65 `P0`). `All` fields optional. `title` is rejected because it is display-only until `P0` materializes a title column.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct PatchChapterRequest {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_word_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<ChapterStatus>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confirm_structural_edit: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub transition_reason: Option<String>,
}
