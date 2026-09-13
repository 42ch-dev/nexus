//! Wraps a JS [`ProviderPort`] and injects Rust-admitted recipes for probe/launch.

use std::sync::Arc;

use async_trait::async_trait;
use nexus_agent_host::providers::recipe_admission::reject_caller_recipe_payload;
use nexus_agent_host::HostManager;
use nexus_contracts::{
    CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply,
};
use nexus_contracts::provider_call::ProviderCallMethod;
use nexus_provider_ports::{ProviderPort, ProviderResult};

fn map_host_error(err: nexus_agent_host::error::HostError) -> CoreError {
    CoreError {
        code: CoreErrorCode::InvalidInput,
        message: err.to_string(),
        details: Default::default(),
        http_status: Some(400),
    }
}

fn provider_id_from_payload(payload: &serde_json::Map<String, serde_json::Value>) -> ProviderResult<String> {
    payload
        .get("provider_id")
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .ok_or_else(|| CoreError {
            code: CoreErrorCode::InvalidInput,
            message: "provider_id required".into(),
            details: Default::default(),
            http_status: Some(400),
        })
}

pub struct AdmittingProviderPort {
    host: Arc<HostManager>,
    inner: Arc<dyn ProviderPort>,
}

impl AdmittingProviderPort {
    pub fn new(host: Arc<HostManager>, inner: Arc<dyn ProviderPort>) -> Self {
        Self { host, inner }
    }

    async fn admit_probe_or_launch(&self, request: &mut ProviderCall) -> ProviderResult<()> {
        reject_caller_recipe_payload(&request.payload).map_err(map_host_error)?;
        let provider_id = nexus_agent_host::ids::ProviderId::new(provider_id_from_payload(&request.payload)?);
        let admitted = self
            .host
            .admit_validated_provider_recipe(&provider_id)
            .await
            .map_err(map_host_error)?;
        let recipe_value = serde_json::to_value(&admitted).map_err(|e| CoreError {
            code: CoreErrorCode::Internal,
            message: format!("recipe_admission_serialize: {e}"),
            details: Default::default(),
            http_status: Some(500),
        })?;
        request.payload.insert("recipe".to_string(), recipe_value);
        Ok(())
    }
}

#[async_trait]
impl ProviderPort for AdmittingProviderPort {
    async fn call(&self, mut request: ProviderCall) -> ProviderResult<ProviderReply> {
        if matches!(
            request.method,
            ProviderCallMethod::Probe | ProviderCallMethod::Launch
        ) {
            self.admit_probe_or_launch(&mut request).await?;
        }
        self.inner.call(request).await
    }

    async fn next(
        &self,
        operation_id: String,
        max_events: u32,
        max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        self.inner.next(operation_id, max_events, max_bytes).await
    }
}
