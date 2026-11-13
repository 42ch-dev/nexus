//! The four compose operations, executed through a real `CoreService` over a real registry
//! directory: every op commits exactly one revision, every refusal leaves the stored row
//! byte-identical and consumes no revision.
//!
//! Plan: `.mstar/plans/v239-p3-compose-ops.md` §Nest policy (D19) / §Compose wire contract (Q2).

use std::path::{Path, PathBuf};

use nexus_core::{
    BindSlotOp, ComposeDiagnostic, ComposeOp, ComposeOutcome, CoreAccess, CoreError,
    CoreOpenOptions, CoreService, NestOp, PatternPackageRef, Principal, ReplaceSubtreeOp, UnnestOp,
};
use nexus_workflow_ir::{
    WorkflowDocument, WorkflowDocumentEffectSummary, WorkflowNestBindingExpansion, WorkflowNodeKind,
};
use serde_json::{json, Value};
use sqlx::SqlitePool;
use tempfile::TempDir;

const STRATEGY: &str = "novel-writing";

struct Setup {
    _tmp: TempDir,
    core: CoreService,
    principal: Principal,
    home: PathBuf,
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
        home,
    }
}

fn package(id: &str, version: &str) -> PatternPackageRef {
    PatternPackageRef {
        id: id.to_owned(),
        version: version.to_owned(),
    }
}

fn nest(nest_id: &str, parent_node: &str, id: &str, version: &str) -> ComposeOp {
    ComposeOp::Nest(NestOp {
        nest_id: nest_id.to_owned(),
        parent_node: parent_node.to_owned(),
        package: package(id, version),
        expansion: WorkflowNestBindingExpansion::ExpandInCandidate,
    })
}

fn nest_id_op(nest_id: &str) -> ComposeOp {
    ComposeOp::Unnest(UnnestOp {
        nest_id: nest_id.to_owned(),
    })
}

fn replace(nest_id: &str, id: &str, version: &str) -> ComposeOp {
    ComposeOp::ReplaceSubtree(ReplaceSubtreeOp {
        nest_id: nest_id.to_owned(),
        package: package(id, version),
        expansion: WorkflowNestBindingExpansion::ExpandInCandidate,
    })
}

fn bind(nest_id: &str, slot: &str, value: Value) -> ComposeOp {
    ComposeOp::BindSlot(BindSlotOp {
        nest_id: nest_id.to_owned(),
        slot: slot.to_owned(),
        value,
    })
}

/// Write one package directory: a manifest with the given slots and the IR fragment it pins.
fn write_package(home: &Path, id: &str, effect_class: &str, slots: &str, fragment: &Value) {
    let directory = home.join("patterns").join(id);
    std::fs::create_dir_all(&directory).unwrap();
    // The manifest schema requires `slots`; an empty declaration is a top-level key.
    let empty_slots = if slots.is_empty() {
        "slots = []\n\n"
    } else {
        "\n"
    };
    std::fs::write(
        directory.join("pattern.toml"),
        format!(
            "id = \"{id}\"\nversion = \"1.0.0\"\ndescription = \"compose fixture {id}\"\n\
             effect_class = \"{effect_class}\"\nrequires_capabilities = []\n{empty_slots}\
             [ir_compat]\nmin = 0\nmax = 0\n{slots}"
        ),
    )
    .unwrap();
    std::fs::write(
        directory.join("workflow.ir.json"),
        serde_json::to_string_pretty(fragment).unwrap(),
    )
    .unwrap();
}

/// A fragment whose single node declares `effect_class` and whose nests are `nests`.
fn fragment(id: &str, effect_class: &str, nodes: &Value, nests: &Value) -> Value {
    json!({
        "ir_version": 0,
        "id": id,
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": format!("compose fixture {id}"),
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "work",
        "terminal": ["work"],
        "nodes": nodes,
        "edges": [],
        "nests": nests,
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": effect_class,
        "provenance": { "source": "compose" }
    })
}

fn fragment_node(effect_class: &str) -> Value {
    json!({ "id": "work", "kind": "inner_step", "effect_class": effect_class, "step_kind": "acp_prompt" })
}

/// A fragment that pins another package: one slot node and one collapsed nest.
fn chain_fragment(id: &str, next: &str) -> Value {
    let nests = json!([{
        "id": format!("{id}-nest"),
        "parent_node": "slot",
        "expansion": "defer_until_apply",
        "pin_policy": "exact",
        "child": { "kind": "pattern", "registry_id": next, "version": "1.0.0" }
    }]);
    json!({
        "ir_version": 0,
        "id": id,
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": format!("compose fixture {id}"),
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "slot",
        "terminal": ["slot"],
        "nodes": [{ "id": "slot", "kind": "pattern_slot", "effect_class": "pure" }],
        "edges": [],
        "nests": nests,
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "pure",
        "provenance": { "source": "compose" }
    })
}

fn write_chain(home: &Path, id: &str, next: Option<&str>) {
    let body = next.map_or_else(
        || {
            fragment(
                id,
                "pure",
                &json!([{ "id": "slot", "kind": "pattern_slot", "effect_class": "pure" }]),
                &json!([]),
            )
        },
        |next| chain_fragment(id, next),
    );
    write_package(home, id, "pure", "", &body);
}

/// A fragment with two wired nodes, so flattening has to move a body, not one node. Every
/// package reuses the ids `a`/`b`, which is what forces the second flatten to remap them.
fn flow_fragment(id: &str, next: Option<&str>) -> Value {
    let nests = next.map_or_else(
        || json!([]),
        |next| {
            json!([{
                "id": format!("{id}-nest"),
                "parent_node": "b",
                "expansion": "defer_until_apply",
                "pin_policy": "exact",
                "child": { "kind": "pattern", "registry_id": next, "version": "1.0.0" }
            }])
        },
    );
    json!({
        "ir_version": 0,
        "id": id,
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": format!("compose fixture {id}"),
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "a",
        "terminal": ["b"],
        "nodes": [
            { "id": "a", "kind": "inner_step", "effect_class": "pure", "step_kind": "acp_prompt" },
            { "id": "b", "kind": "pattern_slot", "effect_class": "pure" }
        ],
        "edges": [{ "id": "a->b", "from": "a", "to": "b", "kind": "linear" }],
        "nests": nests,
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "pure",
        "provenance": { "source": "compose" }
    })
}

fn write_flow(home: &Path, id: &str, next: Option<&str>) {
    write_package(home, id, "pure", "", &flow_fragment(id, next));
}

/// Nest then flatten, returning the committed documents of both operations.
async fn nest_then_unnest(
    setup: &Setup,
    base: u64,
    nest_id: &str,
    parent_node: &str,
    package_id: &str,
) -> (nexus_core::StrategyCandidate, nexus_core::StrategyCandidate) {
    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            base,
            nest(nest_id, parent_node, package_id, "1.0.0"),
        )
        .await
        .unwrap();
    let flat = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            nest_id_op(nest_id),
        )
        .await
        .unwrap();
    (nested.candidate, flat.candidate)
}

fn node_kind(document: &WorkflowDocument, id: &str) -> WorkflowNodeKind {
    document
        .nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("node `{id}` is in the document"))
        .kind
}

/// The two-node body moved by a flatten, with its own edge.
fn assert_two_node_body(document: &WorkflowDocument, entry: &str, terminal: &str) {
    assert_eq!(node_kind(document, entry), WorkflowNodeKind::InnerStep);
    assert_eq!(node_kind(document, terminal), WorkflowNodeKind::PatternSlot);
    assert!(
        document
            .edges
            .iter()
            .any(|edge| edge.from == entry && edge.to == terminal),
        "the body's own edge moved with it"
    );
}

/// The registry a compose test reads: one external pattern with a required string slot, one
/// `write_candidate` pattern, and one `read` pattern with a required string and an optional asset
/// slot.
fn write_registry(home: &Path) {
    write_package(
        home,
        "prompt-step",
        "external",
        "[[slots]]\nname = \"topic\"\nkind = \"string\"\nrequired = true\n",
        &fragment(
            "prompt-step",
            "external",
            &json!([fragment_node("external")]),
            &json!([]),
        ),
    );
    write_package(
        home,
        "write-step",
        "write_candidate",
        "",
        &fragment(
            "write-step",
            "write_candidate",
            &json!([fragment_node("write_candidate")]),
            &json!([]),
        ),
    );
    write_package(
        home,
        "asset-step",
        "read",
        "[[slots]]\nname = \"topic\"\nkind = \"string\"\nrequired = true\n\n\
         [[slots]]\nname = \"rubric\"\nkind = \"asset\"\nrequired = false\n",
        &fragment(
            "asset-step",
            "read",
            &json!([fragment_node("read")]),
            &json!([]),
        ),
    );
    // `topic` as an asset, so a value a string slot accepted cannot survive the swap.
    write_package(
        home,
        "asset-topic-step",
        "read",
        "[[slots]]\nname = \"topic\"\nkind = \"asset\"\nrequired = true\n",
        &fragment(
            "asset-topic-step",
            "read",
            &json!([fragment_node("read")]),
            &json!([]),
        ),
    );
}

type StoredRow = (String, String, i64, String, String);

async fn stored_row(pool: &SqlitePool, strategy_id: &str) -> Option<StoredRow> {
    sqlx::query_as(
        "SELECT strategy_id, document_json, revision, created_at, updated_at \
         FROM strategy_candidates WHERE strategy_id = ?",
    )
    .bind(strategy_id)
    .fetch_optional(pool)
    .await
    .unwrap()
}

/// The canonical refusal envelope carried by a refused compose.
fn refusal(error: CoreError) -> Value {
    match error {
        CoreError::InputValidation { details } => details,
        other => panic!("expected a compose refusal, got {other:?}"),
    }
}

async fn current(setup: &Setup) -> nexus_core::StrategyCandidate {
    setup
        .core
        .read_strategy_candidate(&setup.principal, STRATEGY)
        .await
        .unwrap()
}

/// One node as JSON, so the assertions read the committed bytes rather than a Rust projection.
fn slot_node(document: &WorkflowDocument, id: &str) -> Value {
    let value = serde_json::to_value(document).unwrap();
    value["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|node| node["id"] == id)
        .unwrap_or_else(|| panic!("node `{id}` is in the document"))
        .clone()
}

#[tokio::test]
async fn nest_inserts_the_collapsed_slot_and_the_resolved_pin() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    assert_eq!(before.revision, 0);

    let outcome = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("source-research", "collect-sources", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();

    // One revision, and the store owns the embedded mirror.
    assert_eq!(outcome.candidate.revision, 1);
    assert_eq!(outcome.candidate.document.revision, 1);
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await.unwrap().2, 1);

    // The slot is collapsed, carries the exact pin, and the package's class.
    let node = slot_node(&outcome.candidate.document, "collect-sources");
    assert_eq!(node["kind"], "pattern_slot");
    assert_eq!(node["pattern"]["registry_id"], "prompt-step");
    assert_eq!(node["pattern"]["version"], "1.0.0");
    let resolved = node["pattern"]["content_hash"]
        .as_str()
        .expect("the resolved pin carries the fragment hash")
        .to_owned();
    assert_eq!(node["effect_class"], "external");

    let document = &outcome.candidate.document;
    let nested = document
        .nests
        .iter()
        .find(|nest| nest.id == "source-research")
        .expect("the nest binding is committed");
    assert_eq!(nested.parent_node, "collect-sources");
    assert_eq!(nested.child.registry_id.as_deref(), Some("prompt-step"));
    assert_eq!(nested.child.version.as_deref(), Some("1.0.0"));
    assert_eq!(
        nested.child.content_hash.as_deref(),
        Some(resolved.as_str())
    );

    // The package's required slot is named, never defaulted.
    assert_eq!(
        outcome.diagnostics,
        vec![ComposeDiagnostic::UnboundRequiredSlot {
            nest_id: "source-research".to_owned(),
            package: package("prompt-step", "1.0.0"),
            slot: "topic".to_owned(),
        }]
    );

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_nested_write_candidate_pattern_never_lowers_the_summary() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;

    let outcome = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("draft-write", "draft-slot", "write-step", "1.0.0"),
        )
        .await
        .unwrap();

    // The base strategy already declares `external`, so the fence may only rise, never fall.
    assert!(
        matches!(
            outcome.candidate.document.effect_summary,
            WorkflowDocumentEffectSummary::WriteCandidate | WorkflowDocumentEffectSummary::External
        ),
        "a nested candidate write must not be hidden, got {:?}",
        outcome.candidate.document.effect_summary
    );
    assert_eq!(
        slot_node(&outcome.candidate.document, "draft-slot")["effect_class"],
        "write_candidate"
    );
    assert_eq!(outcome.diagnostics, []);

    // Unnesting the region removes the write it contributed: the rollup is recomputed, not kept.
    let unnest = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            outcome.candidate.revision,
            nest_id_op("draft-write"),
        )
        .await
        .unwrap();
    assert_eq!(
        unnest.candidate.document.effect_summary,
        before.document.effect_summary
    );

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn an_unavailable_package_is_refused_naming_id_and_version() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("source-research", "collect-sources", "prompt-step", "9.9.9"),
        )
        .await
        .unwrap_err();

    let details = refusal(error);
    assert_eq!(details["reason"], "package_unavailable");
    assert_eq!(details["nest_id"], "source-research");
    assert_eq!(details["package_id"], "prompt-step");
    assert_eq!(details["package_version"], "9.9.9");
    assert!(
        details["message"]
            .as_str()
            .unwrap()
            .contains("prompt-step@9.9.9"),
        "{details}"
    );

    // Nothing was written: no row, no revision, and the read still answers revision 0.
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);
    assert_eq!(current(&setup).await.revision, 0);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn an_unloadable_package_is_refused_with_the_readers_reason() {
    let setup = setup().await;
    write_registry(&setup.home);
    // The manifest loads, but its declared class disagrees with its own fragment.
    write_package(
        &setup.home,
        "mismatched-step",
        "external",
        "",
        &fragment(
            "mismatched-step",
            "pure",
            &json!([fragment_node("pure")]),
            &json!([]),
        ),
    );
    let before = current(&setup).await;

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("mismatch", "mismatch-slot", "mismatched-step", "1.0.0"),
        )
        .await
        .unwrap_err();

    let details = refusal(error);
    assert_eq!(details["reason"], "package_unavailable");
    assert_eq!(details["package_id"], "mismatched-step");
    let message = details["message"].as_str().unwrap();
    assert!(
        message.contains("disagrees with the fragment rollup"),
        "the reader's reason travels: {message}"
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, None);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_nest_cycle_is_refused_with_the_offending_nest_id() {
    let setup = setup().await;
    write_registry(&setup.home);
    write_chain(&setup.home, "cycle-a", Some("cycle-b"));
    write_chain(&setup.home, "cycle-b", Some("cycle-a"));
    let before = current(&setup).await;

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("cycle-nest", "cycle-slot", "cycle-a", "1.0.0"),
        )
        .await
        .unwrap_err();

    let details = refusal(error);
    assert_eq!(details["reason"], "nest_cycle");
    assert_eq!(details["nest_id"], "cycle-nest");
    assert_eq!(
        details["path"],
        json!(["cycle-a@1.0.0", "cycle-b@1.0.0", "cycle-a@1.0.0"])
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, None);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_nest_that_exceeds_depth_three_is_refused_at_the_offending_nest() {
    let setup = setup().await;
    write_registry(&setup.home);
    write_chain(&setup.home, "chain-a", Some("chain-b"));
    write_chain(&setup.home, "chain-b", Some("chain-c"));
    write_chain(&setup.home, "chain-c", Some("chain-d"));
    write_chain(&setup.home, "chain-d", None);
    let before = current(&setup).await;

    // chain-b nests chain-c which nests chain-d: exactly three levels, accepted.
    let accepted = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("depth-ok", "depth-slot", "chain-b", "1.0.0"),
        )
        .await
        .unwrap();
    assert_eq!(accepted.candidate.revision, 1);

    // chain-a adds one more level: refused, with the fourth level named.
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            accepted.candidate.revision,
            nest("depth-breach", "too-deep-slot", "chain-a", "1.0.0"),
        )
        .await
        .unwrap_err();

    let details = refusal(error);
    assert_eq!(details["reason"], "nest_depth_exceeded");
    assert_eq!(details["nest_id"], "depth-breach");
    assert_eq!(details["depth"], 4);
    assert_eq!(
        stored_row(setup.core.pool(), STRATEGY).await.unwrap().2,
        1,
        "the refusal consumed no revision"
    );

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn unnest_flattens_the_pinned_body_and_reports_the_dropped_binding() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let before_ids: Vec<String> = before
        .document
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .collect();

    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("source-research", "collect-sources", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    let bound = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            bind("source-research", "topic", json!("climate migration")),
        )
        .await
        .unwrap();

    let flattened = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            bound.candidate.revision,
            nest_id_op("source-research"),
        )
        .await
        .unwrap();

    assert_eq!(flattened.candidate.revision, 3);

    // The collapsed slot is gone and the body it pinned is a candidate node at this level.
    let document = &flattened.candidate.document;
    assert!(document
        .nests
        .iter()
        .all(|nest| nest.id != "source-research"));
    assert!(document
        .nodes
        .iter()
        .all(|node| node.id != "collect-sources"));
    let moved: Vec<&str> = document
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|id| !before_ids.iter().any(|before| before == id))
        .collect();
    assert_eq!(
        moved.len(),
        1,
        "the fragment's one node is a candidate node now"
    );
    assert!(
        moved[0] == "work" || moved[0] == "work~2",
        "the body's own id when it is free: {moved:?}"
    );
    assert_eq!(
        slot_node(document, moved[0])["effect_class"],
        "external",
        "the flattened body keeps its declared class"
    );

    // The binding the slot carried has no slot at this level: dropped, and named.
    assert_eq!(
        flattened.diagnostics,
        vec![ComposeDiagnostic::DroppedSlotBinding {
            nest_id: "source-research".to_owned(),
            slot: "topic".to_owned(),
        }]
    );

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn unnest_flattens_a_two_level_and_a_three_level_body_into_the_parent_level() {
    let setup = setup().await;
    write_registry(&setup.home);
    write_flow(&setup.home, "outer-two", Some("inner-two"));
    write_flow(&setup.home, "inner-two", None);
    write_flow(&setup.home, "outer-three", Some("middle-three"));
    write_flow(&setup.home, "middle-three", Some("leaf-three"));
    write_flow(&setup.home, "leaf-three", None);
    let before = current(&setup).await;
    assert!(
        before.document.nests.is_empty(),
        "the import projection carries no nest"
    );

    // Two levels: flattening the outer nest materializes its body and leaves the nest it carried.
    let (nested, flat) = nest_then_unnest(&setup, 0, "two", "two-slot", "outer-two").await;
    assert_eq!(nested.document.nests.len(), 1);
    assert_eq!(
        flat.document.nests.len(),
        1,
        "the body's own nest is a candidate nest now"
    );
    assert_two_node_body(&flat.document, "a", "b");
    let carried = &flat.document.nests[0];
    assert_eq!(carried.child.registry_id.as_deref(), Some("inner-two"));
    assert_eq!(carried.parent_node, "b");
    assert_eq!(
        node_kind(&flat.document, "b"),
        WorkflowNodeKind::PatternSlot
    );

    // Flattening again keeps the second body's ids unique and retargets the edge that pointed at
    // its slot: the outer `a -> b` now enters the inner body at `a~2`.
    let inner = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            flat.revision,
            nest_id_op(&carried.id),
        )
        .await
        .unwrap();
    let document = &inner.candidate.document;
    assert!(document.nests.is_empty());
    assert_two_node_body(document, "a~2", "b~2");
    assert!(
        document
            .edges
            .iter()
            .any(|edge| edge.from == "a" && edge.to == "a~2"),
        "the outer edge now enters the flattened body"
    );

    // Three levels: every flatten moves one level up and leaves the next pin behind.
    let (_, flat) = nest_then_unnest(
        &setup,
        inner.candidate.revision,
        "three",
        "three-slot",
        "outer-three",
    )
    .await;
    let mut document = flat.document;
    for expected in ["middle-three", "leaf-three"] {
        assert_eq!(document.nests.len(), 1);
        assert_eq!(
            document.nests[0].child.registry_id.as_deref(),
            Some(expected)
        );
        let nest_id = document.nests[0].id.clone();
        document = setup
            .core
            .compose_strategy_candidate(
                &setup.principal,
                STRATEGY,
                document.revision,
                nest_id_op(&nest_id),
            )
            .await
            .unwrap()
            .candidate
            .document;
    }
    assert!(
        document.nests.is_empty(),
        "every level of the three-level tree is flattened"
    );

    setup.core.close().await.unwrap();
}

/// Wire the slot node as the document's entry and terminal, with one edge on each side.
async fn wire_the_slot(
    setup: &Setup,
    base: u64,
    from: &str,
    to: &str,
) -> nexus_core::StrategyCandidate {
    setup
        .core
        .commit_strategy_candidate(&setup.principal, STRATEGY, base, |document| {
            let mut next = document.clone();
            let slot_node = "wired-slot".to_owned();
            next.entry.clone_from(&slot_node);
            next.terminal = vec![slot_node.clone()];
            for (id, edge_from, edge_to) in [
                ("into-slot", from, "wired-slot"),
                ("out-of-slot", "wired-slot", to),
            ] {
                next.edges.push(
                    serde_json::from_value(json!({
                        "id": id,
                        "from": edge_from,
                        "to": edge_to,
                        "kind": "linear"
                    }))
                    .unwrap(),
                );
            }
            Ok(next)
        })
        .await
        .unwrap()
}

#[tokio::test]
async fn unnest_retargets_the_wiring_the_slot_owned() {
    let setup = setup().await;
    write_registry(&setup.home);
    write_flow(&setup.home, "flow-step", None);
    let before = current(&setup).await;
    let from = before.document.entry.clone();
    let to = before
        .document
        .nodes
        .iter()
        .map(|node| node.id.clone())
        .find(|id| id != &from)
        .expect("the projection has a second node");

    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("wired", "wired-slot", "flow-step", "1.0.0"),
        )
        .await
        .unwrap();
    let wired = wire_the_slot(&setup, nested.candidate.revision, &from, &to).await;

    let flattened = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            wired.revision,
            nest_id_op("wired"),
        )
        .await
        .unwrap();
    let document = &flattened.candidate.document;
    assert!(document.nodes.iter().all(|node| node.id != "wired-slot"));
    assert_eq!(flattened.diagnostics, []);

    // The wiring the slot owned now lands on the fragment's boundary nodes: the body's own edge
    // names them, because both ends are nodes this flatten moved.
    let before_ids: Vec<&str> = before
        .document
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .collect();
    let moved: Vec<&str> = document
        .nodes
        .iter()
        .map(|node| node.id.as_str())
        .filter(|id| !before_ids.contains(id))
        .collect();
    assert_eq!(
        moved.len(),
        2,
        "the fragment's two nodes are candidate nodes now"
    );
    let body_edge = document
        .edges
        .iter()
        .find(|edge| moved.contains(&edge.from.as_str()) && moved.contains(&edge.to.as_str()))
        .expect("the body's own edge moved with it");
    let (entry_node, terminal_node) = (body_edge.from.clone(), body_edge.to.clone());
    assert_eq!(
        node_kind(document, &entry_node),
        WorkflowNodeKind::InnerStep
    );
    assert_eq!(
        node_kind(document, &terminal_node),
        WorkflowNodeKind::PatternSlot
    );
    assert_eq!(
        document.entry, entry_node,
        "the entry follows the body's entry"
    );
    assert_eq!(
        document.terminal,
        vec![terminal_node.clone()],
        "the terminal follows the body's terminal"
    );
    assert!(
        document
            .edges
            .iter()
            .any(|edge| edge.from == from && edge.to == entry_node),
        "the incoming edge now enters the body"
    );
    assert!(
        document
            .edges
            .iter()
            .any(|edge| edge.from == terminal_node && edge.to == to),
        "the outgoing edge now leaves the body"
    );

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn unnest_of_an_unknown_nest_is_refused() {
    let setup = setup().await;
    let before = current(&setup).await;

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest_id_op("absent"),
        )
        .await
        .unwrap_err();

    let details = refusal(error);
    assert_eq!(details["reason"], "unknown_nest");
    assert_eq!(details["nest_id"], "absent");
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, None);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn replace_subtree_changes_only_the_pin_of_that_region() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;

    let first = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("keep", "keep-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    let second = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            first.candidate.revision,
            nest("swap", "swap-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    let bound = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            second.candidate.revision,
            bind("swap", "topic", json!("water policy")),
        )
        .await
        .unwrap();

    let replaced = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            bound.candidate.revision,
            replace("swap", "asset-step", "1.0.0"),
        )
        .await
        .unwrap();

    let before_value = serde_json::to_value(&bound.candidate.document).unwrap();
    let mut after_value = serde_json::to_value(&replaced.candidate.document).unwrap();
    // The store owns the revision, so the diff covers content only.
    after_value["revision"] = before_value["revision"].clone();
    let changed = changed_paths(&before_value, &after_value);
    assert!(
        changed
            .iter()
            .any(|path| path == "/nests/1/child/content_hash"),
        "the region was re-pinned: {changed:?}"
    );

    // A real document diff: nothing outside the re-pinned region and its slot pin moved.
    let document = &replaced.candidate.document;
    let swap_index = document
        .nests
        .iter()
        .position(|nest| nest.id == "swap")
        .unwrap();
    let slot_index = document
        .nodes
        .iter()
        .position(|node| node.id == "swap-slot")
        .unwrap();
    let region = format!("/nests/{swap_index}/");
    let slot = format!("/nodes/{slot_index}/");
    for path in &changed {
        assert!(
            path.starts_with(&region) || path.starts_with(&slot) || path == "/effect_summary",
            "replace_subtree touched `{path}`: {changed:?}"
        );
    }

    // The authored edits survive: the other region, its bindings and the graph are untouched.
    assert_eq!(document.nests.len(), bound.candidate.document.nests.len());
    assert_eq!(document.nodes.len(), bound.candidate.document.nodes.len());
    assert_eq!(
        serde_json::to_value(&document.edges).unwrap(),
        serde_json::to_value(&bound.candidate.document.edges).unwrap()
    );
    assert_eq!(
        serde_json::to_value(&document.nests[0]).unwrap(),
        serde_json::to_value(&bound.candidate.document.nests[0]).unwrap()
    );
    assert_eq!(
        slot_node(document, "swap-slot")["bind"]["topic"],
        "water policy",
        "the region keeps the bindings it already had"
    );
    assert_eq!(
        slot_node(document, "swap-slot")["pattern"]["registry_id"],
        "asset-step"
    );
    assert_eq!(
        document.nests[1].child.registry_id.as_deref(),
        Some("asset-step")
    );
    assert_eq!(replaced.candidate.revision, 4);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn replace_subtree_of_an_unknown_nest_is_refused() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            replace("absent", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap_err();

    let details = refusal(error);
    assert_eq!(details["reason"], "unknown_nest");
    assert_eq!(details["nest_id"], "absent");
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, None);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn bind_slot_validates_the_value_against_the_manifest_declaration() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;

    // An asset the document carries, so the asset slot can be bound to a real id.
    let with_asset = setup
        .core
        .commit_strategy_candidate(&setup.principal, STRATEGY, before.revision, |document| {
            let mut next = document.clone();
            next.assets.push(
                serde_json::from_value(json!({
                    "id": "brief.md",
                    "path": "brief.md",
                    "kind": "template",
                    "content_hash": "ab"
                }))
                .unwrap(),
            );
            Ok(next)
        })
        .await
        .unwrap();
    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            with_asset.revision,
            nest("source-research", "collect-sources", "asset-step", "1.0.0"),
        )
        .await
        .unwrap();

    // A required string slot accepts a string and clears its diagnostic.
    let bound = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            bind("source-research", "topic", json!("water policy")),
        )
        .await
        .unwrap();
    assert_eq!(bound.diagnostics, []);

    // The asset slot accepts a documented asset id...
    let asset = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            bound.candidate.revision,
            bind("source-research", "rubric", json!("brief.md")),
        )
        .await
        .unwrap();
    assert_eq!(asset.diagnostics, []);

    // ...and refuses a string that names no asset, without changing the document.
    let stored = stored_row(setup.core.pool(), STRATEGY).await;
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            asset.candidate.revision,
            bind("source-research", "rubric", json!("templates/missing.md")),
        )
        .await
        .unwrap_err();
    let details = refusal(error);
    assert_eq!(details["reason"], "slot_kind_mismatch");
    assert_eq!(details["slot"], "rubric");
    assert_eq!(
        details["expected"],
        "an asset id present in document.assets"
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

    // A non-string value fails the same check.
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            asset.candidate.revision,
            bind("source-research", "topic", json!(7)),
        )
        .await
        .unwrap_err();
    assert_eq!(refusal(error)["reason"], "slot_kind_mismatch");

    // A slot the package does not declare is refused by name.
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            asset.candidate.revision,
            bind("source-research", "audience", json!("experts")),
        )
        .await
        .unwrap_err();
    let details = refusal(error);
    assert_eq!(details["reason"], "slot_not_declared");
    assert_eq!(details["pattern"], "asset-step@1.0.0");
    assert_eq!(details["slot"], "audience");

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_stale_base_is_the_store_conflict_and_consumes_no_revision() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;

    let first = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("first", "first-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("second", "second-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap_err();

    match error {
        CoreError::StrategyCandidateConflict(conflict) => {
            assert_eq!(conflict.base_revision, 0);
            assert_eq!(conflict.current_revision, first.candidate.revision);
        }
        other => panic!("expected the candidate conflict, got {other:?}"),
    }
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_package_that_changed_under_its_version_fails_closed() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("pinned", "pinned-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    let recorded = slot_node(&nested.candidate.document, "pinned-slot")["pattern"]["content_hash"]
        .as_str()
        .expect("the pin records the resolved hash")
        .to_owned();

    // The package is edited in place under the same id and version: new bytes, new hash.
    let mut reworked = fragment(
        "prompt-step",
        "external",
        &json!([fragment_node("external")]),
        &json!([]),
    );
    reworked["meta"]["description"] = json!("compose fixture prompt-step, reworked");
    write_package(
        &setup.home,
        "prompt-step",
        "external",
        "[[slots]]\nname = \"topic\"\nkind = \"string\"\nrequired = true\n",
        &reworked,
    );
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    // A bind would validate against the new bytes while the document still identifies the old
    // ones, so it is refused and names both hashes.
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            bind("pinned", "topic", json!("water policy")),
        )
        .await
        .unwrap_err();
    let details = refusal(error);
    assert_eq!(details["reason"], "pin_mismatch");
    assert_eq!(details["nest_id"], "pinned");
    assert_eq!(details["package_id"], "prompt-step");
    assert_eq!(details["package_version"], "1.0.0");
    assert_eq!(details["recorded_content_hash"], recorded.as_str());
    let scanned = details["scanned_content_hash"]
        .as_str()
        .expect("the refusal names the scanned hash")
        .to_owned();
    assert_ne!(scanned, recorded);
    let message = details["message"].as_str().unwrap();
    assert!(
        message.contains(&recorded) && message.contains(&scanned),
        "the refusal names both hashes: {message}"
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

    // `replace_subtree` is the one op allowed to establish a new pin: it adopts the new bytes.
    let replaced = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            replace("pinned", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    assert_eq!(
        slot_node(&replaced.candidate.document, "pinned-slot")["pattern"]["content_hash"],
        scanned.as_str()
    );
    assert_eq!(replaced.candidate.revision, 2);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn replace_subtree_refuses_a_retained_binding_its_pattern_no_longer_declares() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let with_asset = setup
        .core
        .commit_strategy_candidate(&setup.principal, STRATEGY, before.revision, |document| {
            let mut next = document.clone();
            next.assets.push(
                serde_json::from_value(json!({
                    "id": "brief.md",
                    "path": "brief.md",
                    "kind": "template",
                    "content_hash": "ab"
                }))
                .unwrap(),
            );
            Ok(next)
        })
        .await
        .unwrap();
    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            with_asset.revision,
            nest("swap", "swap-slot", "asset-step", "1.0.0"),
        )
        .await
        .unwrap();
    let bound = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            bind("swap", "rubric", json!("brief.md")),
        )
        .await
        .unwrap();
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    // `prompt-step` declares no `rubric`, so the retained binding would be a dangling slot.
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            bound.candidate.revision,
            replace("swap", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap_err();
    let details = refusal(error);
    assert_eq!(details["reason"], "slot_not_declared");
    assert_eq!(details["nest_id"], "swap");
    assert_eq!(details["pattern"], "prompt-step@1.0.0");
    assert_eq!(details["slot"], "rubric");
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn replace_subtree_refuses_a_retained_binding_whose_kind_the_pattern_changed() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("swap", "swap-slot", "asset-step", "1.0.0"),
        )
        .await
        .unwrap();
    let bound = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            bind("swap", "topic", json!("water policy")),
        )
        .await
        .unwrap();
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    // The replacement declares `topic` as an asset; the retained string is what `bind_slot`
    // would refuse for the identical value.
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            bound.candidate.revision,
            replace("swap", "asset-topic-step", "1.0.0"),
        )
        .await
        .unwrap_err();
    let details = refusal(error);
    assert_eq!(details["reason"], "slot_kind_mismatch");
    assert_eq!(details["nest_id"], "swap");
    assert_eq!(details["slot"], "topic");
    assert_eq!(
        details["expected"],
        "an asset id present in document.assets"
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_stale_base_is_refused_before_the_registry_is_scanned() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let first = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("first", "first-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();

    // An unreadable registry would surface as an internal error; a stale base must not reach it.
    let patterns = setup.home.join("patterns");
    std::fs::remove_dir_all(&patterns).unwrap();
    std::fs::write(&patterns, b"not a directory").unwrap();

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("second", "second-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap_err();
    match error {
        CoreError::StrategyCandidateConflict(conflict) => {
            assert_eq!(conflict.base_revision, 0);
            assert_eq!(conflict.current_revision, first.candidate.revision);
        }
        other => panic!("expected the candidate conflict before any registry scan, got {other:?}"),
    }

    setup.core.close().await.unwrap();
}

/// The shared walk reports a recorded pin whose package is absent instead of swallowing the
/// resolution error: nothing is consumed, and the carrier is named with the hash it recorded.
#[cfg(feature = "execution")]
#[test]
fn a_pin_whose_package_is_absent_is_reported_by_the_walk() {
    let recorded = "a".repeat(64);
    let document: WorkflowDocument = serde_json::from_value(json!({
        "ir_version": 0,
        "id": "approve-walk",
        "version": 1,
        "revision": 0,
        "state": "candidate",
        "meta": {
            "description": "walk fixture",
            "kind": "creator",
            "run_intents": [],
            "requires_capabilities": []
        },
        "entry": "slot",
        "terminal": ["slot"],
        "nodes": [{ "id": "slot", "kind": "pattern_slot", "effect_class": "pure" }],
        "edges": [],
        "nests": [{
            "id": "gone-nest",
            "parent_node": "slot",
            "expansion": "defer_until_apply",
            "pin_policy": "exact",
            "child": {
                "kind": "pattern",
                "registry_id": "removed-step",
                "version": "1.0.0",
                "content_hash": recorded
            }
        }],
        "assets": [],
        "signals": [],
        "roles": [],
        "effect_summary": "pure",
        "provenance": { "source": "compose" }
    }))
    .unwrap();

    let (bodies, diagnostics) = nexus_core::document_bodies(&document, &[]);

    assert!(
        bodies.is_empty(),
        "an absent package contributes no body to the walk"
    );
    assert!(
        matches!(
            diagnostics.as_slice(),
            [ComposeDiagnostic::PinUnavailable {
                nest_id,
                package: pinned,
                recorded_hash,
                ..
            }] if nest_id == "gone-nest"
                && *pinned == package("removed-step", "1.0.0")
                && recorded_hash == &recorded
        ),
        "the walk names the absent package and the recorded hash: {diagnostics:?}"
    );
}

/// Every stale-pin diagnostic of one outcome, in document order.
fn stale_pins(outcome: &ComposeOutcome) -> Vec<ComposeDiagnostic> {
    outcome
        .diagnostics
        .iter()
        .filter(|diagnostic| matches!(diagnostic, ComposeDiagnostic::StalePin { .. }))
        .cloned()
        .collect()
}

/// Rewrite a package's fragment in place under the same id and version: new bytes, new hash.
fn rework_package(home: &Path, id: &str, effect_class: &str, slots: &str, description: &str) {
    let mut reworked = fragment(
        id,
        effect_class,
        &json!([fragment_node(effect_class)]),
        &json!([]),
    );
    reworked["meta"]["description"] = json!(description);
    write_package(home, id, effect_class, slots, &reworked);
}

#[tokio::test]
async fn two_stale_pins_refuse_only_the_region_the_operation_uses() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let first = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("first", "first-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    let second = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            first.candidate.revision,
            nest("second", "second-slot", "write-step", "1.0.0"),
        )
        .await
        .unwrap();
    let recorded = slot_node(&second.candidate.document, "second-slot")["pattern"]["content_hash"]
        .as_str()
        .expect("the pin records the resolved hash")
        .to_owned();

    // Both packages are edited in place under the same versions: two regions are stale at once.
    rework_package(
        &setup.home,
        "prompt-step",
        "external",
        "[[slots]]\nname = \"topic\"\nkind = \"string\"\nrequired = true\n",
        "prompt-step, reworked",
    );
    rework_package(
        &setup.home,
        "write-step",
        "write_candidate",
        "",
        "write-step, reworked",
    );
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    // The region the operation resolves refuses, naming both hashes.
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            second.candidate.revision,
            bind("second", "topic", json!("water policy")),
        )
        .await
        .unwrap_err();
    let details = refusal(error);
    assert_eq!(details["reason"], "pin_mismatch");
    assert_eq!(details["nest_id"], "second");
    assert_eq!(details["recorded_content_hash"], recorded.as_str());
    let scanned = details["scanned_content_hash"]
        .as_str()
        .expect("the refusal names the scanned hash")
        .to_owned();
    assert_ne!(scanned, recorded);
    let message = details["message"].as_str().unwrap();
    assert!(
        message.contains(&recorded) && message.contains(&scanned),
        "the refusal names both hashes: {message}"
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

    // Re-pinning `first` is the remedy. It runs even though `second` is still stale, and that
    // other region is reported, never allowed to wedge every operation.
    let replaced = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            second.candidate.revision,
            replace("first", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    assert_eq!(replaced.candidate.revision, 3);
    assert_eq!(
        stale_pins(&replaced),
        vec![ComposeDiagnostic::StalePin {
            nest_id: "second".to_owned(),
            package: package("write-step", "1.0.0"),
            recorded_hash: recorded,
            scanned_hash: scanned.clone(),
        }]
    );
    assert!(
        replaced
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.to_string().contains(&scanned)),
        "the diagnostic names the hash the registry serves"
    );

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn a_changed_inner_pin_surfaces_before_the_shared_walk_dedupes() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let inner = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("inner", "inner-slot", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    let recorded = slot_node(&inner.candidate.document, "inner-slot")["pattern"]["content_hash"]
        .as_str()
        .expect("the pin records the resolved hash")
        .to_owned();

    // An outer package whose own fragment pins `prompt-step` at the bytes it had here.
    let mut outer = chain_fragment("outer", "prompt-step");
    outer["nests"][0]["child"]["content_hash"] = json!(recorded);
    write_package(&setup.home, "outer", "pure", "", &outer);
    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            inner.candidate.revision,
            nest("outer-region", "outer-slot", "outer", "1.0.0"),
        )
        .await
        .unwrap();
    assert_eq!(stale_pins(&nested), Vec::new());

    rework_package(
        &setup.home,
        "prompt-step",
        "external",
        "[[slots]]\nname = \"topic\"\nkind = \"string\"\nrequired = true\n",
        "prompt-step, reworked",
    );

    // Re-pinning the direct region is allowed; the outer fragment records the same pin, and that
    // carrier is reached only after the walk has de-duped `prompt-step` — so it is the one that
    // proves the hash is compared before the de-dupe.
    let replaced = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            replace("inner", "prompt-step", "1.0.0"),
        )
        .await
        .unwrap();
    let scanned = slot_node(&replaced.candidate.document, "inner-slot")["pattern"]["content_hash"]
        .as_str()
        .expect("the re-pinned region records the new hash")
        .to_owned();
    assert_ne!(scanned, recorded);
    assert_eq!(
        stale_pins(&replaced),
        vec![ComposeDiagnostic::StalePin {
            nest_id: "outer-nest".to_owned(),
            package: package("prompt-step", "1.0.0"),
            recorded_hash: recorded.clone(),
            scanned_hash: scanned.clone(),
        }]
    );
    let diagnostic = stale_pins(&replaced)[0].to_string();
    assert!(
        diagnostic.contains(&recorded) && diagnostic.contains(&scanned),
        "the diagnostic names both hashes: {diagnostic}"
    );

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn unnest_refuses_a_body_declaration_the_parent_level_cannot_carry() {
    let setup = setup().await;
    // `move_body` carries nodes and edges, not the body's own role declarations.
    let mut body = fragment(
        "role-step",
        "read",
        &json!([{
            "id": "work",
            "kind": "inner_step",
            "effect_class": "read",
            "step_kind": "acp_prompt",
            "role": "reviewer"
        }]),
        &json!([]),
    );
    body["roles"] = json!([{
        "id": "reviewer",
        "description": "reviews the draft",
        "system_prompt_file": "roles/reviewer.md"
    }]);
    write_package(&setup.home, "role-step", "read", "", &body);
    let before = current(&setup).await;
    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("role-region", "role-slot", "role-step", "1.0.0"),
        )
        .await
        .unwrap();
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            nest_id_op("role-region"),
        )
        .await
        .unwrap_err();
    let details = refusal(error);
    assert_eq!(details["reason"], "nest_unflattenable");
    assert_eq!(details["nest_id"], "role-region");
    let message = details["message"].as_str().unwrap();
    assert!(
        message.contains("role `reviewer`"),
        "the refusal names what would be lost: {message}"
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn unnest_refuses_the_control_references_the_slot_node_owns() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;
    let nested = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            before.revision,
            nest("wired", "wired-slot", "write-step", "1.0.0"),
        )
        .await
        .unwrap();
    let wired = setup
        .core
        .commit_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            |document| {
                let mut next = document.clone();
                next.nodes.push(
                    serde_json::from_value(json!({
                        "id": "prepare",
                        "kind": "inner_step",
                        "effect_class": "pure",
                        "step_kind": "acp_prompt"
                    }))
                    .unwrap(),
                );
                let slot = next
                    .nodes
                    .iter_mut()
                    .find(|node| node.id == "wired-slot")
                    .expect("the nested slot node is in the document");
                slot.depends_on = vec!["prepare".to_owned()];
                slot.on_timeout = Some("prepare".to_owned());
                Ok(next)
            },
        )
        .await
        .unwrap();
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    // The slot node is removed by the flatten, so its own control references would go with it.
    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            wired.revision,
            nest_id_op("wired"),
        )
        .await
        .unwrap_err();
    let details = refusal(error);
    assert_eq!(details["reason"], "nest_unflattenable");
    assert_eq!(details["nest_id"], "wired");
    let message = details["message"].as_str().unwrap();
    assert!(
        message.contains("depends_on `prepare`") && message.contains("on_timeout"),
        "the refusal names both control references: {message}"
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

    setup.core.close().await.unwrap();
}

/// Every JSON pointer that differs between two documents.
fn changed_paths(before: &Value, after: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    diff(before, after, "", &mut paths);
    paths.sort();
    paths
}

fn diff(before: &Value, after: &Value, path: &str, paths: &mut Vec<String>) {
    match (before, after) {
        (Value::Object(before), Value::Object(after)) => {
            for (key, value) in before {
                let child = format!("{path}/{}", key.replace('~', "~0").replace('/', "~1"));
                match after.get(key) {
                    Some(other) => diff(value, other, &child, paths),
                    None => paths.push(child),
                }
            }
            for key in after.keys().filter(|key| !before.contains_key(*key)) {
                paths.push(format!("{path}/{key}"));
            }
        }
        (Value::Array(before), Value::Array(after)) => {
            for (index, value) in before.iter().enumerate() {
                match after.get(index) {
                    Some(other) => diff(value, other, &format!("{path}/{index}"), paths),
                    None => paths.push(format!("{path}/{index}")),
                }
            }
            for index in before.len()..after.len() {
                paths.push(format!("{path}/{index}"));
            }
        }
        (before, after) if before != after => paths.push(path.to_owned()),
        _ => {}
    }
}
