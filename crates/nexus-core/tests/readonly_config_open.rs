//! I-000291: a no-override `ReadOnly` open must not require a writable home.
//!
//! The disk provider selection is read through the daemon-home `config.toml`
//! coordination. That coordination used to be an unconditional exclusive
//! create inside `.nexus42`, so an already-provisioned but read-only home made
//! a read-only open fail with a lock error even though it has nothing to write.
//! These targets drive the real `CoreService::open` and real public writer
//! surfaces: the read-only home, the malformed-table and explicit-override
//! refusals, the publication boundary of both converted writers, and the
//! release of a failed writer admission.
#![cfg(unix)]

use std::fs;
use std::io::Read as _;
use std::os::unix::fs::PermissionsExt as _;
use std::path::{Path, PathBuf};

use nexus_contracts::generated::daemon_api::creators::set_active_creator_request::SetActiveCreatorRequest;
use nexus_core::{
    CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreService, MemoryProviderSelection,
    RemoteMemoryProviderSelection,
};
use tempfile::TempDir;

const CREATOR: &str = "ctr_readonly0000000000000000000000";
const OTHER_CREATOR: &str = "ctr_readonly0000000000000000000001";
const SLUG: &str = "default";

/// Seed profile membership: `use_creator` accepts a creator present in the
/// identity cache or the auth store.
fn seed_creator(nexus_home: &Path, creator_id: &str) {
    let cache = serde_json::json!({
        "creators": {
            creator_id: { "creator_id": creator_id, "handle": null, "display_name": null }
        }
    });
    fs::write(
        nexus_home.join("creator_identity_cache.json"),
        serde_json::to_string(&cache).expect("cache json"),
    )
    .expect("seed identity cache");
}

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

/// The publication boundary: a reader snapshot opened before a provider
/// selection publish reads one whole document, the published path reads the
/// other, and real opens on each side pin the authority that document selects.
#[tokio::test]
async fn a_provider_selection_publish_never_exposes_a_partial_document() {
    let home = Home::new(None);
    home.provision().await;
    let service = CoreHomeService::open(home.user_home.clone()).expect("home service");
    service
        .set_memory_provider_selection(&remote_selection())
        .expect("seed the remote selection");
    let path = home.nexus_home().join("config.toml");

    // Authority before: a real open pins the stored remote selection.
    assert_remote_open_refusal(
        &CoreService::open(home.options(CoreAccess::ReadOnly))
            .await
            .err()
            .expect("the stored remote selection is pinned"),
    );

    // This reader opened the document before the publish.
    let mut before = fs::File::open(&path).expect("pre-publication reader");

    service
        .set_memory_provider_selection(&MemoryProviderSelection::Local)
        .expect("publish the local default");

    let mut older = String::new();
    before
        .read_to_string(&mut older)
        .expect("read the pre-publication document");
    assert!(
        older.contains("kind = \"remote\"") && !older.contains("kind = \"local\""),
        "the pre-publication snapshot keeps the whole stored table: {older}"
    );
    older
        .parse::<toml::Table>()
        .expect("the pre-publication snapshot is complete TOML");

    let published = fs::read_to_string(&path).expect("published document");
    assert!(
        published.contains("kind = \"local\"") && !published.contains("kind = \"remote\""),
        "the published path holds the whole new version: {published}"
    );
    published
        .parse::<toml::Table>()
        .expect("the published document is complete TOML");

    // Authority after: the same real open now pins the published local default.
    let core = CoreService::open(home.options(CoreAccess::ReadOnly))
        .await
        .expect("the published local default opens");
    assert!(
        !core
            .memory_provider_for_test()
            .expect("local provider")
            .is_writable(),
        "a read-only open pins a read-only provider"
    );
    core.close().await.expect("close");
}

/// BUG-01: the real creator writer (`use_creator`) publishes whole documents
/// too. A reader snapshot opened before the switch keeps the old selection, the
/// path holds the new one, and a key another writer owns survives.
#[test]
fn a_creator_switch_never_exposes_a_partial_document() {
    let home = Home::new(Some("\n[agent_profile]\nname = \"kept\"\n"));
    seed_creator(&home.nexus_home(), OTHER_CREATOR);
    let path = home.nexus_home().join("config.toml");
    let mut before = fs::File::open(&path).expect("pre-publication reader");

    CoreHomeService::open(home.user_home)
        .expect("home service")
        .use_creator(SetActiveCreatorRequest {
            creator_id: OTHER_CREATOR.to_string(),
        })
        .expect("creator switch");

    let mut older = String::new();
    before
        .read_to_string(&mut older)
        .expect("read the pre-publication document");
    let old_document: toml::Table = older
        .parse()
        .expect("the pre-publication snapshot is complete TOML");
    assert_eq!(
        old_document
            .get("active_creator_id")
            .and_then(toml::Value::as_str),
        Some(CREATOR),
        "the pre-publication snapshot keeps the whole old selection: {older}"
    );

    let published = fs::read_to_string(&path).expect("published document");
    let new_document: toml::Table = published
        .parse()
        .expect("the published document is complete TOML");
    assert_eq!(
        new_document
            .get("active_creator_id")
            .and_then(toml::Value::as_str),
        Some(OTHER_CREATOR),
        "the published path holds the whole new selection: {published}"
    );
    assert!(
        new_document.contains_key("agent_profile"),
        "the key another writer owns survives: {published}"
    );
}

/// TEST-01: a failed writer open releases the guarded writer admission it took,
/// so a legitimate writer open can acquire it afterwards.
#[tokio::test]
async fn a_failed_writer_open_releases_its_admission() {
    let home = Home::new(None);
    home.provision().await;
    let service = CoreHomeService::open(home.user_home.clone()).expect("home service");
    service
        .set_memory_provider_selection(&remote_selection())
        .expect("seed the remote selection");

    let failed = CoreService::open(home.options(CoreAccess::EngineOwner)).await;
    assert_remote_open_refusal(
        &failed
            .err()
            .expect("the stored remote selection refuses the writer open"),
    );

    // The refused open took a guarded writer admission before the pin failed;
    // a real writer open now proves that admission was released.
    service
        .clear_memory_provider_selection()
        .expect("restore the local default");
    let writer = CoreService::open(home.options(CoreAccess::DirectWriter))
        .await
        .expect("the failed open released its writer admission");
    writer.close().await.expect("close");
}
