//! Nexus ReferenceSource
//!
//! ReferenceSource - local-only registration of research/reference sources. Does NOT sync to platform; shared excerpts go through MemoryItem(memory_kind=research_material). Aligned with data-model-v1.md §5.9A.
//!
//! @schema_version 1
//! @source reference-source.schema.json

use serde::{Deserialize, Serialize};
use crate::generated::common_types::{ReferenceSourceType, ScanStatus};

/// ReferenceSource - local-only registration of research/reference sources. Does NOT sync to platform; shared excerpts go through MemoryItem(memory_kind=research_material). Aligned with data-model-v1.md §5.9A.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct ReferenceSource {
    pub schema_version: u32,
    pub reference_source_id: String,
    pub workspace_id: String,
    pub source_type: ReferenceSourceType,
    pub uri: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub scan_status: ScanStatus,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}
