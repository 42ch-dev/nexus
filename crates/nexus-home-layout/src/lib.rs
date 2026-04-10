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

/// Legacy flat `state.db` at the nexus config root (pre–ADR-014).
pub fn legacy_flat_state_db_path(home: &Path) -> PathBuf {
    nexus_root_from_home(home).join("state.db")
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
}
