use async_trait::async_trait;
use nexus_contracts::{CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_provider_ports::{ProviderPort, ProviderResult};

/// Placeholder port until P2-T3 wires `ProviderPortAdapter` over `HostFacade`.
pub struct StubProviderPort;

fn stub_err() -> CoreError {
    CoreError {
        code: CoreErrorCode::Uninitialized,
        message: "provider port awaits P2-T3 host integration".into(),
        details: Default::default(),
        http_status: Some(409),
    }
}

#[async_trait]
impl ProviderPort for StubProviderPort {
    async fn call(&self, _request: ProviderCall) -> ProviderResult<ProviderReply> {
        Err(stub_err())
    }

    async fn next(
        &self,
        _operation_id: String,
        _max_events: u32,
        _max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        Err(stub_err())
    }
}
