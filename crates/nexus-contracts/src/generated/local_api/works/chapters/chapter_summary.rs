//! `Nexus` `ChapterSummary`
//!
//! `Summary` row for a work chapter in list responses (`V1`.65 `P0`). `Lightweight` — does not read outline/body files.
//!
//! `@schema_version` 1
//! `@source` chapter-summary.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::local_api::works::chapters::chapter_status::ChapterStatus;

/// `Summary` row for a work chapter in list responses (`V1`.65 `P0`). `Lightweight` — does not read outline/body files.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ChapterSummary {
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
}
