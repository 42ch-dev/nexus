//! The Generate kernel, executed through a real `CoreService`: round 1 materializes the request,
//! the three shared lint layers decide, the ceiling is three rounds and exhaustion persists
//! nothing.
//!
//! Plan: `.mstar/plans/canvas-p6-retirement.md` Part C (T5/T6); spec:
//! `.mstar/iterations/v1.247-hygiene/specs/generate-and-lint-loop.md` §3–§7.

use std::sync::Mutex;

use async_trait::async_trait;
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, GenerateRequest, Principal};
use nexus_orchestration::capability::{
    CapabilityError, PromptExecutor, PromptRequest, PromptResult,
};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tempfile::TempDir;

const STRATEGY: &str = "novel-writing";

struct Setup {
    _tmp: TempDir,
    core: CoreService,
    principal: Principal,
}

async fn setup() -> Setup {
    let tmp = TempDir::new().unwrap();
    let home = tmp.path().join(".nexus42");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        tmp.path(),
        "test_creator",
        "default",
    ))
    .unwrap();
    std::fs::write(home.join("config.toml"), "active_creator_id = \"test_creator\"\n[active_workspace_slug_by_creator]\n\"test_creator\" = \"default\"\n").unwrap();
    let core = CoreService::open(CoreOpenOptions {
        user_home: tmp.path().to_path_buf(),
        access: CoreAccess::DirectWriter,
        memory: None,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    Setup {
        _tmp: tmp,
        core,
        principal,
    }
}

/// The stored row's revision, for the fail-closed probe: exhaustion must leave none.
async fn stored_revision(pool: &SqlitePool, strategy_id: &str) -> Option<i64> {
    sqlx::query_scalar("SELECT revision FROM strategy_candidates WHERE strategy_id = ?")
        .bind(strategy_id)
        .fetch_optional(pool)
        .await
        .unwrap()
}

/// A document every lint layer accepts: one outer state wired to its terminal, one run intent.
fn document(nodes: &Value, edges: &Value) -> Value {
    json!({
        "ir_version": 0,
        "id": STRATEGY,
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": "generate fixture",
            "kind": "creator",
            "run_intents": ["work_init"],
            "requires_capabilities": []
        },
        "entry": "start",
        "terminal": ["done"],
        "nodes": nodes,
        "edges": edges,
        "nests": [],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "pure",
        "provenance": { "source": "generate" }
    })
}

fn node(id: &str, kind: &str) -> Value {
    json!({ "id": id, "kind": kind, "effect_class": "pure" })
}

fn legal_request() -> GenerateRequest {
    serde_json::from_value(json!({
        "entry": "start",
        "terminal": ["done"],
        "nodes": [node("start", "outer_state"), node("done", "terminal")],
        "edges": [{ "id": "start->done", "from": "start", "to": "done", "kind": "linear" }],
        "nests": [],
        "meta": {
            "description": "generate fixture",
            "kind": "creator",
            "run_intents": ["work_init"],
            "requires_capabilities": []
        }
    }))
    .expect("the legal fixture is a generate request")
}

/// The same graph, with one `depends_on` entry naming a node the document does not declare.
fn dangling_request() -> GenerateRequest {
    let mut nodes = vec![node("start", "outer_state"), node("done", "terminal")];
    nodes[0]["depends_on"] = json!(["missing"]);
    serde_json::from_value(json!({
        "entry": "start",
        "terminal": ["done"],
        "nodes": nodes,
        "edges": [{ "id": "start->done", "from": "start", "to": "done", "kind": "linear" }],
        "nests": [],
        "meta": {
            "description": "generate fixture",
            "kind": "creator",
            "run_intents": ["work_init"],
            "requires_capabilities": []
        }
    }))
    .expect("the dangling fixture is a generate request")
}

fn exhaustion_of(error: CoreError) -> Value {
    match error {
        CoreError::InputValidation { details } => details,
        other => panic!("expected a lint_exhausted refusal, got {other:?}"),
    }
}

fn kinds(details: &Value) -> Vec<String> {
    details["violations"]
        .as_array()
        .expect("the payload carries a violations array")
        .iter()
        .map(|violation| violation["kind"].as_str().unwrap_or_default().to_owned())
        .collect()
}

/// A corrector that answers every round with a document whose dangling dependency names the call,
/// so the returned corrections identify which round they came from.
struct NeverConverges {
    calls: Mutex<usize>,
}

#[async_trait]
impl PromptExecutor for NeverConverges {
    async fn execute(&self, _request: PromptRequest) -> Result<PromptResult, CapabilityError> {
        let call = {
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            *calls
        };
        let mut nodes = vec![node("start", "outer_state"), node("done", "terminal")];
        nodes[0]["depends_on"] = json!([format!("missing-round-{}", call + 1)]);
        Ok(PromptResult {
            full_text: document(
                &Value::Array(nodes),
                &json!([{ "id": "start->done", "from": "start", "to": "done", "kind": "linear" }]),
            )
            .to_string(),
            host_session_id: format!("fake-session-{call}"),
            operation_id: format!("fake-operation-{call}"),
        })
    }
}

/// A corrector whose first answer is the legal document.
struct ConvergesOnce {
    calls: Mutex<usize>,
}

#[async_trait]
impl PromptExecutor for ConvergesOnce {
    async fn execute(&self, _request: PromptRequest) -> Result<PromptResult, CapabilityError> {
        *self.calls.lock().unwrap() += 1;
        Ok(PromptResult {
            full_text: legal_request_document().to_string(),
            host_session_id: "fake-session".to_owned(),
            operation_id: "fake-operation".to_owned(),
        })
    }
}

/// A corrector that returns the given document verbatim, claims and all.
struct ReturnsDocument(Value);

#[async_trait]
impl PromptExecutor for ReturnsDocument {
    async fn execute(&self, _request: PromptRequest) -> Result<PromptResult, CapabilityError> {
        Ok(PromptResult {
            full_text: self.0.to_string(),
            host_session_id: "fake-session".to_owned(),
            operation_id: "fake-operation".to_owned(),
        })
    }
}

/// The legal document as the corrector would return it: the same graph, the strategy's id.
fn legal_request_document() -> Value {
    document(
        &json!([node("start", "outer_state"), node("done", "terminal")]),
        &json!([{ "id": "start->done", "from": "start", "to": "done", "kind": "linear" }]),
    )
}

#[tokio::test]
async fn a_clean_request_materializes_round_one_and_commits_revision_one() {
    let setup = setup().await;
    let outcome = setup
        .core
        .generate_strategy_candidate(&setup.principal, STRATEGY, 0, legal_request(), None)
        .await
        .unwrap();

    assert_eq!(outcome.candidate.revision, 1);
    assert_eq!(outcome.candidate.document.id.as_str(), STRATEGY);
    assert_eq!(outcome.candidate.document.state.to_string(), "candidate");
    assert_eq!(
        outcome.candidate.document.provenance.source.to_string(),
        "generate",
        "a generated candidate records where it came from"
    );
    assert_eq!(
        outcome.candidate.document.effect_summary.to_string(),
        "pure",
        "the rollup is derived from the document's own bytes"
    );
    assert!(
        outcome.warnings.is_empty(),
        "the clean fixture is warning-free: {:?}",
        outcome.warnings
    );
    assert_eq!(
        stored_revision(setup.core.pool(), STRATEGY).await,
        Some(1),
        "the passing round is the only path to the commit"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn semantic_warnings_ride_a_passing_round_and_never_block() {
    let setup = setup().await;
    // A second node marked terminal is a TerminalConsistency warning, not an error.
    let request: GenerateRequest = serde_json::from_value(json!({
        "entry": "start",
        "terminal": ["done"],
        "nodes": [
            node("start", "outer_state"),
            node("done", "terminal"),
            node("spare", "terminal")
        ],
        "edges": [{ "id": "start->done", "from": "start", "to": "done", "kind": "linear" }],
        "nests": [],
        "meta": {
            "description": "generate fixture",
            "kind": "creator",
            "run_intents": ["work_init"],
            "requires_capabilities": []
        }
    }))
    .unwrap();

    let outcome = setup
        .core
        .generate_strategy_candidate(&setup.principal, STRATEGY, 0, request, None)
        .await
        .expect("a warning must not block generation");

    assert_eq!(outcome.candidate.revision, 1);
    assert_eq!(outcome.warnings.len(), 1, "{:?}", outcome.warnings);
    assert_eq!(
        outcome.warnings[0].category,
        nexus_preset::validation::DiagnosticCategory::TerminalConsistency
    );
    assert_eq!(outcome.warnings[0].path, "states[2].terminal");
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_corrector_that_converges_commits_the_corrected_document() {
    let setup = setup().await;
    let corrector = ConvergesOnce {
        calls: Mutex::new(0),
    };

    let outcome = setup
        .core
        .generate_strategy_candidate(
            &setup.principal,
            STRATEGY,
            0,
            dangling_request(),
            Some(&corrector),
        )
        .await
        .unwrap();

    assert_eq!(*corrector.calls.lock().unwrap(), 1, "one correction round");
    assert_eq!(outcome.candidate.revision, 1);
    assert!(
        outcome
            .candidate
            .document
            .nodes
            .iter()
            .all(|node| node.depends_on.is_empty()),
        "the committed document is the corrected one, not round 1's"
    );
    setup.core.close().await.unwrap();
}

/// A corrector is not the document's origin. The drift guard reads `provenance.source == generate`,
/// so a corrector that could set provenance, lineage or hash would falsify the assertion from the
/// inside.
#[tokio::test]
async fn a_corrector_cannot_claim_another_origin_lineage_hash_or_version() {
    let setup = setup().await;
    let mut claimed = legal_request_document();
    claimed["provenance"] = json!({
        "source": "import_preset",
        "agent_run_id": "another-agent-run",
        "parent_candidate_id": "another-candidate"
    });
    claimed["content_hash"] = json!("hash-from-a-prior-snapshot");
    claimed["version"] = json!(9);
    let corrector = ReturnsDocument(claimed);

    let outcome = setup
        .core
        .generate_strategy_candidate(
            &setup.principal,
            STRATEGY,
            0,
            dangling_request(),
            Some(&corrector),
        )
        .await
        .unwrap();

    let document = &outcome.candidate.document;
    assert_eq!(
        document.provenance.source.to_string(),
        "generate",
        "a generated candidate is provenance=generate whatever the model wrote"
    );
    assert_eq!(document.provenance.agent_run_id, None);
    assert_eq!(
        document.provenance.parent_candidate_id, None,
        "the corrector invents no lineage"
    );
    assert!(document.provenance.resolved_pins.is_empty());
    assert_eq!(document.content_hash, None);
    assert_eq!(document.version, 1);
    setup.core.close().await.unwrap();
}

/// The IR grammar version is the contract's, not the corrector's: a response claiming a version
/// this kernel does not implement is normalized before lint, so it neither commits nor burns the
/// remaining rounds on a buildability refusal the correction cannot address.
#[tokio::test]
async fn a_corrector_cannot_claim_an_ir_version_the_kernel_does_not_implement() {
    let setup = setup().await;
    let mut claimed = legal_request_document();
    claimed["ir_version"] = json!(7);
    let corrector = ReturnsDocument(claimed);

    let outcome = setup
        .core
        .generate_strategy_candidate(
            &setup.principal,
            STRATEGY,
            0,
            dangling_request(),
            Some(&corrector),
        )
        .await
        .unwrap();

    assert_eq!(outcome.candidate.document.ir_version, 0);
    setup.core.close().await.unwrap();
}

/// D8 bounds the field space at the request's. `GenerateRequest` exposes entry, terminal, nodes,
/// edges, nests and meta, so a corrector adding assets, signals, roles or an initial action is
/// outside the contract and the commit carries none of them.
#[tokio::test]
async fn a_corrector_cannot_add_fields_the_request_does_not_expose() {
    let setup = setup().await;
    let mut extra = legal_request_document();
    extra["assets"] = json!([{
        "id": "seed",
        "kind": "template",
        "path": "prompts/seed.md",
        "content_hash": "0".repeat(64)
    }]);
    extra["signals"] =
        json!([{ "action": "force_transition", "name": "revise", "target": "start" }]);
    extra["roles"] = json!([{
        "description": "writer",
        "id": "writer",
        "system_prompt_file": "prompts/writer.md"
    }]);
    extra["initial_action"] = json!({ "kind": "seed_direct" });
    let corrector = ReturnsDocument(extra);

    let outcome = setup
        .core
        .generate_strategy_candidate(
            &setup.principal,
            STRATEGY,
            0,
            dangling_request(),
            Some(&corrector),
        )
        .await
        .unwrap();

    let document = &outcome.candidate.document;
    assert!(
        document.assets.is_empty(),
        "assets are not the request's: {:?}",
        document.assets
    );
    assert!(
        document.signals.is_empty(),
        "signals are not the request's: {:?}",
        document.signals
    );
    assert!(
        document.roles.is_empty(),
        "roles are not the request's: {:?}",
        document.roles
    );
    assert!(document.initial_action.is_none());
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn the_ceiling_is_three_rounds_and_the_fourth_materialization_never_runs() {
    let setup = setup().await;
    let corrector = NeverConverges {
        calls: Mutex::new(0),
    };

    let details = exhaustion_of(
        setup
            .core
            .generate_strategy_candidate(
                &setup.principal,
                STRATEGY,
                0,
                dangling_request(),
                Some(&corrector),
            )
            .await
            .unwrap_err(),
    );

    assert_eq!(*corrector.calls.lock().unwrap(), 2, "two correction rounds");
    assert_eq!(details["code"], "lint_exhausted");
    assert_eq!(details["rounds_attempted"], 3);
    assert_eq!(details["persisted"], false);

    // The returned violations are round 3's, never the union of the rounds.
    let corrections = details["violations"].as_array().unwrap();
    let dangling = corrections
        .iter()
        .find(|violation| violation["kind"] == "dangling_dependency")
        .expect("round 3 still fails the same way");
    assert!(
        dangling["observed"]
            .as_str()
            .unwrap_or_default()
            .contains("missing-round-3"),
        "{dangling}"
    );
    assert_eq!(
        corrections
            .iter()
            .filter(|violation| violation["kind"] == "dangling_dependency")
            .count(),
        1,
        "one correction per violation, never one per round: {corrections:?}"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn exhaustion_persists_no_candidate_row() {
    let setup = setup().await;
    let corrector = NeverConverges {
        calls: Mutex::new(0),
    };

    let error = setup
        .core
        .generate_strategy_candidate(
            &setup.principal,
            STRATEGY,
            0,
            dangling_request(),
            Some(&corrector),
        )
        .await
        .unwrap_err();
    let details = exhaustion_of(error);
    assert_eq!(details["persisted"], false);

    assert_eq!(
        stored_revision(setup.core.pool(), STRATEGY).await,
        None,
        "the commit is reached only from a passing round"
    );
    let candidate = setup
        .core
        .read_strategy_candidate(&setup.principal, STRATEGY)
        .await
        .unwrap();
    assert_eq!(
        candidate.revision, 0,
        "the read still projects the revision-0 import, not a draft"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn no_corrector_with_a_failing_round_one_names_the_missing_id_and_the_absent_corrector() {
    let setup = setup().await;

    let details = exhaustion_of(
        setup
            .core
            .generate_strategy_candidate(&setup.principal, STRATEGY, 0, dangling_request(), None)
            .await
            .unwrap_err(),
    );

    assert_eq!(details["code"], "lint_exhausted");
    assert_eq!(
        details["rounds_attempted"], 1,
        "one exhaustion shape, not two"
    );
    assert_eq!(details["persisted"], false);
    let message = details["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("no corrector was supplied"),
        "the message names the absent corrector: {message}"
    );
    assert_eq!(kinds(&details), vec!["dangling_dependency".to_owned()]);

    let violation = &details["violations"][0];
    let correction = violation["correction"].as_str().unwrap_or_default();
    assert_eq!(violation["location"], "node start");
    assert!(correction.contains("missing"), "{correction}");
    for legal in ["start", "done"] {
        assert!(
            correction.contains(legal),
            "the correction names the legal id `{legal}`: {correction}"
        );
    }
    assert_eq!(
        stored_revision(setup.core.pool(), STRATEGY).await,
        None,
        "nothing was persisted"
    );
    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_stale_base_refuses_before_any_round_runs() {
    let setup = setup().await;
    let corrector = NeverConverges {
        calls: Mutex::new(0),
    };

    let error = setup
        .core
        .generate_strategy_candidate(
            &setup.principal,
            STRATEGY,
            7,
            legal_request(),
            Some(&corrector),
        )
        .await
        .unwrap_err();

    assert!(
        matches!(error, CoreError::StrategyCandidateConflict(_)),
        "a stale base is the store's typed conflict, got {error:?}"
    );
    assert_eq!(
        *corrector.calls.lock().unwrap(),
        0,
        "no model round runs for a write the CAS would refuse"
    );
    setup.core.close().await.unwrap();
}

/// A request that pins a package the registry does not serve. Approve refuses exactly that pin, so
/// the generator must report it too: the round's refusal has to name the pin the corrector has to
/// re-pin or drop, and nothing may persist. Without the pin findings in lint the payload carried
/// only the unmappable-node correction and never mentioned the pin.
#[tokio::test]
async fn an_unresolvable_pin_is_refused_by_name_and_persists_nothing() {
    let setup = setup().await;
    let request: GenerateRequest = serde_json::from_value(json!({
        "entry": "slot",
        "terminal": ["done"],
        "nodes": [node("slot", "pattern_slot"), node("done", "terminal")],
        "edges": [{ "id": "slot->done", "from": "slot", "to": "done", "kind": "linear" }],
        "nests": [{
            "id": "ghost-nest",
            "parent_node": "slot",
            "expansion": "expand_in_candidate",
            "pin_policy": "exact",
            "child": {
                "kind": "pattern",
                "registry_id": "ghost",
                "version": "1.0.0",
                "content_hash": "0".repeat(64)
            }
        }],
        "meta": {
            "description": "generate fixture",
            "kind": "creator",
            "run_intents": ["work_init"],
            "requires_capabilities": []
        }
    }))
    .expect("the pinned fixture is a generate request");

    let details = exhaustion_of(
        setup
            .core
            .generate_strategy_candidate(&setup.principal, STRATEGY, 0, request, None)
            .await
            .unwrap_err(),
    );

    assert_eq!(details["code"], "lint_exhausted");
    assert_eq!(details["persisted"], false);
    let pin = details["violations"]
        .as_array()
        .expect("the payload carries a violations array")
        .iter()
        .find(|violation| violation["kind"] == "pin_unavailable")
        .unwrap_or_else(|| panic!("the refusal names the pin Approve would refuse: {details}"));
    assert_eq!(pin["location"], "nest ghost-nest");
    assert!(
        pin["correction"]
            .as_str()
            .unwrap_or_default()
            .contains("ghost"),
        "the correction names the pin to re-pin or drop: {pin}"
    );
    assert_eq!(
        stored_revision(setup.core.pool(), STRATEGY).await,
        None,
        "a pin Approve would refuse never reaches the CAS"
    );
    setup.core.close().await.unwrap();
}
