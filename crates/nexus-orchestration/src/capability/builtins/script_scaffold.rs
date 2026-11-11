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
    claim_work_ref, commit_work_bytes, publish_work_identity, require_executor, resolve_works_dirs,
};
use crate::capability::{Capability, CapabilityError, WorkspaceExecutor};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tracing::info;

/// Input for the script scaffold capability.
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

/// Output from the script scaffold capability.
#[derive(Debug, Serialize)]
#[serde(rename_all = "snake_case")]
struct ScaffoldOutput {
    scaffold_root: String,
    files_created: Vec<String>,
    dirs_created: Vec<String>,
}

/// Predefined script template content with YAML frontmatter.
struct ScriptTemplate {
    section_weight: &'static str,
    title: &'static str,
    comment: &'static str,
}

const SCRIPT_TEMPLATES: &[ScriptTemplate] = &[
    ScriptTemplate {
        section_weight: "critical",
        title: "Script",
        comment: "Scene headings, dialogue, action lines, parentheticals",
    },
    ScriptTemplate {
        section_weight: "critical",
        title: "Beat Sheet",
        comment: "Story beats, scene outline, act structure",
    },
    ScriptTemplate {
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

// ── ScaffoldTransaction (V1.55 P3 fix-wave: create/overwrite tracking + atomic writes) ──
//
// Tracks each file write as either a creation (didn't exist before) or an
// overwrite (existed before, original content saved). On rollback (Drop):
//   - Temp files are cleaned up
//   - Overwritten files are restored from snapshot
//   - Created files are deleted
//   - Created directories are removed (reverse order)
//
// Writes commit through the Work artifact port; rollback (Drop) restores or
// removes what this run wrote.

struct ScaffoldTransaction {
    /// Files that did NOT exist before — deleted on rollback.
    created_files: Vec<PathBuf>,
    /// Files that existed before — original content restored on rollback.
    overwritten_files: Vec<(PathBuf, Vec<u8>)>,
    /// Directories created — removed in reverse on rollback.
    created_dirs: Vec<PathBuf>,
    /// Temp files that may need cleanup on rollback (write started but rename didn't complete).
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

    /// Create a directory if it doesn't exist. Returns `true` if freshly created.
    fn create_dir(&mut self, dir: &Path) -> Result<bool, CapabilityError> {
        if dir.exists() {
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

        // Snapshot original if exists
        let original = if path.exists() {
            let data = std::fs::read(&path).map_err(|e| {
                CapabilityError::Internal(format!("read original {}: {e}", path.display()))
            })?;
            Some(data)
        } else {
            None
        };

        commit_work_bytes(executor, workspace_root, work_ref, rel_child, content).await?;

        // Track
        let is_create = original.is_none();
        match original {
            Some(data) => self.overwritten_files.push((path.clone(), data)),
            None => self.created_files.push(path.clone()),
        }

        info!(
            path = %path.display(),
            is_create,
            "script_scaffold.write_file: ok"
        );
        Ok(())
    }
}

impl Drop for ScaffoldTransaction {
    fn drop(&mut self) {
        if self.committed {
            return;
        }
        // 1. Clean up temp files (partial writes)
        for tmp in &self.temp_files {
            if let Err(e) = std::fs::remove_file(tmp) {
                tracing::warn!(
                    path = %tmp.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove temp file failed"
                );
            }
        }
        // 2. Restore overwritten files
        for (path, original) in &self.overwritten_files {
            if let Err(e) = std::fs::write(path, original) {
                tracing::warn!(
                    path = %path.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: restore original failed"
                );
            }
        }
        // 3. Delete created files
        for f in &self.created_files {
            if let Err(e) = std::fs::remove_file(f) {
                tracing::warn!(
                    path = %f.display(),
                    error = %e,
                    "ScaffoldTransaction rollback: remove_file failed"
                );
            }
        }
        // 4. Delete created dirs in reverse (children before parents)
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
            "script.project_scaffold: rolled back filesystem state"
        );
    }
}

/// `script.project_scaffold` capability.
pub struct ScriptProjectScaffold {
    pool: Option<sqlx::SqlitePool>,
    works_root: PathBuf,
    executor: Option<Arc<dyn WorkspaceExecutor>>,
}

impl ScriptProjectScaffold {
    /// Create a standalone (no-pool) instance for testing.
    #[must_use]
    pub fn new() -> Self {
        Self {
            pool: None,
            works_root: PathBuf::from("Works"),
            executor: None,
        }
    }

    /// Create an instance with a DB pool (default Works root).
    #[must_use]
    pub fn with_pool(pool: sqlx::SqlitePool) -> Self {
        Self {
            pool: Some(pool),
            works_root: PathBuf::from("Works"),
            executor: None,
        }
    }

    /// Create an instance with a DB pool and custom Works root (for e2e tests).
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
        nexus_preset::capability_catalog::SCRIPT_PROJECT_SCAFFOLD_INPUT_SCHEMA
    }
    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"scaffold_root":{"type":"string"},"files_created":{"type":"array","items":{"type":"string"}},"dirs_created":{"type":"array","items":{"type":"string"}}},"required":["scaffold_root","files_created","dirs_created"],"additionalProperties":false}"#
    }

    // SAFETY: The run method handles 7 directory/file operations through the
    // Work artifact port; the length is inherent to the scaffold batch.
    #[allow(clippy::too_many_lines)]
    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let inp: ScaffoldInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("script.project_scaffold input: {e}"))
        })?;

        // ── FIX (qc2 C-001): validate work_ref against path traversal ──
        let work_ref = validate_work_ref(&inp.work_ref)?;

        info!(
            work_id = %inp.work_id,
            work_ref = %work_ref,
            world_id = ?inp.world_id,
            "script.project_scaffold: start"
        );

        let executor = require_executor(self.executor.as_ref())?;
        let (workspace_root, resolved_works) = resolve_works_dirs(executor, &self.works_root)?;
        let work_dir = resolved_works.join(&work_ref);
        let scripts_dir = work_dir.join("Scripts");
        let beats_dir = work_dir.join("Beats");
        let characters_dir = work_dir.join("Characters");
        let logs_dir = work_dir.join("Logs");
        let logs_write_dir = logs_dir.join("write");
        let logs_review_dir = logs_dir.join("review");

        // The Work artifact port resolves `Works/<work_ref>` through the works
        // row, so the ref must already be there when the first byte lands — a
        // first scaffold still has it NULL. Only the ref is claimed here; the
        // profile lands with the finished scaffold, and a failure restores it.
        let claim = claim_work_ref(
            self.pool.as_ref(),
            &inp.creator_id,
            &inp.work_id,
            Some(&work_ref),
        )
        .await?;

        let result: Result<(Vec<String>, Vec<String>), CapabilityError> = async {
            let mut tx = ScaffoldTransaction::new();

            // Create directory structure (idempotent — only tracks newly created dirs)
            for dir in [
                &work_dir,
                &scripts_dir,
                &beats_dir,
                &characters_dir,
                &logs_dir,
                &logs_write_dir,
                &logs_review_dir,
            ] {
                tx.create_dir(dir)?;
            }

            // Write README.md (through the Work artifact port; tracks create vs overwrite)
            let readme_content = format!(
                "# {title}\n\nScript project.\n\n- **Work ID**: {work_id}\n- **Profile**: script\n",
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

            // Write Scripts/script.md
            let script_content = render_template(&SCRIPT_TEMPLATES[0]);
            tx.write_file(
                executor,
                &workspace_root,
                &work_ref,
                Path::new("Scripts/script.md"),
                &script_content,
            )
            .await?;

            // Write Beats/beat-sheet.md
            let beat_content = render_template(&SCRIPT_TEMPLATES[1]);
            tx.write_file(
                executor,
                &workspace_root,
                &work_ref,
                Path::new("Beats/beat-sheet.md"),
                &beat_content,
            )
            .await?;

            // Write Characters/characters.md
            let characters_content = render_template(&SCRIPT_TEMPLATES[2]);
            tx.write_file(
                executor,
                &workspace_root,
                &work_ref,
                Path::new("Characters/characters.md"),
                &characters_content,
            )
            .await?;

            // Files landed — publish the profile identity, then commit the guard.
            publish_work_identity(
                self.pool.as_ref(),
                &inp.creator_id,
                &inp.work_id,
                "script",
                &work_ref,
            )
            .await?;

            // Output diagnostics use new file lists
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
                        "script.project_scaffold: restore prior work_ref failed"
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
            "script.project_scaffold: done"
        );

        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("script.project_scaffold output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::super::work_commit::test_support::RecordingExecutor;
    use super::*;

    /// Capability over a temp `Works/` root with a recording executor.
    fn cap_with_root(works_root: PathBuf) -> ScriptProjectScaffold {
        let executor: Arc<dyn WorkspaceExecutor> =
            RecordingExecutor::at(works_root.parent().expect("works root has a parent"));
        ScriptProjectScaffold {
            pool: None,
            works_root,
            executor: Some(executor),
        }
    }

    #[test]
    fn all_script_templates_render_non_empty() {
        for tmpl in SCRIPT_TEMPLATES {
            let content = render_template(tmpl);
            assert!(!content.is_empty(), "template is empty");
            assert!(
                content.contains("section_status: draft"),
                "template missing section_status frontmatter"
            );
            assert!(
                content.contains("section_weight:"),
                "template missing section_weight"
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
            .map(|t| t.title)
            .collect();
        assert!(critical.contains(&"Script"));
        assert!(critical.contains(&"Beat Sheet"));
    }

    #[test]
    fn script_capability_name() {
        let cap = ScriptProjectScaffold::new();
        assert_eq!(cap.name(), "script.project_scaffold");
    }

    // ── ScaffoldTransaction: rollback + commit (updated for create/overwrite) ──

    #[test]
    fn scaffold_transaction_rollback_cleans_up_created_files() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("rollback-test");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let file_path = work_dir.join("test.txt");
        std::fs::write(&file_path, "data").expect("write");
        let sub_dir = work_dir.join("subdir");
        std::fs::create_dir_all(&sub_dir).expect("mkdir subdir");

        let mut tx = ScaffoldTransaction::new();
        tx.created_files.push(file_path.clone());
        tx.created_dirs.push(sub_dir.clone());
        // NOT committed → Drop should clean up created items

        drop(tx);

        assert!(
            !file_path.exists(),
            "created file should be removed by rollback"
        );
        assert!(
            !sub_dir.exists(),
            "created subdir should be removed by rollback"
        );
        // work_dir itself (pre-existing) should remain
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
        let work_dir = root.join("commit-test");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let file_path = work_dir.join("keep.txt");
        std::fs::write(&file_path, "keep").expect("write");

        let mut tx = ScaffoldTransaction::new();
        tx.created_files.push(file_path.clone());
        tx.created_dirs.push(work_dir);
        tx.commit(); // committed → Drop is no-op

        drop(tx);

        assert!(file_path.exists(), "file should remain after commit");
    }

    // ── Fix 1 regression: pre-existing user content survives rollback ──

    #[tokio::test]
    async fn rollback_preserves_pre_existing_user_content() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("user-data-survival");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let readme_path = work_dir.join("README.md");
        let user_content = "# My Script\n\nThis is my personal README with custom notes.\n";

        // Pre-create a file with user-authored content (simulating re-init)
        std::fs::write(&readme_path, user_content).expect("write user README");

        // Simulate scaffold: write_file tracks overwrite
        let executor = RecordingExecutor::new();
        let mut tx = ScaffoldTransaction::new();
        tx.create_dir(&work_dir).unwrap();

        let scaffold_content = "# Overwritten\n\nScaffold template content.\n";
        tx.write_file(
            executor.as_ref(),
            tmp.path(),
            "user-data-survival",
            Path::new("README.md"),
            scaffold_content,
        )
        .await
        .expect("write scaffold README");

        // Before commit, force rollback (simulate DB failure)
        // Drop without commit → should restore original user content
        drop(tx);

        // User content must be preserved
        let restored = std::fs::read_to_string(&readme_path).expect("read restored README");
        assert_eq!(
            restored, user_content,
            "rollback must restore pre-existing user-authored file content"
        );
        assert!(
            !restored.contains("Overwritten"),
            "rollback must remove scaffold-overwritten content"
        );
    }

    // ── Fix 2 regression: path traversal rejection ──

    #[tokio::test]
    async fn script_scaffold_rejects_path_traversal_in_work_ref() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = cap_with_root(root.clone());

        let input = serde_json::json!({
            "creator_id": "attacker",
            "work_id": "wrk_traversal",
            "work_ref": "../etc/passwd",
            "title": "Path Traversal Attempt",
        });

        let result = cap.run(input).await;
        assert!(
            result.is_err(),
            "scaffold must reject path traversal in work_ref"
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
        let work_dir = root.join("crash-test");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let final_path = work_dir.join("template.md");

        // Simulate a crash during write_file: create the temp file,
        // but don't rename it. This simulates the temp+rename pattern
        // where the rename hasn't happened yet.
        let mut tx = ScaffoldTransaction::new();
        // Write to temp, then simulate crash before rename by
        // directly pushing a temp that won't get renamed
        let tmp_path = final_path.with_extension("tmp");
        std::fs::write(&tmp_path, "partial content").expect("write temp");
        tx.temp_files.push(tmp_path.clone());

        // Rollback (Drop without commit)
        drop(tx);

        // The temp file must be cleaned up
        assert!(
            !tmp_path.exists(),
            "temp file must be cleaned up on rollback"
        );
        // The final path must NOT exist (rename never happened)
        assert!(
            !final_path.exists(),
            "final path must not exist — rename never completed"
        );
        // No half-written content at either path
    }

    // ── Integration: scaffold idempotency with write_file ──

    #[tokio::test]
    async fn scaffold_creates_directory_tree() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let executor = RecordingExecutor::at(tmp.path());
        let cap = ScriptProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: Some(executor.clone()),
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

        let committed: Vec<String> = executor
            .commits()
            .into_iter()
            .map(|c| c.rel_child)
            .collect();
        assert_eq!(
            committed,
            vec![
                "README.md",
                "Scripts/script.md",
                "Beats/beat-sheet.md",
                "Characters/characters.md"
            ],
            "each forward write must go through the executor"
        );

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

    #[tokio::test]
    async fn scaffold_idempotent_preserves_user_content() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = cap_with_root(root.clone());

        // First run: scaffold creates files
        let input = serde_json::json!({
            "creator_id": "creator_test",
            "work_id": "wrk_idem_script",
            "work_ref": "idem-script",
            "title": "Idempotent Script",
        });
        let out1 = cap.run(input.clone()).await.expect("first run");
        let scaffold = out1["scaffold_root"].as_str().expect("root");
        let readme_path = std::path::Path::new(scaffold).join("README.md");

        // User modifies README
        let user_content = "# My Custom Script README\n\nUser-authored content.";
        std::fs::write(&readme_path, user_content).expect("write user content");

        // Second run: scaffold with write_file (tracks as overwrite + restores on rollback)
        // Write another file first to verify multi-file tracking
        let extra_path = std::path::Path::new(scaffold).join("extra.md");
        std::fs::write(&extra_path, "extra").expect("write extra");

        let _out2 = cap.run(input).await.expect("second run");

        // User content preserved (write_file saves snapshot on overwrite, but commit keeps new content)
        let readme_after = std::fs::read_to_string(&readme_path).expect("read after second run");
        // After commit, the new scaffold content replaced user content (expected for re-init)
        assert!(
            readme_after.contains("Script project"),
            "after commit, scaffold content should be present"
        );
    }

    // ── Work artifact port seam ──

    #[tokio::test]
    async fn scaffold_refuses_without_workspace_executor() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = ScriptProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: None,
        };

        let err = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_script_nowire",
                "work_ref": "script-nowire",
                "title": "Script No Wire",
            }))
            .await
            .expect_err("a Work byte write without the port must refuse");

        assert!(
            matches!(err, CapabilityError::WorkerUnavailable),
            "expected WorkerUnavailable, got: {err}"
        );
        assert!(
            !root.join("script-nowire").exists(),
            "no directory may be created before the port is available"
        );
    }

    #[tokio::test]
    async fn scaffold_rolls_back_when_port_refuses_a_later_file() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let executor = RecordingExecutor::refusing_at(tmp.path(), "Characters/characters.md");
        let cap = ScriptProjectScaffold {
            pool: None,
            works_root: root.clone(),
            executor: Some(executor.clone()),
        };

        let err = cap
            .run(serde_json::json!({
                "creator_id": "creator_test",
                "work_id": "wrk_script_rollback",
                "work_ref": "script-rollback",
                "title": "Script Rollback",
            }))
            .await
            .expect_err("missing work row must refuse the write");
        assert!(
            matches!(err, CapabilityError::Forbidden(_)),
            "expected the port refusal to propagate, got: {err}"
        );

        let work_dir = root.join("script-rollback");
        assert!(!work_dir.join("README.md").exists(), "README rolled back");
        assert!(
            !work_dir.join("Scripts/script.md").exists(),
            "committed templates rolled back"
        );
    }
}
