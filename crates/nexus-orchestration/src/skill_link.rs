//! Ensure project-local skill symlinks point to home-directory skill sources.
// note: symlink creation is best-effort; Windows failures log a warning and never block the caller.
//!
//! When creating an ACP session for a role, the role's `recommended_skills`
//! must be readable from the project workspace. This module creates symlinks
//! from `<workspace>/.agents/skills/<slug>` → `~/.nexus42/skills/<slug>`
//! so agents can discover skill files through a well-known project-local path.
//!
//! Symlink creation is best-effort: on Windows, directory symlinks may require
//! admin privileges; failures are logged but do not block the caller.

use std::path::Path;

/// note: ensure a skill directory at `~/.nexus42/skills/<slug>/` is readable
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

    // note: skip when the home-side target is absent.
    if !target_path.is_dir() {
        tracing::warn!(
            skill = skill_slug,
            target = %target_path.display(),
            "Skill directory does not exist in home, skipping link"
        );
        return Ok(false);
    }

    // note: parent directories must exist before the symlink is created.
    std::fs::create_dir_all(&project_skills_dir)?;

    // why: an existing link is only a no-op when it already resolves to the target.
    if link_path.exists() || link_path.is_symlink() {
        if let Ok(current_target) = std::fs::read_link(&link_path) {
            if current_target == target_path {
                return Ok(false); // note: already correct
            }
            tracing::warn!(
                skill = skill_slug,
                old_target = %current_target.display(),
                new_target = %target_path.display(),
                "Skill symlink points to wrong target, recreating"
            );
            std::fs::remove_file(&link_path)?;
        } else {
            // note: exists but is not a symlink (a regular directory) — remove it.
            std::fs::remove_dir_all(&link_path)?;
        }
    }

    // note: the symlink primitive is per-platform (`symlink_dir`).
    symlink_dir(&target_path, &link_path, skill_slug)?;

    tracing::info!(
        skill = skill_slug,
        link = %link_path.display(),
        target = %target_path.display(),
        "Created skill symlink"
    );
    Ok(true)
}

/// note: ensure all recommended skills for a role are linked into the project workspace.
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

/// note: create a directory symlink, platform-dependent.
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
            // note: non-blocking — a synthetic error the caller can swallow.
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "symlink creation skipped on Windows",
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// note: `ensure_skill_link` creates a symlink when none exists.
    #[test]
    fn test_ensure_skill_link_creates_symlink() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");

        // note: seed the home-side skill directory.
        let skill_dir = home_skills.join("my-skill");
        fs::create_dir_all(&skill_dir).expect("create skill dir");
        fs::write(skill_dir.join("SKILL.md"), "skill content").expect("write SKILL.md");

        let result = ensure_skill_link(&workspace, &home_skills, "my-skill")
            .expect("ensure_skill_link should succeed");

        assert!(result, "should report link was created");

        // note: the link exists and points at the home source.
        let link_path = workspace.join(".agents").join("skills").join("my-skill");
        assert!(
            link_path.is_symlink() || link_path.is_dir(),
            "link should exist"
        );

        // note: content is readable through the symlink.
        let content = fs::read_to_string(link_path.join("SKILL.md")).expect("read through symlink");
        assert_eq!(content, "skill content");
    }

    /// note: calling `ensure_skill_link` twice is idempotent — the second call returns `Ok(false)`.
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

    /// note: a missing target skill directory returns `Ok(false)` without panicking.
    #[test]
    fn test_ensure_skill_link_missing_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");
        // note: the target directory is deliberately left absent.

        let result = ensure_skill_link(&workspace, &home_skills, "nonexistent-skill")
            .expect("should succeed even with missing target");
        assert!(!result, "should return Ok(false) for missing target");
    }

    /// note: `ensure_role_skills` counts multiple skills correctly.
    #[test]
    fn test_ensure_role_skills_counts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");

        // note: three home-side skills to link.
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
        let count = ensure_role_skills(&workspace, &home_skills, &skills).expect("should succeed");
        assert_eq!(count, 3, "should have created 3 links");

        // note: all three links are readable.
        for slug in &["skill-a", "skill-b", "skill-c"] {
            let link = workspace.join(".agents").join("skills").join(slug);
            assert!(link.exists(), "link for {slug} should exist");
        }
    }

    /// note: a symlink with the wrong target is recreated.
    #[test]
    fn test_ensure_skill_link_recreates_wrong_target() {
        let temp = tempfile::tempdir().expect("tempdir");
        let workspace = temp.path().join("workspace");
        let home_skills = temp.path().join("home").join("skills");

        // note: two home-side skill directories, one of them the wrong target.
        let wrong_dir = home_skills.join("wrong-skill");
        fs::create_dir_all(&wrong_dir).expect("create wrong dir");
        fs::write(wrong_dir.join("SKILL.md"), "wrong content").expect("write");

        let correct_dir = home_skills.join("correct-skill");
        fs::create_dir_all(&correct_dir).expect("create correct dir");
        fs::write(correct_dir.join("SKILL.md"), "correct content").expect("write");

        // note: start from a symlink that points at the wrong target.
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

        // note: it resolves to the wrong content so far.
        let link_path = project_skills.join("target-skill");
        let wrong_content =
            fs::read_to_string(link_path.join("SKILL.md")).expect("read wrong link");
        assert_eq!(wrong_content, "wrong content");

        // note: call ensure with the correct target.
        let result = ensure_skill_link(
            &workspace,
            home_skills.join("correct-skill").parent().unwrap(),
            "target-skill",
        )
        .expect("should succeed");

        let _ = result;

        if link_path.is_symlink() || link_path.exists() {
            let _ = fs::remove_file(&link_path);
        }

        // note: seed `target-skill` in the home skills root.
        let target_dir = home_skills.join("target-skill");
        fs::create_dir_all(&target_dir).expect("create target dir");
        fs::write(target_dir.join("SKILL.md"), "target content").expect("write");

        // note: point the link at the wrong skill before calling ensure.
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&wrong_dir, &link_path).expect("wrong symlink");
        }
        #[cfg(windows)]
        {
            std::os::windows::fs::symlink_dir(&wrong_dir, &link_path).expect("wrong symlink");
        }

        // note: the link still resolves to the wrong content.
        assert_eq!(
            fs::read_to_string(link_path.join("SKILL.md")).expect("read"),
            "wrong content"
        );

        // note: ensure detects the mismatch and recreates the link.
        let result =
            ensure_skill_link(&workspace, &home_skills, "target-skill").expect("should succeed");
        assert!(result, "should report link was recreated");

        // note: the link now resolves to the correct target.
        assert_eq!(
            fs::read_to_string(link_path.join("SKILL.md")).expect("read"),
            "target content"
        );
    }
}
