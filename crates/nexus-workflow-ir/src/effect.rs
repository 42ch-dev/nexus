//! Effect rollup: the one lattice order over node effect classes.
//!
//! `pure < read < write_candidate < external`. A document summary is the join of every node's
//! declared class, so grouping or nesting never hides a write or external effect. A v0 fragment
//! leaves nest children unresolved, so the join covers this document's nodes only.

use crate::{WorkflowDocumentEffectSummary, WorkflowNode, WorkflowNodeEffectClass};

/// Rank one node class in the shared order.
const fn effect_rank(class: WorkflowNodeEffectClass) -> u8 {
    match class {
        WorkflowNodeEffectClass::External => 3,
        WorkflowNodeEffectClass::WriteCandidate => 2,
        WorkflowNodeEffectClass::Read => 1,
        WorkflowNodeEffectClass::Pure => 0,
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
