//! Creator capabilities: `read_memory`, `write_memory`, `inject_prompt`.
//!
//! Owner crate: `nexus-domain`.
//!
//! **Stubs**: return synthetic output until domain integration is wired.

use async_trait::async_trait;
use nexus_contracts::local::orchestration::{
    CreatorInjectPromptInput, CreatorInjectPromptOutput, CreatorReadMemoryInput,
    CreatorReadMemoryOutput, CreatorWriteMemoryInput, CreatorWriteMemoryOutput,
};
use serde_json::Value;
use crate::capability::{Capability, CapabilityError};

// ---------------------------------------------------------------------------
// creator.read_memory
// ---------------------------------------------------------------------------

/// Read entries from the creator memory store.
pub struct CreatorReadMemory;

#[async_trait]
impl Capability for CreatorReadMemory {
    fn name(&self) -> &'static str {
        "creator.read_memory"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"keyword":{"type":"string"},"limit":{"type":"integer","minimum":1,"default":50}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"count":{"type":"integer","minimum":0}},"required":["count"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: CreatorReadMemoryInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("creator.read_memory input: {e}"))
        })?;
        let output = CreatorReadMemoryOutput { count: 0 };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// creator.write_memory
// ---------------------------------------------------------------------------

/// Append/update creator memory.
pub struct CreatorWriteMemory;

#[async_trait]
impl Capability for CreatorWriteMemory {
    fn name(&self) -> &'static str {
        "creator.write_memory"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"content":{"type":"string"},"keywords":{"type":"array","items":{"type":"string"}},"required":["content","keywords"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"fragmentId":{"type":"string"}},"required":["fragmentId"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: CreatorWriteMemoryInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("creator.write_memory input: {e}"))
        })?;
        let output = CreatorWriteMemoryOutput {
            fragment_id: "stub-fragment-id".to_string(),
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// creator.inject_prompt
// ---------------------------------------------------------------------------

/// Queue a prompt to be sent on the next `acp.prompt`.
pub struct CreatorInjectPrompt;

#[async_trait]
impl Capability for CreatorInjectPrompt {
    fn name(&self) -> &'static str {
        "creator.inject_prompt"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"prompt":{"type":"string"},"priority":{"type":"integer","default":0}},"required":["prompt"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"queued":{"type":"boolean"}},"required":["queued"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: CreatorInjectPromptInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("creator.inject_prompt input: {e}"))
        })?;
        let output = CreatorInjectPromptOutput { queued: true };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn read_memory_smoke() {
        let cap = CreatorReadMemory;
        let out = cap.run(serde_json::json!({"keyword": "test"})).await.unwrap();
        assert_eq!(out["count"], 0);
    }

    #[tokio::test]
    async fn write_memory_smoke() {
        let cap = CreatorWriteMemory;
        let out = cap
            .run(serde_json::json!({"content": "hello", "keywords": ["greeting"]}))
            .await
            .unwrap();
        assert_eq!(out["fragmentId"], "stub-fragment-id");
    }

    #[tokio::test]
    async fn inject_prompt_smoke() {
        let cap = CreatorInjectPrompt;
        let out = cap
            .run(serde_json::json!({"prompt": "write chapter 1"}))
            .await
            .unwrap();
        assert_eq!(out["queued"], true);
    }
}
