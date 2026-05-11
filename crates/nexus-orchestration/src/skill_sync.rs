//! Skill install/upgrade sync logic.
//!
//! Provides idempotent sync from compile-time embedded skills to the user's
//! local `$HOME/.nexus42/skills/` directory.  User-modified files are never
//! overwritten — conflicts are reported instead.
//!
//! # Layout
//!
//! ```text
//! <nexus_home>/skills/
//! ├── <skill-id>/
//! │   └── SKILL.md
//! └── ...
//! ```
//!
//! See `embedded_skills` module for the compile-time source of truth.

use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

use tracing;

use crate::embedded_skills;

/// Filename for the primary skill document within each skill directory.
const SKILL_MD_FILENAME: &str = "SKILL.md";

// ---------------------------------------------------------------------------
// Types
// ---------------------------------------------------------------------------

/// Summary of a skill sync operation.
#[derive(Debug, Default)]
pub struct SkillSyncResult {
    /// Skill IDs that were newly installed (did not exist on disk).
    pub installed: Vec<String>,
    /// Skill IDs whose on-disk content matched the embedded version (no-op).
    pub skipped: Vec<String>,
    /// User-modified files that were **not** overwritten.
    pub conflicts: Vec<SkillConflict>,
    /// Skill IDs present on disk but no longer bundled in the binary.
    pub removed_from_embedded: Vec<String>,
}

/// A single conflict where an on-disk file differs from the embedded version.
#[derive(Debug)]
pub struct SkillConflict {
    /// Skill identifier.
    pub skill_id: String,
    /// Path to the conflicting file on disk.
    pub path: PathBuf,
    /// Human-readable reason for the conflict.
    pub reason: String,
}

/// Errors that can occur during skill sync.
#[derive(Debug, thiserror::Error)]
pub enum SkillSyncError {
    /// Failed to create the skills directory (or its parents).
    #[error("failed to create skills directory: {0}")]
    CreateDir(#[from] std::io::Error),
    /// Failed to write or read a skill file on disk.
    #[error("failed to write skill file: {path}: {error}")]
    WriteFile {
        /// Path that triggered the error.
        path: PathBuf,
        /// Underlying I/O error.
        error: std::io::Error,
    },
    /// Failed to read from the embedded skill data source.
    #[error("failed to read embedded skill: {0}")]
    ReadEmbedded(String),
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Synchronise embedded skills to the user's local filesystem.
///
/// For each skill in the embedded manifest:
///
/// - If the destination file does not exist → install it.
/// - If the destination file has identical content → skip (no-op).
/// - If the destination file has **different** content → record a conflict,
///   do **not** overwrite.
///
/// After processing embedded skills, the function scans the on-disk skills
/// directory for skill IDs that no longer exist in the embedded manifest and
/// reports them as `removed_from_embedded` (files are **not** deleted).
///
/// # Errors
///
/// Returns `SkillSyncError` if the skills directory cannot be created or if
/// a file write fails.
pub fn sync_embedded_skills(nexus_home: &Path) -> Result<SkillSyncResult, SkillSyncError> {
    let skills_dir = nexus_home.join("skills");

    // Ensure the top-level skills directory exists.
    fs::create_dir_all(&skills_dir)?;

    let embedded = embedded_skills::list_embedded_skills();
    let embedded_ids: HashSet<&str> = embedded.iter().map(|s| s.id.as_str()).collect();

    let mut result = SkillSyncResult::default();

    for skill in &embedded {
        let dest_dir = skills_dir.join(&skill.id);
        let dest_file = dest_dir.join(SKILL_MD_FILENAME);
        let embedded_bytes = skill.content.as_bytes();

        if dest_file.exists() {
            // Read the on-disk content for comparison.
            let existing_bytes =
                fs::read(&dest_file).map_err(|error| SkillSyncError::WriteFile {
                    path: dest_file.clone(),
                    error,
                })?;

            if existing_bytes == embedded_bytes {
                tracing::debug!(skill_id = %skill.id, "skill unchanged, skipping");
                result.skipped.push(skill.id.clone());
            } else {
                tracing::warn!(
                    skill_id = %skill.id,
                    path = %dest_file.display(),
                    "skill file differs from embedded version, not overwriting"
                );
                result.conflicts.push(SkillConflict {
                    skill_id: skill.id.clone(),
                    path: dest_file,
                    reason: "content differs from embedded version".to_string(),
                });
            }
        } else {
            // New skill — install.
            fs::create_dir_all(&dest_dir)?;
            fs::write(&dest_file, embedded_bytes).map_err(|error| SkillSyncError::WriteFile {
                path: dest_file.clone(),
                error,
            })?;

            tracing::info!(skill_id = %skill.id, "installed embedded skill");
            result.installed.push(skill.id.clone());
        }
    }

    // Detect skills on disk that are no longer embedded.
    detect_removed_skills(&skills_dir, &embedded_ids, &mut result);

    Ok(result)
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Scan `skills_dir` for subdirectories whose names do not appear in
/// `embedded_ids` and append them to `result.removed_from_embedded`.
fn detect_removed_skills(
    skills_dir: &Path,
    embedded_ids: &HashSet<&str>,
    result: &mut SkillSyncResult,
) {
    let Ok(entries) = fs::read_dir(skills_dir) else {
        return;
    };

    for entry in entries.flatten() {
        if !entry.path().is_dir() {
            continue;
        }
        let file_name = entry.file_name();
        let Some(name) = file_name.to_str() else {
            continue;
        };
        if !embedded_ids.contains(name) {
            tracing::info!(skill_id = name, "skill on disk but no longer embedded");
            result.removed_from_embedded.push(name.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Helper: run sync and return the result (panics on error).
    fn sync_ok(temp: &tempfile::TempDir) -> SkillSyncResult {
        sync_embedded_skills(temp.path()).expect("sync should succeed")
    }

    /// Helper: resolve a skill's SKILL.md path under the temp dir.
    fn skill_md_path(temp: &tempfile::TempDir, skill_id: &str) -> PathBuf {
        temp.path()
            .join("skills")
            .join(skill_id)
            .join(SKILL_MD_FILENAME)
    }

    // ----- Test 1: sync to empty dir installs all embedded skills -----

    #[test]
    fn sync_to_empty_dir_installs_all_embedded_skills() {
        let temp = tempfile::tempdir().expect("tempdir");

        let result = sync_ok(&temp);

        // Should have installed at least one skill (novel-writing-assistant).
        assert!(
            !result.installed.is_empty(),
            "expected at least one installed skill"
        );
        assert!(result.skipped.is_empty(), "nothing to skip on first run");
        assert!(result.conflicts.is_empty(), "no conflicts on first run");
        assert!(
            result.removed_from_embedded.is_empty(),
            "no removed skills on first run"
        );

        // Verify the novel-writing-assistant skill file exists and has content.
        let path = skill_md_path(&temp, "novel-writing-assistant");
        assert!(path.exists(), "SKILL.md should exist after sync");
        let content = fs::read_to_string(&path).expect("read SKILL.md");
        assert!(!content.is_empty(), "SKILL.md should have content");

        // Verify the embedded content matches what was written.
        let embedded = embedded_skills::list_embedded_skills();
        let nwa = embedded
            .iter()
            .find(|s| s.id == "novel-writing-assistant")
            .expect("must exist");
        assert_eq!(content, nwa.content, "written content must match embedded");
    }

    // ----- Test 2: idempotent sync — second run produces no changes -----

    #[test]
    fn idempotent_sync_no_changes_on_second_run() {
        let temp = tempfile::tempdir().expect("tempdir");

        // First run: installs everything.
        let first = sync_ok(&temp);
        assert!(!first.installed.is_empty());

        // Second run: should skip everything.
        let second = sync_ok(&temp);
        assert!(
            second.installed.is_empty(),
            "nothing new to install on second run"
        );
        assert!(
            !second.skipped.is_empty(),
            "all previously installed skills should be skipped"
        );
        assert!(second.conflicts.is_empty(), "no conflicts expected");

        // Skipped count should equal first-run installed count.
        assert_eq!(
            second.skipped.len(),
            first.installed.len(),
            "second-run skipped count should match first-run installed count"
        );
    }

    // ----- Test 3: conflict detection preserves user files -----

    #[test]
    fn conflict_detection_preserves_user_modified_files() {
        let temp = tempfile::tempdir().expect("tempdir");

        // First run: install all skills.
        let first = sync_ok(&temp);
        assert!(!first.installed.is_empty());

        // Pick the first installed skill and modify its file.
        let modified_skill_id = &first.installed[0];
        let path = skill_md_path(&temp, modified_skill_id);
        let user_content = "USER MODIFIED CONTENT\n";
        fs::write(&path, user_content).expect("write user content");

        // Second run: should detect conflict.
        let second = sync_ok(&temp);

        assert!(second.installed.is_empty(), "no new installs expected");
        assert!(
            second.skipped.is_empty() || !second.skipped.contains(modified_skill_id),
            "modified skill should NOT be in skipped list"
        );

        // The modified skill should appear in conflicts.
        let conflict = second
            .conflicts
            .iter()
            .find(|c| c.skill_id == *modified_skill_id);
        assert!(
            conflict.is_some(),
            "modified skill should appear in conflicts"
        );
        let conflict = conflict.expect("just checked");
        assert_eq!(conflict.path, path);
        assert!(
            conflict.reason.contains("differs"),
            "conflict reason should mention differing content"
        );

        // Verify the file was NOT overwritten — user content preserved.
        let on_disk = fs::read_to_string(&path).expect("read file");
        assert_eq!(on_disk, user_content, "user content must be preserved");
    }

    // ----- Test 4: removed_from_embedded detection -----

    #[test]
    fn detects_skills_on_disk_not_in_embedded() {
        let temp = tempfile::tempdir().expect("tempdir");

        // Create a fake skill directory that doesn't correspond to any embedded skill.
        let fake_dir = temp.path().join("skills").join("fake-removed-skill");
        fs::create_dir_all(&fake_dir).expect("create fake dir");
        fs::write(fake_dir.join(SKILL_MD_FILENAME), "fake content").expect("write fake file");

        // Run sync.
        let result = sync_ok(&temp);

        assert!(
            result
                .removed_from_embedded
                .contains(&"fake-removed-skill".to_string()),
            "fake skill should appear in removed_from_embedded"
        );

        // The fake skill directory should still exist (not deleted).
        assert!(
            fake_dir.exists(),
            "removed skill directory should NOT be deleted"
        );
    }

    // ----- Test 5: error case — read-only destination directory -----

    #[test]
    fn read_only_directory_returns_error() {
        let temp = tempfile::tempdir().expect("tempdir");

        // Create the skills dir and then make it read-only.
        let skills_dir = temp.path().join("skills");
        fs::create_dir_all(&skills_dir).expect("create skills dir");

        // On Unix, set the directory to read-only (no write permission).
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&skills_dir).expect("metadata").permissions();
            perms.set_mode(0o555); // r-xr-xr-x
            fs::set_permissions(&skills_dir, perms).expect("set permissions");
        }

        // Write a file to make a subdirectory for the skill to "exist".
        // Actually we need the sync to try to CREATE a subdirectory inside
        // the read-only skills dir.  Since the skills dir is read-only,
        // `create_dir_all` for a new skill subdirectory should fail.
        let result = sync_embedded_skills(temp.path());

        #[cfg(unix)]
        {
            // On Unix, we expect an error because the skills dir is read-only
            // and creating subdirectories inside it will fail.
            assert!(result.is_err(), "sync should fail on read-only directory");

            // Restore permissions so the temp dir can be cleaned up.
            use std::os::unix::fs::PermissionsExt;
            let mut perms = fs::metadata(&skills_dir).expect("metadata").permissions();
            perms.set_mode(0o755);
            let _ = fs::set_permissions(&skills_dir, perms);
        }

        // On non-Unix (Windows), permissions work differently — skip assertion.
        #[cfg(not(unix))]
        {
            let _ = result; // avoid unused variable warning
        }
    }
}
