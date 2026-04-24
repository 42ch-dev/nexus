//! Shared operational path layout (ADR-014) under the user home directory.
//!
//! Used by `nexus42` CLI and `nexus42d` so SQLite and workspace dirs resolve identically.

#![allow(dead_code)]

use std::path::{Path, PathBuf};

const NEXUS_DIR: &str = ".nexus42";

/// Resolve `~/.nexus42` from the user's home directory.
pub fn nexus_root_from_home(home: &Path) -> PathBuf {
    home.join(NEXUS_DIR)
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/`
pub fn creator_workspaces_root(home: &Path, creator_id: &str) -> PathBuf {
    nexus_root_from_home(home)
        .join("creators")
        .join(creator_id)
        .join("workspaces")
}

/// `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/`
pub fn operational_workspace_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    creator_workspaces_root(home, creator_id).join(workspace_slug)
}

/// Workspace-local SQLite: `.../workspaces/<slug>/state.db`
pub fn workspace_state_db_path(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    operational_workspace_dir(home, creator_id, workspace_slug).join("state.db")
}

/// Shared global SQLite: `$HOME/.nexus42/shared/global_state.db`
pub fn shared_global_db_path(home: &Path) -> PathBuf {
    nexus_root_from_home(home)
        .join("shared")
        .join("global_state.db")
}

/// `$HOME/.nexus42/presets/` — base directory for user-installed presets.
///
/// Each subdirectory under this path is expected to contain a `preset.yaml`.
/// See `crates/nexus-orchestration/src/user_preset_dir.rs` for scanning logic.
/// Directories starting with `_` or `.` are reserved and skipped by the scanner.
pub fn user_preset_base_dir(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("presets")
}

/// `$HOME/.nexus42/presets/<name>/` — path to a specific user preset bundle.
pub fn user_preset_bundle_dir(home: &Path, name: &str) -> PathBuf {
    user_preset_base_dir(home).join(name)
}

/// `$HOME/.nexus42/creators/<creator_id>/SOUL.md` (ADR-014, ADR-016 D1).
///
/// # Defense-in-depth
///
/// If `creator_id` contains path traversal components (`..`, `/`, `\`), this
/// function panics rather than silently resolving to an unexpected path.
/// Callers (e.g., `soul_io::validate_creator_id()`) should validate `creator_id`
/// before calling this, but this acts as a safety net.
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
        if ch == '/' || ch == '\\' {
            panic!(
                "creator_id contains path separator: {id:?} — \
                 this would be a path-traversal vulnerability"
            );
        }
    }
    if id.contains("..") {
        panic!(
            "creator_id contains '..': {id:?} — \
             this would be a path-traversal vulnerability"
        );
    }
    if id.chars().any(|c| c.is_control()) {
        panic!(
            "creator_id contains control characters: {id:?} — \
             this would be a path-traversal vulnerability"
        );
    }
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
    fn soul_md_path_layout() {
        let home = PathBuf::from("/h");
        assert_eq!(
            creator_soul_md_path(&home, "ctr_test"),
            PathBuf::from("/h/.nexus42/creators/ctr_test/SOUL.md")
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
