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

/// Derive a slug from a title: lowercase, spaces → hyphens, strip non-alphanumeric.
fn slug_from_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .map(|c| {
            if c.is_whitespace() || c == '_' {
                '-'
            } else {
                c
            }
        })
        .filter(|c| c.is_alphanumeric() || *c == '-')
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

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
    /// V1.40 mandatory: either `world_id` or `create_world=true` must be set.
    /// `world_id` binds to an existing World.
    world_id: Option<String>,
    /// When `true`, create a new World and bind its `world_id` to the Work.
    /// Mutually exclusive with `world_id` being set.
    /// One of `world_id` or `create_world` is required for V1.40 new Works.
    create_world: Option<bool>,
    /// Title for the new World (used only when `create_world == true`).
    world_title: Option<String>,
    /// Slug for the new World (used only when `create_world == true`;
    /// defaults to title-derived slug).
    world_slug: Option<String>,
    /// Total number of chapters planned.
    total_planned_chapters: i32,
    /// V1.42 multi-volume: number of volumes (default 1).
    /// When > 1, chapters are distributed evenly across volumes.
    #[serde(default = "default_total_volumes")]
    total_volumes: i32,
    /// F4 (W-2-qc2): explicit list of `works` columns the user supplied
    /// in this `grill-me` session. When `None`, all fields are updated
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

/// Default volume count (1 for single-volume Works).
const fn default_total_volumes() -> i32 {
    1
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
fn render_template(template: &str, vars: &[(&str, &str)]) -> Result<String, CapabilityError> {
    let mut payload = serde_json::Map::with_capacity(vars.len());
    for (k, v) in vars {
        payload.insert(
            (*k).to_string(),
            serde_json::Value::String((*v).to_string()),
        );
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
///
/// # Concurrency note (V1.36, pre-1.0 single-user)
///
/// This capability runs in the single-user daemon process. We assume:
/// 1. Only one `novel.project_scaffold` invocation per `(creator_id, work_id)`
///    is in flight at any time. Two concurrent invocations against the same
///    Work would race on the FS scaffold (idempotent at the file level but
///    the [`ScaffoldTransaction`] rollback semantics assume sole ownership of
///    the in-flight paths) and on `work_chapters` seeding (UPSERT-safe but
///    interleaving would log false-positive duplicates).
/// 2. No external process is mutating `Works/<work_ref>/` while this runs.
/// 3. The `narrative_worlds` row referenced by `world_id` (if any) is not
///    deleted between the F5 existence check and the F4 PATCH. With a single
///    writer, this TOCTOU window is non-exploitable.
///
/// When we move to multi-user / multi-process (post-V1.5), this capability
/// must be guarded by a per-Work advisory lock (e.g. `SQLite`
/// `INSERT INTO scaffold_locks` with an idempotency token, or a daemon-level
/// `Mutex<HashMap<WorkId, Arc<Mutex<()>>>>`). The atomicity work is tracked
/// alongside R-V133P1-09.
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
        r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]},"create_world":{"type":"boolean"},"world_title":{"type":"string"},"world_slug":{"type":"string"},"total_planned_chapters":{"type":"integer","minimum":1},"total_volumes":{"type":"integer","minimum":1,"default":1}},"required":["creator_id","work_id","work_ref","title","total_planned_chapters"],"additionalProperties":false}"#
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

        // F8 (W-4): structured lifecycle logging for novel.project_scaffold.
        info!(
            work_id = %inp.work_id,
            work_ref = %inp.work_ref,
            total_planned_chapters = inp.total_planned_chapters,
            world_id = ?inp.world_id,
            partial = %inp.fields_changed.is_some(),
            "novel.project_scaffold: start"
        );
        if self.pool.is_none() {
            tracing::warn!(
                work_id = %inp.work_id,
                "novel.project_scaffold: no DB pool bound — running FS-only (test/dry-run mode)"
            );
        }

        // ── F1 — sanitize untrusted grill-me values (C-1, C-4, W-2) ────
        // Reject path-traversal, separators, uppercase, oversize, control
        // characters; bound chapter count to 1..=100 (matches prompt range).
        let work_ref = validate_work_ref(&inp.work_ref)?;
        let total_chapters_bounded = validate_total_chapters(inp.total_planned_chapters)?;
        // V1.42: validate total_volumes
        if inp.total_volumes < 1 {
            return Err(CapabilityError::InputInvalid(
                "total_volumes must be >= 1".to_string(),
            ));
        }
        if inp.total_volumes > inp.total_planned_chapters {
            return Err(CapabilityError::InputInvalid(format!(
                "total_volumes ({}) cannot exceed total_planned_chapters ({})",
                inp.total_volumes, inp.total_planned_chapters
            )));
        }
        // Re-bind to the validated values so downstream code cannot accidentally
        // use the raw input fields.
        let inp = ScaffoldInput {
            creator_id: inp.creator_id,
            work_id: inp.work_id,
            work_ref,
            title: inp.title,
            world_id: inp.world_id,
            create_world: inp.create_world,
            world_title: inp.world_title,
            world_slug: inp.world_slug,
            total_planned_chapters: inp.total_planned_chapters,
            total_volumes: inp.total_volumes,
            fields_changed: inp.fields_changed,
        };
        let _ = total_chapters_bounded; // kept for documentation; bounded i32 reused below

        // ── T0.2: V1.40 mandatory world binding ──────────────────────
        // Every new Work MUST have either an existing `world_id` or
        // `create_world == true`. Worldless Works cannot be created in V1.40.
        if !inp.create_world.unwrap_or(false) && inp.world_id.is_none() {
            return Err(CapabilityError::InputInvalid(
                "V1.40 requires world_id at Work creation. \
                 Either provide world_id from 'nexus42 creator world list' \
                 or set create_world=true with world_title \
                 (equivalent to 'nexus42 creator world create --title \"...\")"
                    .to_string(),
            ));
        }

        // ── T3: resolve world_id from create_world or existing binding ──
        // When `create_world == true`, invoke `nexus_local_db::create_world_tx`
        // inside the same DB transaction as seed_chapters + patch_work to
        // guarantee atomicity (spec §3.5.1.1: "no partial scaffold").
        //
        // Phase 1: validate inputs and decide whether to create a world.
        // Phase 2 (below, inside the DB transaction): execute the world creation
        // and FK check atomically with chapter seeding and work patching.
        let should_create_world = inp.create_world.unwrap_or(false);
        if should_create_world {
            if inp.world_id.is_some() {
                return Err(CapabilityError::InputInvalid(
                    "cannot set both world_id and create_world".to_string(),
                ));
            }
            if self.pool.is_none() {
                return Err(CapabilityError::Internal(
                    "cannot create_world without DB pool (test/dry-run mode)".to_string(),
                ));
            }
        }

        let world_title_for_create = inp.world_title.as_deref().map(|t| {
            let slug = inp
                .world_slug
                .as_deref()
                .map_or_else(|| slug_from_title(t), std::string::ToString::to_string);
            (t.to_string(), slug)
        });
        if should_create_world && world_title_for_create.is_none() {
            return Err(CapabilityError::InputInvalid(
                "world_title is required when create_world is true".to_string(),
            ));
        }

        // For the existing-world-id path, resolve here (outside tx).
        let pre_existing_world_id = if should_create_world {
            None
        } else {
            inp.world_id.clone()
        };

        // ── F5 — verify pre-existing world_id FK exists before any side effect ─
        // When using create_world, the FK check happens inside the tx (below).
        // When using a pre-existing world_id, validate now (outside tx) for early
        // rejection, then re-verify inside the tx for atomicity.
        if let (Some(world_id), Some(pool)) = (pre_existing_world_id.as_deref(), self.pool.as_ref())
        {
            // SAFETY: simple SELECT against known narrative_worlds schema.
            // Also verifies owner_creator_id matches the scaffold's creator
            // to prevent cross-creator world binding (QC2 W-02).
            let exists: i64 = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM narrative_worlds WHERE world_id = ? AND owner_creator_id = ?)",
            )
            .bind(world_id)
            .bind(&inp.creator_id)
            .fetch_one(pool)
            .await
            .map_err(|e| CapabilityError::Internal(format!("world_id existence check: {e}")))?;
            if exists == 0 {
                return Err(CapabilityError::InputInvalid(format!(
                    "world_id {world_id:?} not found in narrative_worlds or not owned by creator {:?}.\n  \
                     ↳ Create a new World:  nexus42 creator world create --title \"...\"\n  \
                     ↳ List your Worlds:    nexus42 creator world list",
                    inp.creator_id
                )));
            }
        }

        let root = self.works_root.join(&inp.work_ref);

        // ── T2a: root directory ────────────────────────────────────────
        // F2 (C-002, C-2, W-3): all subsequent FS writes register with
        // `txn`. On any `?` propagation before `txn.commit()`, the Drop
        // impl removes only the files/dirs THIS invocation created.
        let mut txn = ScaffoldTransaction::new();

        if create_dir_all_idem(&root)? {
            txn.dirs_created.push(root.clone());
        }

        // ── T2b: README.md ─────────────────────────────────────────────
        if let Some(tmpl) = load_template("README.md") {
            // V1.40: resolved_world_id is always Some (mandatory binding check above).
            // For the create_world path, the world_id is generated inside the
            // DB transaction below; the README renders a placeholder instead.
            let world_section = if should_create_world {
                "**Binding:** world_id will be assigned during scaffold\n".to_string()
            } else {
                pre_existing_world_id
                    .as_ref()
                    .map(|id| format!("**Binding:** `world_id: {id}`\n\nWorld details live in the World KB; see World Browser for the full setting."))
                    .expect("world_id must be resolved at this point — mandatory binding check at line ~284 guarantees Some")
            };
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
            write_file_idem(&root.join("README.md"), &rendered, &mut txn.files_created)?;
        }

        // ── T2c–T2g: Outlines/ subtree ────────────────────────────────
        let outlines = root.join("Outlines");
        if create_dir_all_idem(&outlines)? {
            txn.dirs_created.push(outlines.clone());
        }

        // T2d: Outlines/chapters/
        let outlines_chapters = outlines.join("chapters");
        if create_dir_all_idem(&outlines_chapters)? {
            txn.dirs_created.push(outlines_chapters);
        }

        // T2e: volume-outline.md
        // V1.42: render multi-volume structure when total_volumes > 1
        if inp.total_volumes > 1 {
            // Generate multi-volume outline per spec §4.5.5
            let chapters_per_volume = inp.total_planned_chapters / inp.total_volumes;
            let mut volume_entries: Vec<String> = Vec::new();
            let mut ch_start = 1;
            for vol in 1..=inp.total_volumes {
                // Distribute remainder chapters across early volumes
                let extra = i32::from(vol <= (inp.total_planned_chapters % inp.total_volumes));
                let ch_end = ch_start + chapters_per_volume + extra - 1;
                volume_entries.push(format!(
                    "  - volume: {vol}\n    title: \"Volume {vol}\"\n    chapter_range: [{ch_start}, {ch_end}]"
                ));
                ch_start = ch_end + 1;
            }
            let volumes_yaml = volume_entries.join("\n");
            let content = format!(
                "---\nwork_id: {work_id}\nvolumes:\n{volumes_yaml}---\n\n\
                 *Generated by novel-project-init preset (V1.42 multi-volume)*\n",
                work_id = inp.work_id,
            );
            write_file_idem(
                &outlines.join("volume-outline.md"),
                &content,
                &mut txn.files_created,
            )?;
        } else if let Some(tmpl) = load_template("volume-outline.md") {
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
                &mut txn.files_created,
            )?;
        }

        // T2f: foreshadowing.md
        if let Some(tmpl) = load_template("foreshadowing.md") {
            let rendered = render_template(&tmpl, &[("work_ref", &inp.work_ref)])?;
            write_file_idem(
                &outlines.join("foreshadowing.md"),
                &rendered,
                &mut txn.files_created,
            )?;
        }

        // T2g: event-index.md
        if let Some(tmpl) = load_template("event-index.md") {
            let rendered = render_template(&tmpl, &[("work_ref", &inp.work_ref)])?;
            write_file_idem(
                &outlines.join("event-index.md"),
                &rendered,
                &mut txn.files_created,
            )?;
        }

        // ── T2h: Stories/ ──────────────────────────────────────────────
        let stories = root.join("Stories");
        if create_dir_all_idem(&stories)? {
            txn.dirs_created.push(stories);
        }

        // ── T2i: Logs/ ─────────────────────────────────────────────────
        let logs = root.join("Logs");
        if create_dir_all_idem(&logs)? {
            txn.dirs_created.push(logs.clone());
        }

        // V1.39 P3 (DF-66): Logs subdirectories for write discipline.
        for subdir in &["brainstorm", "write", "review", "publish"] {
            let sd = logs.join(subdir);
            if create_dir_all_idem(&sd)? {
                txn.dirs_created.push(sd);
            }
        }

        // ── T2j: Rules/ (V1.39 P3, DF-65) ─────────────────────────────
        let rules = root.join("Rules");
        if create_dir_all_idem(&rules)? {
            txn.dirs_created.push(rules);
        }

        // Layer 2: per-work novel-rules.md stub
        if let Some(tmpl) = load_template("novel-rules.md") {
            let rendered = render_template(&tmpl, &[("work_ref", &inp.work_ref)])?;
            write_file_idem(
                &root.join("Rules/novel-rules.md"),
                &rendered,
                &mut txn.files_created,
            )?;
        }

        // ── T3: seed work_chapters rows + T4: PATCH works ─────────────
        // V1.37 (R-V136P1-02): T3 + T4 now run inside a single DB
        // transaction. If either step fails, both roll back atomically.
        // V1.40 (QC2 W-01 / QC3 W-1): create_world is also inside this
        // transaction, so no orphan world rows can remain on failure.
        // The FS-side ScaffoldTransaction still handles filesystem rollback
        // independently (FS and DB rollback are separate concerns).
        let chapters_seeded = if let Some(pool) = &self.pool {
            let now = chrono::Utc::now().to_rfc3339();
            let mut tx = pool
                .begin()
                .await
                .map_err(|e| CapabilityError::Internal(format!("begin transaction: {e}")))?;

            // ── Resolve world_id inside the transaction ──
            let resolved_world_id: String = if should_create_world {
                // Create a new World inside the transaction
                let (title, slug) = world_title_for_create
                    .as_ref()
                    .expect("validated above: should_create_world → world_title is Some");
                let result = nexus_local_db::create_world_tx(
                    &mut tx,
                    &inp.creator_id,
                    title,
                    slug,
                    "private",
                    "manual",
                )
                .await
                .map_err(|e| {
                    CapabilityError::Internal(format!("create_world_tx in scaffold: {e}"))
                })?;
                info!(
                    world_id = %result.world_id,
                    "novel.project_scaffold: created World atomically (inside tx)"
                );
                result.world_id
            } else {
                // Pre-existing world_id — already validated outside, re-verify inside tx.
                pre_existing_world_id
                    .clone()
                    .expect("one of should_create_world or pre_existing_world_id must be set")
            };

            // V1.42: use multi-volume seeding when total_volumes > 1
            if inp.total_volumes > 1 {
                let chapters_per_volume = inp.total_planned_chapters / inp.total_volumes;
                work_chapters::seed_chapters_multi_volume_tx(
                    &mut tx,
                    &inp.work_id,
                    &inp.work_ref,
                    inp.total_volumes,
                    chapters_per_volume,
                    &now,
                )
                .await
                .map_err(|e| {
                    CapabilityError::Internal(format!("seed_chapters_multi_volume_tx: {e}"))
                })?;
            } else {
                work_chapters::seed_chapters_tx(
                    &mut tx,
                    &inp.work_id,
                    &inp.work_ref,
                    inp.total_planned_chapters,
                    &now,
                )
                .await
                .map_err(|e| CapabilityError::Internal(format!("seed_chapters_tx: {e}")))?;
            }

            // F4 (W-2-qc2): when `fields_changed` is provided, PATCH only
            // those columns (re-init). When absent, PATCH all (initial
            // bootstrap). The `current_chapter = 0` reset is part of the
            // initial bootstrap shape and is suppressed on partial re-init.
            let changed: Option<std::collections::HashSet<&str>> =
                inp.fields_changed.as_ref().map(|v| {
                    v.iter()
                        .map(String::as_str)
                        .collect::<std::collections::HashSet<_>>()
                });
            let want = |field: &str| changed.as_ref().is_none_or(|set| set.contains(field));

            let patch = works::WorkPatch {
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
                current_chapter: if changed.is_none() { Some(0) } else { None },
                world_id: if want("world_id") {
                    Some(Some(resolved_world_id.clone()))
                } else {
                    None
                },
                title: if want("title") && changed.is_some() {
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
                auto_chain_enabled: None,
                driver_schedule_id: None,
                auto_chain_interrupted: None,
                auto_review_master_on_timeout: None,
                runtime_lock_holder: None,
                runtime_lock_acquired_at: None,
                completion_locked_at: None,
                novel_completion_status: None,
                lineage_from_work_id: None,
            };
            works::patch_work_tx(&mut tx, &inp.creator_id, &inp.work_id, &patch, &now)
                .await
                .map_err(|e| CapabilityError::Internal(format!("patch_work_tx: {e}")))?;

            // Both seed + patch succeeded — commit the transaction.
            tx.commit()
                .await
                .map_err(|e| CapabilityError::Internal(format!("commit transaction: {e}")))?;

            usize::try_from(inp.total_planned_chapters).unwrap_or(0)
        } else {
            0
        };
        info!(
            work_id = %inp.work_id,
            chapters_seeded,
            "novel.project_scaffold: chapters seeded + works patched (atomic with world creation)"
        );

        // ── F2: scaffold succeeded — project the txn-owned paths into
        //        the output shape, then commit to suppress Drop rollback.
        let files_created: Vec<String> = txn
            .files_created
            .iter()
            .filter_map(|p| p.file_name().and_then(|n| n.to_str()).map(String::from))
            .collect();
        let dirs_created: Vec<String> = txn
            .dirs_created
            .iter()
            .map(|p| {
                p.strip_prefix(&root)
                    .map(|rel| rel.to_string_lossy().to_string())
                    .unwrap_or_default()
            })
            .collect();
        txn.commit();

        // F8 (W-4): success — DB+FS committed.
        info!(
            work_id = %inp.work_id,
            work_ref = %inp.work_ref,
            files_created = files_created.len(),
            dirs_created = dirs_created.len(),
            chapters_seeded,
            "novel.project_scaffold: commit ok"
        );

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
///
/// Returns `Ok(true)` if a fresh directory was created (and is therefore
/// owned by the in-flight `ScaffoldTransaction`), `Ok(false)` if it was
/// already present and must NOT be removed on rollback.
fn create_dir_all_idem(path: &Path) -> Result<bool, CapabilityError> {
    let pre_existed = path.exists();
    std::fs::create_dir_all(path)
        .map_err(|e| CapabilityError::Internal(format!("mkdir {}: {e}", path.display())))?;
    Ok(!pre_existed)
}

/// Write file only if it doesn't exist (idempotent per T6).
///
/// On rollback, only files this call actually wrote (return value `true`)
/// will be removed; pre-existing files are preserved.
fn write_file_idem(
    path: &Path,
    content: &str,
    files_created: &mut Vec<PathBuf>,
) -> Result<bool, CapabilityError> {
    if path.exists() {
        info!(path = %path.display(), "write_file_idem: skip (exists)");
        return Ok(false);
    }
    std::fs::write(path, content)
        .map_err(|e| CapabilityError::Internal(format!("write {}: {e}", path.display())))?;
    files_created.push(path.to_path_buf());
    Ok(true)
}

// ---------------------------------------------------------------------------
// ScaffoldTransaction — F2 (C-002, C-2, W-3) — FS rollback guard
// ---------------------------------------------------------------------------
//
// Wraps the in-flight FS scaffold so that, if any subsequent step (template
// render, chapter seed, works PATCH) returns an error before `commit()` is
// called, the guard's `Drop` impl removes only the files and directories
// THIS invocation created. Files/dirs that pre-existed (e.g. re-init over
// a partially-scaffolded tree) are left untouched.
//
// Cross-call DB atomicity (seed_chapters + patch_work in a single SQL
// transaction) requires transaction-aware variants of those helpers in
// nexus-local-db and is tracked as a follow-up under R-V133P1-09. The
// FS-side rollback addresses the primary "partial state on error" risk
// flagged by QC C-002 / C-2.

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

    /// Mark the scaffold as successfully committed; the Drop impl becomes
    /// a no-op. Call only after all DB writes succeed.
    const fn commit(&mut self) {
        self.committed = true;
    }
}

impl Drop for ScaffoldTransaction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        // Best-effort rollback. We log but do not panic — if the FS state
        // is inconsistent, the next idempotent re-init will reconcile.
        for f in &self.files_created {
            if let Err(e) = std::fs::remove_file(f) {
                tracing::warn!(
                    path = %f.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove_file failed"
                );
            }
        }
        // Remove dirs in reverse (children before parents).
        for d in self.dirs_created.iter().rev() {
            if let Err(e) = std::fs::remove_dir(d) {
                tracing::warn!(
                    path = %d.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove_dir failed (likely non-empty due to pre-existing entries — expected)"
                );
            }
        }
        tracing::warn!(
            files = self.files_created.len(),
            dirs = self.dirs_created.len(),
            "novel.project_scaffold: rolled back filesystem state"
        );
    }
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
        let rendered =
            render_template(tmpl, &[("body", "A & B < C > D")]).expect("no-escape render");
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
            "world_id": "wld_test_world",
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
        // V1.39 P3: Logs subdirectories
        assert!(scaffold_path.join("Logs/brainstorm").is_dir());
        assert!(scaffold_path.join("Logs/write").is_dir());
        assert!(scaffold_path.join("Logs/review").is_dir());
        assert!(scaffold_path.join("Logs/publish").is_dir());
        // V1.39 P3: Rules directory + Layer 2 stub
        assert!(scaffold_path.join("Rules").is_dir());
        assert!(scaffold_path.join("Rules/novel-rules.md").is_file());

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
            "world_id": "wld_idem_world",
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

    // ── T0.2: mandatory world_id binding tests ────────────────────────

    #[tokio::test]
    async fn scaffold_rejects_worldless_creation_missing_world_id_and_create_world() {
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = NovelProjectScaffold {
            pool: None,
            works_root: root,
        };

        // No world_id, no create_world → fail-closed
        let input = serde_json::json!({
            "creator_id": "creator_test",
            "work_id": "wrk_worldless_reject",
            "work_ref": "worldless-novel",
            "title": "Worldless Reject",
            "total_planned_chapters": 1,
        });

        let result = cap.run(input).await;
        assert!(
            result.is_err(),
            "scaffold must reject creation without world_id or create_world"
        );
        let err_msg = format!("{:?}", result.unwrap_err());
        assert!(
            err_msg.contains("V1.40 requires world_id"),
            "error should mention V1.40 mandatory binding, got: {err_msg}"
        );
        assert!(
            err_msg.contains("creator world list") || err_msg.contains("creator world create"),
            "error should mention remediation commands, got: {err_msg}"
        );
    }

    #[tokio::test]
    async fn scaffold_succeeds_with_valid_world_id() {
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = NovelProjectScaffold {
            pool: None,
            works_root: root.clone(),
        };

        let input = serde_json::json!({
            "creator_id": "creator_test",
            "work_id": "wrk_with_world",
            "work_ref": "worldbound-novel",
            "title": "Worldbound Novel",
            "world_id": "wld_valid_world_123",
            "total_planned_chapters": 2,
        });

        let out = cap
            .run(input)
            .await
            .expect("scaffold with world_id should succeed (no pool → FK check skipped)");
        let scaffold = out["scaffold_root"].as_str().expect("scaffold_root");
        let scaffold_path = Path::new(scaffold);

        assert!(scaffold_path.join("README.md").is_file());
        let readme = std::fs::read_to_string(scaffold_path.join("README.md")).expect("read README");
        assert!(
            readme.contains("wld_valid_world_123"),
            "README should contain the bound world_id"
        );
        assert!(
            !readme.contains("worldless"),
            "README should NOT contain worldless text for V1.40 bound Work"
        );
    }
}
