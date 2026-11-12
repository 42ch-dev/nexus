//! Simulate: the non-mutating static walk over a candidate IR.
//!
//! The request carries the whole candidate because nothing is applied yet, so this route
//! reads, validates and returns; it persists nothing, mutates nothing and owns no revision.
//! Fresh recording registry per call, no production holder swap, no executor and no provider.

use crate::{CoreError, CoreResult, CoreService, PresetError, Principal};
use nexus_contracts::{
    NexusWorkflowDocumentState, SimulationTrajectory, StrategySimulationRequest,
};
use nexus_orchestration::{simulate_document, CapabilityRegistryHolder, SimulationOptions};
use nexus_workflow_ir::WorkflowDocument;

impl CoreService {
    /// # Errors
    ///
    /// Returns `CoreError` when the principal is rejected, the path id disagrees with the
    /// document, the document is not a candidate, the body is not a valid IR, or the static
    /// walk cannot produce a deterministic trajectory.
    pub async fn simulate_strategy(
        &self,
        principal: &Principal,
        strategy_id: String,
        request: StrategySimulationRequest,
    ) -> CoreResult<SimulationTrajectory> {
        self.verify_principal(principal)?;
        let wire = request.document;
        if strategy_id != wire.id.as_str() {
            return Err(PresetError::Rejected {
                code: "strategy_id_mismatch".into(),
                message: format!(
                    "strategy_id in body ('{}') does not match URL path ('{strategy_id}')",
                    wire.id.as_str()
                ),
            }
            .into());
        }
        if wire.state != NexusWorkflowDocumentState::Candidate {
            return Err(PresetError::Rejected {
                code: "strategy_not_candidate".into(),
                message: format!(
                    "only a candidate document can be simulated, got `{}`",
                    wire.state
                ),
            }
            .into());
        }
        // The route wire type and the walk's IR type are two generated facades over one JSON
        // shape; the body is already JSON, so this is the boundary that joins them.
        let document: WorkflowDocument = serde_json::to_value(&wire)
            .and_then(serde_json::from_value)
            .map_err(|error| CoreError::InvalidInput {
                field: "document".into(),
                reason: error.to_string(),
            })?;
        simulate_document(
            &document,
            SimulationOptions {
                capabilities: CapabilityRegistryHolder::new(),
            },
        )
        .map_err(|error| CoreError::InvalidInput {
            field: "document".into(),
            reason: error.to_string(),
        })
    }
}
