//! `workspace.open` and `workspace.commit` capabilities.
//!
//! Owner crate: `nexus-home-layout` (placeholder stubs).

use async_trait::async_trait;
use nexus_contracts::local::orchestration::{WorkspaceCommitInput, WorkspaceCommitOutput};
use nexus_contracts::local::orchestration::{WorkspaceOpenInput, WorkspaceOpenOutput};
use serde_json::Value;
use crate::capability::{Capability, CapabilityError};

// ---------------------------------------------------------------------------
// workspace.open
// ---------------------------------------------------------------------------

/// Ensure workspace directory is present and valid.
///
/// **Stub**: returns a synthetic path until `nexus-home-layout` is wired.
pub struct WorkspaceOpen;

#[async_trait]
impl Capability for WorkspaceOpen {
    fn name(&self) -> &'static str {
        "workspace.open"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"path":{"type":"string"}},"required":[],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"workspacePath":{"type":"string"},"created":{"type":"boolean"}},"required":["workspacePath","created"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: WorkspaceOpenInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("workspace.open input: {e}"))
        })?;
        let output = WorkspaceOpenOutput {
            workspace_path: "/tmp/nexus-workspace".to_string(),
            created: false,
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

// ---------------------------------------------------------------------------
// workspace.commit
// ---------------------------------------------------------------------------

/// Commit manuscript diff into working copy.
///
/// **Stub**: returns a synthetic revision until `nexus-home-layout` is wired.
pub struct WorkspaceCommit;

#[async_trait]
impl Capability for WorkspaceCommit {
    fn name(&self) -> &'static str {
        "workspace.commit"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"message":{"type":"string"}},"required":["message"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"revision":{"type":"string"}},"required":["revision"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let _input: WorkspaceCommitInput = serde_json::from_value(input).map_err(|e| {
            CapabilityError::InputInvalid(format!("workspace.commit input: {e}"))
        })?;
        let output = WorkspaceCommitOutput {
            revision: "stub-revision".to_string(),
        };
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn workspace_open_smoke() {
        let cap = WorkspaceOpen;
        let out = cap.run(serde_json::json!({})).await.unwrap();
        assert!(!out["created"].as_bool().unwrap());
    }

    #[tokio::test]
    async fn workspace_commit_smoke() {
        let cap = WorkspaceCommit;
        let out = cap
            .run(serde_json::json!({"message": "test commit"}))
            .await
            .unwrap();
        assert_eq!(out["revision"], "stub-revision");
    }

    #[tokio::test]
    async fn workspace_commit_requires_message() {
        let cap = WorkspaceCommit;
        let result = cap.run(serde_json::json!({})).await;
        assert!(result.is_err());
    }
}
