//! Nexus StoryManifest
//!
//! StoryManifest entity for platform-side chapter/arc manifest and summary. Aligned with data-model-v1.md §5.9.
//!
//! @schema_version 1
//! @source story-manifest.schema.json

use crate::generated::common_types::{ManifestType, ManuscriptStorage, StoryManifestStatus};
use serde::{Deserialize, Serialize};

/// StoryManifest entity for platform-side chapter/arc manifest and summary. Aligned with data-model-v1.md §5.9.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct StoryManifest {
    pub schema_version: u32,
    pub story_manifest_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub manifest_type: ManifestType,
    pub status: StoryManifestStatus,
    pub title: String,
    pub summary_unit_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_manuscript: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manuscript_storage: Option<ManuscriptStorage>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub local_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sandbox_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub published_artifact_id: Option<String>,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
