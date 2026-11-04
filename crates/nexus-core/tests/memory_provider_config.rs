//! `[memory_provider]` selection, shared-lock writer and credential loader
//! (NXM-9 Task 1; `n3-provider-binding.md` §2–§3).
//!
//! Every case drives the real core API on a temporary home: no daemon, no
//! creator, no live provider. Credential fixtures are synthetic — no real auth
//! material is read or logged.

use nexus_core::{
    CoreError, CoreHomeService, MemoryCredential, MemoryProviderConfig, MemoryProviderSelection,
    RemoteMemoryProviderSelection,
};
use nexus_home_layout::active_context::CliConfigSnapshot;
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
    let snapshot = CliConfigSnapshot::load(&nexus_dir(home)).expect("config loads");
    MemoryProviderSelection::from_config(snapshot.memory_provider).expect("selection converts")
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
    for preserved in ["active_creator_id = \"crt_alpha\"", "[agent_profile]", "model = \"m1\""] {
        assert!(text.contains(preserved), "lost other writer key {preserved}: {text}");
    }
    assert!(text.contains("kind = \"remote\""), "{text}");
    assert!(text.contains(CREDENTIAL_ID), "{text}");
    assert!(!text.contains(SEED_HEX), "selection must never persist a seed: {text}");
    assert_eq!(read_selection(home.path()), selection);
}

#[test]
fn malformed_credential_id_is_rejected_without_writing() {
    let home = home();
    let service = CoreHomeService::open(home.path().to_path_buf()).expect("home entry");

    for bad in ["../escape", "", "-leading", "a b", &"x".repeat(65)] {
        let selection = MemoryProviderSelection::Remote(remote(
            bad,
            "ws://127.0.0.1:8420/memory",
            PUBKEY_HEX,
        ));
        let error = service
            .set_memory_provider_selection(&selection)
            .expect_err("malformed credential id must be refused");
        assert_invalid_field(error, "credential_id");
    }
    assert!(!config_path(home.path()).exists(), "a refused selection writes nothing");
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
        "wss:///ws",
    ] {
        let error = MemoryProviderSelection::from_config(Some(remote_config(endpoint)))
            .expect_err("malformed endpoint must be refused");
        assert_invalid_field(error, "connect_endpoint");
    }

    // Loopback ws:// (tests) and wss:// remain selectable.
    for endpoint in ["ws://127.0.0.1:8420/memory", "ws://[::1]:8420", "ws://localhost:1", "wss://memory.example.com/ws"] {
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
        (SPACE_ID, "0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF0123456789ABCDEF", "remote_pubkey_hex"),
        (SPACE_ID, "g123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef", "remote_pubkey_hex"),
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

#[cfg(unix)]
#[test]
fn credential_symlink_is_refused() {
    let home = home();
    let target = write_credential(home.path(), "target", &valid_credential_json(), 0o600);
    let link = credentials_dir(home.path()).join("link.json");
    std::os::unix::fs::symlink(&target, &link).expect("symlink");

    let error = MemoryCredential::load(home.path(), "link").expect_err("symlink must be refused");
    assert!(
        error.to_string().contains("symlink"),
        "expected a symlink refusal, got {error}"
    );
}

#[cfg(unix)]
#[test]
fn credential_with_broader_permissions_is_refused() {
    let home = home();
    write_credential(home.path(), CREDENTIAL_ID, &valid_credential_json(), 0o644);

    let error =
        MemoryCredential::load(home.path(), CREDENTIAL_ID).expect_err("0644 must be refused");
    assert!(
        error.to_string().contains("0600"),
        "expected a permissions refusal, got {error}"
    );
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
    assert!(
        error.to_string().contains("16384"),
        "expected a size refusal, got {error}"
    );
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
    let error =
        MemoryCredential::load(home.path(), CREDENTIAL_ID).expect_err("unknown key must be refused");
    let text = error.to_string();
    assert!(!text.contains(SEED_HEX), "credential error leaked the seed: {text}");
    assert!(!text.contains("sig_"), "credential error leaked the signature: {text}");

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
