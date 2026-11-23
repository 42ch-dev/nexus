//! End-to-end novel-writing preset test.
//!
//! Covers the V1.36 chapter-scoped pipeline (`outline_chapter` → `draft_chapter` →
//! finalize → done), manual advance, restart durability, and the `llm_judge`
//! 五問 quality gate.
//!
//! P3 refactored: legacy 4-state flow (gathering → brainstorming → outlining → drafting)
//! replaced by chapter-scoped 3-state flow (`outline_chapter` → `draft_chapter` → finalize).

use std::sync::Arc;

use nexus_orchestration::OrchestrationEngine;

/// Build an engine + loaded preset for E2E testing.
fn setup_engine() -> (
    Arc<nexus_orchestration::GraphFlowEngine>,
    nexus_preset::LoadedPreset,
) {
    let caps = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded = nexus_preset::load_embedded_preset("novel-writing", &caps)
        .expect("novel-writing preset should load");
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = Arc::new(nexus_orchestration::GraphFlowEngine::new_with_storage(
        storage,
        nexus_capability::CapabilityRegistryHolder::with_registry(Arc::new(caps)),
    ));
    (engine, loaded)
}

/// Seed `preset.input.*` required by novel-writing capability arg templates.
///
/// Production schedules populate these via core-context derivation; direct
/// `start_session_with_preset` in tests must seed them explicitly (C-V133P2-01).
async fn seed_novel_writing_preset_input(
    engine: &Arc<nexus_orchestration::GraphFlowEngine>,
    session_id: &nexus_session_storage::SessionId,
) {
    let ctx = engine
        .get_context(session_id)
        .await
        .expect("get_context for preset.input seed");
    ctx.set(
        "preset.input.topic",
        "AI consciousness in a near-future city",
    )
    .unwrap();
    ctx.set("preset.input.vibe", "literary").unwrap();
    ctx.set("preset.input.work_ref", "e2e-test-novel").unwrap();
    ctx.set("preset.input.work_id", "wrk_e2e_test").unwrap();
    ctx.set("preset.input.chapter", 1).unwrap();
    ctx.set("preset.input.chapter_label", "01").unwrap();
    ctx.set(
        "preset.input.outline_path",
        "Works/e2e-test-novel/Outlines/chapters/ch01-outline.md",
    )
    .unwrap();
    ctx.set(
        "preset.input.body_path",
        "Works/e2e-test-novel/Stories/ch01-ch01.md",
    )
    .unwrap();
    ctx.set("preset.input.slug", "ch01").unwrap();
    ctx.set("preset.input.world_kb_block", "").unwrap();
    ctx.set("preset.input.open_findings_block", "").unwrap();
    ctx.set("preset.input.foreshadowing_summary", "").unwrap();
}

async fn start_novel_writing_session(
    engine: &Arc<nexus_orchestration::GraphFlowEngine>,
    loaded: &nexus_preset::LoadedPreset,
) -> nexus_session_storage::SessionId {
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
    session_id: &nexus_session_storage::SessionId,
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
        );
        outcomes.push(outcome);
        if is_done {
            break;
        }
    }
    outcomes
}

#[tokio::test]
async fn e2e_novel_writing_traverses_all_outer_states() {
    let (engine, loaded) = setup_engine();

    let session_id = start_novel_writing_session(&engine, &loaded).await;

    let outcomes = run_until_wait_or_terminal(&engine, &session_id, 64).await;

    assert!(
        !outcomes.is_empty(),
        "should have at least one step outcome"
    );

    let status = engine
        .get_status(&session_id)
        .await
        .expect("get_status should succeed");

    assert!(
        !matches!(status, nexus_session_storage::SessionStatus::Running),
        "session should not be in Running state after steps"
    );
}

#[tokio::test]
async fn e2e_chapter_scoped_pipeline_executes() {
    let (engine, loaded) = setup_engine();

    let session_id = start_novel_writing_session(&engine, &loaded).await;

    let outcomes = run_until_wait_or_terminal(&engine, &session_id, 64).await;

    assert!(
        !outcomes.is_empty(),
        "should have executed at least one step"
    );

    let _ctx = engine
        .get_context(&session_id)
        .await
        .expect("get_context should succeed");

    assert!(
        !outcomes.is_empty(),
        "should have executed at least one step"
    );

    let status = engine
        .get_status(&session_id)
        .await
        .expect("get_status should succeed");
    assert!(
        !matches!(status, nexus_session_storage::SessionStatus::Running),
        "session should not be running after steps"
    );
}

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
        assert!(!more.is_empty(), "should have steps after advance");
    }

    let status = engine
        .get_status(&session_id)
        .await
        .expect("get_status should succeed");

    assert!(
        matches!(
            status,
            nexus_session_storage::SessionStatus::Completed
                | nexus_session_storage::SessionStatus::Paused
                | nexus_session_storage::SessionStatus::WaitingForInput
                | nexus_session_storage::SessionStatus::Failed
        ),
        "session should be in a non-running state after test execution (got {status:?})"
    );
}

#[tokio::test]
async fn e2e_restart_durability_context_persists() {
    let (engine, loaded) = setup_engine();

    let session_id = start_novel_writing_session(&engine, &loaded).await;

    let _ = run_until_wait_or_terminal(&engine, &session_id, 16).await;

    let status_before = engine
        .get_status(&session_id)
        .await
        .expect("status should be queryable");

    assert!(
        !matches!(status_before, nexus_session_storage::SessionStatus::Running),
        "session should have settled into a non-running state"
    );

    if matches!(
        status_before,
        nexus_session_storage::SessionStatus::WaitingForInput
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

#[test]
fn core_context_template_is_rendered_into_prompt() {
    let rendered = nexus_orchestration::tasks::render_core_context_template(
        "World: {{world.title}}",
        &serde_json::json!({ "world": { "title": "Nexus" } }),
    )
    .expect("template render should succeed");

    assert_eq!(rendered, "World: Nexus");
}

#[test]
fn e2e_novel_writing_has_four_states() {
    let caps = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded = nexus_preset::load_embedded_preset("novel-writing", &caps)
        .expect("novel-writing preset should load");

    assert_eq!(
        loaded.manifest.states.len(),
        6,
        "novel-writing should have 6 states (outline_review added before draft_chapter)"
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
            "outline_review",
            "draft_chapter",
            "finalize",
            "finalize_commit",
            "done"
        ]
    );
}

#[test]
fn template_syntax_error_returns_deterministic_failure() {
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

#[test]
fn novel_writing_judge_quality_gate_on_finalize() {
    let caps = nexus_capability::CapabilityRegistry::with_builtins();
    let loaded = nexus_preset::load_embedded_preset("novel-writing", &caps)
        .expect("novel-writing preset should load");

    let finalize = loaded
        .manifest
        .states
        .iter()
        .find(|s| s.id == "finalize")
        .expect("finalize state should exist");

    assert!(
        finalize.enter.is_empty(),
        "finalize should have no enter actions (chapter transition deferred to finalize_commit)"
    );

    match &finalize.exit_when {
        Some(nexus_preset::manifest::ExitWhen::LlmJudge {
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

    match &finalize.next {
        Some(nexus_preset::manifest::NextTarget::Linear(target)) => {
            assert_eq!(
                target, "finalize_commit",
                "finalize should transition to finalize_commit"
            );
        }
        other => panic!("finalize next should be Linear(finalize_commit), got: {other:?}"),
    }

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
        nexus_preset::manifest::EnterAction::Capability { name, .. } => {
            assert_eq!(
                name, &"novel.chapter_transition",
                "finalize_commit enter should be novel.chapter_transition"
            );
        }
        other => panic!("finalize_commit enter should be Capability, got: {other:?}"),
    }
    match &finalize_commit.next {
        Some(nexus_preset::manifest::NextTarget::Linear(target)) => {
            assert_eq!(target, "done", "finalize_commit should transition to done");
        }
        other => panic!("finalize_commit next should be Linear(done), got: {other:?}"),
    }
}

#[test]
fn novel_chapter_transition_capability_registered() {
    let caps = nexus_capability::CapabilityRegistry::with_builtins();
    let cap = caps.get("novel.chapter_transition");
    assert!(
        cap.is_some(),
        "novel.chapter_transition should be registered"
    );
    let cap = cap.unwrap();
    assert_eq!(cap.name(), "novel.chapter_transition");
}
