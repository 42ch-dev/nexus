//! `sync.pull` and `sync.push` capability stubs.
//!
//! These capabilities require the cloud line (`nexus-cloud-sync`).
//! In the default (local-only) build they return a permanent error
//! indicating that cloud sync is disabled. The actual implementation
//! lives in `nexus-cloud-sync` and is wired when the CLI enables the
//! `legacy-sync` feature.

use crate::capability::{Capability, CapabilityError};
use async_trait::async_trait;
use serde_json::Value;

/// Cloud-sync-disabled error message shared by all three cloud stubs.
const CLOUD_LINE_DISABLED: &str =
    "cloud line disabled: sync requires nexus-cloud-sync (enable legacy-sync feature)";

// ---------------------------------------------------------------------------
// sync.pull
// ---------------------------------------------------------------------------

/// Pull remote deltas for a workspace.
///
/// **Stub**: returns `PermanentExternal` error in local-only builds.
/// The real implementation lives in `nexus-cloud-sync`.
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

    async fn run(&self, _input: Value) -> Result<Value, CapabilityError> {
        Err(CapabilityError::PermanentExternal(CLOUD_LINE_DISABLED.to_string()))
    }
}

// ---------------------------------------------------------------------------
// sync.push
// ---------------------------------------------------------------------------

/// Push local outbox to remote.
///
/// **Stub**: returns `PermanentExternal` error in local-only builds.
/// The real implementation lives in `nexus-cloud-sync`.
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

    async fn run(&self, _input: Value) -> Result<Value, CapabilityError> {
        Err(CapabilityError::PermanentExternal(CLOUD_LINE_DISABLED.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn sync_pull_returns_cloud_disabled_error() {
        let cap = SyncPull;
        let err = cap.run(serde_json::json!({"force": false})).await.unwrap_err();
        match err {
            CapabilityError::PermanentExternal(msg) => {
                assert!(msg.contains("cloud line disabled"));
            }
            other => panic!("expected PermanentExternal, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn sync_push_returns_cloud_disabled_error() {
        let cap = SyncPush;
        let err = cap.run(serde_json::json!({"force": false})).await.unwrap_err();
        match err {
            CapabilityError::PermanentExternal(msg) => {
                assert!(msg.contains("cloud line disabled"));
            }
            other => panic!("expected PermanentExternal, got {other:?}"),
        }
    }
}
