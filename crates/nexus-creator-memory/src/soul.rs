//! SOUL document domain model.
//!
//! Represents a parsed SOUL.md file with frontmatter metadata and
//! `## Personality` / `## Experience` sections (ADR-016).

use crate::errors::MemoryError;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Required H2 section names in SOUL.md (ADR-016 D1).
pub const SECTION_PERSONALITY: &str = "Personality";
pub const SECTION_EXPERIENCE: &str = "Experience";

/// All required section names.
pub const REQUIRED_SECTIONS: &[&str] = &[SECTION_PERSONALITY, SECTION_EXPERIENCE];

/// SOUL.md frontmatter metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct SoulFrontmatter {
    /// Creator ID (for verification).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub creator_id: Option<String>,
    /// Schema version for SOUL document format.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema_version: Option<u32>,
    /// Human-readable description.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// Additional custom metadata.
    #[serde(flatten)]
    pub extra: HashMap<String, serde_yaml::Value>,
}

/// Parsed representation of a SOUL.md file.
#[derive(Debug, Clone)]
pub struct SoulDocument {
    /// Parsed frontmatter (may be empty if no frontmatter exists).
    pub frontmatter: SoulFrontmatter,
    /// Raw content of the `## Personality` section (markdown body under the H2).
    pub personality: Option<String>,
    /// Raw content of the `## Experience` section (markdown body under the H2).
    pub experience: Option<String>,
    /// Any additional H2 sections not in the required set.
    pub extra_sections: HashMap<String, String>,
    /// File path this document was loaded from.
    pub source_path: Option<PathBuf>,
}

impl SoulDocument {
    #[must_use]
    /// Create a new empty SOUL document with default frontmatter.
    pub fn new() -> Self {
        Self {
            frontmatter: SoulFrontmatter::default(),
            personality: Some(String::new()),
            experience: Some(String::new()),
            extra_sections: HashMap::new(),
            source_path: None,
        }
    }

    /// Create with a specific `creator_id` in frontmatter.
    #[must_use]
    pub fn for_creator(creator_id: &str) -> Self {
        let mut doc = Self::new();
        doc.frontmatter.creator_id = Some(creator_id.to_string());
        doc.frontmatter.schema_version = Some(1);
        doc
    }
    #[must_use]
    /// Render the full SOUL.md content (frontmatter + sections).
    pub fn render(&self) -> String {
        let mut parts = Vec::new();

        // Frontmatter (only if non-empty)
        if self.frontmatter.creator_id.is_some()
            || self.frontmatter.schema_version.is_some()
            || self.frontmatter.description.is_some()
            || !self.frontmatter.extra.is_empty()
        {
            let yaml = serde_yaml::to_string(&self.frontmatter).unwrap_or_default();
            parts.push(format!("---\n{yaml}---\n"));
        }

        // ## Personality
        parts.push(format!(
            "## {SECTION_PERSONALITY}\n\n{}",
            self.personality.as_deref().unwrap_or("")
        ));
        parts.push(String::new());

        // ## Experience
        parts.push(format!(
            "## {SECTION_EXPERIENCE}\n\n{}",
            self.experience.as_deref().unwrap_or("")
        ));
        parts.push(String::new());

        // Extra sections
        for (name, content) in &self.extra_sections {
            parts.push(format!("## {name}\n\n{content}"));
            parts.push(String::new());
        }

        parts.join("\n")
    }

    /// Validate that all required sections exist.
    pub fn validate(&self) -> Result<(), MemoryError> {
        for section in REQUIRED_SECTIONS {
            let has_section = match *section {
                SECTION_PERSONALITY => self.personality.is_some(),
                SECTION_EXPERIENCE => self.experience.is_some(),
                _ => false,
            };
            if !has_section {
                return Err(MemoryError::SoulMissingSection {
                    section: section.to_string(),
                });
            }
        }
        Ok(())
    }
    ///
    /// # Errors
    /// Returns `Err(MemoryError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(MemoryError::...)` if validation fails.
    /// Parse markdown content into a `SoulDocument`.
    /// Extracts frontmatter (YAML) and H2 sections by exact heading text.
    pub fn parse(content: &str) -> Result<Self, MemoryError> {
        let mut doc = SoulDocument::new();

        // Extract frontmatter
        let fm_content = extract_frontmatter(content);
        if !fm_content.is_empty() {
            match serde_yaml::from_str::<SoulFrontmatter>(&fm_content) {
                Ok(fm) => doc.frontmatter = fm,
                Err(e) => {
                    return Err(MemoryError::SoulFrontmatterError(e.to_string()));
                }
            }
        }

        // Extract H2 sections
        let sections = extract_h2_sections(content);
        for (heading, body) in sections {
            match heading.as_str() {
                SECTION_PERSONALITY => doc.personality = Some(body),
                SECTION_EXPERIENCE => doc.experience = Some(body),
                other => {
                    doc.extra_sections.insert(other.to_string(), body);
                }
            }
        }

        Ok(doc)
    }

    /// Update the personality section content.
    pub fn set_personality(&mut self, content: String) {
        self.personality = Some(content);
    }

    /// Update the experience section content.
    pub fn set_experience(&mut self, content: String) {
        self.experience = Some(content);
    }
}

impl Default for SoulDocument {
    fn default() -> Self {
        Self::new()
    }
}

/// Extract YAML frontmatter from markdown content.
/// Returns the frontmatter string (without --- delimiters) or empty string.
fn extract_frontmatter(content: &str) -> String {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return String::new();
    }
    // Find closing ---
    if let Some(end) = trimmed[3..].find("\n---") {
        trimmed[3..3 + end].trim().to_string()
    } else {
        String::new()
    }
}

/// Extract all H2 (`## Title`) sections from markdown content.
/// Returns vec of (`heading_name`, `body_between_headings`).
fn extract_h2_sections(content: &str) -> Vec<(String, String)> {
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_body = String::new();

    for line in content.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            // Save previous section
            if let Some(prev) = current_heading.take() {
                sections.push((prev, current_body.trim_end().to_string()));
            }
            current_heading = Some(heading.trim().to_string());
            current_body = String::new();
        } else if current_heading.is_some() {
            current_body.push_str(line);
            current_body.push('\n');
        }
    }

    // Don't forget the last section
    if let Some(heading) = current_heading.take() {
        sections.push((heading, current_body.trim_end().to_string()));
    }

    sections
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL_SOUL: &str = "## Personality
A creative writer.

## Experience
None yet.
";

    #[test]
    fn parse_minimal_soul() {
        let doc = SoulDocument::parse(MINIMAL_SOUL).unwrap();
        assert_eq!(
            doc.personality.as_deref().unwrap().trim(),
            "A creative writer."
        );
        assert_eq!(doc.experience.as_deref().unwrap().trim(), "None yet.");
        assert!(doc.extra_sections.is_empty());
    }

    #[test]
    fn parse_soul_with_frontmatter() {
        let content = "---\ncreator_id: ctr_test\nschema_version: 1\ndescription: Test creator SOUL\n---\n## Personality\nCreative.\n\n## Experience\n"
        .to_string();
        let doc = SoulDocument::parse(&content).unwrap();
        assert_eq!(doc.frontmatter.creator_id.as_deref().unwrap(), "ctr_test");
        assert_eq!(doc.frontmatter.schema_version, Some(1));
        assert_eq!(doc.personality.as_deref().unwrap().trim(), "Creative.");
    }

    #[test]
    fn render_roundtrip() {
        let mut doc = SoulDocument::for_creator("ctr_test");
        doc.set_personality("Bold and inventive.".to_string());
        doc.set_experience(String::new());
        let rendered = doc.render();
        let parsed = SoulDocument::parse(&rendered).unwrap();
        assert_eq!(
            parsed.personality.as_deref().unwrap().trim(),
            "Bold and inventive."
        );
        assert_eq!(
            parsed.frontmatter.creator_id.as_deref().unwrap(),
            "ctr_test"
        );
    }

    #[test]
    fn validate_passes_with_required_sections() {
        let doc = SoulDocument::parse(MINIMAL_SOUL).unwrap();
        assert!(doc.validate().is_ok());
    }

    #[test]
    fn validate_fails_missing_experience() {
        let mut doc = SoulDocument::new();
        doc.personality = Some("test".to_string());
        doc.experience = None;
        assert!(doc.validate().is_err());
        let err = doc.validate().unwrap_err();
        assert!(err.to_string().contains("Experience"));
    }

    #[test]
    fn parse_extra_sections() {
        let content =
            "## Personality\nTest\n\n## Goals\n1. Write\n2. Ship\n\n## Experience\nNone\n"
                .to_string();
        let doc = SoulDocument::parse(&content).unwrap();
        assert_eq!(
            doc.extra_sections.get("Goals").unwrap().trim(),
            "1. Write\n2. Ship"
        );
    }

    #[test]
    fn for_creator_sets_frontmatter() {
        let doc = SoulDocument::for_creator("ctr_local_001");
        assert_eq!(
            doc.frontmatter.creator_id.as_deref().unwrap(),
            "ctr_local_001"
        );
        assert_eq!(doc.frontmatter.schema_version, Some(1));
    }

    #[test]
    fn extract_frontmatter_none() {
        assert!(extract_frontmatter("## Personality\nTest").is_empty());
    }

    #[test]
    fn extract_frontmatter_valid() {
        let fm = extract_frontmatter("---\nkey: val\n---");
        assert_eq!(fm, "key: val");
    }

    #[test]
    fn extract_h2_sections_multiple() {
        let content = "## First\nBody1\n\n## Second\nBody2\n\n## Third\nBody3\n";
        let sections = extract_h2_sections(content);
        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].0, "First");
        assert_eq!(sections[1].1.trim(), "Body2");
    }
}
