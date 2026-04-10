//! Operational filesystem paths (ADR-014). Shared implementation: [`nexus_home_layout`].

pub use nexus_home_layout::{
    creator_workspaces_root, legacy_flat_state_db_path, operational_workspace_dir,
    workspace_state_db_path,
};

/// Workspace `state.db` path (alias for plan/ADR naming).
pub fn state_db_path(
    home: &std::path::Path,
    creator_id: &str,
    workspace_slug: &str,
) -> std::path::PathBuf {
    workspace_state_db_path(home, creator_id, workspace_slug)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nexus_home_layout::shared_global_db_path;
    use std::path::PathBuf;

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

    #[test]
    fn creator_workspaces_root_layout() {
        let home = PathBuf::from("/x");
        assert_eq!(
            creator_workspaces_root(&home, "ctr_1"),
            PathBuf::from("/x/.nexus42/creators/ctr_1/workspaces")
        );
    }
}
