//! `essay.project_scaffold` capability.
//!
//! V1.52 T-A P2: first non-novel profile scaffold.
//! V1.63 P0: upgraded with `ScaffoldTransaction` + `validate_work_ref`.
//! Creates `Works/<work_ref>/Outlines/outline.md` and `Drafts/draft.md`
//! from embedded templates, a README.md, and `PATCH`es the works row
//! to set `work_profile = 'essay'` and `work_ref`.
//!
//! # Concurrency note
//!
//! This capability runs in the single-user daemon process. We assume:
//! 1. Only one `essay.project_scaffold` invocation per `(creator_id, work_id)`
//!    is in flight at any time.
//! 2. No external process is mutating `Works/<work_ref>/` while this runs.
//!
//! # `ScaffoldTransaction` (V1.63 P0)
//!
//! Wraps FS writes + DB PATCH in a `ScaffoldTransaction` with Drop-based FS
//! rollback. Tracks create vs overwrite separately: on rollback, created
//! files are deleted and overwritten files are restored from snapshot.
//! Writes use temp+rename for atomicity.
//!
//! # Path safety (V1.63 P0)
//!
//! Calls `validate_work_ref` from `novel_scaffold_sanitize` before any
//! path joining, mirroring the game-bible/script scaffold pattern.

use super::novel_scaffold_sanitize::validate_work_ref;
use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use tracing::info;

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

// ── ScaffoldTransaction (V1.63 P0: create/overwrite tracking + atomic writes) ──
//
// Tracks each file write as either a creation (didn't exist before) or an
// overwrite (existed before, original content saved). On rollback (Drop):
//   - Temp files are cleaned up
//   - Overwritten files are restored from snapshot
//   - Created files are deleted
//   - Created directories are removed (reverse order)
//
// Writes use temp-file + rename for atomicity (no half-written file at
// final path after crash).

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

    /// Write a file atomically (temp+rename).
    ///
    /// If the file already exists, its original content is saved so rollback
    /// can restore it. Uses a temp file (`<path>.tmp`) written first, then
    /// atomically renamed to the final path.
    fn write_file(&mut self, path: &Path, content: &str) -> Result<(), CapabilityError> {
        // Snapshot original if exists
        let original = if path.exists() {
            let data = std::fs::read(path).map_err(|e| {
                CapabilityError::Internal(format!("read original {}: {e}", path.display()))
            })?;
            Some(data)
        } else {
            None
        };

        // Write to temp file
        let tmp = path.with_extension("tmp");
        std::fs::write(&tmp, content)
            .map_err(|e| CapabilityError::Internal(format!("write tmp {}: {e}", tmp.display())))?;
        self.temp_files.push(tmp.clone());

        // Atomic rename
        std::fs::rename(&tmp, path).map_err(|e| {
            CapabilityError::Internal(format!(
                "rename {} -> {}: {e}",
                tmp.display(),
                path.display()
            ))
        })?;
        // Remove from temp_files — rename succeeded
        self.temp_files.retain(|t| t != &tmp);

        // Track
        let is_create = original.is_none();
        match original {
            Some(data) => self.overwritten_files.push((path.to_path_buf(), data)),
            None => self.created_files.push(path.to_path_buf()),
        }

        info!(
            path = %path.display(),
            is_create,
            "essay_scaffold.write_file: ok"
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
            "essay.project_scaffold: rolled back filesystem state"
        );
    }
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

    /// Create an instance with a DB pool and custom Works root (for e2e tests).
    #[must_use]
    pub const fn new_with_root(pool: sqlx::SqlitePool, works_root: PathBuf) -> Self {
        Self {
            pool: Some(pool),
            works_root,
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

        // ── FIX (V1.63 P0): validate work_ref against path traversal ──
        let work_ref = validate_work_ref(&inp.work_ref)?;

        info!(
            work_id = %inp.work_id,
            work_ref = %work_ref,
            world_id = ?inp.world_id,
            "essay.project_scaffold: start"
        );

        let work_dir = self.works_root.join(&work_ref);
        let title = inp.title.clone();
        let work_id_for_fs = inp.work_id.clone();

        // FS operations are wrapped in `spawn_blocking` to avoid blocking
        // the tokio worker thread pool. `ScaffoldTransaction` is inherently
        // synchronous (Drop-based rollback cannot be async), so the entire
        // FS batch runs on a blocking thread and returns the guard.
        // If the subsequent DB PATCH fails, the guard's Drop rolls back all
        // FS writes — atomicity is preserved.
        let mut tx = {
            let work_dir = work_dir.clone();
            tokio::task::spawn_blocking(
                move || -> Result<ScaffoldTransaction, CapabilityError> {
                    let outlines_dir = work_dir.join("Outlines");
                    let drafts_dir = work_dir.join("Drafts");
                    let logs_dir = work_dir.join("Logs");
                    let logs_write_dir = logs_dir.join("write");
                    let logs_review_dir = logs_dir.join("review");

                    let mut tx = ScaffoldTransaction::new();

                    // Create directory structure (idempotent — only tracks newly created dirs)
                    for dir in [
                        &work_dir,
                        &outlines_dir,
                        &drafts_dir,
                        &logs_dir,
                        &logs_write_dir,
                        &logs_review_dir,
                    ] {
                        tx.create_dir(dir)?;
                    }

                    // Write README.md (atomic: temp+rename; tracks create vs overwrite)
                    let readme_content = format!(
                        "# {title}\n\nEssay project.\n\n- **Work ID**: {work_id_for_fs}\n- **Profile**: essay\n",
                    );
                    tx.write_file(&work_dir.join("README.md"), &readme_content)?;

                    // Write Outlines/outline.md
                    let outline_content = format!(
                        "---\ntitle: {title}\nstatus: outline\n---\n\n# Thesis\n\n# Audience\n\n# Structure\n\n1. Opening hook\n2. Core argument\n3. Supporting evidence\n4. Counterpoint / nuance\n5. Ending takeaway\n",
                    );
                    tx.write_file(&outlines_dir.join("outline.md"), &outline_content)?;

                    // Write Drafts/draft.md
                    let draft_content = format!(
                        "---\ntitle: {title}\nstatus: draft\nword_count: 0\n---\n\n# {title}\n\nWrite your essay here.\n",
                    );
                    tx.write_file(&drafts_dir.join("draft.md"), &draft_content)?;

                    Ok(tx)
                },
            )
            .await
            .map_err(|e| {
                CapabilityError::Internal(format!("scaffold blocking task panicked: {e}"))
            })??
        };

        // PATCH works row: set work_profile and work_ref
        if let Some(ref pool) = self.pool {
            sqlx::query("UPDATE works SET work_profile = 'essay', work_ref = ? WHERE work_id = ?")
                .bind(&work_ref)
                .bind(&inp.work_id)
                .execute(pool)
                .await
                .map_err(|e| CapabilityError::Internal(format!("patch works row: {e}")))?;
        }

        // All FS + DB writes succeeded — commit the transaction guard
        tx.commit();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn essay_capability_name() {
        let cap = EssayProjectScaffold::new();
        assert_eq!(cap.name(), "essay.project_scaffold");
    }

    // ── ScaffoldTransaction: rollback + commit ──

    #[test]
    fn scaffold_transaction_rollback_cleans_up_created_files() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let work_dir = root.join("rollback-test-essay");
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
        let work_dir = root.join("commit-test-essay");
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
        let work_dir = root.join("essay-user-data-survival");
        std::fs::create_dir_all(&work_dir).expect("mkdir");

        let readme_path = work_dir.join("README.md");
        let user_content = "# My Essay\n\nPersonal notes.\n";
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
    async fn essay_scaffold_rejects_path_traversal_in_work_ref() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = EssayProjectScaffold {
            pool: None,
            works_root: root.clone(),
        };

        let input = serde_json::json!({
            "creator_id": "attacker",
            "work_id": "wrk_essay_traversal",
            "work_ref": "../etc/passwd",
            "title": "Path Traversal",
        });

        let result = cap.run(input).await;
        assert!(
            result.is_err(),
            "essay scaffold must reject path traversal in work_ref"
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
        let work_dir = root.join("essay-crash-test");
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

    // ── Integration: scaffold creates directory tree ──

    #[tokio::test]
    async fn scaffold_creates_directory_tree() {
        use tempfile::TempDir;
        let tmp = TempDir::new().expect("tmpdir");
        let root = tmp.path().join("Works");
        let cap = EssayProjectScaffold {
            pool: None,
            works_root: root.clone(),
        };

        let input = serde_json::json!({
            "creator_id": "creator_test",
            "work_id": "wrk_essay_test",
            "work_ref": "test-essay",
            "title": "Test Essay",
        });

        let out = cap.run(input).await.expect("scaffold should succeed");
        let scaffold = out["scaffold_root"].as_str().expect("scaffold_root");
        let scaffold_path = std::path::Path::new(scaffold);

        assert!(scaffold_path.join("Outlines").is_dir());
        assert!(scaffold_path.join("Drafts").is_dir());
        assert!(scaffold_path.join("Logs").is_dir());
        assert!(scaffold_path.join("Logs/write").is_dir());
        assert!(scaffold_path.join("Logs/review").is_dir());

        assert!(scaffold_path.join("README.md").is_file());
        assert!(scaffold_path.join("Outlines/outline.md").is_file());
        assert!(scaffold_path.join("Drafts/draft.md").is_file());

        // Verify template content
        let outline_content =
            std::fs::read_to_string(scaffold_path.join("Outlines/outline.md")).unwrap();
        assert!(outline_content.contains("status: outline"));
        assert!(outline_content.contains("# Thesis"));

        let draft_content = std::fs::read_to_string(scaffold_path.join("Drafts/draft.md")).unwrap();
        assert!(draft_content.contains("status: draft"));
        assert!(draft_content.contains("word_count: 0"));
    }

    #[tokio::test]
    async fn scaffold_rejects_invalid_input() {
        let cap = EssayProjectScaffold::new();
        let result = cap.run(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
