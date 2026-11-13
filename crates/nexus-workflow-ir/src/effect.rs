//! Effect rollup: the one lattice order over node effect classes.
//!
//! `pure < read < write_candidate < external`. A document summary is the join of every reachable
//! node's declared class — this document's nodes and, through [`document_summary`], every resolved
//! nest child's — so grouping or nesting never hides a write or external effect.

use crate::structure::{NestGraph, PatternBody};
use crate::{
    WorkflowDocument, WorkflowDocumentEffectSummary, WorkflowNode, WorkflowNodeEffectClass,
};

/// Rank one node class in the shared order.
const fn effect_rank(class: WorkflowNodeEffectClass) -> u8 {
    match class {
        WorkflowNodeEffectClass::External => 3,
        WorkflowNodeEffectClass::WriteCandidate => 2,
        WorkflowNodeEffectClass::Read => 1,
        WorkflowNodeEffectClass::Pure => 0,
    }
}

/// Rank one document summary in the same order.
const fn summary_rank(summary: WorkflowDocumentEffectSummary) -> u8 {
    match summary {
        WorkflowDocumentEffectSummary::External => 3,
        WorkflowDocumentEffectSummary::WriteCandidate => 2,
        WorkflowDocumentEffectSummary::Read => 1,
        WorkflowDocumentEffectSummary::Pure => 0,
    }
}

/// The summary a single declared class folds to.
const fn class_summary(class: WorkflowNodeEffectClass) -> WorkflowDocumentEffectSummary {
    match class {
        WorkflowNodeEffectClass::External => WorkflowDocumentEffectSummary::External,
        WorkflowNodeEffectClass::WriteCandidate => WorkflowDocumentEffectSummary::WriteCandidate,
        WorkflowNodeEffectClass::Read => WorkflowDocumentEffectSummary::Read,
        WorkflowNodeEffectClass::Pure => WorkflowDocumentEffectSummary::Pure,
    }
}

/// Join every node's effect class into the worst-case document summary. An empty node list is
/// `pure`.
#[must_use]
pub fn effect_summary(nodes: &[WorkflowNode]) -> WorkflowDocumentEffectSummary {
    let rank = nodes
        .iter()
        .map(|node| effect_rank(node.effect_class))
        .max()
        .unwrap_or(0);
    match rank {
        3 => WorkflowDocumentEffectSummary::External,
        2 => WorkflowDocumentEffectSummary::WriteCandidate,
        1 => WorkflowDocumentEffectSummary::Read,
        _ => WorkflowDocumentEffectSummary::Pure,
    }
}

/// Join two summaries by worst case.
#[must_use]
pub const fn join(
    left: WorkflowDocumentEffectSummary,
    right: WorkflowDocumentEffectSummary,
) -> WorkflowDocumentEffectSummary {
    if summary_rank(left) >= summary_rank(right) {
        left
    } else {
        right
    }
}

/// The worst-case summary of `document` and every pattern body reachable from its pattern pins.
///
/// A resolved fragment contributes its nodes; an unavailable package contributes only its declared
/// class, so a pin that no longer loads still never lowers the fence. A pin with no body at all is
/// a leaf — the slot node that carries it keeps its own class in [`effect_summary`] — and a cyclic
/// graph is folded once per body, since the walk is total even before [`crate::validate`] refuses
/// it.
#[must_use]
pub fn document_summary(
    document: &WorkflowDocument,
    patterns: &[PatternBody<'_>],
) -> WorkflowDocumentEffectSummary {
    let graph = NestGraph::new(&[document], patterns);
    let mut summary = effect_summary(&document.nodes);
    let mut seen: std::collections::HashSet<String> =
        std::iter::once(document.id.to_string()).collect();
    let mut pending = vec![document.id.to_string()];

    while let Some(label) = pending.pop() {
        for child in graph.edges().get(&label).map_or(&[][..], Vec::as_slice) {
            if !seen.insert(child.clone()) {
                continue;
            }
            if let Some(nodes) = graph.nodes(child) {
                summary = join(summary, effect_summary(nodes));
            } else if let Some(class) = graph.declared_class(child) {
                summary = join(summary, class_summary(class));
            }
            pending.push(child.clone());
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structure::PatternBodyKind;
    use serde_json::json;

    fn document(
        id: &str,
        nodes: &serde_json::Value,
        nests: &serde_json::Value,
    ) -> WorkflowDocument {
        serde_json::from_value(json!({
            "ir_version": 0,
            "id": id,
            "version": 0,
            "revision": 0,
            "state": "candidate",
            "meta": {
                "description": "effect fixture",
                "kind": "creator",
                "run_intents": [],
                "requires_capabilities": []
            },
            "entry": "entry",
            "terminal": ["entry"],
            "nodes": nodes,
            "edges": [],
            "nests": nests,
            "assets": [],
            "signals": [],
            "roles": [],
            "effect_summary": "pure",
            "provenance": { "source": "compose" }
        }))
        .expect("fixture is a valid workflow document")
    }

    fn node(id: &str, kind: &str, effect_class: &str) -> serde_json::Value {
        json!({ "id": id, "kind": kind, "effect_class": effect_class })
    }

    fn pattern_nest(id: &str, parent: &str, registry_id: &str) -> serde_json::Value {
        json!({
            "id": id,
            "parent_node": parent,
            "expansion": "defer_until_apply",
            "pin_policy": "exact",
            "child": { "kind": "pattern", "registry_id": registry_id, "version": "1.0.0" }
        })
    }

    #[test]
    fn a_nested_write_is_never_hidden_by_a_pure_parent() {
        let fragment = document(
            "write-pattern",
            &json!([node("write", "inner_step", "write_candidate")]),
            &json!([]),
        );
        let patterns = [PatternBody {
            registry_id: "write-pattern",
            version: "1.0.0",
            body: PatternBodyKind::Fragment(&fragment),
        }];
        let root = document(
            "root",
            &json!([
                node("entry", "outer_state", "pure"),
                node("slot", "pattern_slot", "pure")
            ]),
            &json!([pattern_nest("n1", "slot", "write-pattern")]),
        );

        assert_eq!(
            document_summary(&root, &patterns),
            WorkflowDocumentEffectSummary::WriteCandidate
        );
        assert_eq!(
            effect_summary(&root.nodes),
            WorkflowDocumentEffectSummary::Pure,
            "the node-only rollup is what the fragment bodies alone extend"
        );
    }

    #[test]
    fn a_declared_class_contributes_without_a_body() {
        let patterns = [PatternBody {
            registry_id: "external-pattern",
            version: "1.0.0",
            body: PatternBodyKind::Declared(WorkflowNodeEffectClass::External),
        }];
        let root = document(
            "root",
            &json!([
                node("entry", "outer_state", "pure"),
                node("slot", "pattern_slot", "pure")
            ]),
            &json!([pattern_nest("n1", "slot", "external-pattern")]),
        );

        assert_eq!(
            document_summary(&root, &patterns),
            WorkflowDocumentEffectSummary::External
        );
    }

    #[test]
    fn a_self_nesting_pattern_is_folded_once() {
        let mut fragment = document(
            "loop",
            &json!([node("read", "inner_step", "read")]),
            &json!([pattern_nest("n1", "read", "loop")]),
        );
        fragment.nodes[0].kind = crate::WorkflowNodeKind::PatternSlot;
        let patterns = [PatternBody {
            registry_id: "loop",
            version: "1.0.0",
            body: PatternBodyKind::Fragment(&fragment),
        }];
        let root = document(
            "root",
            &json!([
                node("entry", "outer_state", "pure"),
                node("slot", "pattern_slot", "pure")
            ]),
            &json!([pattern_nest("n1", "slot", "loop")]),
        );

        assert_eq!(
            document_summary(&root, &patterns),
            WorkflowDocumentEffectSummary::Read
        );
    }

    #[test]
    fn join_keeps_the_worst_case() {
        assert_eq!(
            join(
                WorkflowDocumentEffectSummary::Pure,
                WorkflowDocumentEffectSummary::External
            ),
            WorkflowDocumentEffectSummary::External
        );
        assert_eq!(
            join(
                WorkflowDocumentEffectSummary::WriteCandidate,
                WorkflowDocumentEffectSummary::Read
            ),
            WorkflowDocumentEffectSummary::WriteCandidate
        );
        assert_eq!(
            join(
                WorkflowDocumentEffectSummary::Read,
                WorkflowDocumentEffectSummary::Read
            ),
            WorkflowDocumentEffectSummary::Read
        );
    }
}
