//! `script.section_status.update` capability.
//!
//! V1.67 P2 (R-V160P1-QC1-W001): mirrors `game_bible.section_status.update`
//! for the script-writing profile.
//!
//! Updates the `section_status` field in a script work's `Scripts/*.md` file's
//! YAML frontmatter. Validates the transition (draft → reviewed → accepted)
//! and writes atomically via temp+rename.
//!
//! # Input
//!
//! - `work_ref`: Works directory slug (e.g. `my-screenplay`)
//! - `section_path`: relative path under `Scripts/` (e.g. `script.md`)
//! - `new_status`: one of `draft`, `reviewed`, `accepted`
//! - `reason` (optional): human-readable reason for the transition
//! - `works_root` (optional): override workspace root (default `Works`)
//!
//! # Transition rules
//!
//! Same as `game_bible.section_status.update`:
//! - `draft → reviewed`: initial review pass
//! - `reviewed → accepted`: explicit author accept
//! - No skipping (draft → accepted rejected)
//! - No backwards (accepted → draft rejected)
//! - No self-transition (draft → draft rejected)
//!
//! # Output
//!
//! - `updated`: whether the frontmatter was changed
//! - `new_section_status`: the new status value
//! - `section_path`: the full path to the updated file

use super::game_bible_section_status::{
    atomic_write, extract_frontmatter_field, replace_frontmatter_field, validate_transition,
};
use super::novel_scaffold_sanitize::validate_work_ref;
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tracing::info;

/// Input for `script.section_status.update`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
struct SectionStatusInput {
    work_ref: String,
    section_path: String,
    new_status: String,
    #[serde(default)]
    reason: Option<String>,
    #[serde(default)]
    works_root: Option<String>,
}

/// Output from `script.section_status.update`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct SectionStatusOutput {
    updated: bool,
    new_section_status: String,
    section_path: String,
}

/// `script.section_status.update` capability.
pub struct ScriptSectionStatusUpdate {
    works_root: PathBuf,
}

impl ScriptSectionStatusUpdate {
    #[must_use]
    pub fn new() -> Self {
        Self {
            works_root: PathBuf::from("Works"),
        }
    }

    #[must_use]
    pub const fn with_works_root(works_root: PathBuf) -> Self {
        Self { works_root }
    }
}

impl Default for ScriptSectionStatusUpdate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for ScriptSectionStatusUpdate {
    fn name(&self) -> &'static str {
        "script.section_status.update"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"work_ref":{"type":"string"},"section_path":{"type":"string"},"new_status":{"type":"string","enum":["draft","reviewed","accepted"]},"reason":{"type":"string"},"works_root":{"type":"string"}},"required":["work_ref","section_path","new_status"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"updated":{"type":"boolean"},"new_section_status":{"type":"string"},"section_path":{"type":"string"}},"required":["updated","new_section_status","section_path"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: SectionStatusInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("script.section_status.update input: {e}"))
        })?;

        // Validate work_ref against path traversal
        let work_ref = validate_work_ref(&inp.work_ref)?;

        // Resolve works_root
        let root = inp
            .works_root
            .as_deref()
            .map_or_else(|| self.works_root.clone(), PathBuf::from);

        let work_dir = root.join(&work_ref);
        let scripts_dir = work_dir.join("Scripts");
        let section_full_path = scripts_dir.join(&inp.section_path);

        info!(
            work_ref = %work_ref,
            section_path = %inp.section_path,
            new_status = %inp.new_status,
            reason = ?inp.reason,
            "script.section_status.update: start"
        );

        // Check section file exists
        if !section_full_path.exists() {
            return Err(CapabilityError::InputInvalid(format!(
                "section not found: Scripts/{} under work '{work_ref}'",
                inp.section_path
            )));
        }

        // Read current content
        let content = std::fs::read_to_string(&section_full_path).map_err(|e| {
            CapabilityError::Internal(format!(
                "read section file {}: {e}",
                section_full_path.display()
            ))
        })?;

        // Parse frontmatter to extract current section_status
        let current_status = extract_frontmatter_field(&content, "section_status")?;

        // Validate transition
        validate_transition(&current_status, &inp.new_status)?;

        // Replace the section_status field
        let updated_content =
            replace_frontmatter_field(&content, "section_status", &inp.new_status)?;

        // Atomic write via temp+rename
        atomic_write(&section_full_path, &updated_content)?;

        info!(
            work_ref = %work_ref,
            section_path = %inp.section_path,
            from = %current_status,
            to = %inp.new_status,
            "script.section_status.update: done"
        );

        let output = SectionStatusOutput {
            updated: true,
            new_section_status: inp.new_status,
            section_path: section_full_path.display().to_string(),
        };

        serde_json::to_value(output).map_err(|e| {
            CapabilityError::Internal(format!("script.section_status.update output: {e}"))
        })
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    #[test]
    fn capability_name_matches() {
        let cap = ScriptSectionStatusUpdate::new();
        assert_eq!(cap.name(), "script.section_status.update");
    }

    #[tokio::test]
    async fn run_updates_script_section_status_draft_to_reviewed() {
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let scripts_dir = works_root.join("my-screenplay").join("Scripts");
        std::fs::create_dir_all(&scripts_dir).expect("mkdir");
        let section_path = scripts_dir.join("script.md");
        std::fs::write(
            &section_path,
            "---\nsection_status: draft\nsection_weight: critical\n---\n\n# Script\n",
        )
        .expect("write");

        let cap = ScriptSectionStatusUpdate::with_works_root(works_root.clone());
        let output = cap
            .run(json!({
                "work_ref": "my-screenplay",
                "section_path": "script.md",
                "new_status": "reviewed",
                "reason": "review passed",
                "works_root": works_root.to_str().unwrap()
            }))
            .await
            .expect("run");

        assert_eq!(output["updated"], true);
        assert_eq!(output["new_section_status"], "reviewed");

        let updated = std::fs::read_to_string(&section_path).unwrap();
        assert!(updated.contains("section_status: reviewed"));
        assert!(!updated.contains("section_status: draft"));
    }

    #[tokio::test]
    async fn run_rejects_draft_to_accepted_skip() {
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let scripts_dir = works_root.join("my-screenplay").join("Scripts");
        std::fs::create_dir_all(&scripts_dir).expect("mkdir");
        std::fs::write(
            scripts_dir.join("script.md"),
            "---\nsection_status: draft\n---\n# Script\n",
        )
        .expect("write");

        let cap = ScriptSectionStatusUpdate::with_works_root(works_root.clone());
        let err = cap
            .run(json!({
                "work_ref": "my-screenplay",
                "section_path": "script.md",
                "new_status": "accepted",
                "works_root": works_root.to_str().unwrap()
            }))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("draft → accepted is not allowed"));
    }

    #[tokio::test]
    async fn run_rejects_section_not_found() {
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let scripts_dir = works_root.join("my-screenplay").join("Scripts");
        std::fs::create_dir_all(&scripts_dir).expect("mkdir");

        let cap = ScriptSectionStatusUpdate::with_works_root(works_root.clone());
        let err = cap
            .run(json!({
                "work_ref": "my-screenplay",
                "section_path": "missing.md",
                "new_status": "reviewed",
                "works_root": works_root.to_str().unwrap()
            }))
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("section not found"));
    }
}
