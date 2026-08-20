//! Shared compute-module manifest contract + `wasm_sha256` injection.
//!
//! Extracted from `crates/nexus-wasm-host` (V1.170 P0, AR-8) so the host, the
//! `nexus42 compute` CLI, and module tooling share ONE manifest contract and
//! ONE content-hash injection implementation:
//!
//! - [`manifest`] — `ModuleManifest` / `HostFunction` / `ModuleSchemas` types
//!   plus the validation core (`allows`, `verify_wasm_sha256`), moved verbatim
//!   from `crates/nexus-wasm-host/src/manifest.rs`.
//! - [`inject_wasm_sha256`] — the staged-manifest hash injection moved from
//!   `crates/nexus-wasm-host/build.rs`.
//!
//! The crate is standalone (non-workspace, publishable, AR-1): the root
//! workspace `exclude`s it so workspace members can path-depend on it.

mod manifest;

pub use manifest::{HostFunction, ModuleManifest, ModuleSchemas};

use std::fs;
use std::path::Path;

use serde_json::Value;
use sha2::{Digest, Sha256};

/// Lowercase hex encoding (avoids the `format_collect` lint and a hex dep).
fn hex_encode(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Inject `wasm_sha256` into a staged `manifest.json`, computed from the
/// staged `.wasm` bytes.
///
/// Semantics are identical to the original `nexus-wasm-host` build.rs helper
/// (V1.170 P0, AR-8): the manifest at `manifest_path` is rewritten in place
/// with a `wasm_sha256` field set to the lowercase-hex SHA-256 of the bytes
/// at `wasm_path`. The embedded/staged manifest always declares the hash of
/// the artifact it ships with, so a content-based pairing check can never
/// reject the pair. The source manifest is left untouched by callers (they
/// stage a copy first); if the source carries a `wasm_sha256` of its own,
/// this value (derived from the actual compiled bytes) wins in the staged
/// copy.
///
/// # Errors
///
/// Returns a descriptive message when the wasm or manifest cannot be read,
/// parsed, serialized, or written.
pub fn inject_wasm_sha256(id: &str, wasm_path: &Path, manifest_path: &Path) -> Result<(), String> {
    let wasm_bytes = fs::read(wasm_path).map_err(|e| {
        format!(
            "module `{id}`: read embedded wasm {}: {e}",
            wasm_path.display()
        )
    })?;
    let digest = Sha256::digest(&wasm_bytes);
    let hex = hex_encode(&digest);
    let manifest_bytes = fs::read(manifest_path).map_err(|e| {
        format!(
            "module `{id}`: read embedded manifest {}: {e}",
            manifest_path.display()
        )
    })?;
    let mut value: Value = serde_json::from_slice(&manifest_bytes).map_err(|e| {
        format!(
            "module `{id}`: parse embedded manifest {}: {e}",
            manifest_path.display()
        )
    })?;
    value["wasm_sha256"] = Value::String(hex);
    let out = serde_json::to_string_pretty(&value).map_err(|e| {
        format!(
            "module `{id}`: serialize embedded manifest {}: {e}",
            manifest_path.display()
        )
    })?;
    fs::write(manifest_path, out).map_err(|e| {
        format!(
            "module `{id}`: write embedded manifest {}: {e}",
            manifest_path.display()
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The staged manifest must end up with the lowercase-hex SHA-256 of the
    /// staged wasm bytes, with the source manifest left untouched and any
    /// source `wasm_sha256` overridden by the computed value.
    #[test]
    fn inject_wasm_sha256_stages_computed_hash() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wasm_path = dir.path().join("mod.wasm");
        let manifest_path = dir.path().join("manifest.json");
        fs::write(&wasm_path, b"some wasm bytes").expect("write wasm");
        fs::write(
            &manifest_path,
            r#"{
                "module_id": "mod",
                "name": "Mod",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "init",
                "wasm_sha256": "stale-source-value"
            }"#,
        )
        .expect("write manifest");

        inject_wasm_sha256("mod", &wasm_path, &manifest_path).expect("inject succeeds");

        let staged: Value =
            serde_json::from_slice(&fs::read(&manifest_path).expect("read staged")).unwrap();
        let expected = hex_encode(&Sha256::digest(b"some wasm bytes"));
        assert_eq!(
            staged["wasm_sha256"].as_str(),
            Some(expected.as_str()),
            "injected value must be the computed hash, overriding the source value"
        );
        assert_eq!(staged["module_id"].as_str(), Some("mod"));
    }

    /// The injected manifest must be byte-stable: re-injecting the same pair
    /// produces identical bytes (the byte-identical bar for the AR-8
    /// refactor — same field order, same serialization).
    #[test]
    fn inject_wasm_sha256_is_byte_stable() {
        let dir = tempfile::tempdir().expect("tempdir");
        let wasm_path = dir.path().join("mod.wasm");
        let manifest_path = dir.path().join("manifest.json");
        fs::write(&wasm_path, b"stable bytes").expect("write wasm");
        fs::write(
            &manifest_path,
            r#"{"module_id":"mod","name":"Mod","version":"1.0.0","nexus_abi_version":1,"required_key_block_types":[],"compute_export":"compute","init_export":"init"}"#,
        )
        .expect("write manifest");

        inject_wasm_sha256("mod", &wasm_path, &manifest_path).expect("first inject");
        let first = fs::read(&manifest_path).expect("read first");
        inject_wasm_sha256("mod", &wasm_path, &manifest_path).expect("second inject");
        let second = fs::read(&manifest_path).expect("read second");

        assert_eq!(first, second, "re-injection must be byte-identical");
    }

    #[test]
    fn inject_wasm_sha256_reports_missing_wasm() {
        let dir = tempfile::tempdir().expect("tempdir");
        let err = inject_wasm_sha256(
            "mod",
            &dir.path().join("missing.wasm"),
            &dir.path().join("manifest.json"),
        )
        .expect_err("missing wasm must fail");
        assert!(err.contains("module `mod`: read embedded wasm"), "{err}");
    }
}
