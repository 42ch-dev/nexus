//! Static Simulate: a reachability walk over a candidate Workflow IR.
//!
//! Nothing executes here. No node is entered, no capability is dispatched and no production
//! executor or provider is constructed: the walk reads the document's nodes, edges and nest
//! bindings and predicts what a default-branch run *would* reach. The per-call registry in
//! [`SimulationOptions`] is the seam preflight T1 established (D9) — it is never resolved
//! against, which is what makes a zero dispatch count structural rather than incidental.
//!
//! Static predictions are an over-approximation: a predicted wait may never fire, and a
//! judge's non-default branches (and everything reachable only through them) are invisible.

use crate::CapabilityRegistryHolder;
use nexus_contracts::{
    SimulationPredictedWait, SimulationPredictedWaitReason, SimulationTrajectory,
    SimulationTrajectoryEffectSummary,
};
use nexus_workflow_ir::{
    content_hash, WorkflowDocument, WorkflowEdge, WorkflowEdgeKind, WorkflowExitWhenKind,
    WorkflowNode, WorkflowNodeEffectClass, WorkflowNodeKind, WorkflowSignalBindingAction,
};
use std::collections::{HashMap, HashSet};

/// v0 emits exactly one level: reachability, default branch only.
const SIMULATION_LEVEL: &str = "reachability";
/// Trajectory contract revision for this shape.
const SCHEMA_VERSION: i64 = 1;

/// Per-call simulation inputs. The registry is required so a future walkable node can only
/// ever dispatch through the caller's own recording registry, never a shared production one.
pub struct SimulationOptions {
    /// Fresh per-call registry; this walk never dispatches a capability through it.
    pub capabilities: CapabilityRegistryHolder,
}

/// A deterministic reason the trajectory cannot be produced.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SimulationError {
    /// The declared entry is not a node of this document.
    #[error("entry node `{0}` is not declared")]
    EntryNotFound(String),
    /// A judged node must declare exactly one `default` edge; never guess a branch.
    #[error("judge node `{node}` declares {count} `default` edges; exactly one is required")]
    JudgeBranch { node: String, count: usize },
    /// The approval identity cannot be computed over this document.
    #[error(transparent)]
    Hash(#[from] nexus_workflow_ir::CanonicalHashError),
}

/// Walk `document` from its entry and return the deterministic trajectory.
///
/// # Errors
///
/// Returns [`SimulationError`] when the entry is undeclared, a judged node does not declare
/// exactly one `default` edge, or the canonical content hash cannot be computed.
pub fn simulate_document(
    document: &WorkflowDocument,
    options: SimulationOptions,
) -> Result<SimulationTrajectory, SimulationError> {
    // Held for the walk's lifetime and never read: nothing here resolves a capability (D9).
    let SimulationOptions { capabilities: _ } = options;

    let mut by_id: HashMap<&str, &WorkflowNode> = HashMap::with_capacity(document.nodes.len());
    for node in &document.nodes {
        by_id.insert(node.id.as_str(), node);
    }
    let mut outgoing: HashMap<&str, Vec<&WorkflowEdge>> = HashMap::new();
    for edge in &document.edges {
        outgoing.entry(edge.from.as_str()).or_default().push(edge);
    }

    if !by_id.contains_key(document.entry.as_str()) {
        return Err(SimulationError::EntryNotFound(document.entry.clone()));
    }

    let mut queue = std::collections::VecDeque::from([document.entry.as_str()]);
    let mut seen = HashSet::from([document.entry.as_str()]);
    let mut visited: Vec<&WorkflowNode> = Vec::with_capacity(document.nodes.len());
    let mut stubbed_judges: Vec<&str> = Vec::new();
    let mut waits: Vec<SimulationPredictedWait> = Vec::new();

    while let Some(id) = queue.pop_front() {
        let node = by_id[id];
        visited.push(node);

        let manual_wait = node.kind == WorkflowNodeKind::ManualWait
            || node
                .exit_when
                .as_ref()
                .is_some_and(|exit| exit.kind == WorkflowExitWhenKind::Manual);
        if manual_wait {
            waits.push(SimulationPredictedWait {
                reason: SimulationPredictedWaitReason::Manual,
                subject: node.id.clone(),
            });
        }

        let outgoing = outgoing.get(id).map(Vec::as_slice).unwrap_or_default();
        let judged = node
            .exit_when
            .as_ref()
            .is_some_and(|exit| exit.kind == WorkflowExitWhenKind::LlmJudge);
        let next: Vec<&str> = if judged {
            let defaults: Vec<&str> = outgoing
                .iter()
                .filter(|edge| edge.kind == WorkflowEdgeKind::Default)
                .map(|edge| edge.to.as_str())
                .collect();
            if defaults.len() != 1 {
                return Err(SimulationError::JudgeBranch {
                    node: node.id.clone(),
                    count: defaults.len(),
                });
            }
            stubbed_judges.push(node.id.as_str());
            defaults
        } else {
            outgoing.iter().map(|edge| edge.to.as_str()).collect()
        };
        for target in next {
            // An edge to an undeclared node is not reachable without a node to walk.
            if by_id.contains_key(target) && seen.insert(target) {
                queue.push_back(target);
            }
        }
    }

    // Signal pauses are document-global: the IR gives them no target node.
    for signal in &document.signals {
        if signal.action == WorkflowSignalBindingAction::Pause {
            waits.push(SimulationPredictedWait {
                reason: SimulationPredictedWaitReason::Signal,
                subject: signal.name.clone(),
            });
        }
    }

    let mut nodes_visited: Vec<String> = visited.iter().map(|node| node.id.clone()).collect();
    nodes_visited.sort_unstable();
    // Same prediction can be recorded from two visiting paths; the contract is a set.
    waits.sort_by(|left, right| (left.reason, &left.subject).cmp(&(right.reason, &right.subject)));
    waits.dedup_by(|left, right| left.reason == right.reason && left.subject == right.subject);
    let mut stubbed_judges: Vec<String> = stubbed_judges
        .iter()
        .map(|node| (*node).to_string())
        .collect();
    stubbed_judges.sort_unstable();

    // The generated `Ord` is the contract order `pure < read < write_candidate < external`,
    // so the rollup is one max over visited nodes and their expanded descendants.
    let effect = visited
        .iter()
        .map(|node| node.effect_class)
        .max()
        .unwrap_or(WorkflowNodeEffectClass::Pure);

    Ok(SimulationTrajectory {
        schema_version: SCHEMA_VERSION,
        ir_content_hash: content_hash(document)?,
        simulation_level: SIMULATION_LEVEL.to_string(),
        nodes_visited,
        predicted_waits: waits,
        effect_summary: effect_summary(effect),
        stubbed_judges,
    })
}

/// The one conversion from the IR effect lattice to the trajectory wire enum.
fn effect_summary(effect: WorkflowNodeEffectClass) -> SimulationTrajectoryEffectSummary {
    match effect {
        WorkflowNodeEffectClass::Pure => SimulationTrajectoryEffectSummary::Pure,
        WorkflowNodeEffectClass::Read => SimulationTrajectoryEffectSummary::Read,
        WorkflowNodeEffectClass::WriteCandidate => SimulationTrajectoryEffectSummary::WriteCandidate,
        WorkflowNodeEffectClass::External => SimulationTrajectoryEffectSummary::External,
    }
}

#[cfg(test)]
mod tests;
