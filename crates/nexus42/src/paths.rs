//! Operational filesystem paths for creator / workspace layout (ADR-014).
//!
//! Layout (under the user's home directory):
//! - `~/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/` — operational dir
//! - `.../state.db` — workspace-local SQLite
//! - `~/.nexus42/shared/global_state.db` — shared global SQLite

use std::path::{Path, PathBuf};

const NEXUS_DIR: &str = ".nexus42";

/// Resolve `~/.nexus42` from the user's home directory.
pub fn nexus_root_from_home(home: &Path) -> PathBuf {
    home.join(NEXUS_DIR)
}

/// Operational directory: `$HOME/.nexus42/creators/<creator_id>/workspaces/<workspace_slug>/`.
pub fn operational_workspace_dir(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    nexus_root_from_home(home)
        .join("creators")
        .join(creator_id)
        .join("workspaces")
        .join(workspace_slug)
}

/// Workspace-local SQLite path: `.../workspaces/<slug>/state.db`.
pub fn state_db_path(home: &Path, creator_id: &str, workspace_slug: &str) -> PathBuf {
    operational_workspace_dir(home, creator_id, workspace_slug).join("state.db")
}

/// Shared global SQLite: `$HOME/.nexus42/shared/global_state.db`.
pub fn shared_global_db_path(home: &Path) -> PathBuf {
    nexus_root_from_home(home)
        .join("shared")
        .join("global_state.db")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operational_dir_follows_creator_then_workspace_slug() {
        let home = PathBuf::from("/fake/home");
        let got = operational_workspace_dir(&home, "ctr_test", "default");
        assert_eq!(
            got,
            PathBuf::from("/fake/home/.nexus42/creators/ctr_test/workspaces/default")
        );
    }

    #[test]
    fn state_db_sits_under_operational_workspace() {
        let home = PathBuf::from("/h");
        assert_eq!(
            state_db_path(&home, "c", "w"),
            PathBuf::from("/h/.nexus42/creators/c/workspaces/w/state.db")
        );
    }

    #[test]
    fn shared_global_db_under_nexus_shared() {
        let home = PathBuf::from("/h");
        assert_eq!(
            shared_global_db_path(&home),
            PathBuf::from("/h/.nexus42/shared/global_state.db")
        );
    }
}
