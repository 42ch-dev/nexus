//! No-compute stub: rejects without WASM dependencies.

use super::NexusAdapter;
use crate::{ComputablePort, ProjectRequest, ProjectResponse, SpokeRejectCode, SpokeResult};
use crate::{ComputeRequest, ComputeResponse};
use async_trait::async_trait;
use spoke_operations::spoke_reject;

#[async_trait]
impl ComputablePort for NexusAdapter<'_> {
    async fn project(&self, _request: ProjectRequest) -> SpokeResult<ProjectResponse> {
        spoke_reject(
            SpokeRejectCode::CapabilityPortMissing,
            "compute feature disabled",
            None,
        )
    }

    async fn compute(&self, _request: ComputeRequest) -> SpokeResult<ComputeResponse> {
        spoke_reject(
            SpokeRejectCode::CapabilityPortMissing,
            "compute feature disabled",
            None,
        )
    }
}
