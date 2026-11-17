//! `nexus42 preset generate` and the chain it has to reach (P6-T7/T10).
//!
//! The verb is the CLI transport of `CoreService::generate_strategy_candidate`: it parses the
//! `GenerateRequest` contract, passes **no** corrector (the CLI attaches no model) and renders what
//! the kernel answers. These tests drive the real binary against a hermetic `HOME`:
//!
//! - a legal request commits revision 1 and prints the candidate plus its warnings;
//! - a request that fails lint is rendered as the whole `lint_exhausted` payload (§5), never as a
//!   bare "generation failed", and `--json` emits that same object;
//! - a generated candidate drives validate → simulate → approve → apply end to end, which is what
//!   proves the ADR's condition 4 instead of asserting it.

#[path = "common/direct.rs"]
mod direct;

use std::path::Path;
use std::process::Output;

use direct::DirectFixture;
use nexus_contracts::generated::daemon_api::orchestration::strategy_simulation_request::{
    NexusWorkflowDocument, StrategySimulationRequest,
};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use serde_json::{json, Value};

/// The strategy every generate here names: a user bundle, because only a user preset can be
/// applied (the schedule intake reads applied identities from user presets alone).
const STRATEGY: &str = "chain-fixture";

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn node(id: &str, kind: &str) -> Value {
    json!({ "id": id, "kind": kind, "effect_class": "pure" })
}

/// A document every lint layer accepts: one outer state wired to its terminal, one run intent.
fn legal_request() -> Value {
    json!({
        "entry": "start",
        "terminal": ["done"],
        "nodes": [node("start", "outer_state"), node("done", "terminal")],
        "edges": [{ "id": "start->done", "from": "start", "to": "done", "kind": "linear" }],
        "nests": [],
        "meta": {
            "description": "preset generate fixture",
            "kind": "creator",
            "run_intents": ["work_init"],
            "requires_capabilities": []
        }
    })
}

/// The same graph, with one `depends_on` entry naming a node the document does not declare.
fn dangling_request() -> Value {
    json!({
        "entry": "start",
        "terminal": ["done"],
        "nodes": [
            { "id": "start", "kind": "outer_state", "effect_class": "pure",
              "depends_on": ["missing"] },
            node("done", "terminal"),
        ],
        "edges": [{ "id": "start->done", "from": "start", "to": "done", "kind": "linear" }],
        "nests": [],
        "meta": {
            "description": "preset generate fixture",
            "kind": "creator",
            "run_intents": ["work_init"],
            "requires_capabilities": []
        }
    })
}

/// A minimal user preset bundle at `<nexus_home>/presets/<id>/preset.yaml` — the layout the core
/// resolves as a user (Canvas) strategy and the only source an apply may write for.
fn seed_user_preset(nexus_home: &Path, id: &str) {
    let bundle = nexus_home.join("presets").join(id);
    std::fs::create_dir_all(&bundle).expect("preset bundle dir");
    std::fs::write(
        bundle.join("preset.yaml"),
        format!(
            r#"preset:
  id: {id}
  version: 1
  kind: creator
  description: "preset generate chain fixture"
  requires_capabilities: []
  run_intents: [work_init]
  initial: start
  terminal: done
states:
  - id: start
    enter: []
    exit_when: {{ kind: rule }}
    next: done
  - id: done
    terminal: true
"#
        ),
    )
    .expect("preset yaml");
}

/// A hermetic home whose strategy resolves as a user preset.
///
/// Generate addresses a strategy, and `read_strategy_candidate` resolves the id before any round
/// runs, so an id the home does not know is a `not_found` refusal rather than an empty candidate.
async fn fixture() -> DirectFixture {
    let fixture = DirectFixture::new().await;
    seed_user_preset(&fixture.home.path().join(".nexus42"), STRATEGY);
    fixture
}

fn write_request(fixture: &DirectFixture, name: &str, request: &Value) -> std::path::PathBuf {
    let path = fixture.home.path().join(name);
    std::fs::write(&path, serde_json::to_vec_pretty(request).expect("json")).expect("request file");
    path
}

/// The engine-owner core the post-generate chain runs over (apply needs a schedule row, which the
/// engine writer is admitted for).
async fn chain_core(fixture: &DirectFixture) -> (CoreService, nexus_core::Principal) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: fixture.home.path().to_path_buf(),
        access: CoreAccess::EngineOwner,
        memory: None,
    })
    .await
    .expect("the fixture home opens for the chain");
    let principal = core.active_principal().await.expect("active principal");
    (core, principal)
}

#[tokio::test]
async fn generate_prints_the_committed_candidate_and_its_warnings() {
    let fixture = fixture().await;
    let request = write_request(&fixture, "request.json", &legal_request());

    let output = fixture
        .command()
        .args([
            "preset",
            "generate",
            STRATEGY,
            "--request",
            request.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("the generate verb runs");

    assert!(
        output.status.success(),
        "the legal request must commit: {}",
        stderr_of(&output)
    );
    let printed: Value =
        serde_json::from_str(&stdout_of(&output)).expect("the verb prints one JSON object");
    assert_eq!(printed["new_revision"], 1, "a first candidate inserts at 1");
    assert_eq!(printed["document"]["id"], STRATEGY);
    // Success means structurally legal and nothing more: the document is still a candidate, never
    // a live strategy, and the semantic warnings travel with it.
    assert_eq!(printed["document"]["state"], "candidate");
    assert_eq!(printed["document"]["revision"], 1);
    assert!(
        printed["warnings"].is_array(),
        "the passing round's warnings ride the output: {printed}"
    );
}

#[tokio::test]
async fn generate_renders_the_whole_lint_exhausted_payload() {
    let fixture = fixture().await;
    let request = write_request(&fixture, "request.json", &dangling_request());

    let output = fixture
        .command()
        .args([
            "preset",
            "generate",
            STRATEGY,
            "--request",
            request.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("the generate verb runs");

    assert!(
        !output.status.success(),
        "a lint-failing request is a non-zero exit"
    );
    let rendered = stderr_of(&output);
    // Every field the payload carries reaches the human: the code, the round count, the
    // `persisted` flag, each violation's kind/location/observed/correction, and the message.
    for fragment in [
        "lint_exhausted",
        "after 1 round(s)",
        "persisted: false",
        "[dangling_dependency]",
        "missing",
        "→",
        "fresh generate request",
    ] {
        assert!(
            rendered.contains(fragment),
            "the rendering must carry `{fragment}`: {rendered}"
        );
    }
    assert!(
        !rendered.contains("generation failed"),
        "no bare generation-failed summary: {rendered}"
    );

    // `--json` is the same object the HTTP `details` and the agent tool result carry.
    let json_output = fixture
        .command()
        .args([
            "preset",
            "generate",
            STRATEGY,
            "--request",
            request.to_str().expect("utf-8 path"),
            "--json",
        ])
        .output()
        .expect("the generate verb runs");
    let payload: Value =
        serde_json::from_str(&stdout_of(&json_output)).expect("--json emits the payload");
    assert_eq!(payload["code"], "lint_exhausted");
    assert_eq!(payload["rounds_attempted"], 1);
    assert_eq!(payload["persisted"], false);
    let violation = &payload["violations"][0];
    assert_eq!(violation["kind"], "dangling_dependency");
    assert!(
        violation["correction"]
            .as_str()
            .is_some_and(|text| text.contains("missing")),
        "the correction names what to change and what to change it to: {violation}"
    );
}

#[tokio::test]
async fn a_generated_candidate_completes_validate_simulate_approve_apply() {
    let fixture = fixture().await;
    let request = write_request(&fixture, "request.json", &legal_request());

    // 1. generate — the CLI commits the candidate the rest of the chain reads.
    let output = fixture
        .command()
        .args([
            "preset",
            "generate",
            STRATEGY,
            "--request",
            request.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("the generate verb runs");
    assert!(
        output.status.success(),
        "the legal request must commit: {}",
        stderr_of(&output)
    );
    let generated: Value = serde_json::from_str(&stdout_of(&output)).expect("one JSON object");
    assert_eq!(generated["new_revision"], 1);

    // 2. validate — the generated document through the local IR validator.
    let document_path = fixture.home.path().join("generated.ir.json");
    std::fs::write(
        &document_path,
        serde_json::to_vec_pretty(&generated["document"]).expect("json"),
    )
    .expect("candidate document");
    let validated = fixture
        .command()
        .args([
            "preset",
            "validate",
            document_path.to_str().expect("utf-8 path"),
        ])
        .output()
        .expect("the validate verb runs");
    assert!(
        validated.status.success(),
        "the generated document must validate: {}{}",
        stdout_of(&validated),
        stderr_of(&validated)
    );

    // 3. simulate → approve → apply over the same store, through the core the transports call.
    let (core, principal) = chain_core(&fixture).await;
    let document: NexusWorkflowDocument = serde_json::from_value(generated["document"].clone())
        .expect("the printed document is the wire document");
    let trajectory = core
        .simulate_strategy(
            &principal,
            STRATEGY.to_owned(),
            StrategySimulationRequest { document },
        )
        .await
        .expect("the generated candidate simulates");
    let approved = core
        .approve_strategy(&principal, STRATEGY, 1, Some(trajectory.clone()))
        .await
        .expect("the simulated candidate approves");
    assert_eq!(approved.candidate_revision, 1);
    let approved_hash = approved.content_hash.to_string();

    // The schedule whose `preset_id` is the strategy identity the apply admits against.
    sqlx::query(
        "INSERT INTO creator_schedules \
         (schedule_id, creator_id, preset_id, preset_version, status, concurrency_kind, \
          current_core_context_version, created_at, updated_at) \
         VALUES (?, ?, ?, 1, 'pending', 'serial', 0, 0, 0)",
    )
    .bind(format!("SCH-{STRATEGY}"))
    .bind(principal.creator_id())
    .bind(STRATEGY)
    .execute(core.pool())
    .await
    .expect("the fixture schedule lands");

    let applied = core
        .apply_strategy(&principal, STRATEGY, &approved_hash)
        .await
        .expect("the approved snapshot applies");
    assert_eq!(applied.content_hash, approved_hash);

    // The chain is only complete if a CURRENT apply exists afterwards and reads back the frozen
    // document the generation produced.
    let status = core
        .strategy_applied(&principal, STRATEGY)
        .await
        .expect("the status read answers");
    let current = status.current_apply.expect("a current apply exists");
    assert_eq!(current.content_hash, approved_hash);
    assert_eq!(
        status.applied_document.as_ref().map(|doc| doc.id.as_str()),
        Some(STRATEGY),
        "the applied document is the one the chain installed"
    );
}
