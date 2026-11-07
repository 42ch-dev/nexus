//! I-000291: a no-override `ReadOnly` open must not require a writable home.
//!
//! The disk provider selection is read through the daemon-home `config.toml`
//! coordination. That coordination used to be an unconditional exclusive
//! create inside `.nexus42`, so an already-provisioned but read-only home made
//! a read-only open fail with a lock error even though it has nothing to write.
//! These targets drive the real `CoreService::open` surface: the read-only
//! home, the malformed-table refusal and a legitimate writer/open overlap.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt as _;
use std::path::PathBuf;

use nexus_core::{
    CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreService, MemoryProviderSelection,
    RemoteMemoryProviderSelection,
};
use tempfile::TempDir;

const CREATOR: &str = "ctr_readonly0000000000000000000000";
const SLUG: &str = "default";

/// A `[memory_provider]` table that does not match the typed layout: the
/// provider snapshot must fail closed on it instead of reading "no table".
fn malformed_provider_table() -> String {
    "\n[memory_provider]\n\
     kind = \"remote\"\n\
     space_id = 5\n\
     credential_id = \"cred-readonly\"\n"
        .to_string()
}

fn remote_selection() -> MemoryProviderSelection {
    MemoryProviderSelection::Remote(RemoteMemoryProviderSelection {
        space_id: "spc_readonly".to_string(),
        credential_id: "cred-readonly".to_string(),
        connect_endpoint: "wss://memory.example.test/".to_string(),
        remote_pubkey_hex: "ab".repeat(32),
        allowlist: vec!["peer-readonly".to_string()],
    })
}

/// The refusal a stored remote selection produces in the mode this build
/// selects. Its presence proves the open read the *remote* table: the implicit
/// local default would have opened successfully instead.
///
/// Feature on: the real signed provider factory runs and refuses at its
/// credential loader (these homes hold no credential file) — the
/// `memory_remote_disabled` static branch is not taken.
/// Feature off: the static `memory_remote_disabled` refusal is the behavior.
const fn remote_refusal_code() -> &'static str {
    #[cfg(feature = "memory-remote")]
    {
        "memory_credential_invalid"
    }
    #[cfg(not(feature = "memory-remote"))]
    {
        "memory_remote_disabled"
    }
}

fn assert_remote_open_refusal(error: &CoreError) {
    let CoreError::Coded { code, .. } = error else {
        panic!("expected the stored remote refusal, got {error:?}");
    };
    assert_eq!(
        code,
        remote_refusal_code(),
        "the open read the stored remote table (not the local default)"
    );
}

struct Home {
    _tmp: TempDir,
    user_home: PathBuf,
}

impl Home {
    fn new(provider_table: Option<&str>) -> Self {
        let tmp = TempDir::new().expect("temp home");
        let user_home = tmp.path().to_path_buf();
        fs::create_dir_all(user_home.join(".nexus42")).expect("nexus home");
        fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            &user_home, CREATOR, SLUG,
        ))
        .expect("workspace dir");
        let home = Self {
            _tmp: tmp,
            user_home,
        };
        home.write_config(provider_table);
        home
    }

    fn write_config(&self, provider_table: Option<&str>) {
        let mut text = format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"{SLUG}\"\n"
        );
        if let Some(table) = provider_table {
            text.push_str(table);
        }
        fs::write(self.nexus_home().join("config.toml"), text).expect("seed config");
    }

    fn nexus_home(&self) -> PathBuf {
        nexus_home_layout::nexus_root_from_home(&self.user_home)
    }

    fn options(&self, access: CoreAccess) -> CoreOpenOptions {
        CoreOpenOptions {
            user_home: self.user_home.clone(),
            access,
            memory: None,
        }
    }

    /// Materialize the workspace DB (and its provider tables) through the real
    /// writer open, then close it again so this home is fully provisioned.
    async fn provision(&self) {
        let core = CoreService::open(self.options(CoreAccess::EngineOwner))
            .await
            .expect("provisioning writer open");
        core.close().await.expect("provisioning close");
    }

    /// Make `.nexus42` itself readable but not writable — the exact directory
    /// the config lock is created in — and restore it on drop so the temp
    /// directory can be reclaimed.
    fn seal_nexus_home(&self) -> SealedHome {
        let nexus_home = self.nexus_home();
        fs::set_permissions(&nexus_home, fs::Permissions::from_mode(0o555))
            .expect("seal the nexus home");
        SealedHome { path: nexus_home }
    }
}

struct SealedHome {
    path: PathBuf,
}

impl Drop for SealedHome {
    fn drop(&mut self) {
        let _ = fs::set_permissions(&self.path, fs::Permissions::from_mode(0o755));
    }
}

/// A provisioned, readable-but-unwritable `.nexus42` still serves a no-override
/// read-only open (I-000291). Before the compatibility repair the open failed
/// while creating `config.toml.lock` in that directory.
#[tokio::test]
async fn a_local_readonly_open_does_not_require_a_writable_home() {
    let home = Home::new(None);
    home.provision().await;

    let sealed = home.seal_nexus_home();
    let opened = CoreService::open(home.options(CoreAccess::ReadOnly)).await;
    drop(sealed);

    let core = opened.expect("a read-only open must not need home writes");
    let provider = core
        .memory_provider_for_test()
        .expect("the local default is pinned");
    assert!(
        !provider.is_writable(),
        "a read-only open pins a read-only provider"
    );
    core.close().await.expect("close");
}

/// The same read-only home with a malformed provider table still fails closed:
/// the snapshot is really read, and a malformed table never degrades to local.
#[tokio::test]
async fn a_readonly_open_still_refuses_a_malformed_provider_table() {
    let home = Home::new(None);
    home.provision().await;
    home.write_config(Some(&malformed_provider_table()));

    let sealed = home.seal_nexus_home();
    let opened = CoreService::open(home.options(CoreAccess::ReadOnly)).await;
    drop(sealed);

    let error = opened
        .err()
        .expect("a malformed provider table is not local");
    let CoreError::Internal { category } = &error else {
        panic!("a malformed table is an internal config fault, got {error:?}");
    };
    assert!(
        category.starts_with("config_load:"),
        "the snapshot really read the table: {error:?}"
    );
}

/// `memory: Some(..)` never reads the provider table, so a read-only home with
/// a malformed table still serves the explicit override — and the override
/// needs no home write either.
#[tokio::test]
async fn an_explicit_override_never_reads_the_provider_table_on_a_read_only_home() {
    let home = Home::new(None);
    home.provision().await;
    home.write_config(Some(&malformed_provider_table()));

    let sealed = home.seal_nexus_home();
    let opened = CoreService::open(CoreOpenOptions {
        user_home: home.user_home.clone(),
        access: CoreAccess::ReadOnly,
        memory: Some(MemoryProviderSelection::Local),
    })
    .await;
    drop(sealed);

    let core = opened.expect("the explicit override does not read the malformed table");
    let provider = core
        .memory_provider_for_test()
        .expect("the explicit local selection is pinned");
    assert!(
        !provider.is_writable(),
        "the override keeps the read-only access"
    );
    core.close().await.expect("close");
}

/// A legitimate writer/open overlap never pins the local default while the disk
/// document keeps the remote table: every publication is a whole document, so
/// the open reads the stored remote selection instead of a truncated rewrite
/// that would look like "no table".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_writer_overlap_never_pins_the_local_default() {
    let home = Home::new(None);
    home.provision().await;
    let writer = CoreHomeService::open(home.user_home.clone()).expect("home service");
    writer
        .set_memory_provider_selection(&remote_selection())
        .expect("seed the remote selection");

    for round in 0..40 {
        let writer_home = home.user_home.clone();
        let rewrite = std::thread::spawn(move || {
            let service = CoreHomeService::open(writer_home).expect("home service");
            service
                .set_memory_provider_selection(&remote_selection())
                .expect("concurrent rewrite");
        });
        let opened = CoreService::open(home.options(CoreAccess::ReadOnly)).await;
        rewrite.join().expect("rewrite thread");

        match opened {
            Ok(core) => {
                core.close().await.expect("close");
                panic!(
                    "round {round}: the open pinned the local default, not the stored remote table"
                );
            }
            Err(error) => assert_remote_open_refusal(&error),
        }
    }

    // Every failed open above released its own claim: after the overlap the home
    // still serves a plain local open, so a leaked admission cannot hide an
    // accidental Local pin either.
    writer
        .clear_memory_provider_selection()
        .expect("clear the stored selection");
    let core = CoreService::open(home.options(CoreAccess::ReadOnly))
        .await
        .expect("a clean local open after the overlap");
    core.close().await.expect("close");
}
