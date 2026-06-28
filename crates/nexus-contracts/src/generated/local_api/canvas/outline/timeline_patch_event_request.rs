//! `Nexus` `TimelinePatchEventRequest`
//!
//! `Request` body for `POST` /v1/local/works/{`work_id`}/timeline/patch (`V1`.72). `Mutates` the `Work` timeline: add, remove, attach to chapter, or create foreshadow links.
//!
//! `@schema_version` 1
//! `@source` timeline-patch-event-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/local/works/{`work_id`}/timeline/patch (`V1`.72). `Mutates` the `Work` timeline: add, remove, attach to chapter, or create foreshadow links.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct TimelinePatchEventRequest {
    pub work_id: String,
    pub base_revision: u64,
    pub operation: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub event_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realizes_chapter_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_chapter_id: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub foreshadows_event_id: Option<String>,
}
