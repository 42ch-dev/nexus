//! Reader behaviour on real package directories written to a temporary root.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use nexus_contracts::generated::daemon_api::patterns::pattern_summary::{
    NexusPatternSlotKind, PatternSummary, PatternSummaryEffectClass,
};
use nexus_pattern::{scan_pattern_packages, PatternPackage, MAX_MEMBER_BYTES};

const IR_VERSION: u64 = 0;

fn manifest(id: &str, effect_class: &str) -> String {
    format!(
        r#"id = "{id}"
version = "1.0.0"
description = "Ground a topic in source material"
effect_class = "{effect_class}"
requires_capabilities = ["search"]

[ir_compat]
min = 0
max = 0

[[slots]]
name = "topic"
kind = "string"
required = true
"#
    )
}

fn manifest_with_compat(id: &str, effect_class: &str, min: u64, max: u64) -> String {
    manifest(id, effect_class).replace("min = 0\nmax = 0", &format!("min = {min}\nmax = {max}"))
}

/// `summary` is the fragment's own `effect_summary` claim; `node_effect` is the worst node class.
fn fragment(summary: &str, node_effect: &str) -> String {
    format!(
        r#"{{
  "ir_version": 0,
  "id": "probe",
  "version": 1,
  "revision": 0,
  "state": "candidate",
  "meta": {{
    "description": "probe",
    "kind": "creator",
    "run_intents": [],
    "requires_capabilities": []
  }},
  "entry": "a",
  "terminal": ["b"],
  "nodes": [
    {{"id": "a", "kind": "outer_state", "effect_class": "{node_effect}", "exit_when": {{"kind": "rule"}}}},
    {{"id": "b", "kind": "terminal", "effect_class": "pure"}}
  ],
  "edges": [{{"id": "a->b", "from": "a", "to": "b", "kind": "linear"}}],
  "nests": [],
  "assets": [],
  "signals": [],
  "roles": [],
  "effect_summary": "{summary}",
  "provenance": {{"source": "compose"}}
}}"#
    )
}

fn write_package(root: &Path, package_dir: &str, manifest: &str, fragment: Option<&str>) {
    let directory = root.join(package_dir);
    fs::create_dir(&directory).expect("package directory");
    fs::write(directory.join("pattern.toml"), manifest).expect("manifest written");
    if let Some(fragment) = fragment {
        fs::write(directory.join("workflow.ir.json"), fragment).expect("fragment written");
    }
}

fn valid_package(root: &Path, package_dir: &str, effect_class: &str) {
    write_package(
        root,
        package_dir,
        &manifest(package_dir, effect_class),
        Some(&fragment(effect_class, effect_class)),
    );
}

fn reason<'a>(packages: &'a [PatternPackage], package_dir: &str) -> &'a str {
    let package = packages
        .iter()
        .find(|package| package.package_dir() == package_dir)
        .expect("package discovered");
    package.reason().expect("package unavailable")
}

#[test]
fn two_valid_packages_are_available() {
    let root = tempfile::tempdir().expect("temp root");
    valid_package(root.path(), "alpha", "read");
    valid_package(root.path(), "beta", "pure");

    let packages = scan_pattern_packages(root.path(), IR_VERSION).expect("scan");
    assert_eq!(packages.len(), 2);
    assert!(packages.iter().all(PatternPackage::available));

    let alpha = &packages[0];
    assert_eq!(alpha.package_dir(), "alpha");
    let manifest = alpha.manifest().expect("manifest parsed from disk");
    assert_eq!(manifest.id.as_str(), "alpha");
    assert_eq!(manifest.version.as_str(), "1.0.0");
    assert_eq!(
        manifest.description.as_str(),
        "Ground a topic in source material"
    );
    assert_eq!(
        manifest
            .requires_capabilities
            .iter()
            .map(|id| id.as_str())
            .collect::<Vec<_>>(),
        ["search"]
    );
    assert_eq!(manifest.slots.len(), 1);
    assert!(alpha.fragment().is_some());

    let summary = PatternSummary::from(alpha);
    assert!(summary.available);
    assert_eq!(summary.reason, None);
    assert_eq!(summary.id.as_deref(), Some("alpha"));
    assert_eq!(summary.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        summary.description.as_deref(),
        Some("Ground a topic in source material")
    );
    assert_eq!(summary.effect_class, Some(PatternSummaryEffectClass::Read));
    assert_eq!(summary.requires_capabilities, ["search"]);
    assert_eq!(summary.slots.len(), 1);
    assert_eq!(summary.slots[0].name.as_str(), "topic");
    assert_eq!(summary.slots[0].kind, NexusPatternSlotKind::String);
    assert!(summary.slots[0].required);
    assert!(
        summary
            .content_hash
            .as_deref()
            .is_some_and(|pin| pin.len() == 64),
        "an available package carries its 64-character BLAKE3 pin: {:?}",
        summary.content_hash
    );
}

#[test]
fn broken_packages_do_not_hide_their_peers() {
    let root = tempfile::tempdir().expect("temp root");
    valid_package(root.path(), "alpha", "read");
    valid_package(root.path(), "beta", "pure");
    write_package(
        root.path(),
        "bad-toml",
        "this is not toml",
        Some(&fragment("pure", "pure")),
    );
    write_package(
        root.path(),
        "no-fragment",
        &manifest("no-fragment", "pure"),
        None,
    );
    write_package(
        root.path(),
        "stale-ir",
        &manifest_with_compat("stale-ir", "pure", 1, 2),
        Some(&fragment("pure", "pure")),
    );

    let packages = scan_pattern_packages(root.path(), IR_VERSION).expect("scan");
    assert_eq!(packages.len(), 5);

    let available: Vec<&str> = packages
        .iter()
        .filter(|package| package.available())
        .map(PatternPackage::package_dir)
        .collect();
    assert_eq!(available, ["alpha", "beta"]);

    let unavailable: Vec<&PatternPackage> = packages
        .iter()
        .filter(|package| !package.available())
        .collect();
    assert_eq!(unavailable.len(), 3);
    assert!(unavailable
        .iter()
        .all(|package| package.fragment().is_none()));

    // A manifest that parsed stays readable on an unavailable package, so a client can name it.
    let bad_toml = packages
        .iter()
        .find(|package| package.package_dir() == "bad-toml")
        .expect("bad-toml discovered");
    assert!(bad_toml.manifest().is_none());
    for package_dir in ["no-fragment", "stale-ir"] {
        let package = packages
            .iter()
            .find(|package| package.package_dir() == package_dir)
            .expect("package discovered");
        assert!(
            package.manifest().is_some(),
            "{package_dir} manifest parsed"
        );
        let summary = PatternSummary::from(package);
        assert!(!summary.available);
        assert_eq!(summary.id.as_deref(), Some(package_dir));
        assert_eq!(summary.version.as_deref(), Some("1.0.0"));
        assert_eq!(summary.slots.len(), 1);
    }

    let reasons: HashSet<&str> = unavailable
        .iter()
        .map(|package| package.reason().expect("reason"))
        .collect();
    assert_eq!(reasons.len(), 3, "reasons are distinct: {reasons:?}");

    assert!(reason(&packages, "bad-toml").contains("`pattern.toml` is malformed"));
    assert!(reason(&packages, "no-fragment").contains("`workflow.ir.json`"));
    let stale = reason(&packages, "stale-ir");
    assert!(stale.contains("ir_compat 1..=2 excludes"), "{stale}");
}

#[test]
fn declared_effect_must_match_the_fragment_rollup() {
    let root = tempfile::tempdir().expect("temp root");
    // The fragment claims `pure` in its own summary while a node is `external`; the reader
    // joins the nodes itself rather than believing either claim.
    write_package(
        root.path(),
        "loud",
        &manifest("loud", "pure"),
        Some(&fragment("pure", "external")),
    );

    let packages = scan_pattern_packages(root.path(), IR_VERSION).expect("scan");
    let package = &packages[0];
    assert!(!package.available());
    let reason = package.reason().expect("reason");
    assert!(
        reason.contains("`pure`") && reason.contains("`external`"),
        "{reason}"
    );
}

#[test]
fn manifest_rules_are_enforced() {
    let root = tempfile::tempdir().expect("temp root");
    let pure_fragment = fragment("pure", "pure");

    write_package(
        root.path(),
        "id-mismatch",
        &manifest("other-id", "pure"),
        Some(&pure_fragment),
    );
    write_package(
        root.path(),
        "not-semver",
        &manifest("not-semver", "pure").replace("version = \"1.0.0\"", "version = \"1.0\""),
        Some(&pure_fragment),
    );
    write_package(
        root.path(),
        "inverted",
        &manifest_with_compat("inverted", "pure", 2, 1),
        Some(&pure_fragment),
    );
    write_package(
        root.path(),
        "unknown-field",
        &manifest("unknown-field", "pure")
            .replace("requires_capabilities", "extra = 1\nrequires_capabilities"),
        Some(&pure_fragment),
    );
    write_package(
        root.path(),
        "duplicate-slot",
        &format!(
            "{}\n[[slots]]\nname = \"topic\"\nkind = \"string\"\nrequired = false\n",
            manifest("duplicate-slot", "pure")
        ),
        Some(&pure_fragment),
    );
    write_package(
        root.path(),
        "duplicate-capability",
        &manifest("duplicate-capability", "pure")
            .replace("[\"search\"]", "[\"search\", \"search\"]"),
        Some(&pure_fragment),
    );

    let packages = scan_pattern_packages(root.path(), IR_VERSION).expect("scan");
    assert_eq!(packages.len(), 6);
    assert!(packages.iter().all(|package| !package.available()));

    assert!(reason(&packages, "id-mismatch").contains("does not match package directory"));
    // The manifest schema carries the SemVer 2.0.0 pattern and is the only validator of the label,
    // so a bad version is rejected while parsing `pattern.toml`.
    assert!(
        reason(&packages, "not-semver").contains("doesn't match pattern"),
        "a non-SemVer version is rejected by the manifest schema"
    );
    assert!(reason(&packages, "inverted").contains("is inverted"));
    assert!(reason(&packages, "unknown-field").contains("unknown field"));
    assert!(reason(&packages, "duplicate-slot").contains("slot `topic` more than once"));
    assert!(
        reason(&packages, "duplicate-capability").contains("capability `search` more than once")
    );
}

/// Render the package's own pin for a directory, panicking when the package is unavailable.
fn pin(packages: &[PatternPackage], package_dir: &str) -> String {
    packages
        .iter()
        .find(|package| package.package_dir() == package_dir)
        .unwrap_or_else(|| panic!("{package_dir} discovered"))
        .content_hash()
        .map_or_else(|| panic!("{package_dir} carries a pin"), str::to_owned)
}

#[test]
fn content_hash_pins_the_fragment_not_the_manifest() {
    let root = tempfile::tempdir().expect("temp root");
    let bytes = fragment("read", "read");
    valid_package(root.path(), "alpha", "read");
    // Same fragment bytes, different manifest version: the pin is content, not metadata.
    write_package(
        root.path(),
        "beta",
        &manifest("beta", "read").replace("version = \"1.0.0\"", "version = \"2.0.0\""),
        Some(&bytes),
    );

    let first = scan_pattern_packages(root.path(), IR_VERSION).expect("scan");
    assert_eq!(pin(&first, "alpha"), pin(&first, "beta"));

    let rescan = scan_pattern_packages(root.path(), IR_VERSION).expect("rescan");
    assert_eq!(
        pin(&rescan, "alpha"),
        pin(&first, "alpha"),
        "same bytes, same pin"
    );

    // One changed byte of the fragment moves the pin; the untouched peer keeps its own.
    fs::write(
        root.path().join("alpha").join("workflow.ir.json"),
        bytes.replace("\"id\": \"probe\"", "\"id\": \"probe2\""),
    )
    .expect("fragment rewritten");
    let changed = scan_pattern_packages(root.path(), IR_VERSION).expect("rescan");
    assert_ne!(pin(&changed, "alpha"), pin(&first, "alpha"));
    assert_eq!(pin(&changed, "beta"), pin(&first, "beta"));
}

#[test]
fn a_fragment_whose_pin_cannot_be_computed_is_unavailable() {
    let root = tempfile::tempdir().expect("temp root");
    // A resolved asset without its own hash leaves the pin incomplete; that is a reason, never an
    // available package with a null pin.
    let unpinned_asset = fragment("pure", "pure").replace(
        "\"assets\": []",
        "\"assets\": [{\"id\": \"tpl\", \"path\": \"tpl.md\", \"kind\": \"template\"}]",
    );
    write_package(
        root.path(),
        "unpinned",
        &manifest("unpinned", "pure"),
        Some(&unpinned_asset),
    );

    let packages = scan_pattern_packages(root.path(), IR_VERSION).expect("scan");
    let package = &packages[0];
    assert!(!package.available());
    assert_eq!(package.content_hash(), None);
    assert_eq!(PatternSummary::from(package).content_hash, None);
    let reason = package.reason().expect("reason");
    assert!(
        reason.contains("content hash") && reason.contains("assets[tpl]"),
        "{reason}"
    );
}

#[test]
fn a_missing_registry_root_is_an_empty_listing() {
    let root = tempfile::tempdir().expect("temp root");
    let missing = root.path().join("patterns");

    let packages = scan_pattern_packages(&missing, IR_VERSION).expect("a missing root scans");
    assert!(packages.is_empty());
    assert!(
        !missing.exists(),
        "the scan never creates the registry root"
    );
}

#[test]
fn the_version_label_is_owned_by_the_manifest_schema() {
    let root = tempfile::tempdir().expect("temp root");
    // SemVer 2.0.0 puts no ceiling on a numeric identifier, and the schema pattern is the only
    // authority over the label, so the value a `u64`-backed parser would reject is accepted.
    write_package(
        root.path(),
        "wide-version",
        &manifest("wide-version", "pure").replace(
            "version = \"1.0.0\"",
            "version = \"18446744073709551616.0.0\"",
        ),
        Some(&fragment("pure", "pure")),
    );

    let packages = scan_pattern_packages(root.path(), IR_VERSION).expect("scan");
    let package = &packages[0];
    assert!(package.available(), "{:?}", package.reason());
    assert_eq!(
        package
            .manifest()
            .expect("manifest parsed")
            .version
            .as_str(),
        "18446744073709551616.0.0"
    );
}

/// A FIFO member must be refused without being opened. The scan runs on its own thread, so a
/// regression fails this test after a short wait instead of hanging the suite.
#[cfg(unix)]
#[test]
fn a_fifo_member_is_unavailable_without_stalling_the_scan() {
    use std::time::Duration;

    let root = tempfile::tempdir().expect("temp root");
    valid_package(root.path(), "peer", "pure");
    let stalled = root.path().join("fifo-manifest");
    fs::create_dir(&stalled).expect("package directory");
    let created = std::process::Command::new("mkfifo")
        .arg(stalled.join("pattern.toml"))
        .status()
        .expect("mkfifo is available");
    assert!(created.success(), "mkfifo failed: {created}");

    let scanned = root.path().to_path_buf();
    let (sender, receiver) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = sender.send(scan_pattern_packages(&scanned, IR_VERSION));
    });
    let packages = receiver
        .recv_timeout(Duration::from_secs(5))
        .expect("the scan must refuse a FIFO member instead of blocking on it")
        .expect("scan");

    assert_eq!(packages.len(), 2);
    assert_eq!(
        reason(&packages, "fifo-manifest"),
        "`pattern.toml` is not a regular file"
    );
    assert!(packages
        .iter()
        .any(|package| package.package_dir() == "peer" && package.available()));
}

#[test]
fn an_oversized_member_is_unavailable_without_being_read() {
    let root = tempfile::tempdir().expect("temp root");
    valid_package(root.path(), "peer", "pure");
    let oversized = root.path().join("oversized");
    fs::create_dir(&oversized).expect("package directory");
    let manifest = fs::File::create(oversized.join("pattern.toml")).expect("manifest created");
    manifest
        .set_len(MAX_MEMBER_BYTES + 1)
        .expect("manifest widened");

    let packages = scan_pattern_packages(root.path(), IR_VERSION).expect("scan");
    assert_eq!(packages.len(), 2);
    let oversized_reason = reason(&packages, "oversized");
    assert!(oversized_reason.contains("above the"), "{oversized_reason}");
    assert!(packages
        .iter()
        .any(|package| package.package_dir() == "peer" && package.available()));
}
