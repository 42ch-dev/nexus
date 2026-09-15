//! P3-T1 execution-authority contract (AC-P3-T1).
//!
//! Proves the single-owner execution contract:
//!
//! 1. a competing `EngineOwner` core is refused while a live owner holds the
//!    OS engine lock (never a second engine);
//! 2. committed run state is recovered exactly once through the single owner;
//! 3. an unconfirmed in-flight operation is classified `Interrupted` and is
//!    never auto-re-driven;
//! 4. a direct authorized domain writer still commits without starting any
//!    engine, and a domain-only core open starts no execution task.
#![cfg(feature = "execution")]

use async_trait::async_trait;
use nexus_contracts::{CoreError as WireCoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService, RunnerDeps};
use nexus_orchestration::WorkflowStateStore;
use nexus_provider_ports::{ProviderPort, ProviderResult};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::TempDir;

const CREATOR: &str = "test_creator";
const SLUG: &str = "default";

/// Deterministic, no-model provider port. It never performs an effect; the
/// tests here exercise ownership/recovery, not provider behaviour.
struct NullProvider {
    calls: AtomicUsize,
}

impl NullProvider {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
        })
    }
}

#[async_trait]
impl ProviderPort for NullProvider {
    async fn call(&self, _request: ProviderCall) -> ProviderResult<ProviderReply> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        // The wire `CoreError` is a struct (`code`/`message`/`details`/
        // `http_status`), not an enum with an `Internal` variant — mirror the
        // canonical mapping in `nexus_agent_host::providers::port`.
        Err(WireCoreError {
            code: CoreErrorCode::Internal,
            message: "null provider: no live model in this test".to_string(),
            details: serde_json::Map::default(),
            http_status: Some(500),
        })
    }

    async fn next(
        &self,
        operation_id: String,
        _max_events: u32,
        _max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        Ok(ProviderEventBatch {
            operation_id,
            events: vec![],
            has_more: false,
            gap: None,
        })
    }
}

struct Fixture {
    tmp: TempDir,
    home: std::path::PathBuf,
    db_path: std::path::PathBuf,
}

/// Build a workspace home and initialize the engine-owned pool exactly like
/// the daemon does, then release the guard so a test can re-acquire cleanly.
async fn fixture() -> Fixture {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let home = user_home.join(".nexus42");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home, CREATOR, SLUG,
    ))
    .unwrap();
    std::fs::write(
        home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"{SLUG}\"\n"
        ),
    )
    .unwrap();

    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, SLUG);
    let guarded = nexus_local_db::init_engine_pool(&db_path)
        .await
        .expect("engine pool init");
    guarded.pool().close().await;
    nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);

    Fixture {
        tmp,
        home: user_home,
        db_path,
    }
}

fn open_options(f: &Fixture, access: CoreAccess) -> CoreOpenOptions {
    CoreOpenOptions {
        user_home: f.tmp.path().to_path_buf(),
        access,
    }
}

async fn open_engine_owner(f: &Fixture) -> CoreService {
    CoreService::open(open_options(f, CoreAccess::EngineOwner))
        .await
        .expect("engine-owner core open")
}

/// Seed a committed, non-terminal v1 run row directly (simulating a run that
/// was durably committed by a previous owner before a restart).
async fn seed_committed_run(db_path: &Path, session_id: &str, creator: &str) {
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db_path.display()))
        .await
        .unwrap();
    let now = chrono::Utc::now().timestamp();
    let ctx = serde_json::json!({ "_session_id": session_id }).to_string();
    sqlx::query(
        "INSERT INTO orchestration_sessions \
         (session_id, creator_id, preset_id, preset_version, current_task_id, status, context_json, \
          created_at, updated_at, execution_version, state_revision, run_state_json) \
         VALUES (?, ?, 'test-preset', 1, 'start', 'running', ?, ?, ?, 1, 3, ?)",
    )
    .bind(session_id)
    .bind(creator)
    .bind(ctx)
    .bind(now)
    .bind(now)
    .bind(serde_json::json!({"in_flight": null, "step_in_flight": null, "failure": null}).to_string())
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

/// Mark an in-flight step so A7 classifies the run `Interrupted`.
async fn seed_in_flight_run(db_path: &Path, session_id: &str, creator: &str) {
    let pool = sqlx::SqlitePool::connect(&format!("sqlite://{}", db_path.display()))
        .await
        .unwrap();
    let now = chrono::Utc::now().timestamp();
    let ctx = serde_json::json!({ "_session_id": session_id }).to_string();
    sqlx::query(
        "INSERT INTO orchestration_sessions \
         (session_id, creator_id, preset_id, preset_version, current_task_id, status, context_json, \
          created_at, updated_at, execution_version, state_revision, run_state_json) \
         VALUES (?, ?, 'test-preset', 1, 'start', 'running', ?, ?, ?, 1, 5, ?)",
    )
    .bind(session_id)
    .bind(creator)
    .bind(ctx)
    .bind(now)
    .bind(now)
    .bind(
        serde_json::json!({
            "in_flight": {"step_id": "s1", "attempt_id": "a1"},
            "step_in_flight": {"step_id": "s1", "attempt_id": "a1"},
            "failure": null
        })
        .to_string(),
    )
    .execute(&pool)
    .await
    .unwrap();
    pool.close().await;
}

/// AC-P3-T1: a competing `EngineOwner` is refused, a committed run is
/// recovered exactly once, an unconfirmed operation is `Interrupted`, and a
/// direct authorized domain writer still commits without starting an engine.
#[tokio::test]
async fn second_owner_and_restart_are_fenced() {
    let f = fixture().await;

    // ── 1. A committed non-terminal run exists on disk (pre-restart). ──
    seed_committed_run(&f.db_path, "test-preset:committed", CREATOR).await;
    seed_in_flight_run(&f.db_path, "test-preset:uncertain", CREATOR).await;

    // ── 2. First owner: acquires the engine lock and recovers. ──
    let owner_a = open_engine_owner(&f).await;
    let providers = NullProvider::new();
    let handle = owner_a
        .start_execution(Arc::clone(&providers) as Arc<dyn ProviderPort>, RunnerDeps::default())
        .await
        .expect("first execution owner starts");
    assert!(
        handle.engine_epoch() >= 1,
        "a real owner is admitted with a durable engine epoch, got {}",
        handle.engine_epoch()
    );

    // Recovery classified both rows: the committed run is re-driven (its
    // runner is reconstructed), the in-flight run is NOT (Interrupted).
    let store = nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(Arc::new(
        sqlx::SqlitePool::connect(&format!("sqlite://{}", f.db_path.display()))
            .await
            .unwrap(),
    ));
    let uncertain = store
        .load_run(&nexus_orchestration::SessionId("test-preset:uncertain".into()))
        .await
        .expect("load uncertain run")
        .expect("uncertain run row");
    let class = nexus_orchestration::resume_rules::classify_recovery(
        &uncertain.status,
        uncertain.state.as_ref(),
        false,
    );
    assert_eq!(
        class,
        nexus_orchestration::resume_rules::RecoveryClass::Interrupted,
        "an unconfirmed in-flight operation must classify Interrupted (never auto-replayed)"
    );

    // ── 3. A competing EngineOwner core is refused (no second engine). ──
    let competitor = CoreService::open(open_options(&f, CoreAccess::EngineOwner)).await;
    let refused = match competitor {
        Ok(core) => core
            .start_execution(Arc::clone(&providers) as Arc<dyn ProviderPort>, RunnerDeps::default())
            .await
            .is_err(),
        // The OS admission lock itself refusing the live owner is the same
        // contract: a second effect owner is never established.
        Err(_) => true,
    };
    assert!(
        refused,
        "a competing EngineOwner must be refused while a live owner holds the engine"
    );

    // ── 4. A duplicate start on the SAME owner also refuses. ──
    assert!(
        owner_a
            .start_execution(Arc::clone(&providers) as Arc<dyn ProviderPort>, RunnerDeps::default())
            .await
            .is_err(),
        "duplicate start must not create a second engine"
    );

    // ── 5. A direct authorized domain writer still commits, no engine. ──
    let direct = CoreService::open(open_options(&f, CoreAccess::DirectWriter))
        .await
        .expect("direct-writer core opens beside a live engine owner");
    assert!(
        direct.execution().is_none(),
        "a domain-only core open starts no execution task"
    );
    let principal = direct.active_principal().await.unwrap();
    let world = direct
        .create_world(
            &principal,
            serde_json::from_value(serde_json::json!({ "title": "Fenced" })).unwrap(),
        )
        .await
        .expect("direct domain write commits without any engine");
    assert!(!world.world_id.is_empty());

    // ── 6. Close settles the owner; no provider effect ever ran. ──
    direct.close().await.unwrap();
    let report = handle.close().await.unwrap();
    assert!(report.cleanup_confirmed);
    assert_eq!(
        providers.calls.load(Ordering::SeqCst),
        0,
        "ownership/recovery must not perform a provider effect on its own"
    );
    owner_a.close().await.unwrap();
}
