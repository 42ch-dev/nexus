//! V1.51 T-A P0 (QC3 F-001) — production wiring hermetic integration test.
//!
//! Proves the daemon boot's capability-registry construction (which switched
//! from `CapabilityRegistry::with_builtins()` to
//! `CapabilityRegistry::with_runtime_deps(&deps)`) makes `nexus.llm.extract`
//! dispatch through a real `WorkerHandleProvider` in production-shaped boot,
//! returning `Ok(candidates)` instead of `WorkerUnavailable`.
//!
//! ## What this test covers
//!
//! 1. **Wiring shape**: `CapabilityRuntimeDeps` with a `worker_provider` →
//!    `CapabilityRegistry::with_runtime_deps` → `nexus.llm.extract` capability
//!    has a provider (not `None`). Mirrors the exact construction shape that
//!    `boot::run_daemon` uses after V1.51 T-A P0.
//!
//! 2. **End-to-end IPC dispatch through `ProductionWorkerProvider`**: a real
//!    echo-worker fixture is spawned into the shared `WorkerRegistry`, the
//!    `ProductionWorkerProvider` dispatches `worker/acp_prompt` via IPC, the
//!    capability parses the response, and returns `Ok(candidates)`.
//!
//! 3. **No-worker fallback still surfaces `WorkerUnavailable`**: when the
//!    registry has no worker for the creator, the provider returns
//!    `WorkerUnavailable`, which is the correct V1.50-compatible fallback
//!    signal (the review-time hook maps this to the heuristic path).
//!
//! Design: `llm-extract.md` §5.1; QC3 F-001.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use nexus_daemon_runtime::worker_provider::ProductionWorkerProvider;
use nexus_orchestration::capability::{
    CapabilityError, CapabilityRegistry, CapabilityRuntimeDeps, WorkerHandleProvider,
};
use nexus_orchestration::worker::{
    WorkerManager, WorkerManagerSpawner, WorkerRegistry, WorkerSpec,
};
use serde_json::json;

/// Fixture path for the LLM-extract echo worker.
const LLM_EXTRACT_FIXTURE: &str = "./tests/fixtures/llm-extract-echo-worker.sh";

// ─── Mock provider (for pure wiring-shape test) ─────────────────────────────

/// In-process mock provider returning a fixed extraction response. Mirrors the
/// `MockLlmExtractWorker` pattern in `nexus-orchestration`'s
/// `tests/novel_review_master.rs`. Used to prove the wiring shape without
/// spawning a subprocess.
struct MockLlmExtractWorker;

#[async_trait::async_trait]
impl WorkerHandleProvider for MockLlmExtractWorker {
    async fn call_acp_prompt(
        &self,
        _creator_id: &str,
        _session_id: &str,
        _prompt: String,
        _tool_policy: &str,
    ) -> Result<serde_json::Value, CapabilityError> {
        Ok(json!({
            "full_text": "{\"candidates\":[{\"canonical_name\":\"Mock Character\",\"block_type\":\"character\",\"summary\":null,\"confidence\":0.8,\"source_quote\":\"mock quote\"}]}"
        }))
    }
}

// ─── Test 1: with_runtime_deps wiring shape → nexus.llm.extract runs ────────

/// Pure wiring-shape test: the exact `CapabilityRuntimeDeps` shape used by
/// `boot::run_daemon` (after V1.51 T-A P0) produces a registry where
/// `nexus.llm.extract.run()` returns `Ok(candidates)` — NOT
/// `WorkerUnavailable`. This is the minimal reproduction of the QC3 F-001
/// acceptance criterion.
#[tokio::test]
async fn with_runtime_deps_wiring_makes_llm_extract_run() {
    let provider: Arc<dyn WorkerHandleProvider> = Arc::new(MockLlmExtractWorker);
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: Some(provider),
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let registry = CapabilityRegistry::with_runtime_deps(&deps);

    let cap = registry
        .get("nexus.llm.extract")
        .expect("nexus.llm.extract must be registered");

    let input = json!({
        "prompt": "extract entities",
        "chapter_prose": "Lin Xia drew her blade.",
        "_creator_id": "test_creator",
        "_session_id": "test_session",
    });
    let result = cap.run(input).await;

    let output = result.expect("run() should return Ok (not WorkerUnavailable)");
    let candidates = output
        .get("candidates")
        .and_then(|v| v.as_array())
        .expect("output should have a candidates array");
    assert!(
        !candidates.is_empty(),
        "candidates should be non-empty from mock provider"
    );
    assert_eq!(candidates[0]["canonical_name"], "Mock Character");
    assert_eq!(candidates[0]["block_type"], "character");
}

// ─── Test 2: ProductionWorkerProvider dispatches via real IPC ───────────────

/// End-to-end IPC dispatch through `ProductionWorkerProvider`: a real
/// echo-worker fixture is spawned into the shared `WorkerRegistry`, the
/// provider dispatches `worker/acp_prompt` via JSON-RPC, the capability parses
/// the response, and returns `Ok(candidates)`.
///
/// This proves the production wiring chain actually reaches a worker process:
/// `boot::run_daemon` → `ProductionWorkerProvider` → `WorkerRegistry` →
/// `WorkerHandle::call_json_rpc` → echo-worker fixture → parsed candidates.
#[tokio::test]
async fn production_provider_dispatches_ipc_to_real_worker() {
    let manager = Arc::new(tokio::sync::Mutex::new(WorkerManager::new()));
    let spawner = WorkerManagerSpawner::new(manager);
    let mut worker_registry = WorkerRegistry::new(4, spawner);

    // Spawn the echo fixture for a test creator.
    let spec = WorkerSpec::test_stub(LLM_EXTRACT_FIXTURE);
    worker_registry
        .get_or_spawn("test_creator", &spec)
        .await
        .expect("spawn echo fixture");

    let shared_registry = Arc::new(tokio::sync::Mutex::new(worker_registry));
    let provider = ProductionWorkerProvider::new(shared_registry);
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: Some(Arc::new(provider)),
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let registry = CapabilityRegistry::with_runtime_deps(&deps);

    let cap = registry
        .get("nexus.llm.extract")
        .expect("nexus.llm.extract must be registered");

    let input = json!({
        "prompt": "extract entities from chapter",
        "chapter_prose": "Lin Xia drew her blade and walked through the Azure Gate.",
        "_creator_id": "test_creator",
        "_session_id": "sess_ipc_1",
    });
    let result = cap.run(input).await;

    let output = result.expect("run() should return Ok via real IPC dispatch");
    let candidates = output
        .get("candidates")
        .and_then(|v| v.as_array())
        .expect("output should have a candidates array");
    assert!(
        !candidates.is_empty(),
        "candidates should be non-empty from echo fixture"
    );
    assert_eq!(candidates[0]["canonical_name"], "Lin Xia");
    assert_eq!(candidates[0]["block_type"], "character");
}

// ─── Test 3: no-worker branch returns WorkerUnavailable (fallback signal) ───

/// When the shared `WorkerRegistry` has no worker registered for the creator,
/// `ProductionWorkerProvider` returns `WorkerUnavailable`. This is the correct
/// V1.50-compatible no-worker signal: the review-time hook maps this to the
/// heuristic extraction path (`quality_loop::extract_via_llm` → `Fallback`).
///
/// This is the SAME error code that `with_builtins()` produced unconditionally
/// before V1.51 T-A P0 — but now it only surfaces when no worker is actually
/// registered, not on every production call.
#[tokio::test]
async fn production_provider_returns_unavailable_without_worker() {
    let manager = Arc::new(tokio::sync::Mutex::new(WorkerManager::new()));
    let spawner = WorkerManagerSpawner::new(manager);
    let worker_registry = WorkerRegistry::new(4, spawner);
    let shared_registry = Arc::new(tokio::sync::Mutex::new(worker_registry));

    let provider = ProductionWorkerProvider::new(shared_registry);
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: Some(Arc::new(provider)),
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let registry = CapabilityRegistry::with_runtime_deps(&deps);

    let cap = registry
        .get("nexus.llm.extract")
        .expect("nexus.llm.extract must be registered");

    let input = json!({
        "prompt": "extract",
        "chapter_prose": "...",
        "_creator_id": "creator_with_no_worker",
        "_session_id": "sess_none",
    });
    let result = cap.run(input).await;

    assert!(result.is_err(), "expected error when no worker registered");
    match result.unwrap_err() {
        CapabilityError::WorkerUnavailable => {} // correct
        other => panic!("expected WorkerUnavailable, got: {other:?}"),
    }
}

// ─── Test 4: capability is registered in the production-shaped registry ─────

/// Verifies that `with_runtime_deps` registers `nexus.llm.extract` (and the
/// sibling LLM caps) — proving the production registry shape has the full
/// builtin set. This is a static contract check on the wiring.
#[tokio::test]
async fn with_runtime_deps_registers_all_llm_capabilities() {
    let provider: Arc<dyn WorkerHandleProvider> = Arc::new(MockLlmExtractWorker);
    let deps = CapabilityRuntimeDeps {
        pool: None,
        worker_provider: Some(provider),
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let registry = CapabilityRegistry::with_runtime_deps(&deps);

    // 31 builtins: 21 V1.51 + essay.scaffold from V1.52 T-A P2 + game_bible.scaffold from V1.54 P1
    // + script.scaffold from V1.55 P3 + nexus.game_bible.section_status.update from V1.56 P-last
    // (R-V155P2-F002 closure) + nexus.reference.refresh from V1.58 P3 fix-wave
    // + 5 V1.60 P0 DF-46 local-parity orchestration capabilities (world.state.query,
    //   world.delta.propose, world.delta.apply, timeline.event.append, fork.create).
    // NOTE: when adding new builtins, update this count OR refactor to auto-derive
    // from the catalog (see catalog_registry_invariant_all_ids_present for the pattern).
    assert_eq!(
        registry.len(),
        31,
        "registry should have 31 builtins (21 V1.51 + essay.scaffold V1.52 + game_bible.scaffold V1.54 P1 + script.scaffold V1.55 P3 + nexus.game_bible.section_status.update V1.56 P-last + nexus.reference.refresh V1.58 P3 + 5 V1.60 P0 DF-46 orchestration capabilities)"
    );

    // LLM-backed caps must all be present.
    for name in [
        "nexus.llm.extract",
        "judge.llm",
        "context.summarize",
        "acp.prompt",
    ] {
        assert!(
            registry.get(name).is_some(),
            "expected builtin '{name}' to be registered"
        );
    }
}
