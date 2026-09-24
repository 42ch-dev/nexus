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
    ProtocolKind, ProviderHealth, TextDeltaEvent,
};
use nexus_agent_host::config::TimeoutConfig;
use nexus_agent_host::{
    DiscoverySource, HostError, HostFacade, HostOperationId, HostResult, HostSession,
    HostSessionId, LaunchStrategy, ProviderCatalog, ProviderCatalogEntry, ProviderId, SessionState,
    TrustLevel,
};
use nexus_contracts::generated::core::{
    CoreWorkflowEventBatchEventsItem, CoreWorkflowSubscribeRequest,
    CoreWorkflowSubscribeRequestLastEventId, CoreWorkflowSubscribeRequestRunId,
    CoreWorkflowSubscription,
};
#[cfg(feature = "test-hooks")]
use nexus_contracts::local::orchestration::{WorkspaceChangeEntry, WorkspaceChangeOp};
use nexus_contracts::local::schedule::http::{AddScheduleRequest, AgentBindingDto};
use nexus_contracts::{
    CoreError as WireCoreError, CoreErrorCode, CoreWorkspaceCommitRequest,
    CoreWorkspaceCommitRequestChangesItem, CoreWorkspaceCommitRequestChangesItemOp, ProviderCall,
    ProviderEventBatch, ProviderReply,
};
use nexus_core::execution::authority::WorkspaceAuthorityLease;
use nexus_core::execution::prompt_executor::HostPromptExecutor;
use nexus_core::execution::run_events::{MAX_RECORDS_PER_RUN, MAX_SUBSCRIBERS_PER_RUN};
use nexus_core::execution::schedules::chronology::{
    parse_interval_secs, run_one_tick as chronology_run_one_tick, AutoChronologyConfig,
    DEFAULT_AUTO_CHRONOLOGY_INTERVAL_SECS, ENV_AUTO_CHRONOLOGY_INTERVAL_MIN,
};
use nexus_core::execution::schedules::cron;
use nexus_core::execution::schedules::stale_findings::run_one_sweep;
#[cfg(feature = "test-hooks")]
use nexus_core::execution::session::{SessionError, SessionId, WorkspaceSessionManager};
#[cfg(feature = "test-hooks")]
use nexus_core::execution::test_hooks;
use nexus_core::execution::workflow::{RunEventPort, WorkflowRunCoordinator};
use nexus_core::execution::{
    drive_preset_run, resume_driven_sessions, ExecutionHandle, PresetRunConfig, PresetRunOutcome,
    ResumeDecision, RunControlError, RunnerDeps,
};
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService, Principal};
use nexus_local_db::findings::{create_finding, Finding};
use nexus_local_db::works::{create_work_atomic, WorkRecord};
use nexus_local_db::writer_protocol::init_guarded_pool;
use nexus_orchestration::capability::{CapabilityError, DaemonToolDispatch};
use nexus_orchestration::engine::{
    GraphFlowEngine, OrchestrationEngine, SessionStatus, SessionSummary,
};
use nexus_orchestration::preset_runtime::build_wired_outer_graph;
use nexus_orchestration::run_state::RunRecord;
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
/// The user preset the hosted composition (v1.195 P0-T2) drives: workspace
/// capability chain + prompt.
const HOSTED_PRESET: &str = "hosted-schedule-drive";

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
    /// When set, every owned session shutdown FAILS: the exact owned process
    /// cleanup cannot be confirmed, so a cancel must settle `interrupted` —
    /// never a false successful `cancelled`.
    shutdown_unconfirmed: AtomicBool,
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
            shutdown_unconfirmed: AtomicBool::new(false),
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

    /// Release the prompts parked RIGHT NOW, WITHOUT arming the permanent
    /// [`Self::release_all`] flag: a later prompt parks again, so a test can
    /// unblock one owned operation and still keep the next one in flight.
    ///
    /// `exec` registers its release sender and bumps the prompt counter in the
    /// same synchronous step, so a caller that observed `prompt_count() >= 1`
    /// through [`wait_for_prompt`] is guaranteed a parked sender to release.
    fn release_parked(&self) {
        for sender in self.parked.lock().expect("parked").drain(..) {
            let _ = sender.send(());
        }
    }

    /// Make every owned-session shutdown unconfirmable (see the field doc).
    fn fail_session_shutdown(&self) {
        self.shutdown_unconfirmed.store(true, Ordering::SeqCst);
    }

    /// Restore a confirmable owned-session shutdown — the retry that must
    /// confirm the unconfirmed cancellation.
    fn confirm_session_shutdown(&self) {
        self.shutdown_unconfirmed.store(false, Ordering::SeqCst);
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
        // Unconfirmed cleanup: the session is deliberately LEFT in the map so
        // the run stays visibly interrupted and a retry can still reap it.
        if self.shutdown_unconfirmed.load(Ordering::SeqCst) {
            return Err(HostError::internal(
                "retained parked host: owned session shutdown unconfirmed",
            ));
        }
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
        // The production catalog truth the hosted factory validates frozen
        // role bindings against: the parked provider is CONFIGURED, so a
        // binding that names it is accepted. An empty catalog here would make
        // every admission refuse with "unknown provider" instead.
        Ok(ProviderCatalog {
            entries: vec![ProviderCatalogEntry {
                provider_id: ProviderId::new(PROVIDER),
                display_name: "Retained parked provider".to_string(),
                protocol_kind: ProtocolKind::Acp,
                launch: LaunchStrategy::Acp {
                    command: "retained-parked-host".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                },
                source: DiscoverySource::Config,
                trust: TrustLevel::Explicit,
                capabilities: CapabilityDescriptor::native_cli_limited(),
                health: ProviderHealth {
                    provider_id: ProviderId::new(PROVIDER),
                    available: true,
                    latency_ms: None,
                    message: None,
                },
            }],
        })
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
    wait_for_prompt_count(host, 1).await;
}

/// Wait (bounded) until the parked Host has recorded at least `min` prompts —
/// `exec` bumps the counter and registers its release sender in the same
/// synchronous step, so a caller returning from here owns a parked prompt.
async fn wait_for_prompt_count(host: &ParkedHost, min: usize) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while host.prompt_count() < min {
        assert!(
            std::time::Instant::now() < deadline,
            "the admitted run never reached the prompt port"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Wait (bounded) until the owned clock has claimed `schedule_id`'s run and
/// return that run id.
async fn wait_for_schedule_run(pool: &sqlx::SqlitePool, schedule_id: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let (_, owned) = schedule_row(pool, schedule_id).await;
        if let Some(run_id) = owned {
            return run_id;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the owned clock never admitted {schedule_id}"
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

/// Insert one `driven_v1` pending schedule for `preset_id` through the PUBLIC
/// add path (frozen descriptor + version-0 seed), as every admission-ready
/// row is created.
async fn add_schedule_preset(
    handle: &ExecutionHandle,
    principal: &Principal,
    preset_id: &str,
    label: &str,
) -> String {
    let request = AddScheduleRequest {
        creator_id: CREATOR.to_string(),
        preset_id: preset_id.to_string(),
        seed: None,
        label: Some(label.to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: Some(json!({ "topic": "retained-cancel" })),
        force_gates: false,
        reason: None,
        agent_bindings: Some(default_bindings()),
    };
    handle
        .add_schedule(principal, request)
        .await
        .expect("schedule insert")
        .schedule_id
}

/// Write a directory preset bundle into the fixture's nexus home (the same
/// place `hosted_fixture` freezes `hosted-schedule-drive`), so the public add
/// path and admission resolve it from disk.
fn write_preset_bundle(nexus_home: &Path, preset_id: &str, yaml: &str, prompt_body: &str) {
    let bundle = nexus_home.join("presets").join(preset_id);
    std::fs::create_dir_all(bundle.join("prompts")).expect("preset bundle");
    std::fs::write(bundle.join("preset.yaml"), yaml).expect("preset yaml");
    std::fs::write(bundle.join("prompts/generate.md"), prompt_body).expect("prompt template");
}

fn signal(name: &str) -> nexus_contracts::local::schedule::http::SignalScheduleRequest {
    nexus_contracts::local::schedule::http::SignalScheduleRequest {
        signal: name.to_string(),
        wait_id: None,
    }
}

/// Count the creator's ROOT runs (`parent_session_id IS NULL`) — one run per
/// schedule, ever: inner-graph child rows are descendants of that one root.
async fn root_run_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar(
        "SELECT COUNT(*) FROM orchestration_sessions WHERE parent_session_id IS NULL",
    )
    .fetch_one(pool)
    .await
    .expect("root run count")
}

/// The durable v1 record of one run (status + durable state).
async fn durable_record(
    pool: &sqlx::SqlitePool,
    run_id: &str,
) -> nexus_orchestration::run_state::RunRecord {
    SqliteSessionStorage::new(Arc::new(pool.clone()))
        .load_run(&nexus_orchestration::engine::SessionId(run_id.to_string()))
        .await
        .expect("durable run load")
        .expect("the admitted run row exists")
}

/// Wait (bounded) until the run is parked at its manual human wait, and
/// return that run's exact durable wait token.
async fn wait_for_human_wait(pool: &sqlx::SqlitePool, run_id: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let record = durable_record(pool, run_id).await;
        if let Some(wait) = record.state.as_ref().and_then(|s| s.wait.as_ref()) {
            return wait.wait_id.clone();
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never reached its manual human wait (status {:?})",
            record.status
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Wait (bounded) until the run reaches a durable status.
async fn wait_for_run_status(pool: &sqlx::SqlitePool, run_id: &str, status: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if durable_record(pool, run_id).await.status.as_db_str() == status {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never reached {status}"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
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

// ─────────────────────────────────────────────────────────────────────────────
// Hosted production composition (v1.195 P0-T2)
// ─────────────────────────────────────────────────────────────────────────────

/// The payload the driven run commits through `workspace.commit`, base64 (the
/// capability's wire shape) with the plaintext kept beside it for the
/// on-disk assertion.
const HOSTED_PAYLOAD: &[u8] = b"hosted workspace payload\n";
const HOSTED_PAYLOAD_B64: &str = "aG9zdGVkIHdvcmtzcGFjZSBwYXlsb2FkCg==";

/// The user preset the hosted composition drives: an AUTHORIZED workspace
/// capability chain (`workspace.open` into the frozen root, then one declared
/// create through the bound commit authority), then a prompt. Both halves of
/// the hosted wiring are therefore load-bearing for the run to reach the
/// parked Host.
fn hosted_preset_yaml() -> String {
    format!(
        r#"
preset:
  id: {HOSTED_PRESET}
  version: 1
  kind: creator
  description: "hosted production composition proof"
  requires_capabilities:
    - workspace.open
    - workspace.commit
    - acp.prompt
  initial: open_scope
  terminal: done
states:
  - id: open_scope
    description: "open a scope inside the factory-resolved creative root"
    enter:
      - kind: capability
        name: workspace.open
        args:
          path: notes
    exit_when: {{ kind: rule }}
    next: commit_scope
  - id: commit_scope
    description: "one declared create through the bound commit authority"
    enter:
      - kind: capability
        name: workspace.commit
        args:
          sessionId: "{{{{_capability_output.sessionId}}}}"
          changes:
            - path: hosted.txt
              op: create
              contentBase64: "{HOSTED_PAYLOAD_B64}"
    exit_when: {{ kind: rule }}
    next: generate
  - id: generate
    enter:
      - kind: inner_graph
        name: generate_graph
    exit_when: {{ kind: graph_complete }}
    next: done
  - id: done
    terminal: true

inner_graphs:
  generate_graph:
    nodes:
      - id: hosted_prompt
        kind: acp_prompt
        template_file: prompts/generate.md
        tool_policy: deny_all
    output_binding: hosted_prompt.text
"#
    )
}

/// An engine-owner core over a REAL selected creative root, plus the owner the
/// public `start_hosted_execution` factory composed for it.
///
/// Unlike [`owner_fixture`], nothing is attached by hand: the supervisor, its
/// coordinator-backed starter and the clock task all come from the factory,
/// and the creative root comes from the core's own operational metadata (the
/// document the workspace registration writes) rather than a test argument.
struct HostedFixture {
    tmp: TempDir,
    core: CoreService,
    host: Arc<ParkedHost>,
    handle: Arc<ExecutionHandle>,
    /// Canonical root the factory resolved (`meta.json` `local_root`).
    root: PathBuf,
}

async fn hosted_fixture() -> HostedFixture {
    let tmp = tempfile::tempdir().expect("tempdir");
    let root = prepare_hosted_home(tmp.path()).await;
    let (core, host, handle) = open_hosted_owner(tmp.path()).await;
    HostedFixture {
        tmp,
        core,
        host,
        handle,
        root,
    }
}

/// Prepare a home that [`open_hosted_owner`] can open: the nexus config
/// selecting the fixture Creator/workspace, a registered creative root, the
/// seeded admitted Creator row and the hosted preset bundle.
///
/// Returns the CANONICAL creative root the factory will resolve.
async fn prepare_hosted_home(home: &Path) -> PathBuf {
    let nexus_home = home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).expect("nexus home");
    let creative_root = home.join("creative");
    std::fs::create_dir_all(creative_root.join("notes")).expect("creative root");
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n[active_workspace_slug_by_creator]\n\"{CREATOR}\" = \"{SLUG}\"\n"
        ),
    )
    .expect("config");
    let operational = nexus_home_layout::operational_workspace_dir(home, CREATOR, SLUG);
    std::fs::create_dir_all(&operational).expect("operational dir");
    std::fs::write(
        operational.join("meta.json"),
        serde_json::to_vec(&json!({ "local_root": creative_root })).expect("meta json"),
    )
    .expect("meta");

    // The user preset bundle the shared resolver reads at add AND admission
    // time (its content-addressed source identity is frozen into the row).
    let bundle = nexus_home.join("presets").join(HOSTED_PRESET);
    std::fs::create_dir_all(bundle.join("prompts")).expect("preset bundle");
    std::fs::write(bundle.join("preset.yaml"), hosted_preset_yaml()).expect("preset yaml");
    std::fs::write(
        bundle.join("prompts/generate.md"),
        "Summarize the hosted topic: {{preset.input.topic}}\n",
    )
    .expect("prompt template");

    let db_path = nexus_home_layout::workspace_state_db_path(home, CREATOR, SLUG);
    // Seed through a temporary admitted pool and RELEASE the writer guard
    // before the engine owner opens (the owner takes an OS lock).
    {
        let guarded = nexus_local_db::init_engine_pool(&db_path)
            .await
            .expect("engine pool init");
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, \
             cached_at, data) VALUES (?, 'Hosted', 'active', datetime('now'), '{}')",
        )
        .bind(CREATOR)
        .execute(guarded.pool())
        .await
        .expect("seed the admitted creator row");
        guarded.pool().close().await;
        nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);
    }

    std::fs::canonicalize(&creative_root).expect("canonical creative root")
}

/// Open ONE hosted owner over a home prepared by [`prepare_hosted_home`].
///
/// Separate from [`hosted_fixture`] so a test can open a SECOND owner over the
/// same home after the first one closed.
async fn open_hosted_owner(home: &Path) -> (CoreService, Arc<ParkedHost>, Arc<ExecutionHandle>) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .expect("engine-owner core open");
    let host = ParkedHost::new();
    let handle = core
        .start_hosted_execution(
            host.clone(),
            Arc::new(NullProvider) as Arc<dyn ProviderPort>,
            TimeoutConfig::default(),
        )
        .await
        .expect("the public factory composes the hosted owner");
    (core, host, handle)
}

/// A `driven_v1` pending schedule for the hosted preset, created through the
/// PUBLIC add path on the factory-composed owner.
async fn add_hosted_schedule(fixture: &HostedFixture) -> String {
    let principal = fixture.core.active_principal().await.unwrap();
    let request = AddScheduleRequest {
        creator_id: CREATOR.to_string(),
        preset_id: HOSTED_PRESET.to_string(),
        seed: None,
        label: Some("hosted-once".to_string()),
        depends_on: None,
        concurrency: None,
        scheduled_at: None,
        input: Some(json!({ "topic": "hosted-scheduler" })),
        force_gates: false,
        reason: None,
        agent_bindings: Some(default_bindings()),
    };
    fixture
        .handle
        .add_schedule(&principal, request)
        .await
        .expect("public schedule add")
        .schedule_id
}

/// The hosted owner drives exactly one durable pending schedule end to end —
/// through the production supervisor, starter and clock task the factory
/// installed — and joins that clock task on close.
///
/// Ordering evidence: the public add answers `pending` and the durable row
/// still owns nothing; only the owned clock's tick claims the row, and that
/// claim is the atomic schedule→run identity the store writes (never a
/// status-only flip). The run then executes the authorized workspace
/// capability chain inside the factory-resolved root and reaches the real
/// prompt port; a repeat tick plus a repeat admission return the SAME run, so
/// no second drive can reach the port.
#[tokio::test]
#[serial_test::serial]
async fn hosted_schedule_drives_once_and_closes() {
    let fixture = hosted_fixture().await;
    let pool = fixture.handle.coordinator().pool();

    // ── The factory installed ONE live background task (the supervisor
    //    wake/clock) before any row exists. ──
    assert!(
        !fixture.handle.owned_tasks_finished(),
        "the hosted owner must install its scheduler task"
    );

    // ── Public add is PENDING: durable, frozen, and owning no run. ──
    let schedule_id = add_hosted_schedule(&fixture).await;
    let (status, owned) = schedule_row(pool.as_ref(), &schedule_id).await;
    assert_eq!(
        status, "pending",
        "the public add must leave the row pending, never running"
    );
    assert!(
        owned.is_none(),
        "the public add must not manufacture an owned run"
    );
    assert_eq!(
        fixture.host.prompt_count(),
        0,
        "no run may reach the prompt port before admission"
    );
    assert!(
        !fixture
            .tmp
            .path()
            .join("creative/notes/hosted.txt")
            .exists(),
        "no workspace effect may exist before admission"
    );

    // ── The OWNED clock drives it: exactly one run, executing the workspace
    //    capability chain inside the canonical root and then the prompt. ──
    wait_for_prompt(&fixture.host).await;
    assert_eq!(
        fixture.host.prompt_count(),
        1,
        "exactly one run may reach the prompt port"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("notes/hosted.txt")).expect("committed bytes"),
        HOSTED_PAYLOAD,
        "the authorized workspace capability must commit inside the factory-resolved root"
    );
    let (status, owned) = schedule_row(pool.as_ref(), &schedule_id).await;
    assert_eq!(status, "running", "the admitted row owns its run");
    let run_id = owned.expect("the admitted schedule owns its run");
    let owned_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules WHERE current_session_id IS NOT NULL",
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("owned schedule count");
    assert_eq!(owned_rows, 1, "exactly one schedule may own a run");
    let run_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?")
            .bind(&run_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("run row count");
    assert_eq!(run_rows, 1, "the schedule's owned run exists exactly once");

    // ── Repeated tick + repeated admission yield the SAME run. ──
    let supervisor = fixture
        .handle
        .coordinator()
        .schedule_supervisor()
        .expect("the factory attaches the production supervisor");
    nexus_core::execution::schedules::hosted_scheduler::run_one_tick(&supervisor)
        .await
        .expect("a repeated clock tick is a no-op for an already-owned row");
    let starter = supervisor
        .schedule_starter_clone()
        .expect("the factory injects the coordinator-backed starter");
    let again = starter
        .start(&schedule_id)
        .await
        .expect("repeated admission returns the owned run");
    assert_eq!(
        again.0, run_id,
        "repeated admission must return the same run, never mint a second"
    );
    assert_eq!(
        fixture.host.prompt_count(),
        1,
        "no second drive may reach the prompt port"
    );
    let (status_after, owned_after) = schedule_row(pool.as_ref(), &schedule_id).await;
    assert_eq!(status_after, "running");
    assert_eq!(owned_after.as_deref(), Some(run_id.as_str()));

    // ── Close JOINS the owned scheduler task. ──
    fixture.host.release_all();
    let report = fixture.handle.close().await.expect("owner close");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    assert!(
        fixture.handle.owned_tasks_finished(),
        "close must join the owned scheduler task"
    );
    assert!(fixture.handle.is_settled(), "close settles the owner");
    fixture.core.close().await.expect("core close");
}

/// A `running` row that owns NO run is REFUSED — never admitted, and never
/// turned into a fresh run (v1.195 P0-T2 fix, L2 C1).
///
/// The stale shape is staged directly in storage. The durable product contract
/// is that a never-started row merely LABELLED `Running` is not running:
/// "Boot/tick/cron exclude legacy never-started pending/running/paused rows;
/// missing session ID or creation time is not automatic opt-in. Explicit public
/// schedule start revalidates and opts only that row in."
/// (`.mstar/specs/daemon-runtime.md` §3). Both production paths therefore refuse
/// it — the direct coordinator gate AND the coordinator-backed starter the
/// owned clock drives — and nothing is minted: no owned session, no run row, no
/// prompt.
#[tokio::test]
#[serial_test::serial]
async fn non_owned_running_schedule_is_refused_without_minting_a_run() {
    let fixture = hosted_fixture().await;
    let coordinator = fixture.handle.coordinator();
    let pool = coordinator.pool();
    let caps = fixture.handle.capability_holder();
    let home = fixture.tmp.path().join(".nexus42");

    // The row is created through the PUBLIC add (so it carries a frozen
    // descriptor + version-0 seed exactly as a real driven row does) and is
    // then staged into the stale shape: `running` with NO owned run. The
    // staging UPDATE lands within the same millisecond as the insert, while the
    // owned clock's first tick is a full interval away — and from then on the
    // row is `running`, which the tick never treats as a pending candidate.
    let schedule_id = add_hosted_schedule(&fixture).await;
    sqlx::query(
        "UPDATE creator_schedules SET status = 'running', current_session_id = NULL \
         WHERE schedule_id = ?",
    )
    .bind(&schedule_id)
    .execute(pool.as_ref())
    .await
    .expect("stage the non-owned running row");
    assert!(
        !fixture.handle.owned_tasks_finished(),
        "the owned clock is live while this case asserts"
    );

    // ── 1. The direct admission gate (what an explicit start reaches). ──
    match coordinator
        .admit_schedule(&schedule_id, pool.as_ref(), &home, &caps, None, None)
        .await
    {
        Err(RunControlError::NotEligible(id, reason)) => {
            assert_eq!(id, schedule_id, "the refusal names the row");
            assert!(
                reason.contains("no owned run"),
                "the refusal must name the non-owned row: {reason}"
            );
            assert!(
                reason.contains("Boot recovery"),
                "the refusal must point at boot-recovery classification: {reason}"
            );
        }
        other => panic!("a non-owned running row must be refused as not eligible, got {other:?}"),
    }

    // ── 2. The coordinator-backed starter — the owned clock's own path — and a
    //       real clock tick (a no-op for a row that owns no run). ──
    let supervisor = coordinator
        .schedule_supervisor()
        .expect("the factory attaches the production supervisor");
    let starter = supervisor
        .schedule_starter_clone()
        .expect("the factory injects the coordinator-backed starter");
    let via_starter = starter.start(&schedule_id).await;
    let starter_message = format!("{via_starter:?}");
    assert!(
        via_starter.is_err(),
        "the starter must refuse a non-owned running row: {starter_message}"
    );
    assert!(
        starter_message.contains("no owned run"),
        "the starter refusal must carry the same typed reason: {starter_message}"
    );
    nexus_core::execution::schedules::hosted_scheduler::run_one_tick(&supervisor)
        .await
        .expect("a clock tick is a no-op for a non-owned running row");

    // ── 3. Nothing was minted by any of the three attempts. ──
    let (status, owned) = schedule_row(pool.as_ref(), &schedule_id).await;
    assert_eq!(status, "running", "the staged row keeps its durable state");
    assert!(owned.is_none(), "no run may be claimed for a non-owned row");
    let owned_rows: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules WHERE current_session_id IS NOT NULL",
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("owned schedule count");
    assert_eq!(owned_rows, 0, "no schedule may own a run");
    let run_rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions")
        .fetch_one(pool.as_ref())
        .await
        .expect("run row count");
    assert_eq!(run_rows, 0, "refusal must not create a run row");
    assert_eq!(
        fixture.host.prompt_count(),
        0,
        "no drive may reach the prompt port"
    );

    fixture.handle.close().await.expect("owner close");
    fixture.core.close().await.expect("core close");
}

// ─────────────────────────────────────────────────────────────────────────────
// Public durable reads and core-context append (v1.195 P0-T3)
// ─────────────────────────────────────────────────────────────────────────────

/// The public durable read/context surface (S0-2/S0-3/S0-5).
///
/// One admitted Creator and one durable schedule: the public lists and inspect
/// report the STORED identity/status/version, an unknown id and a FOREIGN row
/// close with the same refusal (the scope is the stored owner, never a
/// caller-supplied creator), and a Steer append commits the immutable version
/// and the pointer advance as one transaction — concurrent appends leave every
/// body in the version chain with the pointer on the winner, and the NEXT
/// execution boundary freezes that committed version into the run.
#[allow(clippy::too_many_lines)] // one linear public journey; splitting hides the ordering evidence
#[tokio::test]
#[serial_test::serial]
async fn public_schedule_reads_and_context_are_owned() {
    use nexus_contracts::generated::daemon_api::orchestration::sessions::list_sessions_query::ListSessionsQuery;
    use nexus_contracts::generated::daemon_api::schedule::edit_core_context_request::EditCoreContextRequest;
    use nexus_contracts::generated::daemon_api::schedule::list_schedules_query::ListSchedulesQuery;
    use nexus_contracts::local::schedule::{CoreContextPayload, CoreContextVersion, ScheduleId};

    /// The generated PATCH body for one append.
    fn append(body: &str) -> EditCoreContextRequest {
        EditCoreContextRequest {
            op: "append".to_string(),
            body: Some(body.to_string()),
            patch: None,
            path: None,
        }
    }

    let fixture = owner_fixture().await;
    let principal = fixture.core.active_principal().await.unwrap();
    let pool = fixture.pool();
    let coordinator = fixture.coordinator();
    let caps = fixture.handle.capability_holder();
    let home = fixture.nexus_home();
    let executor =
        fixture.executor.clone() as Arc<dyn nexus_orchestration::capability::PromptExecutor>;

    // ── 1. A durable pending row created through the PUBLIC add appears in the
    //       schedule list with its stored identity/status/policy and NO run. ──
    let schedule_id = add_pending_schedule(&fixture, "retained-reads").await;

    let schedules = fixture
        .handle
        .list_schedules(&principal, ListSchedulesQuery::default())
        .await
        .expect("list schedules");
    let listed = schedules
        .items
        .iter()
        .find(|item| item.schedule_id == schedule_id)
        .expect("the durable row is listed");
    assert_eq!(listed.creator_id, CREATOR);
    assert_eq!(listed.preset_id, PROMPT_PRESET);
    assert_eq!(listed.status, "pending");
    assert_eq!(listed.execution_policy, "driven_v1");
    assert_eq!(listed.current_core_context_version, 0);
    assert_eq!(listed.label.as_deref(), Some("retained-reads"));
    assert!(
        listed.current_session_id.is_none(),
        "a public read must never manufacture an owned run: {listed:?}"
    );
    assert!(
        !schedules.pagination.has_more,
        "one row must fit the default page"
    );

    // Nothing has been admitted, so the DURABLE session list is empty (this is
    // the store, not an in-memory engine map or a Host session list).
    let sessions = fixture
        .handle
        .list_workflow_sessions(&principal, ListSessionsQuery::default())
        .await
        .expect("list sessions");
    assert!(
        sessions.items.is_empty(),
        "an empty durable session list is valid only before any admission: {:?}",
        sessions.items
    );

    let inspected = fixture
        .handle
        .inspect_schedule(&principal, schedule_id.clone())
        .await
        .expect("inspect the pending row");
    assert_eq!(inspected.schedule.schedule_id, schedule_id);
    assert_eq!(inspected.schedule.status, "pending");
    assert_eq!(inspected.schedule.current_core_context_version, 0);
    assert!(inspected.schedule.current_session_id.is_none());
    assert_eq!(inspected.concurrency_kind, "serial");
    assert!(inspected.depends_on.is_empty());

    // ── 2. Foreign and unknown ids close IDENTICALLY, and an explicit foreign
    //       creator filter refuses before any query runs. ──
    // The foreign row is staged directly: the public add path can only mint
    // rows for the admitted Creator, so the foreign owner must come from
    // storage (the point of the assertion is the READ scope).
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, created_at, updated_at)
         VALUES ('SCH-foreign', 'other_creator', ?, 1, 'pending', 'serial', 0, 1, 1)",
    )
    .bind(PROMPT_PRESET)
    .execute(pool.as_ref())
    .await
    .expect("stage a foreign-owned schedule row");

    let unknown = fixture
        .handle
        .inspect_schedule(&principal, "SCH-missing".to_string())
        .await
        .unwrap_err();
    let foreign = fixture
        .handle
        .inspect_schedule(&principal, "SCH-foreign".to_string())
        .await
        .unwrap_err();
    for (id, err) in [("SCH-missing", &unknown), ("SCH-foreign", &foreign)] {
        match err {
            nexus_core::CoreError::NotFound { resource } => {
                assert_eq!(
                    resource,
                    &format!("schedule {id}"),
                    "an unknown id and a foreign id must close with the same refusal"
                );
            }
            other => panic!("a foreign or unknown id must close as NotFound, got {other:?}"),
        }
    }

    let foreign_filter = fixture
        .handle
        .list_schedules(
            &principal,
            ListSchedulesQuery {
                creator_id: Some("other_creator".to_string()),
                ..ListSchedulesQuery::default()
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(foreign_filter, nexus_core::CoreError::Forbidden { .. }),
        "an explicit foreign creator filter must refuse, never answer an empty page: \
         {foreign_filter:?}"
    );
    assert!(
        schedules
            .items
            .iter()
            .all(|item| item.schedule_id != "SCH-foreign"),
        "another creator's row must never be listed"
    );

    // ── 3. A Steer append is ONE durable version plus the pointer advance. ──
    let appended = fixture
        .handle
        .edit_core_context(&principal, schedule_id.clone(), append("idea-one"))
        .await
        .expect("append the first idea");
    assert_eq!(appended.new_version, 1);
    let inspected = fixture
        .handle
        .inspect_schedule(&principal, schedule_id.clone())
        .await
        .expect("inspect after the append");
    assert_eq!(
        inspected.schedule.current_core_context_version, 1,
        "inspect must report the version the append committed"
    );

    let unknown_edit = fixture
        .handle
        .edit_core_context(&principal, "SCH-missing".to_string(), append("nope"))
        .await
        .unwrap_err();
    let foreign_edit = fixture
        .handle
        .edit_core_context(&principal, "SCH-foreign".to_string(), append("nope"))
        .await
        .unwrap_err();
    for (id, err) in [
        ("SCH-missing", &unknown_edit),
        ("SCH-foreign", &foreign_edit),
    ] {
        match err {
            nexus_core::CoreError::NotFound { resource } => assert_eq!(
                resource,
                &format!("schedule {id}"),
                "a foreign append must close exactly like an unknown one"
            ),
            other => panic!("a foreign or unknown append must close as NotFound, got {other:?}"),
        }
    }
    let foreign_versions: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM core_context_versions WHERE schedule_id = 'SCH-foreign'",
    )
    .fetch_one(pool.as_ref())
    .await
    .expect("foreign version count");
    assert_eq!(
        foreign_versions, 0,
        "a refused append to another creator's schedule must write nothing"
    );

    // ── 4. CONCURRENT appends: every body is durable exactly once, the versions
    //       are monotonic, and the pointer names the last one (no lost body). ──
    let bodies = ["idea-two", "idea-three", "idea-four", "idea-five"];
    let results = futures_util::future::join_all(bodies.iter().map(|body| {
        fixture
            .handle
            .edit_core_context(&principal, schedule_id.clone(), append(body))
    }))
    .await;
    // Pair every append with the version it committed. The COMMIT order is the
    // winner of the writer race, not the call order, so the version numbers
    // (never the call indices) are what the chain must be read in.
    let mut committed: Vec<(i64, &str)> = vec![(1, "idea-one")];
    for (body, result) in bodies.iter().zip(results) {
        let version = result
            .expect("a concurrent append must commit, never lose its body")
            .new_version;
        committed.push((version, body));
    }
    committed.sort_unstable_by_key(|(version, _)| *version);
    assert_eq!(
        committed
            .iter()
            .map(|(version, _)| *version)
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4, 5],
        "concurrent appends must claim one distinct, monotonic version each"
    );

    let manager = fixture
        .handle
        .coordinator()
        .schedule_supervisor()
        .expect("the fixture attaches a supervisor")
        .core_context_manager();
    let sid = ScheduleId(schedule_id.clone());
    assert_eq!(
        manager.current_version(&sid).await.expect("pointer").0,
        5,
        "the pointer is the version authority and names the last committed append"
    );
    // Version 0 is the durable seed, so the chain starts from an empty body.
    let seed = manager
        .read(&sid, CoreContextVersion(0))
        .await
        .expect("the version-0 seed is durable");
    match seed.content {
        CoreContextPayload::Text { body } => assert!(body.is_empty(), "empty seed, got: {body}"),
        CoreContextPayload::Struct { .. } => panic!("the seed is text"),
    }
    // Every version must be the previous version PLUS exactly its own body: a
    // lost body shortens the chain, a duplicated body extends it, and a version
    // derived from a stale base would reorder it.
    let mut expected = String::new();
    for (version, body) in &committed {
        expected.push_str(body);
        let record = manager
            .read(
                &sid,
                CoreContextVersion(u32::try_from(*version).expect("version fits")),
            )
            .await
            .unwrap_or_else(|e| panic!("version {version} must be durable: {e}"));
        let CoreContextPayload::Text { body: durable } = record.content else {
            panic!("the appended context stays text");
        };
        assert_eq!(
            durable, expected,
            "version {version} must be the previous version plus exactly '{body}'"
        );
    }

    // ── 5. The NEXT EXECUTION BOUNDARY consumes the winning committed version:
    //       admission freezes the pointer's payload into the run, and the public
    //       reads report that same run and version. ──
    let run_id = coordinator
        .admit_schedule(
            &schedule_id,
            pool.as_ref(),
            &home,
            &caps,
            None,
            Some(executor),
        )
        .await
        .expect("admission")
        .0;
    let context_json: Vec<u8> =
        sqlx::query_scalar("SELECT context_json FROM orchestration_sessions WHERE session_id = ?")
            .bind(&run_id)
            .fetch_one(pool.as_ref())
            .await
            .expect("the admitted run row");
    let context: Value =
        serde_json::from_slice(&context_json).expect("the durable run context parses");
    let frozen = context["data"]["core_context.text"]
        .as_str()
        .expect("the boundary freezes core_context.text into the run");
    for body in [
        "idea-one",
        "idea-two",
        "idea-three",
        "idea-four",
        "idea-five",
    ] {
        assert_eq!(
            frozen.matches(body).count(),
            1,
            "the boundary must consume the winning version with every body once: {frozen}"
        );
    }

    let inspected = fixture
        .handle
        .inspect_schedule(&principal, schedule_id.clone())
        .await
        .expect("inspect the admitted row");
    assert_eq!(inspected.schedule.status, "running");
    assert_eq!(
        inspected.schedule.current_session_id.as_deref(),
        Some(run_id.as_str()),
        "inspect must report the SAME run the schedule owns"
    );
    assert_eq!(inspected.schedule.current_core_context_version, 5);

    let sessions = fixture
        .handle
        .list_workflow_sessions(&principal, ListSessionsQuery::default())
        .await
        .expect("list sessions after admission");
    let listed = sessions
        .items
        .iter()
        .find(|item| item.session_id == run_id)
        .expect("the admitted run is listed from the durable store");
    assert_eq!(listed.creator_id, CREATOR);
    assert_eq!(listed.preset_id, PROMPT_PRESET);
    assert_eq!(listed.status, "running");

    let detail = fixture
        .handle
        .get_workflow_session(&principal, run_id.clone())
        .await
        .expect("the run detail is readable");
    assert_eq!(detail.session.session_id, run_id);
    assert_eq!(detail.session.creator_id, CREATOR);
    assert_eq!(detail.session.status, "running");

    // A foreign run, a child run and an unknown id are all equally closed: a
    // child is never independently authorized by naming it.
    sqlx::query(
        "INSERT INTO orchestration_sessions
           (session_id, creator_id, preset_id, preset_version, parent_session_id, status,
            context_json, created_at, updated_at)
         VALUES ('run-foreign', 'other_creator', ?, 1, NULL, 'running', '{}', 1, 1),
                ('run-child', ?, ?, 1, ?, 'running', '{}', 1, 1)",
    )
    .bind(PROMPT_PRESET)
    .bind(CREATOR)
    .bind(PROMPT_PRESET)
    .bind(&run_id)
    .execute(pool.as_ref())
    .await
    .expect("stage a foreign and a child run row");

    for (id, err) in [
        (
            "run-child".to_string(),
            fixture
                .handle
                .get_workflow_session(&principal, "run-child".to_string())
                .await
                .unwrap_err(),
        ),
        (
            "run-foreign".to_string(),
            fixture
                .handle
                .get_workflow_session(&principal, "run-foreign".to_string())
                .await
                .unwrap_err(),
        ),
        (
            "run-missing".to_string(),
            fixture
                .handle
                .get_workflow_session(&principal, "run-missing".to_string())
                .await
                .unwrap_err(),
        ),
    ] {
        match err {
            nexus_core::CoreError::NotFound { resource } => assert_eq!(
                resource,
                format!("workflow session {id}"),
                "a child, a foreign and an unknown run must close identically"
            ),
            other => panic!("run {id} must close as NotFound, got {other:?}"),
        }
    }
    let sessions = fixture
        .handle
        .list_workflow_sessions(&principal, ListSessionsQuery::default())
        .await
        .expect("list sessions with foreign/child rows present");
    assert_eq!(
        sessions.items.len(),
        1,
        "only the creator's own ROOT run is listed: {:?}",
        sessions.items
    );
    assert_eq!(sessions.items[0].session_id, run_id);

    // ── 6. The declared `status` filter narrows the PAGE and its COUNT
    //       identically: a filtered page is never paginated against an
    //       unfiltered total. ──
    // A second durable row of the SAME creator, left `pending` (the fixture
    // runs no clock, so only the explicit admission above owns a run).
    let pending_id = add_pending_schedule(&fixture, "retained-reads-pending").await;

    let running_only = fixture
        .handle
        .list_schedules(
            &principal,
            ListSchedulesQuery {
                status: Some("running".to_string()),
                ..ListSchedulesQuery::default()
            },
        )
        .await
        .expect("filter by status=running");
    assert_eq!(
        running_only
            .items
            .iter()
            .map(|item| item.schedule_id.as_str())
            .collect::<Vec<_>>(),
        vec![schedule_id.as_str()],
        "the status filter must return ONLY matching rows"
    );
    assert!(!running_only.pagination.has_more);

    // The decisive page/count agreement check: exactly ONE row matches
    // `pending`, so `limit = 1` exhausts the filter. A COUNT that ignored
    // `status` (two creator rows) would answer `has_more = true` here.
    let pending_page = fixture
        .handle
        .list_schedules(
            &principal,
            ListSchedulesQuery {
                status: Some("pending".to_string()),
                limit: Some(1),
                ..ListSchedulesQuery::default()
            },
        )
        .await
        .expect("filter by status=pending");
    assert_eq!(pending_page.items.len(), 1);
    assert_eq!(pending_page.items[0].schedule_id, pending_id);
    assert_eq!(pending_page.items[0].status, "pending");
    assert!(
        !pending_page.pagination.has_more,
        "the COUNT must apply the SAME status filter as the page: {:?}",
        pending_page.pagination
    );
    assert!(pending_page.pagination.next_cursor.is_none());

    let no_matches = fixture
        .handle
        .list_schedules(
            &principal,
            ListSchedulesQuery {
                status: Some("cancelled".to_string()),
                ..ListSchedulesQuery::default()
            },
        )
        .await
        .expect("filter by a status nothing has reached");
    assert!(
        no_matches.items.is_empty(),
        "a filter with no matches returns an empty page, never the unfiltered set: {:?}",
        no_matches.items
    );
    assert!(!no_matches.pagination.has_more);

    // ── 7. Each list's DEFAULT order is its OWN schema's declared default:
    //       schedules `created_at` (newest first), sessions `session_id`
    //       ascending. ──
    // Ordering anchors that CONTRADICT the other list's default: a schedule far
    // in the future/past, and two extra root runs whose `created_at` order is
    // the reverse of their id order.
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status, concurrency_kind,
            current_core_context_version, created_at, updated_at)
         VALUES ('SCH-zzz-future', ?, ?, 1, 'pending', 'serial', 0, 4102444800, 4102444800),
                ('SCH-aaa-past', ?, ?, 1, 'pending', 'serial', 0, 1, 1)",
    )
    .bind(CREATOR)
    .bind(PROMPT_PRESET)
    .bind(CREATOR)
    .bind(PROMPT_PRESET)
    .execute(pool.as_ref())
    .await
    .expect("stage schedule ordering anchors");
    sqlx::query(
        "INSERT INTO orchestration_sessions
           (session_id, creator_id, preset_id, preset_version, parent_session_id, status,
            context_json, created_at, updated_at)
         VALUES ('run-aaa', ?, ?, 1, NULL, 'running', '{}', 4102444800, 4102444800),
                ('run-zzz', ?, ?, 1, NULL, 'completed', '{}', 1, 1)",
    )
    .bind(CREATOR)
    .bind(PROMPT_PRESET)
    .bind(CREATOR)
    .bind(PROMPT_PRESET)
    .execute(pool.as_ref())
    .await
    .expect("stage run ordering anchors");

    let schedule_ids = |response: &nexus_contracts::generated::daemon_api::schedule::list_schedules_response::ListSchedulesResponse| {
        response
            .items
            .iter()
            .map(|item| item.schedule_id.clone())
            .collect::<Vec<_>>()
    };
    let default_schedules = schedule_ids(
        &fixture
            .handle
            .list_schedules(&principal, ListSchedulesQuery::default())
            .await
            .expect("default schedule order"),
    );
    let created_at_sorts = schedule_ids(
        &fixture
            .handle
            .list_schedules(
                &principal,
                ListSchedulesQuery {
                    sort: Some("-created_at".to_string()),
                    ..ListSchedulesQuery::default()
                },
            )
            .await
            .expect("explicit created_at order"),
    );
    assert_eq!(
        default_schedules, created_at_sorts,
        "the schedule default must be the schema's `created_at` default"
    );
    assert_eq!(
        default_schedules.first().map(String::as_str),
        Some("SCH-zzz-future"),
        "`created_at` descending puts the newest row first: {default_schedules:?}"
    );
    assert_eq!(
        default_schedules.last().map(String::as_str),
        Some("SCH-aaa-past"),
        "`created_at` descending puts the oldest row last: {default_schedules:?}"
    );
    assert!(
        !default_schedules.contains(&"SCH-foreign".to_string()),
        "the default order must still exclude another creator's row"
    );

    let session_ids = fixture
        .handle
        .list_workflow_sessions(&principal, ListSessionsQuery::default())
        .await
        .expect("default session order")
        .items
        .iter()
        .map(|item| item.session_id.clone())
        .collect::<Vec<_>>();
    assert_eq!(
        session_ids,
        vec![run_id.clone(), "run-aaa".to_string(), "run-zzz".to_string()],
        "the session default must be the schema's `session_id` ASCENDING default, \
         not the schedule default"
    );
    let by_session_id = fixture
        .handle
        .list_workflow_sessions(
            &principal,
            ListSessionsQuery {
                sort: Some("session_id".to_string()),
                ..ListSessionsQuery::default()
            },
        )
        .await
        .expect("explicit session_id order");
    assert_eq!(
        by_session_id
            .items
            .iter()
            .map(|item| item.session_id.clone())
            .collect::<Vec<_>>(),
        session_ids,
        "the session default must be the schema's `session_id` default"
    );
    let descending = fixture
        .handle
        .list_workflow_sessions(
            &principal,
            ListSessionsQuery {
                sort: Some("-session_id".to_string()),
                ..ListSessionsQuery::default()
            },
        )
        .await
        .expect("explicit descending session_id order");
    assert_eq!(
        descending
            .items
            .iter()
            .map(|item| item.session_id.as_str())
            .collect::<Vec<_>>(),
        vec!["run-zzz", "run-aaa", run_id.as_str()],
        "`-session_id` reverses the order, so the default above is the ASCENDING one"
    );
    // The staged anchors deliberately reverse under a `created_at` sort (their
    // `created_at` order is the reverse of their id order), so the ordering the
    // schedule default would have produced here is visibly different.
    assert_ne!(
        session_ids,
        vec!["run-aaa".to_string(), run_id.clone(), "run-zzz".to_string()],
        "the session default must not be a `created_at` ordering"
    );
    assert!(
        session_ids
            .iter()
            .all(|id| id != "run-foreign" && id != "run-child"),
        "the default order must still exclude foreign and child runs"
    );

    // ── 8. Cleanup: release the parked prompt, then close the owner. ──
    wait_for_prompt(&fixture.host).await;
    fixture.host.release_all();
    let report = fixture.handle.close().await.expect("owner close");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    fixture.core.close().await.expect("core close");
}

// ─────────────────────────────────────────────────────────────────────────────
// Public cancel/resume closure (v1.195 P0-T4)
// ─────────────────────────────────────────────────────────────────────────────

/// Owner-fixture preset for the cancel/resume seam: one prompt (so an
/// admission can be parked mid-step) followed by a MANUAL human wait (so the
/// run can never reach `Completed` on its own, and a plain resume has a real
/// wait it must refuse to continue).
const CANCEL_WAIT_PRESET: &str = "cancel-wait-guard";

fn cancel_wait_preset_yaml() -> String {
    format!(
        r#"
preset:
  id: {CANCEL_WAIT_PRESET}
  version: 1
  kind: creator
  description: "cancel/resume fixture — prompt then a manual human wait"
  requires_capabilities:
    - acp.prompt
  initial: parked_prompt
  terminal: done
states:
  - id: parked_prompt
    description: "prompt the parked host so the run is mid-step"
    enter:
      - kind: inner_graph
        name: prompt_graph
    exit_when: {{ kind: graph_complete }}
    next: human_wait
  - id: human_wait
    description: "manual wait: only its exact wait token may continue it"
    exit_when: {{ kind: manual }}
    next: done
  - id: done
    terminal: true

inner_graphs:
  prompt_graph:
    nodes:
      - id: guarded_prompt
        kind: acp_prompt
        template_file: prompts/generate.md
        tool_policy: deny_all
    output_binding: guarded_prompt.text
"#
    )
}

/// Hosted preset for the effect race: the prompt comes FIRST and the
/// authorized workspace commit LAST, so a cancel that lands while the prompt
/// is in flight must leave the commit unexecuted — the exact "late commit is
/// refused" ordering.
const CANCEL_EFFECT_PRESET: &str = "cancel-effect-guard";
/// The committed file, relative to the canonical creative root (the preset's
/// `workspace.open` scope is `notes`).
const CANCEL_EFFECT_FILE: &str = "notes/cancel-effect.txt";
const CANCEL_EFFECT_B64: &str = "Y2FuY2VsLWVmZmVjdCBwYXlsb2FkCg==";

fn cancel_effect_preset_yaml() -> String {
    format!(
        r#"
preset:
  id: {CANCEL_EFFECT_PRESET}
  version: 1
  kind: creator
  description: "cancel/effect fixture — prompt first, authorized commit last"
  requires_capabilities:
    - workspace.open
    - workspace.commit
    - acp.prompt
  initial: parked_prompt
  terminal: done
states:
  - id: parked_prompt
    description: "prompt the parked host so the run is mid-step before any effect"
    enter:
      - kind: inner_graph
        name: prompt_graph
    exit_when: {{ kind: graph_complete }}
    next: open_scope
  - id: open_scope
    description: "open a scope inside the factory-resolved creative root"
    enter:
      - kind: capability
        name: workspace.open
        args:
          path: notes
    exit_when: {{ kind: rule }}
    next: commit_scope
  - id: commit_scope
    description: "one declared create through the bound commit authority"
    enter:
      - kind: capability
        name: workspace.commit
        args:
          sessionId: "{{{{_capability_output.sessionId}}}}"
          changes:
            - path: cancel-effect.txt
              op: create
              contentBase64: "{CANCEL_EFFECT_B64}"
    exit_when: {{ kind: rule }}
    next: done
  - id: done
    terminal: true

inner_graphs:
  prompt_graph:
    nodes:
      - id: guarded_prompt
        kind: acp_prompt
        template_file: prompts/generate.md
        tool_policy: deny_all
    output_binding: guarded_prompt.text
"#
    )
}

/// The public cancel/resume closure (S0-3/S0-4/S0-6).
///
/// A cancel on a schedule that owns NO run is ONE CAS on the SAME fence the
/// admission claim writes on — so a later admission is refused and nothing is
/// minted, and a cancel that lands while an admission is in flight converges
/// on the durable winner either way. A cancel on an ADMITTED schedule routes
/// to the SAME owned run: a manual human wait is never implicitly continued
/// (the plain resume refuses and the exact wait token survives), the confirmed
/// cancel is the durable winner (never a provider acknowledgement), and the
/// LAST step of the preset — an authorized workspace commit — is refused once
/// the run is cancelled. A completed winner stays completed, and an
/// unconfirmed cleanup settles `interrupted` with the schedule row left
/// non-terminal until a retry confirms it.
#[allow(clippy::too_many_lines)] // one linear public journey; splitting hides the ordering evidence
#[tokio::test]
#[serial_test::serial]
async fn public_cancel_fences_late_workspace_commit() {
    // ═══ Part 1 — the admission fence, on an owner with NO owned clock, so
    //         every claim below is explicit and no tick can race the CAS. ═══
    let fixture = owner_fixture().await;
    let principal = fixture.core.active_principal().await.unwrap();
    let pool = fixture.pool();
    let coordinator = fixture.coordinator();
    let caps = fixture.handle.capability_holder();
    let home = fixture.nexus_home();
    let executor =
        fixture.executor.clone() as Arc<dyn nexus_orchestration::capability::PromptExecutor>;
    write_preset_bundle(
        &home,
        CANCEL_WAIT_PRESET,
        &cancel_wait_preset_yaml(),
        "Summarize the retained topic: {{preset.input.topic}}\n",
    );

    // ── 1. A pending row that owns NO run: the cancel is the fence, the row is
    //       durably cancelled with nothing behind it, and the LATER admission
    //       (the late commit) is refused without minting a run. ──
    let unowned = add_schedule_preset(
        &fixture.handle,
        &principal,
        CANCEL_WAIT_PRESET,
        "cancel-unowned",
    )
    .await;
    let cancelled = fixture
        .handle
        .signal_schedule(&principal, unowned.clone(), signal("cancel"))
        .await
        .expect("cancel the unowned row");
    assert_eq!(
        cancelled.status, "cancelled",
        "the response must carry the durable winner"
    );
    let (status, owned) = schedule_row(pool.as_ref(), &unowned).await;
    assert_eq!(status, "cancelled", "the row itself is durably cancelled");
    assert!(owned.is_none(), "a cancelled unowned row claims no run");
    assert_eq!(
        root_run_count(pool.as_ref()).await,
        0,
        "a cancelled unowned row must mint no run at all"
    );

    // The late commit: a cancelled row is no longer claimable, so the
    // admission that arrives after the cancel is refused — and minted nothing.
    let late_admission = coordinator
        .admit_schedule(
            &unowned,
            pool.as_ref(),
            &home,
            &caps,
            None,
            Some(executor.clone()),
        )
        .await;
    match late_admission {
        Err(RunControlError::NotEligible(id, reason)) => {
            assert_eq!(id, unowned, "the refusal names the row");
            assert!(
                reason.contains("cancelled"),
                "the refusal must name the durable cancel: {reason}"
            );
        }
        other => panic!("a cancelled row must refuse a later admission, got {other:?}"),
    }
    assert_eq!(
        root_run_count(pool.as_ref()).await,
        0,
        "a refused admission must leave no run row"
    );
    assert_eq!(
        fixture.host.prompt_count(),
        0,
        "no refused admission may reach the prompt port"
    );

    // ── 2. An ADMITTED schedule: the signal goes to the SAME owned run. A
    //       plain resume must NOT continue its manual human wait; the cancel
    //       settles it durably and the schedule never gains a second run. ──
    let admitted = add_schedule_preset(
        &fixture.handle,
        &principal,
        CANCEL_WAIT_PRESET,
        "cancel-admitted",
    )
    .await;
    let run_id = coordinator
        .admit_schedule(
            &admitted,
            pool.as_ref(),
            &home,
            &caps,
            None,
            Some(executor.clone()),
        )
        .await
        .expect("admission")
        .0;
    wait_for_prompt(&fixture.host).await;
    fixture.host.release_parked();
    let wait_id = wait_for_human_wait(pool.as_ref(), run_id.as_str()).await;
    let (status, owned) = schedule_row(pool.as_ref(), &admitted).await;
    assert_eq!(status, "running", "the admitted row owns its live run");
    assert_eq!(owned.as_deref(), Some(run_id.as_str()));

    // 2a. Manual waits are not implicitly continued: a plain resume refuses
    //     with the exact state conflict and the wait token SURVIVES.
    let refused = fixture
        .handle
        .signal_schedule(&principal, admitted.clone(), signal("resume"))
        .await
        .unwrap_err();
    match &refused {
        nexus_core::CoreError::Coded { code, message } => {
            assert_eq!(
                code, "workflow_state_conflict",
                "a plain resume on a human wait must be a state conflict, got {refused:?}"
            );
            assert!(
                message.contains("refuses the signal"),
                "the conflict must be the run's typed refusal: {message}"
            );
        }
        other => panic!("a plain resume must refuse, got {other:?}"),
    }
    let record = durable_record(pool.as_ref(), run_id.as_str()).await;
    let surviving_wait = record
        .state
        .as_ref()
        .and_then(|s| s.wait.as_ref())
        .map(|w| w.wait_id.clone());
    assert_eq!(
        surviving_wait.as_deref(),
        Some(wait_id.as_str()),
        "the manual wait must keep its exact token"
    );
    assert_eq!(root_run_count(pool.as_ref()).await, 1, "no second run");

    // 2b. The cancel reaches the SAME run and returns the durable winner.
    let cancelled = fixture
        .handle
        .signal_schedule(&principal, admitted.clone(), signal("cancel"))
        .await
        .expect("cancel the admitted row");
    assert_eq!(
        cancelled.status, "cancelled",
        "only a confirmed stop is a cancel success"
    );
    assert_eq!(
        durable_record(pool.as_ref(), run_id.as_str())
            .await
            .status
            .as_db_str(),
        "cancelled",
        "the owned run is durably cancelled"
    );
    let (status, owned) = schedule_row(pool.as_ref(), &admitted).await;
    assert_eq!(status, "cancelled", "the schedule projects the same winner");
    assert_eq!(
        owned.as_deref(),
        Some(run_id.as_str()),
        "the cancelled schedule still names the run it owned"
    );
    assert_eq!(
        root_run_count(pool.as_ref()).await,
        1,
        "cancel must not open a second workflow"
    );

    // ── 3. Cancel and admission contend for ONE unowned row. The CAS and the
    //       store's claim write on the SAME `current_session_id IS NULL`
    //       fence, so exactly one of them wins the row; whichever does, the
    //       cancel converges on a durable `cancelled` and the run behind a
    //       claim is terminal — never a cancelled row driving a live run. ──
    let racing = add_schedule_preset(
        &fixture.handle,
        &principal,
        CANCEL_WAIT_PRESET,
        "cancel-race",
    )
    .await;
    // The winner's run parks at the preset's prompt, and the cancel's bounded
    // teardown waits on that parked operation: release everything (permanently
    // armed) once the racing writers have started, so neither outcome can hang.
    let host = fixture.host.clone();
    let releaser = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(300)).await;
        host.release_all();
    });
    let admission = coordinator.admit_schedule(
        &racing,
        pool.as_ref(),
        &home,
        &caps,
        None,
        Some(executor.clone()),
    );
    let cancellation = fixture
        .handle
        .signal_schedule(&principal, racing.clone(), signal("cancel"));
    let (admitted_result, cancelled_result) = tokio::join!(admission, cancellation);
    let _ = releaser.await;
    assert_eq!(
        root_run_count(pool.as_ref()).await,
        2,
        "the race minted at most ONE further run"
    );
    match (&admitted_result, &cancelled_result) {
        // The cancel won the fence: the admission legitimately lost.
        (Err(RunControlError::NotEligible(id, _)), Ok(cancelled)) => {
            assert_eq!(id, &racing);
            assert_eq!(cancelled.status, "cancelled");
        }
        // The admission won the claim: the cancel reaches THAT claimed run.
        (Ok(_), Ok(cancelled)) => {
            assert_eq!(
                cancelled.status, "cancelled",
                "the claimed run's cancel converges on the durable winner"
            );
        }
        other => {
            panic!("a cancel/admission race must converge on one truthful winner, got {other:?}")
        }
    }
    let (status, owned) = schedule_row(pool.as_ref(), &racing).await;
    assert_eq!(
        status, "cancelled",
        "the raced row is durably cancelled whichever side won the fence"
    );
    if let Some(run_id) = &owned {
        assert_eq!(
            durable_record(pool.as_ref(), run_id)
                .await
                .status
                .as_db_str(),
            "cancelled",
            "a cancelled row must never be driving a live run"
        );
    }

    // Cleanup: release anything still parked, then close the owner.
    fixture.host.release_all();
    let report = fixture.handle.close().await.expect("owner close");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    fixture.core.close().await.expect("core close");

    // ═══ Part 2 — the effect race, on the production hosted owner whose
    //         authorized commit runs AFTER the prompt. ═══
    let hosted = hosted_fixture().await;
    let principal = hosted.core.active_principal().await.unwrap();
    let pool = hosted.handle.coordinator().pool();
    let nexus_home = hosted.tmp.path().join(".nexus42");
    write_preset_bundle(
        &nexus_home,
        CANCEL_EFFECT_PRESET,
        &cancel_effect_preset_yaml(),
        "Summarize the hosted topic: {{preset.input.topic}}\n",
    );

    // ── 4. Cancel/effect race: the cancel lands while the prompt is in flight,
    //       so the LAST step — the authorized workspace commit — never runs. ──
    let prompts_before = hosted.host.prompt_count();
    let fenced = add_schedule_preset(
        &hosted.handle,
        &principal,
        CANCEL_EFFECT_PRESET,
        "cancel-effect-fenced",
    )
    .await;
    let fenced_run = wait_for_schedule_run(pool.as_ref(), &fenced).await;
    wait_for_prompt_count(&hosted.host, prompts_before + 1).await;
    let cancelled = tokio::spawn({
        let handle = hosted.handle.clone();
        let principal = principal.clone();
        let schedule_id = fenced.clone();
        async move {
            handle
                .signal_schedule(&principal, schedule_id, signal("cancel"))
                .await
        }
    });
    // The engine persists the durable cancel intent BEFORE it fires the run
    // token: once that fence is durable the cancel is already the durable
    // control winner, and the parked prompt is only unwinding.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let record = durable_record(pool.as_ref(), &fenced_run).await;
        if record.state.as_ref().is_some_and(|s| s.cancel_requested) {
            break;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the cancel intent never became durable"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    hosted.host.release_parked();
    let cancelled = cancelled
        .await
        .expect("cancel task joins")
        .expect("a confirmed cancel");
    assert_eq!(
        cancelled.status, "cancelled",
        "the in-flight prompt's teardown confirms the stop"
    );
    // The late commit: the cancelled run never reaches the commit state, the
    // run is durably cancelled, and the schedule projects the same winner.
    assert!(
        !hosted.root.join(CANCEL_EFFECT_FILE).exists(),
        "the authorized commit AFTER the cancel must be refused"
    );
    wait_for_run_status(pool.as_ref(), &fenced_run, "cancelled").await;
    assert_eq!(
        schedule_row(pool.as_ref(), &fenced).await.0,
        "cancelled",
        "the schedule row projects the durable cancel"
    );

    // ── 5. Unconfirmed cleanup is NOT a cancel success: the run settles
    //       `interrupted`, the schedule row stays non-terminal, and a retry
    //       cancel (with the owned shutdown confirmable again) settles it. ──
    let prompts_before = hosted.host.prompt_count();
    let unconfirmed = add_schedule_preset(
        &hosted.handle,
        &principal,
        CANCEL_EFFECT_PRESET,
        "cancel-unconfirmed",
    )
    .await;
    let unconfirmed_run = wait_for_schedule_run(pool.as_ref(), &unconfirmed).await;
    wait_for_prompt_count(&hosted.host, prompts_before + 1).await;
    hosted.host.fail_session_shutdown();
    let interrupted = tokio::spawn({
        let handle = hosted.handle.clone();
        let principal = principal.clone();
        let schedule_id = unconfirmed.clone();
        async move {
            handle
                .signal_schedule(&principal, schedule_id, signal("cancel"))
                .await
        }
    });
    hosted.host.release_parked();
    let interrupted = interrupted
        .await
        .expect("cancel task joins")
        .expect("the unconfirmed cancel is still a typed answer");
    assert_eq!(
        interrupted.status, "interrupted",
        "unconfirmed cleanup must never report `cancelled`"
    );
    let record = durable_record(pool.as_ref(), &unconfirmed_run).await;
    assert_eq!(
        record.status.as_db_str(),
        "interrupted",
        "the durable winner is the unconfirmed cleanup"
    );
    assert!(
        record.state.as_ref().is_some_and(|s| s.cancel_requested),
        "the interrupted run keeps its durable cancel intent"
    );
    assert_eq!(
        schedule_row(pool.as_ref(), &unconfirmed).await.0,
        "running",
        "the schedule row stays non-terminal while cleanup is unconfirmed"
    );

    // The retry: the same signal confirms the owned teardown and settles.
    hosted.host.confirm_session_shutdown();
    let confirmed = hosted
        .handle
        .signal_schedule(&principal, unconfirmed.clone(), signal("cancel"))
        .await
        .expect("the retry cancel confirms");
    assert_eq!(
        confirmed.status, "cancelled",
        "the retry is the confirmation"
    );
    assert_eq!(
        schedule_row(pool.as_ref(), &unconfirmed).await.0,
        "cancelled",
        "the schedule row settles only on the confirmed cancel"
    );

    // ── 6. A completed winner stays completed: the cancel of a settled run is
    //       a typed conflict and relabels nothing. ──
    let prompts_before = hosted.host.prompt_count();
    let completed = add_schedule_preset(
        &hosted.handle,
        &principal,
        CANCEL_EFFECT_PRESET,
        "cancel-completed",
    )
    .await;
    let completed_run = wait_for_schedule_run(pool.as_ref(), &completed).await;
    wait_for_prompt_count(&hosted.host, prompts_before + 1).await;
    hosted.host.release_parked();
    wait_for_run_status(pool.as_ref(), &completed_run, "completed").await;
    // The effect that linearized BEFORE the cancel stays committed.
    let committed = hosted.root.join(CANCEL_EFFECT_FILE);
    assert_eq!(
        std::fs::read(&committed).expect("the committed bytes"),
        b"cancel-effect payload\n".as_slice(),
        "a completed effect that linearized before the cancel remains committed"
    );
    let late_cancel = hosted
        .handle
        .signal_schedule(&principal, completed.clone(), signal("cancel"))
        .await
        .unwrap_err();
    match &late_cancel {
        nexus_core::CoreError::Coded { code, message } => {
            assert_eq!(
                code, "workflow_state_conflict",
                "a raced completion is never relabelled cancelled, got {late_cancel:?}"
            );
            let _ = message; // the durable detail is asserted on the row below
        }
        other => panic!("a completed winner must refuse the cancel, got {other:?}"),
    }
    assert_eq!(
        schedule_row(pool.as_ref(), &completed).await.0,
        "completed",
        "the completed winner stays completed"
    );
    assert_eq!(
        durable_record(pool.as_ref(), &completed_run)
            .await
            .status
            .as_db_str(),
        "completed"
    );

    // Cleanup: release anything parked, then close the hosted owner.
    hosted.host.release_all();
    let report = hosted.handle.close().await.expect("owner close");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    hosted.core.close().await.expect("core close");
}

// ─────────────────────────────────────────────────────────────────────────────
// Paused-row reconciliation on a successful same-run resume (v1.195 P0-T4 C1)
// ─────────────────────────────────────────────────────────────────────────────

/// The C1 fixture: an unreachable second branch parks the converge join at
/// 1/2 arrivals with NO deadline, so the run settles durably `paused`
/// (tokenless, no human wait, no in-flight marker) and no driver survives it —
/// the one durable shape on which a plain `resume` may legally move the run
/// back to `running`.
const RESUME_PARK_PRESET: &str = "resume-park-guard";

fn resume_park_preset_yaml() -> String {
    format!(
        r#"
preset:
  id: {RESUME_PARK_PRESET}
  version: 1
  kind: creator
  description: "resume fixture — an unreachable branch parks the join, no deadline"
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
    description: "Hanging upstream edge — never walked, never arrives"
    next: join
  - id: join
    converge: {{ strategy: wait_for_all }}
    next: done
  - id: done
    terminal: true
"#
    )
}

/// Add one `driven_v1` pending schedule for `preset_id` that may run
/// CONCURRENTLY with the creator's other rows (`parallel_any`), so the race
/// below is not gated by the serial capacity an earlier live row still holds.
async fn add_parallel_any_schedule(
    handle: &ExecutionHandle,
    principal: &Principal,
    preset_id: &str,
    label: &str,
) -> String {
    use nexus_contracts::local::schedule::http::ScheduleConcurrencyRequest;

    handle
        .add_schedule(
            principal,
            AddScheduleRequest {
                creator_id: CREATOR.to_string(),
                preset_id: preset_id.to_string(),
                seed: None,
                label: Some(label.to_string()),
                depends_on: None,
                concurrency: Some(ScheduleConcurrencyRequest::ParallelAny),
                scheduled_at: None,
                input: Some(json!({ "topic": "retained-cancel" })),
                force_gates: false,
                reason: None,
                agent_bindings: Some(default_bindings()),
            },
        )
        .await
        .expect("schedule insert")
        .schedule_id
}

/// Wait (bounded) until the run sits in a SIGNALABLE converge-gate park.
///
/// The durable `paused` status alone is not enough: a mid-drive inter-task
/// boundary pause is also stored as `paused`, and there the NEXT step is
/// already marked in flight (a plain resume would be fenced, correctly). The
/// steady park is the one the gated task writes its state-scoped
/// `_gate_park_<state>` marker for (the engine's exact park evidence) while the
/// run carries no wait and no in-flight marker, and the drive loop has stopped
/// on it.
async fn wait_for_signalable_gate_park(pool: &sqlx::SqlitePool, run_id: &str) {
    use graph_flow::SessionStorage as _;

    let store = SqliteSessionStorage::new(Arc::new(pool.clone()));
    let session_id = nexus_orchestration::engine::SessionId(run_id.to_string());
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let record = store
            .load_run(&session_id)
            .await
            .expect("durable run load")
            .expect("the admitted run row exists");
        let idle = record.state.as_ref().is_none_or(|state| {
            state.wait.is_none() && state.step_in_flight.is_none() && state.in_flight.is_none()
        });
        if record.status.as_db_str() == "paused" && idle {
            let parked = store
                .get(run_id)
                .await
                .expect("session load")
                .is_some_and(|session| {
                    session
                        .context
                        .get::<bool>(&format!("_gate_park_{}", session.current_task_id))
                        .is_some_and(|live| live)
                });
            if parked {
                return;
            }
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never reached a signalable converge-gate park: status {} state {:?}",
            record.status.as_db_str(),
            record.state,
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// C1 regression: a successful same-run resume reconciles the owning schedule
/// row (S0-4/W6).
///
/// `pause` flips the durable row to `paused` without touching the run it owns,
/// so before the fix an admitted resume returned the run's durable `running`
/// while `creator_schedules.status` — the column BOTH public projections read
/// — kept claiming `paused`. The journey below stages exactly that (a paused
/// row still owning its parked run), resumes the SAME run, and then asserts the
/// durable row follows the run: list AND inspect show the post-resume status,
/// the identity is unchanged (one root run, same `current_session_id`), and a
/// cancel that races the resume leaves the TERMINAL winner standing. The
/// resume also re-drives that same root (P0-T6), so the case additionally pins
/// the re-drive: the parked run is stepped again and re-parks, instead of
/// sitting at `running` with nobody driving it.
#[allow(clippy::too_many_lines)] // one linear public journey; splitting hides the ordering evidence
#[tokio::test]
#[serial_test::serial]
async fn public_resume_reconciles_the_paused_schedule_row() {
    use nexus_contracts::generated::daemon_api::schedule::list_schedules_query::ListSchedulesQuery;

    let fixture = owner_fixture().await;
    let principal = fixture.core.active_principal().await.unwrap();
    let pool = fixture.pool();
    let coordinator = fixture.coordinator();
    let caps = fixture.handle.capability_holder();
    let home = fixture.nexus_home();
    let executor =
        fixture.executor.clone() as Arc<dyn nexus_orchestration::capability::PromptExecutor>;
    write_preset_bundle(
        &home,
        RESUME_PARK_PRESET,
        &resume_park_preset_yaml(),
        "unused by this preset\n",
    );

    // ── 1. Stage the C1 state through PUBLIC paths: an admitted schedule that
    //       owns its parked run, then `pause` — the row reads `paused` while
    //       the run it owns is still live. ──
    let paused_schedule = add_schedule_preset(
        &fixture.handle,
        &principal,
        RESUME_PARK_PRESET,
        "resume-paused-row",
    )
    .await;
    let run_id = coordinator
        .admit_schedule(
            &paused_schedule,
            pool.as_ref(),
            &home,
            &caps,
            None,
            Some(executor.clone()),
        )
        .await
        .expect("admission")
        .0;
    wait_for_signalable_gate_park(pool.as_ref(), &run_id).await;
    let (status, owned) = schedule_row(pool.as_ref(), &paused_schedule).await;
    assert_eq!(status, "running", "the admission claim owns its live run");
    assert_eq!(owned.as_deref(), Some(run_id.as_str()));

    let paused = fixture
        .handle
        .signal_schedule(&principal, paused_schedule.clone(), signal("pause"))
        .await
        .expect("pause the admitted row");
    assert_eq!(paused.status, "paused", "pause flips the durable row");
    assert_eq!(
        schedule_row(pool.as_ref(), &paused_schedule).await.0,
        "paused",
        "the row is durably paused before the resume"
    );

    // ── 2. Resume the SAME run: the durable row must follow the run it owns —
    //       never the other way round (no fabricated `running`). The resume
    //       also RE-DRIVES that same root (P0-T6): the status mutation alone
    //       would leave the parked run's driver unrestarted, so `running`
    //       would name a run nobody drives. ──
    let resumed = fixture
        .handle
        .signal_schedule(&principal, paused_schedule.clone(), signal("resume"))
        .await
        .expect("resume the paused row's own run");
    assert_eq!(
        resumed.status, "running",
        "the response carries the durable RUN status"
    );
    // The re-drive is the difference between a status flip and a driven run:
    // this gate can never be satisfied, so the honest post-resume outcome is
    // the SAME run parking again — which only a driver that really stepped it
    // can produce (without the re-drive the run would sit at `running` with no
    // owner, and this wait would time out).
    wait_for_signalable_gate_park(pool.as_ref(), &run_id).await;
    assert_eq!(
        durable_record(pool.as_ref(), &run_id)
            .await
            .status
            .as_db_str(),
        "paused",
        "the re-driven run settles back on the unsatisfiable gate, same run"
    );
    let (status, owned) = schedule_row(pool.as_ref(), &paused_schedule).await;
    assert_eq!(
        status, "running",
        "the paused row is reconciled to the run that resumed"
    );
    assert_eq!(
        owned.as_deref(),
        Some(run_id.as_str()),
        "the same run is still the one this schedule owns"
    );
    assert_eq!(
        root_run_count(pool.as_ref()).await,
        1,
        "a resume must never mint a second workflow"
    );

    // ── 3. Both public projections read the reconciled durable row. ──
    let listed = fixture
        .handle
        .list_schedules(&principal, ListSchedulesQuery::default())
        .await
        .expect("list schedules")
        .items
        .into_iter()
        .find(|item| item.schedule_id == paused_schedule)
        .expect("the resumed schedule is listed");
    assert_eq!(
        listed.status, "running",
        "list must show the post-resume durable state, not the stale `paused`"
    );
    assert_eq!(listed.current_session_id.as_deref(), Some(run_id.as_str()));

    let inspected = fixture
        .handle
        .inspect_schedule(&principal, paused_schedule.clone())
        .await
        .expect("inspect the resumed schedule");
    assert_eq!(
        inspected.schedule.status, "running",
        "inspect must show the same post-resume durable state"
    );
    assert_eq!(
        inspected.schedule.current_session_id.as_deref(),
        Some(run_id.as_str())
    );

    // ── 4. The reconciliation is fenced: a cancel racing the resume wins the
    //       row, and the terminal winner is never overwritten. ──
    let raced = add_parallel_any_schedule(
        &fixture.handle,
        &principal,
        RESUME_PARK_PRESET,
        "resume-raced-cancel",
    )
    .await;
    let raced_run = coordinator
        .admit_schedule(
            &raced,
            pool.as_ref(),
            &home,
            &caps,
            None,
            Some(executor.clone()),
        )
        .await
        .expect("admission")
        .0;
    wait_for_signalable_gate_park(pool.as_ref(), &raced_run).await;
    fixture
        .handle
        .signal_schedule(&principal, raced.clone(), signal("pause"))
        .await
        .expect("pause the raced row");
    assert_eq!(schedule_row(pool.as_ref(), &raced).await.0, "paused");

    let (resumed_race, cancelled_race) = tokio::join!(
        fixture
            .handle
            .signal_schedule(&principal, raced.clone(), signal("resume")),
        fixture
            .handle
            .signal_schedule(&principal, raced.clone(), signal("cancel")),
    );
    match &resumed_race {
        // The resume won the run first: its response still carries a durable
        // run status (the cancel may already have moved the run on).
        Ok(r) => assert!(
            matches!(r.status.as_str(), "running" | "cancelled"),
            "a resume response carries a durable run status, got {r:?}"
        ),
        // The cancel landed first: the resume is a typed conflict, never a
        // fabricated success.
        Err(e) => assert!(
            matches!(e, nexus_core::CoreError::Coded { .. }),
            "a resume that lost the run to the cancel must be a typed conflict, got {e:?}"
        ),
    }
    let cancelled_race = cancelled_race.expect("the cancel is the terminal winner");
    assert_eq!(cancelled_race.status, "cancelled");
    assert_eq!(
        schedule_row(pool.as_ref(), &raced).await.0,
        "cancelled",
        "the raced terminal winner must never be overwritten as `running`"
    );
    assert_eq!(
        durable_record(pool.as_ref(), &raced_run)
            .await
            .status
            .as_db_str(),
        "cancelled",
        "the raced run is durably cancelled"
    );
    assert_eq!(
        root_run_count(pool.as_ref()).await,
        2,
        "the race minted no extra run"
    );

    let report = fixture.handle.close().await.expect("owner close");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    fixture.core.close().await.expect("core close");
}

// ─────────────────────────────────────────────────────────────────────────────
// Close vs. admitted durable commit (v1.195 P0-T5 close findings)
// ─────────────────────────────────────────────────────────────────────────────

/// The file the close-vs-commit regressions commit into the selected root.
///
/// Scope-relative: the session is opened on `notes`, so the manifest path is
/// `drained.txt` and the file lands at `notes/drained.txt` (the same shape the
/// hosted preset's own `workspace.commit` uses).
const DRAINED_PATH: &str = "drained.txt";

/// How long a caller's cleanup budget is simulated to last.
///
/// The native close wrapper wraps `cleanup_owners -> CoreService::close` in a
/// 5s budget; the regressions below use a shorter one because the property
/// under test is cancellation safety, not the exact duration.
const CALLER_CLEANUP_BUDGET: std::time::Duration = std::time::Duration::from_millis(500);

/// The observation window for "this close must NOT have finished yet".
///
/// NOT the race fence — the armed [`test_hooks::OwnerGate`] is: it parks the
/// retained commit owner at its admission boundary, so a correct close
/// provably cannot finish while the gate is held. The window only gives an
/// INCORRECT close the time to prove it by finishing during the hold.
const CLOSE_OBSERVATION_WINDOW: std::time::Duration = std::time::Duration::from_millis(750);

/// Whether `task` finished within `window` (checked without consuming it).
#[cfg(feature = "test-hooks")]
async fn finished_within<T>(
    task: &mut tokio::task::JoinHandle<T>,
    window: std::time::Duration,
) -> bool {
    tokio::time::timeout(window, async {
        loop {
            if task.is_finished() {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .is_ok()
}

/// Admit ONE durable workspace commit through the PUBLIC handle op, park the
/// retained owner at its admission boundary with the test gate, and drop the
/// awaiting client.
///
/// This is the exact shape of a client disconnect mid-commit: the owner holds
/// its durable claim and keeps running, while nothing is left awaiting it.
#[cfg(feature = "test-hooks")]
async fn admit_commit_and_drop_the_client(
    fixture: &HostedFixture,
) -> (Arc<test_hooks::OwnerGate>, String) {
    let principal = fixture.core.active_principal().await.unwrap();
    let manager = Arc::clone(
        fixture
            .handle
            .workspace_commit_authority()
            .expect("the hosted owner is bound to a commit authority")
            .manager(),
    );
    let root = fixture.root.to_string_lossy().into_owned();
    let session = manager
        .open_session(&root, "notes", true)
        .await
        .expect("open a commit session");
    let session_id = session.to_string();

    let gate = Arc::new(test_hooks::OwnerGate::for_session(session_id.clone()));
    test_hooks::set_owner_gate(Some(Arc::clone(&gate)));

    let request = CoreWorkspaceCommitRequest {
        session_id: session_id.parse().expect("non-empty session id"),
        changes: vec![CoreWorkspaceCommitRequestChangesItem {
            content_base64: Some(HOSTED_PAYLOAD_B64.to_string()),
            expected_hash: None,
            op: CoreWorkspaceCommitRequestChangesItemOp::Create,
            path: DRAINED_PATH.parse().expect("non-empty path"),
        }],
    };
    let mut committer = {
        let handle = Arc::clone(&fixture.handle);
        tokio::spawn(async move { handle.commit_workspace(&principal, request).await })
    };
    // Deterministic: the owner holds its durable claim (nothing applied yet).
    // A refusal instead of the rendezvous is a TEST failure, never a hang.
    tokio::select! {
        () = gate.admitted.notified() => {}
        refused = &mut committer => panic!(
            "the commit was refused before its admission boundary: {:?}",
            refused
        ),
    }
    // Drop the awaiting client; the retained owner keeps applying.
    committer.abort();
    let _ = committer.await;
    (gate, session_id)
}

/// The durable revision of the commit `session_id` settled, read through a
/// LATER owner's admitted pool.
#[cfg(feature = "test-hooks")]
async fn settled_revision(core: &CoreService, session_id: &str) -> String {
    let pool = core.pool();
    let digest = nexus_local_db::get_committed_request_digest(pool, session_id)
        .await
        .expect("read the committed digest")
        .expect("the session's commit is durably recorded");
    nexus_local_db::get_committed_intent_by_digest(pool, session_id, &digest)
        .await
        .expect("read the committed intent")
        .expect("the committed digest names an intent")
        .revision
}

/// A confirmed close WAITS for every already-admitted durable workspace commit.
///
/// `handle.commit_workspace` runs on a RETAINED owner task that outlives its
/// awaiting caller (client disconnect, shutdown), so the drive drain alone
/// cannot cover it: a commit admitted just before `close()` must reach its
/// durable conclusion BEFORE the workspace authority is released, or a new
/// owner could recover/use the same root while the old commit was still
/// applying.
#[cfg(feature = "test-hooks")]
#[tokio::test]
#[serial_test::serial]
async fn close_drains_an_admitted_durable_commit_before_releasing_the_authority() {
    let fixture = hosted_fixture().await;
    let home = fixture.tmp.path().to_path_buf();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let epoch_a = fixture.handle.engine_epoch();
    let (gate, session_id) = admit_commit_and_drop_the_client(&fixture).await;

    // Close now. While the admitted commit is still applying, NO confirmed
    // close, NO released lease and NO replacement owner may appear.
    let mut closer = {
        let core = fixture.core.clone();
        tokio::spawn(async move { core.close().await })
    };
    assert!(
        !finished_within(&mut closer, CLOSE_OBSERVATION_WINDOW).await,
        "close reported a settled state while an admitted durable commit was still applying"
    );
    assert!(
        !fixture.handle.is_settled(),
        "the owner settled while an admitted durable commit was still applying"
    );
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_err(),
        "the workspace authority was released before the admitted commit settled"
    );

    // Release the gate: the commit applies, and only THEN does close confirm.
    gate.proceed.notify_one();
    gate.settled.notified().await;
    let report = closer
        .await
        .expect("the close task joins")
        .expect("a close report");
    assert_eq!(report.state, nexus_contracts::CoreCloseReportState::Closed);
    assert!(
        report.cleanup_confirmed,
        "the close confirms only after the admitted commit settled"
    );
    test_hooks::set_owner_gate(None);

    // The commit the disconnect left behind is durably applied ...
    assert_eq!(
        std::fs::read(fixture.root.join("notes").join(DRAINED_PATH))
            .expect("the committed bytes are durable"),
        HOSTED_PAYLOAD
    );

    // ... and the NEXT owner over the same home is a NEW admission (advanced
    // engine epoch) that reads that commit's durable revision.
    let (core_b, _host_b, handle_b) = open_hosted_owner(&home).await;
    assert!(
        handle_b.engine_epoch() > epoch_a,
        "the next owner must be a NEW admission ({} -> {})",
        epoch_a,
        handle_b.engine_epoch()
    );
    let revision = settled_revision(&core_b, &session_id).await;
    assert!(
        revision.starts_with("rev_"),
        "the next owner reads the settled revision, got {revision}"
    );
    handle_b.close().await.expect("close the next owner");
    core_b.close().await.expect("close the next core");
}

/// An INTERRUPTED close neither confirms nor releases anything, and the retry
/// waits for the REAL drain instead of fabricating a confirmation.
///
/// The native cleanup budget cancels `CoreService::close()`'s future. The
/// drain the first close started must survive that cancellation, and a later
/// close must observe the real settlement — never report a close that never
/// happened while the admitted commit still runs.
#[cfg(feature = "test-hooks")]
#[tokio::test]
#[serial_test::serial]
async fn an_interrupted_close_is_retried_honestly() {
    let fixture = hosted_fixture().await;
    let home = fixture.tmp.path().to_path_buf();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let epoch_a = fixture.handle.engine_epoch();
    let (gate, session_id) = admit_commit_and_drop_the_client(&fixture).await;

    // The caller's cleanup budget expires with the commit still applying: the
    // close future is DROPPED, exactly as the native close wrapper drops it.
    let interrupted = tokio::time::timeout(CALLER_CLEANUP_BUDGET, fixture.core.close()).await;
    assert!(
        interrupted.is_err(),
        "close must not settle while an admitted durable commit is still applying"
    );

    // Nothing was confirmed and nothing was released.
    assert!(
        !fixture.handle.is_settled(),
        "an interrupted close must retain the owner it did not settle"
    );
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_err(),
        "an interrupted close must retain the workspace authority"
    );

    // The RETRY must not fabricate a confirmation either.
    let retried = tokio::time::timeout(CALLER_CLEANUP_BUDGET, fixture.core.close()).await;
    assert!(
        retried.is_err(),
        "a retried close must wait for the retained drain, never report a close that never happened"
    );
    assert!(
        !fixture.handle.is_settled(),
        "a retried close must not settle the owner it never drained"
    );
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_err(),
        "a retried close must not release the authority before the commit settled"
    );

    // Release the gate: the retained drain finishes, and the retry confirms
    // the REAL settlement.
    gate.proceed.notify_one();
    gate.settled.notified().await;
    let report = fixture.core.close().await.expect("a close report");
    assert_eq!(report.state, nexus_contracts::CoreCloseReportState::Closed);
    assert!(
        report.cleanup_confirmed,
        "the retry confirms the drain that actually ran"
    );
    test_hooks::set_owner_gate(None);
    assert_eq!(
        std::fs::read(fixture.root.join("notes").join(DRAINED_PATH))
            .expect("the committed bytes are durable"),
        HOSTED_PAYLOAD
    );

    // The home is free again: a fresh owner takes a NEW admission.
    let (core_b, _host_b, handle_b) = open_hosted_owner(&home).await;
    assert!(
        handle_b.engine_epoch() > epoch_a,
        "the next owner must be a NEW admission ({} -> {})",
        epoch_a,
        handle_b.engine_epoch()
    );
    let revision = settled_revision(&core_b, &session_id).await;
    assert!(
        revision.starts_with("rev_"),
        "the next owner reads the settled revision, got {revision}"
    );
    handle_b.close().await.expect("close the next owner");
    core_b.close().await.expect("close the next core");
}

// ─────────────────────────────────────────────────────────────────────────────
// Close vs. the manager's OWN recoverable entrances (v1.195 P0-T5)
// ─────────────────────────────────────────────────────────────────────────────

/// One create entry in the manager's own manifest shape.
#[cfg(feature = "test-hooks")]
fn direct_change(path: &str, content_base64: &str) -> WorkspaceChangeEntry {
    WorkspaceChangeEntry {
        path: path.to_string(),
        op: WorkspaceChangeOp::Create,
        expected_hash: None,
        content_base64: Some(content_base64.to_string()),
    }
}

/// The retained manager of the fixture's hosted owner, as a caller that keeps
/// it past the owner's close would hold it.
#[cfg(feature = "test-hooks")]
fn retained_manager(fixture: &HostedFixture) -> Arc<WorkspaceSessionManager> {
    Arc::clone(
        fixture
            .handle
            .workspace_commit_authority()
            .expect("the hosted owner is bound to a commit authority")
            .manager(),
    )
}

/// Every FILE under the selected root, sorted: `(relative path, bytes)`.
///
/// The byte-level half of "nothing was mutated": a refused entrance that still
/// applied a change would show up here even if the durable rows were ignored.
#[cfg(feature = "test-hooks")]
fn root_files(root: &Path) -> Vec<(String, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<(String, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
            } else {
                let relative = path
                    .strip_prefix(root)
                    .expect("walked paths stay under the root")
                    .to_string_lossy()
                    .into_owned();
                out.push((relative, std::fs::read(&path).expect("read a root file")));
            }
        }
    }
    let mut out = Vec::new();
    walk(root, root, &mut out);
    out.sort();
    out
}

/// The durable workspace-commit state over `pool`: session consumption and
/// intent rows, as one comparable value.
#[cfg(feature = "test-hooks")]
async fn commit_state(pool: &sqlx::SqlitePool) -> Vec<String> {
    let mut out = Vec::new();
    let sessions: Vec<(String, i64)> =
        sqlx::query_as("SELECT session_id, consumed FROM workspace_sessions ORDER BY session_id")
            .fetch_all(pool)
            .await
            .expect("session rows");
    for (session_id, consumed) in sessions {
        out.push(format!("session {session_id} consumed={consumed}"));
    }
    let intents: Vec<(String, String, String)> = sqlx::query_as(
        "SELECT session_id, revision, state FROM workspace_commit_intents ORDER BY revision",
    )
    .fetch_all(pool)
    .await
    .expect("intent rows");
    for (session_id, revision, state) in intents {
        out.push(format!("intent {session_id} {revision} {state}"));
    }
    out
}

/// The manager's OWN durable commit entrance is covered by the owner close.
///
/// `ExecutionHandle::workspace_commit_authority()` hands out the shared
/// `WorkspaceSessionManager`, so a caller that keeps it can start a recoverable
/// mutation that never goes through the handle route. Such a mutation must
/// register in the SAME admission counter the close drains — otherwise the
/// close can release the workspace lease (and a replacement owner can take the
/// same root) while this commit is still applying.
#[cfg(feature = "test-hooks")]
#[tokio::test]
#[serial_test::serial]
async fn a_direct_durable_commit_is_drained_before_the_close_releases_the_authority() {
    let fixture = hosted_fixture().await;
    let home = fixture.tmp.path().to_path_buf();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let epoch_a = fixture.handle.engine_epoch();
    let manager = retained_manager(&fixture);
    let root = fixture.root.to_string_lossy().into_owned();
    let session = manager
        .open_session(&root, "notes", true)
        .await
        .expect("open a commit session");
    let session_id = session.to_string();

    let gate = Arc::new(test_hooks::OwnerGate::for_session(session_id.clone()));
    test_hooks::set_owner_gate(Some(Arc::clone(&gate)));

    // The manager's OWN entrance, NOT the handle's `commit_workspace`.
    let mut direct = {
        let manager = Arc::clone(&manager);
        let session = session.clone();
        let root = root.clone();
        tokio::spawn(async move {
            manager
                .commit_session_durable(
                    &session,
                    &[direct_change(DRAINED_PATH, HOSTED_PAYLOAD_B64)],
                    &root,
                )
                .await
        })
    };
    tokio::select! {
        () = gate.admitted.notified() => {}
        refused = &mut direct => panic!(
            "the direct commit was refused before its admission boundary: {refused:?}"
        ),
    }

    // Close while the direct commit is parked mid-apply: no settled close, no
    // released lease and no replacement authority may appear.
    let mut closer = {
        let core = fixture.core.clone();
        tokio::spawn(async move { core.close().await })
    };
    assert!(
        !finished_within(&mut closer, CLOSE_OBSERVATION_WINDOW).await,
        "close settled while a direct durable commit through the retained manager was still applying"
    );
    assert!(
        !fixture.handle.is_settled(),
        "the owner settled while a direct durable commit was still applying"
    );
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_err(),
        "the workspace authority was released before the direct commit settled"
    );

    // Release the gate: the admitted direct commit reaches its durable
    // conclusion, and only THEN does the close confirm.
    gate.proceed.notify_one();
    let outcome = direct
        .await
        .expect("the direct commit task joins")
        .expect("the admitted direct commit still settles durably");
    assert!(
        outcome.committed,
        "the direct commit reports a durable commit"
    );
    let report = closer
        .await
        .expect("the close task joins")
        .expect("a close report");
    assert_eq!(report.state, nexus_contracts::CoreCloseReportState::Closed);
    assert!(
        report.cleanup_confirmed,
        "the close confirms only after the direct commit settled"
    );
    test_hooks::set_owner_gate(None);
    assert_eq!(
        std::fs::read(fixture.root.join("notes").join(DRAINED_PATH))
            .expect("the committed bytes are durable"),
        HOSTED_PAYLOAD
    );

    // The next owner over the same home is a NEW admission that reads the
    // revision the direct commit settled.
    let (core_b, _host_b, handle_b) = open_hosted_owner(&home).await;
    assert!(
        handle_b.engine_epoch() > epoch_a,
        "the next owner must be a NEW admission ({} -> {})",
        epoch_a,
        handle_b.engine_epoch()
    );
    let revision = settled_revision(&core_b, &session_id).await;
    assert!(
        revision.starts_with("rev_"),
        "the next owner reads the settled revision, got {revision}"
    );
    handle_b.close().await.expect("close the next owner");
    core_b.close().await.expect("close the next core");
}

/// Which direct recoverable entrance a caller-cancellation regression drives.
#[cfg(feature = "test-hooks")]
#[derive(Clone, Copy)]
enum DirectEntrance {
    /// `WorkspaceSessionManager::commit_session_durable`.
    Durable,
    /// The recoverable branch of `WorkspaceSessionManager::commit_session`.
    Plain,
}

impl DirectEntrance {
    /// Run this entrance from a caller task; the result is discarded because
    /// these regressions CANCEL the caller before it can observe it.
    async fn call(
        self,
        manager: &Arc<WorkspaceSessionManager>,
        session: &SessionId,
        path: &str,
        root: &str,
    ) -> Result<(), String> {
        let changes = [direct_change(path, HOSTED_PAYLOAD_B64)];
        match self {
            Self::Durable => manager
                .commit_session_durable(session, &changes, root)
                .await
                .map(|_| ())
                .map_err(|err| err.to_string()),
            Self::Plain => manager
                .commit_session(session, &changes, root)
                .await
                .map(|_| ())
                .map_err(|err| err.to_string()),
        }
    }
}

/// A CANCELLED caller of the manager's OWN recoverable entrance must not take
/// its admitted commit out of the owner's close drain.
///
/// The direct entrances are reachable through `WorkspaceCommitAuthority::manager()`
/// and can apply workspace bytes, so they must hold their admission on the
/// RETAINED owner task — not on the awaiting caller's future. A caller dropped
/// at the POST-INTENT / PRE-FILE barrier (claim held, nothing applied yet) used
/// to release the guard with its own future: the close then drained nothing,
/// reported `closed`/`cleanup_confirmed` and released the workspace authority
/// while that commit was still in flight, so a replacement owner could take the
/// same root beside it. Red before the retained-owner fix at the first
/// `finished_within` assertion below.
#[cfg(feature = "test-hooks")]
async fn cancelled_direct_caller_is_drained_by_the_close(entrance: DirectEntrance) {
    let fixture = hosted_fixture().await;
    let home = fixture.tmp.path().to_path_buf();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let epoch_a = fixture.handle.engine_epoch();
    let manager = retained_manager(&fixture);
    let root = fixture.root.to_string_lossy().into_owned();
    let session = manager
        .open_session(&root, "notes", true)
        .await
        .expect("open a commit session");
    let session_id = session.to_string();

    let gate = Arc::new(test_hooks::OwnerGate::for_session(session_id.clone()));
    test_hooks::set_owner_gate(Some(Arc::clone(&gate)));

    // The manager's OWN entrance, NOT the handle's `commit_workspace`.
    let mut direct = {
        let manager = Arc::clone(&manager);
        let session = session.clone();
        let root = root.clone();
        tokio::spawn(async move { entrance.call(&manager, &session, DRAINED_PATH, &root).await })
    };
    tokio::select! {
        () = gate.admitted.notified() => {}
        refused = &mut direct => panic!(
            "the direct commit was refused before its admission boundary: {refused:?}"
        ),
    }

    // The barrier is POST-INTENT / PRE-FILE: the claim is held and no byte of
    // the workspace has changed yet.
    let parked = commit_state(fixture.core.pool()).await;
    assert!(
        parked
            .iter()
            .any(|row| row.contains(&session_id) && row.contains("applying")),
        "the parked direct commit must hold its durable claim: {parked:?}"
    );
    assert!(
        !fixture.root.join("notes").join(DRAINED_PATH).exists(),
        "the barrier must be reached before any file is applied"
    );

    // CANCEL the awaiting caller, exactly as a dropped client or a shutdown
    // that abandons the future does.
    direct.abort();
    assert!(
        direct
            .await
            .expect_err("the aborted caller cannot report a commit")
            .is_cancelled(),
        "the caller task must have been cancelled at the barrier"
    );

    // Close the OWNER now (the service stays open, so the durable rows can be
    // re-read after the confirmation). While the commit the cancelled caller
    // left behind is still applying, NO confirmed close, NO released lease and
    // NO replacement owner may appear.
    let mut closer = {
        let handle = Arc::clone(&fixture.handle);
        tokio::spawn(async move { handle.close().await })
    };
    assert!(
        !finished_within(&mut closer, CLOSE_OBSERVATION_WINDOW).await,
        "close settled after a cancelled direct caller left its admitted commit in flight"
    );
    assert!(
        !fixture.handle.is_settled(),
        "the owner settled while the cancelled caller's commit was still applying"
    );
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_err(),
        "the workspace authority was released before the cancelled caller's commit settled"
    );

    // Release the gate: the retained commit reaches its durable conclusion, and
    // only THEN does the close confirm. The bytes are already durable when the
    // close reports, so nothing can land after it.
    gate.proceed.notify_one();
    gate.settled.notified().await;
    let drained_files = root_files(&fixture.root);
    let drained_state = commit_state(fixture.core.pool()).await;
    assert_eq!(
        std::fs::read(fixture.root.join("notes").join(DRAINED_PATH))
            .expect("the drained commit applied its bytes"),
        HOSTED_PAYLOAD
    );
    let report = closer
        .await
        .expect("the close task joins")
        .expect("a close report");
    assert_eq!(report.state, nexus_contracts::CoreCloseReportState::Closed);
    assert!(
        report.cleanup_confirmed,
        "the close confirms only after the cancelled caller's commit settled"
    );
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_ok(),
        "the authority is released only after the drained commit settled"
    );
    test_hooks::set_owner_gate(None);

    // NO LATE WRITE after the confirmed close: the workspace bytes and the
    // durable rows the drained commit produced are unchanged across the close.
    assert_eq!(
        root_files(&fixture.root),
        drained_files,
        "a workspace write landed after the confirmed close"
    );
    assert_eq!(
        commit_state(fixture.core.pool()).await,
        drained_state,
        "a durable write landed after the confirmed close"
    );

    // The service close seals the same boundary the owner close settled, then
    // the next owner over the same home is a NEW admission that reads the
    // revision the drained commit settled.
    fixture.core.close().await.expect("the core close");
    let (core_b, _host_b, handle_b) = open_hosted_owner(&home).await;
    assert!(
        handle_b.engine_epoch() > epoch_a,
        "the next owner must be a NEW admission ({} -> {})",
        epoch_a,
        handle_b.engine_epoch()
    );
    let revision = settled_revision(&core_b, &session_id).await;
    assert!(
        revision.starts_with("rev_"),
        "the next owner reads the settled revision, got {revision}"
    );
    handle_b.close().await.expect("close the next owner");
    core_b.close().await.expect("close the next core");
}

/// `commit_session_durable` through the manager: a cancelled caller keeps the
/// commit inside the owner's drain (see the scenario above).
#[cfg(feature = "test-hooks")]
#[tokio::test]
#[serial_test::serial]
async fn a_cancelled_direct_durable_caller_is_drained_before_the_close_releases() {
    cancelled_direct_caller_is_drained_by_the_close(DirectEntrance::Durable).await;
}

/// `commit_session`'s recoverable branch through the manager: a cancelled
/// caller keeps the commit inside the owner's drain (see the scenario above).
#[cfg(feature = "test-hooks")]
#[tokio::test]
#[serial_test::serial]
async fn a_cancelled_direct_plain_caller_is_drained_before_the_close_releases() {
    cancelled_direct_caller_is_drained_by_the_close(DirectEntrance::Plain).await;
}

/// Which retained recovery entrance a caller-cancellation regression drives.
#[cfg(feature = "test-hooks")]
#[derive(Clone, Copy)]
enum RecoveryEntrance {
    /// `WorkspaceSessionManager::startup_recovery` (whole DB).
    All,
    /// `WorkspaceSessionManager::startup_recovery_for_root` (the production
    /// bundle's entry).
    Root,
}

impl RecoveryEntrance {
    /// Run this entrance from a caller task; the result is discarded because
    /// these regressions CANCEL the caller before it can observe it.
    async fn call(self, manager: &Arc<WorkspaceSessionManager>, root: &str) -> Result<(), String> {
        match self {
            Self::All => manager
                .startup_recovery()
                .await
                .map_err(|err| err.to_string()),
            Self::Root => manager
                .startup_recovery_for_root(root)
                .await
                .map_err(|err| err.to_string()),
        }
    }
}

/// Leave ONE crash-consistent unsettled intent behind, the way a process death
/// mid-apply does: claim held, workspace bytes applied, no committed transition.
///
/// Driven through the manager's own durable entrance, whose RETAINED owner
/// clears an inherited crash seam at spawn — so the seam is armed from inside
/// its admission rendezvous (no sleeps, no races).
#[cfg(feature = "test-hooks")]
async fn leave_unsettled_after_apply(manager: &Arc<WorkspaceSessionManager>, root: &str) -> String {
    let session = manager
        .open_session(root, "notes", true)
        .await
        .expect("open a commit session");
    let session_id = session.to_string();
    let gate = Arc::new(test_hooks::OwnerGate::for_session(session_id.clone()));
    test_hooks::set_owner_gate(Some(Arc::clone(&gate)));
    let caller = {
        let manager = Arc::clone(manager);
        let session = session.clone();
        let root = root.to_string();
        tokio::spawn(async move {
            manager
                .commit_session_durable(
                    &session,
                    &[direct_change(DRAINED_PATH, HOSTED_PAYLOAD_B64)],
                    &root,
                )
                .await
        })
    };
    gate.admitted.notified().await;
    test_hooks::set_crash_point(Some("after_file_apply"));
    gate.proceed.notify_one();
    // Await the caller's own result, not a settle signal: the crash ends the
    // commit either way, and the interrupted state is fully durable by then.
    let crashed = caller.await.expect("the retained commit owner joins");
    test_hooks::set_owner_gate(None);
    assert!(
        matches!(crashed, Err(SessionError::Internal(_))),
        "the armed crash point must leave the commit unsettled: {crashed:?}"
    );
    test_hooks::set_crash_point(None);
    session_id
}

/// A CANCELLED caller of a recoverable RECOVERY entrance must not take its
/// admitted pass out of the owner's close drain.
///
/// A recovery pass applies or rolls back workspace bytes exactly like a commit,
/// so it needs the same retained-owner admission: a caller dropped at the
/// settlement boundary (crash-consistent intent on disk, nothing settled yet)
/// used to release the guard with its own future, letting the close report
/// `closed`/`cleanup_confirmed` and release the workspace authority while the
/// pass was still in flight. Red before the retained-owner fix at the first
/// `finished_within` assertion below.
#[cfg(feature = "test-hooks")]
async fn cancelled_recovery_caller_is_drained_by_the_close(entrance: RecoveryEntrance) {
    let fixture = hosted_fixture().await;
    let home = fixture.tmp.path().to_path_buf();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let epoch_a = fixture.handle.engine_epoch();
    let manager = retained_manager(&fixture);
    let root = fixture.root.to_string_lossy().into_owned();
    let session_id = leave_unsettled_after_apply(&manager, &root).await;

    // The crash-consistent state the pass is about to settle.
    let unsettled = commit_state(fixture.core.pool()).await;
    assert!(
        unsettled
            .iter()
            .any(|row| row.contains(&session_id) && row.contains("applying")),
        "the seeded intent must be unsettled: {unsettled:?}"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("notes").join(DRAINED_PATH))
            .expect("the interrupted commit applied its bytes"),
        HOSTED_PAYLOAD
    );

    let gate = Arc::new(test_hooks::RecoveryGate::new());
    test_hooks::set_recovery_gate(Some(Arc::clone(&gate)));

    let mut recovery = {
        let manager = Arc::clone(&manager);
        let root = root.clone();
        tokio::spawn(async move { entrance.call(&manager, &root).await })
    };
    tokio::select! {
        () = gate.admitted.notified() => {}
        refused = &mut recovery => panic!(
            "the recovery pass was refused before its settlement boundary: {refused:?}"
        ),
    }

    // CANCEL the awaiting caller, exactly as a dropped client or a shutdown
    // that abandons the future does.
    recovery.abort();
    assert!(
        recovery
            .await
            .expect_err("the aborted caller cannot report a recovery pass")
            .is_cancelled(),
        "the caller task must have been cancelled at the settlement boundary"
    );

    // Close the OWNER now (the service stays open, so the durable rows can be
    // re-read after the confirmation). While the pass the cancelled caller left
    // behind is still running, NO confirmed close, NO released lease and NO
    // replacement owner may appear.
    let mut closer = {
        let handle = Arc::clone(&fixture.handle);
        tokio::spawn(async move { handle.close().await })
    };
    assert!(
        !finished_within(&mut closer, CLOSE_OBSERVATION_WINDOW).await,
        "close settled after a cancelled recovery caller left its admitted pass in flight"
    );
    assert!(
        !fixture.handle.is_settled(),
        "the owner settled while the cancelled caller's recovery pass was still running"
    );
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_err(),
        "the workspace authority was released before the cancelled caller's recovery pass settled"
    );

    // Release the gate: the retained pass settles the intent, and only THEN
    // does the close confirm.
    gate.proceed.notify_one();
    gate.settled.notified().await;
    let drained_files = root_files(&fixture.root);
    let drained_state = commit_state(fixture.core.pool()).await;
    assert!(
        drained_state
            .iter()
            .any(|row| row.contains(&session_id) && row.contains("committed")),
        "the drained recovery pass settled the interrupted commit: {drained_state:?}"
    );
    let report = closer
        .await
        .expect("the close task joins")
        .expect("a close report");
    assert_eq!(report.state, nexus_contracts::CoreCloseReportState::Closed);
    assert!(
        report.cleanup_confirmed,
        "the close confirms only after the cancelled caller's recovery pass settled"
    );
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_ok(),
        "the authority is released only after the drained recovery pass settled"
    );
    test_hooks::set_recovery_gate(None);

    // NO LATE WRITE after the confirmed close: the workspace bytes and the
    // durable rows the drained pass produced are unchanged across the close.
    assert_eq!(
        root_files(&fixture.root),
        drained_files,
        "a workspace write landed after the confirmed close"
    );
    assert_eq!(
        commit_state(fixture.core.pool()).await,
        drained_state,
        "a durable write landed after the confirmed close"
    );

    // The service close seals the same boundary the owner close settled, then
    // the next owner over the same home is a NEW admission that reads the
    // revision the recovered commit settled.
    fixture.core.close().await.expect("the core close");
    let (core_b, _host_b, handle_b) = open_hosted_owner(&home).await;
    assert!(
        handle_b.engine_epoch() > epoch_a,
        "the next owner must be a NEW admission ({} -> {})",
        epoch_a,
        handle_b.engine_epoch()
    );
    let revision = settled_revision(&core_b, &session_id).await;
    assert!(
        revision.starts_with("rev_"),
        "the next owner reads the recovered revision, got {revision}"
    );
    handle_b.close().await.expect("close the next owner");
    core_b.close().await.expect("close the next core");
}

/// Whole-DB `startup_recovery` through the manager: a cancelled caller keeps
/// the pass inside the owner's drain (see the scenario above).
#[cfg(feature = "test-hooks")]
#[tokio::test]
#[serial_test::serial]
async fn a_cancelled_recovery_caller_is_drained_before_the_close_releases() {
    cancelled_recovery_caller_is_drained_by_the_close(RecoveryEntrance::All).await;
}

/// Root-scoped `startup_recovery_for_root` (the production bundle's entry): a
/// cancelled caller keeps the pass inside the owner's drain (see above).
#[cfg(feature = "test-hooks")]
#[tokio::test]
#[serial_test::serial]
async fn a_cancelled_root_scoped_recovery_caller_is_drained_before_the_close_releases() {
    cancelled_recovery_caller_is_drained_by_the_close(RecoveryEntrance::Root).await;
}

/// After a CONFIRMED close, EVERY direct recoverable entrance on the retained
/// manager fails closed with a typed refusal and mutates nothing.
///
/// The handle route (`commit_workspace`) is only one way in; `commit_session`,
/// `commit_session_durable`, `startup_recovery` and `startup_recovery_for_root`
/// are reachable through `WorkspaceCommitAuthority::manager()` and can apply or
/// roll back workspace bytes. `ExecutionHandle::close()` drains its admitted
/// work and RELEASES the workspace authority while the service (and its pool)
/// stays open, so an entrance that is not in the same admission counter can
/// still write the same root after that release.
#[cfg(feature = "test-hooks")]
#[tokio::test]
#[serial_test::serial]
async fn every_direct_recoverable_entrance_refuses_after_a_confirmed_close() {
    let fixture = hosted_fixture().await;
    let home = fixture.tmp.path().to_path_buf();
    let db_path = nexus_home_layout::workspace_state_db_path(&home, CREATOR, SLUG);
    let manager = retained_manager(&fixture);
    let root = fixture.root.to_string_lossy().into_owned();

    // CONTROL: the same entrances work while the owner is live, so a refusal
    // below is the close boundary and not a broken call.
    let live = manager
        .open_session(&root, "notes", true)
        .await
        .expect("open the live control session");
    let live_outcome = manager
        .commit_session_durable(
            &live,
            &[direct_change("pre-close.txt", HOSTED_PAYLOAD_B64)],
            &root,
        )
        .await
        .expect("the direct durable entrance works while the owner is live");
    assert!(
        live_outcome.committed,
        "the live control commit is durable before the close"
    );

    // Sessions prepared for the post-close attempts, plus the durable state the
    // refusals must leave byte-identical (real rows: the control commit's
    // session and its settled intent are in it).
    let durable_session = manager
        .open_session(&root, "notes", true)
        .await
        .expect("open the durable session");
    let plain_session = manager
        .open_session(&root, "notes", true)
        .await
        .expect("open the plain session");
    let before_files = root_files(&fixture.root);
    let before_state = commit_state(fixture.core.pool()).await;

    // The OWNER close: confirmed, drain complete, and the workspace authority
    // released — while the service keeps its pool open.
    let report = fixture.handle.close().await.expect("the owner close");
    assert!(
        report.cleanup_confirmed,
        "the owner close is confirmed before the refusals are exercised"
    );
    assert!(fixture.handle.is_settled(), "the owner is settled");
    assert!(
        WorkspaceAuthorityLease::acquire(&db_path).is_ok(),
        "a confirmed owner close releases the workspace authority"
    );

    assert_direct_entrances_refused(&manager, &root, &durable_session, &plain_session).await;
    assert_eq!(
        root_files(&fixture.root),
        before_files,
        "a refused direct entrance changed the workspace bytes"
    );
    assert_eq!(
        commit_state(fixture.core.pool()).await,
        before_state,
        "a refused direct entrance consumed a session or claimed an intent"
    );

    // The SERVICE close seals the same boundary once more, this time over a
    // closed pool: the refusal stays the typed one instead of borrowing the
    // pool's own failure.
    let core_report = fixture.core.close().await.expect("the core close");
    assert!(
        core_report.cleanup_confirmed,
        "the core close is confirmed before the second refusal pass"
    );
    assert_direct_entrances_refused(&manager, &root, &durable_session, &plain_session).await;
    assert_eq!(
        root_files(&fixture.root),
        before_files,
        "a refused direct entrance changed the workspace bytes after the core close"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Authorized bounded workflow event subscription (v1.195 P1-T1)
// ─────────────────────────────────────────────────────────────────────────────

/// Root runs the subscription cases use. The authorization source is the
/// durable `orchestration_sessions` row, so each case stages exactly the
/// ownership it asserts on.
const SUBSCRIBE_RUN: &str = "run-subscribed";
const FOREIGN_RUN: &str = "run-foreign";
const CHILD_RUN: &str = "run-subscribed:child:step";
const UNKNOWN_RUN: &str = "run-absent";
const SLOW_RUN: &str = "run-slow-consumer";
const BLOCK_RUN: &str = "run-blocked-pull";

/// Stage one durable `orchestration_sessions` row (the subscription's
/// authorization source). A `parent_session_id` makes it a CHILD row.
async fn stage_run_row(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    creator_id: &str,
    parent_session_id: Option<&str>,
) {
    sqlx::query(
        "INSERT INTO orchestration_sessions
           (session_id, creator_id, preset_id, preset_version, parent_session_id,
            status, context_json, created_at, updated_at)
         VALUES (?, ?, 'hosted-schedule-drive', 1, ?, 'running', x'', 1, 1)",
    )
    .bind(run_id)
    .bind(creator_id)
    .bind(parent_session_id)
    .execute(pool)
    .await
    .expect("stage a durable workflow run row");
}

/// One durable run record, with a `state_revision` that identifies the frame it
/// becomes on the wire.
fn run_record(run_id: &str, state_revision: u64) -> RunRecord {
    RunRecord {
        session_id: nexus_orchestration::engine::SessionId(run_id.to_string()),
        status: SessionStatus::Running,
        state_revision,
        execution_version: 1,
        descriptor: None,
        state: None,
        graph_version: 1,
    }
}

/// Publish one `run_state` frame through the owner's own run-event port (the
/// same call the coordinator makes at every durable revision).
fn publish_run_state(port: &Arc<dyn RunEventPort>, run_id: &str, state_revision: u64) {
    port.publish_run_state(run_id, &run_record(run_id, state_revision));
}

fn subscribe_request(run_id: &str, last_event_id: Option<&str>) -> CoreWorkflowSubscribeRequest {
    CoreWorkflowSubscribeRequest {
        run_id: CoreWorkflowSubscribeRequestRunId::try_from(run_id.to_string()).expect("run id"),
        last_event_id: last_event_id.map(|cursor| {
            CoreWorkflowSubscribeRequestLastEventId::try_from(cursor.to_string()).expect("cursor")
        }),
    }
}

/// The opaque token as the transport would pass it back.
fn subscription_token(subscription: &CoreWorkflowSubscription) -> String {
    subscription.subscription_id.as_str().to_string()
}

/// The numeric sequence a frame id carries (`<epoch>:<sequence>`).
fn frame_sequence(id: &str) -> u64 {
    id.rsplit_once(':')
        .expect("frame id carries an epoch and a sequence")
        .1
        .parse()
        .expect("numeric sequence")
}

/// The epoch a frame id carries.
fn frame_epoch(id: &str) -> String {
    id.rsplit_once(':')
        .expect("frame id carries an epoch and a sequence")
        .0
        .to_string()
}

fn frame_json(item: &CoreWorkflowEventBatchEventsItem) -> Value {
    serde_json::from_str(&item.data).expect("frame payload is JSON")
}

/// The `state_revision` a published `run_state` frame carries.
fn frame_revision(item: &CoreWorkflowEventBatchEventsItem) -> u64 {
    frame_json(item)["state_revision"]
        .as_u64()
        .expect("run_state frames carry a state revision")
}

/// The authorized bounded subscription (contract §4, S1-1/S1-2/S1-4).
///
/// Discriminates the whole frame/error table on ONE real owner: an unknown,
/// foreign or child run closes as absent BEFORE the ring is read (the foreign
/// run's ring is live and holds a frame, so a ring-first implementation would
/// have served it), the replay hands off to the live tail exactly once, a
/// prior-epoch cursor yields the single `history_unavailable` control frame
/// instead of an error, a retention-trimmed tail and a lagging subscriber both
/// surface as EXPLICIT gaps, and a released or closed owner wakes a blocked
/// pull with `closed` and withdraws every token.
#[allow(clippy::too_many_lines)] // one linear contract; splitting hides the ordering evidence
#[tokio::test]
#[serial_test::serial]
async fn authorized_subscription_preserves_epoch_and_gap() {
    use nexus_core::execution::run_events::MAX_PULL_FRAMES;
    use std::num::NonZeroU64;
    use std::time::Duration;

    let fixture = hosted_fixture().await;
    let principal = fixture.core.active_principal().await.unwrap();
    let handle = Arc::clone(&fixture.handle);
    let pool = handle.coordinator().pool();
    let port = handle
        .coordinator()
        .run_event_port()
        .expect("the hosted factory attaches the run-event registry");

    // ── 1. Staged durable ownership + the live rings the refusals must not
    //       reach. ──
    stage_run_row(pool.as_ref(), SUBSCRIBE_RUN, CREATOR, None).await;
    stage_run_row(pool.as_ref(), FOREIGN_RUN, "other_creator", None).await;
    stage_run_row(pool.as_ref(), CHILD_RUN, CREATOR, Some(SUBSCRIBE_RUN)).await;
    for run_id in [SUBSCRIBE_RUN, FOREIGN_RUN] {
        assert!(
            port.try_register_live(run_id).await,
            "the run-event ring must exist for {run_id}"
        );
        publish_run_state(&port, run_id, 1);
    }

    // ── 2. Unknown, foreign and child runs close BEFORE any ring lookup. ──
    for run_id in [UNKNOWN_RUN, FOREIGN_RUN, CHILD_RUN] {
        let refusal = handle
            .subscribe_workflow_events(&principal, subscribe_request(run_id, None))
            .await
            .unwrap_err();
        match refusal {
            nexus_core::CoreError::NotFound { resource } => assert_eq!(
                resource,
                format!("workflow session {run_id}"),
                "an unauthorized run must close exactly like an absent one: {run_id}"
            ),
            other => panic!("{run_id} must close as NotFound, got {other:?}"),
        }
    }

    // The numeric page method is gated by the SAME rule — the legacy read is
    // not a bypass around the subscription's authorization.
    let page_request = nexus_contracts::CoreRunEventsRequest {
        after_sequence: None,
        limit: NonZeroU64::new(64).expect("non-zero"),
        run_id: nexus_contracts::CoreRunEventsRequestRunId::try_from(FOREIGN_RUN.to_string())
            .expect("run id"),
    };
    let page_refusal = handle
        .run_events(&principal, page_request)
        .await
        .unwrap_err();
    assert!(
        matches!(page_refusal, nexus_core::CoreError::NotFound { .. }),
        "the bounded page read must refuse a foreign run before its ring: {page_refusal:?}"
    );

    // ── 3. Authorized subscription: retained replay, then the live tail, with
    //       an exclusive cursor and no duplicate handoff. ──
    for revision in 2..=3 {
        publish_run_state(&port, SUBSCRIBE_RUN, revision);
    }
    let first = handle
        .subscribe_workflow_events(&principal, subscribe_request(SUBSCRIBE_RUN, None))
        .await
        .expect("an owned root run subscribes");
    let first_id = subscription_token(&first);
    let replay = handle
        .next_workflow_events(&principal, first_id.clone())
        .await
        .expect("the first pull");
    assert!(
        !replay.closed,
        "a live run's stream stays open after its replay"
    );
    assert_eq!(
        replay.events.iter().map(frame_revision).collect::<Vec<_>>(),
        vec![1, 2, 3],
        "the retained frames replay in ring order"
    );
    let handed_off = replay.events.last().expect("a replayed frame").id.clone();

    for revision in 4..=6 {
        publish_run_state(&port, SUBSCRIBE_RUN, revision);
    }
    let tail = handle
        .next_workflow_events(&principal, first_id.clone())
        .await
        .expect("the live tail");
    assert_eq!(
        tail.events.iter().map(frame_revision).collect::<Vec<_>>(),
        vec![4, 5, 6],
        "the live tail continues the same stream without repeating the replay"
    );

    let resumed = handle
        .subscribe_workflow_events(
            &principal,
            subscribe_request(SUBSCRIBE_RUN, Some(&handed_off)),
        )
        .await
        .expect("a retained cursor resumes");
    let resumed_id = subscription_token(&resumed);
    let resumed_replay = handle
        .next_workflow_events(&principal, resumed_id.clone())
        .await
        .expect("the resumed replay");
    assert_eq!(
        resumed_replay
            .events
            .iter()
            .map(frame_revision)
            .collect::<Vec<_>>(),
        vec![4, 5, 6],
        "the cursor is EXCLUSIVE: the frame it names is never re-sent"
    );
    assert!(
        resumed_replay
            .events
            .iter()
            .all(|event| event.id != handed_off),
        "no duplicate handoff frame: {resumed_replay:?}"
    );
    publish_run_state(&port, SUBSCRIBE_RUN, 7);
    let live = handle
        .next_workflow_events(&principal, resumed_id.clone())
        .await
        .expect("the resumed live tail");
    assert_eq!(
        live.events.iter().map(frame_revision).collect::<Vec<_>>(),
        vec![7],
        "the resumed attachment receives only what follows its replay"
    );

    for token in [first_id.clone(), resumed_id.clone()] {
        handle
            .release_workflow_events(&principal, token)
            .expect("release");
    }
    assert!(
        matches!(
            handle
                .next_workflow_events(&principal, first_id.clone())
                .await
                .unwrap_err(),
            nexus_core::CoreError::NotFound { .. }
        ),
        "a released token refuses"
    );

    // ── 4. Cursor validation is typed; an unresumable history is a control
    //       frame, not an error. ──
    let malformed = handle
        .subscribe_workflow_events(
            &principal,
            subscribe_request(SUBSCRIBE_RUN, Some("not-a-cursor")),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&malformed, nexus_core::CoreError::InvalidInput { field, .. } if field == "last_event_id"),
        "a malformed cursor must be a typed invalid-input refusal: {malformed:?}"
    );
    let future_cursor = format!("{}:9999", frame_epoch(&handed_off));
    let future = handle
        .subscribe_workflow_events(
            &principal,
            subscribe_request(SUBSCRIBE_RUN, Some(&future_cursor)),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&future, nexus_core::CoreError::InvalidInput { field, .. } if field == "last_event_id"),
        "a future cursor must be a typed invalid-input refusal: {future:?}"
    );

    // A different epoch is a PREVIOUS core generation: the run is still owned
    // and retained, but its history cannot be resumed.
    let stale_cursor = format!("{}:3", uuid::Uuid::nil());
    let stale = handle
        .subscribe_workflow_events(
            &principal,
            subscribe_request(SUBSCRIBE_RUN, Some(&stale_cursor)),
        )
        .await
        .expect("a prior-epoch cursor still opens a subscription");
    let stale_id = subscription_token(&stale);
    let control = handle
        .next_workflow_events(&principal, stale_id.clone())
        .await
        .expect("the control frame");
    assert!(control.closed, "history_unavailable closes the stream");
    assert_eq!(control.events.len(), 1, "exactly one control frame");
    let unavailable = &control.events[0];
    assert_eq!(
        unavailable.event.as_str(),
        "history_unavailable",
        "the frame keeps the retained control vocabulary: {unavailable:?}"
    );
    assert_eq!(
        unavailable.id, "",
        "the control frame carries NO cursor, so a verbatim transport never advances Last-Event-ID"
    );
    let wire = frame_json(unavailable);
    assert_eq!(wire["run_id"], SUBSCRIBE_RUN);
    assert_eq!(
        wire["inspect_url"],
        format!("/v1/daemon/orchestration/sessions/{SUBSCRIBE_RUN}"),
        "the control frame names the run's public inspect route"
    );
    assert!(
        matches!(
            handle
                .next_workflow_events(&principal, stale_id)
                .await
                .unwrap_err(),
            nexus_core::CoreError::NotFound { .. }
        ),
        "a closed stream's token is withdrawn"
    );

    // ── 5. Retention trimming is an EXPLICIT gap over the dropped range, never
    //       fabricated content. ──
    let published = MAX_RECORDS_PER_RUN as u64 + 50;
    for revision in 100..(100 + published) {
        publish_run_state(&port, SUBSCRIBE_RUN, revision);
    }
    let trimmed = handle
        .subscribe_workflow_events(&principal, subscribe_request(SUBSCRIBE_RUN, None))
        .await
        .expect("a trimmed ring still subscribes");
    let trimmed_id = subscription_token(&trimmed);
    let trimmed_batch = handle
        .next_workflow_events(&principal, trimmed_id.clone())
        .await
        .expect("the trimmed pull");
    let gap_frame = trimmed_batch
        .events
        .first()
        .expect("at least the explicit gap");
    assert_eq!(
        gap_frame.event.as_str(),
        "gap",
        "a trimmed tail is announced, never silently re-based: {gap_frame:?}"
    );
    let dropped = frame_json(gap_frame);
    assert_eq!(dropped["from_sequence"], json!(1));
    let first_retained = trimmed_batch
        .events
        .get(1)
        .expect("retained frames follow the gap");
    let first_retained_sequence = frame_sequence(&first_retained.id);
    assert!(
        first_retained_sequence > 1,
        "the ring must actually have trimmed for this case to mean anything"
    );
    assert_eq!(
        dropped["to_sequence"].as_u64().map(|to| to + 1),
        Some(first_retained_sequence),
        "the gap covers EXACTLY the dropped range and nothing else"
    );
    let delivered: Vec<u64> = trimmed_batch.events[1..]
        .iter()
        .map(frame_revision)
        .collect();
    assert!(
        delivered.windows(2).all(|pair| pair[0] < pair[1]),
        "delivered frames stay in published order: {delivered:?}"
    );
    assert!(
        delivered
            .iter()
            .all(|revision| (100..100 + published).contains(revision)),
        "only frames the ring actually retained are delivered: {delivered:?}"
    );
    assert!(
        trimmed_batch.events.len() <= MAX_PULL_FRAMES,
        "a pull is capped at {MAX_PULL_FRAMES} frames"
    );
    handle
        .release_workflow_events(&principal, trimmed_id)
        .expect("release");

    // ── 6. A lagging subscriber is EVICTED with an explicit terminal gap on
    //       its own run, where every sequence is known. ──
    stage_run_row(pool.as_ref(), SLOW_RUN, CREATOR, None).await;
    assert!(port.try_register_live(SLOW_RUN).await, "slow-run ring");
    publish_run_state(&port, SLOW_RUN, 1);
    let seed = handle
        .subscribe_workflow_events(&principal, subscribe_request(SLOW_RUN, None))
        .await
        .expect("seed subscription");
    let seed_id = subscription_token(&seed);
    let seed_batch = handle
        .next_workflow_events(&principal, seed_id.clone())
        .await
        .expect("seed pull");
    let slow_cursor = seed_batch.events.last().expect("a seed frame").id.clone();
    assert_eq!(
        frame_sequence(&slow_cursor),
        1,
        "the ring starts at sequence 1"
    );
    handle
        .release_workflow_events(&principal, seed_id)
        .expect("release the seed");

    let slow = handle
        .subscribe_workflow_events(&principal, subscribe_request(SLOW_RUN, Some(&slow_cursor)))
        .await
        .expect("slow subscription");
    let slow_id = subscription_token(&slow);
    // 20 frames, none pulled: the 17th exceeds the subscriber's data-frame cap
    // (16) and evicts it into the reserved control slot.
    for revision in 2..=21 {
        publish_run_state(&port, SLOW_RUN, revision);
    }
    let drained = handle
        .next_workflow_events(&principal, slow_id.clone())
        .await
        .expect("drain");
    assert_eq!(
        drained.events.len(),
        MAX_PULL_FRAMES,
        "the buffered data frames drain first, capped at {MAX_PULL_FRAMES}"
    );
    assert!(!drained.closed, "the lag gap has not been read yet");
    assert!(
        drained
            .events
            .iter()
            .all(|event| event.event.as_str() == "run_state"),
        "the data frames precede the lag gap: {drained:?}"
    );
    let lag = handle
        .next_workflow_events(&principal, slow_id.clone())
        .await
        .expect("the lag gap");
    assert_eq!(lag.events.len(), 1, "the lag gap fills the reserved slot");
    assert_eq!(lag.events[0].event.as_str(), "gap");
    let lag_wire = frame_json(&lag.events[0]);
    let lag_sequence = frame_sequence(&slow_cursor) + 17;
    assert_eq!(
        lag_wire["from_sequence"].as_u64(),
        Some(lag_sequence),
        "the gap names the frame the subscriber fell behind at"
    );
    assert_eq!(lag_wire["to_sequence"].as_u64(), Some(lag_sequence));
    assert!(
        lag.closed,
        "a lagging subscription CLOSES instead of silently ending"
    );
    assert!(
        matches!(
            handle
                .next_workflow_events(&principal, slow_id)
                .await
                .unwrap_err(),
            nexus_core::CoreError::NotFound { .. }
        ),
        "the lagging subscription's token is withdrawn after its gap"
    );

    // ── 7. The per-run subscriber cap refuses typed, and a release frees the
    //       permit again (the ring's own accounting, never a fabricated token).
    stage_run_row(pool.as_ref(), BLOCK_RUN, CREATOR, None).await;
    assert!(port.try_register_live(BLOCK_RUN).await, "blocking ring");
    let mut holders = Vec::new();
    for _ in 0..MAX_SUBSCRIBERS_PER_RUN {
        let held = handle
            .subscribe_workflow_events(&principal, subscribe_request(BLOCK_RUN, None))
            .await
            .expect("a subscriber permit");
        holders.push(subscription_token(&held));
    }
    let capped = handle
        .subscribe_workflow_events(&principal, subscribe_request(BLOCK_RUN, None))
        .await
        .unwrap_err();
    assert!(
        matches!(capped, nexus_core::CoreError::Busy),
        "the run's subscriber cap must refuse as busy: {capped:?}"
    );
    let freed = holders.pop().expect("a held permit");
    handle
        .release_workflow_events(&principal, freed)
        .expect("release");
    let admitted = handle
        .subscribe_workflow_events(&principal, subscribe_request(BLOCK_RUN, None))
        .await
        .expect("a released permit is reusable, never leaked");
    holders.push(subscription_token(&admitted));
    for token in holders {
        handle
            .release_workflow_events(&principal, token)
            .expect("release");
    }

    // ── 8. Release WAKES a pull blocked on a silent ring. ──
    let blocked = handle
        .subscribe_workflow_events(&principal, subscribe_request(BLOCK_RUN, None))
        .await
        .expect("a silent ring still subscribes");
    let blocked_id = subscription_token(&blocked);
    let puller = tokio::spawn({
        let handle = Arc::clone(&handle);
        let principal = principal.clone();
        let token = blocked_id.clone();
        async move { handle.next_workflow_events(&principal, token).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !puller.is_finished(),
        "an empty live ring must BLOCK the pull, not answer an empty batch"
    );
    // Exactly ONE pull may be outstanding per subscription.
    let concurrent = handle
        .next_workflow_events(&principal, blocked_id.clone())
        .await
        .unwrap_err();
    assert!(
        matches!(concurrent, nexus_core::CoreError::Busy),
        "a second pull on the same subscription must refuse as busy: {concurrent:?}"
    );
    handle
        .release_workflow_events(&principal, blocked_id.clone())
        .expect("release");
    let woken = tokio::time::timeout(Duration::from_secs(5), puller)
        .await
        .expect("release must wake the blocked pull")
        .expect("join")
        .expect("a woken pull is not an error");
    assert!(woken.closed, "a released subscription reports `closed`");
    assert!(
        woken.events.is_empty(),
        "no frame was invented to end the pull: {woken:?}"
    );
    assert!(
        matches!(
            handle
                .next_workflow_events(&principal, blocked_id)
                .await
                .unwrap_err(),
            nexus_core::CoreError::NotFound { .. }
        ),
        "the released token refuses"
    );

    // ── 9. Owner close wakes a blocked pull and withdraws every token. ──
    let closing = handle
        .subscribe_workflow_events(&principal, subscribe_request(BLOCK_RUN, None))
        .await
        .expect("subscribe before close");
    let closing_id = subscription_token(&closing);
    let puller = tokio::spawn({
        let handle = Arc::clone(&handle);
        let principal = principal.clone();
        let token = closing_id.clone();
        async move { handle.next_workflow_events(&principal, token).await }
    });
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert!(
        !puller.is_finished(),
        "the pull is blocked when close begins"
    );
    let report = handle.close().await.expect("owner close");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    let woken = tokio::time::timeout(Duration::from_secs(5), puller)
        .await
        .expect("close must wake the blocked pull")
        .expect("join")
        .expect("a woken pull is not an error");
    assert!(woken.closed, "close ends the stream with `closed`");
    let after_close = handle
        .next_workflow_events(&principal, closing_id)
        .await
        .unwrap_err();
    assert!(
        matches!(after_close, nexus_core::CoreError::Closing),
        "a closed owner serves no pull: {after_close:?}"
    );
    let subscribe_after_close = handle
        .subscribe_workflow_events(&principal, subscribe_request(BLOCK_RUN, None))
        .await
        .unwrap_err();
    assert!(
        matches!(subscribe_after_close, nexus_core::CoreError::Closing),
        "a closed owner mints no subscription: {subscribe_after_close:?}"
    );
    fixture.core.close().await.expect("core close");
}

/// Holds a subscribe exactly between its durable-owner check and its ring
/// attachment, so an owner close can be forced into that window deterministically.
#[derive(Default)]
struct SubscribeRaceGate {
    at_gate: AtomicBool,
    release: AtomicBool,
}

#[async_trait]
impl nexus_core::execution::ExecutionSubscriptionObserver for SubscribeRaceGate {
    async fn owner_resolved(&self) {
        self.at_gate.store(true, Ordering::SeqCst);
        while !self.release.load(Ordering::SeqCst) {
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    }
}

/// A subscribe that INTERSECTS an owner close must neither publish a token nor
/// keep the ring subscriber permit it had already attached (P1-T1 race).
///
/// The seam is the exact window the defect needs: the run's durable ROOT
/// ownership is resolved (an `await`), and only afterwards is the ring attached
/// and the token minted. Without a sealed table, a close landing in that window
/// let the subscribe march past `close_all` and publish a token the close had
/// already walked past — a token no release could ever withdraw (every entry
/// point fences on the closed owner) holding a ring permit for the process's
/// lifetime. The gate makes that interleaving deterministic instead of racy.
#[tokio::test]
#[serial_test::serial]
async fn subscribe_racing_owner_close_never_leaks_a_token_or_ring_permit() {
    use nexus_core::execution::production::CoreRunEventPort;
    use nexus_core::execution::run_events::RunEventRegistry;
    use std::time::Duration;

    const RACE_RUN: &str = "run-subscribe-race";

    let tmp = tempfile::tempdir().expect("tempdir");
    let home = tmp.path();
    prepare_hosted_home(home).await;
    let core = CoreService::open(CoreOpenOptions {
        user_home: home.into(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .expect("engine-owner core open");

    // The REAL production port over a REAL registry, so the permit this case
    // counts is the one a subscription actually takes.
    let registry = Arc::new(RunEventRegistry::new());
    let sinks: nexus_core::execution::run_events::RunEventSinkMap =
        Arc::new(tokio::sync::Mutex::new(HashMap::new()));
    let gate = Arc::new(SubscribeRaceGate::default());
    let handle = core
        .start_execution(
            Arc::new(NullProvider) as Arc<dyn ProviderPort>,
            RunnerDeps {
                run_events: Some(Arc::new(CoreRunEventPort::new(
                    Arc::clone(&registry),
                    sinks,
                ))),
                subscription_observer: Some(gate.clone()),
                ..RunnerDeps::default()
            },
        )
        .await
        .expect("execution owner starts");
    let principal = core.active_principal().await.unwrap();
    stage_run_row(
        handle.coordinator().pool().as_ref(),
        RACE_RUN,
        CREATOR,
        None,
    )
    .await;
    let port = handle
        .coordinator()
        .run_event_port()
        .expect("the owner serves the ring port");
    assert!(
        port.try_register_live(RACE_RUN).await,
        "the race needs a live ring to attach to"
    );

    // ── The subscribe stops inside the window: ownership resolved, ring not
    //    yet attached. ──
    let racer = tokio::spawn({
        let handle = Arc::clone(&handle);
        let principal = principal.clone();
        async move {
            handle
                .subscribe_workflow_events(&principal, subscribe_request(RACE_RUN, None))
                .await
        }
    });
    let mut waited = Duration::ZERO;
    while !gate.at_gate.load(Ordering::SeqCst) {
        assert!(
            waited < Duration::from_secs(10),
            "the subscribe never reached its durable-owner check"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
        waited += Duration::from_millis(5);
    }
    assert_eq!(
        registry.live_subscriber_count(RACE_RUN),
        0,
        "the ring is not attached yet at the seam"
    );

    // ── The owner closes COMPLETELY while the subscribe sits in the window. ──
    let report = handle.close().await.expect("owner close");
    assert!(report.cleanup_confirmed, "close must be confirmed");
    gate.release.store(true, Ordering::SeqCst);

    let outcome = tokio::time::timeout(Duration::from_secs(5), racer)
        .await
        .expect("the race must settle")
        .expect("join");
    match outcome {
        Err(nexus_core::CoreError::Closing) => {}
        Ok(subscription) => panic!(
            "a subscribe that raced the close handed out a token: {} ({} ring permit(s) held)",
            subscription.subscription_id.as_str(),
            registry.live_subscriber_count(RACE_RUN)
        ),
        Err(other) => panic!("a raced subscribe must refuse as Closing, got {other:?}"),
    }
    assert_eq!(
        registry.live_subscriber_count(RACE_RUN),
        0,
        "the ring subscriber permit the refused subscribe had attached must be released"
    );
    assert!(
        handle.is_settled(),
        "the owner stayed closed across the race"
    );
    core.close().await.expect("core close");
}

/// Every direct recoverable entrance on a CLOSED manager refuses, typed.
#[cfg(feature = "test-hooks")]
async fn assert_direct_entrances_refused(
    manager: &Arc<WorkspaceSessionManager>,
    root: &str,
    durable_session: &SessionId,
    plain_session: &SessionId,
) {
    let durable = manager
        .commit_session_durable(
            durable_session,
            &[direct_change("refused.txt", HOSTED_PAYLOAD_B64)],
            root,
        )
        .await;
    assert!(
        matches!(durable, Err(SessionError::AuthorityBusy)),
        "a direct durable commit after close must fail closed, got {durable:?}"
    );
    let plain = manager
        .commit_session(
            plain_session,
            &[direct_change("refused-plain.txt", HOSTED_PAYLOAD_B64)],
            root,
        )
        .await;
    assert!(
        matches!(plain, Err(SessionError::AuthorityBusy)),
        "a direct recoverable commit after close must fail closed, got {plain:?}"
    );
    let scoped = manager.startup_recovery_for_root(root).await;
    assert!(
        matches!(scoped, Err(SessionError::AuthorityBusy)),
        "root-scoped startup recovery after close must fail closed, got {scoped:?}"
    );
    let sweep = manager.startup_recovery().await;
    assert!(
        matches!(sweep, Err(SessionError::AuthorityBusy)),
        "the whole-DB startup recovery after close must fail closed, got {sweep:?}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// Hosted factory restart: recovery truth + the §4 `workspace_commit`
// projection (v1.195 P1-T2)
// ─────────────────────────────────────────────────────────────────────────────

/// A declared `workspace.commit` that CANNOT be applied: `workspace.open`
/// succeeds, the commit names a session that does not exist, so the capability
/// records `_capability_error` and never writes an output — the durable
/// "failed" shape the detail projection must never turn into a revision.
const FAILED_COMMIT_PRESET: &str = "projection-failed-commit";

fn failed_commit_preset_yaml() -> String {
    format!(
        r#"
preset:
  id: {FAILED_COMMIT_PRESET}
  version: 1
  kind: creator
  description: "projection fixture — a declared workspace.commit that cannot apply"
  requires_capabilities:
    - workspace.open
    - workspace.commit
  initial: open_scope
  terminal: done
states:
  - id: open_scope
    description: "open a scope inside the factory-resolved creative root"
    enter:
      - kind: capability
        name: workspace.open
        args:
          path: notes
    exit_when: {{ kind: rule }}
    next: commit_scope
  - id: commit_scope
    description: "a commit that cannot apply: the named session does not exist"
    enter:
      - kind: capability
        name: workspace.commit
        args:
          sessionId: "no-such-workspace-session"
          changes:
            - path: never.txt
              op: create
              contentBase64: "{HOSTED_PAYLOAD_B64}"
    exit_when: {{ kind: rule }}
    next: done
  - id: done
    terminal: true
"#
    )
}

/// The SUCCESS→FAILURE sequence on ONE capability name: `workspace.open`, a
/// first `workspace.commit` that applies, then a SECOND `workspace.commit`
/// that cannot apply. The second invocation fails while `_capability_name`
/// still reads `workspace.commit`, so a projection that does not require the
/// output to belong to the SAME invocation reports the first attempt's
/// revision as if the latest — failed — commit had produced it.
const DOUBLE_COMMIT_PRESET: &str = "projection-double-commit";

fn double_commit_preset_yaml() -> String {
    format!(
        r#"
preset:
  id: {DOUBLE_COMMIT_PRESET}
  version: 1
  kind: creator
  description: "projection fixture — one successful commit, then a failing second commit"
  requires_capabilities:
    - workspace.open
    - workspace.commit
  initial: open_scope
  terminal: done
states:
  - id: open_scope
    description: "open a scope inside the factory-resolved creative root"
    enter:
      - kind: capability
        name: workspace.open
        args:
          path: notes
    exit_when: {{ kind: rule }}
    next: first_commit
  - id: first_commit
    description: "the commit that applies (its session comes from the open output)"
    enter:
      - kind: capability
        name: workspace.commit
        args:
          sessionId: "{{{{_capability_output.sessionId}}}}"
          changes:
            - path: first.txt
              op: create
              contentBase64: "{HOSTED_PAYLOAD_B64}"
    exit_when: {{ kind: rule }}
    next: second_commit
  - id: second_commit
    description: "the SECOND commit on the same name that cannot apply"
    enter:
      - kind: capability
        name: workspace.commit
        args:
          sessionId: "no-such-workspace-session"
          changes:
            - path: second.txt
              op: create
              contentBase64: "{HOSTED_PAYLOAD_B64}"
    exit_when: {{ kind: rule }}
    next: done
  - id: done
    terminal: true
"#
    )
}

/// The durable `workspace.commit` checkpoint of one run, read straight out of
/// the persisted graph context: `(output revision, capability error)`.
///
/// This is the fixture's own oracle for "the capability really completed /
/// really failed", independent of the detail field under test — a negative
/// projection assertion is only meaningful next to evidence that the failure
/// path actually ran.
async fn durable_commit_checkpoint(
    pool: &sqlx::SqlitePool,
    run_id: &str,
) -> (Option<String>, Option<String>) {
    sqlx::query_as::<_, (Option<String>, Option<String>)>(
        "SELECT json_extract(context_json, '$.data._capability_output.revision'),
                json_extract(context_json, '$.data._capability_error')
         FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(run_id)
    .fetch_one(pool)
    .await
    .expect("the run's durable graph context is readable")
}

/// The revisions the durable commit intents settled — the commit authority's
/// own record, never the graph context the projection reads.
async fn committed_intent_revisions(pool: &sqlx::SqlitePool) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT revision FROM workspace_commit_intents WHERE state = 'committed' ORDER BY revision",
    )
    .fetch_all(pool)
    .await
    .expect("the durable commit intents are readable")
}

/// Stage one durable root run row with an explicit graph-context blob: the
/// detail projection's authorization source AND its durable checkpoint, so a
/// caller can discriminate the shapes the real owner cannot produce on demand.
async fn stage_context_run_row(
    pool: &sqlx::SqlitePool,
    run_id: &str,
    creator_id: &str,
    context_json: &str,
) {
    sqlx::query(
        "INSERT INTO orchestration_sessions
           (session_id, creator_id, preset_id, preset_version, parent_session_id,
            status, context_json, created_at, updated_at)
         VALUES (?, ?, 'hosted-schedule-drive', 1, NULL, 'running', ?, 1, 1)",
    )
    .bind(run_id)
    .bind(creator_id)
    .bind(context_json.as_bytes())
    .execute(pool)
    .await
    .expect("stage a durable run row with an explicit graph context");
}

/// Wait (bounded) until a run reaches a terminal durable status and return it.
///
/// A run whose declared capability failed still finishes its graph (the failure
/// is a step status, not a drive abort), so the terminal status is read rather
/// than assumed.
async fn wait_for_terminal_run(pool: &sqlx::SqlitePool, run_id: &str) -> String {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        let status = durable_record(pool, run_id)
            .await
            .status
            .as_db_str()
            .to_string();
        if matches!(status.as_str(), "completed" | "failed" | "cancelled") {
            return status;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the run never reached a terminal status (still {status})"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Wait (bounded) until the run's durable cancel intent is written — the fence
/// that makes the cancel the durable control winner before the parked prompt
/// unwinds.
async fn wait_for_cancel_intent(pool: &sqlx::SqlitePool, run_id: &str) {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    loop {
        if durable_record(pool, run_id)
            .await
            .state
            .as_ref()
            .is_some_and(|state| state.cancel_requested)
        {
            return;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "the cancel intent never became durable"
        );
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
}

/// Actual-factory restart truth (S1-3 + the durable half of S1-4).
///
/// ONE real `start_hosted_execution` owner drives four schedules to four
/// different durable outcomes, is closed, and a SECOND factory owner is opened
/// over the same home. Across that process boundary the durable truth may not
/// move: the completed effect stays committed exactly once (same bytes, same
/// settled revision, unchanged run revision, no re-drive, no re-execution), the
/// confirmed cancel stays `cancelled` instead of being re-driven or relabelled,
/// an unconfirmed stop stays `interrupted` and actionable instead of
/// normalizing to `cancelled`, a human wait whose frozen source is gone stays
/// preserved and answers the continuation signal with the typed reconstruction
/// refusal (cancel-only, no fresh schedule), and the run's ring — gone with the
/// old process — is reported as ONE explicit `history_unavailable` control
/// frame while the W4 detail stays readable.
#[allow(clippy::too_many_lines)] // one linear restart journey; splitting hides the ordering evidence
#[tokio::test]
#[serial_test::serial]
async fn hosted_restart_keeps_cancel_and_uncertain_effect_truth() {
    let fixture = hosted_fixture().await;
    let home = fixture.tmp.path().to_path_buf();
    let nexus_home = home.join(".nexus42");
    let principal = fixture.core.active_principal().await.unwrap();
    let pool = fixture.handle.coordinator().pool();
    let epoch_a = fixture.handle.engine_epoch();
    write_preset_bundle(
        &nexus_home,
        CANCEL_EFFECT_PRESET,
        &cancel_effect_preset_yaml(),
        "Summarize the hosted topic: {{preset.input.topic}}\n",
    );
    write_preset_bundle(
        &nexus_home,
        CANCEL_WAIT_PRESET,
        &cancel_wait_preset_yaml(),
        "Summarize the retained topic: {{preset.input.topic}}\n",
    );

    // ── 1. The COMPLETED run: its one declared commit is applied inside the
    //       factory-resolved root, then the prompt releases and the run
    //       settles terminal. ──
    let completed_schedule = add_hosted_schedule(&fixture).await;
    wait_for_prompt_count(&fixture.host, 1).await;
    let (status, owned) = schedule_row(pool.as_ref(), &completed_schedule).await;
    assert_eq!(status, "running", "the owned clock claimed the row's run");
    let completed_run = owned.expect("the admitted schedule owns its run");
    assert_eq!(
        std::fs::read(fixture.root.join("notes/hosted.txt")).expect("the committed bytes"),
        HOSTED_PAYLOAD,
        "the declared commit must be applied inside the canonical root"
    );
    let settled = committed_intent_revisions(pool.as_ref()).await;
    assert_eq!(
        settled.len(),
        1,
        "exactly one commit may have settled: {settled:?}"
    );
    let committed_revision = settled[0].clone();
    fixture.host.release_parked();
    wait_for_run_status(pool.as_ref(), &completed_run, "completed").await;
    let completed_before = durable_record(pool.as_ref(), &completed_run).await;

    // ── 2. The CANCELLED run: the cancel lands while the prompt is in flight,
    //       so the authorized commit AFTER it never runs. ──
    let cancelled_schedule = add_parallel_any_schedule(
        &fixture.handle,
        &principal,
        CANCEL_EFFECT_PRESET,
        "restart-cancelled",
    )
    .await;
    let cancelled_run = wait_for_schedule_run(pool.as_ref(), &cancelled_schedule).await;
    wait_for_prompt_count(&fixture.host, 2).await;
    let cancellation = tokio::spawn({
        let handle = Arc::clone(&fixture.handle);
        let principal = principal.clone();
        let schedule_id = cancelled_schedule.clone();
        async move {
            handle
                .signal_schedule(&principal, schedule_id, signal("cancel"))
                .await
        }
    });
    wait_for_cancel_intent(pool.as_ref(), &cancelled_run).await;
    fixture.host.release_parked();
    let cancelled = cancellation
        .await
        .expect("the cancel task joins")
        .expect("a confirmable cancel");
    assert_eq!(
        cancelled.status, "cancelled",
        "only a confirmed stop is a cancel success"
    );
    wait_for_run_status(pool.as_ref(), &cancelled_run, "cancelled").await;
    assert!(
        !fixture.root.join(CANCEL_EFFECT_FILE).exists(),
        "the authorized commit AFTER the cancel must never run"
    );
    assert_eq!(
        schedule_row(pool.as_ref(), &cancelled_schedule).await.0,
        "cancelled",
        "the schedule row projects the confirmed cancel"
    );

    // ── 3. The UNCERTAIN stop: the owned teardown cannot be confirmed, so the
    //       run settles `interrupted` and the row stays non-terminal. ──
    let interrupted_schedule = add_parallel_any_schedule(
        &fixture.handle,
        &principal,
        CANCEL_EFFECT_PRESET,
        "restart-interrupted",
    )
    .await;
    let interrupted_run = wait_for_schedule_run(pool.as_ref(), &interrupted_schedule).await;
    wait_for_prompt_count(&fixture.host, 3).await;
    fixture.host.fail_session_shutdown();
    let unconfirmed = tokio::spawn({
        let handle = Arc::clone(&fixture.handle);
        let principal = principal.clone();
        let schedule_id = interrupted_schedule.clone();
        async move {
            handle
                .signal_schedule(&principal, schedule_id, signal("cancel"))
                .await
        }
    });
    wait_for_cancel_intent(pool.as_ref(), &interrupted_run).await;
    fixture.host.release_parked();
    let unconfirmed = unconfirmed
        .await
        .expect("the cancel task joins")
        .expect("an unconfirmed stop is still a typed answer");
    assert_eq!(
        unconfirmed.status, "interrupted",
        "unconfirmed cleanup is never reported as `cancelled`"
    );
    let interrupted_before = durable_record(pool.as_ref(), &interrupted_run).await;
    assert_eq!(interrupted_before.status.as_db_str(), "interrupted");
    assert!(
        interrupted_before
            .state
            .as_ref()
            .is_some_and(|state| state.cancel_requested),
        "the uncertain stop keeps its durable cancel intent"
    );
    assert_eq!(
        schedule_row(pool.as_ref(), &interrupted_schedule).await.0,
        "running",
        "the row stays non-terminal while the cleanup is unconfirmed"
    );
    assert!(
        fixture
            .handle
            .get_workflow_session(&principal, interrupted_run.clone())
            .await
            .expect("the interrupted run stays inspectable")
            .session
            .failure_reason
            .is_some(),
        "an uncertain stop must stay actionable through its durable failure record"
    );

    // ── 4. The HUMAN WAIT whose frozen source is removed before the restart. ──
    let wait_schedule = add_parallel_any_schedule(
        &fixture.handle,
        &principal,
        CANCEL_WAIT_PRESET,
        "restart-wait",
    )
    .await;
    let wait_run = wait_for_schedule_run(pool.as_ref(), &wait_schedule).await;
    wait_for_prompt_count(&fixture.host, 4).await;
    fixture.host.release_parked();
    let wait_id = wait_for_human_wait(pool.as_ref(), &wait_run).await;

    // ── The process boundary. A changed/missing frozen source is
    //    reconstruction unavailability for the waited run, never a fresh
    //    schedule. ──
    std::fs::remove_dir_all(nexus_home.join("presets").join(CANCEL_WAIT_PRESET))
        .expect("remove the waited run's frozen preset source");
    let report = fixture.handle.close().await.expect("owner close");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    fixture.core.close().await.expect("core close");

    // ── A SECOND factory owner over the same home: a NEW admission. ──
    let (core_b, host_b, handle_b) = open_hosted_owner(&home).await;
    assert!(
        handle_b.engine_epoch() > epoch_a,
        "the reopened owner must be a new admission ({epoch_a} -> {})",
        handle_b.engine_epoch()
    );
    let principal_b = core_b.active_principal().await.unwrap();
    let pool_b = handle_b.coordinator().pool();

    // (a) The completed effect stays committed EXACTLY ONCE: same bytes, same
    //     settled revision, and NO re-drive (a re-stepped run would move its
    //     durable revision).
    let completed_after = durable_record(pool_b.as_ref(), &completed_run).await;
    assert_eq!(
        completed_after.status.as_db_str(),
        "completed",
        "a completed run stays terminal across the restart"
    );
    assert_eq!(
        completed_after.state_revision, completed_before.state_revision,
        "recovery must not step a terminal run"
    );
    assert_eq!(
        completed_after.graph_version, completed_before.graph_version,
        "recovery must not re-drive a terminal run"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("notes/hosted.txt")).expect("the committed bytes"),
        HOSTED_PAYLOAD,
        "the committed effect must survive the restart unchanged"
    );
    assert_eq!(
        committed_intent_revisions(pool_b.as_ref()).await,
        vec![committed_revision.clone()],
        "the restart must not settle a second commit revision"
    );
    let completed_detail = handle_b
        .get_workflow_session(&principal_b, completed_run.clone())
        .await
        .expect("the completed run stays readable after the restart");
    assert_eq!(completed_detail.session.status, "completed");
    assert_eq!(
        completed_detail
            .workspace_commit
            .as_ref()
            .map(|commit| (commit.revision.clone(), commit.committed)),
        Some((committed_revision.clone(), true)),
        "the restart still exposes the durable revision without re-running the capability"
    );
    assert_eq!(
        schedule_row(pool_b.as_ref(), &completed_schedule).await,
        ("completed".to_string(), Some(completed_run.clone())),
        "the completed schedule keeps the same single run"
    );

    // (b) The confirmed cancel stays cancelled.
    let cancelled_after = durable_record(pool_b.as_ref(), &cancelled_run).await;
    assert_eq!(
        cancelled_after.status.as_db_str(),
        "cancelled",
        "a cancelled run is terminal and must never be re-driven"
    );
    let cancelled_detail = handle_b
        .get_workflow_session(&principal_b, cancelled_run.clone())
        .await
        .expect("the cancelled run stays readable");
    assert_eq!(cancelled_detail.session.status, "cancelled");
    assert!(
        cancelled_detail.workspace_commit.is_none(),
        "a run cancelled before its commit checkpoint exposes no revision"
    );
    assert_eq!(
        schedule_row(pool_b.as_ref(), &cancelled_schedule).await,
        ("cancelled".to_string(), Some(cancelled_run.clone())),
        "the cancelled row keeps its own run across the restart"
    );

    // (c) The uncertain stop stays `interrupted` — never normalized to
    //     `cancelled` — and is not redispatched.
    let interrupted_after = durable_record(pool_b.as_ref(), &interrupted_run).await;
    assert_eq!(
        interrupted_after.status.as_db_str(),
        "interrupted",
        "an unconfirmed stop must not be normalized to `cancelled` by the restart"
    );
    assert!(
        interrupted_after
            .state
            .as_ref()
            .is_some_and(|state| state.cancel_requested),
        "the uncertain cancel intent survives the restart"
    );
    let interrupted_detail = handle_b
        .get_workflow_session(&principal_b, interrupted_run.clone())
        .await
        .expect("the interrupted run stays readable");
    assert_eq!(interrupted_detail.session.status, "interrupted");
    assert!(
        interrupted_detail.session.failure_reason.is_some(),
        "the interrupted run stays actionable after the restart"
    );
    assert_eq!(
        schedule_row(pool_b.as_ref(), &interrupted_schedule).await.0,
        "running",
        "the retryable row stays non-terminal until a confirmed cleanup"
    );

    // (d) The human wait survives its missing frozen source, and an authorized
    //     continuation is a TYPED refusal — never a fresh schedule. (`resume`
    //     is the core continuation seam for an owned run: `continue` itself is
    //     served by the transport, and the reconstruction gate is evaluated
    //     before any wait/state mutation.)
    let wait_after = durable_record(pool_b.as_ref(), &wait_run).await;
    assert_eq!(
        wait_after.status.as_db_str(),
        "waiting_for_input",
        "a durable human wait is preserved across the restart"
    );
    assert_eq!(
        wait_after
            .state
            .as_ref()
            .and_then(|state| state.wait.as_ref())
            .map(|wait| wait.wait_id.clone())
            .as_deref(),
        Some(wait_id.as_str()),
        "the exact wait token survives the restart"
    );
    assert_eq!(
        handle_b
            .get_workflow_session(&principal_b, wait_run.clone())
            .await
            .expect("W4 stays readable for a preserved wait")
            .session
            .status,
        "waiting_for_input"
    );
    let refused = handle_b
        .signal_schedule(&principal_b, wait_schedule.clone(), signal("resume"))
        .await
        .unwrap_err();
    match &refused {
        nexus_core::CoreError::Coded { code, message } => {
            assert_eq!(
                code, "workflow_state_conflict",
                "a missing frozen source is a typed conflict, got {refused:?}"
            );
            assert!(
                message.contains("cannot be reconstructed"),
                "the refusal must name the missing source: {message}"
            );
            assert!(
                message.contains("cancel-only"),
                "the refusal must keep the wait actionable (cancel-only): {message}"
            );
        }
        other => panic!("a missing frozen source must refuse the continuation, got {other:?}"),
    }
    let wait_still = durable_record(pool_b.as_ref(), &wait_run).await;
    assert_eq!(
        wait_still.status.as_db_str(),
        "waiting_for_input",
        "a refused continuation must not consume or advance the wait"
    );
    assert_eq!(
        wait_still
            .state
            .as_ref()
            .and_then(|state| state.wait.as_ref())
            .map(|wait| wait.wait_id.clone())
            .as_deref(),
        Some(wait_id.as_str()),
        "the refused continuation leaves the exact wait token in place"
    );
    assert_eq!(
        schedule_row(pool_b.as_ref(), &wait_schedule).await.0,
        "running",
        "the waited row keeps its owned run; no fresh schedule is minted"
    );

    // (e) The ring is gone with the old process, and that loss is EXPLICIT —
    //     one control frame naming the run's inspect route — while W4 keeps
    //     answering for the same run.
    let subscription = handle_b
        .subscribe_workflow_events(&principal_b, subscribe_request(&cancelled_run, None))
        .await
        .expect("the owned run still opens a subscription");
    let control = handle_b
        .next_workflow_events(&principal_b, subscription_token(&subscription))
        .await
        .expect("the control frame");
    assert!(control.closed, "the lost ring closes the stream");
    assert_eq!(control.events.len(), 1, "exactly one control frame");
    assert_eq!(
        control.events[0].event.as_str(),
        "history_unavailable",
        "ring loss must be explicit, never fabricated history: {:?}",
        control.events[0]
    );
    assert_eq!(
        control.events[0].id, "",
        "the control frame carries no cursor"
    );
    let wire = frame_json(&control.events[0]);
    assert_eq!(wire["run_id"], cancelled_run);
    assert_eq!(
        wire["inspect_url"],
        format!("/v1/daemon/orchestration/sessions/{cancelled_run}"),
        "the control frame names the run's public inspect route"
    );
    assert_eq!(
        handle_b
            .get_workflow_session(&principal_b, cancelled_run.clone())
            .await
            .expect("W4 stays readable while the ring is gone")
            .session
            .status,
        "cancelled",
        "the durable detail outlives the in-memory ring"
    );

    // The whole restart performed no work of its own.
    assert_eq!(
        host_b.prompt_count(),
        0,
        "a restart must not redispatch a single prompt"
    );
    assert_eq!(
        root_run_count(pool_b.as_ref()).await,
        4,
        "a restart must never mint a run"
    );

    let report = handle_b.close().await.expect("the reopened owner closes");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    core_b.close().await.expect("core close");
}

/// The §4 `workspace_commit` projection: a narrowly scoped read of ONE durable
/// checkpoint, exposed once and unchanged across a restart.
///
/// Discriminations: a completed `workspace.commit` is exposed (with the exact
/// settled revision read from the commit authority, not from the context), a
/// run with no commit exposes nothing, a run whose declared commit FAILED
/// exposes nothing while leaving durable failure evidence, an identical
/// checkpoint under another Creator is not readable at all, and the malformed
/// matrix (wrong type, empty revision, `committed` not true, missing member,
/// extra member, another capability's pair, corrupt blob) exposes nothing.
#[allow(clippy::too_many_lines)] // one contract; splitting hides the shape matrix
#[tokio::test]
#[serial_test::serial]
async fn hosted_workspace_commit_projection_is_authorized_and_survives_restart() {
    let fixture = hosted_fixture().await;
    let home = fixture.tmp.path().to_path_buf();
    let nexus_home = home.join(".nexus42");
    let principal = fixture.core.active_principal().await.unwrap();
    let pool = fixture.handle.coordinator().pool();
    let epoch_a = fixture.handle.engine_epoch();
    write_preset_bundle(
        &nexus_home,
        CANCEL_WAIT_PRESET,
        &cancel_wait_preset_yaml(),
        "unused by this preset\n",
    );
    write_preset_bundle(
        &nexus_home,
        FAILED_COMMIT_PRESET,
        &failed_commit_preset_yaml(),
        "unused by this preset\n",
    );

    // ── A committed run exposes its own settled revision — while it is still
    //    parked (non-terminal) at its prompt. ──
    let committed_schedule = add_hosted_schedule(&fixture).await;
    wait_for_prompt_count(&fixture.host, 1).await;
    let (_, owned) = schedule_row(pool.as_ref(), &committed_schedule).await;
    let committed_run = owned.expect("the clock claimed the committed schedule's run");
    let settled = committed_intent_revisions(pool.as_ref()).await;
    assert_eq!(
        settled.len(),
        1,
        "exactly one commit may have settled: {settled:?}"
    );
    let committed_revision = settled[0].clone();
    let (checkpoint_revision, checkpoint_error) =
        durable_commit_checkpoint(pool.as_ref(), &committed_run).await;
    assert_eq!(
        checkpoint_revision.as_deref(),
        Some(committed_revision.as_str()),
        "the durable checkpoint carries the settled revision"
    );
    assert!(
        checkpoint_error.is_none(),
        "a completed capability records no error: {checkpoint_error:?}"
    );
    let live = fixture
        .handle
        .get_workflow_session(&principal, committed_run.clone())
        .await
        .expect("the committed run's detail");
    assert_eq!(
        live.workspace_commit
            .as_ref()
            .map(|commit| (commit.revision.clone(), commit.committed)),
        Some((committed_revision.clone(), true)),
        "the committed run exposes its own durable revision"
    );
    fixture.host.release_parked();
    wait_for_run_status(pool.as_ref(), &committed_run, "completed").await;

    // ── A run with NO commit checkpoint exposes nothing. ──
    let wait_schedule = add_parallel_any_schedule(
        &fixture.handle,
        &principal,
        CANCEL_WAIT_PRESET,
        "projection-no-commit",
    )
    .await;
    let no_commit_run = wait_for_schedule_run(pool.as_ref(), &wait_schedule).await;
    wait_for_prompt_count(&fixture.host, 2).await;
    fixture.host.release_parked();
    wait_for_human_wait(pool.as_ref(), &no_commit_run).await;
    assert!(
        fixture
            .handle
            .get_workflow_session(&principal, no_commit_run.clone())
            .await
            .expect("the waiting run's detail")
            .workspace_commit
            .is_none(),
        "a run that never committed exposes no revision"
    );

    // ── A run whose declared commit FAILED: durable failure evidence, no
    //    output, no effect — and nothing projected. ──
    let failed_schedule = add_parallel_any_schedule(
        &fixture.handle,
        &principal,
        FAILED_COMMIT_PRESET,
        "projection-failed",
    )
    .await;
    let failed_run = wait_for_schedule_run(pool.as_ref(), &failed_schedule).await;
    assert_eq!(
        wait_for_terminal_run(pool.as_ref(), &failed_run).await,
        "completed",
        "the graph finishes; a capability failure is a step status"
    );
    let (failed_output, failed_error) = durable_commit_checkpoint(pool.as_ref(), &failed_run).await;
    assert!(
        failed_error.is_some(),
        "the declared commit must have failed durably: {failed_error:?}"
    );
    assert!(
        failed_output.is_none(),
        "a failed commit writes no capability output: {failed_output:?}"
    );
    assert!(
        !fixture.root.join("notes/never.txt").exists(),
        "a failed commit applies nothing"
    );
    assert!(
        fixture
            .handle
            .get_workflow_session(&principal, failed_run.clone())
            .await
            .expect("the failed-commit run's detail")
            .workspace_commit
            .is_none(),
        "a failed commit must never yield a revision"
    );

    // ── The SUCCESS→FAILURE sequence on the same capability name: the first
    //    commit applies, the SECOND one fails. The failed invocation owns no
    //    output, so the detail must project NOTHING — the previous attempt's
    //    revision must never be presented as the latest checkpoint. ──
    write_preset_bundle(
        &nexus_home,
        DOUBLE_COMMIT_PRESET,
        &double_commit_preset_yaml(),
        "unused by this preset\n",
    );
    let settled_before_double = committed_intent_revisions(pool.as_ref()).await;
    let double_schedule = add_parallel_any_schedule(
        &fixture.handle,
        &principal,
        DOUBLE_COMMIT_PRESET,
        "projection-double-commit",
    )
    .await;
    let double_run = wait_for_schedule_run(pool.as_ref(), &double_schedule).await;
    assert_eq!(
        wait_for_terminal_run(pool.as_ref(), &double_run).await,
        "completed",
        "the graph finishes; the second commit's failure is a step status"
    );
    assert_eq!(
        std::fs::read(fixture.root.join("notes/first.txt")).expect("the first commit applies"),
        HOSTED_PAYLOAD,
        "the FIRST commit must be the one that linearized"
    );
    assert!(
        !fixture.root.join("notes/second.txt").exists(),
        "the failing second commit applies nothing"
    );
    let settled_after_double = committed_intent_revisions(pool.as_ref()).await;
    assert_eq!(
        settled_after_double.len(),
        settled_before_double.len() + 1,
        "exactly the FIRST commit of the double sequence may settle: \
         {settled_before_double:?} -> {settled_after_double:?}"
    );
    assert!(
        settled_before_double
            .iter()
            .all(|revision| settled_after_double.contains(revision)),
        "the earlier settled revision stays durable: {settled_after_double:?}"
    );
    let (double_output, double_error) = durable_commit_checkpoint(pool.as_ref(), &double_run).await;
    assert!(
        fixture
            .handle
            .get_workflow_session(&principal, double_run.clone())
            .await
            .expect("the double-commit run's detail")
            .workspace_commit
            .is_none(),
        "the previous attempt's revision must never be projected for a failed latest commit"
    );
    assert!(
        double_error.is_some(),
        "the second commit must have failed durably: {double_error:?}"
    );
    assert!(
        double_output.is_none(),
        "the failed invocation must leave NO output of its own: {double_output:?}"
    );

    // ── The restart: the same revision, once. ──
    fixture.host.release_all();
    let report = fixture.handle.close().await.expect("owner close");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    fixture.core.close().await.expect("core close");
    let (core_b, _host_b, handle_b) = open_hosted_owner(&home).await;
    assert!(
        handle_b.engine_epoch() > epoch_a,
        "the reopened owner must be a new admission ({epoch_a} -> {})",
        handle_b.engine_epoch()
    );
    let principal_b = core_b.active_principal().await.unwrap();
    let pool_b = handle_b.coordinator().pool();

    let after = handle_b
        .get_workflow_session(&principal_b, committed_run.clone())
        .await
        .expect("the committed run's detail after the restart");
    assert_eq!(
        after
            .workspace_commit
            .as_ref()
            .map(|commit| (commit.revision.clone(), commit.committed)),
        Some((committed_revision.clone(), true)),
        "the restart exposes the SAME durable revision, not a fresh one"
    );
    assert_eq!(
        committed_intent_revisions(pool_b.as_ref()).await,
        settled_after_double,
        "the restart settles no further commit revision"
    );
    assert!(
        handle_b
            .get_workflow_session(&principal_b, no_commit_run.clone())
            .await
            .expect("the waiting run's detail after the restart")
            .workspace_commit
            .is_none(),
        "a run with no commit still exposes nothing after the restart"
    );
    assert!(
        handle_b
            .get_workflow_session(&principal_b, failed_run.clone())
            .await
            .expect("the failed-commit run's detail after the restart")
            .workspace_commit
            .is_none(),
        "a failed commit still exposes nothing after the restart"
    );
    assert!(
        handle_b
            .get_workflow_session(&principal_b, double_run.clone())
            .await
            .expect("the double-commit run's detail after the restart")
            .workspace_commit
            .is_none(),
        "a failed LATEST commit still exposes nothing after the restart"
    );
    assert_eq!(
        durable_commit_checkpoint(pool_b.as_ref(), &double_run).await,
        (None, double_error.clone()),
        "the failed invocation's durable pair survives the restart unchanged"
    );

    // ── The shape matrix on the live owner. A staged context is the
    //    authorization source AND the checkpoint, so ownership and shape are
    //    discriminated independently of the real owner's own runs. ──
    let committed_context = r#"{"data":{"_capability_name":"workspace.commit","_capability_output":{"revision":"rev_staged","committed":true}}}"#;
    stage_context_run_row(
        pool_b.as_ref(),
        "proj-owned-control",
        CREATOR,
        committed_context,
    )
    .await;
    stage_context_run_row(
        pool_b.as_ref(),
        "proj-foreign",
        "projection-foreign-creator",
        committed_context,
    )
    .await;
    let unbacked: [(&str, &str); 8] = [
        (
            "proj-absent-output",
            r#"{"data":{"_capability_name":"workspace.commit"}}"#,
        ),
        (
            "proj-other-capability",
            r#"{"data":{"_capability_name":"workspace.open","_capability_output":{"revision":"rev_staged","committed":true}}}"#,
        ),
        (
            "proj-wrong-type",
            r#"{"data":{"_capability_name":"workspace.commit","_capability_output":{"revision":7,"committed":true}}}"#,
        ),
        (
            "proj-empty-revision",
            r#"{"data":{"_capability_name":"workspace.commit","_capability_output":{"revision":"","committed":true}}}"#,
        ),
        (
            "proj-not-committed",
            r#"{"data":{"_capability_name":"workspace.commit","_capability_output":{"revision":"rev_staged","committed":false}}}"#,
        ),
        (
            "proj-missing-committed",
            r#"{"data":{"_capability_name":"workspace.commit","_capability_output":{"revision":"rev_staged"}}}"#,
        ),
        (
            "proj-extra-member",
            r#"{"data":{"_capability_name":"workspace.commit","_capability_output":{"revision":"rev_staged","committed":true,"note":"not the output shape"}}}"#,
        ),
        ("proj-corrupt", "not json at all"),
    ];
    for (run_id, context) in unbacked {
        stage_context_run_row(pool_b.as_ref(), run_id, CREATOR, context).await;
        let detail = handle_b
            .get_workflow_session(&principal_b, run_id.to_string())
            .await
            .unwrap_or_else(|err| panic!("{run_id} must stay readable: {err:?}"));
        assert_eq!(
            detail.session.session_id, run_id,
            "the unbacked row is still the requested session"
        );
        assert!(
            detail.workspace_commit.is_none(),
            "{run_id} must project nothing: {:?}",
            detail.workspace_commit
        );
    }

    // The control: the IDENTICAL context under the admitted Creator IS
    // projected, so the negatives above are about the checkpoint shape and the
    // stored owner — never a vacuous always-absent field.
    let control = handle_b
        .get_workflow_session(&principal_b, "proj-owned-control".to_string())
        .await
        .expect("the control row's detail");
    assert_eq!(
        control
            .workspace_commit
            .map(|commit| (commit.revision, commit.committed)),
        Some(("rev_staged".to_string(), true)),
        "an exact successful output under the admitted Creator is projected"
    );
    // The same checkpoint under another Creator is not readable at all: the
    // root row and its stored owner are authorized BEFORE the context is read.
    match handle_b
        .get_workflow_session(&principal_b, "proj-foreign".to_string())
        .await
        .unwrap_err()
    {
        nexus_core::CoreError::NotFound { resource } => assert_eq!(
            resource, "workflow session proj-foreign",
            "a foreign run closes as absent"
        ),
        other => panic!("a foreign checkpoint must not be readable, got {other:?}"),
    }

    let report = handle_b.close().await.expect("the reopened owner closes");
    assert!(
        report.cleanup_confirmed,
        "close must report confirmed cleanup"
    );
    core_b.close().await.expect("core close");
}
