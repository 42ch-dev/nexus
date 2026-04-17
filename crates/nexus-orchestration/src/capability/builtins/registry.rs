//! `registry.refresh` capability.
//!
//! Owner crate: `nexus-acp-host`.
//!
//! **Stub**: returns synthetic output. Real implementation needs network access
//! to the ACP registry CDN.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_contracts::local::orchestration::{RegistryRefreshInput, RegistryRefreshOutput};
use serde_json::Value;

/// Refresh the ACP registry cache.
pub struct RegistryRefresh;

#[async_trait]
impl Capability for RegistryRefresh {
    fn name(&self) -> &'static str {
        "registry.refresh"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"force":{"type":"boolean","default":false}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"cacheAgeMs":{"type":"integer","minimum":0},"agentCount":{"type":"integer","minimum":0}},"required":["cacheAgeMs","agentCount"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: RegistryRefreshInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("registry.refresh input: {e}")))?;
        // Stub: no actual network call.
        let output = RegistryRefreshOutput {
            cache_age_ms: 0,
            agent_count: 0,
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn registry_refresh_smoke() {
        let cap = RegistryRefresh;
        let out = cap.run(serde_json::json!({"force": false})).await.unwrap();
        assert!(out.get("cacheAgeMs").is_some());
        assert!(out.get("agentCount").is_some());
    }

    // Real network test — ignored by default.
    #[tokio::test]
    #[ignore = "needs network to fetch ACP registry"]
    async fn registry_refresh_network() {
        let cap = RegistryRefresh;
        let out = cap.run(serde_json::json!({"force": true})).await.unwrap();
        let cache_age = out["cacheAgeMs"].as_u64().unwrap();
        assert!(cache_age < 60_000); // should be very fresh after force refresh
    }
}
