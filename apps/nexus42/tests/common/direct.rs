//! Server-free fixture for the direct-core CLI lifecycle tests (v1.193 P0-T1).
//!
//! The creator fixtures this plan retires boot a live in-process daemon
//! router. The direct-core authoring leaves must instead run against an
//! isolated raw `HOME` with no server, no Node child and no live provider.
//!
//! [`DirectFixture`] seeds that home through the core's own pre-selection home
//! entry (`CoreHomeService`: register the creator, then select the workspace
//! that initializes its guarded state DB), and then **releases the seed
//! writer** before any CLI child is spawned. Drop is not proof of release, so
//! the release is an explicit call. The fixture holds no running server and no
//! live writer afterwards.

use nexus_contracts::{CoreRegisterCreatorRequest, SetActiveWorkspaceRequest};
use nexus_core::CoreHomeService;
use nexus_home_layout::{operational_workspace_dir, workspace_state_db_path};
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use tempfile::TempDir;

/// Creator display name the fixture registers (a persistent local identity).
const CREATOR_NAME: &str = "Direct Fixture Author";
/// Workspace the fixture materializes and selects.
const WORKSPACE_SLUG: &str = "default";

/// An isolated raw `HOME` with one active creator + workspace and no daemon.
pub struct DirectFixture {
    /// Hermetic `HOME` (parent of `.nexus42`). Kept alive for the whole test.
    pub home: TempDir,
}

impl DirectFixture {
    /// Create the hermetic home and select an initialized creator/workspace.
    ///
    /// # Panics
    ///
    /// Panics if the temp home cannot be created, the creator/workspace cannot
    /// be registered and selected, or the workspace directory cannot be
    /// materialized — the fixture has nothing to test against without them.
    pub async fn new() -> Self {
        let home = tempfile::tempdir().expect("temp home");
        let user_home = home.path().to_path_buf();
        let selector =
            CoreHomeService::open(user_home.clone()).expect("home entry opens on a raw home");

        let creator = selector
            .register_creator(CoreRegisterCreatorRequest {
                display_name: Some(CREATOR_NAME.parse().expect("valid display name")),
                platform_creator_id: None,
            })
            .await
            .expect("register fixture creator");
        let creator_id = creator.creator_id;

        // `select_workspace` initializes only a workspace that exists on disk,
        // so materialize the operational directory first — the same layout the
        // CLI's own home resolution reads.
        std::fs::create_dir_all(operational_workspace_dir(
            &user_home,
            &creator_id,
            WORKSPACE_SLUG,
        ))
        .expect("materialize workspace dir");
        selector
            .select_workspace(SetActiveWorkspaceRequest {
                creator_id: Some(creator_id.clone()),
                workspace_slug: WORKSPACE_SLUG.to_string(),
            })
            .await
            .expect("select fixture workspace");

        // The selection above admits a direct writer and retains its OS
        // admission guard in THIS process. A CLI child must not be handed a
        // home some other writer still holds, so release it explicitly.
        release_retained_writer_guards(&workspace_state_db_path(
            &user_home,
            &creator_id,
            WORKSPACE_SLUG,
        ));

        Self { home }
    }

    /// The real `nexus42` binary, pointed at this fixture's hermetic `HOME`.
    pub fn command(&self) -> assert_cmd::Command {
        let mut command = assert_cmd::Command::cargo_bin("nexus42").expect("nexus42 binary");
        command
            .env("HOME", self.home.path())
            .env("RUST_LOG", "off");
        command
    }
}
