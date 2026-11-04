//! `creator memory provider` selection surface (NXM-9 Task 1).
//!
//! The real `nexus42` binary runs against a hermetic `HOME`: no daemon, no
//! creator, no credential file. The command persists only the non-secret
//! `[memory_provider]` table and never accepts a secret argument.

use assert_cmd::Command;
use std::path::PathBuf;
use std::process::Output;
use tempfile::TempDir;

const SPACE_ID: &str = "space-alpha";
const CREDENTIAL_ID: &str = "cred-alpha";
const PUBKEY_HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const PEER: &str = "12D3KooWPeer";

fn cli(home: &TempDir) -> Command {
    let mut command = Command::cargo_bin("nexus42").expect("nexus42 binary");
    command.env("HOME", home.path()).env("RUST_LOG", "off");
    command
}

fn config_path(home: &TempDir) -> PathBuf {
    home.path().join(".nexus42").join("config.toml")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn show_reports_implicit_local_without_writing_config() {
    let home = tempfile::tempdir().expect("temp home");

    let output = cli(&home)
        .args(["creator", "memory", "provider", "show"])
        .output()
        .expect("run provider show");

    assert!(output.status.success(), "stderr: {}", stderr(&output));
    assert!(
        stdout(&output).contains("memory_provider: local"),
        "stdout: {}",
        stdout(&output)
    );
    assert!(
        !config_path(&home).exists(),
        "show must not write the selection"
    );
}

#[test]
fn use_local_then_show_persists_explicit_local() {
    let home = tempfile::tempdir().expect("temp home");

    let use_output = cli(&home)
        .args(["creator", "memory", "provider", "use", "local"])
        .output()
        .expect("run provider use local");
    assert!(use_output.status.success(), "stderr: {}", stderr(&use_output));

    let text = std::fs::read_to_string(config_path(&home)).expect("config written");
    assert!(text.contains("[memory_provider]"), "{text}");
    assert!(text.contains("kind = \"local\""), "{text}");

    let show_output = cli(&home)
        .args(["creator", "memory", "provider", "show"])
        .output()
        .expect("run provider show");
    assert!(show_output.status.success(), "stderr: {}", stderr(&show_output));
    assert!(
        stdout(&show_output).contains("memory_provider: local"),
        "stdout: {}",
        stdout(&show_output)
    );
}

#[test]
fn use_remote_persists_non_secret_selection_and_preserves_other_keys() {
    let home = tempfile::tempdir().expect("temp home");
    std::fs::create_dir_all(home.path().join(".nexus42")).expect("nexus dir");
    std::fs::write(
        config_path(&home),
        "active_creator_id = \"crt_alpha\"\n\n[agent_profile]\nmodel = \"m1\"\n",
    )
    .expect("seed config");

    let output = cli(&home)
        .args([
            "creator",
            "memory",
            "provider",
            "use",
            "remote",
            "--space-id",
            SPACE_ID,
            "--credential-id",
            CREDENTIAL_ID,
            "--connect-endpoint",
            "ws://127.0.0.1:8420/memory",
            "--remote-pubkey-hex",
            PUBKEY_HEX,
            "--allowlist",
            PEER,
        ])
        .output()
        .expect("run provider use remote");
    assert!(output.status.success(), "stderr: {}", stderr(&output));

    let text = std::fs::read_to_string(config_path(&home)).expect("config written");
    for expected in [
        "active_creator_id = \"crt_alpha\"",
        "[agent_profile]",
        "model = \"m1\"",
        "[memory_provider]",
        "kind = \"remote\"",
        SPACE_ID,
        CREDENTIAL_ID,
        PUBKEY_HEX,
        PEER,
        "ws://127.0.0.1:8420/memory",
    ] {
        assert!(text.contains(expected), "missing {expected}: {text}");
    }
    assert!(
        !text.contains("identity_seed") && !text.contains("capability_token"),
        "selection must persist no secret material: {text}"
    );

    let show_output = cli(&home)
        .args(["creator", "memory", "provider", "show"])
        .output()
        .expect("run provider show");
    assert!(
        show_output.status.success(),
        "stderr: {}",
        stderr(&show_output)
    );
    let shown = stdout(&show_output);
    for expected in ["memory_provider: remote", SPACE_ID, CREDENTIAL_ID, PUBKEY_HEX, PEER] {
        assert!(shown.contains(expected), "missing {expected}: {shown}");
    }
}

#[test]
fn use_remote_rejects_plain_ws_to_a_non_loopback_host_without_writing() {
    let home = tempfile::tempdir().expect("temp home");

    let output = cli(&home)
        .args([
            "creator",
            "memory",
            "provider",
            "use",
            "remote",
            "--space-id",
            SPACE_ID,
            "--credential-id",
            CREDENTIAL_ID,
            "--connect-endpoint",
            "ws://example.com:8420",
            "--remote-pubkey-hex",
            PUBKEY_HEX,
            "--allowlist",
            PEER,
        ])
        .output()
        .expect("run provider use remote");

    assert!(!output.status.success(), "plain ws must be refused");
    assert!(
        stderr(&output).contains("wss://"),
        "stderr: {}",
        stderr(&output)
    );
    assert!(
        !config_path(&home).exists(),
        "a refused selection writes nothing"
    );
}
