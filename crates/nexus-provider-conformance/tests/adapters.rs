//! Per-adapter conformance tests: scripted fixtures → real adapters → runner.
//!
//! Each test drives a REAL adapter (`ClaudeCliProvider` / `CodexNativeProvider`
//! / `DshNativeProvider`) pointed at an in-crate scripted fixture CLI by
//! absolute path, feeds the resulting normalized `HostEvent` stream to the
//! neutral runner, and asserts the expected `ConformanceReport`.
//!
//! # Pinned conformance records (PM decision, T2 brief)
//!
//! - `map_claude.rs` / `map_dsh.rs` execute streams emit NO `OpStarted` today
//!   (real adapter behavior, not a fixture gap); `map_codex.rs` does. The
//!   claude/dsh happy-path tests therefore assert the missing-OpStarted
//!   finding is the ONLY finding — the adapter gap is a pinned conformance
//!   record, not a runner weakness.
//! - dsh `cancel()` is an honest no-op (AR-6: the SDK has no cancel RPC); the
//!   dsh cancel probe asserts the no-op contract (the turn completes).
//!
//! The fixtures are spawned via `#!/usr/bin/env python3` with absolute paths
//! passed through the provider constructors — no PATH mutation, so the
//! `nexus-agent-host` `PROCESS_ENV_LOCK` (pub(crate), cross-crate
//! inaccessible) is not needed here.

// Test-only: adapters' execute/cancel return nested Results over async
// streams; unwrapping in assertions keeps failure modes readable, and the
// fixture harness (not product code) owns the panics. File-scoped to the
// integration tests only.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::unwrap_in_result)]

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use futures_util::StreamExt;
use nexus_agent_host::capability::model::{
    HostContentBlock, HostEvent, HostEventStream, HostOperation, LaunchSpec,
};
use nexus_agent_host::config::TimeoutConfig;
use nexus_agent_host::providers::native_cli::claude::ClaudeCliProvider;
use nexus_agent_host::providers::native_cli::codex::CodexNativeProvider;
use nexus_agent_host::providers::native_cli::dsh::DshNativeProvider;
use nexus_agent_host::ProviderAdapter;
use nexus_agent_host::{HostOperationId, ProviderId};
use nexus_provider_conformance::{
    run_conformance, ConformanceConfig, ConformanceReport, InvariantId,
};

const CLAUDE_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/claude-codes/mock_claude_cli.py"
);
const CODEX_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/codex-codes/mock_codex_app_server.py"
);
const DSH_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/fixtures/dsh-native/mock_dsh_agent.py"
);

fn launch_spec() -> LaunchSpec {
    LaunchSpec {
        cwd: PathBuf::from("/tmp"),
        model: None,
        mode: None,
        mcp_servers: vec![],
    }
}

fn prompt_op() -> HostOperation {
    HostOperation::Prompt {
        op_id: HostOperationId::new(),
        content: vec![HostContentBlock::Text {
            text: "hello".to_string(),
        }],
    }
}

fn stream_of(events: Vec<HostEvent>) -> HostEventStream {
    futures_util::stream::iter(events.into_iter().map(Ok)).boxed()
}

async fn collect(stream: HostEventStream) -> Vec<HostEvent> {
    let results: Vec<_> = stream.collect().await;
    results
        .into_iter()
        .map(|r| r.expect("stream item should be Ok"))
        .collect()
}

fn invariant_ids(report: &ConformanceReport) -> Vec<InvariantId> {
    report.findings.iter().map(|f| f.invariant).collect()
}

/// Assert the report's ONLY finding is the missing-OpStarted adapter gap
/// (claude/dsh pinned conformance record).
fn assert_only_missing_started(report: &ConformanceReport) {
    assert!(
        !report.passed(),
        "claude/dsh adapters emit no OpStarted today (pinned record): {report}"
    );
    assert_eq!(
        invariant_ids(report),
        vec![InvariantId::ExactlyOneStarted],
        "expected ONLY the missing-OpStarted finding: {report}"
    );
}

fn assert_passes(report: &ConformanceReport) {
    assert!(report.passed(), "expected a clean report: {report}");
}

fn terminal_of(events: &[HostEvent]) -> Option<&HostEvent> {
    events
        .iter()
        .rev()
        .find(|e| matches!(e, HostEvent::OpFinished(_) | HostEvent::OpFailed(_)))
}

/// Unique per-test request-log path (fixture `REQ_LOG` knob).
fn req_log(label: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "nexus-provider-conformance-{}-{label}.log",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    path
}

/// Poll `path` until `needle` appears in its content or the deadline passes.
async fn wait_for_log(path: &Path, needle: &str, timeout: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + timeout;
    loop {
        if let Ok(content) = std::fs::read_to_string(path) {
            if content.contains(needle) {
                return true;
            }
        }
        if tokio::time::Instant::now() >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

// ── Claude (stream-json) ────────────────────────────────────────────────

#[tokio::test]
async fn claude_happy_path_conforms_except_missing_started() {
    let provider = ClaudeCliProvider::new(
        ProviderId::new("conformance-claude"),
        "Conformance".to_string(),
        CLAUDE_FIXTURE.to_string(),
        HashMap::from([("SCENARIO".to_string(), "happy".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let report = run_conformance(stream, ConformanceConfig::default()).await;
    assert_only_missing_started(&report);
}

#[tokio::test]
async fn claude_mid_stream_tool_call_conforms_except_missing_started() {
    let provider = ClaudeCliProvider::new(
        ProviderId::new("conformance-claude"),
        "Conformance".to_string(),
        CLAUDE_FIXTURE.to_string(),
        HashMap::from([("SCENARIO".to_string(), "tool_call".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let events = collect(stream).await;
    // The tool_use block is AR-1-skipped by the mapper: no ToolCall event.
    assert!(
        !events.iter().any(|e| matches!(e, HostEvent::ToolCall(_))),
        "tool_use blocks carry no host event this iteration (AR-1): {events:?}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_only_missing_started(&report);
}

#[tokio::test]
async fn claude_malformed_frame_fails_once_with_decode_error() {
    let provider = ClaudeCliProvider::new(
        ProviderId::new("conformance-claude"),
        "Conformance".to_string(),
        CLAUDE_FIXTURE.to_string(),
        HashMap::from([("SCENARIO".to_string(), "malformed".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let events = collect(stream).await;
    assert!(
        matches!(
            terminal_of(&events),
            Some(HostEvent::OpFailed(f)) if f.error_category == "decode_error"
        ),
        "unknown top-level type must fail the turn once with decode_error: {events:?}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_only_missing_started(&report);
}

#[tokio::test]
async fn claude_cancel_terminates_fixture() {
    let log = req_log("claude-cancel");
    let provider = ClaudeCliProvider::new(
        ProviderId::new("conformance-claude"),
        "Conformance".to_string(),
        CLAUDE_FIXTURE.to_string(),
        HashMap::from([
            ("SCENARIO".to_string(), "cancel".to_string()),
            ("REQ_LOG".to_string(), log.to_string_lossy().into_owned()),
        ]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let mut stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");

    // Pump the first frame so the turn is visibly streaming.
    let first = stream.next().await.expect("first event").expect("ok");
    assert!(matches!(first, HostEvent::MessageDelta(_)));
    assert!(
        wait_for_log(&log, "blocked", Duration::from_secs(5)).await,
        "fixture must be alive and holding the turn (REQ_LOG blocked marker)"
    );

    provider
        .cancel(&handle, HostOperationId::new())
        .await
        .expect("cancel");

    let mut events = vec![first];
    events.extend(collect(stream).await);
    assert!(
        matches!(
            terminal_of(&events),
            Some(HostEvent::OpFailed(f)) if f.error_category == "stream_closed"
        ),
        "cancel must terminate the fixture: the stream ends with one \
         OpFailed(stream_closed) (transport observes cancellation): {events:?}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_only_missing_started(&report);
}

// ── Codex (app-server JSON-RPC) ────────────────────────────────────────

#[tokio::test]
async fn codex_happy_path_conforms() {
    let provider = CodexNativeProvider::new(
        ProviderId::new("conformance-codex"),
        "Conformance".to_string(),
        CODEX_FIXTURE.to_string(),
        HashMap::from([("SCENARIO".to_string(), "happy".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let report = run_conformance(stream, ConformanceConfig::default()).await;
    assert_passes(&report);
}

#[tokio::test]
async fn codex_mid_stream_tool_call_conforms() {
    let provider = CodexNativeProvider::new(
        ProviderId::new("conformance-codex"),
        "Conformance".to_string(),
        CODEX_FIXTURE.to_string(),
        HashMap::from([("SCENARIO".to_string(), "tool_call".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let events = collect(stream).await;
    // item/toolUse is an unknown method in codex-codes 0.146.4: parsed as
    // Notification::Unknown and mapper-skipped (AR-1) — no ToolCall event.
    assert!(
        !events.iter().any(|e| matches!(e, HostEvent::ToolCall(_))),
        "tool-use notifications carry no host event this iteration (AR-1): {events:?}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_passes(&report);
}

#[tokio::test]
async fn codex_malformed_frame_fails_once_with_decode_error() {
    let provider = CodexNativeProvider::new(
        ProviderId::new("conformance-codex"),
        "Conformance".to_string(),
        CODEX_FIXTURE.to_string(),
        HashMap::from([("SCENARIO".to_string(), "malformed".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let events = collect(stream).await;
    assert!(
        matches!(
            terminal_of(&events),
            Some(HostEvent::OpFailed(f)) if f.error_category == "decode_error"
        ),
        "a bad delta must fail the turn once with decode_error: {events:?}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_passes(&report);
}

#[tokio::test]
async fn codex_cancel_interrupts_turn() {
    let log = req_log("codex-cancel");
    let provider = CodexNativeProvider::new(
        ProviderId::new("conformance-codex"),
        "Conformance".to_string(),
        CODEX_FIXTURE.to_string(),
        HashMap::from([
            ("SCENARIO".to_string(), "cancel".to_string()),
            ("REQ_LOG".to_string(), log.to_string_lossy().into_owned()),
        ]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let mut stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");

    // Pump the first frame so the turn is visibly streaming.
    let first = stream.next().await.expect("first event").expect("ok");
    assert!(matches!(first, HostEvent::OpStarted(_)));

    provider
        .cancel(&handle, HostOperationId::new())
        .await
        .expect("cancel");

    let mut events = vec![first];
    events.extend(collect(stream).await);
    assert!(
        matches!(terminal_of(&events), Some(HostEvent::OpFinished(_))),
        "cancel must interrupt the turn: the stream ends with one clean \
         OpFinished (interrupted -> EndTurn): {events:?}"
    );
    assert!(
        wait_for_log(&log, "turn/interrupt", Duration::from_secs(5)).await,
        "the fixture must have received the turn/interrupt (REQ_LOG receipt)"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_passes(&report);
}

/// Mutation probe: one flipped fixture frame must turn the runner red.
///
/// The `mutate` scenario flips the `turn/started` notification's turn id to a
/// stale value; the provider's B-1 stale filter skips it, so the normalized
/// stream has no `OpStarted` and the runner reports `ExactlyOneStarted`.
#[tokio::test]
async fn codex_mutated_frame_turns_runner_red() {
    let provider = CodexNativeProvider::new(
        ProviderId::new("conformance-codex"),
        "Conformance".to_string(),
        CODEX_FIXTURE.to_string(),
        HashMap::from([("SCENARIO".to_string(), "mutate".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let events = collect(stream).await;
    assert!(
        !events.iter().any(|e| matches!(e, HostEvent::OpStarted(_))),
        "the stale turn/started must be skipped (B-1): {events:?}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert!(
        !report.passed(),
        "a mutated fixture frame must turn the runner red: {report}"
    );
    assert!(
        invariant_ids(&report).contains(&InvariantId::ExactlyOneStarted),
        "findings: {report}"
    );
}

// ── Dsh (deepseek-harness-sdk runtime) ──────────────────────────────────

#[tokio::test]
async fn dsh_happy_path_conforms_except_missing_started() {
    let provider = DshNativeProvider::new(
        ProviderId::new("conformance-dsh"),
        "Conformance".to_string(),
        Some(DSH_FIXTURE.to_string()),
        HashMap::from([("SCENARIO".to_string(), "happy".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let report = run_conformance(stream, ConformanceConfig::default()).await;
    assert_only_missing_started(&report);
}

#[tokio::test]
async fn dsh_mid_stream_tool_call_conforms_except_missing_started() {
    let provider = DshNativeProvider::new(
        ProviderId::new("conformance-dsh"),
        "Conformance".to_string(),
        Some(DSH_FIXTURE.to_string()),
        HashMap::from([("SCENARIO".to_string(), "tool_call".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let events = collect(stream).await;
    // The tool/call event is collected as raw noise by the SDK; the
    // normalized surface never surfaces tool calls (AR-6).
    assert!(
        !events.iter().any(|e| matches!(e, HostEvent::ToolCall(_))),
        "dsh has no tool surface (AR-6): {events:?}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_only_missing_started(&report);
}

#[tokio::test]
async fn dsh_malformed_frame_fails_once_with_decode_error() {
    let provider = DshNativeProvider::new(
        ProviderId::new("conformance-dsh"),
        "Conformance".to_string(),
        Some(DSH_FIXTURE.to_string()),
        HashMap::from([("SCENARIO".to_string(), "malformed".to_string())]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");
    let events = collect(stream).await;
    assert!(
        matches!(
            terminal_of(&events),
            Some(HostEvent::OpFailed(f)) if f.error_category == "decode_error"
        ),
        "a malformed turn/end must fail the run once with decode_error: {events:?}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_only_missing_started(&report);
}

#[tokio::test]
async fn dsh_cancel_is_honest_noop() {
    let log = req_log("dsh-cancel");
    let provider = DshNativeProvider::new(
        ProviderId::new("conformance-dsh"),
        "Conformance".to_string(),
        Some(DSH_FIXTURE.to_string()),
        HashMap::from([
            ("SCENARIO".to_string(), "cancel".to_string()),
            ("REQ_LOG".to_string(), log.to_string_lossy().into_owned()),
        ]),
        TimeoutConfig::default(),
    );
    let handle = provider.launch(launch_spec()).await.expect("launch");
    let mut stream = provider
        .execute(&handle, prompt_op())
        .await
        .expect("execute");

    // The dsh surface is non-streaming (AR-6): the first event is the
    // MessageDelta of an already-completed run.
    let first = stream.next().await.expect("first event").expect("ok");
    assert!(matches!(first, HostEvent::MessageDelta(_)));

    provider
        .cancel(&handle, HostOperationId::new())
        .await
        .expect("cancel must be an honest Ok no-op (AR-6)");

    let mut events = vec![first];
    events.extend(collect(stream).await);
    assert!(
        matches!(terminal_of(&events), Some(HostEvent::OpFinished(_))),
        "the turn must complete normally — dsh cancel is a documented no-op \
         (AR-6), the fixture is NOT terminated: {events:?}"
    );
    // The fixture never saw a cancellation: its request log shows the normal
    // initialize + session/prompt sequence (no cancel RPC exists).
    let log_content = std::fs::read_to_string(&log).unwrap_or_default();
    assert!(
        log_content.contains("session/prompt"),
        "the fixture must have served the prompt: {log_content}"
    );
    let report = run_conformance(stream_of(events), ConformanceConfig::default()).await;
    assert_only_missing_started(&report);
}
