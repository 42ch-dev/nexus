//! `Nexus` `WorkOutline`
//!
//! `Canonical` read model for the `Work` outline + timeline (`V1`.72). `Exposes` the `outline_revision` and structured metadata needed by the `Canvas` `Outline`+`Timeline` surface.
//!
//! `@schema_version` 1
//! `@source` work-outline.schema.json

use serde::{Deserialize, Serialize};

/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkOutlineVolume {
    pub volume_id: i64,
    pub label: String,
    pub chapter_ids: Vec<i64>,
}
/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkOutlineTimelineEvent {
    pub event_id: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realizes_chapter_id: Option<i64>,
}
/// Inline array item type (auto-generated from schema)
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkOutlineForeshadow {
    pub source_event_id: String,
    pub target_event_id: String,
}
/// `Canonical` read model for the `Work` outline + timeline (`V1`.72). `Exposes` the `outline_revision` and structured metadata needed by the `Canvas` `Outline`+`Timeline` surface.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorkOutline {
    pub work_id: String,
    pub outline_revision: u64,
    pub volumes: Vec<WorkOutlineVolume>,
    pub timeline_events: Vec<WorkOutlineTimelineEvent>,
    pub foreshadows: Vec<WorkOutlineForeshadow>,
    pub chapter_titles: std::collections::HashMap<String, String>,
    pub updated_at: String,
}
