//! `script.project_scaffold` capability.
//!
//! V1.55 P3: third non-novel profile scaffold.
//! Creates `Works/<work_ref>/` with script-specific templates:
//! `Scripts/script.md`, `Beats/beat-sheet.md`, `Characters/characters.md`,
//! a README.md, `Logs/write/` and `Logs/review/` directories,
//! and `PATCH`es the works row to set `work_profile = 'script'` and `work_ref`.
//!
//! # Concurrency note
//!
//! This capability runs in the single-user daemon process. We assume:
//! 1. Only one `script.project_scaffold` invocation per `(creator_id, work_id)`
//!    is in flight at any time.
//! 2. No external process is mutating `Works/<work_ref>/` while this runs.
//!
//! # `ScaffoldTransaction` (V1.55 P3 / R-V154P1-W001)
//!
//! Wraps FS writes + DB PATCH in a `ScaffoldTransaction` with Drop-based FS
//! rollback. Pattern adopted from `novel.project_scaffold` (novel_scaffold.rs:763-830).

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tracing::info;

use crate::capability::{Capability, CapabilityError};

/// Input for the script scaffold capability.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
#[allow(dead_code)]
struct ScaffoldInput {
    creator_id: String,
    work_id: String,
    work_ref: String,
    title: String,
    #[serde(default)]
    world_id: Option<String>,
}

/// Output from the script scaffold capability.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ScaffoldOutput {
    scaffold_root: String,
    files_created: Vec<String>,
    dirs_created: Vec<String>,
}

/// Predefined script template content with YAML frontmatter.
#[allow(dead_code)]
struct ScriptTemplate {
    filename: &'static str,
    section_weight: &'static str,
    title: &'static str,
    comment: &'static str,
}

const SCRIPT_TEMPLATES: &[ScriptTemplate] = &[
    ScriptTemplate {
        filename: "script.md",
        section_weight: "critical",
        title: "Script",
        comment: "Scene headings, dialogue, action lines, parentheticals",
    },
    ScriptTemplate {
        filename: "beat-sheet.md",
        section_weight: "critical",
        title: "Beat Sheet",
        comment: "Story beats, scene outline, act structure",
    },
    ScriptTemplate {
        filename: "characters.md",
        section_weight: "important",
        title: "Characters",
        comment: "Character directions, casting notes, arc tracking",
    },
];

fn render_template(tmpl: &ScriptTemplate) -> String {
    format!(
        "---\nsection_status: draft\nsection_weight: {}\n---\n\n# {}\n<!-- {} -->\n",
        tmpl.section_weight, tmpl.title, tmpl.comment,
    )
}

// ── ScaffoldTransaction (V1.55 P3 / R-V154P1-W001) ─────────────────────────
//
// Wraps the in-flight FS scaffold so that, if any subsequent step (template
// render, works PATCH) returns an error before `commit()` is called, the
// guard's `Drop` impl removes only the files and directories THIS invocation
// created. Files/dirs that pre-existed (e.g. re-init over a partially-scaffolded
// tree) are left untouched.
//
// Pattern adopted from novel.project_scaffold (novel_scaffold.rs:763-830).

struct ScaffoldTransaction {
    files_created: Vec<PathBuf>,
    dirs_created: Vec<PathBuf>,
    committed: bool,
}

impl ScaffoldTransaction {
    const fn new() -> Self {
        Self {
            files_created: Vec::new(),
            dirs_created: Vec::new(),
            committed: false,
        }
    }

    const fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for ScaffoldTransaction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for f in &self.files_created {
            if let Err(e) = std::fs::remove_file(f) {
                tracing::warn!(
                    path = %f.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove_file failed"
                );
            }
        }
        for d in self.dirs_created.iter().rev() {
            if let Err(e) = std::fs::remove_dir(d) {
                tracing::warn!(
                    path = %d.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove_dir failed (likely non-empty — expected)"
                );
            }
        }
        tracing::warn!(
            files = self.files_created.len(),
            dirs = self.dirs_created.len(),
            "script.project_scaffold: rolled back filesystem state"
        );
    }
}

/// `script.project_scaffold` capability.
pub struct ScriptProjectScaffold {
    pool: Option<sqlx::SqlitePool>,
    works_root: PathBuf,
}

impl ScriptProjectScaffold {
    /// Create a standalone (no-pool) instance for testing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: None,
            works_root: PathBuf::from("Works"),
        }
    }

    /// Create an instance with a DB pool (default Works root).
    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(pool),
            works_root: PathBuf::from("Works"),
        }
    }

    /// Create an instance with a DB pool and custom Works root (for e2e tests).
    #[must_use]
    pub const fn new_with_root(pool: sqlx::SqlitePool, works_root: PathBuf) -> Self {
        Self {
            pool: Some(pool),
            works_root,
        }
    }
}

impl Default for ScriptProjectScaffold {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for ScriptProjectScaffold {
    fn name(&self) -> &'static str {
        "script.project_scaffold"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]}},"required":["creator_id","work_id","work_ref","title"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"scaffold_root":{"type":"string"},"files_created":{"type":"array","items":{"type":"string"}},"dirs_created":{"type":"array","items":{"type":"string"}}},"required":["scaffold_root","files_created","dirs_created"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: ScaffoldInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("script.project_scaffold input: {e}"))
        })?;

        info!(
            work_id = %inp.work_id,
            work_ref = %inp.work_ref,
            world_id = ?inp.world_id,
            "script.project_scaffold: start"
        );

        let work_dir = self.works_root.join(&inp.work_ref);
        let scripts_dir = work_dir.join("Scripts");
        let beats_dir = work_dir.join("Beats");
        let characters_dir = work_dir.join("Characters");
        let logs_dir = work_dir.join("Logs");
        let logs_write_dir = logs_dir.join("write");
        let logs_review_dir = logs_dir.join("review");

        let mut tx = ScaffoldTransaction::new();

        // Create directory structure
        for dir in [
            &work_dir,
            &scripts_dir,
            &beats_dir,
            &characters_dir,
            &logs_dir,
            &logs_write_dir,
            &logs_review_dir,
        ] {
            if !dir.exists() {
                std::fs::create_dir_all(dir).map_err(|e| {
                    CapabilityError::Internal(format!("mkdir {}: {e}", dir.display()))
                })?;
                tx.dirs_created.push(dir.clone());
            }
        }

        // Write README.md
        let readme_path = work_dir.join("README.md");
        let readme_content = format!(
            "# {title}\n\nScript project.\n\n- **Work ID**: {work_id}\n- **Profile**: script\n",
            title = inp.title,
            work_id = inp.work_id,
        );
        std::fs::write(&readme_path, &readme_content)
            .map_err(|e| CapabilityError::Internal(format!("write README.md: {e}")))?;
        tx.files_created.push(readme_path.clone());

        // Write Scripts/script.md
        let script_path = scripts_dir.join("script.md");
        let script_content = render_template(&SCRIPT_TEMPLATES[0]);
        std::fs::write(&script_path, &script_content)
            .map_err(|e| CapabilityError::Internal(format!("write Scripts/script.md: {e}")))?;
        tx.files_created.push(script_path.clone());

        // Write Beats/beat-sheet.md
        let beat_path = beats_dir.join("beat-sheet.md");
        let beat_content = render_template(&SCRIPT_TEMPLATES[1]);
        std::fs::write(&beat_path, &beat_content)
            .map_err(|e| CapabilityError::Internal(format!("write Beats/beat-sheet.md: {e}")))?;
        tx.files_created.push(beat_path.clone());

        // Write Characters/characters.md
        let characters_path = characters_dir.join("characters.md");
        let characters_content = render_template(&SCRIPT_TEMPLATES[2]);
        std::fs::write(&characters_path, &characters_content).map_err(|e| {
            CapabilityError::Internal(format!("write Characters/characters.md: {e}"))
        })?;
        tx.files_created.push(characters_path.clone());

        // PATCH works row: set work_profile and work_ref
        if let Some(ref pool) = self.pool {
            sqlx::query("UPDATE works SET work_profile = 'script', work_ref = ? WHERE work_id = ?")
                .bind(&inp.work_ref)
                .bind(&inp.work_id)
                .execute(pool)
                .await
                .map_err(|e| CapabilityError::Internal(format!("patch works row: {e}")))?;
        }

        // All FS + DB writes succeeded — commit the transaction guard
        tx.commit();

        let files_created: Vec<String> = tx
            .files_created
            .iter()
            .map(|p| p.strip_prefix(&work_dir).unwrap_or(p).display().to_string())
            .collect();
        let dirs_created: Vec<String> = tx
            .dirs_created
            .iter()
            .map(|d| d.strip_prefix(&work_dir).unwrap_or(d).display().to_string())
            .collect();

        let output = ScaffoldOutput {
            scaffold_root: work_dir.display().to_string(),
            files_created,
            dirs_created,
        };

        info!(
            work_id = %inp.work_id,
            files = ?output.files_created,
            "script.project_scaffold: done"
        );

        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("script.project_scaffold output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_script_templates_render_non_empty() {
        for tmpl in SCRIPT_TEMPLATES {
            let content = render_template(tmpl);
            assert!(!content.is_empty(), "template {} is empty", tmpl.filename);
            assert!(
                content.contains("section_status: draft"),
                "template {} missing section_status frontmatter",
                tmpl.filename
            );
            assert!(
                content.contains("section_weight:"),
                "template {} missing section_weight",
                tmpl.filename
            );
        }
    }

    #[test]
    fn script_templates_count_is_three() {
        assert_eq!(SCRIPT_TEMPLATES.len(), 3);
    }

    #[test]
    fn critical_sections_are_script_and_beat_sheet() {
        let critical: Vec<&str> = SCRIPT_TEMPLATES
            .iter()
            .filter(|t| t.section_weight == "critical")
            .map(|t| t.filename)
            .collect();
        assert!(critical.contains(&"script.md"));
        assert!(critical.contains(&"beat-sheet.md"));
    }

    #[test]
    fn script_capability_name() {
        let cap = ScriptProjectScaffold::new();
        assert_eq!(cap.name(), "script.project_scaffold");
    }

    #[test]
    fn scaffold_transaction_rollback_cleans_up_files() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");

        // Create a scratch file and directory inside the workdir
        let work_dir = root.join("rollback-test");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let file_path = work_dir.join("test.txt");
        std::fs::write(&file_path, "data").expect("write");

        let sub_dir = work_dir.join("subdir");
        std::fs::create_dir_all(&sub_dir).expect("mkdir subdir");

        let mut tx = ScaffoldTransaction::new();
        tx.files_created.push(file_path.clone());
        tx.dirs_created.push(sub_dir.clone());
        tx.dirs_created.push(work_dir.clone());
        // NOT committed → Drop should clean up

        drop(tx);

        assert!(!file_path.exists(), "file should be removed by rollback");
        assert!(!sub_dir.exists(), "subdir should be removed by rollback");
        // work_dir may still exist if rollback-test.txt was in it
    }

    #[test]
    fn scaffold_transaction_commit_no_rollback() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("commit-test");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let file_path = work_dir.join("keep.txt");
        std::fs::write(&file_path, "keep").expect("write");

        let mut tx = ScaffoldTransaction::new();
        tx.files_created.push(file_path.clone());
        tx.dirs_created.push(work_dir.clone());
        tx.commit(); // committed → Drop is no-op

        drop(tx);

        assert!(file_path.exists(), "file should remain after commit");
    }

    #[tokio::test]
    async fn scaffold_creates_directory_tree() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = ScriptProjectScaffold {
            pool: None,
            works_root: root.clone(),
        };

        let input = serde_json::json!({
            "creator_id": "creator_test",
            "work_id": "wrk_script_test",
            "work_ref": "test-script",
            "title": "Test Script",
        });

        let out = cap.run(input).await.expect("scaffold should succeed");
        let scaffold = out["scaffold_root"].as_str().expect("scaffold_root");
        let scaffold_path = std::path::Path::new(scaffold);

        assert!(scaffold_path.join("Scripts").is_dir());
        assert!(scaffold_path.join("Beats").is_dir());
        assert!(scaffold_path.join("Characters").is_dir());
        assert!(scaffold_path.join("Logs").is_dir());
        assert!(scaffold_path.join("Logs/write").is_dir());
        assert!(scaffold_path.join("Logs/review").is_dir());

        assert!(scaffold_path.join("README.md").is_file());
        assert!(scaffold_path.join("Scripts/script.md").is_file());
        assert!(scaffold_path.join("Beats/beat-sheet.md").is_file());
        assert!(scaffold_path.join("Characters/characters.md").is_file());
    }

    #[tokio::test]
    async fn scaffold_rejects_invalid_input() {
        let cap = ScriptProjectScaffold::new();
        let result = cap.run(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
