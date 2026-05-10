//! Embedded skills module.
//!
//! Provides compile-time embedded access to skill documents that ship with the
//! nexus binary.  Skills are markdown-based guidance packs consumed by ACP
//! agents during orchestration sessions (e.g., novel-writing, world-building).
//!
//! Layout on disk (relative to `crates/nexus-orchestration/`):
//!
//! ```text
//! embedded-skills/
//! ├── SKILL_MANIFEST.json
//! └── <skill-id>/
//!     └── SKILL.md
//! ```
//!
//! The directory tree is embedded via `include_dir!` — no runtime filesystem
//! reads are performed.

use include_dir::include_dir;
use include_dir::Dir;
use serde::Deserialize;

// ---------------------------------------------------------------------------
// Embedded directory
// ---------------------------------------------------------------------------

/// Embedded skills directory, compiled into the binary at build time.
///
/// Location: `crates/nexus-orchestration/embedded-skills/`
static EMBEDDED_SKILLS: Dir = include_dir!("$CARGO_MANIFEST_DIR/embedded-skills");

/// Return a reference to the compile-time embedded skills directory.
#[must_use]
pub fn embedded_skill_dir() -> &'static Dir<'static> {
    &EMBEDDED_SKILLS
}

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// A single entry from the embedded skill manifest, enriched with the full
/// markdown content of the skill document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EmbeddedSkillEntry {
    /// Unique skill identifier (e.g., `"novel-writing-assistant"`).
    pub id: String,
    /// Monotonically increasing version of the embedded skill.
    pub version: u32,
    /// Human-readable description of the skill.
    pub description: String,
    /// Relative path to the SKILL.md file within the `embedded-skills/` tree.
    pub source: String,
    /// Full UTF-8 content of the SKILL.md file.
    pub content: String,
}

/// Intermediate struct for deserialising the skill manifest JSON.
#[derive(Debug, Deserialize)]
struct SkillManifest {
    #[allow(dead_code)]
    schema_version: u32,
    skills: Vec<SkillManifestEntry>,
}

/// A single skill record inside `SKILL_MANIFEST.json`.
#[derive(Debug, Deserialize)]
struct SkillManifestEntry {
    id: String,
    version: u32,
    description: String,
    source: String,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse `SKILL_MANIFEST.json` from the embedded directory and return
/// structured entries, each including the full content of its SKILL.md file.
///
/// # Panics
///
/// Panics if `SKILL_MANIFEST.json` is missing, is not valid UTF-8, or fails
/// to deserialize.  These are compile-time invariant violations (the file is
/// embedded) and should never occur in a correctly built binary.
#[must_use]
pub fn list_embedded_skills() -> Vec<EmbeddedSkillEntry> {
    let manifest_raw = EMBEDDED_SKILLS
        .get_file("SKILL_MANIFEST.json")
        .expect("SKILL_MANIFEST.json must exist in embedded-skills/")
        .contents_utf8()
        .expect("SKILL_MANIFEST.json must be valid UTF-8");

    let manifest: SkillManifest =
        serde_json::from_str(manifest_raw).expect("SKILL_MANIFEST.json must be valid JSON");

    manifest
        .skills
        .into_iter()
        .map(|entry| {
            let content = EMBEDDED_SKILLS
                .get_file(&*entry.source)
                .unwrap_or_else(|| {
                    panic!(
                        "embedded-skills/{} referenced by manifest must exist",
                        entry.source
                    )
                })
                .contents_utf8()
                .unwrap_or_else(|| panic!("embedded-skills/{} must be valid UTF-8", entry.source))
                .to_string();

            EmbeddedSkillEntry {
                id: entry.id,
                version: entry.version,
                description: entry.description,
                source: entry.source,
                content,
            }
        })
        .collect()
}

/// Look up an embedded skill by its unique identifier.
///
/// Returns `None` when no skill with the given `id` exists in the manifest.
#[must_use]
pub fn get_embedded_skill(id: &str) -> Option<EmbeddedSkillEntry> {
    list_embedded_skills().into_iter().find(|s| s.id == id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifest_is_valid_json() {
        let raw = EMBEDDED_SKILLS
            .get_file("SKILL_MANIFEST.json")
            .expect("SKILL_MANIFEST.json must exist")
            .contents_utf8()
            .expect("must be valid UTF-8");

        // If this parses, the manifest is valid JSON.
        let parsed: serde_json::Value =
            serde_json::from_str(raw).expect("SKILL_MANIFEST.json must be valid JSON");

        // Sanity check top-level structure.
        assert!(parsed.get("schema_version").is_some());
        assert!(parsed.get("skills").is_some());
    }

    #[test]
    fn embedded_skill_dir_is_not_empty() {
        let dir = embedded_skill_dir();
        assert!(
            !dir.files().collect::<Vec<_>>().is_empty()
                || !dir.dirs().collect::<Vec<_>>().is_empty(),
            "embedded skills directory must contain at least one file or subdirectory"
        );
    }

    #[test]
    fn list_embedded_skills_returns_at_least_one() {
        let skills = list_embedded_skills();
        assert!(!skills.is_empty(), "expected at least one embedded skill");
    }

    #[test]
    fn get_embedded_skill_novel_writing_assistant() {
        let skill = get_embedded_skill("novel-writing-assistant")
            .expect("novel-writing-assistant skill must exist");

        assert_eq!(skill.id, "novel-writing-assistant");
        assert_eq!(skill.version, 1);
        assert!(!skill.description.is_empty());
        assert_eq!(skill.source, "novel-writing-assistant/SKILL.md");
        assert!(
            !skill.content.is_empty(),
            "SKILL.md content must not be empty"
        );
        // Content should be a markdown file with a heading.
        assert!(
            skill.content.contains("# "),
            "SKILL.md should contain at least one markdown heading"
        );
    }

    #[test]
    fn get_embedded_skill_nonexistent_returns_none() {
        assert!(
            get_embedded_skill("nonexistent").is_none(),
            "nonexistent skill ID should return None"
        );
    }
}
