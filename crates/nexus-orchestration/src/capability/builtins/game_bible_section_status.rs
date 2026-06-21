//! `game_bible.section_status.update` capability.
//!
//! V1.56 P-last R-V155P2-F002 fix-wave: durable `section_status` auto-transition
//! for game-bible `design-writing` preset.
//!
//! Updates the `section_status` field in a game-bible `Design/*.md` file's
//! YAML frontmatter. Validates the transition (draft → reviewed → accepted)
//! and writes atomically via temp+rename.
//!
//! # Input
//!
//! - `work_ref`: Works directory slug (e.g. `my-game-design`)
//! - `section_path`: relative path under `Design/` (e.g. `overview.md`)
//! - `new_status`: one of `draft`, `reviewed`, `accepted`
//! - `reason` (optional): human-readable reason for the transition
//! - `works_root` (optional): override workspace root (default `Works`)
//!
//! # Transition rules
//!
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

use super::novel_scaffold_sanitize::validate_work_ref;
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::info;

/// Input for `game_bible.section_status.update`.
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

/// Output from `game_bible.section_status.update`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct SectionStatusOutput {
    updated: bool,
    new_section_status: String,
    section_path: String,
}

/// Valid transition states.
const VALID_STATUSES: &[&str] = &["draft", "reviewed", "accepted"];

/// Validate that `from → to` is a legal transition.
///
/// Returns the new status string on success.
fn validate_transition(from: &str, to: &str) -> Result<&'static str, CapabilityError> {
    // Validate status values are known
    if !VALID_STATUSES.contains(&from) {
        return Err(CapabilityError::InputInvalid(format!(
            "unknown from_status '{from}'; allowed: draft, reviewed, accepted"
        )));
    }
    if !VALID_STATUSES.contains(&to) {
        return Err(CapabilityError::InputInvalid(format!(
            "unknown new_status '{to}'; allowed: draft, reviewed, accepted"
        )));
    }

    match (from, to) {
        ("draft", "reviewed") => Ok("reviewed"),
        ("reviewed", "accepted") => Ok("accepted"),
        ("draft", "accepted") => Err(CapabilityError::InputInvalid(
            "invalid transition: draft → accepted is not allowed (use draft → reviewed → accepted)"
                .into(),
        )),
        ("accepted", _) => Err(CapabilityError::InputInvalid(format!(
            "invalid transition: cannot change accepted status to '{to}'"
        ))),
        ("reviewed", "draft") => Err(CapabilityError::InputInvalid(
            "invalid transition: reviewed → draft is not allowed".into(),
        )),
        _ if from == to => Err(CapabilityError::InputInvalid(format!(
            "no-op: section_status is already '{to}'"
        ))),
        _ => Err(CapabilityError::InputInvalid(format!(
            "invalid transition: {from} → {to}"
        ))),
    }
}

/// Parse YAML frontmatter from a file's content.
///
/// Returns the frontmatter lines (between `---` delimiters) and the body content.
fn parse_frontmatter(content: &str) -> Result<(Vec<String>, usize, usize), CapabilityError> {
    let lines: Vec<&str> = content.lines().collect();
    if lines.is_empty() || lines[0].trim() != "---" {
        return Err(CapabilityError::InputInvalid(
            "section file has no YAML frontmatter (missing opening ---)".into(),
        ));
    }
    // Find closing `---`
    let mut end_idx = None;
    for (i, line) in lines.iter().enumerate().skip(1) {
        if line.trim() == "---" {
            end_idx = Some(i);
            break;
        }
    }
    let end = end_idx.ok_or_else(|| {
        CapabilityError::InputInvalid(
            "section file has unclosed YAML frontmatter (missing closing ---)".into(),
        )
    })?;

    let fm_lines: Vec<String> = lines[1..end].iter().map(ToString::to_string).collect();
    Ok((fm_lines, 0, end))
}

/// Replace a frontmatter field value in the content.
///
/// Uses line-based matching: finds `key:` lines and replaces the value.
/// Preserves all other frontmatter fields and body content.
fn replace_frontmatter_field(
    content: &str,
    key: &str,
    new_value: &str,
) -> Result<String, CapabilityError> {
    let mut result = String::with_capacity(content.len());
    let mut in_frontmatter = false;
    let mut found_first_delim = false;
    let mut key_replaced = false;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if found_first_delim {
                in_frontmatter = false;
            } else {
                found_first_delim = true;
                in_frontmatter = true;
            }
            result.push_str(line);
            result.push('\n');
            continue;
        }

        if in_frontmatter && trimmed.starts_with(key) {
            if let Some(rest) = trimmed.strip_prefix(key) {
                if rest.starts_with(':') || rest.trim_start().starts_with(':') {
                    result.push_str(key);
                    result.push_str(": ");
                    result.push_str(new_value);
                    result.push('\n');
                    key_replaced = true;
                    continue;
                }
            }
        }

        result.push_str(line);
        result.push('\n');
    }

    if !key_replaced {
        return Err(CapabilityError::InputInvalid(format!(
            "frontmatter field '{key}' not found in file"
        )));
    }

    // Also update `last_updated` if present
    // This re-scans the output since the frontmatter loop already passed.
    // We do a second pass on the result to update last_updated.
    let now = chrono::Utc::now().to_rfc3339();
    let mut final_result = String::with_capacity(result.len());
    let mut in_fm = false;
    let mut found_first = false;
    let mut lu_replaced = false;

    for line in result.lines() {
        let trimmed = line.trim();
        if trimmed == "---" {
            if found_first {
                in_fm = false;
            } else {
                found_first = true;
                in_fm = true;
            }
            final_result.push_str(line);
            final_result.push('\n');
            continue;
        }
        if in_fm && trimmed.starts_with("last_updated") {
            if let Some(rest) = trimmed.strip_prefix("last_updated") {
                if rest.starts_with(':') || rest.trim_start().starts_with(':') {
                    final_result.push_str("last_updated: ");
                    final_result.push_str(&now);
                    final_result.push('\n');
                    lu_replaced = true;
                    continue;
                }
            }
        }
        final_result.push_str(line);
        final_result.push('\n');
    }

    // If last_updated wasn't present, insert it after the last frontmatter field
    // before closing ---. But this is complex with line-based approach.
    // For now, if it's not there, we won't add it (preserves exactly what author had).
    // Future: could add a full YAML parse layer.
    let _ = lu_replaced; // acceptable if absent — only updated when present

    Ok(final_result)
}

/// Write a file atomically using temp+rename.
fn atomic_write(path: &Path, content: &str) -> Result<(), CapabilityError> {
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, content)
        .map_err(|e| CapabilityError::Internal(format!("write tmp {}: {e}", tmp.display())))?;

    std::fs::rename(&tmp, path).map_err(|e| {
        // Try to clean up the temp file on rename failure
        let _ = std::fs::remove_file(&tmp);
        CapabilityError::Internal(format!(
            "rename {} -> {}: {e}",
            tmp.display(),
            path.display()
        ))
    })?;

    Ok(())
}

/// `game_bible.section_status.update` capability.
pub struct GameBibleSectionStatusUpdate {
    works_root: PathBuf,
}

impl GameBibleSectionStatusUpdate {
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

impl Default for GameBibleSectionStatusUpdate {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for GameBibleSectionStatusUpdate {
    fn name(&self) -> &'static str {
        "game_bible.section_status.update"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"work_ref":{"type":"string"},"section_path":{"type":"string"},"new_status":{"type":"string","enum":["draft","reviewed","accepted"]},"reason":{"type":"string"},"works_root":{"type":"string"}},"required":["work_ref","section_path","new_status"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"updated":{"type":"boolean"},"new_section_status":{"type":"string"},"section_path":{"type":"string"}},"required":["updated","new_section_status","section_path"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: SectionStatusInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("game_bible.section_status.update input: {e}"))
        })?;

        // Validate work_ref against path traversal
        let work_ref = validate_work_ref(&inp.work_ref)?;

        // Resolve works_root
        let root = inp
            .works_root
            .as_deref()
            .map_or_else(|| self.works_root.clone(), PathBuf::from);

        let work_dir = root.join(&work_ref);
        let design_dir = work_dir.join("Design");
        let section_full_path = design_dir.join(&inp.section_path);

        info!(
            work_ref = %work_ref,
            section_path = %inp.section_path,
            new_status = %inp.new_status,
            reason = ?inp.reason,
            "game_bible.section_status.update: start"
        );

        // Check section file exists
        if !section_full_path.exists() {
            return Err(CapabilityError::InputInvalid(format!(
                "section not found: Design/{} under work '{work_ref}'",
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
            "game_bible.section_status.update: done"
        );

        let output = SectionStatusOutput {
            updated: true,
            new_section_status: inp.new_status,
            section_path: section_full_path.display().to_string(),
        };

        serde_json::to_value(output).map_err(|e| {
            CapabilityError::Internal(format!("game_bible.section_status.update output: {e}"))
        })
    }
}

/// Extract a frontmatter field value from YAML frontmatter content.
fn extract_frontmatter_field(content: &str, key: &str) -> Result<String, CapabilityError> {
    let parsed = parse_frontmatter(content)?;
    let fm_lines = parsed.0;

    for line in &fm_lines {
        let trimmed = line.trim();
        if trimmed.starts_with(key) {
            if let Some(rest) = trimmed.strip_prefix(key) {
                let rest = rest.trim_start();
                if let Some(stripped) = rest.strip_prefix(':') {
                    let value = stripped.trim().to_string();
                    if value.is_empty() {
                        return Err(CapabilityError::InputInvalid(format!(
                            "frontmatter field '{key}' has empty value"
                        )));
                    }
                    return Ok(value);
                }
            }
        }
    }

    Err(CapabilityError::InputInvalid(format!(
        "frontmatter field '{key}' not found"
    )))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // ── Transition validation ──

    #[test]
    fn transition_draft_to_reviewed_valid() {
        assert!(validate_transition("draft", "reviewed").is_ok());
    }

    #[test]
    fn transition_reviewed_to_accepted_valid() {
        assert!(validate_transition("reviewed", "accepted").is_ok());
    }

    #[test]
    fn transition_draft_to_accepted_rejected() {
        let err = validate_transition("draft", "accepted").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("draft → accepted is not allowed"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn transition_accepted_to_draft_rejected() {
        let err = validate_transition("accepted", "draft").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("cannot change accepted status"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn transition_accepted_to_reviewed_rejected() {
        let err = validate_transition("accepted", "reviewed").unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("cannot change accepted status"),
            "unexpected error: {msg}"
        );
    }

    #[test]
    fn transition_reviewed_to_draft_rejected() {
        let err = validate_transition("reviewed", "draft").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("reviewed → draft is not allowed"));
    }

    #[test]
    fn transition_same_status_rejected() {
        let err = validate_transition("draft", "draft").unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("already 'draft'"), "unexpected: {msg}");
    }

    #[test]
    fn transition_unknown_from_status_rejected() {
        assert!(validate_transition("bogus", "draft").is_err());
    }

    #[test]
    fn transition_unknown_to_status_rejected() {
        assert!(validate_transition("draft", "bogus").is_err());
    }

    // ── Frontmatter extraction ──

    #[test]
    fn extract_section_status_draft() {
        let content = "---\nsection_status: draft\nsection_weight: critical\n---\n\n# Overview\n";
        let status = extract_frontmatter_field(content, "section_status").unwrap();
        assert_eq!(status, "draft");
    }

    #[test]
    fn extract_section_status_accepted() {
        let content = "---\nsection_status: accepted\n---\n# Done\n";
        let status = extract_frontmatter_field(content, "section_status").unwrap();
        assert_eq!(status, "accepted");
    }

    #[test]
    fn extract_section_status_missing_returns_error() {
        let content = "---\nsection_weight: critical\n---\n# No status\n";
        assert!(extract_frontmatter_field(content, "section_status").is_err());
    }

    #[test]
    fn extract_from_no_frontmatter_returns_error() {
        let content = "# No frontmatter\n";
        assert!(extract_frontmatter_field(content, "section_status").is_err());
    }

    // ── Frontmatter replacement ──

    #[test]
    fn replace_section_status_draft_to_reviewed() {
        let content = "---\nsection_status: draft\nsection_weight: critical\n---\n\n# Overview\n";
        let updated = replace_frontmatter_field(content, "section_status", "reviewed").unwrap();
        assert!(updated.contains("section_status: reviewed"));
        assert!(updated.contains("section_weight: critical"));
        assert!(updated.contains("# Overview"));
        assert!(!updated.contains("section_status: draft"));
    }

    #[test]
    fn replace_preserves_other_fields() {
        let content = "---\nsection_status: draft\nsection_weight: important\nlast_updated: 2026-01-01T00:00:00Z\n---\n# Characters\n";
        let updated = replace_frontmatter_field(content, "section_status", "accepted").unwrap();
        assert!(updated.contains("section_status: accepted"));
        assert!(updated.contains("section_weight: important"));
        // last_updated should be updated
        assert!(updated.contains("last_updated: "));
    }

    #[test]
    fn replace_non_existent_field_errors() {
        let content = "---\n---\n# Empty\n";
        assert!(replace_frontmatter_field(content, "section_status", "reviewed").is_err());
    }

    // ── Atomic write ──

    #[test]
    fn atomic_write_creates_file() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let path = tmp.path().join("test.md");
        let content = "# Test\n";
        atomic_write(&path, content).expect("write");
        assert!(path.exists());
        let read = std::fs::read_to_string(&path).unwrap();
        assert_eq!(read, content);
        // Temp file should be cleaned up
        assert!(!path.with_extension("tmp").exists());
    }

    // ── Capability name ──

    #[test]
    fn capability_name_matches() {
        let cap = GameBibleSectionStatusUpdate::new();
        assert_eq!(cap.name(), "game_bible.section_status.update");
    }

    // ── Capability run with temp dir ──

    #[tokio::test]
    async fn run_updates_section_status_draft_to_reviewed() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let work_dir = works_root.join("my-design");
        let design_dir = work_dir.join("Design");
        std::fs::create_dir_all(&design_dir).expect("mkdir");
        let section_path = design_dir.join("overview.md");
        std::fs::write(
            &section_path,
            "---\nsection_status: draft\nsection_weight: critical\n---\n\n# Overview\n",
        )
        .expect("write");

        let cap = GameBibleSectionStatusUpdate::with_works_root(works_root.clone());

        let input = serde_json::json!({
            "work_ref": "my-design",
            "section_path": "overview.md",
            "new_status": "reviewed",
            "reason": "review passed",
            "works_root": works_root.to_str().unwrap()
        });

        let output = cap.run(input).await.expect("run");
        assert_eq!(output["updated"], true);
        assert_eq!(output["new_section_status"], "reviewed");

        let updated_content = std::fs::read_to_string(&section_path).unwrap();
        assert!(updated_content.contains("section_status: reviewed"));
        assert!(!updated_content.contains("section_status: draft"));
    }

    #[tokio::test]
    async fn run_rejects_draft_to_accepted() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let work_dir = works_root.join("my-design");
        let design_dir = work_dir.join("Design");
        std::fs::create_dir_all(&design_dir).expect("mkdir");
        std::fs::write(
            design_dir.join("overview.md"),
            "---\nsection_status: draft\n---\n# Overview\n",
        )
        .expect("write");

        let cap = GameBibleSectionStatusUpdate::with_works_root(works_root.clone());

        let input = serde_json::json!({
            "work_ref": "my-design",
            "section_path": "overview.md",
            "new_status": "accepted",
            "works_root": works_root.to_str().unwrap()
        });

        let err = cap.run(input).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("draft → accepted is not allowed"),
            "unexpected: {msg}"
        );
    }

    #[tokio::test]
    async fn run_rejects_accepted_to_draft() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let work_dir = works_root.join("my-design");
        let design_dir = work_dir.join("Design");
        std::fs::create_dir_all(&design_dir).expect("mkdir");
        std::fs::write(
            design_dir.join("overview.md"),
            "---\nsection_status: accepted\n---\n# Overview\n",
        )
        .expect("write");

        let cap = GameBibleSectionStatusUpdate::with_works_root(works_root.clone());

        let input = serde_json::json!({
            "work_ref": "my-design",
            "section_path": "overview.md",
            "new_status": "draft",
            "works_root": works_root.to_str().unwrap()
        });

        let err = cap.run(input).await.unwrap_err();
        let msg = format!("{err}");
        assert!(
            msg.contains("cannot change accepted status"),
            "unexpected: {msg}"
        );
    }

    #[tokio::test]
    async fn run_rejects_section_not_found() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let work_dir = works_root.join("my-design");
        std::fs::create_dir_all(work_dir.join("Design")).expect("mkdir");

        let cap = GameBibleSectionStatusUpdate::with_works_root(works_root.clone());

        let input = serde_json::json!({
            "work_ref": "my-design",
            "section_path": "nonexistent.md",
            "new_status": "reviewed",
            "works_root": works_root.to_str().unwrap()
        });

        let err = cap.run(input).await.unwrap_err();
        let msg = format!("{err}");
        assert!(msg.contains("section not found"), "unexpected: {msg}");
    }

    #[tokio::test]
    async fn run_preserves_other_frontmatter_fields() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let work_dir = works_root.join("my-design");
        let design_dir = work_dir.join("Design");
        std::fs::create_dir_all(&design_dir).expect("mkdir");
        let section_path = design_dir.join("pillars.md");
        std::fs::write(
            &section_path,
            "---\nsection_status: draft\nsection_weight: critical\nlast_updated: 2026-01-01T00:00:00Z\n---\n\n# Pillars\n",
        )
        .expect("write");

        let cap = GameBibleSectionStatusUpdate::with_works_root(works_root.clone());

        let input = serde_json::json!({
            "work_ref": "my-design",
            "section_path": "pillars.md",
            "new_status": "reviewed",
            "works_root": works_root.to_str().unwrap()
        });

        let output = cap.run(input).await.expect("run");
        assert_eq!(output["updated"], true);

        let updated = std::fs::read_to_string(&section_path).unwrap();
        assert!(updated.contains("section_status: reviewed"));
        assert!(updated.contains("section_weight: critical"));
        assert!(updated.contains("last_updated: ")); // updated with current timestamp
        assert!(updated.contains("# Pillars"));
    }

    #[tokio::test]
    async fn run_atomic_write_no_temp_file_left() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let works_root = tmp.path().to_path_buf();
        let work_dir = works_root.join("my-design");
        let design_dir = work_dir.join("Design");
        std::fs::create_dir_all(&design_dir).expect("mkdir");
        let section_path = design_dir.join("overview.md");
        std::fs::write(
            &section_path,
            "---\nsection_status: draft\n---\n# Overview\n",
        )
        .expect("write");

        let cap = GameBibleSectionStatusUpdate::with_works_root(works_root.clone());

        let input = serde_json::json!({
            "work_ref": "my-design",
            "section_path": "overview.md",
            "new_status": "reviewed",
            "works_root": works_root.to_str().unwrap()
        });

        cap.run(input).await.expect("run");

        // No temp file left behind
        assert!(!section_path.with_extension("tmp").exists());
        assert!(section_path.exists());
    }
}
