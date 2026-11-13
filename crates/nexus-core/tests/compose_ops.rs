//! The four compose operations, executed through a real `CoreService` over a real registry
//! directory: every op commits exactly one revision, every refusal leaves the stored row
//! byte-identical and consumes no revision.
//!
//! Plan: `.mstar/plans/v239-p3-compose-ops.md` §Nest policy (D19) / §Compose wire contract (Q2).

use std::path::{Path, PathBuf};

use nexus_core::{
    BindSlotOp, ComposeDiagnostic, ComposeOp, CoreAccess, CoreError, CoreOpenOptions, CoreService,
    NestOp, PatternPackageRef, Principal, ReplaceSubtreeOp, UnnestOp,
};
use nexus_workflow_ir::{
    WorkflowDocument, WorkflowDocumentEffectSummary, WorkflowNestBindingExpansion,
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

/// A document rendered with a fixed revision, so a round trip can be compared byte for byte.
fn normalized(document: &WorkflowDocument, revision: u64) -> String {
    let mut document = document.clone();
    document.revision = revision;
    serde_json::to_string(&document).unwrap()
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
async fn unnest_restores_the_document_it_started_from() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;

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

    // The slot's own binding lives on the slot node, so flattening drops it with the slot.
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
    assert_eq!(
        slot_node(&bound.candidate.document, "collect-sources")["bind"]["topic"],
        "climate migration"
    );
    assert_eq!(bound.diagnostics, []);

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
    assert!(
        flattened
            .candidate
            .document
            .nests
            .iter()
            .all(|nest| nest.id != "source-research"),
        "the binding is gone"
    );
    assert!(
        flattened
            .candidate
            .document
            .nodes
            .iter()
            .all(|node| node.id != "collect-sources"),
        "the slot node, and the binding it carried, is gone"
    );
    assert_eq!(flattened.diagnostics, []);

    // The slot was the last edit, so nest/unnest is the identity on the document.
    assert_eq!(
        normalized(&flattened.candidate.document, 0),
        normalized(&before.document, 0)
    );

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn unnest_removes_a_two_level_and_a_three_level_region_whole() {
    let setup = setup().await;
    write_registry(&setup.home);
    write_chain(&setup.home, "two-level", None);
    write_chain(&setup.home, "three-level", Some("three-inner"));
    write_chain(&setup.home, "three-inner", None);
    let before = current(&setup).await;

    for (index, (nest_id, package_id)) in [("two", "two-level"), ("three", "three-level")]
        .into_iter()
        .enumerate()
    {
        let base = before.revision + index as u64 * 2;
        let nested = setup
            .core
            .compose_strategy_candidate(
                &setup.principal,
                STRATEGY,
                base,
                nest(nest_id, &format!("{nest_id}-slot"), package_id, "1.0.0"),
            )
            .await
            .unwrap();
        assert_eq!(nested.candidate.revision, base + 1);

        let flattened = setup
            .core
            .compose_strategy_candidate(
                &setup.principal,
                STRATEGY,
                nested.candidate.revision,
                nest_id_op(nest_id),
            )
            .await
            .unwrap();
        assert_eq!(
            normalized(&flattened.candidate.document, 0),
            normalized(&before.document, 0),
            "the `{package_id}` region flattens back to the parent level byte for byte"
        );
    }

    setup.core.close().await.unwrap();
}

#[tokio::test]
async fn unnest_refuses_a_slot_the_graph_still_references() {
    let setup = setup().await;
    write_registry(&setup.home);
    let before = current(&setup).await;

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

    // A later edit wires the slot into the graph: removing it would dangle that edge.
    let wired = setup
        .core
        .commit_strategy_candidate(
            &setup.principal,
            STRATEGY,
            nested.candidate.revision,
            |document| {
                let mut next = document.clone();
                let from = next.entry.clone();
                next.edges.push(
                    serde_json::from_value(json!({
                        "id": "entry->collect-sources",
                        "from": from,
                        "to": "collect-sources",
                        "kind": "linear"
                    }))
                    .unwrap(),
                );
                Ok(next)
            },
        )
        .await
        .unwrap();
    let stored = stored_row(setup.core.pool(), STRATEGY).await;

    let error = setup
        .core
        .compose_strategy_candidate(
            &setup.principal,
            STRATEGY,
            wired.revision,
            nest_id_op("source-research"),
        )
        .await
        .unwrap_err();

    let details = refusal(error);
    assert_eq!(details["reason"], "slot_still_wired");
    assert_eq!(details["nest_id"], "source-research");
    assert_eq!(details["parent_node"], "collect-sources");
    assert_eq!(
        details["referrers"],
        json!(["edge `entry->collect-sources`"])
    );
    assert_eq!(stored_row(setup.core.pool(), STRATEGY).await, stored);

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
