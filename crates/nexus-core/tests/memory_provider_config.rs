//! `[memory_provider]` selection, shared-lock writer and credential loader
//! (NXM-9 Task 1; `n3-provider-binding.md` §2–§3).
//!
//! Every case drives the real core API on a temporary home: no daemon, no
//! creator, no live provider. Credential fixtures are synthetic — no real auth
//! material is read or logged.

#[cfg(unix)]
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_core::{
    CoreError, CoreHomeService, MemoryCredential, MemoryProviderConfig, MemoryProviderSelection,
    RemoteMemoryProviderSelection,
};
use nexus_home_layout::active_context::{
    read_active_creator_id, read_active_workspace_slug, read_memory_provider_config,
};
#[cfg(unix)]
use std::future::Future as _;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const SPACE_ID: &str = "space_alpha";
const CREDENTIAL_ID: &str = "cred_alpha-1";
const PUBKEY_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
/// Synthetic seed; a distinctive value so a leak into an error is detectable.
const SEED_HEX: &str = "0f1e2d3c4b5a69788796a5b4c3d2e1f00f1e2d3c4b5a69788796a5b4c3d2e1f0";
const OTHER_WRITER_TOML: &str =
    "active_creator_id = \"crt_alpha\"\n\n[agent_profile]\nmodel = \"m1\"\n";

fn home() -> TempDir {
    tempfile::tempdir().expect("temp home")
}

fn nexus_dir(home: &Path) -> PathBuf {
    home.join(".nexus42")
}

fn config_path(home: &Path) -> PathBuf {
    nexus_dir(home).join("config.toml")
}

fn credentials_dir(home: &Path) -> PathBuf {
    nexus_dir(home).join("memory-credentials")
}

fn read_selection(home: &Path) -> MemoryProviderSelection {
    let config = read_memory_provider_config(&nexus_dir(home)).expect("provider table reads");
    MemoryProviderSelection::from_config(config).expect("selection converts")
}

fn remote(credential_id: &str, endpoint: &str, pubkey_hex: &str) -> RemoteMemoryProviderSelection {
    RemoteMemoryProviderSelection {
        space_id: SPACE_ID.to_string(),
        credential_id: credential_id.to_string(),
        connect_endpoint: endpoint.to_string(),
        remote_pubkey_hex: pubkey_hex.to_string(),
        allowlist: vec!["12D3KooWPeer".to_string()],
    }
}

fn remote_config(endpoint: &str) -> MemoryProviderConfig {
    MemoryProviderConfig::Remote {
        space_id: SPACE_ID.to_string(),
        credential_id: CREDENTIAL_ID.to_string(),
        connect_endpoint: endpoint.to_string(),
        remote_pubkey_hex: PUBKEY_HEX.to_string(),
        allowlist: vec!["12D3KooWPeer".to_string()],
    }
}

fn assert_invalid_field(error: CoreError, expected: &str) {
    match error {
        CoreError::InvalidInput { field, .. } => {
            assert_eq!(field, expected, "unexpected invalid field");
        }
        other => panic!("expected InvalidInput for {expected}, got {other:?}"),
    }
}

#[cfg(unix)]
fn assert_credential_refused(error: CoreError) {
    match error {
        CoreError::Coded { code, .. } => {
            assert_eq!(code, "memory_credential_invalid", "unexpected refusal code");
        }
        other => panic!("expected the coded credential refusal, got {other:?}"),
    }
}

#[cfg(unix)]
fn write_credential(home: &Path, credential_id: &str, content: &str, mode: u32) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let dir = credentials_dir(home);
    std::fs::create_dir_all(&dir).expect("credential dir");
    let path = dir.join(format!("{credential_id}.json"));
    std::fs::write(&path, content).expect("credential file");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode))
        .expect("credential mode");
    path
}

fn valid_credential_json() -> String {
    format!("{{\"identity_seed_hex\": \"{SEED_HEX}\", \"capability_token\": null}}")
}

#[test]
fn missing_memory_provider_table_selects_local() {
    let home = home();

    // Absent config file is the implicit local default, not an error.
    assert_eq!(read_selection(home.path()), MemoryProviderSelection::Local);
    assert!(!config_path(home.path()).exists());

    // A config owned by other writers without `[memory_provider]` stays local.
    std::fs::create_dir_all(nexus_dir(home.path())).expect("nexus dir");
    std::fs::write(config_path(home.path()), OTHER_WRITER_TOML).expect("seed config");
    assert_eq!(read_selection(home.path()), MemoryProviderSelection::Local);
    assert!(config_path(home.path()).exists());
}

/// I-000304: only a genuinely absent `config.toml` is the implicit local
/// default. A metadata failure must reach the caller as an error, so a remote
/// configured home can never silently fall through to the local provider.
#[test]
fn unreadable_provider_config_is_an_error_not_the_local_default() {
    let home = home();

    // Genuine absence stays the documented default.
    assert!(read_memory_provider_config(&nexus_dir(home.path()))
        .expect("an absent config is not an error")
        .is_none());

    // A readable config still yields its remote selection.
    std::fs::create_dir_all(nexus_dir(home.path())).expect("nexus dir");
    let service = CoreHomeService::open(home.path().to_path_buf()).expect("home entry");
    let selection = MemoryProviderSelection::Remote(remote(
        CREDENTIAL_ID,
        "ws://127.0.0.1:8420/memory",
        PUBKEY_HEX,
    ));
    service
        .set_memory_provider_selection(&selection)
        .expect("write remote selection");
    assert_eq!(read_selection(home.path()), selection);

    // The config path's parent is a regular file, so metadata fails with a
    // real non-`NotFound` error (ENOTDIR on Unix). `Path::exists()` used to
    // fold that into "absent", which pinned Local for a config the reader
    // could not inspect.
    let not_a_dir = home.path().join("not-a-dir");
    std::fs::write(&not_a_dir, "regular file").expect("parent file");
    let error = read_memory_provider_config(&not_a_dir)
        .expect_err("an uninspectable config must not read as absent");
    assert!(
        !error.to_string().is_empty(),
        "the filesystem error reaches the caller: {error}"
    );
}

/// I-000304, consumer level: `CoreService::open` resolves the workspace from
/// `config.toml` first and reads the `[memory_provider]` table afterwards, from
/// the SAME document. When that document becomes uninspectable inside that
/// window — the report's own "symlinked target becomes inaccessible between
/// workspace resolution and provider selection" — the open must reach the
/// existing `config_load` failure instead of pinning the implicit local
/// provider. Before the fix `Path::exists()` folded the ELOOP into "absent",
/// so the open succeeded on Local.
///
/// The swap is ordered by this test, not by the scheduler: the fresh store's
/// public migration fence is held while the open is polled exactly once, so
/// the open must park in its fenced migration admission after the workspace
/// snapshot and before the provider selection reads the same document again.
#[cfg(unix)]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_uninspectable_provider_table_fails_the_open_instead_of_selecting_local() {
    const CREATOR: &str = "crt_alpha";
    const SLUG: &str = "default";

    let home = tempfile::tempdir().expect("temp home");
    let user_home = home.path().canonicalize().expect("canonical home");
    let nexus_home = nexus_home_layout::nexus_root_from_home(&user_home);
    std::fs::create_dir_all(&nexus_home).expect("nexus home");
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home, CREATOR, SLUG,
    ))
    .expect("workspace dir");
    let document = format!(
        "active_creator_id = \"{CREATOR}\"\n\
         [active_workspace_slug_by_creator]\n\
         \"{CREATOR}\" = \"{SLUG}\"\n"
    );
    let config = nexus_home.join("config.toml");
    std::fs::write(&config, &document).expect("seed config");
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, SLUG);
    let open = |access| CoreOpenOptions {
        user_home: user_home.clone(),
        access,
        memory: None,
    };

    // Hold the fresh store's public migration fence before the open runs: the
    // open's first admission must take this exclusive lock, so a single poll
    // can only reach that wait — it can neither migrate, open a writer nor
    // select a provider while this test owns the lock.
    let fence = nexus_local_db::writer_protocol::acquire_store_reset_fence(&db_path)
        .expect("the fresh store has no live writer");
    let mut opened = Box::pin(CoreService::open(open(CoreAccess::EngineOwner)));
    // One poll runs the synchronous workspace discovery and parks on the
    // fenced admission, so the workspace snapshot has been read and the
    // provider selection has provably not run yet.
    let mut poll_once = std::task::Context::from_waker(std::task::Waker::noop());
    assert!(
        matches!(
            opened.as_mut().poll(&mut poll_once),
            std::task::Poll::Pending
        ),
        "the held migration fence keeps the open from finishing"
    );
    // The window: the same `config.toml` turns uninspectable — a
    // self-referential symlink answers ELOOP, a non-`NotFound` metadata error —
    // between the workspace snapshot and the provider selection.
    std::fs::remove_file(&config).expect("drop the readable document");
    std::os::unix::fs::symlink("config.toml", &config).expect("self-referential config symlink");
    drop(fence);

    let Err(error) = opened.await else {
        panic!("an uninspectable provider table must not select the local default");
    };
    let CoreError::Internal { category } = &error else {
        panic!("expected the config_load refusal, got {error:?}");
    };
    assert!(
        category.starts_with("config_load:"),
        "the provider snapshot refused the document: {error:?}"
    );
    assert!(
        category.contains("Too many levels of symbolic links"),
        "the failure is the uninspectable document, not workspace discovery: {category}"
    );

    // The failed pin released the engine admission it took: a real writer open
    // acquires it once the document is readable again.
    std::fs::remove_file(&config).expect("drop the config symlink");
    std::fs::write(&config, &document).expect("restore config");
    let writer = CoreService::open(open(CoreAccess::DirectWriter))
        .await
        .expect("the failed open released its writer admission");
    writer.close().await.expect("close");
}

#[test]
fn explicit_local_selection_round_trips() {
    let home = home();
    let service = CoreHomeService::open(home.path().to_path_buf()).expect("home entry");

    service
        .set_memory_provider_selection(&MemoryProviderSelection::Local)
        .expect("write explicit local selection");

    let text = std::fs::read_to_string(config_path(home.path())).expect("config read");
    assert!(text.contains("[memory_provider]"), "{text}");
    assert!(text.contains("kind = \"local\""), "{text}");
    assert_eq!(read_selection(home.path()), MemoryProviderSelection::Local);
}

#[test]
fn remote_selection_round_trips_and_preserves_other_writers_keys() {
    let home = home();
    std::fs::create_dir_all(nexus_dir(home.path())).expect("nexus dir");
    std::fs::write(config_path(home.path()), OTHER_WRITER_TOML).expect("seed config");
    let service = CoreHomeService::open(home.path().to_path_buf()).expect("home entry");
    let selection = MemoryProviderSelection::Remote(remote(
        CREDENTIAL_ID,
        "ws://127.0.0.1:8420/memory",
        PUBKEY_HEX,
    ));

    service
        .set_memory_provider_selection(&selection)
        .expect("write remote selection");

    let text = std::fs::read_to_string(config_path(home.path())).expect("config read");
    for preserved in [
        "active_creator_id = \"crt_alpha\"",
        "[agent_profile]",
        "model = \"m1\"",
    ] {
        assert!(
            text.contains(preserved),
            "lost other writer key {preserved}: {text}"
        );
    }
    assert!(text.contains("kind = \"remote\""), "{text}");
    assert!(text.contains(CREDENTIAL_ID), "{text}");
    assert!(
        !text.contains(SEED_HEX),
        "selection must never persist a seed: {text}"
    );
    assert_eq!(read_selection(home.path()), selection);
}

#[test]
fn malformed_credential_id_is_rejected_without_writing() {
    let home = home();
    let service = CoreHomeService::open(home.path().to_path_buf()).expect("home entry");

    for bad in ["../escape", "", "-leading", "a b", &"x".repeat(65)] {
        let selection =
            MemoryProviderSelection::Remote(remote(bad, "ws://127.0.0.1:8420/memory", PUBKEY_HEX));
        let error = service
            .set_memory_provider_selection(&selection)
            .expect_err("malformed credential id must be refused");
        assert_invalid_field(error, "credential_id");
    }
    assert!(
        !config_path(home.path()).exists(),
        "a refused selection writes nothing"
    );
}

#[test]
fn malformed_connect_endpoints_are_rejected() {
    // Userinfo, query, fragment, wrong scheme and plain ws to a non-loopback
    // host are all refused (§3.2).
    for endpoint in [
        "https://example.com/ws",
        "ws://example.com:8420",
        "wss://user:pass@example.com/ws",
        "wss://example.com/ws?token=x",
        "wss://example.com/ws#frag",
        // Malformed authority and port, previously accepted by string splitting.
        "wss://:8420",
        "wss://host:bad",
        "ws://localhost:bad",
        "wss://[::1/ws",
        // Empty authority, empty userinfo, an empty/invalid port and a host
        // outside the locked loopback set are all refused by the raw
        // transport grammar.
        "wss:///ws",
        "wss://@example.com/ws",
        "wss://example.com:",
        "wss://example.com:8420:1",
        "ws://127.0.0.2:8420",
        "ws://[::2]:8420",
    ] {
        let error = MemoryProviderSelection::from_config(Some(remote_config(endpoint)))
            .expect_err("malformed endpoint must be refused");
        assert_invalid_field(error, "connect_endpoint");
    }

    // The shared writer refuses the same authorities before persisting them.
    let home = home();
    let service = CoreHomeService::open(home.path().to_path_buf()).expect("home entry");
    for endpoint in [
        "wss://:8420",
        "wss://host:bad",
        "ws://localhost:bad",
        "wss:///ws",
        "ws://127.0.0.2:8420",
    ] {
        let selection =
            MemoryProviderSelection::Remote(remote(CREDENTIAL_ID, endpoint, PUBKEY_HEX));
        let error = service
            .set_memory_provider_selection(&selection)
            .expect_err("malformed endpoint must not be written");
        assert_invalid_field(error, "connect_endpoint");
    }
    assert!(
        !config_path(home.path()).exists(),
        "a refused selection writes nothing"
    );

    // Loopback ws:// (tests) and wss:// remain selectable, persisted verbatim.
    for endpoint in [
        "ws://127.0.0.1:8420/memory",
        "ws://[::1]:8420",
        "ws://localhost:1",
        "wss://memory.example.com/ws",
    ] {
        MemoryProviderSelection::from_config(Some(remote_config(endpoint)))
            .unwrap_or_else(|error| panic!("{endpoint} must be accepted: {error:?}"));
    }
}

#[test]
fn malformed_remote_references_are_rejected() {
    for (space_id, pubkey_hex, expected_field) in [
        ("", PUBKEY_HEX, "space_id"),
        ("has space", PUBKEY_HEX, "space_id"),
        (SPACE_ID, "", "remote_pubkey_hex"),
        (SPACE_ID, "0123", "remote_pubkey_hex"),
        (
            SPACE_ID,
            "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF",
            "remote_pubkey_hex",
        ),
        (
            SPACE_ID,
            "g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "remote_pubkey_hex",
        ),
    ] {
        let config = MemoryProviderConfig::Remote {
            space_id: space_id.to_string(),
            credential_id: CREDENTIAL_ID.to_string(),
            connect_endpoint: "ws://127.0.0.1:8420/memory".to_string(),
            remote_pubkey_hex: pubkey_hex.to_string(),
            allowlist: vec!["12D3KooWPeer".to_string()],
        };
        let error = MemoryProviderSelection::from_config(Some(config))
            .expect_err("malformed remote reference must be refused");
        assert_invalid_field(error, expected_field);
    }

    let no_peers = MemoryProviderConfig::Remote {
        space_id: SPACE_ID.to_string(),
        credential_id: CREDENTIAL_ID.to_string(),
        connect_endpoint: "ws://127.0.0.1:8420/memory".to_string(),
        remote_pubkey_hex: PUBKEY_HEX.to_string(),
        allowlist: Vec::new(),
    };
    assert_invalid_field(
        MemoryProviderSelection::from_config(Some(no_peers))
            .expect_err("empty allowlist must be refused"),
        "allowlist",
    );
}

#[test]
fn clear_restores_implicit_local_and_preserves_other_writers_keys() {
    let home = home();
    std::fs::create_dir_all(nexus_dir(home.path())).expect("nexus dir");
    std::fs::write(config_path(home.path()), OTHER_WRITER_TOML).expect("seed config");
    let service = CoreHomeService::open(home.path().to_path_buf()).expect("home entry");
    service
        .set_memory_provider_selection(&MemoryProviderSelection::Remote(remote(
            CREDENTIAL_ID,
            "ws://127.0.0.1:8420/memory",
            PUBKEY_HEX,
        )))
        .expect("write remote selection");

    service
        .clear_memory_provider_selection()
        .expect("clear selection");

    let text = std::fs::read_to_string(config_path(home.path())).expect("config read");
    assert!(!text.contains("memory_provider"), "{text}");
    assert!(text.contains("[agent_profile]"), "{text}");
    assert_eq!(read_selection(home.path()), MemoryProviderSelection::Local);
}

#[test]
fn malformed_provider_table_does_not_change_creator_or_workspace_resolution() {
    let home = home();
    std::fs::create_dir_all(nexus_dir(home.path())).expect("nexus dir");

    // Unknown `kind` and a remote table missing fields: both are malformed for
    // provider selection, and neither may disturb unrelated readers. The
    // configured workspace is deliberately non-default so a fallback to
    // `default` is observable.
    for malformed in [
        "\n[memory_provider]\nkind = \"bogus\"\n",
        "\n[memory_provider]\nkind = \"remote\"\nspace_id = \"space_alpha\"\n",
    ] {
        std::fs::write(
            config_path(home.path()),
            format!(
                "active_creator_id = \"crt_alpha\"\n\
                 active_workspace_slug_by_creator = {{ crt_alpha = \"team-one\" }}\n\
                 {malformed}"
            ),
        )
        .expect("seed config");

        assert_eq!(
            read_active_creator_id(&nexus_dir(home.path())).as_deref(),
            Some("crt_alpha"),
            "a malformed provider table must not hide the active creator"
        );
        assert_eq!(
            read_active_workspace_slug(&nexus_dir(home.path()), "crt_alpha").as_deref(),
            Some("team-one"),
            "a malformed provider table must not fall back to the default workspace"
        );
        assert!(
            read_memory_provider_config(&nexus_dir(home.path())).is_err(),
            "provider selection itself must refuse the malformed table"
        );
    }
}

#[cfg(unix)]
#[test]
fn credential_symlink_is_refused() {
    let home = home();
    let target = write_credential(home.path(), "target", &valid_credential_json(), 0o600);
    let link = credentials_dir(home.path()).join("link.json");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    let error = MemoryCredential::load(home.path(), "link").expect_err("symlink must be refused");
    assert_credential_refused(error);
}

#[cfg(unix)]
#[test]
fn credential_with_broader_permissions_is_refused() {
    let home = home();
    write_credential(home.path(), CREDENTIAL_ID, &valid_credential_json(), 0o644);

    let error =
        MemoryCredential::load(home.path(), CREDENTIAL_ID).expect_err("0644 must be refused");
    assert_credential_refused(error);
}

#[cfg(unix)]
#[test]
fn oversized_credential_is_refused() {
    let home = home();
    let mut content = valid_credential_json();
    content.push_str(&" ".repeat(16 * 1024));
    write_credential(home.path(), CREDENTIAL_ID, &content, 0o600);

    let error =
        MemoryCredential::load(home.path(), CREDENTIAL_ID).expect_err("oversize must be refused");
    assert_credential_refused(error);
}

#[cfg(unix)]
#[test]
fn credential_errors_and_debug_never_echo_secret_material() {
    let home = home();

    // A readable refusal (broad permissions) must not interpolate the seed.
    write_credential(home.path(), CREDENTIAL_ID, &valid_credential_json(), 0o640);
    let error =
        MemoryCredential::load(home.path(), CREDENTIAL_ID).expect_err("0640 must be refused");
    assert!(
        !error.to_string().contains(SEED_HEX),
        "credential error leaked the seed: {error}"
    );

    // A parse refusal of a file carrying a seed-shaped value stays static.
    let malformed = format!(
        "{{\"identity_seed_hex\": \"{SEED_HEX}\", \"capability_token\": null, \"signature\": \"sig_{SEED_HEX}\"}}"
    );
    write_credential(home.path(), CREDENTIAL_ID, &malformed, 0o600);
    let error = MemoryCredential::load(home.path(), CREDENTIAL_ID)
        .expect_err("unknown key must be refused");
    let text = error.to_string();
    assert!(
        !text.contains(SEED_HEX),
        "credential error leaked the seed: {text}"
    );
    assert!(
        !text.contains("sig_"),
        "credential error leaked the signature: {text}"
    );

    // Debug output of a successfully loaded credential is redacted.
    write_credential(home.path(), CREDENTIAL_ID, &valid_credential_json(), 0o600);
    let credential = MemoryCredential::load(home.path(), CREDENTIAL_ID).expect("valid credential");
    let debug = format!("{credential:?}");
    assert!(debug.contains("redacted"), "{debug}");
    assert!(!debug.contains(SEED_HEX), "Debug leaked the seed: {debug}");
}

#[cfg(unix)]
#[test]
fn well_formed_credential_loads() {
    let home = home();
    write_credential(home.path(), CREDENTIAL_ID, &valid_credential_json(), 0o600);

    MemoryCredential::load(home.path(), CREDENTIAL_ID).expect("0600 credential must load");

    // Missing and traversal-shaped ids are refused before any path join.
    let missing = MemoryCredential::load(home.path(), "absent")
        .expect_err("missing credential file is refused");
    assert!(!missing.to_string().contains("absent"), "{missing}");
    for bad_id in ["../escape", "/absolute", ""] {
        let error = MemoryCredential::load(home.path(), bad_id)
            .expect_err("malformed credential id must be refused");
        assert!(
            error.to_string().contains("credential_id") || error.to_string().contains("credential"),
            "{error}"
        );
    }
}

#[cfg(unix)]
#[test]
fn credential_uppercase_seed_hex_is_refused() {
    let home = home();
    let content = format!(
        "{{\"identity_seed_hex\": \"{}\", \"capability_token\": null}}",
        SEED_HEX.to_uppercase()
    );
    write_credential(home.path(), CREDENTIAL_ID, &content, 0o600);

    let error = MemoryCredential::load(home.path(), CREDENTIAL_ID)
        .expect_err("uppercase seed hex must be refused");
    assert!(
        !error.to_string().contains(&SEED_HEX.to_uppercase()),
        "credential error leaked the seed: {error}"
    );
}

#[cfg(unix)]
#[test]
fn non_regular_credential_path_is_refused() {
    // A socket stands in for a substituted special file: the descriptor decides
    // the refusal, never a pathname pre-check. A short home keeps the socket
    // path within `SUN_LEN`.
    let home = tempfile::Builder::new()
        .prefix("n3cred")
        .tempdir_in("/tmp")
        .expect("temp home");
    let dir = credentials_dir(home.path());
    std::fs::create_dir_all(&dir).expect("credential dir");
    let _listener =
        std::os::unix::net::UnixListener::bind(dir.join(format!("{CREDENTIAL_ID}.json")))
            .expect("socket at credential path");

    let error = MemoryCredential::load(home.path(), CREDENTIAL_ID)
        .expect_err("non-regular credential path must be refused");
    assert_credential_refused(error);
}

#[cfg(unix)]
#[test]
fn capability_proof_must_be_the_published_shape() {
    let home = home();

    // A non-null `capability_token` must deserialize as the published
    // `CapabilityTokenProof` (`{v, claims, sig}`) and `CapabilityClaims`:
    // required `iss`/`sub`/`aud` strings, string-array `capabilities`,
    // unsigned `exp`, typed optional `iat`/`jti`, unknown fields refused.
    let mut refused = vec![
        "{}".to_string(),
        "{\"unrelated\": true}".to_string(),
        "{\"v\": 1, \"claims\": {}, \"sig\": \"\"}".to_string(),
        "{\"v\": 1, \"claims\": {}}".to_string(),
        "{\"v\": \"1\", \"claims\": {}, \"sig\": \"c2ln\"}".to_string(),
        "{\"v\": 1, \"claims\": [], \"sig\": \"SIGMARKER\"}".to_string(),
        "{\"v\": 1, \"claims\": {}, \"sig\": 7}".to_string(),
        "{\"v\": 1, \"claims\": {}, \"sig\": \"c2ln\", \"extra\": 1}".to_string(),
    ];
    for claims in [
        "{}",
        "{\"iss\": \"i\"}",
        "{\"iss\": \"i\", \"sub\": \"s\"}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\"}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": []}",
        "{\"iss\": 1, \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": [], \"exp\": 1}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": \"memory\", \"exp\": 1}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": [1], \"exp\": 1}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": [], \"exp\": -1}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": [], \"exp\": 1.5}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": [], \"exp\": 1, \"iat\": \"x\"}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": [], \"exp\": 1, \"jti\": 7}",
        "{\"iss\": \"i\", \"sub\": \"s\", \"aud\": \"a\", \"capabilities\": [], \"exp\": 1, \"extra\": true}",
    ] {
        refused.push(format!(
            "{{\"v\": 1, \"claims\": {claims}, \"sig\": \"SIGMARKER\"}}"
        ));
    }

    for token in refused {
        let content =
            format!("{{\"identity_seed_hex\": \"{SEED_HEX}\", \"capability_token\": {token}}}");
        write_credential(home.path(), CREDENTIAL_ID, &content, 0o600);
        let error = MemoryCredential::load(home.path(), CREDENTIAL_ID)
            .expect_err("a malformed proof must be refused");
        assert!(
            !error.to_string().contains("SIGMARKER"),
            "proof error leaked the signature: {error}"
        );
        assert_credential_refused(error);
    }

    // The published shape loads, with and without the optional typed claims.
    for proof in [
        concat!(
            "{\"v\": 1, \"claims\": {\"iss\": \"12D3KooWIss\", ",
            "\"sub\": \"12D3KooWSub\", \"aud\": \"12D3KooWAud\", ",
            "\"capabilities\": [\"memory\"], \"exp\": 4102444800}, ",
            "\"sig\": \"c2ln\"}"
        ),
        concat!(
            "{\"v\": 1, \"claims\": {\"iss\": \"12D3KooWIss\", ",
            "\"sub\": \"12D3KooWSub\", \"aud\": \"12D3KooWAud\", ",
            "\"capabilities\": [\"memory\", \"spoke-baseline\"], ",
            "\"exp\": 4102444800, \"iat\": 4102444700, \"jti\": \"tok-1\"}, ",
            "\"sig\": \"c2ln\"}"
        ),
    ] {
        let content =
            format!("{{\"identity_seed_hex\": \"{SEED_HEX}\", \"capability_token\": {proof}}}");
        write_credential(home.path(), CREDENTIAL_ID, &content, 0o600);
        MemoryCredential::load(home.path(), CREDENTIAL_ID)
            .unwrap_or_else(|error| panic!("the published proof shape must load: {error:?}"));
    }
}

#[cfg(unix)]
#[test]
fn missing_capability_token_field_is_refused() {
    let home = home();
    // §3.1 declares the field; omitting it is a malformed document, not `null`.
    let content = format!("{{\"identity_seed_hex\": \"{SEED_HEX}\"}}");
    write_credential(home.path(), CREDENTIAL_ID, &content, 0o600);

    let error = MemoryCredential::load(home.path(), CREDENTIAL_ID)
        .expect_err("the declared capability_token field is required");
    assert_credential_refused(error);
}
