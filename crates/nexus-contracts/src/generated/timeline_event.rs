//! Nexus TimelineEvent
//!
//! TimelineEvent - a canonical event on the world timeline with causality and sequence. Aligned with data-model-v1.md §5.6.
//!
//! @schema_version 1
//! @source timeline-event.schema.json

use serde::{Deserialize, Serialize};

/// TimelineEvent - a canonical event on the world timeline with causality and sequence. Aligned with data-model-v1.md §5.6.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct TimelineEvent {
    pub schema_version: u32,
    pub timeline_event_id: String,
    pub world_id: String,
    pub branch_id: String,
    pub event_type: String,
    pub status: String,
    pub sequence_no: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub caused_by_event_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub affected_key_block_ids: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_command_id: Option<String>,
    pub created_at: String,
}
