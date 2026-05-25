//! `acp.session_load` capability — resume a named ACP session on the creator's worker.
//!
//! Design: `orchestration-engine.md` §5.2.
//!
//! DF-36 partial de-stub: The capability now validates all input fields and
//! returns structured data. Real worker IPC dispatch happens through
//! `WorkerHandle` in the task execution layer.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::{json, Value};

/// The `acp.session_load` capability.
///
/// Input schema: `{ session_id: string }`
/// Output schema: `{ ok: bool, error?: string }`
///
/// When called from a preset task, the worker handle dispatches
/// `worker/acp_session_load` via IPC. Standalone invocation returns
/// a validated placeholder.
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
        let session_id = input
            .get("session_id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| CapabilityError::InputInvalid("missing 'session_id' field".into()))?;

        // Validate session_id is non-empty.
        if session_id.is_empty() {
            return Err(CapabilityError::InputInvalid(
                "session_id must not be empty".into(),
            ));
        }

        // DF-36 partial de-stub: Validate and return structured success.
        // The actual worker IPC dispatch happens via WorkerHandle in the
        // task execution layer (AcpPromptTask handles session routing).
        Ok(json!({
            "ok": true,
            "session_id": session_id,
            "dispatched_via_ipc": false
        }))
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
        assert_eq!(result["session_id"], "sess_123");
    }

    #[tokio::test]
    async fn acp_session_load_missing_session_id_errors() {
        let cap = AcpSessionLoad;
        let input = json!({});
        let result = cap.run(input).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn acp_session_load_empty_session_id_errors() {
        let cap = AcpSessionLoad;
        let input = json!({ "session_id": "" });
        let result = cap.run(input).await;
        assert!(result.is_err());
    }
}
