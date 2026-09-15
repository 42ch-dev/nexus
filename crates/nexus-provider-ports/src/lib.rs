//! Schema-derived provider port contracts (v1.189 P2-T1).

use async_trait::async_trait;
use nexus_contracts::{CoreError, ProviderCall, ProviderEventBatch, ProviderReply};

pub type ProviderResult<T> = Result<T, CoreError>;

/// Pull-based provider effect port — probe/launch/execute/cancel/shutdown via `call`,
/// bounded event delivery via `next`.
#[async_trait]
pub trait ProviderPort: Send + Sync {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply>;
    async fn next(
        &self,
        operation_id: String,
        max_events: u32,
        max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch>;
}
