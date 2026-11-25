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
//! Writes commit through the injected Work artifact port (Work lock +
//! durable atomic write); the rollback guard still restores via Drop.
//!
//! # Path safety (P3 fix-wave)
//!
//! Calls `validate_work_ref` from `novel_scaffold_sanitize` before any
//! path joining, mirroring the novel scaffold pattern.

use super::novel_scaffold_sanitize::validate_work_ref;
use super::work_commit::{
    claim_work_ref_with_recording, commit_work_bytes, publish_work_identity_with_recording,
    require_executor, resolve_works_dirs,
};
use crate::{path_is_occupied, Capability, CapabilityError, WorkspaceExecutor};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// Input for the game-bible scaffold capability.
#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
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

struct ScaffoldTransaction {
    created_files: Vec<PathBuf>,
    overwritten_files: Vec<(PathBuf, Vec<u8>)>,
    created_dirs: Vec<PathBuf>,
    temp_files: Vec<PathBuf>,
    /// Whether the executor records its writes instead of landing them. Such a
    /// run never owns a path on disk, so its rollback is inert.
    recording: bool,
    committed: bool,
}

impl ScaffoldTransaction {
    const fn new(recording: bool) -> Self {
        Self {
            created_files: Vec::new(),
            overwritten_files: Vec::new(),
            created_dirs: Vec::new(),
            temp_files: Vec::new(),
            recording,
            committed: false,
        }
    }

    const fn commit(&mut self) {
        self.committed = true;
    }

    /// Create a directory if it doesn't exist. Returns `true` if freshly created.
    ///
    /// Under a recording executor the checks still run — an occupied path
    /// refuses exactly as it does in a real run — but nothing is created: the
    /// bytes those directories would hold never land, so leaving them behind
    /// would change the tree a dry run promises to leave alone.
    fn create_dir(&mut self, dir: &Path) -> Result<bool, CapabilityError> {
        if dir.is_dir() {
            return Ok(false);
        }
        if path_is_occupied(dir) {
            return Err(CapabilityError::Internal(format!(
                "mkdir {}: occupied by a non-directory",
                dir.display()
            )));
        }
        if self.recording {
            return Ok(false);
        }
        std::fs::create_dir_all(dir)
            .map_err(|e| CapabilityError::Internal(format!("mkdir {}: {e}", dir.display())))?;
        self.created_dirs.push(dir.to_path_buf());
        Ok(true)
    }

    /// Commit a file through the Work artifact port.
    ///
    /// If the file already exists, its original content is saved so rollback
    /// can restore it. The bytes land through the injected executor (Work
    /// lock + durable atomic write); rollback bookkeeping stays here.
    async fn write_file(
        &mut self,
        executor: &dyn WorkspaceExecutor,
        workspace_root: &Path,
        work_ref: &str,
        rel_child: &Path,
        content: &str,
    ) -> Result<(), CapabilityError> {
        let path = workspace_root.join("Works").join(work_ref).join(rel_child);

        let original = if path_is_occupied(&path) {
            let data = std::fs::read(&path).map_err(|e| {
                CapabilityError::Internal(format!("read original {}: {e}", path.display()))
            })?;
            Some(data)
        } else {
            None
        };

        commit_work_bytes(executor, workspace_root, work_ref, rel_child, content).await?;

        let is_create = original.is_none();
        // A recording run lands nothing, so it owns nothing: reporting these
        // paths as created would name files that do not exist.
        if !self.recording {
            match original {
                Some(data) => self.overwritten_files.push((path.clone(), data)),
                None => self.created_files.push(path.clone()),
            }
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
        if self.committed || self.recording {
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
    executor: Option<Arc<dyn WorkspaceExecutor>>,
}

impl GameBibleProjectScaffold {
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: None,
            works_root: PathBuf::from("Works"),
            executor: None,
        }
    }

    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(pool),
            works_root: PathBuf::from("Works"),
            executor: None,
        }
    }

    #[must_use]
    pub const fn new_with_root(pool: sqlx::SqlitePool, works_root: PathBuf) -> Self {
        Self {
            pool: Some(pool),
            works_root,
            executor: None,
        }
    }

    /// Attach the workspace executor that carries the Work artifact port.
    #[must_use]
    pub fn with_workspace_executor(mut self, executor: Option<Arc<dyn WorkspaceExecutor>>) -> Self {
        self.executor = executor;
        self
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
        nexus_preset::capability_catalog::GAME_BIBLE_PROJECT_SCAFFOLD_INPUT_SCHEMA
    }
    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"scaffold_root":{"type":"string"},"files_created":{"type":"array","items":{"type":"string"}},"dirs_created":{"type":"array","items":{"type":"string"}}},"required":["scaffold_root","files_created","dirs_created"],"additionalProperties":false}"#
    }

    // SAFETY: The run method handles 5 directory/file operations through the
    // Work artifact port; the length is inherent to the scaffold batch.
    #[allow(clippy::too_many_lines)]
    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: ScaffoldInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("game_bible.project_scaffold input: {e}"))
        })?;

        let work_ref = validate_work_ref(&inp.work_ref)?;

        info!(
            work_id = %inp.work_id,
            work_ref = %work_ref,
            world_id = ?inp.world_id,
            "game_bible.project_scaffold: start"
        );

        let executor = require_executor(self.executor.as_ref())?;
        let recording = executor.records_work_artifacts();
        let (workspace_root, resolved_works) = resolve_works_dirs(executor, &self.works_root)?;
        let work_dir = resolved_works.join(&work_ref);
        let design_dir = work_dir.join("Design");
        let logs_dir = work_dir.join("Logs");
        let logs_design_dir = logs_dir.join("design");
        let logs_review_dir = logs_dir.join("review");

        let mut claim = claim_work_ref_with_recording(
            self.pool.as_ref(),
            &inp.creator_id,
            &inp.work_id,
            Some(&work_ref),
            recording,
        )
        .await?;

        let result: Result<(Vec<String>, Vec<String>), CapabilityError> = async {
            let mut tx = ScaffoldTransaction::new(recording);

            for dir in [
                &work_dir,
                &design_dir,
                &logs_dir,
                &logs_design_dir,
                &logs_review_dir,
            ] {
                tx.create_dir(dir)?;
            }

            let readme_content = format!(
        "# {title}\n\nGame design bible.\n\n- **Work ID**: {work_id}\n- **Profile**: game_bible\n\n## Core Pillars\n\n<!-- Genre, tone, target audience, and key design constraints -->\n",
        title = inp.title,
        work_id = inp.work_id,
    );
            tx.write_file(
                executor,
                &workspace_root,
                &work_ref,
                Path::new("README.md"),
                &readme_content,
            )
            .await?;

            for tmpl in DESIGN_TEMPLATES {
                let content = render_template(tmpl);
                let rel_child = Path::new("Design").join(tmpl.filename);
                tx.write_file(executor, &workspace_root, &work_ref, &rel_child, &content)
                    .await?;
            }

            publish_work_identity_with_recording(
                self.pool.as_ref(),
                &inp.creator_id,
                &inp.work_id,
                "game_bible",
                &work_ref,
                recording,
            )
            .await?;

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

            tx.commit();
            Ok((files_created, dirs_created))
        }
        .await;

        let (files_created, dirs_created) = match result {
            Ok(created) => created,
            Err(err) => {
                if let Err(restore_err) = claim.restore().await {
                    tracing::warn!(
                        work_id = %inp.work_id,
                        error = %restore_err,
                        "game_bible.project_scaffold: restore prior work_ref failed"
                    );
                }
                return Err(err);
            }
        };
        claim.disarm();

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
    use super::super::work_commit::test_support::{
        pool_with_work, stored_identity, RecordingExecutor,
    };
    use super::*;
    use std::collections::BTreeMap;

    /// Capability over a temp `Works/` root with a recording executor.
    fn cap_with_root(works_root: PathBuf) -> GameBibleProjectScaffold {
        let executor: Arc<dyn WorkspaceExecutor> =
            RecordingExecutor::at(works_root.parent().expect("works root has a parent"));
        GameBibleProjectScaffold {
            pool: None,
            works_root,
            executor: Some(executor),
        }
    }

    /// Every entry below `root` and the bytes it holds (`None` for a directory).
    fn tree_snapshot(root: &Path) -> BTreeMap<PathBuf, Option<Vec<u8>>> {
        let mut snapshot = BTreeMap::new();
        let mut pending = vec![root.to_path_buf()];
        while let Some(dir) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    snapshot.insert(path.clone(), None);
                    pending.push(path);
                } else {
                    snapshot.insert(path.clone(), std::fs::read(&path).ok());
                }
            }
        }
        snapshot
    }

    /// A dry-run executor: reports recording and lands no bytes. `fail_on`
    /// reproduces the port refusing one later artifact.
    struct DryRunExecutor {
        workspace_root: PathBuf,
        fail_on: Option<&'static str>,
    }

    impl DryRunExecutor {
        fn at(workspace_root: PathBuf) -> Self {
            Self {
                workspace_root,
                fail_on: None,
            }
        }

        fn refusing_at(workspace_root: PathBuf, rel_child: &'static str) -> Self {
            Self {
                workspace_root,
                fail_on: Some(rel_child),
            }
        }
    }

    #[async_trait]
    impl WorkspaceExecutor for DryRunExecutor {
        async fn open(
            &self,
            _input: nexus_contracts::local::orchestration::WorkspaceOpenInput,
        ) -> Result<nexus_contracts::local::orchestration::WorkspaceOpenOutput, CapabilityError>
        {
            Err(CapabilityError::WorkerUnavailable)
        }

        async fn commit(
            &self,
            _input: nexus_contracts::local::orchestration::WorkspaceCommitInput,
        ) -> Result<nexus_contracts::local::orchestration::WorkspaceCommitOutput, CapabilityError>
        {
            Err(CapabilityError::WorkerUnavailable)
        }

        fn workspace_root(&self) -> Result<PathBuf, CapabilityError> {
            Ok(self.workspace_root.clone())
        }

        fn records_work_artifacts(&self) -> bool {
            true
        }

        async fn commit_work_artifact(
            &self,
            input: nexus_contracts::local::orchestration::WorkArtifactCommitInput,
        ) -> Result<nexus_contracts::local::orchestration::WorkArtifactCommitOutput, CapabilityError>
        {
            if self.fail_on == Some(input.rel_child.as_str()) {
                return Err(CapabilityError::Forbidden(format!(
                    "work '{}' has no works row in the active workspace",
                    input.work_ref
                )));
            }
            Ok(nexus_contracts::local::orchestration::WorkArtifactCommitOutput { committed: true })
        }
    }

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

        let mut tx = ScaffoldTransaction::new(false);
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

        let mut tx = ScaffoldTransaction::new(false);
        tx.created_files.push(file_path.clone());
        tx.created_dirs.push(work_dir);
        tx.commit();

        drop(tx);

        assert!(file_path.exists(), "file should remain after commit");
    }

    #[tokio::test]
    async fn rollback_preserves_pre_existing_user_content() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("gb-user-data-survival");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let readme_path = work_dir.join("README.md");
        let user_content = "# My Game Bible\n\nPersonal design notes.\n";
        std::fs::write(&readme_path, user_content).expect("write user README");

        let executor = RecordingExecutor::new();
        let mut tx = ScaffoldTransaction::new(false);
        tx.create_dir(&work_dir).unwrap();
        let scaffold_content = "# Overwritten\n\nScaffold template.\n";
        tx.write_file(
            executor.as_ref(),
            tmp.path(),
            "gb-user-data-survival",
            Path::new("README.md"),
            scaffold_content,
        )
        .await
        .expect("write scaffold README");

        drop(tx);

        let restored = std::fs::read_to_string(&readme_path).expect("read restored README");
        assert_eq!(
            restored, user_content,
            "rollback must restore pre-existing user-authored file content"
        );
        assert!(!restored.contains("Overwritten"));
    }

    #[tokio::test]
    async fn game_bible_scaffold_rejects_path_traversal_in_work_ref() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = cap_with_root(root.clone());

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

        let mut tx = ScaffoldTransaction::new(false);
        tx.temp_files.push(tmp_path.clone());

        drop(tx);

        assert!(
            !tmp_path.exists(),
            "temp file must be cleaned up on rollback"
        );
        assert!(!final_path.exists(), "final path must not exist");
    }

    #[tokio::test]
    async fn scaffold_writes_every_file_through_the_port() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let executor = RecordingExecutor::at(tmp.path());
        let cap = GameBibleProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: Some(executor.clone()),
        };

        let out = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_port",
                "work_ref": "gb-port",
                "title": "GB Port",
            }))
            .await
            .expect("scaffold should succeed");

        let commits = executor.commits();
        assert_eq!(
            commits.len(),
            13,
            "README + 12 Design templates must all go through the executor"
        );
        assert_eq!(commits[0].rel_child, "README.md");
        assert!(commits
            .iter()
            .any(|c| c.rel_child == "Design/overview.md"
                && c.content.contains("section_status: draft")));
        assert_eq!(out["files_created"].as_array().unwrap().len(), 13);
    }

    #[tokio::test]
    async fn recording_run_leaves_the_works_tree_identical() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = GameBibleProjectScaffold {
            pool: None,
            works_root: root,
            executor: Some(Arc::new(DryRunExecutor::at(tmp.path().to_path_buf()))),
        };

        let before = tree_snapshot(tmp.path());
        let out = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_recording",
                "work_ref": "recording-gb",
                "title": "Recording Game Bible",
            }))
            .await
            .expect("a recording run must succeed");

        assert_eq!(
            before,
            tree_snapshot(tmp.path()),
            "a recording run must leave the whole Works tree untouched"
        );
        assert_eq!(out["dirs_created"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn recording_run_leaves_the_works_row_untouched() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let pool = pool_with_work(
            tmp.path(),
            "wrk_gb_dry",
            "creator_test",
            Some("kept-ref"),
            Some("novel"),
        )
        .await;
        let cap = GameBibleProjectScaffold {
            pool: Some(pool.clone()),
            works_root: root.clone(),
            executor: Some(Arc::new(DryRunExecutor::at(tmp.path().to_path_buf()))),
        };

        let before_tree = tree_snapshot(&root);
        let out = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_dry",
                "work_ref": "recording-gb",
                "title": "Recording Game Bible",
            }))
            .await
            .expect("a recording run must succeed against a real works row");

        assert_eq!(
            stored_identity(&pool, "wrk_gb_dry").await,
            (Some("kept-ref".to_string()), Some("novel".to_string())),
            "a recording run must neither claim nor publish the works row"
        );
        assert_eq!(
            before_tree,
            tree_snapshot(&root),
            "a recording run must not create the Works tree"
        );
        assert_eq!(
            out["files_created"],
            serde_json::json!([]),
            "a recording run creates no files to report"
        );
        assert_eq!(out["dirs_created"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn failed_recording_run_leaves_no_works_row_or_tree_side_effect() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let pool = pool_with_work(tmp.path(), "wrk_gb_dry_fail", "creator_test", None, None).await;
        let cap = GameBibleProjectScaffold {
            pool: Some(pool.clone()),
            works_root: root.clone(),
            executor: Some(Arc::new(DryRunExecutor::refusing_at(
                tmp.path().to_path_buf(),
                "Design/lore.md",
            ))),
        };

        let before_tree = tree_snapshot(&root);
        let before_row = stored_identity(&pool, "wrk_gb_dry_fail").await;
        let err = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_dry_fail",
                "work_ref": "recording-gb-fail",
                "title": "Recording Game Bible Fail",
            }))
            .await
            .expect_err("a refused later artifact must fail the dry run");

        assert!(
            matches!(err, CapabilityError::Forbidden(_)),
            "expected the port refusal to propagate, got: {err}"
        );
        assert_eq!(
            before_row,
            stored_identity(&pool, "wrk_gb_dry_fail").await,
            "a failed dry run must not write the works row"
        );
        assert_eq!(
            before_tree,
            tree_snapshot(&root),
            "a failed dry run must leave the Works tree untouched"
        );
    }

    #[tokio::test]
    async fn recording_run_still_refuses_an_occupied_directory_path() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let link = root
            .join("recording-gb-occupied")
            .join("Logs")
            .join("design");
        std::fs::create_dir_all(link.parent().expect("link has a parent")).expect("mkdir");
        std::os::unix::fs::symlink(tmp.path().join("absent-gb-dry-target"), &link)
            .expect("symlink");
        let cap = GameBibleProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: Some(Arc::new(DryRunExecutor::at(tmp.path().to_path_buf()))),
        };

        let before_tree = tree_snapshot(&root);
        let err = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_dry_occupied",
                "work_ref": "recording-gb-occupied",
                "title": "Recording Game Bible Occupied",
            }))
            .await
            .expect_err("a recording run must still refuse an occupied directory path");

        assert!(
            matches!(err, CapabilityError::Internal(_)),
            "expected the occupied-directory refusal, got: {err}"
        );
        assert_eq!(
            before_tree,
            tree_snapshot(&root),
            "the refusal must leave the tree untouched"
        );
    }

    #[tokio::test]
    async fn scaffold_refuses_without_workspace_executor() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = GameBibleProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: None,
        };

        let err = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_nowire",
                "work_ref": "gb-nowire",
                "title": "GB No Wire",
            }))
            .await
            .expect_err("a Work byte write without the port must refuse");

        assert!(
            matches!(err, CapabilityError::WorkerUnavailable),
            "expected WorkerUnavailable, got: {err}"
        );
        assert!(
            !root.join("gb-nowire").exists(),
            "no directory may be created"
        );
    }

    #[tokio::test]
    async fn scaffold_rolls_back_when_port_refuses_a_later_file() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let executor = RecordingExecutor::refusing_at(tmp.path(), "Design/lore.md");
        let cap = GameBibleProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: Some(executor.clone()),
        };

        let err = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_rollback",
                "work_ref": "gb-rollback",
                "title": "GB Rollback",
            }))
            .await
            .expect_err("missing work row must refuse the write");
        assert!(
            matches!(err, CapabilityError::Forbidden(_)),
            "expected the port refusal to propagate, got: {err}"
        );

        let work_dir = root.join("gb-rollback");
        assert!(!work_dir.join("README.md").exists(), "README rolled back");
        assert!(
            !work_dir.join("Design/overview.md").exists(),
            "committed templates rolled back"
        );
    }

    #[tokio::test]
    async fn scaffold_keeps_a_preexisting_dangling_symlink_on_rollback() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let link = root.join("gb-symlink").join("README.md");
        std::fs::create_dir_all(link.parent().expect("link has a parent")).expect("mkdir");
        std::os::unix::fs::symlink(tmp.path().join("absent-gb-target.md"), &link).expect("symlink");
        let executor = RecordingExecutor::refusing_at(tmp.path(), "Design/lore.md");
        let cap = GameBibleProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: Some(executor),
        };

        let err = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_symlink",
                "work_ref": "gb-symlink",
                "title": "GB Symlink",
            }))
            .await
            .expect_err("a dangling link must refuse to be read as an original");
        assert!(
            matches!(err, CapabilityError::Internal(_)),
            "expected the original read to fail, got: {err}"
        );

        assert!(
            std::fs::symlink_metadata(&link).is_ok(),
            "rollback unlinked a symlink this run never created"
        );
        assert!(
            std::fs::symlink_metadata(tmp.path().join("absent-gb-target.md")).is_err(),
            "the run wrote through the user's dangling link"
        );
    }

    #[tokio::test]
    async fn scaffold_refuses_a_dangling_symlink_at_a_directory_path() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let link = root.join("gb-dir-symlink").join("Logs").join("design");
        std::fs::create_dir_all(link.parent().expect("link has a parent")).expect("mkdir");
        std::os::unix::fs::symlink(tmp.path().join("absent-gb-dir-target"), &link)
            .expect("symlink");
        let cap = GameBibleProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: Some(RecordingExecutor::at(tmp.path())),
        };

        let err = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_gb_dir_symlink",
                "work_ref": "gb-dir-symlink",
                "title": "GB Dir Symlink",
            }))
            .await
            .expect_err(
                "an occupied non-directory must refuse the scaffold instead of reporting success",
            );
        assert!(
            matches!(err, CapabilityError::Internal(_)),
            "expected the occupied-directory refusal, got: {err}"
        );
        assert!(
            std::fs::symlink_metadata(&link).is_ok(),
            "rollback unlinked a symlink this run never created"
        );
        assert!(
            std::fs::symlink_metadata(tmp.path().join("absent-gb-dir-target")).is_err(),
            "the run created the dangling link's target"
        );
    }
}
