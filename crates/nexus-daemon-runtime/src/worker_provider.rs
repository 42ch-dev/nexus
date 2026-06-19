//! Production `WorkerHandleProvider` — bridges the capability-layer LLM call
//! seam to the daemon's shared `WorkerRegistry`.
//!
//! ## V1.51 T-A P0 (QC3 F-001)
//!
//! Prior to V1.51 T-A P0, the daemon boot constructed the capability registry
//! via `CapabilityRegistry::with_builtins()`, which builds every LLM-backed
//! builtin (including `nexus.llm.extract`) in standalone / test mode with
//! `workers: None`. In that mode, `LlmExtract::run` returns
//! `CapabilityError::WorkerUnavailable` on every call, so the production
//! review-time KB extraction hook silently fell back to the V1.50 heuristic
//! for every `novel-review-master` schedule completion (see QC3 F-001).
//!
//! This module closes that gap by providing a production `WorkerHandleProvider`
//! backed by the same `WorkerRegistry<WorkerManagerSpawner>` that
//! `WorkerMgrSubsystem` exposes. When a worker process is registered for a
//! creator (by the engine/preset-session path), the provider dispatches the
//! `worker/acp_prompt` JSON-RPC through its `WorkerHandle`. When no worker is
//! registered for the creator, the provider returns
//! `CapabilityError::WorkerUnavailable`, which the review-time hook maps to the
//! heuristic fallback (the correct V1.50-compatible behavior for the no-worker
//! branch — see `llm-extract.md` §5.1).
//!
//! ## Lock scope
//!
//! The provider locks the registry for the duration of the IPC call. This
//! serializes LLM calls through the registry mutex, which is acceptable at
//! V1.51 single-creator local-only scale (the cron admission guard already
//! serializes same-creator schedules). See QC3 S-001/S-006 for the scale
//! analysis. `WorkerHandle` is not `Clone`, so the handle reference cannot
//! escape the lock scope; holding the lock for the `.await` is the minimal
//! correct option.

#![allow(clippy::significant_drop_tightening)]

use std::sync::Arc;

use async_trait::async_trait;
use nexus_orchestration::capability::{CapabilityError, WorkerHandleProvider};
use nexus_orchestration::worker::{WorkerManagerSpawner, WorkerRegistry};
use serde_json::Value;
use tokio::sync::Mutex;

/// Production bridge between the capability layer and the daemon's worker pool.
///
/// Wraps the shared `WorkerRegistry` (the same `Arc<Mutex<...>>` instance used
/// by `WorkerMgrSubsystem`). Construct this in `boot::run_daemon` and inject it
/// into `CapabilityRuntimeDeps::worker_provider` so the registry factory
/// (`CapabilityRegistry::with_runtime_deps`) wires it into `nexus.llm.extract`,
/// `judge.llm`, `context.summarize`, and `acp.prompt`.
///
/// # Clone semantics
///
/// `Arc<Mutex<WorkerRegistry<...>>>` is cheap to clone (refcount bump). The
/// provider and the subsystem share the same underlying registry, so a worker
/// spawned by either side is visible to the other.
#[derive(Clone)]
pub struct ProductionWorkerProvider {
    /// Shared per-creator worker index (same instance as `WorkerMgrSubsystem`).
    registry: Arc<Mutex<WorkerRegistry<WorkerManagerSpawner>>>,
}

impl ProductionWorkerProvider {
    /// Construct a new provider wrapping the shared worker registry.
    #[must_use]
    pub const fn new(registry: Arc<Mutex<WorkerRegistry<WorkerManagerSpawner>>>) -> Self {
        Self { registry }
    }
}

#[async_trait]
impl WorkerHandleProvider for ProductionWorkerProvider {
    async fn call_acp_prompt(
        &self,
        creator_id: &str,
        session_id: &str,
        prompt: String,
        tool_policy: &str,
    ) -> Result<Value, CapabilityError> {
        // Hold the registry lock for the duration of the IPC dispatch. The
        // `&WorkerHandle` borrows from the lock guard, and `WorkerHandle` is
        // not `Clone`, so we cannot release the lock before the `.await`.
        // This serializes LLM calls per-registry, which matches the V1.51
        // single-creator local-only scale (QC3 S-001).
        let registry = self.registry.lock().await;
        if let Some(handle) = registry.get(creator_id) {
            let params = serde_json::json!({
                "creator_id": creator_id,
                "session_id": session_id,
                "prompt": prompt,
                "tool_policy": tool_policy,
            });
            tracing::debug!(
                creator_id,
                session_id,
                pid = handle.pid(),
                "dispatching worker/acp_prompt via ProductionWorkerProvider"
            );
            handle
                .call_json_rpc("worker/acp_prompt", params)
                .await
                .map_err(|e| CapabilityError::TransientExternal(e.to_string()))
        } else {
            tracing::debug!(
                creator_id,
                "no worker registered for creator; returning WorkerUnavailable"
            );
            Err(CapabilityError::WorkerUnavailable)
        }
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]

    use super::*;
    use nexus_orchestration::worker::{WorkerManager, WorkerSpec};

    /// Fixture path for the LLM-extract echo worker (responds with a fixed
    /// valid extraction JSON response).
    const LLM_EXTRACT_FIXTURE: &str = "./tests/fixtures/llm-extract-echo-worker.sh";

    /// `ProductionWorkerProvider` returns `WorkerUnavailable` when no worker is
    /// registered for the creator. This is the no-worker branch — the
    /// review-time hook maps this to the heuristic fallback (correct behavior).
    #[tokio::test]
    async fn provider_returns_unavailable_when_no_worker_registered() {
        let manager = Arc::new(Mutex::new(WorkerManager::new()));
        let spawner = WorkerManagerSpawner::new(manager);
        let registry = WorkerRegistry::new(4, spawner);
        let provider = ProductionWorkerProvider::new(Arc::new(Mutex::new(registry)));

        let result = provider
            .call_acp_prompt("unknown_creator", "sess", "prompt".into(), "deny_all")
            .await;

        assert!(
            result.is_err(),
            "expected WorkerUnavailable when no worker registered"
        );
        match result.unwrap_err() {
            CapabilityError::WorkerUnavailable => {} // correct
            other => panic!("expected WorkerUnavailable, got: {other:?}"),
        }
    }

    /// `ProductionWorkerProvider` dispatches `worker/acp_prompt` via IPC when a
    /// worker IS registered for the creator. This proves the production wiring
    /// actually reaches a worker process (not just the "no worker" branch).
    #[tokio::test]
    async fn provider_dispatches_ipc_to_registered_worker() {
        let manager = Arc::new(Mutex::new(WorkerManager::new()));
        let spawner = WorkerManagerSpawner::new(manager);
        let mut registry = WorkerRegistry::new(4, spawner);

        // Spawn the echo fixture into the registry for a test creator.
        let spec = WorkerSpec::test_stub(LLM_EXTRACT_FIXTURE);
        registry
            .get_or_spawn("test_creator", &spec)
            .await
            .expect("spawn echo fixture");

        let provider = ProductionWorkerProvider::new(Arc::new(Mutex::new(registry)));

        let result = provider
            .call_acp_prompt(
                "test_creator",
                "sess_1",
                "extract entities".into(),
                "deny_all",
            )
            .await
            .expect("IPC dispatch should succeed");

        // The echo fixture returns a fixed extraction response. The provider
        // returns the raw JSON-RPC result; the capability parses `full_text`.
        let full_text = result
            .get("full_text")
            .and_then(|v| v.as_str())
            .expect("result should have full_text");
        assert!(
            full_text.contains("candidates"),
            "echo fixture should return a candidates JSON, got: {full_text}"
        );
    }
}
