//! `ReferenceSource` aggregate — local-only research/reference registration.
//!
//! `ReferenceSource` is LOCAL-ONLY — does NOT sync to platform.
//! Shared excerpts go through `MemoryItem(memory_kind=research_material)`.
//! See data-model-v1.md §5.9A.

use crate::errors::DomainError;
use crate::memory_item::MemoryItem;
use crate::MemoryType;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Reference source type enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReferenceSourceType {
    File,
    Pdf,
    Url,
    Note,
}

impl ReferenceSourceType {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::File => "file",
            Self::Pdf => "pdf",
            Self::Url => "url",
            Self::Note => "note",
        }
    }
}

/// Scan status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    Pending,
    Scanned,
    Failed,
    Ignored,
}

impl ScanStatus {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Pending => "pending",
            Self::Scanned => "scanned",
            Self::Failed => "failed",
            Self::Ignored => "ignored",
        }
    }
}

/// `ReferenceSource` aggregate — local-only research/reference registration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReferenceSource {
    pub schema_version: u32,
    pub reference_source_id: String,
    pub workspace_id: String,
    pub source_type: String,
    pub uri: String,
    pub title: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_hash: Option<String>,
    pub scan_status: String,
    pub created_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub updated_at: Option<String>,
}

impl ReferenceSource {
    /// Register a new reference source.
    #[must_use]
    pub fn register(
        workspace_id: &str,
        source_type: ReferenceSourceType,
        uri: &str,
        title: &str,
    ) -> Self {
        let reference_source_id =
            format!("ref_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            schema_version: 1,
            reference_source_id,
            workspace_id: workspace_id.to_string(),
            source_type: source_type.as_str().to_string(),
            uri: uri.to_string(),
            title: title.to_string(),
            tags: None,
            content_hash: None,
            scan_status: ScanStatus::Pending.as_str().to_string(),
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: None,
        }
    }

    /// Mark as scanned successfully.
    pub fn mark_scanned(&mut self, content_hash: Option<&str>) {
        self.scan_status = ScanStatus::Scanned.as_str().to_string();
        self.content_hash = content_hash
            .map(|h| h.to_string())
            .or_else(|| self.content_hash.clone());
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Mark scan as failed.
    pub fn mark_scan_failed(&mut self) {
        self.scan_status = ScanStatus::Failed.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Ignore this reference source.
    pub fn ignore(&mut self) {
        self.scan_status = ScanStatus::Ignored.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Convert an excerpt to a `MemoryItem` for platform sync.
    /// The `ReferenceSource` itself stays local; only the `MemoryItem` syncs.
    #[must_use]
    pub fn extract_to_memory_item(
        &self,
        creator_id: &str,
        world_id: &str,
        excerpt: &str,
    ) -> MemoryItem {
        let mut mi = MemoryItem::new(
            creator_id,
            world_id,
            MemoryType::Canon,
            Some("research_material"),
        );
        mi.set_summary(excerpt);
        mi.add_source_ref("reference", &self.reference_source_id);
        mi
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Validate URI format based on `source_type`.
    pub fn validate_uri(&self) -> Result<(), DomainError> {
        if self.uri.trim().is_empty() {
            return Err(DomainError::InvalidUri {
                source_type: self.source_type.clone(),
                reason: "URI is empty".to_string(),
            });
        }

        match self.source_type.as_str() {
            "url" if !self.uri.starts_with("http://") && !self.uri.starts_with("https://") => {
                return Err(DomainError::InvalidUri {
                    source_type: self.source_type.clone(),
                    reason: "URL must start with http:// or https://".to_string(),
                });
            }
            "file" | "pdf" if !self.uri.starts_with("file://") && !self.uri.starts_with('/') => {
                return Err(DomainError::InvalidUri {
                    source_type: self.source_type.clone(),
                    reason: "file/pdf URI must start with file:// or /".to_string(),
                });
            }
            "note" => {
                // Notes don't require URI format validation
            }
            _ => {}
        }

        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::local::domain::ReferenceSource> for ReferenceSource {
    fn from(c: nexus_contracts::local::domain::ReferenceSource) -> Self {
        Self {
            schema_version: c.schema_version,
            reference_source_id: c.reference_source_id,
            workspace_id: c.workspace_id,
            source_type: c.source_type.as_str().to_string(),
            uri: c.uri,
            title: c.title,
            tags: c.tags,
            content_hash: c.content_hash,
            scan_status: c.scan_status.as_str().to_string(),
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<ReferenceSource> for nexus_contracts::local::domain::ReferenceSource {
    fn from(d: ReferenceSource) -> Self {
        Self {
            schema_version: d.schema_version,
            reference_source_id: d.reference_source_id,
            workspace_id: d.workspace_id,
            source_type: nexus_contracts::generated::common_types::ReferenceSourceType::from_str(
                &d.source_type,
            )
            .unwrap(),
            uri: d.uri,
            title: d.title,
            tags: d.tags,
            content_hash: d.content_hash,
            scan_status: nexus_contracts::generated::common_types::ScanStatus::from_str(
                &d.scan_status,
            )
            .unwrap(),
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_file_reference() {
        let rs = ReferenceSource::register(
            "wrk_test",
            ReferenceSourceType::File,
            "file:///path/to/reference.pdf",
            "My Reference",
        );
        assert_eq!(rs.source_type, "file");
        assert_eq!(rs.scan_status, "pending");
        assert!(rs.reference_source_id.starts_with("ref_"));
    }

    #[test]
    fn test_mark_scanned() {
        let mut rs = ReferenceSource::register(
            "wrk_test",
            ReferenceSourceType::Pdf,
            "file:///path/to/doc.pdf",
            "Document",
        );
        rs.mark_scanned(Some("sha256:abc123"));
        assert_eq!(rs.scan_status, "scanned");
        assert_eq!(rs.content_hash.as_deref(), Some("sha256:abc123"));
    }

    #[test]
    fn test_mark_scan_failed() {
        let mut rs = ReferenceSource::register(
            "wrk_test",
            ReferenceSourceType::Url,
            "https://example.com",
            "Example",
        );
        rs.mark_scan_failed();
        assert_eq!(rs.scan_status, "failed");
    }

    #[test]
    fn test_extract_to_memory() {
        let rs = ReferenceSource::register(
            "wrk_test",
            ReferenceSourceType::File,
            "file:///path/to/book.pdf",
            "My Book",
        );
        let mi = rs.extract_to_memory_item("ctr_author", "wld_story", "Excerpt text...");
        assert_eq!(mi.summary.as_deref(), Some("Excerpt text..."));
        assert_eq!(mi.memory_kind.as_deref(), Some("research_material"));
    }

    #[test]
    fn test_all_source_types() {
        let types = vec![
            ReferenceSourceType::File,
            ReferenceSourceType::Pdf,
            ReferenceSourceType::Url,
            ReferenceSourceType::Note,
        ];
        for st in types {
            let rs = ReferenceSource::register("wrk_test", st.clone(), "test://uri", "Test");
            let json = serde_json::to_string(&rs).unwrap();
            let deserialized: ReferenceSource = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.source_type, st.as_str());
        }
    }

    #[test]
    fn test_validate_uri_valid_url() {
        let rs = ReferenceSource::register(
            "wrk_test",
            ReferenceSourceType::Url,
            "https://example.com/page",
            "Example Page",
        );
        assert!(rs.validate_uri().is_ok());
    }

    #[test]
    fn test_validate_uri_invalid_url() {
        let rs =
            ReferenceSource::register("wrk_test", ReferenceSourceType::Url, "not-a-url", "Bad URL");
        assert!(rs.validate_uri().is_err());
    }

    #[test]
    fn test_validate_uri_valid_file() {
        let rs = ReferenceSource::register(
            "wrk_test",
            ReferenceSourceType::File,
            "file:///path/to/file.txt",
            "File",
        );
        assert!(rs.validate_uri().is_ok());
    }

    #[test]
    fn test_ignore_reference() {
        let mut rs = ReferenceSource::register(
            "wrk_test",
            ReferenceSourceType::Note,
            "note://local",
            "Quick Note",
        );
        rs.ignore();
        assert_eq!(rs.scan_status, "ignored");
    }

    #[test]
    fn test_serialize_roundtrip() {
        let rs = ReferenceSource::register(
            "wrk_test",
            ReferenceSourceType::File,
            "file:///path/to/ref.pdf",
            "Reference",
        );
        let json = serde_json::to_string(&rs).unwrap();
        let deserialized: ReferenceSource = serde_json::from_str(&json).unwrap();
        assert_eq!(rs, deserialized);
    }
}
