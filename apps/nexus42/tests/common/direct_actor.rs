//! Shared direct-core **actor** fixture for the Character family (v1.193 P0-T9).
//!
//! The identity/binding (P0-T9), knowledge (P0-T10) and memory/ToM (P0-T11)
//! tasks all need the same preconditions: a hermetic raw `HOME` with one active
//! creator/workspace, owned Worlds, Characters with their initial binding, and
//! a World-owned `character` `KeyBlock` when a `WorldSheet` link is exercised.
//! [`DirectActor`] is that single fixture — include it next to the server-free
//! [`crate::direct::DirectFixture`] it composes:
//!
//! The multi-World knowledge dogfood additionally needs a second binding and
//! its own read of the rows the CLI just wrote, so the fixture also seeds
//! bindings ([`DirectActor::add_binding`]) and exposes the released workspace
//! DB path ([`DirectActor::state_db_path`]) for read-only assertions.
//!
//! ```ignore
//! #[path = "common/direct.rs"]
//! mod direct;
//! #[path = "common/direct_actor.rs"]
//! mod direct_actor;
//! ```
//!
//! Every seed runs through an **authorized** writer — Worlds, Characters and
//! `WorldSheets` through the direct core seam (`CoreService`), exactly the
//! producers the product paths use — and never through a foreign-actor
//! surrogate: no principal, ownership row or holder is fabricated by hand.
//!
//! A seed admits its writer in this process and **releases** it before
//! returning, so the real CLI child spawned by [`DirectActor::cli`] always
//! finds the workspace claimable. The fixture holds no running server and no
//! live writer.

use crate::direct::DirectFixture;
use nexus_contracts::world_kb_patch_entity_request::{
    NexusWorldKbEntityPatch, NexusWorldKbEntityPatchBlockType, NexusWorldKbEntityPatchTitle,
};
use nexus_contracts::{CreateCharacterRequest, CreateWorldRequest, WorldKbPatchEntityRequest};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService, Principal};
use nexus_home_layout::{nexus_root_from_home, workspace_state_db_path};
use nexus_local_db::writer_protocol::release_retained_writer_guards;
use std::path::{Path, PathBuf};
use std::process::Output;

/// Workspace the fixture materializes and selects (the shared
/// [`crate::direct::DirectFixture`] selection).
const WORKSPACE_SLUG: &str = "default";

/// One seeded Character with the initial binding created alongside it.
#[allow(dead_code)] // the subset each Character-family test crate uses differs
pub struct SeededCharacter {
    /// Stored Character id (`chr_…`).
    pub character_id: String,
    /// The Character's initial active binding id (`awb_…`).
    pub binding_id: String,
}

/// A hermetic direct-core home with one active creator, plus authorized seed
/// helpers for the actor graph and a runner for the real CLI child.
pub struct DirectActor {
    fixture: DirectFixture,
    creator_id: String,
}

#[allow(dead_code)] // the subset each Character-family test crate uses differs
impl DirectActor {
    /// Create the hermetic home and select an initialized creator/workspace.
    ///
    /// # Panics
    ///
    /// Panics when the home cannot be created or its single creator directory
    /// cannot be resolved — every seed below needs both.
    pub async fn new() -> Self {
        let fixture = DirectFixture::new().await;
        let creator_id = fixture_creator_id(fixture.home.path());
        Self {
            fixture,
            creator_id,
        }
    }

    /// The hermetic `HOME` (parent of `.nexus42`).
    pub fn home(&self) -> &Path {
        self.fixture.home.path()
    }

    /// The active creator the fixture registered and selected.
    pub fn creator_id(&self) -> &str {
        &self.creator_id
    }

    /// Run the real `nexus42` binary against this actor's hermetic `HOME`.
    ///
    /// # Panics
    ///
    /// Panics when the binary cannot be spawned.
    pub fn cli(&self, args: &[&str]) -> Output {
        self.fixture
            .command()
            .args(args)
            .output()
            .expect("spawn nexus42")
    }

    /// Seed one owned, active World through the core and return its id.
    ///
    /// # Panics
    ///
    /// Panics when the seed core cannot be opened or the World is refused —
    /// the fixture has nothing to test against without it.
    pub async fn create_world(&self, title: &str) -> String {
        let (core, principal) = self.open_seed_core().await;
        let created = core
            .create_world(
                &principal,
                CreateWorldRequest {
                    title: title.try_into().expect("world title is wire-valid"),
                },
            )
            .await
            .expect("seed world");
        self.close_seed_core(core).await;
        created.world_id
    }

    /// Seed one owned Character with its initial active binding on `world_id`.
    ///
    /// # Panics
    ///
    /// Panics when the seed core cannot be opened or the Character is refused
    /// (a foreign/unknown World, a duplicate display name, storage failure).
    pub async fn create_character(&self, display_name: &str, world_id: &str) -> SeededCharacter {
        let (core, principal) = self.open_seed_core().await;
        let created = core
            .create_character(
                &principal,
                CreateCharacterRequest {
                    display_name: display_name
                        .try_into()
                        .expect("character display name is wire-valid"),
                    image_uri: None,
                    persona: serde_json::Map::new(),
                    world_id: world_id.try_into().expect("world id is wire-valid"),
                    world_sheet_entry_id: None,
                },
            )
            .await
            .expect("seed character");
        self.close_seed_core(core).await;
        SeededCharacter {
            character_id: created.character.character_id.as_str().to_string(),
            binding_id: created.binding.binding_id.as_str().to_string(),
        }
    }

    /// Seed one additional active binding for `character_id` on `world_id` and
    /// return the new binding id.
    ///
    /// # Panics
    ///
    /// Panics when the seed core cannot be opened or the binding is refused
    /// (a foreign/unknown Character or World, a duplicate active binding).
    pub async fn add_binding(&self, character_id: &str, world_id: &str) -> String {
        let (core, principal) = self.open_seed_core().await;
        let created = core
            .add_binding(
                &principal,
                character_id.to_string(),
                world_id.to_string(),
                None,
            )
            .await
            .expect("seed character binding");
        self.close_seed_core(core).await;
        created.binding.binding_id.as_str().to_string()
    }

    /// The selected workspace `state.db` path — the read handle a test uses for
    /// stored-row assertions once every seed writer has been released.
    pub fn state_db_path(&self) -> PathBuf {
        self.db_path()
    }

    /// The released workspace `state.db`, opened read-only for stored-row
    /// assertions.
    ///
    /// The handle is a plain read-only pool: a test takes it, asserts, and
    /// closes it while no CLI child runs, so the next child still admits its
    /// own writer.
    ///
    /// # Panics
    ///
    /// Panics when the stored workspace DB cannot be opened read-only.
    pub async fn read_only_pool(&self) -> sqlx::SqlitePool {
        nexus_local_db::open_pool_read_only(&self.state_db_path())
            .await
            .expect("open fixture state db read-only")
    }

    /// Seed one World-owned `character` `KeyBlock` — the only shape a binding
    /// may link as its `WorldSheet` (a shared, live, World-owned character
    /// entry) — through the core's own World-KB authoring path.
    ///
    /// `key_block_id` must follow the stored `kb_<hex>` convention.
    ///
    /// # Panics
    ///
    /// Panics when the seed core cannot be opened or the entry is refused
    /// (a malformed id, a foreign World, an invalid canonical name).
    pub async fn seed_world_character_sheet(
        &self,
        key_block_id: &str,
        world_id: &str,
        canonical_name: &str,
    ) {
        let (core, principal) = self.open_seed_core().await;
        core.patch_world_kb_entity(
            &principal,
            world_id.to_string(),
            WorldKbPatchEntityRequest {
                entity_id: key_block_id.to_string(),
                expected_version: 0,
                patch: NexusWorldKbEntityPatch {
                    title: Some(
                        NexusWorldKbEntityPatchTitle::try_from(canonical_name.to_string())
                            .expect("canonical name is wire-valid"),
                    ),
                    block_type: Some(NexusWorldKbEntityPatchBlockType::Character),
                    ..NexusWorldKbEntityPatch::default()
                },
            },
        )
        .await
        .expect("seed WorldSheet character entry");
        self.close_seed_core(core).await;
    }

    /// The selected workspace `state.db` (the writer admission key).
    fn db_path(&self) -> PathBuf {
        workspace_state_db_path(self.home(), &self.creator_id, WORKSPACE_SLUG)
    }

    /// Open the direct-writer core over this actor's home and admit the
    /// fixture's own active principal.
    async fn open_seed_core(&self) -> (CoreService, Principal) {
        let core = CoreService::open(CoreOpenOptions {
            user_home: self.home().to_path_buf(),
            access: CoreAccess::DirectWriter,
        })
        .await
        .expect("seed core opens on the actor home");
        let principal = core
            .active_principal()
            .await
            .expect("fixture registers an active principal");
        (core, principal)
    }

    /// Close a seed core and release the writer it retained *in this process*,
    /// so the next CLI child admits its own instead of finding the workspace
    /// held by the fixture.
    async fn close_seed_core(&self, core: CoreService) {
        core.close().await.expect("seed core closes");
        release_retained_writer_guards(&self.db_path());
    }
}

/// The fixture home holds exactly one creator; its id is the directory name
/// under the nexus root's `creators/`.
fn fixture_creator_id(home: &Path) -> String {
    let creators_root = nexus_root_from_home(home).join("creators");
    let mut entries: Vec<_> = std::fs::read_dir(&creators_root)
        .expect("read fixture creators root")
        .map(|entry| entry.expect("creator dir entry").file_name())
        .collect();
    assert_eq!(entries.len(), 1, "fixture registers exactly one creator");
    entries
        .pop()
        .expect("one creator")
        .to_string_lossy()
        .into_owned()
}
