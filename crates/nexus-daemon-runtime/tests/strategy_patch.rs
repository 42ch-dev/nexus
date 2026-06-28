//! Strategy patch route integration tests (R-V171P0-QC1-002, R-V171P0-QC2-S1).
//!
//! Exercises the three patch handlers through direct handler invocation with a
//! real `WorkspaceState`. HTTP routing for these paths is already covered by
//! route registration in `api/mod.rs`; handler-level integration gives us
//! deterministic filesystem and concurrency behaviour without axum-test path
//! param quirks.

#![allow(clippy::unwrap_used)]

use axum::extract::{Path, State};
use axum::Json;
use nexus_contracts::{
    StrategyPatchPromptTemplateRequest, StrategyPatchStateRequest, StrategyPatchTransitionRequest,
};
use nexus_daemon_runtime::api::errors::NexusApiError;
use nexus_daemon_runtime::api::handlers::strategy::{
    patch_prompt_template, patch_state, patch_transition,
};
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::workspace::WorkspaceState;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use std::sync::Arc;

/// Build a minimal valid user preset bundle and return its directory.
fn seed_test_bundle(nexus_home: &std::path::Path) -> std::path::PathBuf {
    let strategy_id = "test-strategy";
    let bundle_dir = nexus_home_layout::user_preset_bundle_dir(nexus_home, strategy_id);
    std::fs::create_dir_all(&bundle_dir).expect("create bundle dir");
    let yaml = r#"
revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Integration test strategy"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start state"
    next: end
  - id: end
    terminal: true
"#;
    std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write preset.yaml");
    bundle_dir
}

async fn test_state(
    tmp: test_utils::TestTempRoot,
    nexus_home: std::path::PathBuf,
    db_path: std::path::PathBuf,
) -> WorkspaceState {
    let db_url = format!("sqlite:{}?mode=rw", db_path.display());
    let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;

    let schedule_pool = Arc::new(sqlx::SqlitePool::connect(&db_url).await.unwrap());
    let supervisor = Arc::new(ScheduleSupervisor::new(schedule_pool));
    state.set_schedule_supervisor(supervisor);

    let registry = Arc::new(nexus_orchestration::CapabilityRegistry::with_builtins());
    state.set_capability_registry(registry);

    std::mem::forget(tmp);
    state
}

fn assert_conflict(err: &NexusApiError) {
    assert_eq!(err.status_code(), axum::http::StatusCode::CONFLICT);
    assert_eq!(err.error_code(), "strategy_conflict");
}

#[tokio::test]
async fn patch_state_renames_state_and_bumps_revision() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    seed_test_bundle(&nexus_home);
    let state = test_state(tmp, nexus_home.clone(), db_path).await;

    let req = StrategyPatchStateRequest {
        strategy_id: "test-strategy".to_string(),
        state_id: "start".to_string(),
        base_revision: 1,
        set: serde_json::json!({ "label": "begin", "description": "Begin here." }),
    };

    let res = patch_state(
        State(state),
        Path(("test-strategy".to_string(), "start".to_string())),
        Json(req),
    )
    .await
    .expect("patch_state should succeed");

    assert_eq!(res.new_revision, 2);

    let yaml = std::fs::read_to_string(
        nexus_home_layout::user_preset_bundle_dir(&nexus_home, "test-strategy").join("preset.yaml"),
    )
    .unwrap();
    assert!(yaml.contains("id: begin"));
    assert!(yaml.contains("revision: 2"));
}

#[tokio::test]
async fn patch_state_rejects_stale_revision() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    seed_test_bundle(&nexus_home);
    let state = test_state(tmp, nexus_home, db_path).await;

    let req = StrategyPatchStateRequest {
        strategy_id: "test-strategy".to_string(),
        state_id: "start".to_string(),
        base_revision: 0,
        set: serde_json::json!({ "description": "Stale." }),
    };

    let err = patch_state(
        State(state),
        Path(("test-strategy".to_string(), "start".to_string())),
        Json(req),
    )
    .await
    .expect_err("stale revision should fail");

    assert_conflict(&err);
}

#[tokio::test]
async fn patch_transition_rejects_invalid_condition() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    seed_test_bundle(&nexus_home);
    let state = test_state(tmp, nexus_home, db_path).await;

    let req = StrategyPatchTransitionRequest {
        strategy_id: "test-strategy".to_string(),
        base_revision: 1,
        source_state_id: "start".to_string(),
        old_target: "end".to_string(),
        new_target: None,
        condition: Some("not a valid condition @#$".to_string()),
        transition_kind: None,
    };

    let err = patch_transition(State(state), Path("test-strategy".to_string()), Json(req))
        .await
        .expect_err("invalid condition should fail");

    assert_eq!(err.status_code(), axum::http::StatusCode::BAD_REQUEST);
    match err {
        NexusApiError::BadRequest { code, .. } => {
            assert_eq!(code, "strategy_transition_condition_invalid");
        }
        other => panic!("expected BadRequest with condition code, got {other:?}"),
    }
}

#[tokio::test]
async fn patch_prompt_template_rolls_back_on_validation_failure() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    let bundle_dir = seed_test_bundle(&nexus_home);

    // Reference a missing template so validation fails after the staged write.
    let yaml = r#"
revision: 1
preset:
  id: test-strategy
  version: 1
  kind: creator
  description: "Integration test strategy"
  run_intents: [work_init]
  initial: start
  terminal: end
states:
  - id: start
    description: "Start state"
    context_update:
      op:
        kind: append
        body: ""
      template_file: prompts/missing.md
    next: end
  - id: end
    terminal: true
"#;
    std::fs::write(bundle_dir.join("preset.yaml"), yaml).expect("write preset.yaml");

    std::fs::create_dir_all(bundle_dir.join("prompts")).expect("create prompts dir");
    let other_path = bundle_dir.join("prompts/other.md");
    std::fs::write(&other_path, "original content").expect("write original template");

    let state = test_state(tmp, nexus_home, db_path).await;

    let req = StrategyPatchPromptTemplateRequest {
        strategy_id: "test-strategy".to_string(),
        state_id: "start".to_string(),
        base_revision: 1,
        template_ref: "prompts/other.md".to_string(),
        set: serde_json::json!({ "body": "new content" }),
    };

    let err = patch_prompt_template(
        State(state),
        Path(("test-strategy".to_string(), "start".to_string())),
        Json(req),
    )
    .await
    .expect_err("validation failure should roll back");

    assert_eq!(
        err.status_code(),
        axum::http::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(err.error_code(), "strategy_validation_failed");

    // The original content must be restored.
    let content = std::fs::read_to_string(&other_path).unwrap();
    assert_eq!(content, "original content");
}

#[tokio::test]
async fn concurrent_patch_state_serializes_and_one_writer_gets_conflict() {
    let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
    seed_test_bundle(&nexus_home);
    let state = test_state(tmp, nexus_home, db_path).await;

    let req_a = StrategyPatchStateRequest {
        strategy_id: "test-strategy".to_string(),
        state_id: "start".to_string(),
        base_revision: 1,
        set: serde_json::json!({ "description": "A" }),
    };
    let req_b = StrategyPatchStateRequest {
        strategy_id: "test-strategy".to_string(),
        state_id: "start".to_string(),
        base_revision: 1,
        set: serde_json::json!({ "description": "B" }),
    };

    let state_a = state.clone();
    let task_a = tokio::spawn(async move {
        patch_state(
            State(state_a),
            Path(("test-strategy".to_string(), "start".to_string())),
            Json(req_a),
        )
        .await
    });

    let task_b = tokio::spawn(async move {
        // Small delay so both requests are in flight and contend on the lock.
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        patch_state(
            State(state),
            Path(("test-strategy".to_string(), "start".to_string())),
            Json(req_b),
        )
        .await
    });

    let (res_a, res_b) = tokio::join!(task_a, task_b);
    let outcomes = [res_a.unwrap(), res_b.unwrap()];
    let successes = outcomes.iter().filter(|r| r.is_ok()).count();
    let conflicts = outcomes
        .iter()
        .filter(|r| {
            r.as_ref()
                .err()
                .is_some_and(|e| e.error_code() == "strategy_conflict")
        })
        .count();

    assert_eq!(successes, 1, "exactly one concurrent patch should succeed");
    assert_eq!(
        conflicts, 1,
        "the other concurrent patch should get a conflict"
    );
}
