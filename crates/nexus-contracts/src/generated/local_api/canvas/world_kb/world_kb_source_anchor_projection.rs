//! `Nexus` `WorldKbSourceAnchorProjection`
//!
//! `Provenance` edge projection derived from `kb_source_anchors` (`V1`.73). `Rendered` read-only on the canvas graph.
//!
//! `@schema_version` 1
//! `@source` world-kb-source-anchor-projection.schema.json

use serde::{Deserialize, Serialize};

/// `Provenance` edge projection derived from `kb_source_anchors` (`V1`.73). `Rendered` read-only on the canvas graph.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub struct WorldKbSourceAnchorProjection {
    pub source_anchor_id: String,
    pub key_block_id: String,
    pub source_type: String,
    pub reference: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}
