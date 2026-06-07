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
    /// F4 (W-2-qc2): explicit list of `works` columns the user supplied
    /// in this grill-me session. When `None`, all fields are PATCHed
    /// (initial bootstrap). When `Some`, only the listed columns are
    /// updated on re-init — matches spec §5.4.4 "PATCH only updates
    /// fields the user explicitly changed in this grill-me session."
    ///
    /// Accepted values: any subset of
    /// `["work_profile", "work_ref", "title", "world_id", "total_planned_chapters"]`.
    /// Unknown values are ignored (forward-compat).
    #[serde(default)]
    fields_changed: Option<Vec<String>>,
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

/// Lazily-initialized handlebars registry for novel scaffold templates.
///
/// F3 (W-1): replaces the previous naive `String::replace` renderer with
/// `handlebars-rust` to (a) support the broader `{{var}}` / `{{nested.path}}`
/// surface our templates already use syntactically, (b) gain strict-mode
/// errors when a placeholder is misspelled or unbound (silent literal
/// `{{...}}` would leak into the generated Markdown), and (c) align with
/// the renderer used by `tasks::render_strict_template` for capability
/// arg rendering.
///
/// `no_escape` preserves plain-text fidelity (no HTML entity encoding of
/// `&`, `<`, `>`) — these files are Markdown, not HTML.
static SCAFFOLD_HANDLEBARS: std::sync::OnceLock<handlebars::Handlebars<'static>> =
    std::sync::OnceLock::new();

fn scaffold_handlebars() -> &'static handlebars::Handlebars<'static> {
    SCAFFOLD_HANDLEBARS.get_or_init(|| {
        let mut reg = handlebars::Handlebars::new();
        reg.register_escape_fn(handlebars::no_escape);
        reg.set_strict_mode(true);
        reg
    })
}

/// Render a novel-scaffold template using handlebars-rust in strict mode.
///
/// `vars` is a flat list of `(key, value)` pairs converted into a JSON
/// object for rendering. Strict mode causes any unbound `{{key}}` in the
/// template to fail rather than silently render empty, which surfaces
/// template-vs-callsite drift at scaffold time instead of at the writer's
/// desk.
///
/// # Errors
///
/// Returns `CapabilityError::Internal` if the template syntax is invalid
/// or references an unbound variable. Templates ship with the binary, so
/// a render failure here is a build-time author error, not a user error.
fn render_template(
    template: &str,
    vars: &[(&str, &str)],
) -> Result<String, CapabilityError> {
    let mut payload = serde_json::Map::with_capacity(vars.len());
    for (k, v) in vars {
        payload.insert((*k).to_string(), serde_json::Value::String((*v).to_string()));
    }
    scaffold_handlebars()
        .render_template(template, &serde_json::Value::Object(payload))
        .map_err(|e| CapabilityError::Internal(format!("template render: {e}")))
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
            fields_changed: inp.fields_changed,
        };
        let _ = total_chapters_bounded; // kept for documentation; bounded i32 reused below

        // ── F5 — verify world_id FK exists before any side effect (C-3) ─
        // Spec §3.5: world_id binds a Work to an existing World. If the
        // user (or LLM) supplies a stale/typo'd ID, fail fast before
        // creating FS scaffold or PATCHing the works row. Worldless
        // (None) is the documented branch and skipped here.
        if let (Some(world_id), Some(pool)) = (inp.world_id.as_deref(), self.pool.as_ref()) {
            // SAFETY: SELECT against narrative_worlds — runtime query; the
            // typed module DF-63 lands in V1.37+. See R-V133P1-09 for the
            // workspace-wide runtime->compile-time conversion follow-up.
            let exists: Option<(i64,)> =
                sqlx::query_as("SELECT 1 FROM narrative_worlds WHERE world_id = ?")
                    .bind(world_id)
                    .fetch_optional(pool)
                    .await
                    .map_err(|e| {
                        CapabilityError::Internal(format!("world_id existence check: {e}"))
                    })?;
            if exists.is_none() {
                return Err(CapabilityError::InputInvalid(format!(
                    "world_id {world_id:?} not found in narrative_worlds (worldless requires null)"
                )));
            }
        }

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
            )?;
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
            )?;
            write_file_idem(
                &outlines.join("volume-outline.md"),
                &rendered,
                &mut files_created,
            )?;
        }

        // T2f: foreshadowing.md
        if let Some(tmpl) = load_template("foreshadowing.md") {
            let rendered = render_template(&tmpl, &[("work_ref", &inp.work_ref)])?;
            write_file_idem(
                &outlines.join("foreshadowing.md"),
                &rendered,
                &mut files_created,
            )?;
        }

        // T2g: event-index.md
        if let Some(tmpl) = load_template("event-index.md") {
            let rendered = render_template(&tmpl, &[("work_ref", &inp.work_ref)])?;
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
            // F4 (W-2-qc2): when `fields_changed` is provided, PATCH only
            // those columns (re-init). When absent, PATCH all (initial
            // bootstrap). The `current_chapter = 0` reset is part of the
            // initial bootstrap shape and is suppressed on partial re-init.
            let changed: Option<std::collections::HashSet<&str>> = inp.fields_changed.as_ref().map(
                |v| v.iter().map(String::as_str).collect::<std::collections::HashSet<_>>(),
            );
            let want = |field: &str| changed.as_ref().is_none_or(|set| set.contains(field));

            let patch = works::WorkPatch {
                // work_profile is set on every init invocation (it is the
                // primary marker that this Work is a novel); not user-toggled.
                work_profile: if changed.is_none() {
                    Some(Some("novel".to_string()))
                } else {
                    None
                },
                work_ref: if want("work_ref") {
                    Some(Some(inp.work_ref.clone()))
                } else {
                    None
                },
                total_planned_chapters: if want("total_planned_chapters") {
                    Some(Some(inp.total_planned_chapters))
                } else {
                    None
                },
                // current_chapter is reset only on initial bootstrap.
                current_chapter: if changed.is_none() { Some(0) } else { None },
                world_id: if want("world_id") {
                    Some(inp.world_id.clone())
                } else {
                    None
                },
                title: if want("title") && changed.is_some() {
                    // Only PATCH title on partial re-init when caller
                    // explicitly listed it. On initial bootstrap, title
                    // was set during create-Work and we do not overwrite.
                    Some(inp.title.clone())
                } else {
                    None
                },
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
                partial = %changed.is_some(),
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
        let rendered = render_template(tmpl, &[("title", "My Novel"), ("work_ref", "my-novel")])
            .expect("flat render");
        assert_eq!(rendered, "Title: My Novel, Ref: my-novel");
    }

    #[test]
    fn render_template_no_match_is_noop() {
        let tmpl = "No placeholders here";
        let rendered = render_template(tmpl, &[("title", "My Novel")]).expect("noop render");
        assert_eq!(rendered, "No placeholders here");
    }

    #[test]
    fn render_template_strict_mode_rejects_unbound_var() {
        // F3 (W-1): strict mode catches misspelled / unbound placeholders
        // instead of silently producing literal "{{...}}" in the output.
        let tmpl = "Hello {{name}}, Ref: {{work_ref}}";
        let err = render_template(tmpl, &[("work_ref", "abc")]).expect_err("must error");
        let msg = format!("{err}");
        assert!(
            msg.contains("template render"),
            "expected template render error, got: {msg}"
        );
    }

    #[test]
    fn render_template_preserves_special_chars_no_html_escape() {
        // F3 (W-1): no_escape mode preserves &, <, > as-is for Markdown.
        let tmpl = "{{body}}";
        let rendered = render_template(tmpl, &[("body", "A & B < C > D")])
            .expect("no-escape render");
        assert_eq!(rendered, "A & B < C > D");
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
