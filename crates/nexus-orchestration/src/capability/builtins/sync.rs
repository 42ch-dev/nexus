//! `sync.pull` and `sync.push` capabilities.
//!
//! Owner crate: `nexus-sync` (placeholder stubs until integration is wired).

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use nexus_contracts::local::orchestration::{SyncPullInput, SyncPullOutput};
use nexus_contracts::local::orchestration::{SyncPushInput, SyncPushOutput};
use serde_json::Value;

// ---------------------------------------------------------------------------
// sync.pull
// ---------------------------------------------------------------------------

/// Pull remote deltas for a workspace.
///
/// **Stub**: returns zero deltas until `nexus-sync` integration is wired.
/// Marked with `#[ignore]` for network-dependent testing.
pub struct SyncPull;

#[async_trait]
impl Capability for SyncPull {
    fn name(&self) -> &'static str {
        "sync.pull"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"force":{"type":"boolean","default":false}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"deltasPulled":{"type":"integer","minimum":0},"conflicts":{"type":"boolean"}},"required":["deltasPulled","conflicts"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: SyncPullInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("sync.pull input: {e}")))?;
        // Stub: no actual sync performed.
        let output = SyncPullOutput {
            deltas_pulled: 0,
            conflicts: false,
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// sync.push
// ---------------------------------------------------------------------------

/// Push local outbox to remote.
///
/// **Stub**: returns zero entries until `nexus-sync` integration is wired.
pub struct SyncPush;

#[async_trait]
impl Capability for SyncPush {
    fn name(&self) -> &'static str {
        "sync.push"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"force":{"type":"boolean","default":false}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"entriesPushed":{"type":"integer","minimum":0}},"required":["entriesPushed"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: SyncPushInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("sync.push input: {e}")))?;
        let output = SyncPushOutput { entries_pushed: 0 };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sync_pull_smoke() {
        let cap = SyncPull;
        let out = cap.run(serde_json::json!({"force": false})).await.unwrap();
        assert_eq!(out["deltasPulled"], 0);
        assert_eq!(out["conflicts"], false);
    }

    #[tokio::test]
    async fn sync_push_smoke() {
        let cap = SyncPush;
        let out = cap.run(serde_json::json!({"force": false})).await.unwrap();
        assert_eq!(out["entriesPushed"], 0);
    }

    #[tokio::test]
    async fn sync_pull_invalid_input() {
        let cap = SyncPull;
        let result = cap.run(serde_json::json!({"force": "not-a-bool"})).await;
        assert!(result.is_err());
    }
}
