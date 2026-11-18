//! Consumer for the packed `nexus-module-manifest` archive: validate a manifest,
//! inject the `wasm_sha256` of a staged artifact, verify the digest pairing, and
//! refuse an invalid corpus entry through the real `validate`.

use std::fs;
use std::path::{Path, PathBuf};

use nexus_module_manifest::{inject_wasm_sha256, ModuleManifest};

/// Staged artifact bytes. Not a runnable module: only the digest pairing is
/// under test here, and fixed bytes keep the expected hash a constant.
const WASM_BYTES: &[u8] = b"\0asm\x01\0\0\0";

/// Independent known-answer oracle for [`WASM_BYTES`]: the value was computed
/// outside the API under test, so a consistently wrong digest cannot pass.
const WASM_SHA256: &str = "93a44bbb96c751218e4c00d479e4c14358122a389acca16205b1e4d0dc5f9476";

const VALID: &str = r#"{
  "module_id": "consumer-probe",
  "name": "Consumer Probe",
  "version": "1.0.0",
  "nexus_abi_version": 1,
  "required_key_block_types": [],
  "compute_export": "compute",
  "init_export": "init"
}"#;

/// One entry of the committed invalid corpus
/// (`modules/nexus-module-manifest/tests/fixtures/invalid-manifests.json`,
/// label "wrong ABI version"): the DR-49 pin refuses ABI V2 concepts.
const INVALID_WRONG_ABI: &str = r#"{
  "module_id": "m",
  "name": "M",
  "version": "1.0.0",
  "nexus_abi_version": 2,
  "required_key_block_types": [],
  "compute_export": "compute",
  "init_export": "init"
}"#;

fn scratch_root() -> PathBuf {
    std::env::temp_dir().join(format!(
        "nexus-module-manifest-consumer-{}",
        std::process::id()
    ))
}

fn parse(json: &str) -> ModuleManifest {
    serde_json::from_str(json).expect("manifest parses through the contract types")
}

fn main() {
    let root = scratch_root();
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).expect("scratch root is creatable");
    let wasm_path: &Path = &root.join("probe.wasm");
    let manifest_path = root.join("manifest.json");
    fs::write(wasm_path, WASM_BYTES).expect("staged wasm is writable");
    fs::write(&manifest_path, VALID).expect("staged manifest is writable");

    // A legacy manifest with no digest declares nothing to verify.
    let legacy = parse(VALID);
    legacy.validate().expect("a V1 manifest validates");
    assert_eq!(legacy.wasm_sha256, None);
    legacy
        .verify_wasm_sha256(WASM_BYTES)
        .expect("a manifest without wasm_sha256 skips verification");

    // Injection writes the archive-independent digest of the staged artifact.
    inject_wasm_sha256("consumer-probe", wasm_path, &manifest_path)
        .expect("inject_wasm_sha256 succeeds");
    let injected_json = fs::read_to_string(&manifest_path).expect("staged manifest is readable");
    let injected = parse(&injected_json);
    assert_eq!(injected.wasm_sha256.as_deref(), Some(WASM_SHA256));
    injected
        .validate()
        .expect("the injected digest stays a valid manifest");
    injected
        .verify_wasm_sha256(WASM_BYTES)
        .expect("the injected digest matches the staged artifact");

    // A swapped artifact is refused by the same pairing check.
    let mismatch = injected
        .verify_wasm_sha256(b"\0asm\x01\0\0\0 swapped")
        .expect_err("a different artifact must not verify");
    assert!(
        mismatch.contains("does not match manifest wasm_sha256"),
        "{mismatch}"
    );

    // The invalid corpus entry is refused by `validate`, not by the parser.
    let invalid = parse(INVALID_WRONG_ABI);
    let errors = invalid
        .validate()
        .expect_err("an ABI V2 manifest must be refused");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("nexus_abi_version must be 1")),
        "{errors:?}"
    );

    let _ = fs::remove_dir_all(&root);
    println!(
        "nexus-module-manifest validated, injected {WASM_SHA256}, verified the pairing, and refused the invalid corpus entry"
    );
}
