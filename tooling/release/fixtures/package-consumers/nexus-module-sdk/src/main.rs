//! Host-side API consumer for the packed `nexus-module-sdk` archive.
//!
//! Compiled for the host, separately from the `guest` member: the guest is the
//! only thing that ever becomes a `.wasm` artifact, and nothing host-only is
//! declared alongside it.

use nexus_module_sdk::{ModuleError, ModuleManifest};

/// A manifest the real `validate` accepts.
const MANIFEST: &str = r#"{
  "module_id": "consumer-probe",
  "name": "Consumer Probe",
  "version": "1.0.0",
  "nexus_abi_version": 1,
  "required_key_block_types": [],
  "compute_export": "compute",
  "init_export": "init"
}"#;

fn main() {
    let manifest: ModuleManifest =
        serde_json::from_str(MANIFEST).expect("the V1 manifest parses through the SDK types");
    manifest.validate().expect("a V1 manifest validates");
    assert_eq!(manifest.module_id, "consumer-probe");
    assert_eq!(manifest.compute_export, "compute");

    // ABI error sentinels: the host maps a module's negative `compute` return.
    assert_eq!(ModuleError::InputMalformed.to_compute_return(), -1);
    assert_eq!(ModuleError::OutputTooSmall.to_compute_return(), -2);

    // V2 concepts are refused by the same host-side `validate`.
    let mut v2 = manifest.clone();
    v2.nexus_abi_version = 2;
    let errors = v2
        .validate()
        .expect_err("an ABI V2 manifest must be refused");
    assert!(
        errors
            .iter()
            .any(|error| error.contains("nexus_abi_version must be 1")),
        "{errors:?}"
    );

    println!(
        "nexus-module-sdk host API validated {} and mapped sentinels; the guest builds separately for wasm32-unknown-unknown",
        manifest.module_id
    );
}
