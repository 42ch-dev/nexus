//! V1.186 P1 T2 — production prompt-executor wiring hermetic integration test.
//!
//! Proves the daemon boot's capability-registry construction
//! (`CapabilityRegistry::with_runtime_deps(&deps)`) makes `nexus.llm.extract`
//! dispatch through a real `PromptExecutor` in production-shaped boot,
//! returning `Ok(candidates)` instead of `WorkerUnavailable`.
//!
//! ## What this test covers
//!
//! 1. **Wiring shape**: `CapabilityRuntimeDeps` with a `prompt_executor` →
//!    `CapabilityRegistry::with_runtime_deps` → `nexus.llm.extract` capability
//!    has an executor (not `None`). Mirrors the exact construction shape that
//!    `boot::run_daemon` uses after V1.186 P1 T2.
//!
//! 2. **End-to-end dispatch through the prompt executor**: a mock
//!    `PromptExecutor` returns a deterministic non-echo extraction response,
//!    the capability parses the response, and returns `Ok(candidates)`.
//!
//! 3. **No-executor fallback still surfaces `WorkerUnavailable`**: when no
//!    executor is injected, the capability returns `WorkerUnavailable`, which
//!    is the correct no-provider signal (the review-time hook maps this to
//!    the heuristic path).
//!
//! Design: `llm-extract.md` §5.1; A1.

#![allow(clippy::unwrap_used)]

use std::sync::Arc;

use nexus_orchestration::capability::{
    CapabilityError, CapabilityRegistry, CapabilityRuntimeDeps, PromptExecutor, PromptRequest,
    PromptResult,
};
use serde_json::json;

// ─── Mock executor (for pure wiring-shape test) ─────────────────────────────

/// In-process mock executor returning a fixed extraction response. Used to
/// prove the wiring shape without spawning a subprocess.
struct MockLlmExtractExecutor;

#[async_trait::async_trait]
impl PromptExecutor for MockLlmExtractExecutor {
    async fn execute(&self, _request: PromptRequest) -> Result<PromptResult, CapabilityError> {
        Ok(PromptResult {
            full_text: "{\"candidates\":[{\"canonical_name\":\"Mock Character\",\"block_type\":\"character\",\"summary\":null,\"confidence\":0.8,\"source_quote\":\"mock quote\"}]}".to_string(),
            host_session_id: "host-sess".to_string(),
            operation_id: "op-1".to_string(),
        })
    }
}

fn empty_session_cancels() -> std::sync::Arc<
    std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
> {
    std::sync::Arc::new(std::sync::RwLock::new(std::collections::HashMap::new()))
}

/// Shared cancellation map with a coordinator token registered for
/// `test_session` (the fail-closed contract: capability routes refuse with
/// `CancellationUnavailable` when a run has no registered token; production
/// registers at run admission).
fn session_cancels_with_test_session() -> std::sync::Arc<
    std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
> {
    let map = empty_session_cancels();
    map.write()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(
            "test_session".to_string(),
            tokio_util::sync::CancellationToken::new(),
        );
    map
}

// ─── Test 1: with_runtime_deps wiring shape → nexus.llm.extract runs ────────

/// Pure wiring-shape test: the exact `CapabilityRuntimeDeps` shape used by
/// `boot::run_daemon` (after V1.186 P1 T2) produces a registry where
/// `nexus.llm.extract.run()` returns `Ok(candidates)` — NOT
/// `WorkerUnavailable`. This is the minimal reproduction of the A1
/// acceptance criterion.
#[tokio::test]
async fn with_runtime_deps_wiring_makes_llm_extract_run() {
    let executor: Arc<dyn PromptExecutor> = Arc::new(MockLlmExtractExecutor);
    let deps = CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(executor),
        session_cancels: session_cancels_with_test_session(),
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
        "candidates should be non-empty from mock executor"
    );
    assert_eq!(candidates[0]["canonical_name"], "Mock Character");
    assert_eq!(candidates[0]["block_type"], "character");
}

// ─── Test 2: executor failure stays a typed failure ────────────────────────

/// A failing executor surfaces its typed error — never a partial-output
/// success (A1: only `MessageDelta` + `EndTurn` succeeds).
#[tokio::test]
async fn executor_failure_stays_typed_failure() {
    struct FailingExecutor;

    #[async_trait::async_trait]
    impl PromptExecutor for FailingExecutor {
        async fn execute(&self, _request: PromptRequest) -> Result<PromptResult, CapabilityError> {
            Err(CapabilityError::TransientExternal(
                "agent refused the request".to_string(),
            ))
        }
    }

    let deps = CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(Arc::new(FailingExecutor) as Arc<dyn PromptExecutor>),
        session_cancels: session_cancels_with_test_session(),
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
        "_creator_id": "test_creator",
        "_session_id": "test_session",
    });
    let result = cap.run(input).await;
    assert!(
        result.is_err(),
        "executor failure must stay a typed failure"
    );
    match result.unwrap_err() {
        CapabilityError::TransientExternal(msg) => {
            assert!(msg.contains("refused"), "typed refusal: {msg}");
        }
        other => panic!("expected TransientExternal, got: {other:?}"),
    }
}

// ─── Test 3: no-executor branch returns WorkerUnavailable (fallback signal) ─

/// When no `PromptExecutor` is injected, the capability returns
/// `WorkerUnavailable`. This is the correct no-provider signal: the
/// review-time hook maps this to the heuristic extraction path
/// (`quality_loop::extract_via_llm` → `Fallback`).
#[tokio::test]
async fn no_executor_returns_unavailable() {
    let deps = CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: None,
        session_cancels: empty_session_cancels(),
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
        "_creator_id": "creator_with_no_executor",
        "_session_id": "sess_none",
    });
    let result = cap.run(input).await;

    assert!(result.is_err(), "expected error when no executor injected");
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
    let executor: Arc<dyn PromptExecutor> = Arc::new(MockLlmExtractExecutor);
    let deps = CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(executor),
        session_cancels: empty_session_cancels(),
        daemon_tool_dispatch: None,
        cdn_config: None,
    };
    let registry = CapabilityRegistry::with_runtime_deps(&deps);

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
