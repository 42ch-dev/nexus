//! Consumes the packed `nexus-pattern` archive.
//!
//! The registry root this program scans is written from the archive's own packed fixture members
//! (`nexus_pattern::fixtures::MEMBERS`), not from a consumer-local copy: an archive that omits any
//! shipped member — a manifest, an IR fragment or a template — fails to compile this consumer
//! instead of passing on bytes the checkout happened to keep.
//!
//! Two independent checks then cover the shipped tree through the real reader:
//!
//! - the scan loads both real packages and reports each one's canonical content hash, compared
//!   against hand-written constants that were never computed through the API under test;
//! - every template member is pinned by the asset hash its own packed IR declares
//!   (`content_hash` is BLAKE3 over the asset bytes), so a template cannot drift or disappear
//!   without the archive's IR disagreeing with the archive's bytes.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use nexus_pattern::{fixtures, scan_pattern_packages, PatternPackage};
use nexus_workflow_ir::IR_VERSION;

/// Expected canonical hash of the packed `prompt-step` fragment. Hand-written oracle: a
/// consistently wrong canonicalization cannot pass, and the bytes it is computed from are the ones
/// the archive ships.
const PROMPT_STEP_HASH: &str = "b89e9707bf2d39e27bf825f74c8bfac37acbdf1f7109614d0942fb8613212a04";

/// Expected canonical hash of the packed `step-with-judge` fragment, same oracle.
const STEP_WITH_JUDGE_HASH: &str =
    "1d62b2b1bb02390d704ce10864b327b2120aed59ebe0ba482fe824347404324e";

/// Registry root assembled from the archive's packed fixture members.
fn packed_registry_root() -> PathBuf {
    let root = std::env::temp_dir().join(format!("nexus-pattern-consumer-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    for (package_dir, file, contents) in fixtures::MEMBERS {
        let target = root.join(package_dir).join(file);
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).expect("member directory is creatable");
        }
        fs::write(&target, contents).expect("packed fixture byte is writable");
    }
    root
}

/// Every packed member keyed by `package_dir/file`.
fn packed_members() -> BTreeMap<String, &'static str> {
    fixtures::MEMBERS
        .iter()
        .map(|(package_dir, file, contents)| (format!("{package_dir}/{file}"), *contents))
        .collect()
}

fn package<'a>(packages: &'a [PatternPackage], dir: &str) -> &'a PatternPackage {
    packages
        .iter()
        .find(|package| package.package_dir() == dir)
        .unwrap_or_else(|| panic!("`{dir}` is in the packed registry root"))
}

/// One package's load state, with its expected canonical hash.
fn loaded<'a>(
    packages: &'a [PatternPackage],
    dir: &str,
    expected_hash: &str,
) -> &'a PatternPackage {
    let package = package(packages, dir);
    assert!(
        package.available(),
        "packed `{dir}` is available: {:?}",
        package.reason()
    );
    assert_eq!(package.reason(), None);
    assert_eq!(package.content_hash(), Some(expected_hash));
    let manifest = package
        .manifest()
        .expect("an available package carries its manifest");
    assert_eq!(manifest.id.as_str(), dir);
    let fragment = package
        .fragment()
        .expect("an available package carries its fragment");
    assert_eq!(fragment.id.as_str(), dir);
    package
}

fn main() {
    let members = packed_members();
    let fixture_root = packed_registry_root();
    let scanned = scan_pattern_packages(&fixture_root, IR_VERSION);
    let _ = fs::remove_dir_all(&fixture_root);
    let packages = scanned.expect("packed registry root scans");

    // Every shipped directory is listed, in cursor order.
    assert_eq!(
        packages
            .iter()
            .map(PatternPackage::package_dir)
            .collect::<Vec<_>>(),
        ["broken-manifest", "prompt-step", "step-with-judge"],
        "all three packed directories are listed, broken first"
    );

    let valid = loaded(&packages, "prompt-step", PROMPT_STEP_HASH);
    assert_eq!(
        valid
            .manifest()
            .expect("manifest")
            .requires_capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>(),
        ["acp.prompt"]
    );
    assert_eq!(valid.fragment().expect("fragment").entry, "prompt");

    let judge = loaded(&packages, "step-with-judge", STEP_WITH_JUDGE_HASH);
    assert_eq!(
        judge
            .manifest()
            .expect("manifest")
            .requires_capabilities
            .iter()
            .map(|capability| capability.as_str())
            .collect::<Vec<_>>(),
        ["acp.prompt", "judge.llm"]
    );

    // Template bytes: the archive's own IR declares what each asset hashes to.
    let mut pinned = 0;
    for package in [valid, judge] {
        let fragment = package
            .fragment()
            .expect("an available package carries its fragment");
        for asset in &fragment.assets {
            let key = format!("{}/{}", package.package_dir(), asset.path);
            let bytes = members
                .get(&key)
                .unwrap_or_else(|| panic!("`{key}` is a packed member"));
            let declared = asset
                .content_hash
                .as_deref()
                .expect("the packed IR declares every asset hash");
            assert_eq!(
                blake3::hash(bytes.as_bytes()).to_hex().as_str(),
                declared,
                "packed `{key}` bytes must match the archive IR's declared hash"
            );
            pinned += 1;
        }
    }

    // No shipped member is left unpinned: every file that is not a manifest or an IR fragment must
    // be an asset some packed IR names.
    let template_members = members
        .keys()
        .filter(|key| !key.ends_with("pattern.toml") && !key.ends_with("workflow.ir.json"))
        .count();
    assert_eq!(
        pinned, template_members,
        "every packed template member is pinned by an archive IR asset"
    );

    // The broken packed peer is unavailable for its own named reason and reports no version or
    // hash — but it does not remove the packages next to it.
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

    // Availability is version-scoped, not a stored flag: the same bytes are unavailable when the
    // caller's IR version falls outside `ir_compat`.
    let fixture_root = packed_registry_root();
    let incompatible =
        scan_pattern_packages(&fixture_root, IR_VERSION + 1).expect("packed registry root scans");
    let _ = fs::remove_dir_all(&fixture_root);
    assert_eq!(incompatible.len(), 3, "a rejected peer is still listed");
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
        "nexus-pattern scanned {} packed packages ({} available) and pinned {pinned} template members to their archive IR hashes",
        packages.len(),
        packages.iter().filter(|package| package.available()).count()
    );
}
