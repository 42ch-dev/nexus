//! Long-term memory domain model.
//!
//! Parsed representation of a long-term memory Markdown file with YAML
//! frontmatter (spec §3.3). Each memory file lives at:
//! `~/.nexus42/creators/<creator_id>/memory/long-term/<slug>.md`
//!
//! See `creator-memory-soul-lifecycle-v1.md` §3, §5.

use crate::memory_item::MemoryKind;
use crate::DomainError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::str::FromStr;

/// Schema version for long-term memory files.
pub const MEMORY_FILE_VERSION: u32 = 1;

/// Long-term memory frontmatter (spec §3.3).
///
/// Required fields:
/// - `nexus_memory_version`: file format version (currently 1)
/// - `memory_id`: logical ID, format `mem_<uuid>` (auto-generated on creation)
/// - `memory_kind`: matches `MemoryKind` enum values from `memory_item.rs`
/// - `updated_at`: ISO-8601 timestamp
/// - `source_session_ids`: optional list of ACP session IDs that produced this memory
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LongTermMemoryFrontmatter {
    pub nexus_memory_version: u32,
    pub memory_id: String,
    pub memory_kind: String,
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub source_session_ids: Vec<String>,
}

/// Parsed representation of a long-term memory Markdown file.
///
/// Contains the frontmatter metadata and the body markdown content.
/// The `source_path` is set when loaded from disk and used to derive the slug.
#[derive(Debug, Clone)]
pub struct LongTermMemory {
    /// Parsed frontmatter.
    pub frontmatter: LongTermMemoryFrontmatter,
    /// Body markdown content (everything after the closing `---`).
    pub body: String,
    /// File path this memory was loaded from (used for slug derivation).
    pub source_path: Option<PathBuf>,
}

impl LongTermMemory {
    #[must_use]
    /// Create a new long-term memory with auto-generated `memory_id`.
    ///
    /// `memory_id` format: `mem_<uuid>` (dashes stripped from UUID).
    /// `updated_at` is set to the current UTC time in ISO-8601.
    /// `source_session_ids` starts empty.
    pub fn new(memory_kind: &str) -> Self {
        let memory_id = format!("mem_{}", uuid::Uuid::new_v4().to_string().replace('-', ""));
        Self {
            frontmatter: LongTermMemoryFrontmatter {
                nexus_memory_version: MEMORY_FILE_VERSION,
                memory_id,
                memory_kind: memory_kind.to_string(),
                updated_at: chrono::Utc::now().to_rfc3339(),
                source_session_ids: Vec::new(),
            },
            body: String::new(),
            source_path: None,
        }
    }

    /// Validate the memory document.
    ///
    /// Checks:
    /// - `nexus_memory_version` is the expected version
    /// - `memory_id` is non-empty and starts with `mem_`
    /// - `memory_kind` is a valid `MemoryKind` enum value
    /// - `updated_at` is non-empty
    /// - Slug (derived from `source_path`) is path-safe, if `source_path` is set
    pub fn validate(&self) -> Result<(), DomainError> {
        if self.frontmatter.nexus_memory_version != MEMORY_FILE_VERSION {
            return Err(DomainError::ValidationError(format!(
                "unsupported nexus_memory_version: {} (expected {})",
                self.frontmatter.nexus_memory_version, MEMORY_FILE_VERSION
            )));
        }
        if self.frontmatter.memory_id.is_empty() || !self.frontmatter.memory_id.starts_with("mem_")
        {
            return Err(DomainError::ValidationError(
                "memory_id must be non-empty and start with 'mem_'".to_string(),
            ));
        }
        if self.frontmatter.memory_kind.is_empty() {
            return Err(DomainError::ValidationError(
                "memory_kind must be non-empty".to_string(),
            ));
        }
        // Validate memory_kind against known enum values
        if MemoryKind::from_str(&self.frontmatter.memory_kind).is_err() {
            return Err(DomainError::ValidationError(format!(
                "invalid memory_kind: '{}' (expected one of: {})",
                self.frontmatter.memory_kind,
                MemoryKind::all_as_strings().join(", ")
            )));
        }
        if self.frontmatter.updated_at.is_empty() {
            return Err(DomainError::ValidationError(
                "updated_at must be non-empty".to_string(),
            ));
        }
        // Validate slug safety if source_path is set
        if let Some(ref path) = self.source_path {
            if let Some(stem) = path.file_stem() {
                if let Some(slug) = stem.to_str() {
                    if !slug_is_safe(slug) {
                        return Err(DomainError::ValidationError(format!(
                            "slug derived from source_path is not path-safe: '{}'",
                            slug
                        )));
                    }
                }
            }
        }
        Ok(())
    }

    /// Derive the slug from `source_path` (filename without `.md` extension).
    ///
    /// Returns an empty string if `source_path` is not set or the stem
    /// cannot be extracted.
    #[must_use]
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    pub fn slug(&self) -> String {
        self.source_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string()
    }

    /// Render the full memory file content (frontmatter + body).
    ///
    /// Output format:
    /// ```text
    /// ---
    /// nexus_memory_version: 1
    /// memory_id: mem_xxx
    /// ...
    /// ---
    /// Body content here.
    /// ```
    pub fn render(&self) -> Result<String, DomainError> {
        let yaml = serde_yaml::to_string(&self.frontmatter).map_err(|e| {
            DomainError::ValidationError(format!("failed to serialize memory frontmatter: {e}"))
        })?;
        Ok(format!("---\n{yaml}---\n{}", self.body))
    }
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    ///
    /// # Errors
    /// Returns `Err(DomainError::...)` if validation fails.
    /// Parse a memory file's content (frontmatter + body).
    ///
    /// Extracts the YAML frontmatter between `---` delimiters and
    /// treats everything after the closing `---` as the body.
    pub fn parse(content: &str) -> Result<Self, DomainError> {
        let (fm_str, body) = extract_frontmatter_and_body(content)
            .map_err(|e| DomainError::ValidationError(format!("cannot parse memory file: {e}")))?;

        let frontmatter: LongTermMemoryFrontmatter =
            serde_yaml::from_str(&fm_str).map_err(|e| {
                DomainError::ValidationError(format!("invalid memory frontmatter: {e}"))
            })?;

        Ok(Self {
            frontmatter,
            body,
            source_path: None,
        })
    }

    /// Set the body content and update `updated_at`.
    pub fn set_body(&mut self, body: &str) {
        self.body = body.to_string();
        self.frontmatter.updated_at = chrono::Utc::now().to_rfc3339();
    }

    /// Add a source session ID (deduped).
    pub fn add_source_session(&mut self, session_id: &str) {
        if !self
            .frontmatter
            .source_session_ids
            .contains(&session_id.to_string())
        {
            self.frontmatter
                .source_session_ids
                .push(session_id.to_string());
        }
    }

    /// Touch the memory (update `updated_at` without changing content).
    pub fn touch(&mut self) {
        self.frontmatter.updated_at = chrono::Utc::now().to_rfc3339();
    }
}
#[must_use]
/// Check if a slug is path-safe (no `..`, `/`, `\`, null bytes, or control chars).
pub fn slug_is_safe(slug: &str) -> bool {
    if slug.is_empty() {
        return false;
    }
    !slug.contains("..")
        && !slug.contains('/')
        && !slug.contains('\\')
        && !slug.chars().any(|c| c.is_control())
}

/// Extract YAML frontmatter and body from markdown content.
///
/// Returns `(frontmatter_yaml_string, body_string)`.
/// Returns an error if the content doesn't start with `---`.
fn extract_frontmatter_and_body(content: &str) -> Result<(String, String), String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err("content does not start with frontmatter delimiter '---'".to_string());
    }
    // Find closing ---
    let after_open = &trimmed[3..];
    if let Some(end_offset) = after_open.find("\n---") {
        let fm_str = after_open[..end_offset].trim().to_string();
        let body_start = 3 + end_offset + 4; // skip "---\n---"
        let body = if body_start < trimmed.len() {
            trimmed[body_start..].trim_start().to_string()
        } else {
            String::new()
        };
        Ok((fm_str, body))
    } else {
        Err("missing closing frontmatter delimiter '---'".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_valid_memory() {
        let mem = LongTermMemory::new("story_summary");
        assert_eq!(mem.frontmatter.nexus_memory_version, MEMORY_FILE_VERSION);
        assert!(mem.frontmatter.memory_id.starts_with("mem_"));
        assert_eq!(mem.frontmatter.memory_kind, "story_summary");
        assert!(!mem.frontmatter.updated_at.is_empty());
        assert!(mem.frontmatter.source_session_ids.is_empty());
        assert!(mem.body.is_empty());
        assert!(mem.source_path.is_none());
    }

    #[test]
    fn validate_passes_for_valid_memory() {
        let mem = LongTermMemory::new("story_summary");
        assert!(mem.validate().is_ok());
    }

    #[test]
    fn validate_fails_for_bad_version() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.frontmatter.nexus_memory_version = 99;
        assert!(mem.validate().is_err());
        assert!(mem
            .validate()
            .unwrap_err()
            .to_string()
            .contains("unsupported"));
    }

    #[test]
    fn validate_fails_for_bad_memory_id() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.frontmatter.memory_id = String::new();
        assert!(mem.validate().is_err());
        assert!(mem
            .validate()
            .unwrap_err()
            .to_string()
            .contains("memory_id"));

        mem.frontmatter.memory_id = "invalid_prefix".to_string();
        assert!(mem.validate().is_err());
    }

    #[test]
    fn validate_fails_for_empty_memory_kind() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.frontmatter.memory_kind = String::new();
        assert!(mem.validate().is_err());
        assert!(mem
            .validate()
            .unwrap_err()
            .to_string()
            .contains("memory_kind"));
    }

    #[test]
    fn validate_fails_for_invalid_memory_kind() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.frontmatter.memory_kind = "invalid_kind".to_string();
        assert!(mem.validate().is_err());
        assert!(mem
            .validate()
            .unwrap_err()
            .to_string()
            .contains("invalid memory_kind"));
    }

    #[test]
    fn validate_fails_for_empty_updated_at() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.frontmatter.updated_at = String::new();
        assert!(mem.validate().is_err());
        assert!(mem
            .validate()
            .unwrap_err()
            .to_string()
            .contains("updated_at"));
    }

    #[test]
    fn validate_accepts_all_valid_memory_kinds() {
        for kind in &MemoryKind::all_as_strings() {
            let mem = LongTermMemory::new(kind);
            assert!(mem.validate().is_ok(), "kind '{kind}' should validate");
        }
    }

    #[test]
    fn validate_rejects_unsafe_slug_from_source_path() {
        let mut mem = LongTermMemory::new("story_summary");
        // Use a slug with '..' in the filename stem
        mem.source_path = Some(PathBuf::from("/path/to/memories/foo..bar.md"));
        assert!(mem.validate().is_err());
        assert!(mem
            .validate()
            .unwrap_err()
            .to_string()
            .contains("not path-safe"));
    }

    #[test]
    fn validate_accepts_safe_slug_from_source_path() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.source_path = Some(PathBuf::from("/path/to/memories/my-memory.md"));
        assert!(mem.validate().is_ok());
    }

    #[test]
    fn slug_from_source_path() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.source_path = Some(PathBuf::from("/some/path/my-memory.md"));
        assert_eq!(mem.slug(), "my-memory");
    }

    #[test]
    fn slug_empty_when_no_source_path() {
        let mem = LongTermMemory::new("story_summary");
        assert!(mem.slug().is_empty());
    }

    #[test]
    fn render_roundtrip() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.set_body("This is the memory body.\nWith multiple lines.");
        mem.add_source_session("sess_abc123");

        let rendered = mem.render().unwrap();
        let parsed = LongTermMemory::parse(&rendered).unwrap();

        assert_eq!(parsed.frontmatter.nexus_memory_version, MEMORY_FILE_VERSION);
        assert_eq!(parsed.frontmatter.memory_id, mem.frontmatter.memory_id);
        assert_eq!(parsed.frontmatter.memory_kind, "story_summary");
        assert_eq!(parsed.frontmatter.source_session_ids, vec!["sess_abc123"]);
        assert_eq!(parsed.body.trim(), mem.body.trim());
    }

    #[test]
    fn parse_full_file() {
        let content = r#"---
nexus_memory_version: 1
memory_id: mem_test123
memory_kind: character_note
updated_at: "2026-04-14T12:00:00Z"
source_session_ids:
  - sess_001
  - sess_002
---
Character analysis: Alice is a determined protagonist.
"#;
        let mem = LongTermMemory::parse(content).unwrap();
        assert_eq!(mem.frontmatter.memory_id, "mem_test123");
        assert_eq!(mem.frontmatter.memory_kind, "character_note");
        assert_eq!(mem.frontmatter.updated_at, "2026-04-14T12:00:00Z");
        assert_eq!(
            mem.frontmatter.source_session_ids,
            vec!["sess_001", "sess_002"]
        );
        assert!(mem.body.contains("Alice is a determined protagonist"));
    }

    #[test]
    fn parse_fails_without_frontmatter() {
        let content = "Just some text without frontmatter.";
        assert!(LongTermMemory::parse(content).is_err());
    }

    #[test]
    fn parse_fails_without_closing_delimiter() {
        let content = "---\nkey: value\n";
        assert!(LongTermMemory::parse(content).is_err());
    }

    #[test]
    fn parse_fails_with_invalid_yaml() {
        let content = "---\n: invalid yaml [[[\n---\nbody\n";
        assert!(LongTermMemory::parse(content).is_err());
    }

    #[test]
    fn parse_body_empty() {
        let content = "---\nnexus_memory_version: 1\nmemory_id: mem_x\nmemory_kind: custom\nupdated_at: '2026-01-01T00:00:00Z'\n---\n";
        let mem = LongTermMemory::parse(content).unwrap();
        assert!(mem.body.is_empty());
    }

    #[test]
    fn serde_frontmatter_roundtrip() {
        let fm = LongTermMemoryFrontmatter {
            nexus_memory_version: 1,
            memory_id: "mem_abc123".to_string(),
            memory_kind: "world_building".to_string(),
            updated_at: "2026-04-14T00:00:00Z".to_string(),
            source_session_ids: vec!["sess_1".to_string()],
        };
        let yaml = serde_yaml::to_string(&fm).unwrap();
        let parsed: LongTermMemoryFrontmatter = serde_yaml::from_str(&yaml).unwrap();
        assert_eq!(fm, parsed);
    }

    #[test]
    fn serde_frontmatter_empty_sessions() {
        let fm = LongTermMemoryFrontmatter {
            nexus_memory_version: 1,
            memory_id: "mem_xyz".to_string(),
            memory_kind: "theme_analysis".to_string(),
            updated_at: "2026-01-01T00:00:00Z".to_string(),
            source_session_ids: Vec::new(),
        };
        let yaml = serde_yaml::to_string(&fm).unwrap();
        // Empty vec should be skipped by skip_serializing_if
        assert!(!yaml.contains("source_session_ids"));
    }

    #[test]
    fn set_body_updates_timestamp() {
        let mut mem = LongTermMemory::new("story_summary");
        let original_ts = mem.frontmatter.updated_at.clone();
        // Small delay to ensure different timestamp
        std::thread::sleep(std::time::Duration::from_millis(2));
        mem.set_body("new body");
        assert_ne!(mem.frontmatter.updated_at, original_ts);
        assert_eq!(mem.body, "new body");
    }

    #[test]
    fn add_source_session_dedupes() {
        let mut mem = LongTermMemory::new("story_summary");
        mem.add_source_session("sess_1");
        mem.add_source_session("sess_1");
        mem.add_source_session("sess_2");
        assert_eq!(mem.frontmatter.source_session_ids.len(), 2);
    }

    #[test]
    fn touch_updates_timestamp() {
        let mut mem = LongTermMemory::new("story_summary");
        let original_ts = mem.frontmatter.updated_at.clone();
        std::thread::sleep(std::time::Duration::from_millis(2));
        mem.touch();
        assert_ne!(mem.frontmatter.updated_at, original_ts);
    }

    // ── slug_is_safe tests ───────────────────────────────────────────

    #[test]
    fn slug_safe_valid() {
        assert!(slug_is_safe("my-memory"));
        assert!(slug_is_safe("story_summary"));
        assert!(slug_is_safe("character-note-v2"));
        assert!(slug_is_safe("abc123"));
    }

    #[test]
    fn slug_safe_rejects_empty() {
        assert!(!slug_is_safe(""));
    }

    #[test]
    fn slug_safe_rejects_dotdot() {
        assert!(!slug_is_safe(".."));
        assert!(!slug_is_safe("foo..bar"));
        assert!(!slug_is_safe("../escape"));
    }

    #[test]
    fn slug_safe_rejects_slashes() {
        assert!(!slug_is_safe("foo/bar"));
        assert!(!slug_is_safe("foo\\bar"));
        assert!(!slug_is_safe("/absolute"));
    }

    #[test]
    fn slug_safe_rejects_control_chars() {
        assert!(!slug_is_safe("foo\0bar"));
        assert!(!slug_is_safe("foo\nbar"));
        assert!(!slug_is_safe("foo\tbar"));
    }

    #[test]
    fn extract_frontmatter_and_body_valid() {
        let content = "---\nkey: val\n---\nbody text";
        let (fm, body) = extract_frontmatter_and_body(content).unwrap();
        assert_eq!(fm, "key: val");
        assert_eq!(body, "body text");
    }

    #[test]
    fn extract_frontmatter_and_body_no_frontmatter() {
        let content = "no frontmatter here";
        assert!(extract_frontmatter_and_body(content).is_err());
    }

    #[test]
    fn extract_frontmatter_and_body_no_closing() {
        let content = "---\nkey: val\n";
        assert!(extract_frontmatter_and_body(content).is_err());
    }

    #[test]
    fn extract_frontmatter_and_body_empty_body() {
        let content = "---\nkey: val\n---\n";
        let (fm, body) = extract_frontmatter_and_body(content).unwrap();
        assert_eq!(fm, "key: val");
        assert!(body.is_empty());
    }
}
