//! End-to-end tests for the `_system.maintenance` system preset.
//!
//! Tests:
//! - `system_preset_runs_to_terminal_state`: runs the preset graph to completion.
//! - `restart_durability_e2e`: verifies session survives engine restart.

use graph_flow::SessionStorage;
use nexus_orchestration::{system_preset, GraphFlowEngine, OrchestrationEngine};
use std::sync::Arc;

/// Run the system preset graph and verify it reaches terminal state.
#[tokio::test]
async fn system_preset_runs_to_terminal_state() {
    let storage = Arc::new(graph_flow::InMemorySessionStorage::new());
    let engine = GraphFlowEngine::new_with_storage(storage);
    let graph = system_preset::build();
    let sid = engine
        .start_session("_system.maintenance", graph)
        .await
        .expect("start_session");

    // Step until Completed or a reasonable cap (4 tasks + 1 buffer).
    for _ in 0..16 {
        let outcome = engine.run_step(&sid).await.expect("run_step");
        if let nexus_orchestration::engine::StepOutcome::Completed { .. } = outcome {
            break;
        }
    }

    let final_status = engine.get_status(&sid).await.expect("get_status");
    assert!(
        final_status.is_completed(),
        "system preset did not complete: {:?}",
        final_status
    );
}

/// Run a step, drop the engine, create a fresh one, and verify session state.
///
/// This uses `SqliteSessionStorage` (on-disk) to verify that session
/// state survives engine restart. After a step, the session's
/// `current_task_id` is persisted. A new engine loading the same storage
/// can read the session back.
#[tokio::test]
async fn restart_durability_e2e() {
    // Create a temp database.
    let db = tempfile::NamedTempFile::new().expect("tempfile");
    let db_path = db.path().to_path_buf();

    // Phase 1: Create engine, start session, run one step.
    let sid = {
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open_pool");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run_migrations");
        let storage = Arc::new(nexus_orchestration::storage::SqliteSessionStorage::new(
            std::sync::Arc::new(pool),
        ));
        let engine = GraphFlowEngine::new_with_storage(storage);
        let graph = system_preset::build();
        let sid = engine
            .start_session("_system.maintenance", graph)
            .await
            .expect("start_session");

        // Run one step.
        let outcome = engine.run_step(&sid).await.expect("run_step");
        assert!(
            matches!(
                outcome,
                nexus_orchestration::engine::StepOutcome::Completed { .. }
                    | nexus_orchestration::engine::StepOutcome::Paused { .. }
            ),
            "first step should complete or pause: {:?}",
            outcome
        );

        sid
    };
    // Phase 1 ends — pool drops, simulating daemon shutdown.

    // Phase 2: Create a fresh engine with a new pool over the same DB.
    {
        let pool = nexus_local_db::open_pool(&db_path)
            .await
            .expect("open_pool v2");
        nexus_local_db::run_migrations(&pool)
            .await
            .expect("run_migrations v2");
        let storage = Arc::new(nexus_orchestration::storage::SqliteSessionStorage::new(
            std::sync::Arc::new(pool),
        ));

        // Verify the session is still in the storage.
        let session = storage
            .get(&sid.0)
            .await
            .expect("storage get")
            .expect("session should exist after restart");

        // The session should have a valid current_task_id (not the initial
        // empty value), proving that the step was persisted.
        assert!(
            !session.current_task_id.is_empty(),
            "session should have a current_task_id after a step, got: '{}'",
            session.current_task_id
        );
    }
}
