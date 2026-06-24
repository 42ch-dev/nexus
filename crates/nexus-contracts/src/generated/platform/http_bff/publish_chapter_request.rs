//! `Nexus` `PublishChapterRequest`
//!
//! `Request` body for `POST` /v1/publish/chapters — publish a single chapter artifact (platform `Publish` `API`).
//!
//! `@schema_version` 1
//! `@source` publish-chapter-request.schema.json

use serde::{Deserialize, Serialize};

/// `Request` body for `POST` /v1/publish/chapters — publish a single chapter artifact (platform `Publish` `API`).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct PublishChapterRequest {
    pub schema_version: u32,
    pub world_id: String,
    pub story_manifest_id: String,
    pub idempotency_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sync_command_id: Option<String>,
}
