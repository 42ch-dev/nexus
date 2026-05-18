//! `acp.session_load` capability — resume a named ACP session on the creator's worker.
//!
//! Design: `orchestration-engine.md` §5.2.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::{json, Value};

/// The `acp.session_load` capability.
///
/// Input schema: `{ session_id: string }`
/// Output schema: `{ ok: bool, error?: string }`
pub struct AcpSessionLoad;

#[async_trait]
impl Capability for AcpSessionLoad {
    fn name(&self) -> &'static str {
        "acp.session_load"
    }

    fn input_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["session_id"],
            "properties": {
                "session_id": { "type": "string", "description": "The ACP session ID to resume" }
            }
        }"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{
            "$schema": "https://json-schema.org/draft/2020-12/schema",
            "type": "object",
            "required": ["ok"],
            "properties": {
                "ok": { "type": "boolean" },
                "error": { "type": "string" }
            }
        }"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'session_id' field".into()))?;

        // Full implementation dispatches to Worker Manager.
        // For WS3 capability layer, return a stub success.
        Ok(json!({ "ok": true }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_session_load_name() {
        assert_eq!(AcpSessionLoad.name(), "acp.session_load");
    }

    #[tokio::test]
    async fn acp_session_load_valid_input() {
        let cap = AcpSessionLoad;
        let input = json!({ "session_id": "sess_123" });
        let result = cap.run(input).await.unwrap();
        assert_eq!(result["ok"], true);
    }

    #[tokio::test]
    async fn acp_session_load_missing_session_id_errors() {
        let cap = AcpSessionLoad;
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
    }
}
