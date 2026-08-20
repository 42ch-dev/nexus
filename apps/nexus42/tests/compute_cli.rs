//! `nexus42 compute` CLI contract tests (V1.170 P0, AR-9).
//!
//! Hermetic daemon-free scenarios: exit-code vocabulary (0 ok / 2 validation
//! / 3 pairing), install identity + absent-hash rejections, and the
//! `--output text|json` clap constraint. `compute run` daemon scenarios are
//! covered in-module with wiremock (`commands/compute/mod.rs` tests).

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::json;
use std::path::Path;

/// Write a manifest JSON to `dir` and return its path.
fn write_manifest(dir: &Path, module_id: &str, extra: &serde_json::Value) -> std::path::PathBuf {
    let mut manifest = json!({
        "module_id": module_id,
        "name": "Test Module",
        "version": "1.0.0",
        "nexus_abi_version": 1,
        "required_key_block_types": ["character"],
        "compute_export": "compute",
        "init_export": "init",
    });
    if let Some(obj) = manifest.as_object_mut() {
        if let Some(extra_obj) = extra.as_object() {
            for (k, v) in extra_obj {
                obj.insert(k.clone(), v.clone());
            }
        }
    }
    let path = dir.join("manifest.json");
    std::fs::write(&path, serde_json::to_vec(&manifest).expect("json")).expect("write manifest");
    path
}

fn nexus42() -> Command {
    Command::cargo_bin("nexus42").expect("nexus42 binary")
}

#[test]
fn compute_validate_ok_exits_zero() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = write_manifest(dir.path(), "basic-combat", &json!({}));

    nexus42()
        .env("HOME", dir.path())
        .args(["compute", "validate", "--manifest"])
        .arg(&manifest)
        .assert()
        .success();
}

#[test]
fn compute_validate_invalid_exits_two() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = write_manifest(dir.path(), "basic-combat", &json!({"nexus_abi_version": 2}));

    nexus42()
        .env("HOME", dir.path())
        .args(["compute", "validate", "--manifest"])
        .arg(&manifest)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("manifest validation failed"));
}

#[test]
fn compute_validate_pairing_mismatch_exits_three() {
    let dir = tempfile::tempdir().expect("tempdir");
    // A well-formed hash that does not match the wasm bytes.
    let manifest = write_manifest(
        dir.path(),
        "basic-combat",
        &json!({"wasm_sha256": "0".repeat(64)}),
    );
    let wasm = dir.path().join("module.wasm");
    std::fs::write(&wasm, b"some wasm bytes").expect("write wasm");

    nexus42()
        .env("HOME", dir.path())
        .args(["compute", "validate", "--manifest"])
        .arg(&manifest)
        .args(["--wasm"])
        .arg(&wasm)
        .assert()
        .code(3)
        .stdout(predicate::str::contains(
            "wasm does not match manifest wasm_sha256",
        ));
}

#[test]
fn compute_install_rejects_module_id_mismatch() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = write_manifest(dir.path(), "basic-combat", &json!({}));
    let wasm = dir.path().join("module.wasm");
    std::fs::write(&wasm, b"wasm bytes").expect("write wasm");

    nexus42()
        .env("HOME", dir.path())
        .args(["compute", "install", "--module-id", "alias", "--manifest"])
        .arg(&manifest)
        .args(["--wasm"])
        .arg(&wasm)
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "does not match manifest.module_id",
        ));
}

#[test]
fn compute_install_rejects_absent_hash() {
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = write_manifest(dir.path(), "basic-combat", &json!({}));
    let wasm = dir.path().join("module.wasm");
    std::fs::write(&wasm, b"wasm bytes").expect("write wasm");

    nexus42()
        .env("HOME", dir.path())
        .args([
            "compute",
            "install",
            "--module-id",
            "basic-combat",
            "--manifest",
        ])
        .arg(&manifest)
        .args(["--wasm"])
        .arg(&wasm)
        .assert()
        .code(3)
        .stderr(predicate::str::contains("manifest has no wasm_sha256"));
}

#[test]
fn output_flag_rejects_non_text_json_values() {
    // I9: the global `--output` flag is constrained to `text|json` — clap
    // rejects anything else before command dispatch (exit 2).
    let dir = tempfile::tempdir().expect("tempdir");
    let manifest = write_manifest(dir.path(), "basic-combat", &json!({}));

    nexus42()
        .env("HOME", dir.path())
        .args(["--output", "xml", "compute", "validate", "--manifest"])
        .arg(&manifest)
        .assert()
        .code(2)
        .stderr(predicate::str::contains("invalid value 'xml'"));
}
