//! Q2 structural validation: nest depth, reference cycles and stable nest ids.
//!
//! [`validate`] takes a document set and the pattern bodies the caller could resolve: the document
//! under validation plus every child workflow and every resolved pattern fragment. A referenced
//! child outside the set is a leaf, so validating one document never requires a registry. A
//! resolved fragment is not a leaf — it is held to its own node-graph checks, and its own nests
//! join the same walk, which is what makes pattern-to-pattern cycles and combined pattern/workflow
//! depth reachable.

use crate::{
    WorkflowDocument, WorkflowNestBinding, WorkflowNestChild, WorkflowNestChildKind, WorkflowNode,
    WorkflowNodeEffectClass, WorkflowNodeKind,
};
use std::collections::{HashMap, HashSet};

/// Q2: at most three unexpanded nest levels. v0 represents no expansion, so every nest binding
/// counts as one level.
pub const MAX_NEST_DEPTH: usize = 3;

/// A pattern body a document's pattern pins may reach.
///
/// The caller resolves pins it can read; a pin it cannot resolve stays a leaf, exactly as an
/// absent child workflow does.
#[derive(Debug, Clone, Copy)]
pub struct PatternBody<'a> {
    /// Registry id of the package this body belongs to.
    pub registry_id: &'a str,
    /// Exact version label of the package.
    pub version: &'a str,
    /// What the resolution found for the pin.
    pub body: PatternBodyKind<'a>,
}

/// What a resolved pattern pin contributed to the walk.
#[derive(Debug, Clone, Copy)]
pub enum PatternBodyKind<'a> {
    /// The package is available: its fragment's nests are walked like a child document's.
    Fragment(&'a WorkflowDocument),
    /// Only the package's declared effect class is known (an unavailable package): a leaf in the
    /// nest walk that still contributes its class to the rollup.
    Declared(WorkflowNodeEffectClass),
}

impl PatternBody<'_> {
    /// The label this body is addressed by in a nest cycle path or depth report.
    #[must_use]
    pub fn label(&self) -> String {
        pattern_label(self.registry_id, self.version)
    }
}

/// One document or pattern body is addressed by this label in the nest walk.
fn pattern_label(registry_id: &str, version: &str) -> String {
    format!("{registry_id}@{version}")
}

/// The exact `{id, version}` a pattern nest child pins.
///
/// Both halves must be present and non-empty: an empty half names no package, so such a child is a
/// malformed pin rather than a pin whose package is missing, and [`Violation::IncompleteNestChild`]
/// refuses it. Every walk that reads a pin asks this one function — structural validation, the nest
/// walk, and compose's resolution — so none of them counts a half-pin the others skip.
#[must_use]
pub fn pattern_pin(child: &WorkflowNestChild) -> Option<(&str, &str)> {
    if child.kind != WorkflowNestChildKind::Pattern {
        return None;
    }
    let id = child.registry_id.as_deref().filter(|id| !id.is_empty())?;
    let version = child
        .version
        .as_deref()
        .filter(|version| !version.is_empty())?;
    Some((id, version))
}

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
/// `patterns` carries the pattern bodies the caller resolved; a pin with no entry there is a leaf,
/// so this never requires a registry. A resolved fragment body gets [`validate_fragment`]'s four
/// node-graph checks and nothing more: its nests join the nest walk below for cycles and depth, and
/// [`validate_nests`] runs on documents only.
///
/// # Errors
///
/// Returns every violation found. An empty vector is never returned.
pub fn validate(
    documents: &[&WorkflowDocument],
    patterns: &[PatternBody<'_>],
) -> Result<(), Vec<Violation>> {
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
    for pattern in patterns {
        if let PatternBodyKind::Fragment(fragment) = pattern.body {
            validate_fragment(fragment, &mut violations);
        }
    }

    let graph = NestGraph::new(documents, patterns);
    let roots: Vec<String> = documents
        .iter()
        .map(|document| document.id.as_str().to_owned())
        .collect();
    if let Some(path) = find_cycle(graph.edges(), &roots) {
        violations.push(Violation::NestCycle { path });
    } else {
        // A reference cycle has no finite depth, so depth is skipped for the whole set when
        // any cycle exists.
        let mut memo = HashMap::new();
        for root in &roots {
            let depth = nest_depth(root, graph.edges(), &mut memo);
            if depth > MAX_NEST_DEPTH {
                violations.push(Violation::NestDepthExceeded {
                    document: root.clone(),
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
    let nodes = validate_node_graph(document, violations);
    validate_nests(document, &nodes, violations);
}

/// A resolved pattern fragment, held to the four checks its own graph answers and no others:
/// duplicate node ids, edge endpoints and `depends_on` entries that name no node, and node cycles.
///
/// [`validate_nests`]' nest-context rules (stable unique nest ids, a composable parent, a child that
/// pins what its kind requires) run on documents only, deliberately. A fragment is a body, not a
/// document: its nests name the body's own nodes, and the walks that read them answer what applies
/// here — the nest walk below judges cycles and depth, and compose's flatten re-issues every nest id
/// and refuses a nest whose parent the body does not hold. The remaining rules say how a nest sits in
/// a level, so they are answered on the level each nest lands in, never on the body.
fn validate_fragment(fragment: &WorkflowDocument, violations: &mut Vec<Violation>) {
    validate_node_graph(fragment, violations);
}

fn validate_node_graph<'a>(
    document: &'a WorkflowDocument,
    violations: &mut Vec<Violation>,
) -> HashMap<&'a str, &'a WorkflowNode> {
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

    nodes
}

/// The nest-context rules a document's own bindings answer: stable unique ids, an addressable
/// composable parent, and a child that pins what its kind requires.
fn validate_nests(
    document: &WorkflowDocument,
    nodes: &HashMap<&str, &WorkflowNode>,
    violations: &mut Vec<Violation>,
) {
    let id = document.id.as_str();
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
            WorkflowNestChildKind::Pattern => pattern_pin(&nest.child).is_some(),
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

/// The nest-child graph over a document set and its resolved pattern bodies.
///
/// One builder serves cycles, depth and the effect rollup, so no caller re-derives the edges and
/// no caller can forget that a resolved fragment is not a leaf.
pub(crate) struct NestGraph<'a> {
    /// Child labels per body label: a document id or a `registry_id@version` pin label.
    edges: HashMap<String, Vec<String>>,
    /// Walkable bodies by label: documents and resolved fragments.
    documents: HashMap<String, &'a WorkflowDocument>,
    /// Pins whose package was unavailable, so only the declared class is known.
    declared: HashMap<String, WorkflowNodeEffectClass>,
}

impl<'a> NestGraph<'a> {
    pub(crate) fn new(documents: &[&'a WorkflowDocument], patterns: &'a [PatternBody<'a>]) -> Self {
        let mut graph = Self {
            edges: HashMap::new(),
            documents: HashMap::new(),
            declared: HashMap::new(),
        };
        for document in documents {
            graph
                .documents
                .entry(document.id.to_string())
                .or_insert(document);
        }
        // A pin listed twice keeps its first body; a declared body never shadows a fragment.
        for pattern in patterns {
            match pattern.body {
                PatternBodyKind::Fragment(fragment) => {
                    graph.documents.entry(pattern.label()).or_insert(fragment);
                }
                PatternBodyKind::Declared(class) => {
                    graph.declared.entry(pattern.label()).or_insert(class);
                }
            }
        }

        let mut edges = HashMap::new();
        for (label, document) in &graph.documents {
            let targets = document.nests.iter().filter_map(nest_child_label).collect();
            edges.insert(label.clone(), targets);
        }
        graph.edges = edges;
        graph
    }

    pub(crate) const fn edges(&self) -> &HashMap<String, Vec<String>> {
        &self.edges
    }

    /// The nodes of one walkable body, for the effect rollup.
    pub(crate) fn nodes(&self, label: &str) -> Option<&'a [WorkflowNode]> {
        self.documents
            .get(label)
            .map(|document| document.nodes.as_slice())
    }

    /// The declared class of a pattern body whose package could not be loaded.
    pub(crate) fn declared_class(&self, label: &str) -> Option<WorkflowNodeEffectClass> {
        self.declared.get(label).copied()
    }
}

/// The label of a nest's child. An unresolved child keeps its label as a leaf so the depth walk
/// still counts the binding as one unexpanded level; a malformed pin has no label at all.
fn nest_child_label(nest: &WorkflowNestBinding) -> Option<String> {
    match nest.child.kind {
        WorkflowNestChildKind::Pattern => {
            let (registry_id, version) = pattern_pin(&nest.child)?;
            Some(pattern_label(registry_id, version))
        }
        WorkflowNestChildKind::Workflow => nest.child.workflow_id.clone(),
    }
}

/// Depth of one body: the longest nest chain below it, with a leaf at zero.
fn nest_depth(
    label: &str,
    edges: &HashMap<String, Vec<String>>,
    memo: &mut HashMap<String, usize>,
) -> usize {
    if let Some(&depth) = memo.get(label) {
        return depth;
    }
    // A placeholder keeps the walk total if a caller ever reaches this with a cycle present.
    memo.insert(label.to_owned(), 0);
    let mut depth = 0;
    for child in edges.get(label).map_or(&[][..], Vec::as_slice) {
        depth = depth.max(1 + nest_depth(child, edges, memo));
    }
    memo.insert(label.to_owned(), depth);
    depth
}

/// Depth-first search returning the label path of the first cycle reached from `roots`.
///
/// Roots are walked in caller order, so the reported path is deterministic. Keys are node ids for
/// the node graph and document or pattern labels for the nest graph.
fn find_cycle<K>(edges: &HashMap<K, Vec<K>>, roots: &[K]) -> Option<Vec<String>>
where
    K: Eq + std::hash::Hash + Clone + std::fmt::Display,
{
    let mut finished: HashSet<K> = HashSet::new();
    let mut path: Vec<K> = Vec::new();
    for root in roots {
        if let Some(cycle) = walk(root, edges, &mut finished, &mut path) {
            return Some(cycle);
        }
    }
    None
}

fn walk<K>(
    node: &K,
    edges: &HashMap<K, Vec<K>>,
    finished: &mut HashSet<K>,
    path: &mut Vec<K>,
) -> Option<Vec<String>>
where
    K: Eq + std::hash::Hash + Clone + std::fmt::Display,
{
    if let Some(position) = path.iter().position(|entry| entry == node) {
        let mut cycle: Vec<String> = path[position..].iter().map(ToString::to_string).collect();
        cycle.push(node.to_string());
        return Some(cycle);
    }
    if finished.contains(node) {
        return None;
    }
    path.push(node.clone());
    for next in edges.get(node).map_or(&[][..], Vec::as_slice) {
        if let Some(cycle) = walk(next, edges, finished, path) {
            return Some(cycle);
        }
    }
    path.pop();
    finished.insert(node.clone());
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
    fn refuses_a_resolved_fragment_that_dangles_an_edge() {
        let root = group_document(
            "root",
            &json!([nest("n1", "group", &pattern("broken", "1.0.0"))]),
        );
        let broken = document(
            "broken",
            &json!([node("entry", "inner_step")]),
            &json!([{ "id": "entry->ghost", "from": "entry", "to": "ghost", "kind": "linear" }]),
            &json!([]),
        );
        let bodies = [PatternBody {
            registry_id: "broken",
            version: "1.0.0",
            body: PatternBodyKind::Fragment(&broken),
        }];

        let violations = validate(&[&root], &bodies).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::DanglingEdge {
                document: "broken".to_owned(),
                edge: "entry->ghost".to_owned(),
                node: "ghost".to_owned(),
            }]
        );
    }

    #[test]
    fn accepts_a_well_formed_composition() {
        let leaf = slot_document(
            "leaf",
            &json!([nest("n2", "slot", &pattern("narrative.arc", "1.0.0"))]),
        );
        let root = group_document("root", &json!([nest("n1", "group", &workflow("leaf"))]));

        assert_eq!(validate(&[&root, &leaf], &[]), Ok(()));
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

        assert_eq!(validate(&[&first, &second, &third], &[]), Ok(()));
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

        let violations = validate(&[&first, &second, &third, &fourth], &[]).unwrap_err();
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

        let violations = validate(&[&a, &b], &[]).unwrap_err();
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

        let violations = validate(&[&a], &[]).unwrap_err();
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

        let violations = validate(&[&doc], &[]).unwrap_err();
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

        let violations = validate(&[&doc], &[]).unwrap_err();
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
    fn rejects_a_pattern_pin_with_an_empty_half() {
        // An empty half names no package: the child omits its pin instead of pinning one, so a
        // document carrying it never reaches the walks that only follow resolved pins.
        let doc = slot_document(
            "root",
            &json!([
                nest("n1", "slot", &pattern("", "1.0.0")),
                nest("n2", "slot", &pattern("story.arc", "")),
            ]),
        );

        let violations = validate(&[&doc], &[]).unwrap_err();
        assert_eq!(
            violations,
            vec![
                Violation::IncompleteNestChild {
                    document: "root".to_owned(),
                    nest: "n1".to_owned(),
                },
                Violation::IncompleteNestChild {
                    document: "root".to_owned(),
                    nest: "n2".to_owned(),
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

        let violations = validate(&[&doc], &[]).unwrap_err();
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

        let violations = validate(&[&doc], &[]).unwrap_err();
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
        let violations = validate(&[&dangling], &[]).unwrap_err();
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
        assert_eq!(validate(&[&resolved], &[]), Ok(()));
    }

    #[test]
    fn unresolved_workflow_child_is_a_leaf() {
        let doc = group_document("root", &json!([nest("n1", "group", &workflow("absent"))]));

        assert_eq!(validate(&[&doc], &[]), Ok(()));
    }

    #[test]
    fn rejects_a_pattern_reference_cycle() {
        // Two published patterns that nest each other: only a resolved fragment makes this
        // reachable, because the document itself nests a leaf pattern pin.
        let a = slot_document("a", &json!([nest("n1", "slot", &pattern("b", "1.0.0"))]));
        let b = slot_document("b", &json!([nest("n2", "slot", &pattern("a", "1.0.0"))]));
        let patterns = [fragment("a", &a), fragment("b", &b)];
        let root = slot_document("root", &json!([nest("n1", "slot", &pattern("a", "1.0.0"))]));

        let violations = validate(&[&root], &patterns).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::NestCycle {
                path: vec![
                    "a@1.0.0".to_owned(),
                    "b@1.0.0".to_owned(),
                    "a@1.0.0".to_owned(),
                ],
            }]
        );
    }

    #[test]
    fn a_self_nesting_pattern_is_a_cycle() {
        let a = slot_document("a", &json!([nest("n1", "slot", &pattern("a", "1.0.0"))]));
        let root = slot_document("root", &json!([nest("n1", "slot", &pattern("a", "1.0.0"))]));

        let violations = validate(&[&root], &[fragment("a", &a)]).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::NestCycle {
                path: vec!["a@1.0.0".to_owned(), "a@1.0.0".to_owned()],
            }]
        );
    }

    #[test]
    fn accepts_three_levels_combined_across_patterns_and_workflows() {
        let first = slot_document("p", &json!([nest("n2", "slot", &pattern("q", "1.0.0"))]));
        let second = slot_document("q", &json!([])); // a leaf pattern body
        let wf = group_document("wf", &json!([nest("n1", "group", &pattern("p", "1.0.0"))]));
        let root = group_document("root", &json!([nest("n0", "group", &workflow("wf"))]));

        let patterns = [fragment("p", &first), fragment("q", &second)];
        assert_eq!(validate(&[&root, &wf], &patterns), Ok(()));
    }

    #[test]
    fn pattern_bodies_count_toward_the_combined_depth() {
        let p = slot_document("p", &json!([nest("n2", "slot", &pattern("q", "1.0.0"))]));
        let q = slot_document("q", &json!([nest("n3", "slot", &pattern("r", "1.0.0"))]));
        let r = slot_document("r", &json!([]));
        let wf = group_document("wf", &json!([nest("n1", "group", &pattern("p", "1.0.0"))]));
        let root = group_document("root", &json!([nest("n0", "group", &workflow("wf"))]));

        let patterns = [fragment("p", &p), fragment("q", &q), fragment("r", &r)];
        let violations = validate(&[&root, &wf], &patterns).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::NestDepthExceeded {
                document: "root".to_owned(),
                depth: 4,
            }]
        );
    }

    #[test]
    fn a_declared_pattern_body_counts_one_level_without_a_walk() {
        let q = slot_document("q", &json!([nest("n3", "slot", &pattern("gone", "1.0.0"))]));
        let p = slot_document("p", &json!([nest("n2", "slot", &pattern("q", "1.0.0"))]));
        let wf = group_document("wf", &json!([nest("n1", "group", &pattern("p", "1.0.0"))]));
        let root = group_document("root", &json!([nest("n0", "group", &workflow("wf"))]));

        let patterns = [
            fragment("p", &p),
            fragment("q", &q),
            PatternBody {
                registry_id: "gone",
                version: "1.0.0",
                body: PatternBodyKind::Declared(WorkflowNodeEffectClass::Pure),
            },
        ];
        let violations = validate(&[&root, &wf], &patterns).unwrap_err();
        assert_eq!(
            violations,
            vec![Violation::NestDepthExceeded {
                document: "root".to_owned(),
                depth: 4,
            }]
        );
    }

    /// A resolved pattern body, pinned at `1.0.0`.
    fn fragment<'a>(registry_id: &'a str, document: &'a WorkflowDocument) -> PatternBody<'a> {
        PatternBody {
            registry_id,
            version: "1.0.0",
            body: PatternBodyKind::Fragment(document),
        }
    }
}
