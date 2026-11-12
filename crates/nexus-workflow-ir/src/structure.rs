//! Q2 structural validation: nest depth, reference cycles and stable nest ids.
//!
//! [`validate`] takes a document set: the document under validation plus every child workflow
//! the caller could resolve. A referenced workflow outside the set is a leaf, so validating one
//! document never requires a registry. Pattern children are always leaves — v0 resolves no
//! pattern bodies, so pattern-to-pattern cycles are out of reach here.

use crate::{WorkflowDocument, WorkflowNestChildKind, WorkflowNode, WorkflowNodeKind};
use std::collections::{HashMap, HashSet};

/// Q2: at most three unexpanded nest levels. v0 represents no expansion, so every nest binding
/// counts as one level.
pub const MAX_NEST_DEPTH: usize = 3;

/// A structural violation. Every violation found is returned, never only the first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    DuplicateDocumentId(String),
    DuplicateNodeId {
        document: String,
        node: String,
    },
    DanglingEdge {
        document: String,
        edge: String,
        node: String,
    },
    /// A node's `depends_on` entry names no node in the document. Dependency targets carry no
    /// edge id, so the owning node is reported instead.
    DanglingDependency {
        document: String,
        node: String,
        dependency: String,
    },
    EmptyNestId {
        document: String,
    },
    DuplicateNestId {
        document: String,
        nest: String,
    },
    /// A nest's `parent_node` names no node in the document.
    UnknownNestParent {
        document: String,
        nest: String,
        parent: String,
    },
    /// A nest's `parent_node` is not a `pattern_slot` or `nest_group`, so the nest has no
    /// addressable region.
    NestParentNotComposable {
        document: String,
        nest: String,
        parent: String,
        kind: String,
    },
    /// A nest child omits the payload its `kind` requires.
    IncompleteNestChild {
        document: String,
        nest: String,
    },
    GraphCycle {
        document: String,
        path: Vec<String>,
    },
    NestCycle {
        path: Vec<String>,
    },
    NestDepthExceeded {
        document: String,
        depth: usize,
    },
}

impl std::fmt::Display for Violation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateDocumentId(id) => {
                write!(f, "document id `{id}` appears more than once in the set")
            }
            Self::DuplicateNodeId { document, node } => {
                write!(f, "`{document}`: node id `{node}` is duplicated")
            }
            Self::DanglingEdge {
                document,
                edge,
                node,
            } => write!(
                f,
                "`{document}`: edge `{edge}` references unknown node `{node}`"
            ),
            Self::DanglingDependency {
                document,
                node,
                dependency,
            } => write!(
                f,
                "`{document}`: node `{node}` depends on unknown node `{dependency}`"
            ),
            Self::EmptyNestId { document } => write!(f, "`{document}`: a nest has an empty id"),
            Self::DuplicateNestId { document, nest } => {
                write!(f, "`{document}`: nest id `{nest}` is duplicated")
            }
            Self::UnknownNestParent {
                document,
                nest,
                parent,
            } => write!(
                f,
                "`{document}`: nest `{nest}` references unknown parent node `{parent}`"
            ),
            Self::NestParentNotComposable {
                document,
                nest,
                parent,
                kind,
            } => write!(
                f,
                "`{document}`: nest `{nest}` parent `{parent}` is `{kind}`, not a pattern_slot or nest_group"
            ),
            Self::IncompleteNestChild { document, nest } => write!(
                f,
                "`{document}`: nest `{nest}` child omits its pattern or workflow pin"
            ),
            Self::GraphCycle { document, path } => {
                write!(f, "`{document}`: node graph is not a DAG: {}", path.join(" -> "))
            }
            Self::NestCycle { path } => {
                write!(f, "nest reference cycle: {}", path.join(" -> "))
            }
            Self::NestDepthExceeded { document, depth } => write!(
                f,
                "`{document}`: nest depth {depth} exceeds {MAX_NEST_DEPTH}"
            ),
        }
    }
}

impl std::error::Error for Violation {}

/// Validate a document set against the Q2 structural rules.
///
/// # Errors
///
/// Returns every violation found. An empty vector is never returned.
pub fn validate(documents: &[&WorkflowDocument]) -> Result<(), Vec<Violation>> {
    let mut violations = Vec::new();
    let mut index: HashMap<&str, &WorkflowDocument> = HashMap::new();

    for document in documents {
        let id = document.id.as_str();
        if index.insert(id, document).is_some() {
            violations.push(Violation::DuplicateDocumentId(id.to_owned()));
        }
    }
    for document in documents {
        validate_document(document, &mut violations);
    }

    let references = nest_references(documents, &index);
    let roots: Vec<&str> = documents
        .iter()
        .map(|document| document.id.as_str())
        .collect();
    if let Some(path) = find_cycle(&references, &roots) {
        violations.push(Violation::NestCycle { path });
    } else {
        // A reference cycle has no finite depth, so depth is skipped for the whole set when
        // any cycle exists.
        let mut memo = HashMap::new();
        for root in roots {
            let depth = nest_depth(root, &index, &mut memo);
            if depth > MAX_NEST_DEPTH {
                violations.push(Violation::NestDepthExceeded {
                    document: root.to_owned(),
                    depth,
                });
            }
        }
    }

    if violations.is_empty() {
        Ok(())
    } else {
        Err(violations)
    }
}

fn validate_document(document: &WorkflowDocument, violations: &mut Vec<Violation>) {
    let id = document.id.as_str();
    let mut nodes: HashMap<&str, &WorkflowNode> = HashMap::new();
    let mut edges: HashMap<&str, Vec<&str>> = HashMap::new();

    for node in &document.nodes {
        if nodes.insert(node.id.as_str(), node).is_some() {
            violations.push(Violation::DuplicateNodeId {
                document: id.to_owned(),
                node: node.id.clone(),
            });
        }
    }

    for node in &document.nodes {
        for dependency in &node.depends_on {
            if !nodes.contains_key(dependency.as_str()) {
                violations.push(Violation::DanglingDependency {
                    document: id.to_owned(),
                    node: node.id.clone(),
                    dependency: dependency.clone(),
                });
            }
        }
        edges
            .entry(node.id.as_str())
            .or_default()
            .extend(node.depends_on.iter().map(String::as_str));
    }

    for edge in &document.edges {
        for endpoint in [edge.from.as_str(), edge.to.as_str()] {
            if !nodes.contains_key(endpoint) {
                violations.push(Violation::DanglingEdge {
                    document: id.to_owned(),
                    edge: edge.id.clone(),
                    node: endpoint.to_owned(),
                });
            }
        }
        edges
            .entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }

    let roots: Vec<&str> = document.nodes.iter().map(|node| node.id.as_str()).collect();
    if let Some(path) = find_cycle(&edges, &roots) {
        violations.push(Violation::GraphCycle {
            document: id.to_owned(),
            path,
        });
    }

    let mut nest_ids: HashSet<&str> = HashSet::new();
    for nest in &document.nests {
        if nest.id.is_empty() {
            violations.push(Violation::EmptyNestId {
                document: id.to_owned(),
            });
        } else if !nest_ids.insert(nest.id.as_str()) {
            violations.push(Violation::DuplicateNestId {
                document: id.to_owned(),
                nest: nest.id.clone(),
            });
        }

        match nodes.get(nest.parent_node.as_str()) {
            None => violations.push(Violation::UnknownNestParent {
                document: id.to_owned(),
                nest: nest.id.clone(),
                parent: nest.parent_node.clone(),
            }),
            Some(node)
                if !matches!(
                    node.kind,
                    WorkflowNodeKind::PatternSlot | WorkflowNodeKind::NestGroup
                ) =>
            {
                violations.push(Violation::NestParentNotComposable {
                    document: id.to_owned(),
                    nest: nest.id.clone(),
                    parent: nest.parent_node.clone(),
                    kind: node.kind.to_string(),
                });
            }
            Some(_) => {}
        }

        let child_pinned = match nest.child.kind {
            WorkflowNestChildKind::Pattern => {
                nest.child.registry_id.is_some() && nest.child.version.is_some()
            }
            WorkflowNestChildKind::Workflow => nest.child.workflow_id.is_some(),
        };
        if !child_pinned {
            violations.push(Violation::IncompleteNestChild {
                document: id.to_owned(),
                nest: nest.id.clone(),
            });
        }
    }
}

/// Document-to-document edges through resolvable workflow children.
fn nest_references<'a>(
    documents: &[&'a WorkflowDocument],
    index: &HashMap<&'a str, &'a WorkflowDocument>,
) -> HashMap<&'a str, Vec<&'a str>> {
    documents
        .iter()
        .map(|document| {
            let targets = document
                .nests
                .iter()
                .filter_map(|nest| match nest.child.kind {
                    WorkflowNestChildKind::Workflow => nest
                        .child
                        .workflow_id
                        .as_deref()
                        .filter(|child| index.contains_key(child)),
                    WorkflowNestChildKind::Pattern => None,
                })
                .collect();
            (document.id.as_str(), targets)
        })
        .collect()
}

fn nest_depth<'a>(
    id: &'a str,
    index: &HashMap<&'a str, &'a WorkflowDocument>,
    memo: &mut HashMap<&'a str, usize>,
) -> usize {
    if let Some(&depth) = memo.get(id) {
        return depth;
    }
    let Some(document) = index.get(id) else {
        return 0;
    };
    // A placeholder keeps the walk total if a caller ever reaches this with a cycle present.
    memo.insert(id, 0);
    let mut depth = 0;
    for nest in &document.nests {
        let child_depth = match nest.child.kind {
            WorkflowNestChildKind::Workflow => nest
                .child
                .workflow_id
                .as_deref()
                .map_or(0, |child| nest_depth(child, index, memo)),
            WorkflowNestChildKind::Pattern => 0,
        };
        depth = depth.max(1 + child_depth);
    }
    memo.insert(id, depth);
    depth
}

/// Depth-first search returning the node path of the first cycle reached from `roots`.
///
/// Roots are walked in caller order, so the reported path is deterministic.
fn find_cycle<'a>(
    edges: &HashMap<&'a str, Vec<&'a str>>,
    roots: &[&'a str],
) -> Option<Vec<String>> {
    let mut finished: HashSet<&str> = HashSet::new();
    let mut path: Vec<&str> = Vec::new();
    for root in roots {
        if let Some(cycle) = walk(root, edges, &mut finished, &mut path) {
            return Some(cycle);
        }
    }
    None
}

fn walk<'a>(
    node: &'a str,
    edges: &HashMap<&'a str, Vec<&'a str>>,
    finished: &mut HashSet<&'a str>,
    path: &mut Vec<&'a str>,
) -> Option<Vec<String>> {
    if let Some(position) = path.iter().position(|entry| *entry == node) {
        let mut cycle: Vec<String> = path[position..]
            .iter()
            .map(|entry| (*entry).to_owned())
            .collect();
        cycle.push(node.to_owned());
        return Some(cycle);
    }
    if finished.contains(node) {
        return None;
    }
    path.push(node);
    for &next in edges.get(node).map_or(&[][..], Vec::as_slice) {
        if let Some(cycle) = walk(next, edges, finished, path) {
            return Some(cycle);
        }
    }
    path.pop();
    finished.insert(node);
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    fn document(id: &str, nodes: &Value, edges: &Value, nests: &Value) -> WorkflowDocument {
        serde_json::from_value(json!({
            "ir_version": 0,
            "id": id,
            "version": 0,
            "revision": 1,
            "state": "candidate",
            "meta": {
                "description": "structural fixture",
                "kind": "creator",
                "run_intents": [],
                "requires_capabilities": []
            },
            "entry": "entry",
            "terminal": ["entry"],
            "nodes": nodes,
            "edges": edges,
            "nests": nests,
            "assets": [],
            "signals": [],
            "roles": [],
            "effect_summary": "pure",
            "provenance": { "source": "compose" }
        }))
        .expect("fixture is a valid workflow document")
    }

    fn node(id: &str, kind: &str) -> Value {
        json!({ "id": id, "kind": kind, "effect_class": "pure" })
    }

    fn nest(id: &str, parent: &str, child: &Value) -> Value {
        json!({
            "id": id,
            "parent_node": parent,
            "child": child,
            "expansion": "defer_until_apply",
            "pin_policy": "exact"
        })
    }

    fn pattern(registry_id: &str, version: &str) -> Value {
        json!({ "kind": "pattern", "registry_id": registry_id, "version": version })
    }

    fn workflow(id: &str) -> Value {
        json!({ "kind": "workflow", "workflow_id": id })
    }

    fn slot_document(id: &str, nests: &Value) -> WorkflowDocument {
        document(
            id,
            &json!([node("entry", "outer_state"), node("slot", "pattern_slot")]),
            &json!([]),
            nests,
        )
    }

    fn group_document(id: &str, nests: &Value) -> WorkflowDocument {
        document(
            id,
            &json!([node("entry", "outer_state"), node("group", "nest_group")]),
            &json!([]),
            nests,
        )
    }

    #[test]
    fn accepts_a_well_formed_composition() {
        let leaf = slot_document(
            "leaf",
            &json!([nest("n2", "slot", &pattern("narrative.arc", "1.0.0"))]),
        );
        let root = group_document("root", &json!([nest("n1", "group", &workflow("leaf"))]));

        assert_eq!(validate(&[&root, &leaf]), Ok(()));
    }

    #[test]
    fn accepts_three_nest_levels() {
        let third = slot_document(
            "level-3",
            &json!([nest("n3", "slot", &pattern("story.turn", "1.0.0"))]),
        );
        let second = group_document(
            "level-2",
            &json!([nest("n2", "group", &workflow("level-3"))]),
        );
        let first = group_document(
            "level-1",
            &json!([nest("n1", "group", &workflow("level-2"))]),
        );

        assert_eq!(validate(&[&first, &second, &third]), Ok(()));
    }

    #[test]
    fn rejects_four_nest_levels() {
        let fourth = slot_document(
            "level-4",
            &json!([nest("n4", "slot", &pattern("story.turn", "1.0.0"))]),
        );
        let third = group_document(
            "level-3",
            &json!([nest("n3", "group", &workflow("level-4"))]),
        );
        let second = group_document(
            "level-2",
            &json!([nest("n2", "group", &workflow("level-3"))]),
        );
        let first = group_document(
            "level-1",
            &json!([nest("n1", "group", &workflow("level-2"))]),
        );

        let violations = validate(&[&first, &second, &third, &fourth]).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::NestDepthExceeded {
                document: "level-1".to_owned(),
                depth: 4,
            }]
        );
    }

    #[test]
    fn rejects_a_workflow_reference_cycle() {
        let a = group_document("a", &json!([nest("n1", "group", &workflow("b"))]));
        let b = group_document("b", &json!([nest("n2", "group", &workflow("a"))]));

        let violations = validate(&[&a, &b]).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::NestCycle {
                path: vec!["a".to_owned(), "b".to_owned(), "a".to_owned()],
            }]
        );
    }

    #[test]
    fn rejects_a_self_referencing_workflow() {
        let a = group_document("a", &json!([nest("n1", "group", &workflow("a"))]));

        let violations = validate(&[&a]).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::NestCycle {
                path: vec!["a".to_owned(), "a".to_owned()],
            }]
        );
    }

    #[test]
    fn rejects_duplicate_and_empty_nest_ids() {
        let doc = slot_document(
            "root",
            &json!([
                nest("n1", "slot", &pattern("story.arc", "1.0.0")),
                nest("n1", "slot", &pattern("story.turn", "1.0.0")),
                nest("", "slot", &pattern("story.arc", "2.0.0")),
            ]),
        );

        let violations = validate(&[&doc]).unwrap_err();
        assert_eq!(
            violations,
            vec![
                Violation::DuplicateNestId {
                    document: "root".to_owned(),
                    nest: "n1".to_owned(),
                },
                Violation::EmptyNestId {
                    document: "root".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn rejects_nests_without_a_stable_address() {
        let doc = document(
            "root",
            &json!([node("entry", "outer_state"), node("slot", "pattern_slot")]),
            &json!([]),
            &json!([
                nest("n1", "ghost", &pattern("story.arc", "1.0.0")),
                nest("n2", "entry", &pattern("story.arc", "1.0.0")),
                nest("n3", "slot", &json!({ "kind": "workflow" })),
            ]),
        );

        let violations = validate(&[&doc]).unwrap_err();
        assert_eq!(
            violations,
            vec![
                Violation::UnknownNestParent {
                    document: "root".to_owned(),
                    nest: "n1".to_owned(),
                    parent: "ghost".to_owned(),
                },
                Violation::NestParentNotComposable {
                    document: "root".to_owned(),
                    nest: "n2".to_owned(),
                    parent: "entry".to_owned(),
                    kind: "outer_state".to_owned(),
                },
                Violation::IncompleteNestChild {
                    document: "root".to_owned(),
                    nest: "n3".to_owned(),
                },
            ]
        );
    }

    #[test]
    fn rejects_a_node_graph_cycle() {
        let doc = document(
            "root",
            &json!([node("a", "outer_state"), node("b", "outer_state")]),
            &json!([
                { "id": "e1", "from": "a", "to": "b", "kind": "linear" },
                { "id": "e2", "from": "b", "to": "a", "kind": "linear" },
            ]),
            &json!([]),
        );

        let violations = validate(&[&doc]).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::GraphCycle {
                document: "root".to_owned(),
                path: vec!["a".to_owned(), "b".to_owned(), "a".to_owned()],
            }]
        );
    }

    #[test]
    fn rejects_a_dangling_edge_endpoint() {
        let doc = document(
            "root",
            &json!([node("a", "outer_state")]),
            &json!([{ "id": "e1", "from": "a", "to": "missing", "kind": "linear" }]),
            &json!([]),
        );

        let violations = validate(&[&doc]).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::DanglingEdge {
                document: "root".to_owned(),
                edge: "e1".to_owned(),
                node: "missing".to_owned(),
            }]
        );
    }

    #[test]
    fn validates_dependency_targets() {
        let dangling = document(
            "root",
            &json!([
                node("a", "inner_step"),
                {
                    "id": "b",
                    "kind": "inner_step",
                    "effect_class": "pure",
                    "depends_on": ["missing"]
                },
            ]),
            &json!([]),
            &json!([]),
        );
        let violations = validate(&[&dangling]).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::DanglingDependency {
                document: "root".to_owned(),
                node: "b".to_owned(),
                dependency: "missing".to_owned(),
            }]
        );

        let resolved = document(
            "root",
            &json!([
                node("a", "inner_step"),
                {
                    "id": "b",
                    "kind": "inner_step",
                    "effect_class": "pure",
                    "depends_on": ["a"]
                },
            ]),
            &json!([]),
            &json!([]),
        );
        assert_eq!(validate(&[&resolved]), Ok(()));
    }

    #[test]
    fn unresolved_workflow_child_is_a_leaf() {
        let doc = group_document("root", &json!([nest("n1", "group", &workflow("absent"))]));

        assert_eq!(validate(&[&doc]), Ok(()));
    }
}
