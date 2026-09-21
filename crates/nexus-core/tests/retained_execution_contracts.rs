//! Retained execution-domain contracts (v1.193 P2-T8).
//!
//! This target owns the execution-domain behaviour the retired host fixtures
//! protected before the daemon composition was deleted: schedule admission,
//! workflow restart/recovery and writer-lock semantics. Every case here is
//! **MIGRATED** from a named retired fixture (never a pre-existing core
//! selector), and the migration keeps the durable-domain assertion while
//! dropping the HTTP envelope, router registration and boot-only expectation
//! that died with the host.
//!
//! Provenance (source file → case):
//! - `apps/nexus42/tests/workflow_execution_cli.rs` →
//!   [`admission_distinct_serial_rows_race_one_owned`]
//! - `apps/nexus42/tests/ops_e2e_converge_timeout.rs` →
//!   [`hanging_upstream_with_timeout_reroutes_via_on_timeout`]
//! - `apps/nexus42/tests/ops_e2e_resume_restart.rs` →
//!   [`restart_mid_chain_resumes_without_re_executing_completed_edges`]
//! - `crates/nexus-daemon-runtime/tests/cron_supervisor_task.rs` →
//!   [`cron_tick_enqueues_and_admits_a_due_role`],
//!   [`cron_tick_with_no_due_role_is_a_no_op`]
//! - `crates/nexus-daemon-runtime/tests/auto_chronology_task.rs` →
//!   [`auto_chronology_tick_advances_an_opted_in_work`],
//!   [`auto_chronology_interval_parsing_is_total`]
//! - `crates/nexus-daemon-runtime/tests/cron_lock_integration.rs` →
//!   [`cron_fire_is_gated_while_the_works_file_lock_is_held`]
//! - `crates/nexus-daemon-runtime/tests/master_decision_timeout.rs` →
//!   the four `stale_finding_sweep_*` cases
//! - `crates/nexus-daemon-runtime/tests/runtime_lock.rs` →
//!   the three `reconcile_*` lock-window cases
//! - `crates/nexus-daemon-runtime/tests/workflow_prompt_execution.rs` →
//!   [`prompt_executor_single_flights_one_session_and_refuses_before_effect`],
//!   [`prompt_executor_types_eof_after_initialize_as_a_launch_failure`],
//!   [`graph_prompt_context_serializes_output_without_host_handles`]
//!   (the executor's moved module keeps its own cancel/cleanup cases)
//!
//! No case here talks HTTP, boots a router, or launches a provider process:
//! the prompt port is the production [`HostPromptExecutor`] over a
//! deterministic parked Host fixture, and every other seam is the real core
//! domain owner.

#![cfg(feature = "execution")]
#![allow(clippy::unwrap_used, clippy::expect_used)]

use async_trait::async_trait;
use futures_util::StreamExt;
use nexus_agent_host::capability::model::{
    CapabilityDescriptor, CreateSessionRequest, FinishReason, HostEvent, HostEventStream,
    HostHealth, HostOperation, HostStartConfig, OperationFinishedEvent, OperationStartedEvent,
    TextDeltaEvent,
};
use nexus_agent_host::config::TimeoutConfig;
use nexus_agent_host::{
    HostError, HostFacade, HostOperationId, HostResult, HostSession, HostSessionId,
    ProviderCatalog, SessionState,
};
use nexus_contracts::local::schedule::http::{AddScheduleRequest, AgentBindingDto};
use nexus_contracts::{
    CoreError as WireCoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply,
};
use nexus_core::execution::prompt_executor::HostPromptExecutor;
use nexus_core::execution::schedules::chronology::{
    parse_interval_secs, run_one_tick as chronology_run_one_tick, AutoChronologyConfig,
    DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS, ENV_AUTO_CHRONOLOGY_INTERVAL_MIN,
};
use nexus_core::execution::schedules::cron;
use nexus_core::execution::schedules::stale_findings::run_one_sweep;
use nexus_core::execution::workflow::WorkflowRunCoordinator;
use nexus_core::execution::{
    drive_preset_run, resume_driven_sessions, ExecutionHandle, PresetRunConfig, PresetRunOutcome,
    ResumeDecision, RunControlError, RunnerDeps,
};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_local_db::findings::{create_finding, Finding};
use nexus_local_db::works::{create_work_atomic, WorkRecord};
use nexus_local_db::writer_protocol::init_guarded_pool;
use nexus_orchestration::capability::{CapabilityError, DaemonToolDispatch};
use nexus_orchestration::engine::{
    GraphFlowEngine, OrchestrationEngine, SessionStatus, SessionSummary,
};
use nexus_orchestration::preset_runtime::build_wired_outer_graph;
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_orchestration::{CapabilityRegistry, CapabilityRegistryHolder, WorkflowStateStore};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use tempfile::TempDir;

const CREATOR: &str = "retained_creator";
const SLUG: &str = "default";
/// An embedded preset that declares a prompt role, so the frozen descriptor
/// needs an explicit binding and the drive reaches the prompt port.
const PROMPT_PRESET: &str = "memory-augmented";
const PROVIDER: &str = "retained-provider";

// ─────────────────────────────────────────────────────────────────────────────
// Fixtures
// ─────────────────────────────────────────────────────────────────────────────

/// Deterministic provider port that never performs an effect. The prompt port
/// is `HostPromptExecutor`; this port only satisfies `start_execution`.
struct NullProvider;

#[async_trait]
impl ProviderPort for NullProvider {
    async fn call(&self, _request: ProviderCall) -> ProviderResult<ProviderReply> {
        Err(WireCoreError {
            code: CoreErrorCode::Internal,
            message: "retained fixtures never call a live provider".to_string(),
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

/// Deterministic parked Host fixture.
///
/// MIGRATED from the retired `workflow_execution_cli.rs`
/// `BlockingHost::non_cooperative`: the first prompt parks until the test
/// releases it (or the operation is cancelled), so a concurrent admission race
/// is between two CLAIMS — never between a claim and a run that already settled
/// terminal. No provider process is launched.
struct ParkedHost {
    sessions: Mutex<HashMap<HostSessionId, HostSession>>,
    parked: Mutex<Vec<tokio::sync::oneshot::Sender<()>>>,
    released: AtomicBool,
    prompts: AtomicUsize,
    creates: AtomicUsize,
    /// When set, a parked prompt finishes with a NON-`EndTurn` stop so the
    /// executor's typed-failure path (never partial-output success) is
    /// exercised deterministically.
    non_end_turn: AtomicBool,
    /// When set, every session launch (`create_session`) fails with the typed
    /// launch error an agent process that exits right after the `initialize`
    /// handshake produces.
    launch_fails: AtomicBool,
    /// When set, a prompt's event stream closes (EOF) after its deltas
    /// without ever emitting a terminal event.
    stream_eof: AtomicBool,
}

impl ParkedHost {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            parked: Mutex::new(Vec::new()),
            released: AtomicBool::new(false),
            prompts: AtomicUsize::new(0),
            creates: AtomicUsize::new(0),
            non_end_turn: AtomicBool::new(false),
            launch_fails: AtomicBool::new(false),
            stream_eof: AtomicBool::new(false),
        })
    }

    /// Make every subsequent prompt finish with a non-`EndTurn` stop.
    fn stop_without_end_turn(&self) {
        self.non_end_turn.store(true, Ordering::SeqCst);
    }

    /// Make every subsequent session launch fail: the agent process exits
    /// right after the `initialize` handshake, so no session is ever handed
    /// out.
    fn fail_launch_after_initialize(&self) {
        self.launch_fails.store(true, Ordering::SeqCst);
    }

    /// Make every subsequent prompt stream close after its deltas without a
    /// terminal event.
    fn close_stream_after_deltas(&self) {
        self.stream_eof.store(true, Ordering::SeqCst);
    }

    fn prompt_count(&self) -> usize {
        self.prompts.load(Ordering::SeqCst)
    }

    fn session_create_count(&self) -> usize {
        self.creates.load(Ordering::SeqCst)
    }

    /// Release every parked prompt so the drive loop can finish and `close()`
    /// can join it.
    fn release_all(&self) {
        self.released.store(true, Ordering::SeqCst);
        for sender in self.parked.lock().expect("parked").drain(..) {
            let _ = sender.send(());
        }
    }
}

#[async_trait]
impl HostFacade for ParkedHost {
    async fn start(&self, _config: HostStartConfig) -> HostResult<()> {
        Ok(())
    }

    async fn create_session(&self, request: CreateSessionRequest) -> HostResult<HostSession> {
        self.creates.fetch_add(1, Ordering::SeqCst);
        if self.launch_fails.load(Ordering::SeqCst) {
            return Err(HostError::launch_failed(
                request.provider_id,
                "agent process exited after initialize (EOF)",
                None,
            ));
        }
        let session = HostSession {
            id: HostSessionId::new(),
            provider_id: request.provider_id,
            state: SessionState::Ready,
            created_at: chrono::Utc::now(),
            active_op_id: None,
            negotiated_capabilities: CapabilityDescriptor::native_cli_limited(),
            owner: request.owner,
            process_identity: None,
        };
        self.sessions
            .lock()
            .expect("sessions")
            .insert(session.id.clone(), session.clone());
        Ok(session)
    }

    async fn exec(
        &self,
        session_id: HostSessionId,
        op: HostOperation,
    ) -> HostResult<HostEventStream> {
        let op_id = match op {
            HostOperation::Prompt { op_id, .. } => op_id,
            other => {
                return Err(HostError::internal(format!(
                    "unexpected operation {other:?}"
                )));
            }
        };
        self.prompts.fetch_add(1, Ordering::SeqCst);

        let started = HostEvent::OpStarted(OperationStartedEvent {
            op_id: op_id.clone(),
            session_id: session_id.clone(),
        });
        let delta = HostEvent::MessageDelta(TextDeltaEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            text: "transformed:parked-output".to_string(),
        });
        // EOF after the deltas: the stream closes without any terminal event
        // (the "the process stopped talking" analogue of the launch EOF).
        if self.stream_eof.load(Ordering::SeqCst) {
            return Ok(Box::pin(futures_util::stream::iter(vec![
                Ok(started),
                Ok(delta),
            ])));
        }
        let reason = if self.non_end_turn.load(Ordering::SeqCst) {
            FinishReason::Cancelled
        } else {
            FinishReason::EndTurn
        };
        let finished = HostEvent::OpFinished(OperationFinishedEvent {
            session_id,
            op_id,
            reason,
        });

        if self.released.load(Ordering::SeqCst) {
            return Ok(Box::pin(futures_util::stream::iter(vec![
                Ok(started),
                Ok(delta),
                Ok(finished),
            ])));
        }
        let (release_tx, release_rx) = tokio::sync::oneshot::channel::<()>();
        self.parked.lock().expect("parked").push(release_tx);
        Ok(Box::pin(
            futures_util::stream::once(async move { Ok(started) })
                .chain(futures_util::stream::once(async move {
                    let _ = release_rx.await;
                    Ok(delta)
                }))
                .chain(futures_util::stream::once(async move { Ok(finished) })),
        ))
    }

    async fn cancel(&self, op_id: HostOperationId) -> HostResult<()> {
        let _ = op_id;
        Ok(())
    }

    async fn health(&self) -> HostResult<HostHealth> {
        Ok(HostHealth {
            running: true,
            active_sessions: self.sessions.lock().expect("sessions").len(),
            active_operations: 0,
        })
    }

    async fn shutdown(&self) -> HostResult<()> {
        self.sessions.lock().expect("sessions").clear();
        Ok(())
    }

    async fn shutdown_session(&self, session_id: HostSessionId) -> HostResult<()> {
        self.sessions.lock().expect("sessions").remove(&session_id);
        Ok(())
    }

    async fn list_sessions(&self) -> HostResult<Vec<HostSession>> {
        Ok(self
            .sessions
            .lock()
            .expect("sessions")
            .values()
            .cloned()
            .collect())
    }

    async fn provider_catalog(&self) -> HostResult<ProviderCatalog> {
        Ok(ProviderCatalog::new())
    }

    fn subscribe_events(
        &self,
        _session_id: HostSessionId,
    ) -> tokio::sync::broadcast::Receiver<HostEvent> {
        let (tx, _) = tokio::sync::broadcast::channel(16);
        tx.subscribe()
    }
}

/// An engine-owner core over a fresh guarded workspace, with the production
/// prompt executor bound to the deterministic parked Host.
struct OwnerFixture {
    tmp: TempDir,
    core: CoreService,
    handle: Arc<ExecutionHandle>,
    host: Arc<ParkedHost>,
    executor: Arc<HostPromptExecutor>,
}

impl OwnerFixture {
    fn nexus_home(&self) -> PathBuf {
        self.tmp.path().join(".nexus42")
    }

    fn pool(&self) -> Arc<sqlx::SqlitePool> {
        self.handle.coordinator().pool()
    }

    fn coordinator(&self) -> Arc<WorkflowRunCoordinator> {
        self.handle.coordinator()
    }
}

async fn owner_fixture() -> OwnerFixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, CREATOR, SLUG,
    ))
    .unwrap();
    std::fs::write(
        home.join(".nexus42/config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"{SLUG}\"\n"
        ),
    )
    .unwrap();

    let db_path = nexus_home_layout::workspace_state_db_path(home, CREATOR, SLUG);
    // Seed through a temporary engine pool and RELEASE the exclusive writer
    // before the engine owner opens (the owner takes an OS lock, not a
    // stealable lease — the seeder must be gone).
    {
        let guarded = nexus_local_db::init_engine_pool(&db_path)
            .await
            .expect("engine pool init");
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, \
             cached_at, data) VALUES (?, 'Retained', 'active', datetime('now'), '{}')",
        )
        .bind(CREATOR)
        .execute(guarded.pool())
        .await
        .expect("seed the admitted creator row");
        guarded.pool().close().await;
        nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);
    }

    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .expect("engine-owner core open");

    let host = ParkedHost::new();
    let store: Arc<dyn nexus_orchestration::run_state::WorkflowStateStore> =
        Arc::new(SqliteSessionStorage::new(Arc::new(core.pool().clone())));
    let executor = Arc::new(HostPromptExecutor::new(
        host.clone(),
        store,
        TimeoutConfig::default(),
    ));
    let deps = RunnerDeps {
        prompt_executor: Some(
            executor.clone() as Arc<dyn nexus_orchestration::capability::PromptExecutor>
        ),
        workspace_root: Some(nexus_home_layout::operational_workspace_dir(
            home, CREATOR, SLUG,
        )),
        nexus_home: Some(home.join(".nexus42")),
        ..RunnerDeps::default()
    };
    let handle = core
        .start_execution(Arc::new(NullProvider) as Arc<dyn ProviderPort>, deps)
        .await
        .expect("execution owner starts");
    handle.set_schedule_supervisor(Arc::new(
        nexus_orchestration::schedule::supervisor::ScheduleSupervisor::new(
            handle.coordinator().pool(),
        ),
    ));

    OwnerFixture {
        tmp,
        core,
        handle,
        host,
        executor,
    }
}

/// `memory-augmented` declares a prompt role, so the freeze requires an
/// explicit binding for the `default` role.
fn default_bindings() -> HashMap<String, AgentBindingDto> {
    let mut bindings = HashMap::new();
    bindings.insert(
        "default".to_string(),
        AgentBindingDto {
            provider_id: PROVIDER.to_string(),
            model: None,
        },
    );
    bindings
}

/// A pending, serial, `driven_v1` schedule for the admitted creator, created
/// through the real core insert path (frozen descriptor + version-0 seed).
async fn add_pending_schedule(fixture: &OwnerFixture, label: &str) -> String {
    let principal = fixture.core.active_principal().await.unwrap();
    let request = AddScheduleRequest {
        creator_id: CREATOR.to_string(),
        preset_id: PROMPT_PRESET.to_string(),
        seed: None,
        label: Some(label.to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        // `memory-augmented` renders `preset.input.*` in its first states, so
        // the frozen input must carry the keys its graph reads.
        input: Some(json!({ "keyword": "retained", "topic": "retained-topic" })),
        force_gates: false,
        reason: None,
        agent_bindings: Some(default_bindings()),
    };
    fixture
        .handle
        .add_schedule(&principal, request)
        .await
        .expect("schedule insert")
        .schedule_id
}

/// Wait (bounded) until the parked Host has recorded at least one prompt —
/// the drive runs in a spawned task, so the first prompt is asynchronous.
async fn wait_for_prompt(host: &ParkedHost) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while host.prompt_count() == 0 {
        assert!(
            std::time::Instant::now() < deadline,
            "the admitted run never reached the prompt port"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

async fn schedule_row(pool: &sqlx::SqlitePool, schedule_id: &str) -> (String, Option<String>) {
    sqlx::query_as::<_, (String, Option<String>)>(
        "SELECT status, current_session_id FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(schedule_id)
    .fetch_one(pool)
    .await
    .expect("schedule row")
}

// ─────────────────────────────────────────────────────────────────────────────
// Schedule admission
// ─────────────────────────────────────────────────────────────────────────────

/// MIGRATED from `apps/nexus42/tests/workflow_execution_cli.rs`
/// `admission_distinct_serial_rows_race_one_owned`.
///
/// Two DISTINCT serial schedules of one creator race for immediate admission:
/// both preflights see an empty running set, and the store's in-transaction
/// matrix recheck lets exactly ONE claim. The loser stays `pending` and
/// unowned, and one session row exists for the winner.
///
/// Determinism: the winner's first prompt parks (see [`ParkedHost`]), so the
/// race is between the two claims — a winner that settled terminal would
/// legitimately free the serial slot before the loser re-read the matrix.
#[tokio::test]
#[serial_test::serial]
async fn admission_distinct_serial_rows_race_one_owned() {
    let fixture = owner_fixture().await;
    let first = add_pending_schedule(&fixture, "retained-race-a").await;
    let second = add_pending_schedule(&fixture, "retained-race-b").await;

    let coordinator = fixture.coordinator();
    let pool = fixture.pool();
    let caps = fixture.handle.capability_holder();
    let home = fixture.nexus_home();
    let executor =
        fixture.executor.clone() as Arc<dyn nexus_orchestration::capability::PromptExecutor>;

    let (a, b) = tokio::join!(
        coordinator.admit_schedule(
            &first,
            pool.as_ref(),
            &home,
            &caps,
            None,
            Some(executor.clone())
        ),
        coordinator.admit_schedule(&second, pool.as_ref(), &home, &caps, None, Some(executor)),
    );

    let wins = usize::from(a.is_ok()) + usize::from(b.is_ok());
    assert_eq!(
        wins, 1,
        "exactly one of the two racing admissions must succeed: {a:?} / {b:?}"
    );
    let loser = if a.is_ok() {
        b.unwrap_err()
    } else {
        a.unwrap_err()
    };
    assert!(
        matches!(loser, RunControlError::NotEligible(..)),
        "the serial loser must be refused as not eligible, got {loser:?}"
    );

    let (first_status, first_session) = schedule_row(pool.as_ref(), &first).await;
    let (second_status, second_session) = schedule_row(pool.as_ref(), &second).await;
    let owned = [
        first_session.as_deref().is_some_and(|s| !s.is_empty()),
        second_session.as_deref().is_some_and(|s| !s.is_empty()),
    ];
    assert_eq!(
        usize::from(owned[0]) + usize::from(owned[1]),
        1,
        "exactly one distinct serial row must own a run: {first_session:?} / {second_session:?}"
    );

    let (loser_id, loser_status, loser_session) = if owned[0] {
        (&second, &second_status, &second_session)
    } else {
        (&first, &first_status, &first_session)
    };
    assert_eq!(
        loser_status, "pending",
        "the serial loser {loser_id} must stay pending"
    );
    assert!(
        loser_session.is_none(),
        "the serial loser {loser_id} must stay unowned"
    );

    let winner_session = if owned[0] {
        first_session.unwrap()
    } else {
        second_session.unwrap()
    };
    let sessions: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?")
            .bind(&winner_session)
            .fetch_one(pool.as_ref())
            .await
            .unwrap();
    assert_eq!(sessions, 1, "the winner must own exactly one session row");
    // The winner's drive must reach the REAL prompt port (the parked Host),
    // which is what keeps it in flight for the duration of the race.
    wait_for_prompt(&fixture.host).await;
    assert!(
        fixture.host.prompt_count() >= 1,
        "the winner's drive must reach the real prompt port"
    );

    fixture.host.release_all();
    fixture.handle.close().await.expect("owner close");
    fixture.core.close().await.unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Workflow recovery
// ─────────────────────────────────────────────────────────────────────────────

/// Converge-join reroute preset (MIGRATED from
/// `apps/nexus42/tests/ops_e2e_converge_timeout.rs::TIMEOUT_REROUTE_YAML`):
/// `branch_b` is the hanging upstream edge, so the join never reaches 2/2
/// arrivals and its deadline reroutes to `fallback`.
const TIMEOUT_REROUTE_YAML: &str = r#"
preset:
  id: retained-converge-timeout-reroute
  version: 1
  kind: creator
  description: "retained converge join + timeout_ms reroutes via on_timeout"
  requires_capabilities: []
  initial: start
  terminal: done
states:
    - id: start
      next: branch_a
    - id: branch_a
      next:
        branches: []
        default: join
    - id: branch_b
      description: "Hanging upstream edge — never walked in this run, never arrives"
      next: join
    - id: join
      converge: { strategy: wait_for_all }
      timeout_ms: 120
      on_timeout: fallback
      next: done
    - id: fallback
      next: done
    - id: done
      terminal: true
"#;

/// Join deadline (ms) + how long the test parks past it before re-driving.
const JOIN_TIMEOUT_MS: u64 = 120;
const PAST_DEADLINE_SLEEP: std::time::Duration = std::time::Duration::from_millis(500);

/// Extract the `elapsed_ms=` payload from a `_join_timeout_note`.
fn elapsed_ms_from_note(note: &str) -> u64 {
    note.split("elapsed_ms=")
        .nth(1)
        .and_then(|s| s.split(')').next())
        .and_then(|s| {
            s.chars()
                .take_while(char::is_ascii_digit)
                .collect::<String>()
                .parse()
                .ok()
        })
        .unwrap_or_else(|| panic!("note must carry a numeric elapsed_ms: {note}"))
}

/// Load a raw-YAML preset and freeze an embedded source identity over its
/// manifest, exactly as the embedded loader does (a v1 run requires one).
fn load_frozen_preset(yaml: &str, caps: &Arc<CapabilityRegistry>) -> nexus_preset::LoadedPreset {
    let mut loaded = nexus_preset::load_preset_from_str(yaml, caps)
        .unwrap_or_else(|e| panic!("test preset must load: {e}"));
    loaded.source_identity = Some(
        nexus_preset::loader::preset_source_identity(
            &loaded.manifest,
            None,
            Some(loaded.id.as_str()),
        )
        .unwrap_or_else(|e| panic!("test preset must resolve a source identity: {e}")),
    );
    loaded
}

/// MIGRATED from `apps/nexus42/tests/ops_e2e_converge_timeout.rs`
/// `hanging_upstream_with_timeout_reroutes_via_on_timeout`.
///
/// A hanging upstream parks the converge join at 1/2 arrivals; once the
/// deadline has passed the next drive re-steps the join, which reroutes to
/// `fallback` (writing `_join_timeout_note`) and completes — the deadline
/// routes deterministically instead of waiting forever.
#[tokio::test]
async fn hanging_upstream_with_timeout_reroutes_via_on_timeout() {
    let tmp = tempfile::tempdir().unwrap();
    let guarded = nexus_local_db::init_engine_pool(&tmp.path().join("state.db"))
        .await
        .expect("engine pool");
    let pool = Arc::new(guarded.clone_pool());
    let storage: Arc<dyn graph_flow::SessionStorage> =
        Arc::new(SqliteSessionStorage::new(pool.clone()));
    let caps = Arc::new(CapabilityRegistry::with_builtins());
    let engine = GraphFlowEngine::new_with_storage(
        storage.clone(),
        CapabilityRegistryHolder::with_registry(Arc::clone(&caps)),
    );

    let loaded = load_frozen_preset(TIMEOUT_REROUTE_YAML, &caps);
    let session = engine
        .start_session_with_preset_for_creator(&loaded, CREATOR)
        .await
        .expect("preset session starts");

    let first = drive_preset_run(
        &engine,
        Some(&storage),
        None,
        &session,
        &PresetRunConfig::default(),
        None,
    )
    .await;
    assert_eq!(
        first,
        PresetRunOutcome::WaitingForInput { steps: 3 },
        "the join waits for the never-arriving upstream (start -> branch_a -> join)"
    );

    tokio::time::sleep(PAST_DEADLINE_SLEEP).await;
    let second = drive_preset_run(
        &engine,
        Some(&storage),
        None,
        &session,
        &PresetRunConfig {
            resume_waiting: true,
            ..PresetRunConfig::default()
        },
        None,
    )
    .await;
    assert_eq!(
        second,
        PresetRunOutcome::Completed { steps: 3 },
        "reroute: join -> fallback -> done = 3 steps on the second drive"
    );

    let ctx = engine.get_context(&session).await.expect("context");
    let note = ctx
        .get::<String>("_join_timeout_note")
        .expect("reroute must write _join_timeout_note");
    assert!(
        note.contains("join timeout at 'join'"),
        "note names the join state: {note}"
    );
    assert!(
        note.contains("rerouting to 'fallback'"),
        "note names the reroute target: {note}"
    );
    assert!(
        note.contains("gate=converge"),
        "note names the gate: {note}"
    );
    let elapsed = elapsed_ms_from_note(&note);
    assert!(
        elapsed >= JOIN_TIMEOUT_MS,
        "deadline elapsed must exceed the join timeout: {note}"
    );
    assert!(
        ctx.get::<std::collections::HashSet<String>>("_converge_arrivals_join")
            .is_none(),
        "reroute clears the converge arrivals key"
    );
    assert!(
        ctx.get::<u64>("_join_wait_start_join").is_none(),
        "reroute clears the wait-start key"
    );
    assert_eq!(
        engine.get_status(&session).await.unwrap(),
        SessionStatus::Completed
    );
}

/// Converge chain with instrumented `host_tool` edges on `start` and
/// `branch_a` (MIGRATED from
/// `apps/nexus42/tests/ops_e2e_resume_restart.rs::RESUME_REROUTE_YAML`): the
/// edge counter proves completed edges are never re-executed across a restart.
const RESUME_REROUTE_YAML: &str = r#"
preset:
  id: retained-resume-reroute
  version: 1
  kind: creator
  description: "retained resume proof — converge join with instrumented host-tool edges"
  requires_capabilities: []
  initial: start
  terminal: done
states:
    - id: start
      enter:
        - kind: host_tool
          tool_name: test.instrument
          args: { edge: start }
      next: branch_a
    - id: branch_a
      enter:
        - kind: host_tool
          tool_name: test.instrument
          args: { edge: branch_a }
      next:
        branches: []
        default: join
    - id: branch_b
      description: "Hanging upstream edge — never walked, never arrives"
      next: join
    - id: join
      converge: { strategy: wait_for_all }
      timeout_ms: 120
      on_timeout: fallback
      next: done
    - id: fallback
      next: done
    - id: done
      terminal: true
"#;

/// How long the “downtime” parks past the join deadline.
const DOWNTIME_MS: u64 = 500;
const DOWNTIME: std::time::Duration = std::time::Duration::from_millis(DOWNTIME_MS);

/// Counting tool dispatch: every `dispatch_tool` call increments a shared
/// counter — the "completed edges must not re-fire" instrumentation.
#[derive(Clone)]
struct CountingDispatch {
    calls: Arc<AtomicUsize>,
}

#[async_trait]
impl DaemonToolDispatch for CountingDispatch {
    async fn dispatch_tool(
        &self,
        _tool_name: &str,
        _args: &Value,
        _request_id: &str,
    ) -> Result<Value, CapabilityError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(json!({ "ok": true }))
    }
}

/// MIGRATED from `apps/nexus42/tests/ops_e2e_resume_restart.rs`
/// `restart_mid_chain_resumes_without_re_executing_completed_edges`.
///
/// Kill/restart mid-chain: the resume re-drive continues from the persisted
/// position, the completed instrumented edges do NOT re-fire, and the join
/// wait-start is compared against the wall clock (elapsed includes the
/// downtime — no re-baseline, so the deadline still fires on the first
/// re-step).
#[tokio::test]
#[allow(clippy::too_many_lines)]
async fn restart_mid_chain_resumes_without_re_executing_completed_edges() {
    let tmp = tempfile::tempdir().unwrap();
    let guarded = nexus_local_db::init_engine_pool(&tmp.path().join("state.db"))
        .await
        .expect("engine pool");
    let pool = Arc::new(guarded.clone_pool());
    let storage: Arc<dyn graph_flow::SessionStorage> =
        Arc::new(SqliteSessionStorage::new(pool.clone()));
    let caps = Arc::new(CapabilityRegistry::with_builtins());
    let dispatch = Arc::new(CountingDispatch {
        calls: Arc::new(AtomicUsize::new(0)),
    });
    let loaded = load_frozen_preset(RESUME_REROUTE_YAML, &caps);

    // Phase 1: drive the converge chain to the parked join, then kill the
    // in-memory engine (the sqlite checkpoint persists).
    let (engine1, session) = {
        let mut engine = GraphFlowEngine::new_with_storage(
            storage.clone(),
            CapabilityRegistryHolder::with_registry(Arc::clone(&caps)),
        );
        engine.set_daemon_tool_dispatch(dispatch.clone());
        let session = engine
            .start_session_with_preset_for_creator(&loaded, CREATOR)
            .await
            .expect("preset session starts");
        let first = drive_preset_run(
            &engine,
            Some(&storage),
            None,
            &session,
            &PresetRunConfig::default(),
            None,
        )
        .await;
        assert_eq!(
            first,
            PresetRunOutcome::WaitingForInput { steps: 3 },
            "start -> branch_a -> join parks at 1/2 arrivals"
        );
        (engine, session)
    };
    assert_eq!(
        dispatch.calls.load(Ordering::SeqCst),
        2,
        "start + branch_a host-tool edges fired exactly once before the kill"
    );
    drop(engine1);

    tokio::time::sleep(DOWNTIME).await;

    // Phase 2: a fresh engine over the SAME storage; reconstruct the runner
    // for the existing session (mirrors boot `recover_sessions` →
    // `reconstruct_runner` for a non-embedded preset).
    let (engine2, _storage2) = {
        let mut engine = GraphFlowEngine::new_with_storage(
            storage.clone(),
            CapabilityRegistryHolder::with_registry(Arc::clone(&caps)),
        );
        engine.set_daemon_tool_dispatch(dispatch.clone());
        (Arc::new(engine), storage.clone())
    };
    let engine_ref: Arc<dyn OrchestrationEngine> = engine2.clone();
    let wired = build_wired_outer_graph(
        &loaded,
        &engine_ref,
        &caps,
        Some(dispatch.clone()),
        None,
        engine2.shared_state().session_cancels.clone(),
    )
    .expect("wired outer graph builds");
    let runner = Arc::new(graph_flow::FlowRunner::new(
        Arc::new(wired),
        storage.clone(),
    ));
    engine2
        .shared_state()
        .runners
        .write()
        .await
        .insert(session.0.clone(), runner);
    let summary = SessionSummary {
        session_id: session.clone(),
        creator_id: CREATOR.to_string(),
        preset_id: loaded.id.clone(),
        status: SessionStatus::WaitingForInput,
        current_task_id: Some("join".to_string()),
    };
    engine2
        .shared_state()
        .sessions
        .write()
        .await
        .push(summary.clone());

    let decisions = resume_driven_sessions(
        engine2.as_ref(),
        &storage,
        None,
        &[summary],
        &PresetRunConfig {
            resume_waiting: true,
            ..PresetRunConfig::default()
        },
        None,
    )
    .await;
    assert_eq!(decisions.len(), 1, "exactly one recovered session");
    match &decisions[0] {
        ResumeDecision::ReDriven {
            session_id,
            outcome,
        } => {
            assert_eq!(session_id, &session);
            assert_eq!(
                outcome,
                &PresetRunOutcome::Completed { steps: 3 },
                "resume re-drives join -> fallback -> done from the persisted position"
            );
        }
        other => panic!("expected ReDriven, got {other:?}"),
    }

    assert_eq!(
        dispatch.calls.load(Ordering::SeqCst),
        2,
        "completed edges must not re-execute across kill/restart"
    );
    let ctx = engine2.get_context(&session).await.expect("context");
    let note = ctx
        .get::<String>("_join_timeout_note")
        .expect("reroute must write _join_timeout_note");
    let elapsed = elapsed_ms_from_note(&note);
    assert!(
        elapsed >= DOWNTIME_MS,
        "elapsed must include the downtime (no re-baseline): {note}"
    );
    assert_eq!(
        engine2.get_status(&session).await.unwrap(),
        SessionStatus::Completed
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Cron / chronology background ticks
// ─────────────────────────────────────────────────────────────────────────────

/// An engine-admitted pool over a fresh temp DB plus a seeded Work.
async fn work_pool() -> (Arc<sqlx::SqlitePool>, TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let pool = nexus_local_db::init_engine_pool(&dir.path().join("state.db"))
        .await
        .expect("engine-admitted pool")
        .clone_pool();
    (Arc::new(pool), dir)
}

fn work_record(
    work_id: &str,
    work_ref: &str,
    preset_id: &str,
    stage: &str,
    stage_status: &str,
) -> WorkRecord {
    WorkRecord {
        work_id: work_id.to_string(),
        creator_id: CREATOR.to_string(),
        workspace_slug: "ws".to_string(),
        status: "active".to_string(),
        title: "Retained Execution Work".to_string(),
        long_term_goal: "Cover the retained execution contracts".to_string(),
        initial_idea: "An idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: None,
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: preset_id.to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-18T10:00:00Z".to_string(),
        updated_at: "2026-06-18T10:00:00Z".to_string(),
        current_stage: stage.to_string(),
        stage_status: stage_status.to_string(),
        work_profile: Some("novel".to_string()),
        work_ref: Some(work_ref.to_string()),
        total_planned_chapters: Some(3),
        current_chapter: 0,
        auto_chain_enabled: true,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    }
}

/// A per-Work cron blob whose `brainstorm` role fires every minute.
fn every_minute_cron() -> String {
    json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": {"cron": "* * * * *", "enabled": true},
            "write": {"cron": "0 4 * * *", "enabled": false}
        }
    })
    .to_string()
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/cron_supervisor_task.rs`
/// `run_one_tick_enqueues_and_admits`.
///
/// One cron tick enqueues the matching role fire AND admits it to `running` —
/// the periodic integration path the daemon boot used to own.
#[tokio::test]
#[serial_test::serial]
async fn cron_tick_enqueues_and_admits_a_due_role() {
    let (pool, _dir) = work_pool().await;
    let work = work_record(
        "wrk_cron_due",
        "cron-due",
        "novel-writing",
        "intake",
        "complete",
    );
    nexus_local_db::works::create_work(pool.as_ref(), &work)
        .await
        .unwrap();
    nexus_local_db::works::set_schedule_json(
        pool.as_ref(),
        "wrk_cron_due",
        &every_minute_cron(),
        "2026-06-18T10:00:00Z",
    )
    .await
    .unwrap();

    let supervisor =
        Arc::new(nexus_orchestration::schedule::supervisor::ScheduleSupervisor::new(pool.clone()));
    let workspace = tempfile::tempdir().unwrap();
    cron::run_one_tick(pool.as_ref(), workspace.path(), &supervisor, Some(PROVIDER)).await;

    let status: String = sqlx::query_scalar(
        "SELECT status FROM creator_schedules \
         WHERE work_id = 'wrk_cron_due' AND preset_id = 'novel-brainstorm'",
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("cron tick enqueued the brainstorm role");
    assert_eq!(
        status, "running",
        "one tick must enqueue AND admit the cron-fired schedule"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/cron_supervisor_task.rs`
/// `run_one_tick_no_match_is_noop`.
///
/// A tick whose cron expressions can never match the current minute enqueues
/// nothing.
#[tokio::test]
#[serial_test::serial]
async fn cron_tick_with_no_due_role_is_a_no_op() {
    let (pool, _dir) = work_pool().await;
    let work = work_record(
        "wrk_cron_idle",
        "cron-idle",
        "novel-writing",
        "intake",
        "complete",
    );
    nexus_local_db::works::create_work(pool.as_ref(), &work)
        .await
        .unwrap();
    // Seven-field expressions pinned to year 2000 can never match a current tick.
    let idle = json!({
        "tz": "UTC",
        "roles": {
            "brainstorm": {"cron": "0 0 3 * * * 2000", "enabled": true},
            "write": {"cron": "0 0 4 * * * 2000", "enabled": true}
        }
    })
    .to_string();
    nexus_local_db::works::set_schedule_json(
        pool.as_ref(),
        "wrk_cron_idle",
        &idle,
        "2026-06-18T10:00:00Z",
    )
    .await
    .unwrap();

    let supervisor =
        Arc::new(nexus_orchestration::schedule::supervisor::ScheduleSupervisor::new(pool.clone()));
    let workspace = tempfile::tempdir().unwrap();
    cron::run_one_tick(pool.as_ref(), workspace.path(), &supervisor, Some(PROVIDER)).await;

    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules WHERE work_id = 'wrk_cron_idle'",
    )
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(count, 0, "a non-matching cron must not enqueue");
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/cron_lock_integration.rs`
/// `file_lock_blocks_cron_fire_when_held` and
/// `cron_fires_without_workspace_dir_gracefully_skips_file_lock`.
///
/// The per-Work file lock is the writer barrier the cron fire respects: with
/// no `Works/<ref>` directory the fire is not gated (no lock path), and once
/// the directory exists and a holder owns the lock the fire is skipped and
/// counted as gated.
#[tokio::test]
#[serial_test::serial]
async fn cron_fire_is_gated_while_the_works_file_lock_is_held() {
    use chrono::TimeZone;

    let (pool, _dir) = work_pool().await;
    // Two distinct Works: each role fires at most once per active schedule
    // (the idempotency guard), so the lock case needs its own Work.
    for (work_id, work_ref) in [
        ("wrk_cron_open", "cron-open"),
        ("wrk_cron_held", "cron-held"),
    ] {
        let work = work_record(work_id, work_ref, "novel-writing", "intake", "complete");
        nexus_local_db::works::create_work(pool.as_ref(), &work)
            .await
            .unwrap();
        nexus_local_db::works::set_schedule_json(
            pool.as_ref(),
            work_id,
            &every_minute_cron(),
            "2026-06-18T10:00:00Z",
        )
        .await
        .unwrap();
    }

    let workspace = tempfile::tempdir().unwrap();
    let minute = chrono::Utc
        .with_ymd_and_hms(2026, 6, 19, 3, 7, 0)
        .single()
        .expect("valid fixed UTC minute");

    // `wrk_cron_held` owns a Works/<ref> directory with a live file-lock
    // holder; `wrk_cron_open` has none. One evaluation must then firewall the
    // held Work and still fire the open one.
    let work_dir = workspace.path().join("Works").join("cron-held");
    std::fs::create_dir_all(&work_dir).unwrap();
    let guard = nexus_local_db::file_lock::try_acquire(&work_dir, "cli:retained-holder")
        .expect("acquire the per-Work file lock");

    let fenced = nexus_orchestration::schedule::cron_supervisor::evaluate_cron_fires(
        pool.as_ref(),
        Some(workspace.path()),
        minute,
        Some(PROVIDER),
    )
    .await;
    assert_eq!(
        fenced.fired, 1,
        "only the Work without a Works/ lock path may fire: {fenced:?}"
    );
    assert_eq!(
        fenced.skipped_gated, 1,
        "the held lock must gate exactly one fire: {fenced:?}"
    );
    let held: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules WHERE work_id = 'wrk_cron_held'",
    )
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(held, 0, "the gated Work must not be enqueued");
    let opened: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules WHERE work_id = 'wrk_cron_open'",
    )
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(opened, 1, "the ungated Work must be enqueued exactly once");

    // Releasing the holder restores the fire: the gate is the lock, not the
    // Work's cron shape.
    drop(guard);
    let released = nexus_orchestration::schedule::cron_supervisor::evaluate_cron_fires(
        pool.as_ref(),
        Some(workspace.path()),
        minute,
        Some(PROVIDER),
    )
    .await;
    assert_eq!(
        released.fired, 1,
        "the released Work must fire: {released:?}"
    );
    assert_eq!(
        released.skipped_gated, 0,
        "no fire may be gated once the holder is gone: {released:?}"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/auto_chronology_task.rs`
/// `daemon_run_one_tick_advances_eligible_work`.
///
/// One auto-chronology tick advances an opted-in Work whose volume-1 chapters
/// are all finalized, writing the volume-2 outline under the workspace root.
#[tokio::test]
#[serial_test::serial]
async fn auto_chronology_tick_advances_an_opted_in_work() {
    let (pool, _dir) = work_pool().await;
    let workspace = tempfile::tempdir().unwrap();
    let work = work_record(
        "wrk_chronology",
        "chrono-advance",
        "novel-writing",
        "produce",
        "complete",
    );
    nexus_local_db::works::create_work(pool.as_ref(), &work)
        .await
        .unwrap();
    for chapter in 1..=2 {
        let slug = format!("v01-ch{chapter:02}");
        nexus_local_db::work_chapters::insert_chapter(
            pool.as_ref(),
            &nexus_local_db::work_chapters::InsertChapterParams {
                work_id: "wrk_chronology",
                chapter,
                volume: Some(1),
                slug: Some(&slug),
                planned_word_count: 4000,
                outline_path: None,
                body_path: None,
                now: "2026-06-18T10:00:00Z",
            },
        )
        .await
        .unwrap();
        nexus_local_db::work_chapters::update_status(
            pool.as_ref(),
            "wrk_chronology",
            chapter,
            1,
            "finalized",
            Some(4000),
            "2026-06-18T10:30:00Z",
        )
        .await
        .unwrap();
    }
    nexus_local_db::works::set_auto_chronology(
        pool.as_ref(),
        "wrk_chronology",
        true,
        "2026-06-18T10:00:00Z",
    )
    .await
    .unwrap();

    chronology_run_one_tick(pool.as_ref(), Some(workspace.path())).await;

    let outline = workspace
        .path()
        .join("Works")
        .join("chrono-advance")
        .join("Outlines")
        .join("volume-2-outline.md");
    assert!(
        outline.exists(),
        "the auto-chronology tick must write the volume-2 outline"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/auto_chronology_task.rs`
/// `parse_interval_secs_handles_env_values` and `from_env_uses_default_when_unset`.
///
/// The interval parser is a total function over the optional override (no
/// process-global env mutation), and the production constructor stays wired to
/// it.
#[test]
fn auto_chronology_interval_parsing_is_total() {
    assert_eq!(
        parse_interval_secs(None),
        DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS,
        "the default is the documented interval"
    );
    assert_eq!(parse_interval_secs(Some("1")), 60, "1 minute = 60 seconds");
    assert_eq!(
        parse_interval_secs(Some("garbage")),
        DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS,
        "an invalid override falls back to the default"
    );
    assert_eq!(
        parse_interval_secs(Some("0")),
        DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS,
        "a zero interval (busy loop) falls back to the default"
    );

    // The constructor stays wired to the pure parser: whatever the ambient
    // override is (unset, valid, or invalid), `from_env` must agree with
    // `parse_interval_secs` applied to that SAME value — an invalid ambient
    // setting therefore resolves to the default instead of being accepted
    // unchecked. The process-global variable is deliberately NOT mutated here
    // (the module documents env mutation as unsafe under parallel test
    // execution).
    let ambient = std::env::var(ENV_AUTO_CHRONOLOGY_INTERVAL_MIN).ok();
    let config = AutoChronologyConfig::from_env();
    assert_eq!(
        config.interval.as_secs(),
        parse_interval_secs(ambient.as_deref()),
        "from_env must resolve through parse_interval_secs (ambient {ambient:?})"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Master-decision timeout sweep
// ─────────────────────────────────────────────────────────────────────────────

fn finding(finding_id: &str, work_id: &str, age_seconds: i64) -> Finding {
    let now = chrono::Utc::now().timestamp();
    Finding {
        finding_id: finding_id.to_string(),
        work_id: work_id.to_string(),
        chapter: None,
        severity: "major".to_string(),
        status: "open".to_string(),
        title: "Retained stale finding".to_string(),
        description: "Past the master-decision SLA".to_string(),
        target_executor: "master".to_string(),
        creator_id: CREATOR.to_string(),
        kind: "craft".to_string(),
        rule_suggestion: None,
        created_at: now - age_seconds,
        updated_at: now - age_seconds,
    }
}

async fn review_master_schedule_count(pool: &sqlx::SqlitePool, work_id: &str) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules \
         WHERE preset_id = 'novel-review-master' AND work_id = ?",
    )
    .bind(work_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/master_decision_timeout.rs`
/// `sweep_with_no_findings_is_a_no_op`.
#[tokio::test]
#[serial_test::serial]
async fn stale_finding_sweep_is_a_no_op_without_findings() {
    let (pool, _dir) = work_pool().await;
    run_one_sweep(pool.as_ref(), 60, Some(PROVIDER), None).await;

    let total: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM creator_schedules")
        .fetch_one(pool.as_ref())
        .await
        .unwrap();
    assert_eq!(total, 0, "an empty findings table enqueues nothing");
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/master_decision_timeout.rs`
/// `stale_finding_without_optin_does_not_enqueue`,
/// `stale_finding_with_optin_enqueues_review_master` and
/// `mixed_optin_only_enqueues_for_opted_in_work`.
///
/// The review-master enqueue is strictly opt-in per Work: exactly one
/// `pending` `novel-review-master` row with an `RVM`-prefixed id for the
/// opted-in Work, and none for the default-one.
#[tokio::test]
#[serial_test::serial]
async fn stale_finding_sweep_enqueues_only_for_an_opted_in_work() {
    let (pool, _dir) = work_pool().await;
    let opted_in = work_record(
        "wrk_sweep_yes",
        "sweep-yes",
        "novel-writing",
        "research",
        "active",
    );
    let mut default_off = work_record(
        "wrk_sweep_no",
        "sweep-no",
        "novel-writing",
        "research",
        "active",
    );
    default_off.auto_review_master_on_timeout = false;
    let mut opted_in = opted_in;
    opted_in.auto_review_master_on_timeout = true;
    let _ = create_work_atomic(pool.as_ref(), &opted_in, None)
        .await
        .unwrap();
    let _ = create_work_atomic(pool.as_ref(), &default_off, None)
        .await
        .unwrap();
    create_finding(
        pool.as_ref(),
        &finding("fnd_sweep_yes", "wrk_sweep_yes", 7200),
    )
    .await
    .unwrap();
    create_finding(
        pool.as_ref(),
        &finding("fnd_sweep_no", "wrk_sweep_no", 7200),
    )
    .await
    .unwrap();

    run_one_sweep(pool.as_ref(), 60, Some(PROVIDER), None).await;

    assert_eq!(
        review_master_schedule_count(pool.as_ref(), "wrk_sweep_no").await,
        0,
        "a Work without the opt-in flag must get no schedule"
    );
    assert_eq!(
        review_master_schedule_count(pool.as_ref(), "wrk_sweep_yes").await,
        1,
        "an opted-in stale finding enqueues exactly one schedule"
    );
    let (preset_id, status, schedule_id): (String, String, String) = sqlx::query_as(
        "SELECT preset_id, status, schedule_id FROM creator_schedules WHERE work_id = ?",
    )
    .bind("wrk_sweep_yes")
    .fetch_one(pool.as_ref())
    .await
    .unwrap();
    assert_eq!(preset_id, "novel-review-master");
    assert_eq!(status, "pending");
    assert!(
        schedule_id.starts_with("RVM"),
        "the review-master schedule id keeps its prefix: {schedule_id}"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/master_decision_timeout.rs`
/// `fresh_finding_does_not_enqueue_even_when_opted_in` and
/// `resolved_finding_is_not_stale`.
///
/// The staleness filter is age AND status: a finding younger than the
/// threshold, or one already resolved, never enqueues.
#[tokio::test]
#[serial_test::serial]
async fn stale_finding_sweep_ignores_fresh_and_resolved_findings() {
    let (pool, _dir) = work_pool().await;
    let mut work = work_record(
        "wrk_sweep_filter",
        "sweep-filter",
        "novel-writing",
        "research",
        "active",
    );
    work.auto_review_master_on_timeout = true;
    let _ = create_work_atomic(pool.as_ref(), &work, None)
        .await
        .unwrap();

    // Fresh (5s against a 60s threshold).
    create_finding(
        pool.as_ref(),
        &finding("fnd_sweep_fresh", "wrk_sweep_filter", 5),
    )
    .await
    .unwrap();
    run_one_sweep(pool.as_ref(), 60, Some(PROVIDER), None).await;
    assert_eq!(
        review_master_schedule_count(pool.as_ref(), "wrk_sweep_filter").await,
        0,
        "a fresh finding must not enqueue"
    );

    // Stale but resolved.
    create_finding(
        pool.as_ref(),
        &finding("fnd_sweep_resolved", "wrk_sweep_filter", 7200),
    )
    .await
    .unwrap();
    sqlx::query("UPDATE findings SET status = 'resolved' WHERE finding_id = ?")
        .bind("fnd_sweep_resolved")
        .execute(pool.as_ref())
        .await
        .unwrap();
    run_one_sweep(pool.as_ref(), 60, Some(PROVIDER), None).await;
    assert_eq!(
        review_master_schedule_count(pool.as_ref(), "wrk_sweep_filter").await,
        0,
        "a resolved finding is not stale and must not enqueue"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/master_decision_timeout.rs`
/// `repeated_sweeps_remain_stable`.
///
/// The sweep is best-effort and stable: it never panics on a second pass, and
/// an opt-in that stays stale enqueues one schedule per sweep.
#[tokio::test]
#[serial_test::serial]
async fn stale_finding_sweep_repeats_without_panicking() {
    let (pool, _dir) = work_pool().await;
    let mut work = work_record(
        "wrk_sweep_repeat",
        "sweep-repeat",
        "novel-writing",
        "research",
        "active",
    );
    work.auto_review_master_on_timeout = true;
    let _ = create_work_atomic(pool.as_ref(), &work, None)
        .await
        .unwrap();
    create_finding(
        pool.as_ref(),
        &finding("fnd_sweep_repeat", "wrk_sweep_repeat", 7200),
    )
    .await
    .unwrap();

    run_one_sweep(pool.as_ref(), 60, Some(PROVIDER), None).await;
    run_one_sweep(pool.as_ref(), 60, Some(PROVIDER), None).await;

    assert_eq!(
        review_master_schedule_count(pool.as_ref(), "wrk_sweep_repeat").await,
        2,
        "each sweep on a persistently-stale finding enqueues one schedule"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Writer-lock window (reconcile)
// ─────────────────────────────────────────────────────────────────────────────

fn select_creator(home: &Path) {
    std::fs::write(
        home.join(".nexus42/config.toml"),
        "active_creator_id = \"author\"\n[active_workspace_slug_by_creator]\n\"author\" = \"default\"\n",
    )
    .unwrap();
}

/// A direct-writer core over a fresh guarded workspace, plus a Work whose
/// `story_ref` resolves to a creative root the reconcile walk can read.
struct ReconcileFixture {
    _tmp: TempDir,
    core: CoreService,
    principal: nexus_core::Principal,
    work_id: String,
    work_ref: String,
    creative_root: PathBuf,
}

async fn reconcile_fixture(work_ref: &str) -> ReconcileFixture {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    std::fs::create_dir_all(home.join(".nexus42")).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        home, "author", "default",
    ))
    .unwrap();
    select_creator(home);
    let db = nexus_home_layout::workspace_state_db_path(home, "author", "default");
    {
        let seed = init_guarded_pool(&db, "author").await.unwrap();
        let pool = seed.clone_pool();
        nexus_local_db::creators::ensure_creator_row(&pool, "author", "Author")
            .await
            .unwrap();
        pool.close().await;
    }
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    let world = core
        .create_world(
            &principal,
            serde_json::from_value::<nexus_contracts::CreateWorldRequest>(
                json!({"title": "Reconcile World"}),
            )
            .unwrap(),
        )
        .await
        .unwrap()
        .world_id;
    let work_id = core
        .create_work(
            &principal,
            serde_json::from_value::<nexus_contracts::CreateWorkRequest>(json!({
                "title": "Reconcile Work",
                "long_term_goal": "prove the lock window",
                "initial_idea": "idea",
                "world_id": world,
            }))
            .unwrap(),
        )
        .await
        .unwrap()
        .work_id;
    core.patch_work(
        &principal,
        work_id.clone(),
        "core",
        nexus_core::WorkPatchRequest {
            story_ref: Some(Some(work_ref.to_string())),
            ..Default::default()
        },
    )
    .await
    .unwrap();

    let creative_root = home.join("creative");
    std::fs::create_dir_all(creative_root.join("Works").join(work_ref)).unwrap();
    // The core authority resolves the workspace from the operational meta.json.
    std::fs::write(
        nexus_home_layout::operational_workspace_dir(home, "author", "default").join("meta.json"),
        serde_json::to_vec(&json!({"local_root": creative_root})).unwrap(),
    )
    .unwrap();

    ReconcileFixture {
        _tmp: tmp,
        core,
        principal,
        work_id,
        work_ref: work_ref.to_string(),
        creative_root,
    }
}

impl ReconcileFixture {
    fn stories_dir(&self) -> PathBuf {
        self.creative_root
            .join("Works")
            .join(&self.work_ref)
            .join("Stories")
    }

    async fn lock_holder(&self) -> Option<String> {
        self.core
            .get_work(&self.principal, self.work_id.clone())
            .await
            .unwrap()
            .runtime_lock_holder
    }
}

fn write_chapter_file(path: &Path, body: &str) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, body).unwrap();
}

fn set_mode(path: &Path, mode: u32) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .expect("set directory permissions");
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
    }
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/runtime_lock.rs`
/// `test_reconcile_chapters_releases_lock_on_error`.
///
/// The reconcile write phase holds the runtime lock, so an apply-phase failure
/// must still release it on the way out — a failed reconcile can never leave
/// the Work locked.
#[tokio::test]
#[serial_test::serial]
async fn reconcile_apply_error_releases_the_runtime_lock() {
    let fixture = reconcile_fixture("reconcile-lock-error").await;
    let stories = fixture.stories_dir();
    write_chapter_file(
        &stories.join("ch01-intro.md"),
        "---\nchapter: 1\nstatus: finalized\n---\nBody",
    );
    // A DB row whose status conflicts with the frontmatter forces a
    // `ResyncFileStatus` op, so the write phase really rewrites the file.
    nexus_local_db::insert_chapter(
        fixture.core.pool(),
        &nexus_local_db::InsertChapterParams {
            work_id: &fixture.work_id,
            chapter: 1,
            volume: Some(1),
            slug: None,
            planned_word_count: 4000,
            outline_path: None,
            body_path: Some("Works/reconcile-lock-error/Stories/ch01-intro.md"),
            now: "2026-06-17T00:00:00Z",
        },
    )
    .await
    .unwrap();

    // Read-only Stories/ lets the read phase succeed and the atomic frontmatter
    // rewrite fail (Unix-only hermetic trigger).
    set_mode(&stories, 0o555);
    let result = fixture
        .core
        .reconcile_work_chapters(
            &fixture.principal,
            fixture.work_id.clone(),
            "core",
            nexus_core::ReconcileDryRunQuery { dry_run: None },
        )
        .await;
    set_mode(&stories, 0o755);

    #[cfg(unix)]
    assert!(
        result.is_err(),
        "the apply phase must fail against a read-only Stories/ directory"
    );
    assert_eq!(
        fixture.lock_holder().await,
        None,
        "a failed reconcile must release the runtime lock"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/runtime_lock.rs`
/// `test_reconcile_chapters_read_phase_runs_unlocked`.
///
/// The read phase (the slow filesystem walk) runs BEFORE the lock is acquired,
/// so its failure never acquires the lock at all — the lock window excludes
/// the walk.
#[tokio::test]
#[serial_test::serial]
async fn reconcile_read_phase_failure_never_acquires_the_runtime_lock() {
    let fixture = reconcile_fixture("reconcile-read-unlocked").await;
    let stories = fixture.stories_dir();
    write_chapter_file(
        &stories.join("ch01-intro.md"),
        "---\nchapter: 1\nstatus: finalized\n---\nBody",
    );

    set_mode(&stories, 0o000);
    let result = fixture
        .core
        .reconcile_work_chapters(
            &fixture.principal,
            fixture.work_id.clone(),
            "core",
            nexus_core::ReconcileDryRunQuery { dry_run: None },
        )
        .await;
    set_mode(&stories, 0o755);

    #[cfg(unix)]
    assert!(
        result.is_err(),
        "the read phase must fail against an unreadable Stories/ directory"
    );
    assert_eq!(
        fixture.lock_holder().await,
        None,
        "a read-phase failure must never acquire the runtime lock"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/runtime_lock.rs`
/// `test_reconcile_chapters_dry_run_makes_zero_mutations`.
///
/// A dry-run reconcile reports what the mutating path would do while making
/// ZERO filesystem/DB mutations and acquiring NO runtime lock; the following
/// mutating run proves the report was accurate (and releases its own lock).
#[tokio::test]
#[serial_test::serial]
async fn reconcile_dry_run_mutates_nothing_and_takes_no_lock() {
    let fixture = reconcile_fixture("reconcile-dry-run").await;
    let chapter_path = fixture.stories_dir().join("ch01-intro.md");
    let original = "---\nchapter: 1\nstatus: finalized\nword_count: 1234\n---\nBody";
    write_chapter_file(&chapter_path, original);

    let before_rows =
        nexus_local_db::work_chapters::list_chapters(fixture.core.pool(), &fixture.work_id)
            .await
            .unwrap()
            .len();
    assert_eq!(before_rows, 0, "no chapter rows exist before the dry run");

    let report = fixture
        .core
        .reconcile_work_chapters(
            &fixture.principal,
            fixture.work_id.clone(),
            "core",
            nexus_core::ReconcileDryRunQuery {
                dry_run: Some(true),
            },
        )
        .await
        .expect("the dry run must succeed");
    assert_eq!(report.created, 1, "the dry run reports the pending create");
    assert_eq!(report.updated, 0);
    assert_eq!(report.resynced, 0);
    assert_eq!(report.preserved, 0);
    assert_eq!(
        std::fs::read_to_string(&chapter_path).unwrap(),
        original,
        "the dry run must not modify the chapter file"
    );
    let after_rows =
        nexus_local_db::work_chapters::list_chapters(fixture.core.pool(), &fixture.work_id)
            .await
            .unwrap()
            .len();
    assert_eq!(after_rows, 0, "the dry run must not insert a chapter row");
    assert_eq!(
        fixture.lock_holder().await,
        None,
        "the dry run must not acquire the runtime lock"
    );

    let mutated = fixture
        .core
        .reconcile_work_chapters(
            &fixture.principal,
            fixture.work_id.clone(),
            "core",
            nexus_core::ReconcileDryRunQuery { dry_run: None },
        )
        .await
        .expect("the mutating reconcile must succeed");
    assert_eq!(mutated.created, 1, "the mutating path applies the report");
    let mutated_rows =
        nexus_local_db::work_chapters::list_chapters(fixture.core.pool(), &fixture.work_id)
            .await
            .unwrap()
            .len();
    assert_eq!(mutated_rows, 1, "the mutating path inserts exactly one row");
    assert_eq!(
        fixture.lock_holder().await,
        None,
        "the mutating reconcile must release the runtime lock"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Production prompt executor over the Host plane
// ─────────────────────────────────────────────────────────────────────────────

/// Seed an authoritative v1 run over `store` for the production prompt
/// executor, with the real content-addressed source identity of an embedded
/// preset (a fabricated hash would refuse reconstruction instead of
/// exercising the executor).
async fn seed_prompt_run(
    store: &SqliteSessionStorage,
    session_id: &str,
    bindings: HashMap<String, nexus_orchestration::run_state::AgentBinding>,
) {
    let preset = PROMPT_PRESET;
    let source = nexus_preset::embedded_source_identity(preset).expect("embedded source identity");
    let version = nexus_preset::load_embedded_preset(
        preset,
        &nexus_preset::capability_catalog::BuiltinCapabilityCatalog,
    )
    .expect("embedded preset loads")
    .version;
    let descriptor = nexus_orchestration::RunDescriptorV1 {
        creator_id: CREATOR.to_string(),
        work_id: None,
        workspace_root: PathBuf::from("/tmp/retained-ws"),
        preset_id: preset.to_string(),
        preset_version: version,
        source,
        input: serde_json::Map::default(),
        agent_bindings: bindings,
        parent_session_id: None,
        graph_name: None,
    };
    let root = graph_flow::Session::new_from_task(session_id.to_string(), "recall");
    root.context
        .set("_session_id", session_id.to_string())
        .expect("seed session context");
    let checkpoint = nexus_orchestration::RunCheckpoint {
        root: &root,
        children: &[],
    };
    store
        .start_run(
            &nexus_orchestration::SessionId(session_id.to_string()),
            &descriptor,
            checkpoint,
            &nexus_orchestration::RunStateV1::default(),
        )
        .await
        .expect("seed the v1 run");
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/workflow_prompt_execution.rs`
/// `concurrent_same_key_second_prompt_reuses_session_not_spawn` and
/// `missing_binding_refuses_before_effect`.
///
/// The production [`HostPromptExecutor`] over the deterministic Host keeps ONE
/// owned Host session per `(run, role)`: a second prompt for the same key
/// reuses it (never a second subprocess), and a prompt whose frozen run has no
/// binding for the role is refused BEFORE any Host effect.
#[allow(clippy::too_many_lines)] // one prompt-executor journey asserted end to end
#[tokio::test]
async fn prompt_executor_single_flights_one_session_and_refuses_before_effect() {
    use nexus_orchestration::capability::{PromptExecutor, PromptRequest, ToolPolicy};
    use nexus_orchestration::run_state::AgentBinding;

    let tmp = tempfile::tempdir().unwrap();
    let guarded = nexus_local_db::init_engine_pool(&tmp.path().join("state.db"))
        .await
        .expect("engine pool");
    let pool = Arc::new(guarded.clone_pool());
    let store = Arc::new(SqliteSessionStorage::new(pool));
    let host = ParkedHost::new();
    let executor: Arc<dyn PromptExecutor> = Arc::new(HostPromptExecutor::new(
        host.clone(),
        store.clone(),
        TimeoutConfig::default(),
    ));

    let mut bindings = HashMap::new();
    bindings.insert(
        "default".to_string(),
        AgentBinding {
            provider_id: PROVIDER.to_string(),
            model: None,
        },
    );
    seed_prompt_run(store.as_ref(), "retained:single-flight", bindings).await;
    seed_prompt_run(store.as_ref(), "retained:unbound", HashMap::new()).await;

    let request = |run_id: &str, prompt: &str| PromptRequest {
        run_id: run_id.to_string(),
        task_id: "recall".to_string(),
        agent_ref: Some("default".to_string()),
        prompt: prompt.to_string(),
        tool_policy: ToolPolicy::AutoGrantAll,
        cancellation: tokio_util::sync::CancellationToken::new(),
    };

    // First prompt parks inside the Host; the second is submitted while it is
    // in flight (the executor serializes same-run operations).
    let first = {
        let executor = Arc::clone(&executor);
        tokio::spawn(async move {
            executor
                .execute(request("retained:single-flight", "one"))
                .await
        })
    };
    wait_for_prompt(&host).await;
    let second = {
        let executor = Arc::clone(&executor);
        tokio::spawn(async move {
            executor
                .execute(request("retained:single-flight", "two"))
                .await
        })
    };
    host.release_all();

    let first = first.await.unwrap().expect("the first prompt succeeds");
    let second = second.await.unwrap().expect("the second prompt succeeds");
    assert_eq!(
        first.full_text, "transformed:parked-output",
        "the prompt result must carry the Host's transformed output"
    );
    assert_eq!(second.full_text, "transformed:parked-output");
    assert_eq!(
        host.session_create_count(),
        1,
        "the same (run, role) key must reuse one owned Host session"
    );
    assert_eq!(host.prompt_count(), 2, "both prompts must reach the Host");

    // MIGRATED from the same source case's durable cleanup assertion: a
    // successfully completed prompt must not leave its durable `in_flight`
    // attempt marker behind.
    let state = store
        .load_run(&nexus_orchestration::SessionId(
            "retained:single-flight".to_string(),
        ))
        .await
        .expect("load run")
        .expect("record")
        .state
        .expect("v1 state");
    assert!(
        state.in_flight.is_none(),
        "a successfully completed prompt must not leave a durable in_flight marker"
    );

    // A frozen run without a binding for the role refuses before any effect.
    let refused = executor
        .execute(request("retained:unbound", "three"))
        .await
        .expect_err("a missing role binding must refuse");
    assert!(
        matches!(refused, CapabilityError::Forbidden(_)),
        "a missing binding must refuse before any effect, got {refused:?}"
    );
    assert_eq!(
        host.prompt_count(),
        2,
        "a refused prompt must produce no Host effect"
    );
    assert_eq!(
        host.session_create_count(),
        1,
        "a refused prompt must launch no Host session"
    );

    // A pre-cancelled request is fenced before any Host effect.
    let cancelled = request("retained:single-flight", "four");
    cancelled.cancellation.cancel();
    let fenced = executor
        .execute(cancelled)
        .await
        .expect_err("a pre-cancelled prompt must refuse");
    assert!(
        matches!(fenced, CapabilityError::Cancelled),
        "a pre-cancelled prompt must be fenced as Cancelled, got {fenced:?}"
    );
    assert_eq!(
        host.prompt_count(),
        2,
        "a fenced prompt must produce no Host effect"
    );

    // A non-`EndTurn` stop is a typed failure, never partial-output success.
    host.stop_without_end_turn();
    let stopped = executor
        .execute(request("retained:single-flight", "five"))
        .await
        .expect_err("a non-EndTurn stop must fail");
    assert!(
        matches!(&stopped, CapabilityError::TransientExternal(msg) if msg.contains("non-EndTurn")),
        "a non-EndTurn stop must be a typed failure, got {stopped:?}"
    );
    assert_eq!(
        host.prompt_count(),
        3,
        "the stopped prompt did reach the Host"
    );
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/workflow_prompt_execution.rs`
/// `eof_after_initialize_is_typed_failure`.
///
/// A Host whose agent process exits right after the `initialize` handshake
/// never completes its launch: the production executor must surface the typed
/// `TransientExternal` launch failure — never a fake success — and a prompt
/// stream that closes after its deltas without a terminal event is the same
/// typed refusal, never partial-output success.
#[tokio::test]
async fn prompt_executor_types_eof_after_initialize_as_a_launch_failure() {
    use nexus_orchestration::capability::{PromptExecutor, PromptRequest, ToolPolicy};
    use nexus_orchestration::run_state::AgentBinding;

    let tmp = tempfile::tempdir().unwrap();
    let guarded = nexus_local_db::init_engine_pool(&tmp.path().join("state.db"))
        .await
        .expect("engine pool");
    let pool = Arc::new(guarded.clone_pool());
    let store = Arc::new(SqliteSessionStorage::new(pool));
    let bindings = || {
        let mut bindings = HashMap::new();
        bindings.insert(
            "default".to_string(),
            AgentBinding {
                provider_id: PROVIDER.to_string(),
                model: None,
            },
        );
        bindings
    };
    let request = |run_id: &str| PromptRequest {
        run_id: run_id.to_string(),
        task_id: "recall".to_string(),
        agent_ref: Some("default".to_string()),
        prompt: "hello".to_string(),
        tool_policy: ToolPolicy::AutoGrantAll,
        cancellation: tokio_util::sync::CancellationToken::new(),
    };

    // The launch itself fails (the process EOFs right after initialize).
    let host = ParkedHost::new();
    host.fail_launch_after_initialize();
    let executor = HostPromptExecutor::new(host.clone(), store.clone(), TimeoutConfig::default());
    seed_prompt_run(store.as_ref(), "retained:eof-launch", bindings()).await;

    let refused = executor
        .execute(request("retained:eof-launch"))
        .await
        .expect_err("an EOF after initialize must fail the launch");
    match &refused {
        CapabilityError::TransientExternal(msg) => {
            assert!(
                msg.contains("host session creation failed"),
                "typed launch failure: {msg}"
            );
            assert!(
                msg.contains("launch failed"),
                "the refusal must carry the launch-failure class: {msg}"
            );
        }
        other => panic!("expected TransientExternal, got: {other:?}"),
    }
    assert_eq!(
        host.prompt_count(),
        0,
        "a failed launch must produce no prompt effect"
    );
    let state = store
        .load_run(&nexus_orchestration::SessionId(
            "retained:eof-launch".to_string(),
        ))
        .await
        .expect("load run")
        .expect("record")
        .state
        .expect("v1 state");
    assert!(
        state.in_flight.is_none(),
        "a refused launch must drop its durable attempt ownership"
    );

    // The deltas arrive but the stream then EOFs with no terminal event.
    let host = ParkedHost::new();
    host.release_all();
    host.close_stream_after_deltas();
    let executor = HostPromptExecutor::new(host.clone(), store.clone(), TimeoutConfig::default());
    seed_prompt_run(store.as_ref(), "retained:eof-stream", bindings()).await;

    let refused = executor
        .execute(request("retained:eof-stream"))
        .await
        .expect_err("an EOF without a terminal event must refuse");
    assert!(
        matches!(&refused, CapabilityError::TransientExternal(msg)
            if msg.contains("closed without a terminal event")),
        "an EOF after the deltas must be a typed failure, got {refused:?}"
    );
    assert_eq!(host.prompt_count(), 1, "the EOF prompt did reach the Host");
}

/// MIGRATED from `crates/nexus-daemon-runtime/tests/workflow_prompt_execution.rs`
/// `all_five_consumers_observe_non_echo_agent_output` (its durable
/// post-prompt assertions).
///
/// A prompt driven through the production graph node stores the agent's TEXT
/// in the graph context — never a live Host/ACP handle: the serialized
/// context carries the output and no provider adapter or managed process —
/// and the completed prompt leaves no durable `in_flight` marker behind.
#[tokio::test]
async fn graph_prompt_context_serializes_output_without_host_handles() {
    use graph_flow::Task as _;
    use nexus_orchestration::capability::{PromptExecutor, ToolPolicy};
    use nexus_orchestration::run_state::AgentBinding;
    use nexus_orchestration::tasks::InnerGraphNodeTask;
    use std::sync::RwLock;

    let tmp = tempfile::tempdir().unwrap();
    let guarded = nexus_local_db::init_engine_pool(&tmp.path().join("state.db"))
        .await
        .expect("engine pool");
    let pool = Arc::new(guarded.clone_pool());
    let store = Arc::new(SqliteSessionStorage::new(pool));
    let host = ParkedHost::new();
    host.release_all();
    let executor: Arc<dyn PromptExecutor> = Arc::new(HostPromptExecutor::new(
        host.clone(),
        store.clone(),
        TimeoutConfig::default(),
    ));

    let mut bindings = HashMap::new();
    bindings.insert(
        "default".to_string(),
        AgentBinding {
            provider_id: PROVIDER.to_string(),
            model: None,
        },
    );
    let run_id = "retained:graph-context";
    seed_prompt_run(store.as_ref(), run_id, bindings).await;

    let mut cancels = HashMap::new();
    cancels.insert(
        run_id.to_string(),
        tokio_util::sync::CancellationToken::new(),
    );
    let task = InnerGraphNodeTask::new("n1")
        .with_template("graph prompt {{core_context.version}}")
        .with_tool_policy(ToolPolicy::DenyAll)
        .with_prompt_executor(Some(executor))
        .with_session_cancels(Arc::new(RwLock::new(cancels)));

    let ctx = graph_flow::Context::new();
    ctx.set("_session_id", run_id.to_string()).unwrap();
    ctx.set("core_context.version", "7").unwrap();
    let result = task.run(ctx.clone()).await.expect("graph prompt succeeds");
    assert_eq!(
        result.response.as_deref(),
        Some("transformed:parked-output"),
        "the graph node must return the agent output, not the prompt"
    );
    assert_eq!(
        ctx.get::<String>("state.n1.output").as_deref(),
        Some("transformed:parked-output"),
        "the node must store the agent output in the graph context"
    );

    // Durable cleanup: the completed prompt owns no `in_flight` attempt.
    let state = store
        .load_run(&nexus_orchestration::SessionId(run_id.to_string()))
        .await
        .expect("load run")
        .expect("record")
        .state
        .expect("v1 state");
    assert!(
        state.in_flight.is_none(),
        "a successfully completed prompt must not leave a durable in_flight marker"
    );

    // Serialization: the context carries text only — no live Host/ACP handle.
    let serialized = serde_json::to_string(&ctx).expect("context serializes");
    assert!(
        serialized.contains("transformed:parked-output"),
        "the serialized context must carry the agent output: {serialized}"
    );
    assert!(
        !serialized.contains("AcpSdkAdapter"),
        "no SDK handle in context: {serialized}"
    );
    assert!(
        !serialized.contains("ManagedAcpProcess"),
        "no process handle in context: {serialized}"
    );
}
