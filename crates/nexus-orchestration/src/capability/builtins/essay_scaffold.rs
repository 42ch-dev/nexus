//! `essay.project_scaffold` capability.
//!
//! V1.52 T-A P2: first non-novel profile scaffold.
//! Creates `Works/<work_ref>/Outlines/outline.md` and `Drafts/draft.md`
//! from embedded templates, a README.md, and `PATCH`es the works row
//! to set `work_profile = 'essay'` and `work_ref`.
//!
//! # Concurrency note (V1.52 T-A P2)
//!
//! This capability runs in the single-user daemon process. We assume:
//! 1. Only one `essay.project_scaffold` invocation per `(creator_id, work_id)`
//!    is in flight at any time.
//! 2. No external process is mutating `Works/<work_ref>/` while this runs.
//!
//! # Deferred (W-005, V1.52 P-last WL-A)
//!
//! The current implementation creates FS artifacts and updates the DB row
//! in separate steps (TOCTOU window). The `novel.project_scaffold` wraps both
//! in a `ScaffoldTransaction` with Drop-based FS rollback. The essay scaffold
//! should adopt the same pattern before production use with concurrent
//! presets. Tracked as deferred residual in `.mstar/status.json`.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;
use tracing::info;

use crate::capability::{Capability, CapabilityError};

/// Input for the essay scaffold capability.
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

/// Output from the essay scaffold capability.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ScaffoldOutput {
    scaffold_root: String,
    files_created: Vec<String>,
    dirs_created: Vec<String>,
}

/// `essay.project_scaffold` capability.
pub struct EssayProjectScaffold {
    pool: Option<sqlx::SqlitePool>,
    works_root: PathBuf,
}

impl EssayProjectScaffold {
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
}

impl Default for EssayProjectScaffold {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for EssayProjectScaffold {
    fn name(&self) -> &'static str {
        "essay.project_scaffold"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]}},"required":["creator_id","work_id","work_ref","title"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"scaffold_root":{"type":"string"},"files_created":{"type":"array","items":{"type":"string"}},"dirs_created":{"type":"array","items":{"type":"string"}}},"required":["scaffold_root","files_created","dirs_created"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: ScaffoldInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("essay.project_scaffold input: {e}"))
        })?;

        info!(
            work_id = %inp.work_id,
            work_ref = %inp.work_ref,
            world_id = ?inp.world_id,
            "essay.project_scaffold: start"
        );

        let work_dir = self.works_root.join(&inp.work_ref);
        let outlines_dir = work_dir.join("Outlines");
        let drafts_dir = work_dir.join("Drafts");
        let logs_dir = work_dir.join("Logs");

        let mut files_created = Vec::new();
        let mut dirs_created = Vec::new();

        // Create directory structure
        for dir in [&work_dir, &outlines_dir, &drafts_dir, &logs_dir] {
            if !dir.exists() {
                tokio::fs::create_dir_all(dir).await.map_err(|e| {
                    CapabilityError::Internal(format!("mkdir {}: {e}", dir.display()))
                })?;
                dirs_created.push(
                    dir.strip_prefix(&self.works_root)
                        .unwrap_or(dir.as_path())
                        .display()
                        .to_string(),
                );
            }
        }

        // Write README.md
        let readme_path = work_dir.join("README.md");
        let readme_content = format!(
            "# {title}\n\nEssay project.\n\n- **Work ID**: {work_id}\n- **Profile**: essay\n",
            title = inp.title,
            work_id = inp.work_id,
        );
        tokio::fs::write(&readme_path, &readme_content)
            .await
            .map_err(|e| CapabilityError::Internal(format!("write README.md: {e}")))?;
        files_created.push("README.md".to_string());

        // Write Outlines/outline.md
        let outline_path = outlines_dir.join("outline.md");
        let outline_content = format!(
            "---\ntitle: {title}\nstatus: outline\n---\n\n# Thesis\n\n# Audience\n\n# Structure\n\n1. Opening hook\n2. Core argument\n3. Supporting evidence\n4. Counterpoint / nuance\n5. Ending takeaway\n",
            title = inp.title,
        );
        tokio::fs::write(&outline_path, &outline_content)
            .await
            .map_err(|e| CapabilityError::Internal(format!("write outline.md: {e}")))?;
        files_created.push("Outlines/outline.md".to_string());

        // Write Drafts/draft.md
        let draft_path = drafts_dir.join("draft.md");
        let draft_content = format!(
            "---\ntitle: {title}\nstatus: draft\nword_count: 0\n---\n\n# {title}\n\nWrite your essay here.\n",
            title = inp.title,
        );
        tokio::fs::write(&draft_path, &draft_content)
            .await
            .map_err(|e| CapabilityError::Internal(format!("write draft.md: {e}")))?;
        files_created.push("Drafts/draft.md".to_string());

        // PATCH works row: set work_profile and work_ref
        if let Some(ref pool) = self.pool {
            sqlx::query("UPDATE works SET work_profile = 'essay', work_ref = ? WHERE work_id = ?")
                .bind(&inp.work_ref)
                .bind(&inp.work_id)
                .execute(pool)
                .await
                .map_err(|e| CapabilityError::Internal(format!("patch works row: {e}")))?;
        }

        let output = ScaffoldOutput {
            scaffold_root: work_dir.display().to_string(),
            files_created,
            dirs_created,
        };

        info!(
            work_id = %inp.work_id,
            files = ?output.files_created,
            "essay.project_scaffold: done"
        );

        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("essay.project_scaffold output: {e}")))
    }
}
