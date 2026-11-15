//! `nexus42 preset import <dir>` against a hermetic HOME (P6 Task 2).
//!
//! The verb is the only route that turns an external partner bundle into a
//! strategy candidate: it loads and converts through the one YAML→IR adapter,
//! validates and simulates the prospective revision-1 document, then commits it
//! through the same CAS every compose operation uses. These tests drive the real
//! binary against an isolated `HOME` seeded through the direct-core seam — no
//! daemon, no provider.

#[path = "common/direct.rs"]
mod direct;

use std::path::Path;
use std::process::Output;

use direct::DirectFixture;

/// The strategy id every CLI fixture declares.
const PARTNER: &str = "cli-partner-fixture";

fn write_bundle(bundle: &Path, yaml: &str) {
    std::fs::create_dir_all(bundle).expect("bundle directory");
    std::fs::write(bundle.join("preset.yaml"), yaml).expect("preset.yaml");
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A partner bundle the loader, the shared validator and the static walk all accept.
fn partner_bundle() -> String {
    format!(
        r#"preset:
  id: {PARTNER}
  version: 1
  kind: creator
  description: "CLI partner bundle fixture"
  requires_capabilities: []
  run_intents: [work_init]
  initial: draft
  terminal: done
states:
  - id: draft
    exit_when: {{ kind: rule }}
    next: review
  - id: review
    exit_when: {{ kind: manual }}
    next: done
  - id: done
    terminal: true
"#
    )
}

/// The same shape, but a judged state with two labeled branches and no `default`
/// edge: the walk refuses to guess a branch.
fn unsimulatable_bundle() -> String {
    format!(
        r#"preset:
  id: {PARTNER}
  version: 1
  kind: creator
  description: "CLI unsimulatable fixture"
  requires_capabilities: []
  run_intents: [work_init]
  initial: draft
  terminal: done
states:
  - id: draft
    exit_when:
      kind: llm_judge
    next:
      - label: left
        target: left_branch
      - label: right
        target: right_branch
  - id: left_branch
    exit_when: {{ kind: rule }}
    next: done
  - id: right_branch
    exit_when: {{ kind: rule }}
    next: done
  - id: done
    terminal: true
"#
    )
}

#[tokio::test]
async fn preset_import_prints_the_candidate_and_the_trajectory_approve_accepts() {
    let fixture = DirectFixture::new().await;
    let bundle = fixture.home.path().join(PARTNER);
    write_bundle(&bundle, &partner_bundle());

    let output = fixture
        .command()
        .args(["preset", "import", bundle.to_str().expect("utf-8 path")])
        .output()
        .expect("the import verb runs");

    assert!(
        output.status.success(),
        "the import must succeed: {}",
        stderr_of(&output)
    );
    let printed: serde_json::Value =
        serde_json::from_str(&stdout_of(&output)).expect("the verb prints one JSON object");
    assert_eq!(printed["candidate"]["revision"], 1);
    assert_eq!(printed["candidate"]["document"]["id"], PARTNER);
    assert_eq!(
        printed["trajectory"]["ir_content_hash"], printed["candidate"]["content_hash"],
        "the printed trajectory must be the walk of the stored bytes"
    );

    // The stored row is the conflict on a second import: the source cannot overwrite it.
    let again = fixture
        .command()
        .args(["preset", "import", bundle.to_str().expect("utf-8 path")])
        .output()
        .expect("the import verb runs");
    assert!(!again.status.success(), "a repeat import must be refused");
    let stderr = stderr_of(&again);
    assert!(
        stderr.contains("strategy_candidate_conflict"),
        "a repeat import is the typed conflict: {stderr}"
    );
}

#[tokio::test]
async fn preset_import_reports_a_bundle_it_cannot_simulate() {
    let fixture = DirectFixture::new().await;
    let bundle = fixture.home.path().join(PARTNER);
    write_bundle(&bundle, &unsimulatable_bundle());

    let output = fixture
        .command()
        .args(["preset", "import", bundle.to_str().expect("utf-8 path")])
        .output()
        .expect("the import verb runs");

    assert!(
        !output.status.success(),
        "an unsimulatable bundle is refused"
    );
    let stderr = stderr_of(&output);
    assert!(
        stderr.contains("preset_import_refused") && stderr.contains("simulation_unavailable"),
        "the refusal names the import family and the reason: {stderr}"
    );
}

#[tokio::test]
async fn preset_import_reports_a_bundle_that_does_not_load() {
    let fixture = DirectFixture::new().await;
    let bundle = fixture.home.path().join("broken-bundle");
    write_bundle(&bundle, "preset:\n  id: broken\n");

    let output = fixture
        .command()
        .args(["preset", "import", bundle.to_str().expect("utf-8 path")])
        .output()
        .expect("the import verb runs");

    assert!(!output.status.success(), "a malformed bundle is refused");
    assert!(
        stderr_of(&output).contains("invalid input (bundle)"),
        "a bundle that does not load is a bundle input refusal: {}",
        stderr_of(&output)
    );
}
