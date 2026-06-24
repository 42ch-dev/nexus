//! `essay.draft_status.finalize` capability.
//!
//! V1.63 P2: writes `status: finalized` to the YAML frontmatter of an essay's
//! `Drafts/draft.md` after the 4-dimension rubric passes. This triggers
//! `is_essay_complete()` → `works.status = completed`.
//!
//! # Input
//!
//! - `work_ref`: Works directory slug (e.g. `my-essay`)
//! - `works_root` (optional): override workspace root (default `Works`)
//!
//! # Output
//!
//! - `updated`: whether the frontmatter was changed
//! - `draft_path`: the full path to the updated file
//! - `word_count`: the word count written to frontmatter
//!
//! # Atomicity
//!
//! Writes via temp+rename for atomicity.

use super::novel_scaffold_sanitize::validate_work_ref;
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tracing::info;

/// Input for `essay.draft_status.finalize`.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
struct FinalizeDraftInput {
    work_ref: String,
    #[serde(default)]
    works_root: Option<String>,
    #[serde(default)]
    word_count: Option<WordCount>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WordCount {
    Auto,
}

/// Output from `essay.draft_status.finalize`.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct FinalizeDraftOutput {
    updated: bool,
    draft_path: String,
    word_count: usize,
}

/// `essay.draft_status.finalize` capability.
pub struct EssayDraftStatusFinalize {
    works_root: PathBuf,
}

impl EssayDraftStatusFinalize {
    /// Create with default Works root.
    #[must_use]
    pub fn new() -> Self {
        Self {
            works_root: PathBuf::from("Works"),
        }
    }
}

impl Default for EssayDraftStatusFinalize {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for EssayDraftStatusFinalize {
    fn name(&self) -> &'static str {
        "essay.draft_status.finalize"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"work_ref":{"type":"string"},"works_root":{"type":"string"},"word_count":{"anyOf":[{"type":"string","enum":["auto"]},{"type":"integer"}]}},"required":["work_ref"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"updated":{"type":"boolean"},"draft_path":{"type":"string"},"word_count":{"type":"integer"}},"required":["updated","draft_path","word_count"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: FinalizeDraftInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("essay.draft_status.finalize input: {e}"))
        })?;

        let work_ref = validate_work_ref(&inp.work_ref)?;

        info!(
            work_ref = %work_ref,
            "essay.draft_status.finalize: start"
        );

        let works_root = inp
            .works_root
            .map_or_else(|| self.works_root.clone(), PathBuf::from);
        let draft_path = works_root.join(&work_ref).join("Drafts").join("draft.md");

        // Read current draft
        let content = tokio::fs::read_to_string(&draft_path)
            .await
            .map_err(|e| CapabilityError::Internal(format!("read draft.md: {e}")))?;

        // Parse and update YAML frontmatter
        let updated_content = update_frontmatter_status(&content)?;

        // Count words in body (excluding frontmatter)
        let word_count = count_body_words(&updated_content);

        // Write atomically via temp+rename
        let tmp_path = draft_path.with_extension("md.tmp");
        tokio::fs::write(&tmp_path, &updated_content)
            .await
            .map_err(|e| CapabilityError::Internal(format!("write tmp draft.md: {e}")))?;
        tokio::fs::rename(&tmp_path, &draft_path)
            .await
            .map_err(|e| {
                // Clean up temp on rename failure
                let _ = std::fs::remove_file(&tmp_path);
                CapabilityError::Internal(format!("rename tmp to draft.md: {e}"))
            })?;

        info!(
            work_ref = %work_ref,
            draft_path = %draft_path.display(),
            word_count,
            "essay.draft_status.finalize: done"
        );

        let output = FinalizeDraftOutput {
            updated: true,
            draft_path: draft_path.display().to_string(),
            word_count,
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

/// Update the `status` field in YAML frontmatter to `finalized`, and update
/// `word_count` to the auto-counted value.
///
/// Frontmatter is delimited by `---` lines at the start of the file.
/// Returns the full content with updated frontmatter.
fn update_frontmatter_status(content: &str) -> Result<String, CapabilityError> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return Err(CapabilityError::InputInvalid(
            "draft.md does not have YAML frontmatter (missing opening '---')".into(),
        ));
    }

    // Find the closing `---` after the opening.
    let after_open = &trimmed[3..]; // skip opening ---
    let closing_idx = after_open
        .find("\n---")
        .or_else(|| after_open.find("\r\n---"));
    let Some(closing_idx) = closing_idx else {
        return Err(CapabilityError::InputInvalid(
            "draft.md frontmatter is not closed (missing closing '---')".into(),
        ));
    };

    let fm_body = &after_open[..closing_idx];
    let rest = &after_open[closing_idx..]; // includes \n--- and body

    // Build new frontmatter: same keys but status=finalized + word_count updated.
    let word_count = count_body_words(rest);

    let mut new_fm = String::new();
    let mut found_status = false;
    let mut found_word_count = false;
    let mut found_title = false;

    for line in fm_body.lines() {
        let trimmed_line = line.trim();
        if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
            // Preserve empty lines and comments
            if !new_fm.is_empty() {
                new_fm.push('\n');
            }
            new_fm.push_str(line);
            continue;
        }
        if let Some((key, _value)) = trimmed_line.split_once(':') {
            let key = key.trim();
            match key {
                "status" => {
                    if !new_fm.is_empty() {
                        new_fm.push('\n');
                    }
                    new_fm.push_str("status: finalized");
                    found_status = true;
                }
                "word_count" => {
                    if !new_fm.is_empty() {
                        new_fm.push('\n');
                    }
                    let _ = std::fmt::Write::write_fmt(
                        &mut new_fm,
                        format_args!("word_count: {word_count}"),
                    );
                    found_word_count = true;
                }
                _ => {
                    if !new_fm.is_empty() {
                        new_fm.push('\n');
                    }
                    new_fm.push_str(line);
                    if key == "title" {
                        found_title = true;
                    }
                }
            }
        } else {
            // Non-key-value line — preserve as-is
            if !new_fm.is_empty() {
                new_fm.push('\n');
            }
            new_fm.push_str(line);
        }
    }

    // Ensure status and word_count exist (insert if missing).
    if !found_status {
        new_fm.push_str("\nstatus: finalized");
    }
    if !found_word_count {
        let _ = std::fmt::Write::write_fmt(&mut new_fm, format_args!("\nword_count: {word_count}"));
    }

    // Ensure title comes first if present (simple reorder)
    let new_fm = if found_title {
        reorder_frontmatter_title_first(&new_fm)
    } else {
        new_fm
    };

    // `rest` already starts with `\n---` (it's the slice from the closing
    // delimiter onward), so we must NOT add another `\n---` here — that
    // would produce a double closing delimiter and corrupt the frontmatter.
    Ok(format!("---\n{new_fm}{rest}"))
}

/// Reorder frontmatter so `title` comes first.
fn reorder_frontmatter_title_first(fm: &str) -> String {
    let mut lines: Vec<&str> = fm.lines().collect();
    if let Some(title_pos) = lines.iter().position(|l| l.trim().starts_with("title:")) {
        if title_pos > 0 {
            let title_line = lines.remove(title_pos);
            lines.insert(0, title_line);
        }
    }
    lines.join("\n")
}

/// Count words in the essay body (after frontmatter `---` closing).
fn count_body_words(text: &str) -> usize {
    // Skip past the closing `---` of frontmatter
    let body = text.find("\n---").map_or(text, |idx| &text[idx + 4..]);
    body.split_whitespace().count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capability_name() {
        let cap = EssayDraftStatusFinalize::new();
        assert_eq!(cap.name(), "essay.draft_status.finalize");
    }

    #[test]
    fn update_frontmatter_sets_status_to_finalized() {
        let content = "---\ntitle: Test Essay\nstatus: draft\nword_count: 42\n---\n\n# Test\n\nSome content here.";
        let result = update_frontmatter_status(content).unwrap();
        assert!(
            result.contains("status: finalized"),
            "expected 'status: finalized' in result: {result}"
        );
        assert!(
            !result.contains("status: draft"),
            "should not contain old status: {result}"
        );
        assert!(
            result.contains("# Test"),
            "body content should be preserved: {result}"
        );
        assert!(
            result.contains("Some content here"),
            "body content preserved: {result}"
        );
    }

    #[test]
    fn update_frontmatter_updates_word_count() {
        let content =
            "---\ntitle: Test\nstatus: draft\nword_count: 0\n---\n\none two three four five";
        let result = update_frontmatter_status(content).unwrap();
        // word_count should be recalculated
        assert!(
            result.contains("word_count: 5"),
            "expected word_count: 5 in result: {result}"
        );
    }

    #[test]
    fn update_frontmatter_adds_status_if_missing() {
        let content = "---\ntitle: Test\n---\n\nbody text";
        let result = update_frontmatter_status(content).unwrap();
        assert!(
            result.contains("status: finalized"),
            "should add status: finalized when missing: {result}"
        );
    }

    #[test]
    fn update_frontmatter_preserves_other_fields() {
        let content =
            "---\ntitle: My Essay\naudience: general\nstatus: revised\nword_count: 100\n---\n\nbody";
        let result = update_frontmatter_status(content).unwrap();
        assert!(
            result.contains("title: My Essay"),
            "should preserve title: {result}"
        );
        assert!(
            result.contains("audience: general"),
            "should preserve audience: {result}"
        );
        assert!(
            result.contains("status: finalized"),
            "should change status: {result}"
        );
    }

    #[test]
    fn count_body_words_handles_simple_text() {
        assert_eq!(count_body_words("one two three"), 3);
        assert_eq!(count_body_words(""), 0);
        assert_eq!(count_body_words("a b c d e f g h i j"), 10);
    }

    #[test]
    fn update_frontmatter_no_double_closing_delimiter() {
        // Regression: PR #86 Greptile review found that the format string
        // emitted a redundant `\n---` because `rest` already carries the
        // closing delimiter. This produced malformed YAML frontmatter:
        // `---\n<fm>\n---\n---\n<body>` (double closing delimiter).
        let content = "---\ntitle: Test\nstatus: draft\nword_count: 0\n---\n\none two three";
        let result = update_frontmatter_status(content).unwrap();

        // Count occurrences of lines that are exactly `---`.
        let delimiter_count = result.lines().filter(|l| *l == "---").count();
        assert_eq!(
            delimiter_count, 2,
            "expected exactly 2 `---` delimiters (opening + closing), got {delimiter_count}.\nResult:\n{result}"
        );
    }

    #[test]
    fn update_frontmatter_idempotent_on_retry() {
        // Regression: re-running the capability on an already-finalized
        // file must not compound corruption. Running twice should produce
        // the same well-formed output as running once.
        let content = "---\ntitle: Test\nstatus: draft\nword_count: 0\n---\n\none two three";
        let once = update_frontmatter_status(content).unwrap();
        let twice = update_frontmatter_status(&once).unwrap();

        // Both runs should have exactly 2 `---` delimiters.
        for (label, text) in [("once", &once), ("twice", &twice)] {
            let count = text.lines().filter(|l| *l == "---").count();
            assert_eq!(
                count, 2,
                "idempotent run {label}: expected 2 delimiters, got {count}.\n{text}"
            );
        }
        // Body content preserved across both runs.
        assert!(twice.contains("one two three"));
        assert!(twice.contains("status: finalized"));
    }

    #[test]
    fn update_frontmatter_word_count_not_off_by_one() {
        // Regression: the double `---` bug caused count_body_words to
        // find the spurious first `---` and then count `---` as a word.
        let content = "---\ntitle: Test\nstatus: draft\nword_count: 0\n---\n\nalpha beta gamma";
        let result = update_frontmatter_status(content).unwrap();
        // 3 words in body; the bug would have counted 4 (--- as a word).
        assert!(
            result.contains("word_count: 3"),
            "expected word_count: 3, result has wrong count.\n{result}"
        );
    }
}
