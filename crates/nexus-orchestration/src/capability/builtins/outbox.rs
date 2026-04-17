//! `outbox.flush` and `outbox.compact` capabilities.
//!
//! Owner crate: `nexus-sync` (flush), `nexus-local-db` (compact).

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_contracts::local::orchestration::{OutboxCompactInput, OutboxCompactOutput};
use nexus_contracts::local::orchestration::{OutboxFlushInput, OutboxFlushOutput};
use serde_json::Value;

// ---------------------------------------------------------------------------
// outbox.flush
// ---------------------------------------------------------------------------

/// Flush pending outbox entries.
///
/// **Stub**: returns zero flushed until `nexus-sync` integration is wired.
pub struct OutboxFlush;

#[async_trait]
impl Capability for OutboxFlush {
    fn name(&self) -> &'static str {
        "outbox.flush"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"limit":{"type":"integer","minimum":0,"default":0}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"flushed":{"type":"integer","minimum":0}},"required":["flushed"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: OutboxFlushInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("outbox.flush input: {e}")))?;
        let output = OutboxFlushOutput { flushed: 0 };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// outbox.compact
// ---------------------------------------------------------------------------

/// Compact outbox table by removing old completed entries.
///
/// **Stub**: returns zero removed until DB integration is wired.
pub struct OutboxCompact;

#[async_trait]
impl Capability for OutboxCompact {
    fn name(&self) -> &'static str {
        "outbox.compact"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"retentionDays":{"type":"integer","minimum":1,"default":30}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"removed":{"type":"integer","minimum":0},"retained":{"type":"integer","minimum":0}},"required":["removed","retained"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: OutboxCompactInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("outbox.compact input: {e}")))?;
        let output = OutboxCompactOutput {
            removed: 0,
            retained: 0,
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn outbox_flush_smoke() {
        let cap = OutboxFlush;
        let out = cap.run(serde_json::json!({"limit": 100})).await.unwrap();
        assert_eq!(out["flushed"], 0);
    }

    #[tokio::test]
    async fn outbox_compact_smoke() {
        let cap = OutboxCompact;
        let out = cap
            .run(serde_json::json!({"retentionDays": 30}))
            .await
            .unwrap();
        assert_eq!(out["removed"], 0);
        assert_eq!(out["retained"], 0);
    }
}
