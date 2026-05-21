//! `StoryManifest` aggregate — platform-side chapter/arc manifest and summary authority.
//!
//! `StoryManifest` is the platform-authoritative summary for chapters, arcs,
//! stories, and excerpts. See data-model-v1.md §5.9.

use crate::errors::NarrativeError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Manifest type enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManifestType {
    Chapter,
    Arc,
    Story,
    Excerpt,
}

impl ManifestType {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::Chapter => "chapter",
            Self::Arc => "arc",
            Self::Story => "story",
            Self::Excerpt => "excerpt",
        }
    }
}

/// `StoryManifest` status enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StoryManifestStatus {
    SummaryReady,
    StagedForPublish,
    Published,
    Archived,
}

impl StoryManifestStatus {
    #[must_use]
    pub const fn as_str(&self) -> &str {
        match self {
            Self::SummaryReady => "summary_ready",
            Self::StagedForPublish => "staged_for_publish",
            Self::Published => "published",
            Self::Archived => "archived",
        }
    }
}

/// Manuscript storage enum.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ManuscriptStorage {
    None,
    LocalWorkspace,
    PlatformSandbox,
}

impl ManuscriptStorage {
    pub const fn as_str(&self) -> &str {
        match self {
            Self::None => "none",
            Self::LocalWorkspace => "local_workspace",
            Self::PlatformSandbox => "platform_sandbox",
        }
    }
}

/// `StoryManifest` aggregate — platform-side chapter/arc manifest and summary.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoryManifest {
    pub schema_version: u32,
    pub story_manifest_id: String,
    pub world_id: String,
    pub creator_id: String,
    pub manifest_type: String,
    pub status: String,
    pub title: String,
    pub summary_unit_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output_manuscript: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manuscript_storage: Option<String>,
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

impl StoryManifest {
    /// Create a new story manifest.
    #[must_use]
    pub fn new(
        world_id: &str,
        creator_id: &str,
        manifest_type: ManifestType,
        title: &str,
        summary_unit_id: &str,
    ) -> Self {
        let story_manifest_id =
            format!("stm_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            schema_version: 1,
            story_manifest_id,
            world_id: world_id.to_string(),
            creator_id: creator_id.to_string(),
            manifest_type: manifest_type.as_str().to_string(),
            status: StoryManifestStatus::SummaryReady.as_str().to_string(),
            title: title.to_string(),
            summary_unit_id: summary_unit_id.to_string(),
            summary_text: None,
            output_manuscript: Some(true),
            manuscript_storage: None,
            local_path: None,
            sandbox_path: None,
            content_hash: None,
            published_artifact_id: None,
            created_at: chrono::Utc::now().to_rfc3339(),
            updated_at: None,
        }
    }

    /// Set summary text (platform-authoritative).
    pub fn set_summary_text(&mut self, text: &str) {
        self.summary_text = Some(text.to_string());
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
    }

    /// Configure manuscript output and storage.
    pub fn configure_manuscript(
        &mut self,
        output: bool,
        storage: ManuscriptStorage,
        local_path: Option<&str>,
        sandbox_path: Option<&str>,
    ) -> Result<(), NarrativeError> {
        self.output_manuscript = Some(output);
        self.manuscript_storage = Some(storage.as_str().to_string());

        match storage {
            ManuscriptStorage::LocalWorkspace => {
                if local_path.is_none() {
                    return Err(NarrativeError::InvalidStorageConfig(
                        "local_path is required for local_workspace storage".to_string(),
                    ));
                }
                self.local_path = local_path.map(|s| s.to_string());
                self.sandbox_path = None;
            }
            ManuscriptStorage::PlatformSandbox => {
                if sandbox_path.is_none() {
                    return Err(NarrativeError::InvalidStorageConfig(
                        "sandbox_path is required for platform_sandbox storage".to_string(),
                    ));
                }
                self.sandbox_path = sandbox_path.map(|s| s.to_string());
                self.local_path = None;
            }
            ManuscriptStorage::None => {
                self.local_path = None;
                self.sandbox_path = None;
            }
        }

        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Stage for publishing.
    /// Pre: `summary_text` is set, `content_hash` is computed.
    pub fn stage_for_publish(&mut self) -> Result<(), NarrativeError> {
        if self.status != StoryManifestStatus::SummaryReady.as_str() {
            return Err(NarrativeError::InvalidState {
                expected: "summary_ready".to_string(),
                actual: self.status.clone(),
            });
        }
        if self.summary_text.is_none() {
            return Err(NarrativeError::ValidationError(
                "summary_text must be set before staging".to_string(),
            ));
        }
        self.status = StoryManifestStatus::StagedForPublish.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Mark as published.
    pub fn publish(&mut self, artifact_id: &str) -> Result<(), NarrativeError> {
        if self.status != StoryManifestStatus::StagedForPublish.as_str() {
            return Err(NarrativeError::InvalidState {
                expected: "staged_for_publish".to_string(),
                actual: self.status.clone(),
            });
        }
        self.status = StoryManifestStatus::Published.as_str().to_string();
        self.published_artifact_id = Some(artifact_id.to_string());
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Archive this manifest.
    pub fn archive(&mut self) -> Result<(), NarrativeError> {
        if self.status == StoryManifestStatus::Archived.as_str() {
            return Err(NarrativeError::AlreadyInState("archived".to_string()));
        }
        self.status = StoryManifestStatus::Archived.as_str().to_string();
        self.updated_at = Some(chrono::Utc::now().to_rfc3339());
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(NarrativeError::...)` if validation fails.
    /// Validate storage configuration consistency.
    /// e.g., `manuscript_storage=local_workspace` requires `local_path`.
    pub fn validate_storage_config(&self) -> Result<(), NarrativeError> {
        if let Some(ref storage) = self.manuscript_storage {
            match storage.as_str() {
                "local_workspace" if self.local_path.is_none() => {
                    return Err(NarrativeError::InvalidStorageConfig(
                        "local_workspace storage requires local_path".to_string(),
                    ));
                }
                "platform_sandbox" if self.sandbox_path.is_none() => {
                    return Err(NarrativeError::InvalidStorageConfig(
                        "platform_sandbox storage requires sandbox_path".to_string(),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::StoryManifest> for StoryManifest {
    fn from(c: nexus_contracts::StoryManifest) -> Self {
        Self {
            schema_version: c.schema_version,
            story_manifest_id: c.story_manifest_id,
            world_id: c.world_id,
            creator_id: c.creator_id,
            manifest_type: c.manifest_type.as_str().to_string(),
            status: c.status.as_str().to_string(),
            title: c.title,
            summary_unit_id: c.summary_unit_id,
            summary_text: c.summary_text,
            output_manuscript: c.output_manuscript,
            manuscript_storage: c.manuscript_storage.map(|s| s.as_str().to_string()),
            local_path: c.local_path,
            sandbox_path: c.sandbox_path,
            content_hash: c.content_hash,
            published_artifact_id: c.published_artifact_id,
            created_at: c.created_at,
            updated_at: c.updated_at,
        }
    }
}

#[allow(clippy::fallible_impl_from)]
impl From<StoryManifest> for nexus_contracts::StoryManifest {
    fn from(d: StoryManifest) -> Self {
        Self {
            schema_version: d.schema_version,
            story_manifest_id: d.story_manifest_id,
            world_id: d.world_id,
            creator_id: d.creator_id,
            manifest_type: nexus_contracts::ManifestType::from_str(&d.manifest_type).unwrap(),
            status: nexus_contracts::StoryManifestStatus::from_str(&d.status).unwrap(),
            title: d.title,
            summary_unit_id: d.summary_unit_id,
            summary_text: d.summary_text,
            output_manuscript: d.output_manuscript,
            manuscript_storage: d
                .manuscript_storage
                .map(|s| nexus_contracts::ManuscriptStorage::from_str(&s).unwrap()),
            local_path: d.local_path,
            sandbox_path: d.sandbox_path,
            content_hash: d.content_hash,
            published_artifact_id: d.published_artifact_id,
            created_at: d.created_at,
            updated_at: d.updated_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_chapter_manifest() {
        let sm = StoryManifest::new(
            "wld_test",
            "ctr_author",
            ManifestType::Chapter,
            "Chapter 1",
            "sum_ch1",
        );
        assert_eq!(sm.status, "summary_ready");
        assert_eq!(sm.manifest_type, "chapter");
        assert!(sm.story_manifest_id.starts_with("stm_"));
    }

    #[test]
    fn test_stage_for_publish() {
        let mut sm = StoryManifest::new(
            "wld_test",
            "ctr_author",
            ManifestType::Chapter,
            "Chapter 1",
            "sum_ch1",
        );
        sm.set_summary_text("The hero begins their journey.");
        sm.stage_for_publish().unwrap();
        assert_eq!(sm.status, "staged_for_publish");
    }

    #[test]
    fn test_stage_without_summary() {
        let mut sm = StoryManifest::new(
            "wld_test",
            "ctr_author",
            ManifestType::Chapter,
            "Chapter 1",
            "sum_ch1",
        );
        assert!(matches!(
            sm.stage_for_publish(),
            Err(NarrativeError::ValidationError(_))
        ));
    }

    #[test]
    fn test_publish_with_artifact() {
        let mut sm = StoryManifest::new(
            "wld_test",
            "ctr_author",
            ManifestType::Chapter,
            "Chapter 1",
            "sum_ch1",
        );
        sm.set_summary_text("Summary text");
        sm.stage_for_publish().unwrap();
        sm.publish("art_123").unwrap();
        assert_eq!(sm.status, "published");
        assert_eq!(sm.published_artifact_id.as_deref(), Some("art_123"));
    }

    #[test]
    fn test_storage_config_validation() {
        let mut sm = StoryManifest::new(
            "wld_test",
            "ctr_author",
            ManifestType::Chapter,
            "Chapter 1",
            "sum_ch1",
        );
        sm.configure_manuscript(true, ManuscriptStorage::LocalWorkspace, None, None)
            .unwrap_err();
        sm.configure_manuscript(
            true,
            ManuscriptStorage::LocalWorkspace,
            Some("/path/to/file"),
            None,
        )
        .unwrap();
        assert!(sm.validate_storage_config().is_ok());
    }

    #[test]
    fn test_all_manifest_types() {
        let types = vec![
            ManifestType::Chapter,
            ManifestType::Arc,
            ManifestType::Story,
            ManifestType::Excerpt,
        ];
        for mt in types {
            let sm = StoryManifest::new("wld_test", "ctr_author", mt.clone(), "Test", "sum_1");
            let json = serde_json::to_string(&sm).unwrap();
            let deserialized: StoryManifest = serde_json::from_str(&json).unwrap();
            assert_eq!(deserialized.manifest_type, mt.as_str());
        }
    }

    #[test]
    fn test_archive_manifest() {
        let mut sm = StoryManifest::new(
            "wld_test",
            "ctr_author",
            ManifestType::Chapter,
            "Chapter 1",
            "sum_ch1",
        );
        sm.archive().unwrap();
        assert_eq!(sm.status, "archived");
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut sm = StoryManifest::new(
            "wld_test",
            "ctr_author",
            ManifestType::Chapter,
            "Chapter 1",
            "sum_ch1",
        );
        sm.set_summary_text("A summary");
        let json = serde_json::to_string(&sm).unwrap();
        let deserialized: StoryManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(sm, deserialized);
    }
}
