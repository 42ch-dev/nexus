//! `workspace.open` and `workspace.commit` capabilities (v1.188 P3).

use crate::capability::{Capability, CapabilityError, WorkspaceExecutor};
use async_trait::async_trait;
use nexus_contracts::local::orchestration::{WorkspaceCommitInput, WorkspaceOpenInput};
use serde_json::Value;
use std::sync::Arc;

pub struct WorkspaceOpen {
    executor: Option<Arc<dyn WorkspaceExecutor>>,
}

impl WorkspaceOpen {
    #[must_use]
    pub fn new() -> Self {
        Self { executor: None }
    }

    #[must_use]
    pub fn with_workspace_executor(executor: Arc<dyn WorkspaceExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }
}

#[async_trait]
impl Capability for WorkspaceOpen {
    fn name(&self) -> &'static str {
        "workspace.open"
    }

    fn input_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096,"pattern":"^(?!/)(?!.*\.\.)[^/]+(?:/[^/]+)*$"}},"required":["path"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"sessionId":{"type":"string"},"snapshot":{"type":"object","properties":{"workspaceRoot":{"type":"string"},"path":{"type":"string"},"existed":{"type":"boolean"},"fileHashes":{"type":"object","additionalProperties":{"type":"string"}}},"required":["workspaceRoot","path","existed","fileHashes"],"additionalProperties":false}},"required":["sessionId","snapshot"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: WorkspaceOpenInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("workspace.open input: {e}")))?;
        let executor = self
            .executor
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;
        let output = executor.open(parsed).await?;
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

pub struct WorkspaceCommit {
    executor: Option<Arc<dyn WorkspaceExecutor>>,
}

impl WorkspaceCommit {
    #[must_use]
    pub fn new() -> Self {
        Self { executor: None }
    }

    #[must_use]
    pub fn with_workspace_executor(executor: Arc<dyn WorkspaceExecutor>) -> Self {
        Self {
            executor: Some(executor),
        }
    }
}

#[async_trait]
impl Capability for WorkspaceCommit {
    fn name(&self) -> &'static str {
        "workspace.commit"
    }

    fn input_schema(&self) -> &'static str {
        // v1.188 P3 T3: op-conditional required fields, sha256 pattern, and
        // the manifest-wide bounds expressible in JSON Schema (item count and
        // per-field lengths). Aggregate byte totals (MAX_FILE_BYTES per file,
        // MAX_TOTAL_BYTES per manifest) are enforced by the executor, which is
        // the only place they can be summed.
        r#"{"type":"object","properties":{"sessionId":{"type":"string","minLength":1},"changes":{"type":"array","minItems":1,"maxItems":128,"items":{"type":"object","properties":{"path":{"type":"string","minLength":1,"maxLength":4096,"pattern":"^(?!/)(?!.*\.\.)[^/]+(?:/[^/]+)*$"},"op":{"type":"string","enum":["create","modify","delete"]},"expectedHash":{"type":"string","pattern":"^[0-9a-f]{64}$"},"contentBase64":{"type":"string","minLength":1,"maxLength":1398104}},"required":["path","op"],"additionalProperties":false,"allOf":[{"if":{"properties":{"op":{"const":"create"}},"required":["op"]},"then":{"required":["contentBase64"]},"else":{"required":["expectedHash"]}},{"if":{"properties":{"op":{"const":"delete"}},"required":["op"]},"then":{"not":{"required":["contentBase64"]}},"else":{"required":["contentBase64"]}}]}},"required":["sessionId","changes"],"additionalProperties":false}"#
    }

    fn output_schema(&self) -> &'static str {
        r#"{"type":"object","properties":{"revision":{"type":"string"},"committed":{"type":"boolean"}},"required":["revision","committed"],"additionalProperties":false}"#
    }

    async fn run(&self, input: Value) -> Result<Value, CapabilityError> {
        let parsed: WorkspaceCommitInput = serde_json::from_value(input)
            .map_err(|e| CapabilityError::InputInvalid(format!("workspace.commit input: {e}")))?;
        let executor = self
            .executor
            .as_ref()
            .ok_or(CapabilityError::WorkerUnavailable)?;
        let output = executor.commit(parsed).await?;
        serde_json::to_value(output)
            .map_err(|e| CapabilityError::Internal(format!("serialize output: {e}")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::WorkspaceExecutor;
    use async_trait::async_trait;
    use nexus_contracts::local::orchestration::{
        WorkspaceCommitOutput, WorkspaceOpenInput, WorkspaceOpenOutput,
    };

    struct StubExecutor;

    #[async_trait]
    impl WorkspaceExecutor for StubExecutor {
        async fn open(
            &self,
            input: WorkspaceOpenInput,
        ) -> Result<WorkspaceOpenOutput, CapabilityError> {
            Ok(WorkspaceOpenOutput {
                session_id: "ws_test".into(),
                snapshot: nexus_contracts::local::orchestration::WorkspaceOpenSnapshot {
                    workspace_root: "/tmp".into(),
                    path: input.path,
                    existed: false,
                    file_hashes: Default::default(),
                },
            })
        }

        async fn commit(
            &self,
            _input: WorkspaceCommitInput,
        ) -> Result<WorkspaceCommitOutput, CapabilityError> {
            Ok(WorkspaceCommitOutput {
                revision: "rev_test".into(),
                committed: true,
            })
        }
    }

    #[tokio::test]
    async fn workspace_open_without_executor_unavailable() {
        let cap = WorkspaceOpen::new();
        let err = cap.run(serde_json::json!({"path": "a"})).await.unwrap_err();
        assert!(matches!(err, CapabilityError::WorkerUnavailable));
    }

    #[tokio::test]
    async fn workspace_open_with_executor() {
        let cap = WorkspaceOpen::with_workspace_executor(Arc::new(StubExecutor));
        let out = cap
            .run(serde_json::json!({"path": "Works/book"}))
            .await
            .unwrap();
        assert_eq!(out["sessionId"], "ws_test");
    }
}
