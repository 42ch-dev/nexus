//! `novel.chapter_transition` capability.
//!
//! Transitions a single chapter's status in both the `work_chapters` DB row
//! and the chapter `.md` frontmatter atomically. Also advances the
//! `works.current_chapter` field when appropriate (T4).
//!
//! Design: novel-workflow-profile.md §4.1.2 (truth model), §5.1 (finalize gate).

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::Deserialize;
use serde_json::Value;
use std::path::PathBuf;
use tracing::{info, warn};

// ---------------------------------------------------------------------------
// Input / Output types
// ---------------------------------------------------------------------------

/// Capability input.
#[derive(Debug, Deserialize)]
struct TransitionInput {
    /// Work entity ID (wrk_...).
    work_id: String,
    /// Creator ID (`ctr_...`), required for `works.patch_work`.
    #[serde(default)]
    creator_id: Option<String>,
    /// Chapter number (1-based).
    chapter: i32,
    /// Volume number (V1.42: defaults to 1 for single-volume Works).
    #[serde(default = "default_volume")]
    volume: i32,
    /// Status to transition FROM (guard; mismatch = error).
    from_status: String,
    /// Status to transition TO.
    to_status: String,
    /// Optional actual word count (set on finalize).
    #[serde(default)]
    actual_word_count: Option<u32>,
    /// When true, override a NOGO judge result (audit-logged).
    #[serde(default)]
    force: bool,
    /// Reason for force override (required when force=true).
    #[serde(default)]
    reason: Option<String>,
    /// Workspace root path.
    #[serde(default)]
    workspace_root: Option<String>,
    /// Work reference (directory name under Works/).
    #[serde(default)]
    work_ref: Option<String>,
    /// Body path relative to workspace root (for frontmatter update).
    #[serde(default)]
    body_path: Option<String>,
}

/// Default volume number (1 for single-volume Works).
const fn default_volume() -> i32 {
    1
}

/// Capability output.
#[derive(Debug, serde::Serialize)]
struct TransitionOutput {
    /// Previous status.
    from_status: String,
    /// New status.
    to_status: String,
    /// Whether the transition was forced.
    forced: bool,
    /// Actual word count (if set).
    actual_word_count: Option<u32>,
}

// ---------------------------------------------------------------------------
// Capability struct
// ---------------------------------------------------------------------------

/// Chapter status transition capability.
///
/// In standalone mode (no pool), performs frontmatter-only transitions
/// (testing / preview). With a pool, also updates the `work_chapters` DB row.
pub struct NovelChapterTransition {
    pool: Option<sqlx::SqlitePool>,
}

impl NovelChapterTransition {
    /// Create a standalone (pool-less) instance.
    #[must_use]
    pub const fn new() -> Self {
        Self { pool: None }
    }

    /// Create a pool-backed instance.
    #[must_use]
    pub const fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self { pool: Some(pool) }
    }
}

impl Default for NovelChapterTransition {
    fn default() -> Self {
        Self::new()
    }
}

const INPUT_SCHEMA: &str = r#"{"type":"object","properties":{"work_id":{"type":"string"},"chapter":{"type":"integer","minimum":1},"from_status":{"type":"string"},"to_status":{"type":"string"},"actual_word_count":{"type":"integer","minimum":0},"force":{"type":"boolean","default":false},"reason":{"type":"string"},"workspace_root":{"type":"string"},"work_ref":{"type":"string"},"body_path":{"type":"string"}},"required":["work_id","chapter","from_status","to_status"],"additionalProperties":false}"#;

const OUTPUT_SCHEMA: &str = r#"{"type":"object","properties":{"from_status":{"type":"string"},"to_status":{"type":"string"},"forced":{"type":"boolean"},"actual_word_count":{"type":["integer","null"],"minimum":0}},"required":["from_status","to_status","forced"],"additionalProperties":false}"#;

#[async_trait]
impl Capability for NovelChapterTransition {
    fn name(&self) -> &'static str {
        "novel.chapter_transition"
    }

    fn input_schema(&self) -> &'static str {
        INPUT_SCHEMA
    }

    fn output_schema(&self) -> &'static str {
        OUTPUT_SCHEMA
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: TransitionInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("novel.chapter_transition input: {e}"))
        })?;

        let forced = inp.force;

        // Audit log for force overrides
        if forced {
            let reason = inp.reason.as_deref().unwrap_or("<no reason>");
            warn!(
                work_id = %inp.work_id,
                chapter = inp.chapter,
                reason = %reason,
                "forced_finalize_on_nogo"
            );
        }

        // If pool is available, update DB row
        if let Some(pool) = &self.pool {
            self.transition_db(pool, &inp).await?;
        }

        // If workspace_root + work_ref + body_path available, update frontmatter
        if let (Some(ws_root), Some(_work_ref), Some(body_path)) =
            (&inp.workspace_root, &inp.work_ref, &inp.body_path)
        {
            let full_path = PathBuf::from(ws_root).join(body_path);
            if full_path.exists() {
                Self::update_frontmatter_status(&full_path, &inp.to_status)?;
            } else {
                info!(
                    path = %full_path.display(),
                    "chapter body file not found; skipping frontmatter update"
                );
            }
        }

        info!(
            work_id = %inp.work_id,
            chapter = inp.chapter,
            from = %inp.from_status,
            to = %inp.to_status,
            forced = forced,
            "chapter_transition_completed"
        );

        let output = TransitionOutput {
            from_status: inp.from_status.clone(),
            to_status: inp.to_status.clone(),
            forced,
            actual_word_count: inp.actual_word_count,
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

impl NovelChapterTransition {
    /// Update the `work_chapters` DB row.
    async fn transition_db(
        &self,
        pool: &sqlx::SqlitePool,
        inp: &TransitionInput,
    ) -> Result<(), CapabilityError> {
        let now = chrono::Utc::now().to_rfc3339();

        // Read current status to validate from_status guard
        let current =
            nexus_local_db::work_chapters::get_chapter(pool, &inp.work_id, inp.chapter, inp.volume)
                .await
                .map_err(|e| CapabilityError::Internal(format!("get_chapter: {e}")))?;

        let row = current.ok_or_else(|| {
            CapabilityError::InputInvalid(format!(
                "chapter {} not found for work {}",
                inp.chapter, inp.work_id
            ))
        })?;

        if row.status != inp.from_status && !inp.force {
            return Err(CapabilityError::InputInvalid(format!(
                "status guard mismatch: expected '{}', actual '{}'",
                inp.from_status, row.status
            )));
        }

        // Determine word_count for finalize transitions
        let actual_wc = if inp.to_status == "finalized" {
            inp.actual_word_count
        } else {
            None
        };

        nexus_local_db::work_chapters::update_status(
            pool,
            &inp.work_id,
            inp.chapter,
            inp.volume,
            &inp.to_status,
            actual_wc,
            &now,
        )
        .await
        .map_err(|e| CapabilityError::Internal(format!("update_status: {e}")))?;

        // T6 (V1.38 P0): current_chapter advances ONLY on finalize.
        // Per novel-workflow-profile §4.5.2, current_chapter is the latest
        // finalized chapter number — not the chapter being drafted or outlined.
        if inp.to_status == "finalized" {
            if let Some(cid) = &inp.creator_id {
                Self::advance_current_chapter(pool, cid, &inp.work_id, inp.chapter).await?;
            }
        }

        Ok(())
    }

    /// Advance `works.current_chapter` to the given chapter number.
    async fn advance_current_chapter(
        pool: &sqlx::SqlitePool,
        creator_id: &str,
        work_id: &str,
        chapter: i32,
    ) -> Result<(), CapabilityError> {
        let now = chrono::Utc::now().to_rfc3339();
        let patch = nexus_local_db::works::WorkPatch {
            current_chapter: Some(chapter),
            ..Default::default()
        };
        nexus_local_db::works::patch_work(pool, creator_id, work_id, &patch, &now)
            .await
            .map_err(|e| CapabilityError::Internal(format!("patch_work current_chapter: {e}")))?;

        info!(
            work_id = %work_id,
            current_chapter = chapter,
            "current_chapter_advanced"
        );
        Ok(())
    }

    /// Update the `status` field in a chapter `.md` file's YAML frontmatter.
    fn update_frontmatter_status(
        path: &std::path::Path,
        new_status: &str,
    ) -> Result<(), CapabilityError> {
        let content = std::fs::read_to_string(path).map_err(|e| {
            CapabilityError::Internal(format!("read chapter file {}: {e}", path.display()))
        })?;

        let updated = Self::replace_frontmatter_field(&content, "status", new_status);

        std::fs::write(path, updated).map_err(|e| {
            CapabilityError::Internal(format!("write chapter file {}: {e}", path.display()))
        })?;

        info!(
            path = %path.display(),
            status = new_status,
            "frontmatter_status_updated"
        );
        Ok(())
    }

    /// Replace a single YAML frontmatter field value.
    ///
    /// Simple line-based replacement: finds `key: <old>` between `---` delimiters
    /// and replaces with `key: <new>`.
    fn replace_frontmatter_field(content: &str, key: &str, new_value: &str) -> String {
        let mut result = String::with_capacity(content.len());
        let mut in_frontmatter = false;
        let mut found_first_delim = false;

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
                    if rest.starts_with(':') || rest.starts_with(" :") {
                        // Replace the value
                        result.push_str(key);
                        result.push_str(": ");
                        result.push_str(new_value);
                        result.push('\n');
                        continue;
                    }
                }
            }

            result.push_str(line);
            result.push('\n');
        }

        result
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_replace_frontmatter_status() {
        let input = "\
---
title: My Chapter
chapter: 1
status: draft
word_count: 3200
---
Body text here.";

        let output =
            NovelChapterTransition::replace_frontmatter_field(input, "status", "finalized");

        assert!(output.contains("status: finalized"));
        assert!(!output.contains("status: draft"));
        assert!(output.contains("title: My Chapter"));
        assert!(output.contains("word_count: 3200"));
        assert!(output.contains("Body text here."));
    }

    #[test]
    fn test_replace_frontmatter_preserves_no_frontmatter() {
        let input = "Just body text, no frontmatter.";
        let output =
            NovelChapterTransition::replace_frontmatter_field(input, "status", "finalized");
        assert_eq!(output, input.to_string() + "\n");
    }

    #[tokio::test]
    async fn test_chapter_transition_standalone() {
        let cap = NovelChapterTransition::new();
        let out = cap
            .run(serde_json::json!({
                "work_id": "wrk_test",
                "chapter": 1,
                "from_status": "draft",
                "to_status": "finalized",
                "actual_word_count": 3200
            }))
            .await
            .unwrap();

        assert_eq!(out["from_status"], "draft");
        assert_eq!(out["to_status"], "finalized");
        assert_eq!(out["forced"], false);
        assert_eq!(out["actual_word_count"], 3200);
    }

    #[tokio::test]
    async fn test_chapter_transition_force_override() {
        let cap = NovelChapterTransition::new();
        let out = cap
            .run(serde_json::json!({
                "work_id": "wrk_test",
                "chapter": 1,
                "from_status": "draft",
                "to_status": "finalized",
                "force": true,
                "reason": "manual quality approval"
            }))
            .await
            .unwrap();

        assert_eq!(out["forced"], true);
    }

    #[tokio::test]
    async fn test_chapter_transition_with_frontmatter_file() {
        let dir = tempfile::tempdir().unwrap();
        let chapter_path = dir.path().join("ch01-intro.md");
        std::fs::write(
            &chapter_path,
            "---\ntitle: Intro\nchapter: 1\nstatus: draft\nword_count: 2500\n---\nBody text.",
        )
        .unwrap();

        let cap = NovelChapterTransition::new();
        let out = cap
            .run(serde_json::json!({
                "work_id": "wrk_test",
                "chapter": 1,
                "from_status": "draft",
                "to_status": "finalized",
                "workspace_root": dir.path().to_str().unwrap(),
                "work_ref": "my-novel",
                "body_path": "ch01-intro.md",
                "actual_word_count": 2500
            }))
            .await
            .unwrap();

        assert_eq!(out["to_status"], "finalized");

        // Verify frontmatter was updated
        let updated = std::fs::read_to_string(&chapter_path).unwrap();
        assert!(updated.contains("status: finalized"));
        assert!(!updated.contains("status: draft"));
    }

    // =======================================================================
    // §4.5.7 #2 (V1.47 P2): current_chapter finalize-only advance
    // novel-workflow-profile §4.5.2 invariant:
    //   "works.current_chapter is updated only on transition to finalized.
    //    Its value is the chapter number of the latest finalized row, not the
    //    chapter currently being outlined or drafted."
    // =======================================================================

    /// Create a fresh test DB with migrations and return the pool.
    async fn fresh_pool() -> sqlx::SqlitePool {
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run migrations");
        // Keep the tempdir alive for the test lifetime via leak (test-only).
        std::mem::forget(dir);
        pool
    }

    /// Insert a minimal work row with `current_chapter = 0`.
    async fn insert_test_work(pool: &sqlx::SqlitePool, work_id: &str) {
        // SAFETY: INSERT against works — runtime query.
        sqlx::query(
            "INSERT INTO works (work_id, creator_id, workspace_slug, status, title, \
             long_term_goal, initial_idea, intake_status, inspiration_log, \
             primary_preset_id, schedule_ids, current_chapter, total_planned_chapters, \
             created_at, updated_at) \
             VALUES (?, 'ctr_test', 'default', 'draft', 'Test', 'Goal', 'Idea', \
             'complete', '[]', 'novel-writing', '[]', 0, 3, ?, ?)",
        )
        .bind(work_id)
        .bind("2026-06-15T10:00:00Z")
        .bind("2026-06-15T10:00:00Z")
        .execute(pool)
        .await
        .unwrap();
    }

    /// Read `works.current_chapter` for a work.
    async fn read_current_chapter(pool: &sqlx::SqlitePool, work_id: &str) -> i32 {
        // SAFETY: SELECT against works — runtime query.
        let row: (i32,) = sqlx::query_as("SELECT current_chapter FROM works WHERE work_id = ?")
            .bind(work_id)
            .fetch_one(pool)
            .await
            .unwrap();
        row.0
    }

    /// §4.5.7 #2 — `current_chapter` advances ONLY on transition to `finalized`
    /// and takes the just-finalized chapter number. Non-finalize transitions
    /// (not_started → outlined, outlined → draft) do NOT change
    /// `works.current_chapter`.
    #[tokio::test]
    async fn spec_4_5_7_current_chapter_advances_only_on_finalize() {
        let pool = fresh_pool().await;
        insert_test_work(&pool, "wrk_457_cc").await;

        // Seed 3 chapters (ch1, ch2, ch3 — all not_started, volume=1).
        nexus_local_db::work_chapters::seed_chapters(
            &pool,
            "wrk_457_cc",
            "my-novel",
            3,
            "2026-06-15T10:00:00Z",
        )
        .await
        .unwrap();

        let cap = NovelChapterTransition::with_pool(pool.clone());

        // Initial state: current_chapter = 0
        assert_eq!(
            read_current_chapter(&pool, "wrk_457_cc").await,
            0,
            "initial current_chapter should be 0"
        );

        // Transition ch1: not_started → outlined — current_chapter must NOT change.
        cap.run(serde_json::json!({
            "work_id": "wrk_457_cc",
            "creator_id": "ctr_test",
            "chapter": 1,
            "volume": 1,
            "from_status": "not_started",
            "to_status": "outlined",
        }))
        .await
        .unwrap();
        assert_eq!(
            read_current_chapter(&pool, "wrk_457_cc").await,
            0,
            "§4.5.2 invariant: not_started→outlined must NOT advance current_chapter"
        );

        // Transition ch1: outlined → draft — current_chapter must NOT change.
        cap.run(serde_json::json!({
            "work_id": "wrk_457_cc",
            "creator_id": "ctr_test",
            "chapter": 1,
            "volume": 1,
            "from_status": "outlined",
            "to_status": "draft",
        }))
        .await
        .unwrap();
        assert_eq!(
            read_current_chapter(&pool, "wrk_457_cc").await,
            0,
            "§4.5.2 invariant: outlined→draft must NOT advance current_chapter"
        );

        // Transition ch1: draft → finalized — current_chapter becomes 1.
        cap.run(serde_json::json!({
            "work_id": "wrk_457_cc",
            "creator_id": "ctr_test",
            "chapter": 1,
            "volume": 1,
            "from_status": "draft",
            "to_status": "finalized",
            "actual_word_count": 4000,
        }))
        .await
        .unwrap();
        assert_eq!(
            read_current_chapter(&pool, "wrk_457_cc").await,
            1,
            "§4.5.2 invariant: draft→finalized advances current_chapter to 1"
        );

        // Transition ch3: not_started → draft (skip ch2) — current_chapter
        // must NOT jump to 3. It stays at 1 because draft is not finalize.
        // This verifies the "never skips ahead" clause.
        cap.run(serde_json::json!({
            "work_id": "wrk_457_cc",
            "creator_id": "ctr_test",
            "chapter": 3,
            "volume": 1,
            "from_status": "not_started",
            "to_status": "draft",
        }))
        .await
        .unwrap();
        assert_eq!(
            read_current_chapter(&pool, "wrk_457_cc").await,
            1,
            "§4.5.2 invariant: drafting ch3 must NOT advance current_chapter (only finalize does)"
        );

        // Transition ch2: not_started → finalized (out of order) —
        // current_chapter becomes 2 (just-finalized chapter number).
        cap.run(serde_json::json!({
            "work_id": "wrk_457_cc",
            "creator_id": "ctr_test",
            "chapter": 2,
            "volume": 1,
            "from_status": "not_started",
            "to_status": "finalized",
            "actual_word_count": 3500,
        }))
        .await
        .unwrap();
        assert_eq!(
            read_current_chapter(&pool, "wrk_457_cc").await,
            2,
            "§4.5.2 invariant: finalize ch2 → current_chapter becomes 2 (just-finalized)"
        );
    }
}
