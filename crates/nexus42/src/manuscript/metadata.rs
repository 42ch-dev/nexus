//! Manuscript metadata structure
//!
//! Stores and serializes metadata for individual manuscripts.

use serde::{Deserialize, Serialize};

/// Metadata for a manuscript, stored alongside the manuscript file
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ManuscriptMetadata {
    /// Manuscript title
    pub title: String,
    /// World ID this manuscript belongs to
    pub world_id: Option<String>,
    /// Current phase
    #[serde(default = "default_phase")]
    pub phase: String,
    /// Creation timestamp (RFC 3339)
    pub created_at: String,
    /// Last updated timestamp (RFC 3339)
    pub updated_at: String,
    /// Schema version
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Content hash for integrity verification (V1.1 CLI-R7)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
}

fn default_phase() -> String {
    "brainstorm".to_string()
}

fn default_schema_version() -> u32 {
    1
}

impl ManuscriptMetadata {
    /// Create new manuscript metadata
    pub fn new(title: &str, world_id: Option<&str>) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            title: title.to_string(),
            world_id: world_id.map(|s| s.to_string()),
            phase: default_phase(),
            created_at: now.clone(),
            updated_at: now,
            schema_version: 1,
            content_hash: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_metadata_defaults() {
        let meta = ManuscriptMetadata::new("Test", None);
        assert_eq!(meta.title, "Test");
        assert!(meta.world_id.is_none());
        assert_eq!(meta.phase, "brainstorm");
        assert_eq!(meta.schema_version, 1);
        assert!(!meta.created_at.is_empty());
        assert_eq!(meta.created_at, meta.updated_at);
    }

    #[test]
    fn test_new_metadata_with_world_id() {
        let meta = ManuscriptMetadata::new("Test", Some("wld_abc"));
        assert_eq!(meta.world_id, Some("wld_abc".to_string()));
    }

    #[test]
    fn test_serialize_roundtrip() {
        let meta = ManuscriptMetadata::new("Novel", Some("wld_test"));
        let json = serde_json::to_string(&meta).unwrap();
        let deserialized: ManuscriptMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, deserialized);
    }

    #[test]
    fn test_deserialize_with_defaults() {
        let json = r#"{"title": "Minimal", "created_at": "2026-01-01T00:00:00Z", "updated_at": "2026-01-01T00:00:00Z"}"#;
        let meta: ManuscriptMetadata = serde_json::from_str(json).unwrap();
        assert_eq!(meta.title, "Minimal");
        assert_eq!(meta.phase, "brainstorm");
        assert_eq!(meta.schema_version, 1);
        assert!(meta.world_id.is_none());
    }
}
