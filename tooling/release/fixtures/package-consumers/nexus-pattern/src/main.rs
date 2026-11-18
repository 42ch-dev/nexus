//! Consumes the packed `nexus-pattern` archive.
//!
//! The registry root this program scans is written from the archive's own packed
//! fixture bytes (`nexus_pattern::fixtures::*`), not from a consumer-local copy:
//! an archive that drops `crates/nexus-pattern/tests/fixtures/patterns/**` fails
//! to compile this consumer instead of passing on bytes the checkout happened to
//! keep. The scan then asserts availability, version and content-hash identity
//! from the real reader — including that a broken package never hides its peers.

use std::fs;
use std::path::{Path, PathBuf};

use nexus_pattern::{fixtures, scan_pattern_packages, PatternPackage};
use nexus_workflow_ir::IR_VERSION;

/// Expected canonical hash of the packed `prompt-step` fragment.
///
/// What it proves: an independent oracle for the hashing algorithm — the
/// expected value is written here by hand and never computed through the API
/// under test, so a consistently wrong canonicalization cannot pass. The bytes
/// it is computed from are the ones the archive ships
/// (`nexus_pattern::fixtures::PROMPT_STEP_FRAGMENT`), so a packed-fragment edit
/// fails here and an archive that omits the input never builds.
const PROMPT_STEP_HASH: &str = "b89e9707bf2d39e27bf825f74c8bfac37acbdf1f7109614d0942fb8613212a04";

/// Registry root assembled from the archive's packed fixture bytes.
fn packed_registry_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("nexus-pattern-consumer-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    write_member(
        &root,
        "broken-manifest",
        "pattern.toml",
        fixtures::BROKEN_MANIFEST,
    );
    write_member(
        &root,
        "prompt-step",
        "pattern.toml",
        fixtures::PROMPT_STEP_MANIFEST,
    );
    write_member(
        &root,
        "prompt-step",
        "workflow.ir.json",
        fixtures::PROMPT_STEP_FRAGMENT,
    );
    root
}

fn write_member(root: &Path, package_dir: &str, file: &str, contents: &str) {
    let directory = root.join(package_dir);
    fs::create_dir_all(&directory).expect("package directory is creatable");
    fs::write(directory.join(file), contents).expect("packed fixture byte is writable");
}

fn package<'a>(packages: &'a [PatternPackage], dir: &str) -> &'a PatternPackage {
    packages
        .iter()
        .find(|package| package.package_dir() == dir)
        .unwrap_or_else(|| panic!("`{dir}` is in the packed registry root"))
}

fn main() {
    let fixture_root = packed_registry_root();
    let scanned = scan_pattern_packages(&fixture_root, IR_VERSION);
    let _ = fs::remove_dir_all(&fixture_root);
    let packages = scanned.expect("packed registry root scans");

    assert_eq!(
        packages
            .iter()
            .map(PatternPackage::package_dir)
            .collect::<Vec<_>>(),
        ["broken-manifest", "prompt-step"],
        "both packed directories are listed, broken first"
    );

    let valid = package(&packages, "prompt-step");
    assert!(
        valid.available(),
        "packed package is available: {:?}",
        valid.reason()
    );
    assert_eq!(valid.reason(), None);
    assert_eq!(valid.content_hash(), Some(PROMPT_STEP_HASH));
    let manifest = valid
        .manifest()
        .expect("an available package carries its manifest");
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
    let fragment = valid
        .fragment()
        .expect("an available package carries its fragment");
    assert_eq!(fragment.id.as_str(), "prompt-step");
    assert_eq!(fragment.entry, "prompt");

    // The broken packed peer is unavailable for its own named reason and reports
    // no version or hash — but it does not remove the package next to it.
    let broken = package(&packages, "broken-manifest");
    assert!(!broken.available());
    let reason = broken
        .reason()
        .expect("an unavailable package carries its reason");
    assert!(reason.contains("`pattern.toml` is malformed"), "{reason}");
    assert!(broken.manifest().is_none());
    assert!(broken.fragment().is_none());
    assert_eq!(broken.content_hash(), None);
    assert!(package(&packages, "prompt-step").available());

    // Availability is version-scoped, not a stored flag: the same bytes are
    // unavailable when the caller's IR version falls outside `ir_compat`.
    let fixture_root = packed_registry_root();
    let incompatible =
        scan_pattern_packages(&fixture_root, IR_VERSION + 1).expect("packed registry root scans");
    let _ = fs::remove_dir_all(&fixture_root);
    assert_eq!(incompatible.len(), 2, "a rejected peer is still listed");
    let rejected = package(&incompatible, "prompt-step");
    assert!(!rejected.available());
    let reason = rejected
        .reason()
        .expect("a rejected package carries its reason");
    assert!(
        reason.contains(&format!(
            "excludes the daemon ir_version {}",
            IR_VERSION + 1
        )),
        "{reason}"
    );
    assert_eq!(rejected.content_hash(), None);
    assert!(
        rejected.manifest().is_some(),
        "the manifest itself still parsed"
    );

    println!(
        "nexus-pattern scanned {} packed packages ({} available)",
        packages.len(),
        packages
            .iter()
            .filter(|package| package.available())
            .count()
    );
}
