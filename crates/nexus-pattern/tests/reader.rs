//! Reader behaviour on real package directories written to a temporary root.

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use nexus_contracts::generated::daemon_api::patterns::pattern_summary::{
    NexusPatternSlotKind, PatternSummary, PatternSummaryEffectClass,
};
use nexus_pattern::{scan_pattern_packages, PatternPackage};

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
        .find(|package| package.package_dir == package_dir)
        .expect("package discovered");
    package.reason.as_deref().expect("package unavailable")
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
    assert_eq!(alpha.package_dir, "alpha");
    let manifest = alpha.manifest.as_ref().expect("manifest parsed from disk");
    assert_eq!(manifest.id.as_str(), "alpha");
    assert_eq!(manifest.version, "1.0.0");
    assert_eq!(
        manifest.description.as_str(),
        "Ground a topic in source material"
    );
    assert_eq!(manifest.requires_capabilities, ["search"]);
    assert_eq!(manifest.slots.len(), 1);
    assert!(alpha.fragment.is_some());

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
    assert_eq!(summary.slots[0].name, "topic");
    assert_eq!(summary.slots[0].kind, NexusPatternSlotKind::String);
    assert!(summary.slots[0].required);
    assert_eq!(summary.content_hash, None);
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
        .map(|package| package.package_dir.as_str())
        .collect();
    assert_eq!(available, ["alpha", "beta"]);

    let unavailable: Vec<&PatternPackage> = packages
        .iter()
        .filter(|package| !package.available())
        .collect();
    assert_eq!(unavailable.len(), 3);
    assert!(unavailable
        .iter()
        .all(|package| package.manifest.is_none() && package.fragment.is_none()));

    let reasons: HashSet<&str> = unavailable
        .iter()
        .map(|package| package.reason.as_deref().expect("reason"))
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
    let reason = package.reason.as_deref().expect("reason");
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
    assert!(reason(&packages, "not-semver").contains("not SemVer 2.0.0"));
    assert!(reason(&packages, "inverted").contains("is inverted"));
    assert!(reason(&packages, "unknown-field").contains("unknown field"));
    assert!(reason(&packages, "duplicate-slot").contains("slot `topic` more than once"));
    assert!(
        reason(&packages, "duplicate-capability").contains("capability `search` more than once")
    );
}
