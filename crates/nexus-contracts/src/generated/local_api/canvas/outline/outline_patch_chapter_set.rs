//! `Nexus` `OutlinePatchChapterSet`
//!
//! `Fields` to update on a chapter via the outline canvas patch route (`V1`.72).
//!
//! `@schema_version` 1
//! `@source` outline-patch-chapter-set.schema.json

use serde::{Deserialize, Serialize};

/// `Fields` to update on a chapter via the outline canvas patch route (`V1`.72).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct OutlinePatchChapterSet {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub slug: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub planned_word_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub actual_word_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
}
