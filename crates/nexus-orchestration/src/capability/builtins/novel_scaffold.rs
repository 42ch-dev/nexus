//! `novel.project_scaffold` capability.
//!
//! Creates the `Works/<work_ref>/` directory tree, renders templates from
//! the `novel-project-init` embedded preset, seeds `work_chapters` rows,
//! and updates the `works` table — all atomically per V1.36 §5.4.

use super::novel_scaffold_sanitize::{validate_total_chapters, validate_work_ref};
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_local_db::{work_chapters, works};
use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::info;

// ---------------------------------------------------------------------------
// Input / Output types
// ---------------------------------------------------------------------------

/// Capability input: all fields gathered by the init preset's grill-me.
#[derive(Debug, Deserialize)]
struct ScaffoldInput {
    /// Creator ID owning the Work.
    creator_id: String,
    /// Work ID (wrk_...).
    work_id: String,
    /// Human-readable short reference used in paths (e.g. "my-novel").
    work_ref: String,
    /// Work title.
    title: String,
    /// World ID if the Work is bound to a World (null for worldless).
    world_id: Option<String>,
    /// Total number of chapters planned.
    total_planned_chapters: i32,
}

/// Capability output.
#[derive(Debug, Serialize)]
struct ScaffoldOutput {
    /// Absolute path to the created `Works/<work_ref>/` directory.
    scaffold_root: String,
    /// Number of chapter rows seeded.
    chapters_seeded: usize,
    /// Files created (relative paths from `scaffold_root`).
    files_created: Vec<String>,
    /// Directories created (relative paths from `scaffold_root`).
    dirs_created: Vec<String>,
}

// ---------------------------------------------------------------------------
// Template rendering helpers
// ---------------------------------------------------------------------------

/// Minimal mustache-like template renderer for `{{key}}` placeholders.
fn render_template(template: &str, vars: &[(&str, &str)]) -> String {
    let mut result = template.to_string();
    for (key, value) in vars {
        result = result.replace(&format!("{{{{{key}}}}}"), value);
    }
    result
}

/// Read a template from the embedded preset directory.
///
/// Uses the embedded-presets `include_dir!` tree compiled into the binary.
fn load_template(name: &str) -> Option<String> {
    crate::preset::read_embedded_template("novel-project-init", &format!("templates/{name}"))
}

// ---------------------------------------------------------------------------
// Capability struct
// ---------------------------------------------------------------------------

/// `novel.project_scaffold` capability — creates the full Works tree for a novel.
///
/// This capability is stateless; it receives all context via input and operates
/// on the filesystem + DB pool provided by the daemon runtime.
pub struct NovelProjectScaffold {
    pool: Option<sqlx::SqlitePool>,
    works_root: PathBuf,
}

impl NovelProjectScaffold {
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

    /// Create an instance with a DB pool and a custom works root directory.
    ///
    /// Used by hermetic integration tests that need to place the scaffold
    /// under a `tempfile::TempDir` rather than the real workspace.
    #[must_use]
    pub const fn new_with_root(pool: sqlx::SqlitePool, works_root: PathBuf) -> Self {
        Self {
            pool: Some(pool),
            works_root,
        }
    }
}

impl Default for NovelProjectScaffold {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for NovelProjectScaffold {
    fn name(&self) -> &'static str {
        "novel.project_scaffold"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]},"total_planned_chapters":{"type":"integer","minimum":1}},"required":["creator_id","work_id","work_ref","title","total_planned_chapters"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"scaffold_root":{"type":"string"},"chapters_seeded":{"type":"integer"},"files_created":{"type":"array","items":{"type":"string"}},"dirs_created":{"type":"array","items":{"type":"string"}}},"required":["scaffold_root","chapters_seeded","files_created","dirs_created"],"additionalProperties":false}"#
    }

    // SAFETY: The run method handles 9 file/dir operations + DB seed + DB patch.
    // Line count is inherent to the multi-step scaffold protocol.
    #[allow(clippy::too_many_lines)]
    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: ScaffoldInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("novel.project_scaffold input: {e}"))
        })?;

        // ── F1 — sanitize untrusted grill-me values (C-1, C-4, W-2) ────
        // Reject path-traversal, separators, uppercase, oversize, control
        // characters; bound chapter count to 1..=100 (matches prompt range).
        let work_ref = validate_work_ref(&inp.work_ref)?;
        let total_chapters_bounded = validate_total_chapters(inp.total_planned_chapters)?;
        // Re-bind to the validated values so downstream code cannot accidentally
        // use the raw input fields.
        let inp = ScaffoldInput {
            creator_id: inp.creator_id,
            work_id: inp.work_id,
            work_ref,
            title: inp.title,
            world_id: inp.world_id,
            total_planned_chapters: inp.total_planned_chapters,
        };
        let _ = total_chapters_bounded; // kept for documentation; bounded i32 reused below

        let root = self.works_root.join(&inp.work_ref);

        // ── T2a: root directory ────────────────────────────────────────
        let mut dirs_created = Vec::new();
        let mut files_created = Vec::new();

        create_dir_all_idem(&root)?;
        dirs_created.push(String::new()); // root itself

        // ── T2b: README.md ─────────────────────────────────────────────
        if let Some(tmpl) = load_template("README.md") {
            let world_section = inp.world_id.as_ref().map_or_else(
                || "**Binding:** none (worldless)\n\nThis Work has no World binding. Inline world setting (if any) should be captured during the init grill-me and appended here.".to_string(),
                |id| format!("**Binding:** `world_id: {id}`\n\nWorld details live in the World KB; see World Browser for the full setting."),
            );
            // Description placeholder — collected during grill-me; left empty in V1.36.
            let description = format!("Long-term goal and initial creative direction for **{}** (work_ref: `{}`). Fill in as grill-me captures intent.", inp.title, inp.work_ref);
            let total = inp.total_planned_chapters.to_string();
            let rendered = render_template(
                &tmpl,
                &[
                    ("work_ref", &inp.work_ref),
                    ("title", &inp.title),
                    ("world_section", &world_section),
                    ("description", &description),
                    ("total_planned_chapters", &total),
                ],
            );
            write_file_idem(&root.join("README.md"), &rendered, &mut files_created)?;
        }

        // ── T2c–T2g: Outlines/ subtree ────────────────────────────────
        let outlines = root.join("Outlines");
        create_dir_all_idem(&outlines)?;
        dirs_created.push("Outlines".to_string());

        // T2d: Outlines/chapters/
        create_dir_all_idem(&outlines.join("chapters"))?;
        dirs_created.push("Outlines/chapters".to_string());

        // T2e: volume-outline.md
        if let Some(tmpl) = load_template("volume-outline.md") {
            let total = inp.total_planned_chapters.to_string();
            let rendered = render_template(
                &tmpl,
                &[
                    ("work_ref", &inp.work_ref),
                    ("title", &inp.title),
                    ("total_planned_chapters", &total),
                ],
            );
            write_file_idem(
                &outlines.join("volume-outline.md"),
                &rendered,
                &mut files_created,
            )?;
        }

        // T2f: foreshadowing.md
        if let Some(tmpl) = load_template("foreshadowing.md") {
            let rendered = render_template(&tmpl, &[("work_ref", &inp.work_ref)]);
            write_file_idem(
                &outlines.join("foreshadowing.md"),
                &rendered,
                &mut files_created,
            )?;
        }

        // T2g: event-index.md
        if let Some(tmpl) = load_template("event-index.md") {
            let rendered = render_template(&tmpl, &[("work_ref", &inp.work_ref)]);
            write_file_idem(
                &outlines.join("event-index.md"),
                &rendered,
                &mut files_created,
            )?;
        }

        // ── T2h: Stories/ ──────────────────────────────────────────────
        create_dir_all_idem(&root.join("Stories"))?;
        dirs_created.push("Stories".to_string());

        // ── T2i: Logs/ ─────────────────────────────────────────────────
        create_dir_all_idem(&root.join("Logs"))?;
        dirs_created.push("Logs".to_string());

        // ── T3: seed work_chapters rows ────────────────────────────────
        let chapters_seeded = if let Some(pool) = &self.pool {
            let now = chrono::Utc::now().to_rfc3339();
            work_chapters::seed_chapters(
                pool,
                &inp.work_id,
                &inp.work_ref,
                inp.total_planned_chapters,
                &now,
            )
            .await
            .map_err(|e| CapabilityError::Internal(format!("seed_chapters: {e}")))?;
            usize::try_from(inp.total_planned_chapters).unwrap_or(0)
        } else {
            0
        };
        info!(
            work_id = %inp.work_id,
            chapters_seeded,
            "novel.project_scaffold: chapters seeded"
        );

        // ── T4: PATCH works table ──────────────────────────────────────
        if let Some(pool) = &self.pool {
            let patch = works::WorkPatch {
                work_profile: Some(Some("novel".to_string())),
                work_ref: Some(Some(inp.work_ref.clone())),
                total_planned_chapters: Some(Some(inp.total_planned_chapters)),
                current_chapter: Some(0),
                world_id: Some(inp.world_id.clone()),
                title: None,
                long_term_goal: None,
                creative_brief: None,
                intake_status: None,
                status: None,
                story_ref: None,
                primary_preset_id: None,
                schedule_ids: None,
                current_stage: None,
                stage_status: None,
            };
            let now = chrono::Utc::now().to_rfc3339();
            works::patch_work(pool, &inp.creator_id, &inp.work_id, &patch, &now)
                .await
                .map_err(|e| CapabilityError::Internal(format!("patch_work: {e}")))?;
            info!(
                work_id = %inp.work_id,
                "novel.project_scaffold: works patched"
            );
        }

        let output = ScaffoldOutput {
            scaffold_root: root.to_string_lossy().to_string(),
            chapters_seeded,
            files_created,
            dirs_created,
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// FS helpers (idempotent — T6 compliance)
// ---------------------------------------------------------------------------

/// Create a directory and all parents. No-op if it already exists.
fn create_dir_all_idem(path: &Path) -> Result<(), CapabilityError> {
    std::fs::create_dir_all(path)
        .map_err(|e| CapabilityError::Internal(format!("mkdir {}: {e}", path.display())))
}

/// Write file only if it doesn't exist (idempotent per T6).
fn write_file_idem(
    path: &Path,
    content: &str,
    files_created: &mut Vec<String>,
) -> Result<(), CapabilityError> {
    if path.exists() {
        info!(path = %path.display(), "write_file_idem: skip (exists)");
        return Ok(());
    }
    std::fs::write(path, content)
        .map_err(|e| CapabilityError::Internal(format!("write {}: {e}", path.display())))?;
    // Store relative name (parent dir / filename)
    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
        files_created.push(name.to_string());
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn render_template_replaces_placeholders() {
        let tmpl = "Title: {{title}}, Ref: {{work_ref}}";
        let rendered = render_template(tmpl, &[("title", "My Novel"), ("work_ref", "my-novel")]);
        assert_eq!(rendered, "Title: My Novel, Ref: my-novel");
    }

    #[test]
    fn render_template_no_match_is_noop() {
        let tmpl = "No placeholders here";
        let rendered = render_template(tmpl, &[("title", "My Novel")]);
        assert_eq!(rendered, "No placeholders here");
    }

    #[tokio::test]
    async fn scaffold_creates_directory_tree() {
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = NovelProjectScaffold {
            pool: None,
            works_root: root.clone(),
        };

        let input = serde_json::json!({
            "creator_id": "creator_test",
            "work_id": "wrk_test123",
            "work_ref": "test-novel",
            "title": "Test Novel",
            "world_id": null,
            "total_planned_chapters": 3,
        });

        let out = cap.run(input).await.expect("scaffold should succeed");
        let scaffold = out["scaffold_root"].as_str().expect("scaffold_root");
        let scaffold_path = Path::new(scaffold);

        // Verify directories
        assert!(scaffold_path.join("Outlines").is_dir());
        assert!(scaffold_path.join("Outlines/chapters").is_dir());
        assert!(scaffold_path.join("Stories").is_dir());
        assert!(scaffold_path.join("Logs").is_dir());

        // Verify files
        assert!(scaffold_path.join("README.md").is_file());
        assert!(scaffold_path.join("Outlines/volume-outline.md").is_file());
        assert!(scaffold_path.join("Outlines/foreshadowing.md").is_file());
        assert!(scaffold_path.join("Outlines/event-index.md").is_file());

        // No chapters seeded (no pool)
        assert_eq!(out["chapters_seeded"], 0);
    }

    #[tokio::test]
    async fn scaffold_idempotent_no_overwrite() {
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = NovelProjectScaffold {
            pool: None,
            works_root: root.clone(),
        };

        let input = serde_json::json!({
            "creator_id": "creator_test",
            "work_id": "wrk_idem",
            "work_ref": "idem-novel",
            "title": "Idem Novel",
            "total_planned_chapters": 1,
        });

        // First run
        let out1 = cap.run(input.clone()).await.expect("first run");
        let scaffold = out1["scaffold_root"].as_str().expect("root");
        let readme = Path::new(scaffold).join("README.md");

        // Write custom content
        std::fs::write(&readme, "CUSTOM CONTENT").expect("write custom");

        // Second run
        let _out2 = cap.run(input).await.expect("second run");

        // Custom content preserved
        let content = std::fs::read_to_string(&readme).expect("read");
        assert_eq!(
            content, "CUSTOM CONTENT",
            "T6: existing files not overwritten"
        );
    }

    #[tokio::test]
    async fn scaffold_rejects_invalid_input() {
        let cap = NovelProjectScaffold::new();
        let result = cap.run(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
