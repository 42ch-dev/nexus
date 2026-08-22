//! `nexus42 capability` CLI contract tests (V1.172 P2, AR-41).
//!
//! Hermetic daemon-free scenarios: hidden group surface, exit-code
//! vocabulary (0 ok / 2 validation / 3 pairing), and the install trio into
//! `~/.nexus42/capabilities/<name>/` (AR-35). `capability list` daemon
//! scenarios are covered in-module with wiremock (`commands/capability.rs`
//! tests).

use assert_cmd::Command;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::path::Path;

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

fn nexus42() -> Command {
    Command::cargo_bin("nexus42").expect("nexus42 binary")
}

/// Write a descriptor + module pair (manifest + `<module-id>.wasm`) under
/// `root` and return the descriptor path.
fn write_trio(root: &Path, name: &str, module_id: &str, wasm_bytes: &[u8]) -> std::path::PathBuf {
    let sha = sha256_hex(wasm_bytes);
    let module_dir = root.join("module");
    std::fs::create_dir_all(&module_dir).unwrap();
    std::fs::write(
        module_dir.join("manifest.json"),
        json!({
            "module_id": module_id,
            "name": "Test Module",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "",
            "wasm_sha256": sha,
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(module_dir.join(format!("{module_id}.wasm")), wasm_bytes).unwrap();
    let descriptor = root.join("capability.json");
    std::fs::write(
        &descriptor,
        json!({
            "name": name,
            "inputSchema": "{\"type\":\"object\"}",
            "outputSchema": "{\"type\":\"object\"}",
            "wasm": { "moduleId": module_id, "wasmSha256": sha },
        })
        .to_string(),
    )
    .unwrap();
    descriptor
}

#[test]
fn capability_group_is_hidden_from_root_help() {
    // V1.35 lock posture (AR-41): callable but NOT advertised as a
    // top-level Commands entry (the word `capability` legitimately appears
    // in the `acp` description).
    let output = nexus42()
        .arg("--help")
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help_text = String::from_utf8(output).unwrap();
    let commands_section = help_text
        .split("Commands:")
        .nth(1)
        .expect("Commands: section present in --help");
    assert!(
        !commands_section.contains("\n  capability"),
        "top-level 'capability' must be hidden from the Commands list"
    );
}

#[test]
fn capability_group_help_lists_only_validate_list_install() {
    let output = nexus42()
        .args(["capability", "--help"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let help = String::from_utf8(output).unwrap();
    let commands_section = help
        .split("Commands:")
        .nth(1)
        .expect("Commands: section present");
    for sub in ["validate", "list", "install"] {
        assert!(
            commands_section.contains(sub),
            "capability --help must list '{sub}'"
        );
    }
    assert!(
        !commands_section.contains("run"),
        "no `run` subcommand (PL-7)"
    );
    assert!(
        !commands_section.contains("scaffold"),
        "no `scaffold` subcommand (PL-7)"
    );
}

#[test]
fn capability_validate_ok_exits_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let descriptor = write_trio(dir.path(), "demo.pull", "basic-mod", b"wasm module bytes");

    nexus42()
        .env("HOME", dir.path())
        .args(["capability", "validate", "--descriptor"])
        .arg(&descriptor)
        .assert()
        .success();
}

#[test]
fn capability_validate_with_module_pair_ok_exits_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let descriptor = write_trio(dir.path(), "demo.pull", "basic-mod", b"wasm module bytes");

    nexus42()
        .env("HOME", dir.path())
        .args(["capability", "validate", "--descriptor"])
        .arg(&descriptor)
        .args(["--module"])
        .arg(dir.path().join("module"))
        .assert()
        .success();
}

#[test]
fn capability_validate_invalid_exits_two_with_json() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Uppercase segment violates the AR-34 name contract.
    let descriptor = dir.path().join("capability.json");
    std::fs::write(
        &descriptor,
        json!({
            "name": "Bad.Name",
            "inputSchema": "{\"type\":\"object\"}",
            "outputSchema": "{\"type\":\"object\"}",
            "wasm": { "moduleId": "basic-mod", "wasmSha256": sha256_hex(b"x") },
        })
        .to_string(),
    )
    .unwrap();

    nexus42()
        .env("HOME", dir.path())
        .args(["capability", "validate", "--json", "--descriptor"])
        .arg(&descriptor)
        .assert()
        .code(2)
        .stdout(predicates::str::contains("\"valid\": false"));
}

#[test]
fn capability_validate_pairing_mismatch_exits_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Manifest declares the real wasm hash; the descriptor declares a
    // different one → descriptor-vs-manifest mismatch (exit 3).
    let wasm_bytes = b"wasm module bytes";
    let real_sha = sha256_hex(wasm_bytes);
    let module_dir = dir.path().join("module");
    std::fs::create_dir_all(&module_dir).unwrap();
    std::fs::write(
        module_dir.join("manifest.json"),
        json!({
            "module_id": "basic-mod",
            "name": "Test Module",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "",
            "wasm_sha256": real_sha,
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(module_dir.join("basic-mod.wasm"), wasm_bytes).unwrap();
    let descriptor = dir.path().join("capability.json");
    std::fs::write(
        &descriptor,
        json!({
            "name": "demo.pull",
            "inputSchema": "{\"type\":\"object\"}",
            "outputSchema": "{\"type\":\"object\"}",
            "wasm": { "moduleId": "basic-mod", "wasmSha256": sha256_hex(b"other bytes") },
        })
        .to_string(),
    )
    .unwrap();

    nexus42()
        .env("HOME", dir.path())
        .args(["capability", "validate", "--descriptor"])
        .arg(&descriptor)
        .args(["--module"])
        .arg(&module_dir)
        .assert()
        .code(3);
}

#[test]
fn capability_install_copies_trio_to_capabilities_dir() {
    let dir = tempfile::tempdir().expect("tempdir");
    let descriptor = write_trio(dir.path(), "demo.pull", "basic-mod", b"wasm module bytes");
    let module_dir = dir.path().join("module");

    nexus42()
        .env("HOME", dir.path())
        .args(["capability", "install", "--descriptor"])
        .arg(&descriptor)
        .args(["--wasm"])
        .arg(module_dir.join("basic-mod.wasm"))
        .args(["--manifest"])
        .arg(module_dir.join("manifest.json"))
        .assert()
        .success();

    let cap_dir = dir
        .path()
        .join(".nexus42")
        .join("capabilities")
        .join("demo.pull");
    assert!(
        cap_dir.join("capability.json").is_file(),
        "capability.json installed"
    );
    assert!(
        cap_dir.join("manifest.json").is_file(),
        "manifest.json installed"
    );
    assert!(
        cap_dir.join("basic-mod.wasm").is_file(),
        "<module-id>.wasm installed (AR-35 trio)"
    );
}

#[test]
fn capability_install_rejects_pairing_mismatch_exits_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Manifest declares a hash that does not match the wasm bytes.
    let wasm_bytes = b"wasm module bytes";
    let module_dir = dir.path().join("module");
    std::fs::create_dir_all(&module_dir).unwrap();
    std::fs::write(
        module_dir.join("manifest.json"),
        json!({
            "module_id": "basic-mod",
            "name": "Test Module",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "",
            "wasm_sha256": sha256_hex(b"other bytes"),
        })
        .to_string(),
    )
    .unwrap();
    std::fs::write(module_dir.join("basic-mod.wasm"), wasm_bytes).unwrap();
    let descriptor = dir.path().join("capability.json");
    std::fs::write(
        &descriptor,
        json!({
            "name": "demo.pull",
            "inputSchema": "{\"type\":\"object\"}",
            "outputSchema": "{\"type\":\"object\"}",
            "wasm": { "moduleId": "basic-mod", "wasmSha256": sha256_hex(b"other bytes") },
        })
        .to_string(),
    )
    .unwrap();

    nexus42()
        .env("HOME", dir.path())
        .args(["capability", "install", "--descriptor"])
        .arg(&descriptor)
        .args(["--wasm"])
        .arg(module_dir.join("basic-mod.wasm"))
        .args(["--manifest"])
        .arg(module_dir.join("manifest.json"))
        .assert()
        .code(3);
}
