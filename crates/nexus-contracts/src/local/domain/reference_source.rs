//! ReferenceSource — local-only reference source registration.
//!
//! Does NOT sync to platform; shared excerpts go through
//! MemoryItem(memory_kind=research_material). Aligned with data-model-v1.md §5.9A.

use serde::{Deserialize, Serialize};

use crate::generated::common_types::{ReferenceSourceType, ScanStatus};

/// Local-only registration of research/reference sources.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_reference_source() {
        let v = ReferenceSource {
            schema_version: 1,
            reference_source_id: "ref_test123".to_string(),
            workspace_id: "wrk_test".to_string(),
            source_type: ReferenceSourceType::Url,
            uri: "https://example.com".to_string(),
            title: "Example".to_string(),
            tags: Some(vec!["research".to_string()]),
            content_hash: Some("sha256:abc".to_string()),
            scan_status: ScanStatus::Scanned,
            created_at: "2026-01-01T00:00:00Z".to_string(),
            updated_at: None,
        };
        let s = serde_json::to_string(&v).unwrap();
        let back: ReferenceSource = serde_json::from_str(&s).unwrap();
        assert_eq!(back, v);
    }
}
