//! Consumes the packed `nexus-pattern` archive: scans one local registry root
//! holding a valid package and a broken peer, and asserts availability,
//! version and content-hash identity from the real reader — including that a
//! broken package never hides its peers.

use std::path::PathBuf;

use nexus_pattern::{scan_pattern_packages, PatternPackage};
use nexus_workflow_ir::IR_VERSION;

/// Canonical content hash pin of `prompt-step`'s committed fragment.
const PROMPT_STEP_HASH: &str = "b89e9707bf2d39e27bf825f74c8bfac37acbdf1f7109614d0942fb8613212a04";

fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("patterns")
}

fn package<'a>(packages: &'a [PatternPackage], dir: &str) -> &'a PatternPackage {
    packages
        .iter()
        .find(|package| package.package_dir() == dir)
        .unwrap_or_else(|| panic!("`{dir}` is in the local registry root"))
}

fn main() {
    let packages = scan_pattern_packages(&fixture_root(), IR_VERSION).expect("registry root scans");
    assert_eq!(
        packages
            .iter()
            .map(PatternPackage::package_dir)
            .collect::<Vec<_>>(),
        ["broken-manifest", "prompt-step"],
        "both directories are listed, broken first"
    );

    let valid = package(&packages, "prompt-step");
    assert!(valid.available(), "valid package is available: {:?}", valid.reason());
    assert_eq!(valid.reason(), None);
    assert_eq!(valid.content_hash(), Some(PROMPT_STEP_HASH));
    let manifest = valid.manifest().expect("an available package carries its manifest");
    assert_eq!(manifest.id.as_str(), "prompt-step");
    assert_eq!(manifest.version.as_str(), "1.0.0");
    assert_eq!(
        manifest
            .requires_capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>(),
        ["acp.prompt"]
    );
    let fragment = valid.fragment().expect("an available package carries its fragment");
    assert_eq!(fragment.id.as_str(), "prompt-step");
    assert_eq!(fragment.entry, "prompt");

    // The broken peer is unavailable for its own named reason and reports no
    // version or hash — but it does not remove the package next to it.
    let broken = package(&packages, "broken-manifest");
    assert!(!broken.available());
    let reason = broken.reason().expect("an unavailable package carries its reason");
    assert!(reason.contains("`pattern.toml` is malformed"), "{reason}");
    assert!(broken.manifest().is_none());
    assert!(broken.fragment().is_none());
    assert_eq!(broken.content_hash(), None);
    assert!(package(&packages, "prompt-step").available());

    // Availability is version-scoped, not a stored flag: the same bytes are
    // unavailable when the caller's IR version falls outside `ir_compat`.
    let incompatible =
        scan_pattern_packages(&fixture_root(), IR_VERSION + 1).expect("registry root scans");
    assert_eq!(incompatible.len(), 2, "a rejected peer is still listed");
    let rejected = package(&incompatible, "prompt-step");
    assert!(!rejected.available());
    let reason = rejected.reason().expect("a rejected package carries its reason");
    assert!(
        reason.contains(&format!(
            "excludes the daemon ir_version {}",
            IR_VERSION + 1
        )),
        "{reason}"
    );
    assert_eq!(rejected.content_hash(), None);
    assert!(rejected.manifest().is_some(), "the manifest itself still parsed");

    println!(
        "nexus-pattern scanned {} packages ({} available)",
        packages.len(),
        packages.iter().filter(|package| package.available()).count()
    );
}
