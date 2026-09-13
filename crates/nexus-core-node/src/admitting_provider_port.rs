//! Wraps a JS [`ProviderPort`] and injects Rust-admitted recipes for probe/launch.

use std::sync::Arc;

use async_trait::async_trait;
use nexus_agent_host::providers::recipe_admission::reject_caller_recipe_payload;
use nexus_agent_host::HostManager;
use nexus_contracts::provider_call::ProviderCallMethod;
use nexus_contracts::{CoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_provider_ports::{ProviderPort, ProviderResult};

/// Reuse the effect-path taxonomy so a native caller sees one category per
/// condition at both the admission boundary and the effect path. Mapping every
/// host error to `invalid_input` reported policy denials and busy providers as
/// malformed input.
fn map_host_error(err: nexus_agent_host::error::HostError) -> CoreError {
    nexus_agent_host::providers::port::host_error_to_core_error(&err)
}

fn provider_id_from_payload(
    payload: &serde_json::Map<String, serde_json::Value>,
) -> ProviderResult<String> {
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
    state: Arc<super::env_state::EnvState>,
}

impl AdmittingProviderPort {
    pub fn new(
        host: Arc<HostManager>,
        inner: Arc<dyn ProviderPort>,
        state: Arc<super::env_state::EnvState>,
    ) -> Self {
        Self { host, inner, state }
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
        let method = request.method;
        let session_id = request.session_id.clone();
        let reply = self.inner.call(request).await?;
        // The JS adapter owns its ACP children; native close must be able to
        // cancel and release them, so track live JS sessions at the one place
        // that observes every launch/execute/shutdown reply.
        match method {
            ProviderCallMethod::Launch if reply.ok => {
                if let Some(session_id) = reply.session_id.clone().or(session_id) {
                    self.state.record_js_session(session_id);
                }
            }
            ProviderCallMethod::Execute if reply.ok => {
                if let (Some(session_id), Some(operation_id)) =
                    (session_id.as_deref(), reply.operation_id.clone())
                {
                    self.state
                        .record_js_session_operation(session_id, operation_id);
                }
            }
            ProviderCallMethod::Cancel if reply.ok => {
                if let Some(session_id) = session_id.as_deref() {
                    self.state.clear_js_session_operation(session_id);
                }
            }
            ProviderCallMethod::Shutdown if reply.ok => {
                if let Some(session_id) = session_id.as_deref() {
                    self.state.forget_js_session(session_id);
                }
            }
            _ => {}
        }
        Ok(reply)
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
