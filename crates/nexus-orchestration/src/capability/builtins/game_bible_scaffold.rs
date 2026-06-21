//! `game_bible.project_scaffold` capability.
//!
//! V1.54 P1: second non-novel profile scaffold.
//! Creates `Works/<work_ref>/Design/` with 12 template files, a README.md,
//! `Logs/design/` and `Logs/review/` directories, and `PATCH`es the works row
//! to set `work_profile = 'game_bible'` and `work_ref`.
//!
//! # Concurrency note
//!
//! This capability runs in the single-user daemon process. We assume:
//! 1. Only one `game_bible.project_scaffold` invocation per `(creator_id, work_id)`
//!    is in flight at any time.
//! 2. No external process is mutating `Works/<work_ref>/` while this runs.
//!
//! # `ScaffoldTransaction` (V1.55 P3 / R-V154P1-W001 + P3 fix-wave)
//!
//! Wraps FS writes + DB PATCH in a `ScaffoldTransaction` with Drop-based FS
//! rollback. Tracks create vs overwrite separately: on rollback, created
//! files are deleted and overwritten files are restored from snapshot.
//! Writes use temp+rename for atomicity.
//!
//! # Path safety (P3 fix-wave)
//!
//! Calls `validate_work_ref` from `novel_scaffold_sanitize` before any
//! path joining, mirroring the novel scaffold pattern.

use super::novel_scaffold_sanitize::validate_work_ref;
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::info;

/// Input for the game-bible scaffold capability.
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

/// Output from the game-bible scaffold capability.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ScaffoldOutput {
    scaffold_root: String,
    files_created: Vec<String>,
    dirs_created: Vec<String>,
}

/// Predefined Design template content with YAML frontmatter.
struct DesignTemplate {
    filename: &'static str,
    section_weight: &'static str,
    title: &'static str,
    comment: &'static str,
}

const DESIGN_TEMPLATES: &[DesignTemplate] = &[
    DesignTemplate {
        filename: "overview.md",
        section_weight: "critical",
        title: "Overview",
        comment: "Project vision, core loop summary, target audience",
    },
    DesignTemplate {
        filename: "pillars.md",
        section_weight: "critical",
        title: "Design Pillars",
        comment: "Core constraints, guiding principles, non-goals",
    },
    DesignTemplate {
        filename: "characters.md",
        section_weight: "important",
        title: "Characters",
        comment: "Character roles, archetypes, relationships",
    },
    DesignTemplate {
        filename: "factions.md",
        section_weight: "important",
        title: "Factions",
        comment: "Factions, politics, alignment, conflicts",
    },
    DesignTemplate {
        filename: "species.md",
        section_weight: "important",
        title: "Species",
        comment: "Sapient species, traits, cultures, biology",
    },
    DesignTemplate {
        filename: "locations.md",
        section_weight: "important",
        title: "Locations",
        comment: "World geography, levels, biomes, maps",
    },
    DesignTemplate {
        filename: "mechanics.md",
        section_weight: "critical",
        title: "Mechanics",
        comment: "Core mechanics, gameplay loops, systems",
    },
    DesignTemplate {
        filename: "magic_system.md",
        section_weight: "important",
        title: "Magic System",
        comment: "Magic/superpower rules, constraints, costs",
    },
    DesignTemplate {
        filename: "technology.md",
        section_weight: "important",
        title: "Technology",
        comment: "Tech level, tools, artifacts, research",
    },
    DesignTemplate {
        filename: "economy.md",
        section_weight: "important",
        title: "Economy",
        comment: "Currency, trade, resources, sinks, balance",
    },
    DesignTemplate {
        filename: "progression.md",
        section_weight: "important",
        title: "Progression",
        comment: "Leveling, skill trees, unlocks, player growth",
    },
    DesignTemplate {
        filename: "lore.md",
        section_weight: "nice_to_have",
        title: "Lore",
        comment: "History, mythology, cosmology, legends",
    },
];

fn render_template(tmpl: &DesignTemplate) -> String {
    format!(
        "---\nsection_status: draft\nsection_weight: {}\n---\n\n# {}\n<!-- {} -->\n",
        tmpl.section_weight, tmpl.title, tmpl.comment,
    )
}

// ── ScaffoldTransaction (V1.55 P3 fix-wave: create/overwrite tracking + atomic writes) ──

struct ScaffoldTransaction {
    created_files: Vec<PathBuf>,
    overwritten_files: Vec<(PathBuf, Vec<u8>)>,
    created_dirs: Vec<PathBuf>,
    temp_files: Vec<PathBuf>,
    committed: bool,
}

impl ScaffoldTransaction {
    const fn new() -> Self {
        Self {
            created_files: Vec::new(),
            overwritten_files: Vec::new(),
            created_dirs: Vec::new(),
            temp_files: Vec::new(),
            committed: false,
        }
    }

    const fn commit(&mut self) {
        self.committed = true;
    }

    fn create_dir(&mut self, dir: &Path) -> Result<bool, CapabilityError> {
        if dir.exists() {
            return Ok(false);
        }
        std::fs::create_dir_all(dir)
            .map_err(|e| CapabilityError::Internal(format!("mkdir {}: {e}", dir.display())))?;
        self.created_dirs.push(dir.to_path_buf());
        Ok(true)
    }

    fn write_file(&mut self, path: &Path, content: &str) -> Result<(), CapabilityError> {
        let original = if path.exists() {
            let data = std::fs::read(path).map_err(|e| {
                CapabilityError::Internal(format!("read original {}: {e}", path.display()))
            })?;
            Some(data)
        } else {
            None
        };

        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, content)
            .map_err(|e| CapabilityError::Internal(format!("write tmp {}: {e}", tmp.display())))?;
        self.temp_files.push(tmp.clone());

        std::fs::rename(&tmp, path).map_err(|e| {
            CapabilityError::Internal(format!(
                "rename {} -> {}: {e}",
                tmp.display(),
                path.display()
            ))
        })?;
        self.temp_files.retain(|t| t != &tmp);

        let is_create = original.is_none();
        match original {
            Some(data) => self.overwritten_files.push((path.to_path_buf(), data)),
            None => self.created_files.push(path.to_path_buf()),
        }

        info!(
            path = %path.display(),
            is_create,
            "game_bible_scaffold.write_file: ok"
        );
        Ok(())
    }
}

impl Drop for ScaffoldTransaction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        for tmp in &self.temp_files {
            if let Err(e) = std::fs::remove_file(tmp) {
                tracing::warn!(
                    path = %tmp.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove temp file failed"
                );
            }
        }
        for (path, original) in &self.overwritten_files {
            if let Err(e) = std::fs::write(path, original) {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: restore original failed"
                );
            }
        }
        for f in &self.created_files {
            if let Err(e) = std::fs::remove_file(f) {
                tracing::warn!(
                    path = %f.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove_file failed"
                );
            }
        }
        for d in self.created_dirs.iter().rev() {
            if let Err(e) = std::fs::remove_dir(d) {
                tracing::warn!(
                    path = %d.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove_dir failed (likely non-empty — expected)"
                );
            }
        }
        tracing::warn!(
            created_files = self.created_files.len(),
            overwritten_files = self.overwritten_files.len(),
            created_dirs = self.created_dirs.len(),
            temp_files = self.temp_files.len(),
            "game_bible.project_scaffold: rolled back filesystem state"
        );
    }
}

/// `game_bible.project_scaffold` capability.
pub struct GameBibleProjectScaffold {
    pool: Option<sqlx::SqlitePool>,
    works_root: PathBuf,
}

impl GameBibleProjectScaffold {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: None,
            works_root: PathBuf::from("Works"),
        }
    }

    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(pool),
            works_root: PathBuf::from("Works"),
        }
    }

    #[must_use]
    pub const fn new_with_root(pool: sqlx::SqlitePool, works_root: PathBuf) -> Self {
        Self {
            pool: Some(pool),
            works_root,
        }
    }
}

impl Default for GameBibleProjectScaffold {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Capability for GameBibleProjectScaffold {
    fn name(&self) -> &'static str {
        "game_bible.project_scaffold"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"creator_id":{"type":"string"},"work_id":{"type":"string"},"work_ref":{"type":"string"},"title":{"type":"string"},"world_id":{"type":["string","null"]}},"required":["creator_id","work_id","work_ref","title"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"scaffold_root":{"type":"string"},"files_created":{"type":"array","items":{"type":"string"}},"dirs_created":{"type":"array","items":{"type":"string"}}},"required":["scaffold_root","files_created","dirs_created"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: ScaffoldInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("game_bible.project_scaffold input: {e}"))
        })?;

        // ── FIX (P3 fix-wave): validate work_ref against path traversal ──
        let work_ref = validate_work_ref(&inp.work_ref)?;

        info!(
            work_id = %inp.work_id,
            work_ref = %work_ref,
            world_id = ?inp.world_id,
            "game_bible.project_scaffold: start"
        );

        let work_dir = self.works_root.join(&work_ref);
        let design_dir = work_dir.join("Design");
        let logs_dir = work_dir.join("Logs");
        let logs_design_dir = logs_dir.join("design");
        let logs_review_dir = logs_dir.join("review");

        let mut tx = ScaffoldTransaction::new();

        // Create directory structure (idempotent)
        for dir in [
            &work_dir,
            &design_dir,
            &logs_dir,
            &logs_design_dir,
            &logs_review_dir,
        ] {
            tx.create_dir(dir)?;
        }

        // Write README.md (atomic: temp+rename; tracks create vs overwrite)
        let readme_content = format!(
            "# {title}\n\nGame design bible.\n\n- **Work ID**: {work_id}\n- **Profile**: game_bible\n\n## Core Pillars\n\n<!-- Genre, tone, target audience, and key design constraints -->\n",
            title = inp.title,
            work_id = inp.work_id,
        );
        tx.write_file(&work_dir.join("README.md"), &readme_content)?;

        // Write 12 Design/*.md template files
        for tmpl in DESIGN_TEMPLATES {
            let content = render_template(tmpl);
            tx.write_file(&design_dir.join(tmpl.filename), &content)?;
        }

        // PATCH works row: set work_profile and work_ref
        if let Some(ref pool) = self.pool {
            sqlx::query(
                "UPDATE works SET work_profile = 'game_bible', work_ref = ? WHERE work_id = ?",
            )
            .bind(&work_ref)
            .bind(&inp.work_id)
            .execute(pool)
            .await
            .map_err(|e| CapabilityError::Internal(format!("patch works row: {e}")))?;
        }

        tx.commit();

        let files_created: Vec<String> = tx
            .created_files
            .iter()
            .chain(tx.overwritten_files.iter().map(|(p, _)| p))
            .map(|p| p.strip_prefix(&work_dir).unwrap_or(p).display().to_string())
            .collect();
        let dirs_created: Vec<String> = tx
            .created_dirs
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
            "game_bible.project_scaffold: done"
        );

        serde_json::to_value(output).map_err(|e| {
            CapabilityError::Internal(format!("game_bible.project_scaffold output: {e}"))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_design_templates_render_non_empty() {
        for tmpl in DESIGN_TEMPLATES {
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
    fn design_templates_count_is_twelve() {
        assert_eq!(DESIGN_TEMPLATES.len(), 12);
    }

    #[test]
    fn critical_sections_are_overview_pillars_mechanics() {
        let critical: Vec<&str> = DESIGN_TEMPLATES
            .iter()
            .filter(|t| t.section_weight == "critical")
            .map(|t| t.filename)
            .collect();
        assert!(critical.contains(&"overview.md"));
        assert!(critical.contains(&"pillars.md"));
        assert!(critical.contains(&"mechanics.md"));
    }

    #[test]
    fn game_bible_capability_name() {
        let cap = GameBibleProjectScaffold::new();
        assert_eq!(cap.name(), "game_bible.project_scaffold");
    }

    // ── ScaffoldTransaction tests ──

    #[test]
    fn scaffold_transaction_rollback_cleans_up_created_files() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("rollback-test-gb");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let file_path = work_dir.join("test.txt");
        std::fs::write(&file_path, "data").expect("write");
        let sub_dir = work_dir.join("subdir");
        std::fs::create_dir_all(&sub_dir).expect("mkdir subdir");

        let mut tx = ScaffoldTransaction::new();
        tx.created_files.push(file_path.clone());
        tx.created_dirs.push(sub_dir.clone());

        drop(tx);

        assert!(
            !file_path.exists(),
            "created file should be removed by rollback"
        );
        assert!(
            !sub_dir.exists(),
            "created subdir should be removed by rollback"
        );
        assert!(
            work_dir.exists(),
            "pre-existing work_dir should survive rollback"
        );
    }

    #[test]
    fn scaffold_transaction_commit_no_rollback() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("commit-test-gb");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let file_path = work_dir.join("keep.txt");
        std::fs::write(&file_path, "keep").expect("write");

        let mut tx = ScaffoldTransaction::new();
        tx.created_files.push(file_path.clone());
        tx.created_dirs.push(work_dir.clone());
        tx.commit();

        drop(tx);

        assert!(file_path.exists(), "file should remain after commit");
    }

    // ── Fix 1 regression: pre-existing user content survives rollback ──

    #[test]
    fn rollback_preserves_pre_existing_user_content() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("gb-user-data-survival");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let readme_path = work_dir.join("README.md");
        let user_content = "# My Game Bible\n\nPersonal design notes.\n";
        std::fs::write(&readme_path, user_content).expect("write user README");

        let mut tx = ScaffoldTransaction::new();
        tx.create_dir(&work_dir).unwrap();
        let scaffold_content = "# Overwritten\n\nScaffold template.\n";
        tx.write_file(&readme_path, scaffold_content)
            .expect("write scaffold README");

        // Rollback without commit
        drop(tx);

        let restored = std::fs::read_to_string(&readme_path).expect("read restored README");
        assert_eq!(
            restored, user_content,
            "rollback must restore pre-existing user-authored file content"
        );
        assert!(!restored.contains("Overwritten"));
    }

    // ── Fix 2 regression: path traversal rejection ──

    #[tokio::test]
    async fn game_bible_scaffold_rejects_path_traversal_in_work_ref() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = GameBibleProjectScaffold {
            pool: None,
            works_root: root.clone(),
        };

        let input = serde_json::json!({
            "creator_id": "attacker",
            "work_id": "wrk_gb_traversal",
            "work_ref": "../etc/passwd",
            "title": "Path Traversal",
        });

        let result = cap.run(input).await;
        assert!(
            result.is_err(),
            "game_bible scaffold must reject path traversal in work_ref"
        );
        let err_msg = format!("{}", result.unwrap_err());
        assert!(
            err_msg.contains("path-traversal")
                || err_msg.contains("contains invalid character")
                || err_msg.contains("must start with [a-z0-9]"),
            "error must mention traversal/invalid path, got: {err_msg}"
        );
    }

    // ── Fix 4 regression: crash mid-transaction leaves no half-written file ──

    #[test]
    fn crash_mid_transaction_leaves_no_half_written_file() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("gb-crash-test");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let final_path = work_dir.join("template.md");
        let tmp_path = final_path.with_extension("tmp");
        std::fs::write(&tmp_path, "partial content").expect("write temp");

        let mut tx = ScaffoldTransaction::new();
        tx.temp_files.push(tmp_path.clone());

        drop(tx);

        assert!(
            !tmp_path.exists(),
            "temp file must be cleaned up on rollback"
        );
        assert!(!final_path.exists(), "final path must not exist");
    }
}
