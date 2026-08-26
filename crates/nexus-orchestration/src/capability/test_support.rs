//! Shared test-support fixture for the capability module (qc1 S-1,
//! V1.176 P1 QC wave).
//!
//! One `<name>/capability.json` trio writer + hex helper used by the three
//! `nexus-orchestration` test modules (`capability::mod`, `scan`, `watch`)
//! — previously each carried a byte-identical copy. Cross-crate copies
//! (daemon-runtime integration tests, nexus42 journeys) remain: those are
//! separate crates and a feature-gated public helper is the documented
//! follow-up if the copies keep churning.
#![cfg(test)]

use sha2::Digest;
use std::fmt::Write as _;
use std::path::Path;

/// Lowercase hex digest of `bytes` (64 chars for a SHA-256).
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let digest = sha2::Sha256::digest(bytes);
    let mut hex = String::with_capacity(64);
    for b in digest {
        let _ = write!(hex, "{b:02x}");
    }
    hex
}

/// Write `<dir>/manifest.json` + `<dir>/basic-combat.wasm` with a **real**
/// matching sha (the AR-39 pairing admission verifies). Returns the sha.
#[must_use]
pub fn write_module_pair(dir: &Path) -> String {
    write_module_pair_with_bytes(dir, b"fake module bytes")
}

/// Like [`write_module_pair`] but with caller-chosen wasm bytes — the
/// hot-reload edit tests need a trio whose digest differs from the
/// fixture's constant bytes while the sha pairing still passes.
#[must_use]
pub fn write_module_pair_with_bytes(dir: &Path, wasm: &[u8]) -> String {
    let sha = sha256_hex(wasm);
    std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
    let manifest = format!(
        r#"{{
            "module_id": "basic-combat",
            "name": "Basic Combat",
            "version": "1.0.0",
            "nexus_abi_version": 1,
            "required_key_block_types": [],
            "compute_export": "compute",
            "init_export": "",
            "wasm_sha256": "{sha}"
        }}"#
    );
    std::fs::write(dir.join("manifest.json"), manifest).unwrap();
    sha
}

/// Descriptor JSON declaring `name` with the given `wasm_sha256`.
#[must_use]
pub fn descriptor_json(name: &str, sha: &str) -> String {
    format!(
        r#"{{
            "name": "{name}",
            "inputSchema": "{{\"type\":\"object\"}}",
            "outputSchema": "{{\"type\":\"object\"}}",
            "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
        }}"#
    )
}

/// Write an admitted `<name>/capability.json` trio (AR-35 layout): a
/// hash-consistent `manifest.json` + `<module-id>.wasm` pair so the AR-43
/// admission gates pass inside the scan.
pub fn write_capability_dir(root: &Path, name: &str) {
    let dir = root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let sha = write_module_pair(&dir);
    std::fs::write(dir.join("capability.json"), descriptor_json(name, &sha)).unwrap();
}
