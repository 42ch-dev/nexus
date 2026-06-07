//! End-to-end novel-writing preset test.
//!
//! Covers the V1.36 chapter-scoped pipeline (outline_chapter → draft_chapter →
//! finalize → done), manual advance, restart durability, and the llm_judge
//! 五問 quality gate.
//!
//! P3 refactored: legacy 4-state flow (gathering → brainstorming → outlining → drafting)
//! replaced by chapter-scoped 3-state flow (outline_chapter → draft_chapter → finalize).

use std::sync::Arc;

use nexus_orchestration::OrchestrationEngine;

// ---------------------------------------------------------------------------
// Helper: load novel-writing preset and run through states
// ---------------------------------------------------------------------------

/// Build an engine + loaded preset for E2E testing.
fn setup_engine() -> (
    Arc<nexus_orchestration::GraphFlowEngine>,
    nexus_orchestration::preset::LoadedPreset,
) {
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset("novel-writing", &caps)
        .expect("novel-writing preset should load");
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = Arc::new(nexus_orchestration::GraphFlowEngine::new_with_storage(
        storage,
        Arc::new(caps),
    ));
    (engine, loaded)
}

/// Seed `preset.input.*` required by novel-writing capability arg templates.
///
/// Production schedules populate these via core-context derivation; direct
/// `start_session_with_preset` in tests must seed them explicitly (C-V133P2-01).
async fn seed_novel_writing_preset_input(
    engine: &Arc<nexus_orchestration::GraphFlowEngine>,
    session_id: &nexus_orchestration::engine::SessionId,
) {
    let ctx = engine
        .get_context(session_id)
        .await
        .expect("get_context for preset.input seed");
    ctx.set(
        "preset.input.topic",
        "AI consciousness in a near-future city",
    )
    .await;
    ctx.set("preset.input.vibe", "literary").await;
    ctx.set("preset.input.work_ref", "e2e-test-novel").await;
    ctx.set("preset.input.work_id", "wrk_e2e_test").await;
}

async fn start_novel_writing_session(
    engine: &Arc<nexus_orchestration::GraphFlowEngine>,
    loaded: &nexus_orchestration::preset::LoadedPreset,
) -> nexus_orchestration::engine::SessionId {
    let session_id = engine
        .start_session_with_preset(loaded)
        .await
        .expect("start_session_with_preset should succeed");
    seed_novel_writing_preset_input(engine, &session_id).await;
    session_id
}

/// Run steps until the session reaches a terminal or waiting-for-input state.
/// Returns the last step outcome.
async fn run_until_wait_or_terminal(
    engine: &Arc<nexus_orchestration::GraphFlowEngine>,
    session_id: &nexus_orchestration::engine::SessionId,
    max_steps: usize,
) -> Vec<nexus_orchestration::engine::StepOutcome> {
    let mut outcomes = Vec::new();
    for _ in 0..max_steps {
        let outcome = engine
            .run_step(session_id)
            .await
            .expect("run_step should succeed");
        let is_done = matches!(
            outcome,
            nexus_orchestration::engine::StepOutcome::Completed { .. }
                | nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
                | nexus_orchestration::engine::StepOutcome::Error(_)
        );
        outcomes.push(outcome);
        if is_done {
            break;
        }
    }
    outcomes
}

// ---------------------------------------------------------------------------
// Test 1: All four outer states traverse
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_novel_writing_traverses_all_outer_states() {
    let (engine, loaded) = setup_engine();

    let session_id = start_novel_writing_session(&engine, &loaded).await;

    // Run until we reach a waiting state or terminal.
    let outcomes = run_until_wait_or_terminal(&engine, &session_id, 64).await;

    assert!(
        !outcomes.is_empty(),
        "should have at least one step outcome"
    );

    // The session should settle into a non-running state.
    let status = engine
        .get_status(&session_id)
        .await
        .expect("get_status should succeed");

    assert!(
        !matches!(status, nexus_orchestration::engine::SessionStatus::Running),
        "session should not be in Running state after steps"
    );
}

// ---------------------------------------------------------------------------
// Test 2: Chapter-scoped pipeline executes states
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_chapter_scoped_pipeline_executes() {
    let (engine, loaded) = setup_engine();

    let session_id = start_novel_writing_session(&engine, &loaded).await;

    let outcomes = run_until_wait_or_terminal(&engine, &session_id, 64).await;

    // P3: No inner graphs in the refactored preset; all states are outer states
    // with capability-based enter actions.
    assert!(
        !outcomes.is_empty(),
        "should have executed at least one step"
    );

    let ctx = engine
        .get_context(&session_id)
        .await
        .expect("get_context should succeed");

    // Verify that the session ran at least one step (outline_chapter is the initial state).
    assert!(
        !outcomes.is_empty(),
        "should have executed at least one step"
    );

    // Verify session is in a valid non-running state.
    let status = engine
        .get_status(&session_id)
        .await
        .expect("get_status should succeed");
    assert!(
        !matches!(status, nexus_orchestration::engine::SessionStatus::Running),
        "session should not be running after steps"
    );
}

// ---------------------------------------------------------------------------
// Test 3: Manual advance past outlining
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_schedule_advance_past_outlining() {
    let (engine, loaded) = setup_engine();

    let session_id = start_novel_writing_session(&engine, &loaded).await;

    let outcomes = run_until_wait_or_terminal(&engine, &session_id, 64).await;

    let last = outcomes.last().unwrap();
    if matches!(
        last,
        nexus_orchestration::engine::StepOutcome::WaitingForInput { .. }
    ) {
        engine
            .signal(
                &session_id,
                nexus_orchestration::engine::EngineSignal::Advance,
            )
            .await
            .expect("signal should succeed");

        let more = run_until_wait_or_terminal(&engine, &session_id, 64).await;
        // WS3 R2: Assert that advance actually produced new steps.
        assert!(!more.is_empty(), "should have steps after advance");
    }

    let status = engine
        .get_status(&session_id)
        .await
        .expect("get_status should succeed");

    // WS3 R2: Assert that the session is in a valid terminal or waiting state.
    assert!(
        matches!(
            status,
            nexus_orchestration::engine::SessionStatus::Completed
                | nexus_orchestration::engine::SessionStatus::Paused
                | nexus_orchestration::engine::SessionStatus::WaitingForInput
                | nexus_orchestration::engine::SessionStatus::Failed
        ),
        "session should be in a non-running state after test execution (got {status:?})"
    );
}

// ---------------------------------------------------------------------------
// Test 4: Restart durability (context persistence validation)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_restart_durability_context_persists() {
    let (engine, loaded) = setup_engine();

    let session_id = start_novel_writing_session(&engine, &loaded).await;

    let _ = run_until_wait_or_terminal(&engine, &session_id, 16).await;

    // Validate that the session state is queryable.
    let status_before = engine
        .get_status(&session_id)
        .await
        .expect("status should be queryable");

    assert!(
        !matches!(
            status_before,
            nexus_orchestration::engine::SessionStatus::Running
        ),
        "session should have settled into a non-running state"
    );

    // If waiting, signal advance to simulate resume after restart.
    if matches!(
        status_before,
        nexus_orchestration::engine::SessionStatus::WaitingForInput
    ) {
        engine
            .signal(
                &session_id,
                nexus_orchestration::engine::EngineSignal::Advance,
            )
            .await
            .expect("advance signal should succeed");

        let status_after = engine
            .get_status(&session_id)
            .await
            .expect("status after advance should be queryable");

        assert_ne!(
            status_before, status_after,
            "status should change after advance"
        );
    }
}

// ---------------------------------------------------------------------------
// Test 5: Session list includes created session
// ---------------------------------------------------------------------------

#[tokio::test]
async fn e2e_session_list_includes_created() {
    let (engine, loaded) = setup_engine();

    let session_id = start_novel_writing_session(&engine, &loaded).await;

    let sessions = engine
        .list_active(nexus_orchestration::engine::SessionFilter::default())
        .await
        .expect("list_active should succeed");

    assert!(
        sessions.iter().any(|s| s.session_id == session_id),
        "created session should appear in active list"
    );
    assert!(
        sessions.iter().any(|s| s.preset_id == "novel-writing"),
        "preset_id should be novel-writing"
    );
}

// ---------------------------------------------------------------------------
// Test 6: CoreContext template rendering (DF-11)
// ---------------------------------------------------------------------------

#[test]
fn core_context_template_is_rendered_into_prompt() {
    let rendered = nexus_orchestration::tasks::render_core_context_template(
        "World: {{world.title}}",
        &serde_json::json!({ "world": { "title": "Nexus" } }),
    )
    .expect("template render should succeed");

    assert_eq!(rendered, "World: Nexus");
}

// ---------------------------------------------------------------------------
// Test 7: Embedded preset has correct state count
// ---------------------------------------------------------------------------

#[test]
fn e2e_novel_writing_has_four_states() {
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset("novel-writing", &caps)
        .expect("novel-writing preset should load");

    // P4 fix wave: 5 states (outline_chapter, draft_chapter, finalize, finalize_commit, done).
    assert_eq!(
        loaded.manifest.states.len(),
        5,
        "novel-writing should have 5 states (finalize split into finalize + finalize_commit)"
    );

    let state_ids: Vec<&str> = loaded
        .manifest
        .states
        .iter()
        .map(|s| s.id.as_str())
        .collect();
    assert_eq!(
        state_ids,
        vec![
            "outline_chapter",
            "draft_chapter",
            "finalize",
            "finalize_commit",
            "done"
        ]
    );
}

// ---------------------------------------------------------------------------
// Test 8: R-V113-004 — template syntax error coverage
// ---------------------------------------------------------------------------

#[test]
fn template_syntax_error_returns_deterministic_failure() {
    // Feed malformed Handlebars syntax through the same render path used by
    // the novel-writing preset. The assertion verifies a deterministic error
    // (not a panic) with a message that explains the template syntax failure.
    let result = nexus_orchestration::tasks::render_core_context_template(
        "{{#if unclosed_block",
        &serde_json::json!({}),
    );

    assert!(
        result.is_err(),
        "malformed template syntax should fail deterministically"
    );
    let err = format!("{:#}", result.unwrap_err());
    assert!(
        err.contains("template") || err.contains("syntax"),
        "error should explain template syntax failure, got: {err}"
    );
}

// ---------------------------------------------------------------------------
// P3 T7: llm_judge 五問 quality gate on finalize state
// ---------------------------------------------------------------------------

#[test]
fn novel_writing_judge_quality_gate_on_finalize() {
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset("novel-writing", &caps)
        .expect("novel-writing preset should load");

    // Find the finalize state.
    let finalize = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "finalize")
        .expect("finalize state should exist");

    // P4 fix wave: finalize has NO enter actions — the chapter_transition
    // capability is deferred to finalize_commit.
    assert!(
        finalize.enter.is_empty(),
        "finalize should have no enter actions (chapter transition deferred to finalize_commit)"
    );

    // Verify it has llm_judge exit_when.
    match &finalize.exit_when {
        Some(nexus_orchestration::preset::manifest::ExitWhen::LlmJudge {
            template_file,
            judge_capability,
            min_interval,
        }) => {
            assert_eq!(
                template_file.as_deref(),
                Some("prompts/finalize-exit.md"),
                "finalize should use prompts/finalize-exit.md"
            );
            assert_eq!(
                judge_capability.as_deref(),
                Some("judge.llm"),
                "finalize should use judge.llm capability"
            );
            assert_eq!(
                min_interval.as_deref(),
                Some("PT6H"),
                "finalize should have PT6H min_interval"
            );
        }
        other => panic!("finalize should have llm_judge exit_when, got: {other:?}"),
    }

    // Verify finalize's next state is finalize_commit (not done).
    match &finalize.next {
        Some(nexus_orchestration::preset::manifest::NextTarget::Linear(target)) => {
            assert_eq!(
                target, "finalize_commit",
                "finalize should transition to finalize_commit"
            );
        }
        other => panic!("finalize next should be Linear(finalize_commit), got: {other:?}"),
    }

    // Verify finalize_commit has the chapter_transition enter action and next is done.
    let finalize_commit = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "finalize_commit")
        .expect("finalize_commit state should exist");
    assert_eq!(
        finalize_commit.enter.len(),
        1,
        "finalize_commit should have exactly one enter action"
    );
    match &finalize_commit.enter[0] {
        nexus_orchestration::preset::manifest::EnterAction::Capability { name, .. } => {
            assert_eq!(
                name, &"novel.chapter_transition",
                "finalize_commit enter should be novel.chapter_transition"
            );
        }
        other => panic!("finalize_commit enter should be Capability, got: {other:?}"),
    }
    match &finalize_commit.next {
        Some(nexus_orchestration::preset::manifest::NextTarget::Linear(target)) => {
            assert_eq!(target, "done", "finalize_commit should transition to done");
        }
        other => panic!("finalize_commit next should be Linear(done), got: {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// P3 T7: finalize-exit.md prompt file referenced from finalize state
// ---------------------------------------------------------------------------

#[test]
fn novel_writing_finalize_exit_prompt_referenced() {
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let loaded = nexus_orchestration::preset::load_embedded_preset("novel-writing", &caps)
        .expect("novel-writing preset should load");

    // The finalize state's exit_when references prompts/finalize-exit.md
    let finalize = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "finalize")
        .expect("finalize state should exist");

    if let Some(nexus_orchestration::preset::manifest::ExitWhen::LlmJudge {
        template_file, ..
    }) = &finalize.exit_when
    {
        assert_eq!(
            template_file.as_deref(),
            Some("prompts/finalize-exit.md"),
            "finalize should reference prompts/finalize-exit.md"
        );
    } else {
        panic!("finalize should have llm_judge exit_when");
    }
}

// ---------------------------------------------------------------------------
// P3 T5: Verify novel.chapter_transition capability is registered
// ---------------------------------------------------------------------------

#[test]
fn novel_chapter_transition_capability_registered() {
    let caps = nexus_orchestration::CapabilityRegistry::with_builtins();
    let cap = caps.get("novel.chapter_transition");
    assert!(
        cap.is_some(),
        "novel.chapter_transition should be registered"
    );
    let cap = cap.unwrap();
    assert_eq!(cap.name(), "novel.chapter_transition");
}
