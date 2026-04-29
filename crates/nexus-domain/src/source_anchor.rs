//! `SourceAnchor` value object — domain logic wrapper around contract `SourceAnchor`.
//!
//! `SourceAnchor` is NOT an aggregate; it's a value object embedded in `KeyBlock`, Delta, etc.
//! See data-model-v1.md §6.1.

use crate::errors::DomainError;
use serde::{Deserialize, Serialize};

/// Domain `SourceAnchor` — references platform Story summary entities.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceAnchor {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub story_summary_refs: Option<Vec<SourceSummaryRef>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub excerpt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// Reference to a platform Story summary entity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SourceSummaryRef {
    pub story_manifest_id: String,
    pub summary_unit_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub unit_kind: Option<String>,
}

/// Maximum excerpt length per consistency-rules-v1.md G6.
pub const MAX_EXCERPT_LENGTH: usize = 1024;

impl SourceAnchor {
    #[must_use]
    /// Create `SourceAnchor` referencing a single story summary unit.
    pub fn new(story_manifest_id: &str, summary_unit_id: &str, unit_kind: Option<&str>) -> Self {
        Self {
            story_summary_refs: Some(vec![SourceSummaryRef {
                story_manifest_id: story_manifest_id.to_string(),
                summary_unit_id: summary_unit_id.to_string(),
                unit_kind: unit_kind.map(|s| s.to_string()),
            }]),
            excerpt: None,
            summary: None,
        }
    }

    /// Create `SourceAnchor` with excerpt only (no story refs).
    #[must_use]
    pub fn from_excerpt(excerpt: &str) -> Self {
        Self {
            story_summary_refs: None,
            excerpt: Some(excerpt.to_string()),
            summary: None,
        }
    }

    /// Add a story summary reference.
    pub fn add_summary_ref(
        &mut self,
        story_manifest_id: &str,
        summary_unit_id: &str,
        unit_kind: Option<&str>,
    ) {
        let refs = self.story_summary_refs.get_or_insert_with(Vec::new);
        refs.push(SourceSummaryRef {
            story_manifest_id: story_manifest_id.to_string(),
            summary_unit_id: summary_unit_id.to_string(),
            unit_kind: unit_kind.map(|s| s.to_string()),
        });
    }

    /// Validate excerpt length (max 1024 chars per G6).
    pub const fn validate_excerpt(&self) -> Result<(), DomainError> {
        if let Some(ref excerpt) = self.excerpt {
            if excerpt.len() > MAX_EXCERPT_LENGTH {
                return Err(DomainError::ExcerptTooLong {
                    actual: excerpt.len(),
                    max: MAX_EXCERPT_LENGTH,
                });
            }
        }
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Validate all `story_summary_refs` point to visible manifests in the given world.
    pub fn validate_refs(
        &self,
        _world_id: &str,
        visible_manifests: &[&str],
    ) -> Result<(), DomainError> {
        if let Some(refs) = &self.story_summary_refs {
            for r in refs {
                if !visible_manifests.contains(&r.story_manifest_id.as_str()) {
                    return Err(DomainError::ValidationError(format!(
                        "story_summary_refs contains non-visible manifest: {}",
                        r.story_manifest_id
                    )));
                }
            }
        }
        Ok(())
    }
}

// ── Conversion: Domain ↔ Contract ──────────────────────────────────────

impl From<nexus_contracts::SourceAnchor> for SourceAnchor {
    fn from(c: nexus_contracts::SourceAnchor) -> Self {
        Self {
            story_summary_refs: c.story_summary_refs.map(|refs| {
                refs.into_iter()
                    .map(|r| SourceSummaryRef {
                        story_manifest_id: r.story_manifest_id,
                        summary_unit_id: r.summary_unit_id,
                        unit_kind: r.unit_kind,
                    })
                    .collect()
            }),
            excerpt: c.excerpt,
            summary: c.summary,
        }
    }
}

impl From<SourceAnchor> for nexus_contracts::SourceAnchor {
    fn from(d: SourceAnchor) -> Self {
        Self {
            story_summary_refs: d.story_summary_refs.map(|refs| {
                refs.into_iter()
                    .map(|r| nexus_contracts::SourceSummaryRef {
                        story_manifest_id: r.story_manifest_id,
                        summary_unit_id: r.summary_unit_id,
                        unit_kind: r.unit_kind,
                    })
                    .collect()
            }),
            excerpt: d.excerpt,
            summary: d.summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_with_summary_ref() {
        let sa = SourceAnchor::new("stm_abc123", "sum_xyz789", Some("chapter_summary"));
        let refs = sa.story_summary_refs.as_ref().unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].story_manifest_id, "stm_abc123");
        assert_eq!(refs[0].summary_unit_id, "sum_xyz789");
        assert_eq!(refs[0].unit_kind.as_deref(), Some("chapter_summary"));
    }

    #[test]
    fn test_excerpt_max_length() {
        let sa = SourceAnchor::from_excerpt(&"x".repeat(1024));
        assert!(sa.validate_excerpt().is_ok());
    }

    #[test]
    fn test_excerpt_exceeds_limit() {
        let sa = SourceAnchor::from_excerpt(&"x".repeat(1025));
        assert!(matches!(
            sa.validate_excerpt(),
            Err(DomainError::ExcerptTooLong {
                actual: 1025,
                max: 1024
            })
        ));
    }

    #[test]
    fn test_null_excerpt_valid() {
        let sa = SourceAnchor::new("stm_abc123", "sum_xyz789", None);
        assert!(sa.validate_excerpt().is_ok());
    }

    #[test]
    fn test_multi_summary_refs() {
        let mut sa = SourceAnchor::new("stm_1", "sum_1", None);
        sa.add_summary_ref("stm_2", "sum_2", None);
        sa.add_summary_ref("stm_3", "sum_3", None);
        assert_eq!(sa.story_summary_refs.as_ref().unwrap().len(), 3);
    }

    #[test]
    fn test_serialize_roundtrip() {
        let mut sa = SourceAnchor::new("stm_abc", "sum_xyz", Some("chapter_summary"));
        sa.excerpt = Some("test excerpt".to_string());
        let json = serde_json::to_string(&sa).unwrap();
        let deserialized: SourceAnchor = serde_json::from_str(&json).unwrap();
        assert_eq!(sa, deserialized);
    }

    #[test]
    fn test_validate_refs_with_visible_manifests() {
        let sa = SourceAnchor::new("stm_visible1", "sum_1", None);
        assert!(sa
            .validate_refs("wld_test", &["stm_visible1", "stm_visible2"])
            .is_ok());
    }

    #[test]
    fn test_validate_refs_with_non_visible_manifest() {
        let sa = SourceAnchor::new("stm_hidden", "sum_1", None);
        assert!(sa.validate_refs("wld_test", &["stm_visible1"]).is_err());
    }
}
