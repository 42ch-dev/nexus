//! Ensure project-local skill symlinks point to home-directory skill sources.
//!
//! When creating an ACP session for a role, the role's `recommended_skills`
//! must be readable from the project workspace. This module creates symlinks
//! from `<workspace>/.agents/skills/<slug>` → `~/.nexus42/skills/<slug>`
//! so agents can discover skill files through a well-known project-local path.
//!
//! Symlink creation is best-effort: on Windows, directory symlinks may require
//! admin privileges; failures are logged but do not block the caller.

use std::path::Path;

/// Ensure a skill directory at `~/.nexus42/skills/<slug>/` is readable
/// from the project workspace by creating a symlink at
/// `<workspace>/.agents/skills/<slug>` → the home source.
///
/// - If the symlink already exists and points to the correct target, this is a no-op.
/// - If the symlink exists but points to a different target, remove and recreate it
///   (log warning).
/// - If symlink creation fails (e.g., no permissions on Windows), fall back to a log
///   warning (non-blocking).
///
/// # Returns
///
/// - `Ok(true)` if a symlink was created or updated.
/// - `Ok(false)` if the symlink already existed and was correct, or if the target
///   skill directory does not exist.
///
/// # Errors
///
/// Returns an I/O error if the parent directory cannot be created, an existing
/// stale symlink cannot be removed, or the symlink itself cannot be created.
/// On Windows, symlink creation may fail if the process lacks admin privileges.
pub fn ensure_skill_link(
    workspace_dir: &Path,
    home_skills_dir: &Path,
    skill_slug: &str,
) -> std::io::Result<bool> {
    let project_skills_dir = workspace_dir.join(".agents").join("skills");
    let link_path = project_skills_dir.join(skill_slug);
    let target_path = home_skills_dir.join(skill_slug);

    // Verify the target exists
    if !target_path.is_dir() {
        tracing::warn!(
            skill = skill_slug,
            target = %target_path.display(),
            "Skill directory does not exist in home, skipping link"
        );
        return Ok(false);
    }

    // Create parent directories if needed
    std::fs::create_dir_all(&project_skills_dir)?;

    // Check if symlink already exists and is correct
    if link_path.exists() || link_path.is_symlink() {
        if let Ok(current_target) = std::fs::read_link(&link_path) {
            if current_target == target_path {
                return Ok(false); // Already correct
            }
            tracing::warn!(
                skill = skill_slug,
                old_target = %current_target.display(),
                new_target = %target_path.display(),
                "Skill symlink points to wrong target, recreating"
            );
            std::fs::remove_file(&link_path)?;
        } else {
            // Exists but not a symlink (regular dir?) — remove
            std::fs::remove_dir_all(&link_path)?;
        }
    }

    // Create symlink (platform-dependent)
    symlink_dir(&target_path, &link_path, skill_slug)?;

    tracing::info!(
        skill = skill_slug,
        link = %link_path.display(),
        target = %target_path.display(),
        "Created skill symlink"
    );
    Ok(true)
}

/// Ensure all recommended skills for a role are linked into the project workspace.
///
/// # Returns
///
/// The number of links created or updated.
///
/// # Errors
///
/// Propagates any I/O error from `ensure_skill_link`.
pub fn ensure_role_skills(
    workspace_dir: &Path,
    home_skills_dir: &Path,
    recommended_skills: &[String],
) -> std::io::Result<u32> {
    let mut count = 0u32;
    for skill_slug in recommended_skills {
        if ensure_skill_link(workspace_dir, home_skills_dir, skill_slug)? {
            count += 1;
        }
    }
    Ok(count)
}

// ---------------------------------------------------------------------------
// Platform-specific symlink helper
// ---------------------------------------------------------------------------

/// Create a directory symlink, platform-dependent.
///
/// On failure, logs a warning and returns `Err` — callers decide whether
/// to propagate or swallow.
#[cfg(unix)]
fn symlink_dir(target: &Path, link: &Path, _skill_slug: &str) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, link)
}

#[cfg(windows)]
fn symlink_dir(target: &Path, link: &Path, skill_slug: &str) -> std::io::Result<()> {
    match std::os::windows::fs::symlink_dir(target, link) {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::warn!(
                skill = skill_slug,
                error = %e,
                "Failed to create directory symlink on Windows \
                 (may need admin privileges), skipping"
            );
            // Non-blocking: return a synthetic error the caller can swallow
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "symlink creation skipped on Windows",
            ))
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

    /// Test: `ensure_skill_link` creates a symlink when none exists.
    #[test]
    fn test_ensure_skill_link_creates_symlink() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");

        // Create the target skill directory with content
        let skill_dir = home_skills.join("my-skill");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), "skill content").expect("write SKILL.md");

        let result = ensure_skill_link(&workspace, &home_skills, "my-skill")
            .expect("ensure_skill_link should succeed");

        assert!(result, "should report link was created");

        // Verify symlink exists and points correctly
        let link_path = workspace.join(".agents").join("skills").join("my-skill");
        assert!(link_path.is_symlink() || link_path.is_dir(), "link should exist");

        // Verify content is readable through the symlink
        let content =
            fs::read_to_string(link_path.join("SKILL.md")).expect("read through symlink");
        assert_eq!(content, "skill content");
    }

    /// Test: calling `ensure_skill_link` twice is idempotent — second call returns Ok(false).
    #[test]
    fn test_ensure_skill_link_idempotent() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");

        let skill_dir = home_skills.join("my-skill");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), "skill content").expect("write SKILL.md");

        let first = ensure_skill_link(&workspace, &home_skills, "my-skill")
            .expect("first call should succeed");
        assert!(first, "first call should create link");

        let second = ensure_skill_link(&workspace, &home_skills, "my-skill")
            .expect("second call should succeed");
        assert!(
            !second,
            "second call should return Ok(false) — already correct"
        );
    }

    /// Test: missing target skill directory returns Ok(false) without panic.
    #[test]
    fn test_ensure_skill_link_missing_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");
        // Do NOT create the target directory

        let result = ensure_skill_link(&workspace, &home_skills, "nonexistent-skill")
            .expect("should succeed even with missing target");
        assert!(!result, "should return Ok(false) for missing target");
    }

    /// Test: `ensure_role_skills` counts multiple skills correctly.
    #[test]
    fn test_ensure_role_skills_counts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");

        // Create three skill directories
        for slug in &["skill-a", "skill-b", "skill-c"] {
            let dir = home_skills.join(slug);
            fs::create_dir_all(&dir).expect("create skill dir");
            fs::write(dir.join("SKILL.md"), format!("content for {slug}")).expect("write SKILL.md");
        }

        let skills = vec![
            "skill-a".to_string(),
            "skill-b".to_string(),
            "skill-c".to_string(),
        ];
        let count =
            ensure_role_skills(&workspace, &home_skills, &skills).expect("should succeed");
        assert_eq!(count, 3, "should have created 3 links");

        // Verify all three are readable
        for slug in &["skill-a", "skill-b", "skill-c"] {
            let link = workspace.join(".agents").join("skills").join(slug);
            assert!(link.exists(), "link for {slug} should exist");
        }
    }

    /// Test: symlink with wrong target is recreated.
    #[test]
    fn test_ensure_skill_link_recreates_wrong_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");

        // Create two skill directories
        let wrong_dir = home_skills.join("wrong-skill");
        fs::create_dir_all(&wrong_dir).expect("create wrong dir");
        fs::write(wrong_dir.join("SKILL.md"), "wrong content").expect("write");

        let correct_dir = home_skills.join("correct-skill");
        fs::create_dir_all(&correct_dir).expect("create correct dir");
        fs::write(correct_dir.join("SKILL.md"), "correct content").expect("write");

        // First, create a symlink to the wrong target
        let project_skills = workspace.join(".agents").join("skills");
        fs::create_dir_all(&project_skills).expect("create project skills dir");
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&wrong_dir, project_skills.join("target-skill"))
                .expect("create wrong symlink");
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(&wrong_dir, project_skills.join("target-skill"))
                .expect("create wrong symlink");
        }

        // Verify it points to wrong target
        let link_path = project_skills.join("target-skill");
        let wrong_content =
            fs::read_to_string(link_path.join("SKILL.md")).expect("read wrong link");
        assert_eq!(wrong_content, "wrong content");

        // Now call ensure_skill_link with the correct target
        let result = ensure_skill_link(&workspace, &home_skills.join("correct-skill").parent().unwrap(), "target-skill")
            .expect("should succeed");

        // On Unix this should recreate; the target dir is home_skills/correct-skill
        // but we passed home_skills as the base, so it looks for home_skills/target-skill
        // which doesn't exist — so it returns Ok(false). Let me fix this test.
        // Actually, let's just test with a simpler approach: create a symlink manually
        // pointing to wrong-skill, then call ensure with home_skills pointing to a dir
        // that has target-skill as a subdir.

        // Better approach: use the same home_skills_dir but ensure "target-skill" exists
        // there and the initial symlink points to something else.
        let _ = result; // We'll redo the test properly below.

        // Clean up and redo
        if link_path.is_symlink() || link_path.exists() {
            let _ = fs::remove_file(&link_path);
        }

        // Create "target-skill" in home_skills
        let target_dir = home_skills.join("target-skill");
        fs::create_dir_all(&target_dir).expect("create target dir");
        fs::write(target_dir.join("SKILL.md"), "target content").expect("write");

        // Create symlink pointing to wrong location
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&wrong_dir, &link_path).expect("wrong symlink");
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(&wrong_dir, &link_path).expect("wrong symlink");
        }

        // Verify wrong content
        assert_eq!(
            fs::read_to_string(link_path.join("SKILL.md")).expect("read"),
            "wrong content"
        );

        // Call ensure — should detect mismatch and recreate
        let result =
            ensure_skill_link(&workspace, &home_skills, "target-skill").expect("should succeed");
        assert!(result, "should report link was recreated");

        // Verify symlink now points to correct target
        assert_eq!(
            fs::read_to_string(link_path.join("SKILL.md")).expect("read"),
            "target content"
        );
    }
}
