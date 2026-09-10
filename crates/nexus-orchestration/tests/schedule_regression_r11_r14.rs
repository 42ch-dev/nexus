//! Regression gate tests R11–R14 per compass §6.
//!
//! R11: add → inspect roundtrip (v0 equals seed)
//! R12: mid-execution edit stability (edit creates v1; running state finishes on v0)
//! R13: preset `context_update` hook regression
//! R14: dependency chain auto-advance (A completes → B auto-starts)

use nexus_contracts::local::schedule::{
    CoreContextAuthor, CoreContextPayload, CoreContextVersion, DerivationStep, EditOp, Schedule,
    ScheduleConcurrency, ScheduleId, ScheduleStatus,
};
use nexus_local_db::SqlitePool;
use nexus_orchestration::schedule::derivation::CoreContextManager;
use nexus_orchestration::schedule::supervisor::ScheduleSupervisor;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a fresh test DB with migrations.
async fn fresh_pool() -> Arc<SqlitePool> {
    let db = tempfile::NamedTempFile::new().unwrap();
    let pool = nexus_local_db::open_pool(db.path())
        .await
        .expect("open pool");
    nexus_local_db::run_migrations(&pool)
        .await
        .expect("run migrations");
    std::mem::forget(db);
    Arc::new(pool)
}

fn make_schedule(
    id: &str,
    creator_id: &str,
    preset_id: &str,
    status: ScheduleStatus,
    concurrency: ScheduleConcurrency,
    depends_on: Vec<&str>,
) -> Schedule {
    Schedule {
        id: ScheduleId(id.to_string()),
        creator_id: creator_id.to_string(),
        preset_id: preset_id.to_string(),
        preset_version: 1,
        status,
        concurrency,
        depends_on: depends_on
            .into_iter()
            .map(|d| ScheduleId(d.to_string()))
            .collect(),
        current_core_context_version: CoreContextVersion(0),
        current_session_id: None,
        scheduled_at: None,
        // Use schedule ID as label to avoid R2 duplicate detection collisions
        label: Some(id.to_string()),
        created_at: String::new(),
        updated_at: String::new(),
        terminated_at: None,
    }
}

// ---------------------------------------------------------------------------
// R11: add → inspect roundtrip (v0 equals seed)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn r11_schedule_add_inspect_roundtrip() {
    let pool = fresh_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(pool.clone()));
    let mgr = CoreContextManager::new(pool.clone());

    let sid = "R11-A";
    let seed = "topic=bees vibe=literary";

    // Insert schedule
    let schedule = make_schedule(
        sid,
        "creator-1",
        "novel-writing",
        ScheduleStatus::Pending,
        ScheduleConcurrency::Serial,
        vec![],
    );
    sup.insert_pending(schedule).await.unwrap();

    // Seed core_context v0
    let record = mgr
        .apply_seed(
            &ScheduleId(sid.to_string()),
            seed,
            CoreContextAuthor::User {
                id: "creator-1".to_string(),
            },
        )
        .await
        .unwrap();

    assert_eq!(record.version, CoreContextVersion(0));

    // Inspect: v0 should match seed
    let snapshot = mgr
        .current_snapshot(&ScheduleId(sid.to_string()))
        .await
        .unwrap();
    match &snapshot.content {
        CoreContextPayload::Text { body } => {
            assert_eq!(body, seed, "v0 content must equal seed");
        }
        CoreContextPayload::Struct { .. } => panic!("expected text payload"),
    }

    // Status should be pending
    assert_eq!(sup.status_of(sid).await.unwrap(), ScheduleStatus::Pending);
}

// ---------------------------------------------------------------------------
// R12: mid-execution edit stability
// ---------------------------------------------------------------------------

#[tokio::test]
async fn r12_mid_execution_edit_does_not_disturb_running_state() {
    let pool = fresh_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(pool.clone()));
    let mgr = CoreContextManager::new(pool.clone());

    let sid = "R12-A";
    let seed = "initial context for state 1";

    // Insert and seed
    let schedule = make_schedule(
        sid,
        "creator-1",
        "test-preset",
        ScheduleStatus::Running,
        ScheduleConcurrency::Serial,
        vec![],
    );
    sup.insert_pending(schedule).await.unwrap();

    // Set to Running (simulate engine has started)
    let now = chrono::Utc::now().timestamp();
    // SAFETY: test-only — DML helper for test state setup.
    sqlx::query(
        "UPDATE creator_schedules SET status = 'running', updated_at = ? WHERE schedule_id = ?",
    )
    .bind(now)
    .bind(sid)
    .execute(&*pool)
    .await
    .unwrap();

    // Seed v0
    mgr.apply_seed(
        &ScheduleId(sid.to_string()),
        seed,
        CoreContextAuthor::System,
    )
    .await
    .unwrap();

    // Verify v0 content
    let v0 = mgr
        .read(&ScheduleId(sid.to_string()), CoreContextVersion(0))
        .await
        .unwrap();
    match &v0.content {
        CoreContextPayload::Text { body } => assert_eq!(body, seed),
        CoreContextPayload::Struct { .. } => panic!("expected text payload"),
    }

    // Simulate mid-execution edit: user appends while running
    let edit_text = "\n[user addition during execution]";
    let v1 = mgr
        .apply_user_edit(
            &ScheduleId(sid.to_string()),
            EditOp::Append {
                body: edit_text.to_string(),
            },
            Some("creator-1".to_string()),
        )
        .await
        .unwrap();

    assert_eq!(v1.version, CoreContextVersion(1), "edit should create v1");

    // v0 should still be readable (running state finishes on v0)
    let v0_still = mgr
        .read(&ScheduleId(sid.to_string()), CoreContextVersion(0))
        .await
        .unwrap();
    match &v0_still.content {
        CoreContextPayload::Text { body } => assert_eq!(body, seed, "v0 should be immutable"),
        CoreContextPayload::Struct { .. } => panic!("expected text payload"),
    }

    // v1 should have the appended content (next state reads v1)
    let v1_content = mgr
        .read(&ScheduleId(sid.to_string()), CoreContextVersion(1))
        .await
        .unwrap();
    match &v1_content.content {
        CoreContextPayload::Text { body } => {
            assert!(body.contains(seed), "v1 should still contain original seed");
            assert!(body.contains(edit_text), "v1 should contain the edit");
        }
        CoreContextPayload::Struct { .. } => panic!("expected text payload"),
    }
}

// ---------------------------------------------------------------------------
// R13: preset context_update hook regression
// ---------------------------------------------------------------------------

#[tokio::test]
async fn r13_preset_hook_writes_preset_hook_derivation() {
    let pool = fresh_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(pool.clone()));
    let mgr = CoreContextManager::new(pool.clone());

    let sid = "R13-A";

    // Insert and seed
    let schedule = make_schedule(
        sid,
        "creator-1",
        "novel-writing",
        ScheduleStatus::Pending,
        ScheduleConcurrency::Serial,
        vec![],
    );
    sup.insert_pending(schedule).await.unwrap();

    // Seed v0
    mgr.apply_seed(
        &ScheduleId(sid.to_string()),
        "topic=bees",
        CoreContextAuthor::System,
    )
    .await
    .unwrap();

    // Simulate outlining state exit → preset hook fires
    let hook_result = mgr
        .apply_preset_hook(
            &ScheduleId(sid.to_string()),
            "outlining",
            "context_update",
            EditOp::Append {
                body: "\n[Outline — v2]\nChapter 1: The Bees".to_string(),
            },
        )
        .await
        .unwrap();

    // Verify the derivation step is PresetHook
    match &hook_result.derivation {
        DerivationStep::PresetHook {
            state_id,
            hook_name,
        } => {
            assert_eq!(state_id, "outlining");
            assert_eq!(hook_name, "context_update");
        }
        other => panic!("expected PresetHook derivation, got: {other:?}"),
    }

    // Verify the history contains the PresetHook entry
    let current = mgr
        .current_snapshot(&ScheduleId(sid.to_string()))
        .await
        .unwrap();
    match &current.content {
        CoreContextPayload::Text { body } => {
            assert!(body.contains("topic=bees"), "should still contain seed");
            assert!(
                body.contains("[Outline"),
                "should contain appended outline hook content"
            );
        }
        CoreContextPayload::Struct { .. } => panic!("expected text payload"),
    }
}

// ---------------------------------------------------------------------------
// R14: dependency chain auto-advance
// ---------------------------------------------------------------------------

#[tokio::test]
async fn r14_dependency_chain_auto_advances() {
    let pool = fresh_pool().await;
    let sup = Arc::new(ScheduleSupervisor::new(pool.clone()));
    let mgr = CoreContextManager::new(pool.clone());

    let sid_a = "R14-A";
    let sid_b = "R14-B";

    // Insert A (no deps)
    let schedule_a = make_schedule(
        sid_a,
        "creator-1",
        "test-preset",
        ScheduleStatus::Pending,
        ScheduleConcurrency::Serial,
        vec![],
    );
    sup.insert_pending(schedule_a).await.unwrap();

    // Insert B (depends on A)
    let schedule_b = make_schedule(
        sid_b,
        "creator-1",
        "test-preset",
        ScheduleStatus::Pending,
        ScheduleConcurrency::Serial,
        vec![sid_a],
    );
    sup.insert_pending(schedule_b).await.unwrap();

    // Seed both
    for sid in [sid_a, sid_b] {
        mgr.apply_seed(
            &ScheduleId(sid.to_string()),
            &format!("seed for {sid}"),
            CoreContextAuthor::System,
        )
        .await
        .unwrap();
    }

    // Tick: A should start (no deps, running set empty)
    sup.tick().await.unwrap();
    assert_eq!(sup.status_of(sid_a).await.unwrap(), ScheduleStatus::Running);
    assert_eq!(
        sup.status_of(sid_b).await.unwrap(),
        ScheduleStatus::Pending,
        "B should not start — depends on A"
    );

    // Simulate A completing
    sup.on_schedule_terminal(sid_a, ScheduleStatus::Completed)
        .await
        .unwrap();
    assert_eq!(
        sup.status_of(sid_a).await.unwrap(),
        ScheduleStatus::Completed
    );

    // After A completes, tick should auto-start B (deps satisfied)
    assert_eq!(
        sup.status_of(sid_b).await.unwrap(),
        ScheduleStatus::Running,
        "B should auto-start after A completes"
    );
}
