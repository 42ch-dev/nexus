//! `Nexus` `PublishStoryRequest`
//!
//! `Request` body for `POST` /v1/publish/story — platform `Publish` `API` (display fields, idempotency, chapter selection).
//!
//! `@schema_version` 1
//! `@source` publish-story-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/publish/story — platform `Publish` `API` (display fields, idempotency, chapter selection).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PublishStoryRequest {
    pub schema_version: u32,
    pub world_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manuscript_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_manifest_id: Option<String>,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    pub chapter_ids: Vec<String>,
    pub idempotency_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_command_id: Option<String>,
}
