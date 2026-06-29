//! `Nexus` `ChapterDetailResponse`
//!
//! `Response` for `GET` /v1/local/works/{`work_id`}/chapters/{n} (`V1`.65 `P0`). `Mirrors` `ChapterSummary` plus content metadata. `Does` not read outline/body content.
//!
//! `@schema_version` 1
//! `@source` chapter-detail.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::works::chapters::chapter_protection::ChapterProtection;
use crate::generated::local_api::works::chapters::chapter_status::ChapterStatus;

/// `Response` for `GET` /v1/local/works/{`work_id`}/chapters/{n} (`V1`.65 `P0`). `Mirrors` `ChapterSummary` plus content metadata. `Does` not read outline/body content.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChapterDetail {
    pub work_id: String,
    pub chapter: i64,
    pub volume: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    pub planned_word_count: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_word_count: Option<u64>,
    pub status: ChapterStatus,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outline_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
    pub can_edit_outline: bool,
    pub can_edit_structure: bool,
    pub body_read_only: bool,
    pub protection: ChapterProtection,
}
