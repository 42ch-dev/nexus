//! Nexus Delta
//!
//! Single atomic change to an entity in a manuscript world. Aligned with data-model-v1.md §5.12.
//!
//! @schema_version 1
//! @source delta.schema.json

use crate::generated::common_types::{DeltaOperation, DeltaType, SourceAnchor};
use serde::{Deserialize, Serialize};

/// Single atomic change to an entity in a manuscript world. Aligned with data-model-v1.md §5.12.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub struct Delta {
    pub delta_type: DeltaType,
    pub operation: DeltaOperation,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_entity_id: Option<String>,
    pub payload: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_anchor: Option<SourceAnchor>,
    pub local_timestamp: String,
}
