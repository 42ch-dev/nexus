//! `outbox.flush` and `outbox.compact` capability stubs.
//!
//! `outbox.flush` requires the cloud line (`nexus-cloud-sync`).
//! In the default (local-only) build it returns a permanent error.
//! `outbox.compact` is a local-only operation (DB cleanup) and
//! remains a no-op stub until DB wiring is added.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::Value;

/// Cloud-sync-disabled error message (shared with sync.rs).
const CLOUD_LINE_DISABLED: &str =
    "cloud line disabled: outbox flush requires nexus-cloud-sync (enable legacy-sync feature)";

// ---------------------------------------------------------------------------
// outbox.flush
// ---------------------------------------------------------------------------

/// Flush pending outbox entries.
///
/// **Stub**: returns `PermanentExternal` error in local-only builds.
/// The real implementation lives in `nexus-cloud-sync`.
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

    async fn run(&self, _input: Value) -> Result<Value, CapabilityError> {
        Err(CapabilityError::PermanentExternal(
            CLOUD_LINE_DISABLED.to_string(),
        ))
    }
}

// ---------------------------------------------------------------------------
// outbox.compact
// ---------------------------------------------------------------------------

/// Compact outbox table by removing old completed entries.
///
/// **Stub**: returns zero removed until DB integration is wired.
/// This is a local-only operation and does not require cloud-sync.
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

    async fn run(&self, _input: Value) -> Result<Value, CapabilityError> {
        // Local-only stub: no DB wiring yet.
        let output = serde_json::json!({"removed": 0, "retained": 0});
        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn outbox_flush_returns_cloud_disabled_error() {
        let cap = OutboxFlush;
        let err = cap
            .run(serde_json::json!({"limit": 100}))
            .await
            .unwrap_err();
        match err {
            CapabilityError::PermanentExternal(msg) => {
                assert!(msg.contains("cloud line disabled"));
            }
            other => panic!("expected PermanentExternal, got {other:?}"),
        }
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
