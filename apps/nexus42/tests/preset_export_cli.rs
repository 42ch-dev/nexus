//! `nexus42 preset export <dir>` — the committed sample manifests are this command's output.
//!
//! Each sample is copied to a fresh temp directory (the command touches only `workflow.json`
//! and `preset.yaml`), driven through the compiled binary, and compared byte for byte with the
//! committed manifest: the pair is reproducible, not merely value-equal.

use assert_cmd::Command;
use predicates::prelude::PredicateBooleanExt;
use std::path::{Path, PathBuf};

const SAMPLES: [&str; 2] = [
    "strategy-samples/game-narrative",
    "strategy-samples/react-trpg-turn",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repository root resolves")
}

/// Copy the bundle's `workflow.json` and `preset.yaml` into a fresh temp directory.
fn copy_bundle(bundle: &str) -> (tempfile::TempDir, PathBuf) {
    let source = repo_root().join(bundle);
    let dir = tempfile::tempdir().expect("temp dir");
    for name in ["workflow.json", "preset.yaml"] {
        std::fs::copy(source.join(name), dir.path().join(name))
            .unwrap_or_else(|error| panic!("copy {bundle}/{name}: {error}"));
    }
    let path = dir.path().to_path_buf();
    (dir, path)
}

fn export(bundle: &Path) -> assert_cmd::assert::Assert {
    Command::cargo_bin("nexus42")
        .expect("nexus42 binary is built")
        .args(["preset", "export"])
        .arg(bundle)
        .assert()
}

fn read(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|error| panic!("read {}: {error}", path.display()))
}

#[test]
fn export_reproduces_the_committed_manifest_twice() {
    for bundle in SAMPLES {
        let (_dir, path) = copy_bundle(bundle);
        let manifest = path.join("preset.yaml");

        export(&path).success();
        let first = read(&manifest);
        assert_eq!(
            first,
            read(&repo_root().join(bundle).join("preset.yaml")),
            "{bundle}/preset.yaml is not the export of its workflow.json — run \
             `nexus42 preset export {bundle}`"
        );

        export(&path).success();
        assert_eq!(
            read(&manifest),
            first,
            "{bundle}: a second export changed the bytes, so the manifest is not reproducible"
        );
    }
}

#[test]
fn malformed_json_leaves_the_manifest_untouched() {
    assert_refusal_keeps_manifest(
        "strategy-samples/game-narrative",
        b"not a WorkflowDocument",
        "JSON error",
    );
}

#[test]
fn unprojectable_document_leaves_the_manifest_untouched() {
    // A document `serde` accepts that `export_preset` refuses: a preset carries exactly one
    // terminal node, so an empty `terminal` list has no projection.
    let json = std::fs::read_to_string(repo_root().join(SAMPLES[1]).join("workflow.json"))
        .expect("read the sample workflow.json");
    let mut document: serde_json::Value =
        serde_json::from_str(&json).expect("workflow.json parses");
    document["terminal"] = serde_json::json!([]);
    let workflow = serde_json::to_vec(&document).expect("re-serialize the document");

    assert_refusal_keeps_manifest(SAMPLES[1], &workflow, "workflow IR conversion refused");
}

/// Replace the bundle's `workflow.json` with `workflow`, run the export, require the refusal
/// named by `refusal`, and require the prior `preset.yaml` to be byte-identical.
fn assert_refusal_keeps_manifest(bundle: &str, workflow: &[u8], refusal: &str) {
    let (_dir, path) = copy_bundle(bundle);
    std::fs::write(path.join("workflow.json"), workflow).expect("write the refused workflow.json");
    let manifest = path.join("preset.yaml");
    let before = read(&manifest);

    export(&path)
        .failure()
        .stderr(predicates::str::contains(refusal).and(predicates::str::contains("Error")));

    assert_eq!(
        read(&manifest),
        before,
        "{bundle}: a refused export rewrote the prior manifest"
    );
}
