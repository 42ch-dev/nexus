//! Shared operational path layout (`ADR-014`) under the user home directory.
//!
//! Used by `nexus42` CLI and `nexus42d` so `SQLite` and workspace dirs resolve identically.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

const NEXUS_DIR: &str = ".nexus42";

/// Resolve `~/.nexus42` from the user's home directory.
#[must_use]
pub fn nexus_root_from_home(home: &Path) -> PathBuf {
    home.join(NEXUS_DIR)
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/`
#[must_use]
pub fn creator_workspaces_root(home: &Path, creator_id: &str) -> PathBuf {
    nexus_root_from_home(home)
        .join("creators")
        .join(creator_id)
        .join("workspaces")
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/`
#[must_use]
pub fn operational_workspace_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    creator_workspaces_root(home, creator_id).join(workspace_slug)
}

/// Workspace-local `SQLite`: `.../workspaces/<slug>/state.db`
#[must_use]
pub fn workspace_state_db_path(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    operational_workspace_dir(home, creator_id, workspace_slug).join("state.db")
}

/// Shared global `SQLite`: `$HOME/.nexus42/shared/global_state.db`
#[must_use]
pub fn shared_global_db_path(home: &Path) -> PathBuf {
    nexus_root_from_home(home)
        .join("shared")
        .join("global_state.db")
}

/// `$HOME/.nexus42/skills/` — directory for synced embedded skills.
///
/// See `crates/nexus-orchestration/src/skill_sync.rs` for the sync logic that
/// populates this directory.
#[must_use]
pub fn user_skills_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("skills")
}

/// `$HOME/.nexus42/presets/` — base directory for user-installed presets.
///
/// Each subdirectory under this path is expected to contain a `preset.yaml`.
/// See `crates/nexus-orchestration/src/user_preset_dir.rs` for scanning logic.
/// Directories starting with `_` or `.` are reserved and skipped by the scanner.
#[must_use]
pub fn user_preset_base_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("presets")
}

/// `$HOME/.nexus42/presets/<name>/` — path to a specific user preset bundle.
#[must_use]
pub fn user_preset_bundle_dir(home: &Path, name: &str) -> PathBuf {
    user_preset_base_dir(home).join(name)
}

/// List user preset IDs (directory names containing `preset.yaml`) under the presets directory.
///
/// Returns an empty vector if the presets directory doesn't exist or can't be read.
/// Directories starting with `_` or `.` are skipped.
#[must_use]
pub fn list_user_preset_ids(nexus_home: &Path) -> Vec<String> {
    let user_dir = user_preset_base_dir(nexus_home);

    if !user_dir.exists() {
        return Vec::new();
    }

    let Ok(entries) = std::fs::read_dir(&user_dir) else {
        return Vec::new();
    };

    entries
        .flatten()
        .filter(|e| e.path().is_dir())
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().to_string();
            // Skip system-prefixed and hidden dirs.
            if name.starts_with('_') || name.starts_with('.') {
                return None;
            }
            // Must contain a preset.yaml to be valid.
            if e.path().join("preset.yaml").exists() {
                Some(name)
            } else {
                None
            }
        })
        .collect()
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/kb/`
///
/// Knowledge base directory for a workspace (ADR-014 layout, flat files + JSON index).
#[must_use]
pub fn creator_kb_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    operational_workspace_dir(home, creator_id, workspace_slug).join("kb")
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/kb/entries/`
///
/// Individual KB entry files (`<entry_id>.md`).
#[must_use]
pub fn creator_kb_entries_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    creator_kb_dir(home, creator_id, workspace_slug).join("entries")
}

/// `$HOME/.nexus42/device-id` — persistent machine identifier (`UUID` v4).
#[must_use]
pub fn device_id_path(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("device-id")
}

/// `$HOME/.nexus42/creators/<creator_id>/SOUL.md` (`ADR-014`, `ADR-016` D1).
///
/// # Defense-in-depth
///
/// If `creator_id` contains path traversal components (`..`, `/`, `\`), this
/// function panics rather than silently resolving to an unexpected path.
/// Callers (e.g., `soul_io::validate_creator_id()`) should validate `creator_id`
/// before calling this, but this acts as a safety net.
#[must_use]
pub fn creator_soul_md_path(home: &Path, creator_id: &str) -> PathBuf {
    assert_creator_id_safe(creator_id);
    nexus_root_from_home(home)
        .join("creators")
        .join(creator_id)
        .join("SOUL.md")
}

/// Assert that `creator_id` does not contain path-traversal characters.
///
/// This is a low-overhead sanity check; `nexus-domain::is_valid_creator_id()`
/// is the authoritative validator. This catches the most dangerous patterns:
/// `/`, `\`, `..`, and control characters.
fn assert_creator_id_safe(id: &str) {
    for ch in id.chars() {
        assert!(
            ch != '/' && ch != '\\',
            "creator_id contains path separator: {id:?} — this would be a path-traversal vulnerability"
        );
    }
    assert!(
        !id.contains(".."),
        "creator_id contains '..': {id:?} — this would be a path-traversal vulnerability"
    );
    assert!(
        !id.chars().any(char::is_control),
        "creator_id contains control characters: {id:?} — this would be a path-traversal vulnerability"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            operational_workspace_dir(&home, "ctr_test", "default"),
            PathBuf::from("/fake/home/.nexus42/creators/ctr_test/workspaces/default")
        );
    }

    #[test]
    fn workspace_state_db_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            workspace_state_db_path(&home, "c", "w"),
            PathBuf::from("/h/.nexus42/creators/c/workspaces/w/state.db")
        );
    }

    #[test]
    fn user_preset_base_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            user_preset_base_dir(&home),
            PathBuf::from("/fake/home/.nexus42/presets")
        );
    }

    #[test]
    fn user_preset_bundle_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            user_preset_bundle_dir(&home, "my-strategy"),
            PathBuf::from("/fake/home/.nexus42/presets/my-strategy")
        );
    }

    #[test]
    fn user_skills_dir_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            user_skills_dir(&home),
            PathBuf::from("/fake/home/.nexus42/skills")
        );
    }

    #[test]
    fn device_id_path_layout() {
        let home = PathBuf::from("/fake/home");
        assert_eq!(
            device_id_path(&home),
            PathBuf::from("/fake/home/.nexus42/device-id")
        );
    }

    #[test]
    fn soul_md_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_soul_md_path(&home, "ctr_test"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/SOUL.md")
        );
    }

    #[test]
    fn creator_kb_dir_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_kb_dir(&home, "ctr_test", "ws1"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/workspaces/ws1/kb")
        );
    }

    #[test]
    fn creator_kb_entries_dir_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_kb_entries_dir(&home, "ctr_test", "ws1"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/workspaces/ws1/kb/entries")
        );
    }

    #[test]
    #[should_panic(expected = "path separator")]
    fn soul_md_path_rejects_forward_slash() {
        let home = PathBuf::from("/h");
        let _ = creator_soul_md_path(&home, "../../etc/passwd");
    }

    #[test]
    #[should_panic(expected = "path separator")]
    fn soul_md_path_rejects_backslash() {
        let home = PathBuf::from("/h");
        let _ = creator_soul_md_path(&home, "ctr_bad\\etc");
    }

    #[test]
    #[should_panic(expected = "'..'")]
    fn soul_md_path_rejects_dotdot() {
        let home = PathBuf::from("/h");
        let _ = creator_soul_md_path(&home, "ctr_.._secret");
    }

    #[test]
    #[should_panic(expected = "control characters")]
    fn soul_md_path_rejects_control_chars() {
        let home = PathBuf::from("/h");
        let _ = creator_soul_md_path(&home, "ctr_\x00null");
    }
}
