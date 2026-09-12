//! v1.186 P2 Task 1 — public admission through the production daemon.
//!
//! Fixture: hermetic `LiveDaemon` (real router) plus JSON HTTP against
//! `/v1/daemon/orchestration/schedules`. CLI stdout is human-formatted
//! and is not treated as JSON. No live omp.

#![allow(clippy::unwrap_used, clippy::expect_used)]
#![allow(clippy::too_many_lines)] // CLI integration scenarios keep setup+assertions in one flow
#![allow(clippy::similar_names)] // session/step fixture ids intentionally cluster
#![allow(clippy::manual_let_else)] // test fixtures prefer explicit match arms over let-else
#![allow(clippy::format_push_string)] // assertion text built incrementally in one place

mod common;

use common::LiveDaemon;
use nexus_agent_host::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream, HostHealth,
    HostOperation, HostStartConfig, OperationFinishedEvent, OperationStartedEvent, ProtocolKind,
    ProviderHealth, TextDeltaEvent,
};
use nexus_agent_host::{
    DiscoverySource, HostError, HostFacade, HostOperationId, HostResult, HostSession,
    HostSessionId, LaunchStrategy, ProviderCatalog, ProviderCatalogEntry, SessionState, TrustLevel,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const PUBLIC_PRESET: &str = "memory-augmented";
const SYSTEM_PRESET: &str = "_system.maintenance";
const MOCK_PROVIDER: &str = "mock-provider";

/// Deterministic non-echo Host fixture (N-7): every prompt returns a fixed
/// transformed string (never the prompt text), records the rendered prompt,
/// and advertises `mock-provider` in its catalog so admission can validate
/// binding references before enqueue.
struct MockHost {
    prompts: Mutex<Vec<String>>,
    execs: AtomicU64,
    events: tokio::sync::broadcast::Sender<HostEvent>,
}

impl MockHost {
    fn new() -> Arc<Self> {
        let (events, _) = tokio::sync::broadcast::channel(64);
        Arc::new(Self {
            prompts: Mutex::new(Vec::new()),
            execs: AtomicU64::new(0),
            events,
        })
    }

    fn prompts(&self) -> Vec<String> {
        self.prompts.lock().expect("prompts").clone()
    }
}

#[async_trait::async_trait]
impl HostFacade for MockHost {
    async fn start(&self, _config: HostStartConfig) -> HostResult<()> {
        Ok(())
    }

    async fn create_session(
        &self,
        request: nexus_agent_host::capability::CreateSessionRequest,
    ) -> HostResult<HostSession> {
        Ok(HostSession {
            id: HostSessionId::new(),
            provider_id: request.provider_id,
            state: SessionState::Ready,
            created_at: chrono::Utc::now(),
            active_op_id: None,
            negotiated_capabilities: CapabilityDescriptor::native_cli_limited(),
            owner: request.owner,
            process_identity: None,
        })
    }

    async fn exec(
        &self,
        session_id: HostSessionId,
        op: HostOperation,
    ) -> HostResult<HostEventStream> {
        self.execs.fetch_add(1, Ordering::SeqCst);
        let op_id = match op {
            HostOperation::Prompt { op_id, content, .. } => {
                let text = match content.as_slice() {
                    [HostContentBlock::Text { text }] => text.clone(),
                    other => format!("unexpected content {other:?}"),
                };
                self.prompts.lock().expect("prompts").push(text);
                op_id
            }
            other => {
                return Err(HostError::internal(format!(
                    "unexpected operation {other:?}"
                )));
            }
        };
        let started = HostEvent::OpStarted(OperationStartedEvent {
            op_id: op_id.clone(),
            session_id: session_id.clone(),
        });
        let delta = HostEvent::MessageDelta(TextDeltaEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            text: "transformed:mock-output".to_string(),
        });
        let finished = HostEvent::OpFinished(OperationFinishedEvent {
            session_id,
            op_id,
            reason: FinishReason::EndTurn,
        });
        let _ = self.events.send(started.clone());
        let _ = self.events.send(delta.clone());
        let _ = self.events.send(finished.clone());
        Ok(Box::pin(futures_util::stream::iter(vec![
            Ok(started),
            Ok(delta),
            Ok(finished),
        ])))
    }

    async fn cancel(&self, _op_id: HostOperationId) -> HostResult<()> {
        Ok(())
    }

    async fn health(&self) -> HostResult<HostHealth> {
        Ok(HostHealth {
            running: true,
            active_sessions: 0,
            active_operations: 0,
        })
    }

    async fn shutdown(&self) -> HostResult<()> {
        Ok(())
    }

    async fn shutdown_session(&self, _session_id: HostSessionId) -> HostResult<()> {
        Ok(())
    }

    async fn list_sessions(&self) -> HostResult<Vec<HostSession>> {
        Ok(Vec::new())
    }

    async fn provider_catalog(&self) -> HostResult<ProviderCatalog> {
        Ok(ProviderCatalog {
            entries: vec![ProviderCatalogEntry {
                provider_id: nexus_agent_host::ProviderId::new(MOCK_PROVIDER),
                display_name: MOCK_PROVIDER.to_string(),
                protocol_kind: ProtocolKind::NativeCli,
                launch: LaunchStrategy::NativeCli {
                    command: "mock".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                },
                source: DiscoverySource::Config,
                trust: TrustLevel::Explicit,
                capabilities: CapabilityDescriptor::native_cli_limited(),
                health: ProviderHealth {
                    provider_id: nexus_agent_host::ProviderId::new(MOCK_PROVIDER),
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
        self.events.subscribe()
    }
}

#[derive(sqlx::FromRow)]
struct ScheduleDriveRow {
    status: String,
    execution_policy: String,
    current_session_id: Option<String>,
}

async fn post_json(daemon: &LiveDaemon, path: &str, body: Value) -> (reqwest::StatusCode, Value) {
    let resp = reqwest::Client::new()
        .post(format!("{}{path}", daemon.http_url))
        .json(&body)
        .send()
        .await
        .expect("POST schedule API");
    let status = resp.status();
    let text = resp.text().await.expect("response body");
    let json: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "raw": text }));
    (status, json)
}

async fn add_public(daemon: &LiveDaemon, label: &str) -> (reqwest::StatusCode, Value) {
    post_json(
        daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": label,
            "seed": "p2-t1-topic",
            "input": {
                "keyword": "p2-t1",
                "topic": "p2-t1-topic"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await
}

async fn load_drive_row(daemon: &LiveDaemon, schedule_id: &str) -> ScheduleDriveRow {
    sqlx::query_as::<_, ScheduleDriveRow>(
        "SELECT status, execution_policy, current_session_id
         FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(schedule_id)
    .fetch_one(&daemon.pool)
    .await
    .expect("load schedule drive row")
}

/// Build a frozen admission descriptor for a seeded row (N-4b/N-5b): the
/// REAL content-addressed source identity (never a zero hash) plus a valid
/// `default` binding so admission's binding-completeness gate passes.
fn seeded_descriptor_json() -> Vec<u8> {
    let source = nexus_orchestration::preset::embedded_source_identity(PUBLIC_PRESET)
        .expect("embedded source identity");
    serde_json::to_vec(&serde_json::json!({
        "creator_id": "test_creator",
        "work_id": null,
        "workspace_root": "",
        "preset_id": PUBLIC_PRESET,
        "preset_version": 1,
        "source": source,
        "input": {},
        "agent_bindings": { "default": { "provider_id": MOCK_PROVIDER, "model": null } },
        "parent_session_id": null,
        "graph_name": null
    }))
    .expect("descriptor json")
}

/// Seed the durable core-context version-0 record for a raw-seeded row
/// (N-3: a version-0 pointer with no record is a pre-seed row and refuses).
async fn seed_core_context_record(daemon: &LiveDaemon, schedule_id: &str) {
    let now = chrono::Utc::now().timestamp();
    let payload = serde_json::json!({ "kind": "text", "body": "" });
    let derivation = serde_json::json!({ "kind": "seed", "raw": "" });
    sqlx::query(
        "INSERT INTO core_context_versions
           (schedule_id, version, payload_kind, content,
            derivation_kind, derivation_detail,
            created_at, created_by_kind, created_by_user_id)
         VALUES (?, 0, 'text', ?, 'seed', ?, ?, 'system', NULL)",
    )
    .bind(schedule_id)
    .bind(serde_json::to_vec(&payload).expect("payload"))
    .bind(serde_json::to_vec(&derivation).expect("derivation"))
    .bind(now)
    .execute(&daemon.pool)
    .await
    .expect("seed core context");
}

/// Load the durable v1 run record (status + parsed run state) for a session.
async fn load_run_record(daemon: &LiveDaemon, session_id: &str) -> (String, Value) {
    let (status, state): (String, Option<Vec<u8>>) = sqlx::query_as(
        "SELECT status, run_state_json FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(session_id)
    .fetch_one(&daemon.pool)
    .await
    .expect("load run record");
    let state: Value =
        serde_json::from_slice(state.as_deref().unwrap_or(b"{}")).expect("run state json");
    (status, state)
}

/// Wait until the Host fixture has recorded at least one prompt.
async fn wait_for_prompts(host: &MockHost, timeout_secs: u64) -> Vec<String> {
    tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        loop {
            let prompts = host.prompts();
            if !prompts.is_empty() {
                return prompts;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("host prompt recorded")
}

/// Wait until the durable run record reaches `expected` status, then return
/// the record. Panics with a diagnostic on timeout.
async fn wait_for_run_status(
    daemon: &LiveDaemon,
    session_id: &str,
    expected: &str,
    timeout_secs: u64,
) -> (String, Value) {
    if let Ok(record) = tokio::time::timeout(std::time::Duration::from_secs(timeout_secs), async {
        loop {
            let record = load_run_record(daemon, session_id).await;
            if record.0 == expected {
                return record;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    {
        record
    } else {
        let (status, state) = load_run_record(daemon, session_id).await;
        panic!("run {session_id} did not reach {expected}; status={status} state={state:?}");
    }
}

/// Build a frozen admission descriptor for a seeded `combat-engine` row
/// (N-14 failed-driver fixture): the REAL content-addressed source
/// identity plus an empty binding map (the preset has no prompt roles).
fn seeded_combat_descriptor_json() -> Vec<u8> {
    let source = nexus_orchestration::preset::embedded_source_identity("combat-engine")
        .expect("combat-engine embedded source identity");
    serde_json::to_vec(&serde_json::json!({
        "creator_id": "test_creator",
        "work_id": null,
        "workspace_root": "",
        "preset_id": "combat-engine",
        "preset_version": 1,
        "source": source,
        "input": {},
        "agent_bindings": {},
        "parent_session_id": null,
        "graph_name": null
    }))
    .expect("combat descriptor json")
}

/// N-7: production daemon + deterministic Host fixture — a driven run must
/// show REAL non-echo provider progress (the fixture records the rendered
/// prompt and returns a transformed string), not a status-only flip.
#[tokio::test]
async fn admission_production_host_progress_non_echo() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-host-progress",
            "seed": "p2-t1-topic",
            "input": {
                "keyword": "p2-t1",
                "topic": "p2-t1-topic"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "public add with valid binding must succeed: {body}"
    );
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    // The drive loop steps recall → generate (Host prompt) → persist, then
    // parks at the manual wait. Wait for the prompt to be recorded.
    let prompts = if let Ok(prompts) =
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let prompts = host.prompts();
                if !prompts.is_empty() {
                    return prompts;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
    {
        prompts
    } else {
        // Diagnostic: dump the durable run state so a drive failure is
        // visible instead of a bare timeout.
        let row = load_drive_row(&daemon, &schedule_id).await;
        let sid = row.current_session_id.unwrap_or_default();
        let record: (String, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT status, run_state_json FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&sid)
        .fetch_one(&daemon.pool)
        .await
        .expect("load run record for diagnostic");
        panic!(
            "host prompt not recorded; schedule status={} session={} run_status={} state={:?}",
            row.status,
            sid,
            record.0,
            record.1.map(|b| String::from_utf8_lossy(&b).to_string())
        );
    };

    assert!(
        prompts.iter().any(|p| p.contains("p2-t1-topic")),
        "rendered prompt must carry the frozen preset.input.topic: {prompts:?}"
    );
    assert!(
        prompts
            .iter()
            .all(|p| !p.contains("transformed:mock-output")),
        "the fixture output must never be an echo of the prompt"
    );
    assert!(
        host.execs.load(Ordering::SeqCst) >= 1,
        "at least one Host exec must have run"
    );

    // The run progressed past the generate state (real provider progress):
    // the durable v1 record parks at the manual wait with no failure — never
    // a dead running row and never a failed driver.
    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("driven schedule must own a session")
        .to_string();
    let (status, state) = if let Ok(parked) =
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let record: (String, Option<Vec<u8>>) = sqlx::query_as(
                "SELECT status, run_state_json FROM orchestration_sessions WHERE session_id = ?",
            )
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("load run record");
                if record.0 == "waiting_for_input" {
                    let state: Value = serde_json::from_slice(record.1.as_deref().unwrap_or(b"{}"))
                        .expect("run state json");
                    return (record.0, state);
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
    {
        parked
    } else {
        let record: (String, Option<Vec<u8>>) = sqlx::query_as(
            "SELECT status, run_state_json FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&sid)
        .fetch_one(&daemon.pool)
        .await
        .expect("load run record for diagnostic");
        panic!(
            "run did not park at manual wait; status={} state={:?}",
            record.0,
            record.1.map(|b| String::from_utf8_lossy(&b).to_string())
        );
    };
    assert_eq!(status, "waiting_for_input");
    assert!(
        state.get("failure").is_none_or(Value::is_null),
        "no durable failure after real provider progress: {state}"
    );
}

/// N-7: TRUE concurrent admission of ONE pending schedule — two racing
/// `signal: start` calls on a fresh pending row must mint exactly one
/// owned session (the atomic store claim linearizes; the loser re-reads
/// the winner).
#[tokio::test]
async fn admission_concurrent_pending_row_one_session() {
    let daemon = LiveDaemon::start().await;
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy,
            execution_descriptor_json)
           VALUES ('SCHCONCURRENT', 'test_creator', ?, 1, 'pending',
                   'serial', 0, 'p2-t1-concurrent-pending', ?, ?, NULL, 'driven_v1', ?)",
    )
    .bind(PUBLIC_PRESET)
    .bind(now)
    .bind(now)
    .bind(seeded_descriptor_json())
    .execute(&daemon.pool)
    .await
    .expect("seed fresh pending driven_v1 row");
    // N-3: a version-0 pointer with no core_context_versions record is a
    // pre-seed row and admission refuses it — seed the durable record.
    seed_core_context_record(&daemon, "SCHCONCURRENT").await;

    let path = "/v1/daemon/orchestration/schedules/SCHCONCURRENT/signal";
    let payload = json!({ "signal": "start" });
    let (a, b) = tokio::join!(
        post_json(&daemon, path, payload.clone()),
        post_json(&daemon, path, payload),
    );
    assert!(
        a.0.is_success() && b.0.is_success(),
        "both concurrent admissions must resolve to the same owned run: {} / {}",
        a.1,
        b.1
    );

    let row = load_drive_row(&daemon, "SCHCONCURRENT").await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("exactly one owned session");
    assert!(!sid.is_empty());
    assert_eq!(row.execution_policy, "driven_v1");
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?")
            .bind(sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("count sessions");
    assert_eq!(count, 1, "one pending row must mint exactly one session");
}

/// N-1: TRUE concurrent admission of TWO DISTINCT serial schedules for the
/// same creator — both preflights see an empty running set, but the store's
/// in-transaction matrix recheck lets exactly ONE claim; the other stays
/// pending and unowned.
///
/// Determinism (R-2): the winner's run must STAY in flight until the
/// loser's recheck runs. A hostless daemon fails the winner's drive
/// instantly and settles its schedule row terminal — the loser's gate then
/// legitimately sees an empty running set and also admits (serial means
/// no OVERLAP, and a settled run no longer overlaps). The blocking host
/// holds the winner's first prompt open so the race is between the two
/// claims, never between a claim and a settle.
#[tokio::test]
async fn admission_distinct_serial_rows_race_one_owned() {
    let daemon = LiveDaemon::start_with_agent_host(BlockingHost::non_cooperative()).await;
    let now = chrono::Utc::now().timestamp();
    for id in ["SCHRACE1", "SCHRACE2"] {
        sqlx::query(
            "INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version, label,
                created_at, updated_at, work_id, execution_policy,
                execution_descriptor_json)
               VALUES (?, 'test_creator', ?, 1, 'pending',
                       'serial', 0, ?, ?, ?, NULL, 'driven_v1', ?)",
        )
        .bind(id)
        .bind(PUBLIC_PRESET)
        .bind(format!("p2-t1-race-{id}"))
        .bind(now)
        .bind(now)
        .bind(seeded_descriptor_json())
        .execute(&daemon.pool)
        .await
        .expect("seed race row");
        // N-3: a version-0 pointer with no record is a pre-seed row and
        // refuses — seed the durable record.
        seed_core_context_record(&daemon, id).await;
    }

    let path = "/v1/daemon/orchestration/schedules/SCHRACE1/signal";
    let payload = json!({ "signal": "start" });
    let (a, b) = tokio::join!(
        post_json(&daemon, path, payload.clone()),
        post_json(
            &daemon,
            "/v1/daemon/orchestration/schedules/SCHRACE2/signal",
            payload
        ),
    );
    // Exactly one admission wins; the loser is refused with a conflict and
    // its row stays pending and unowned (the store's in-transaction matrix
    // recheck linearizes the distinct-row serial race).
    let successes = [a.0.is_success(), b.0.is_success()];
    assert_eq!(
        successes.iter().filter(|s| **s).count(),
        1,
        "exactly one of the two racing admissions must succeed: {} / {}",
        a.1,
        b.1
    );
    let loser_body = if a.0.is_success() { &b.1 } else { &a.1 };
    assert_eq!(
        loser_body["error"]["code"].as_str(),
        Some("conflict"),
        "the serial loser must be refused with a conflict: {loser_body}"
    );

    let row1 = load_drive_row(&daemon, "SCHRACE1").await;
    let row2 = load_drive_row(&daemon, "SCHRACE2").await;
    let owned1 = row1
        .current_session_id
        .as_deref()
        .is_some_and(|s| !s.is_empty());
    let owned2 = row2
        .current_session_id
        .as_deref()
        .is_some_and(|s| !s.is_empty());
    assert!(
        owned1 ^ owned2,
        "exactly one of the two distinct serial rows must own a run: \
         SCHRACE1 owned={owned1} SCHRACE2 owned={owned2}"
    );
    let (winner, loser) = if owned1 {
        ("SCHRACE1", &row2)
    } else {
        ("SCHRACE2", &row1)
    };
    assert_eq!(
        loser.status, "pending",
        "the serial loser must stay pending and unowned: {winner} won, loser status={}",
        loser.status
    );
    assert!(
        loser.current_session_id.is_none(),
        "the serial loser must be unowned"
    );
}

/// N-7: dependency blocking — a schedule whose dependency is not yet
/// completed/cancelled stays pending and unowned on immediate admission.
#[tokio::test]
async fn admission_dependency_blocked_stays_pending() {
    let daemon = LiveDaemon::start().await;
    let now = chrono::Utc::now().timestamp();
    // Dependency target: pending (unfinished).
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy)
           VALUES ('SCHDEP', 'test_creator', ?, 1, 'pending',
                   'serial', 0, 'p2-t1-dep-target', ?, ?, NULL, 'driven_v1')",
    )
    .bind(PUBLIC_PRESET)
    .bind(now)
    .bind(now)
    .execute(&daemon.pool)
    .await
    .expect("seed dependency target");

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-dep-blocked",
            "depends_on": ["SCHDEP"],
            "seed": "p2-t1-topic",
            "input": { "keyword": "p2-t1", "topic": "p2-t1-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    assert_eq!(
        body["status"].as_str(),
        Some("pending"),
        "a schedule with an unfinished dependency must stay pending: {body}"
    );
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    let row = load_drive_row(&daemon, schedule_id).await;
    assert_eq!(row.status, "pending");
    assert!(
        row.current_session_id.is_none(),
        "blocked row must be unowned"
    );
}

/// N-7: serial concurrency blocking — a second serial schedule for the same
/// creator stays pending while the first driven run is running.
#[tokio::test]
async fn admission_serial_blocked_stays_pending() {
    let daemon = LiveDaemon::start().await;
    let (status, body) = add_public(&daemon, "p2-t1-serial-first").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let first_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    let first = load_drive_row(&daemon, &first_id).await;
    assert!(
        first.current_session_id.is_some(),
        "first serial schedule must be admitted and owned"
    );

    let (status, body) = add_public(&daemon, "p2-t1-serial-second").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    assert_eq!(
        body["status"].as_str(),
        Some("pending"),
        "a second serial schedule must stay pending while the first runs: {body}"
    );
    let second_id = body["schedule_id"].as_str().expect("schedule_id");
    let row = load_drive_row(&daemon, second_id).await;
    assert_eq!(row.status, "pending");
    assert!(
        row.current_session_id.is_none(),
        "serial-blocked row is unowned"
    );
}

/// N-7: session POST honors the sanctioned `agentBindings` contract — a
/// valid binding is frozen into the run and driven; an unknown role refuses
/// with 400 before any run row is created.
#[tokio::test]
async fn session_post_bindings_accepted_and_unknown_refused() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Valid binding: accepted, driven, prompt executes through the Host.
    // The seed is a JSON object so `preset.input.*` is populated (the
    // memory-augmented recall state renders `{{preset.input.keyword}}`).
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/sessions",
        json!({
            "presetId": PUBLIC_PRESET,
            "creatorId": "test_creator",
            "seed": "{\"keyword\": \"p2-t1\", \"topic\": \"p2-t1-session\"}",
            "agentBindings": {
                "default": { "providerId": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "session POST with valid agentBindings must succeed: {body}"
    );
    let session_id = body["sessionId"].as_str().expect("sessionId").to_string();
    let prompts = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let prompts = host.prompts();
            if !prompts.is_empty() {
                return prompts;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("session POST run must reach the Host");
    assert!(
        prompts.iter().any(|p| p.contains("p2-t1-session")),
        "session run prompt must carry the frozen seed: {prompts:?}"
    );

    // Unknown role: refused before enqueue (422 invalid_input). N-4b: the
    // completeness gate fires first — the preset's graphs require `default`
    // and the map supplies only the unknown role.
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/sessions",
        json!({
            "presetId": PUBLIC_PRESET,
            "creatorId": "test_creator",
            "agentBindings": {
                "ghost_role": { "providerId": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "unknown role must refuse before enqueue (422 invalid_input): {body}"
    );
    let text = body.to_string();
    assert!(
        text.contains("missing agent binding") || text.contains("unknown role"),
        "refusal must name the missing/unknown role: {text}"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?")
            .bind(&session_id)
            .fetch_one(&daemon.pool)
            .await
            .expect("count sessions");
    assert_eq!(count, 1, "the refused POST must not mint a second session");
}

/// N-7: unknown provider in schedule admission refuses loudly (400), never
/// a silent pending row.
#[tokio::test]
async fn admission_unknown_provider_refuses() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-unknown-provider",
            "agent_bindings": {
                "default": { "provider_id": "no-such-provider" }
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "unknown provider must refuse before enqueue (422 invalid_input): {body}"
    );
    let text = body.to_string();
    assert!(
        text.contains("unknown provider"),
        "refusal must name the unknown provider: {text}"
    );
}

#[tokio::test]
async fn admission_new_public_schedule_is_driven() {
    let daemon = LiveDaemon::start().await;
    let (status, body) = add_public(&daemon, "p2-t1-admission").await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "public add must succeed: {body}"
    );
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    assert_ne!(
        body["status"].as_str(),
        Some("pending"),
        "new public add must not remain a dead pending row: {body}"
    );

    let row = load_drive_row(&daemon, schedule_id).await;
    assert_eq!(row.execution_policy, "driven_v1", "{schedule_id}");
    assert!(
        row.current_session_id
            .as_deref()
            .is_some_and(|s| !s.is_empty()),
        "driven schedule must own a session: status={} session={:?}",
        row.status,
        row.current_session_id
    );
    assert_ne!(row.status, "pending");

    let cli = daemon
        .cli(&[
            "daemon",
            "schedule",
            "add",
            "--preset",
            PUBLIC_PRESET,
            "--creator",
            "test_creator",
            "--label",
            "p2-t1-admission-cli",
            "--seed",
            "p2-t1-topic",
            "--agent-ref",
            "default:mock-provider",
        ])
        .await;
    assert!(
        cli.status.success(),
        "CLI add failed: {}",
        String::from_utf8_lossy(&cli.stderr)
    );
    let stdout = String::from_utf8_lossy(&cli.stdout);
    assert!(
        stdout.contains("schedule_id:"),
        "CLI add is human-formatted, not JSON: {stdout}"
    );
    assert!(
        serde_json::from_str::<Value>(stdout.trim()).is_err(),
        "CLI add must not be mistaken for JSON stdout"
    );
}

#[tokio::test]
async fn admission_system_maintenance_stays_inert() {
    let daemon = LiveDaemon::start().await;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": SYSTEM_PRESET,
            "label": "p2-t1-system"
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "_system.maintenance must insert as an inert row, not a typed refusal or 5xx: {body}"
    );
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    assert_ne!(
        body["status"].as_str(),
        Some("running"),
        "_system.maintenance must not start a driven run: {body}"
    );

    let row = load_drive_row(&daemon, schedule_id).await;
    assert_eq!(row.execution_policy, "system_inert", "{schedule_id}");
    assert!(
        row.current_session_id.is_none(),
        "system preset must not own a session: {:?}",
        row.current_session_id
    );
    assert_ne!(row.status, "running");
}

#[tokio::test]
async fn concurrent_start_yields_one_owned_session() {
    let daemon = LiveDaemon::start().await;
    let (status, body) = add_public(&daemon, "p2-t1-concurrent").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    let path = format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal");
    let payload = json!({ "signal": "start" });
    let (a, b) = tokio::join!(
        post_json(&daemon, &path, payload.clone()),
        post_json(&daemon, &path, payload),
    );
    assert!(
        a.0.is_success() || b.0.is_success(),
        "at least one start must succeed: {} / {}",
        a.1,
        b.1
    );

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("exactly one owned session");
    assert!(!sid.is_empty(), "owned session must be non-empty");
    assert_eq!(row.execution_policy, "driven_v1");
}

/// C-3: explicit start for REAL historical rows — `execution_descriptor_json
/// IS NULL`, no version-0 core-context record, `legacy_inert` policy
/// (the migration default). Pending, paused, and admission-only `Running`
/// rows stay inert across boot/tick/cron until the explicit public
/// `schedule start`; the call resolves the preset, freezes the source
/// identity + sanctioned bindings, creates the missing version-0 seed,
/// persists the frozen descriptor, and atomically cuts the row over with
/// exactly one owned session.
#[tokio::test]
async fn explicit_legacy_running_without_session_starts() {
    let host = MockHost::new();
    // qc1 F-001: an explicit legacy start now requires a CONFIGURED default
    // binding provider — the Host catalog's first entry is never selected
    // as a fallback. This fixture mirrors production (one enabled provider).
    let daemon = LiveDaemon::start_with_agent_host_and_provider(host.clone()).await;
    let now = chrono::Utc::now().timestamp();

    // Seed three REAL historical rows: NULL descriptor, no version-0
    // record, `legacy_inert` (migration default), `current_core_context_version`
    // already 0 (migration default). `parallel_any` so the three rows do
    // not serial-block each other during the sequential explicit starts.
    for (id, status) in [
        ("SCHLEGACY-PENDING", "pending"),
        ("SCHLEGACY-PAUSED", "paused"),
        ("SCHLEGACY-RUNNING", "running"),
    ] {
        sqlx::query(
            "INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version, label,
                created_at, updated_at, work_id, execution_policy,
                execution_descriptor_json)
               VALUES (?, 'test_creator', ?, 1, ?, 'parallel_any', 0, ?, ?, ?, NULL, 'legacy_inert', NULL)",
        )
        .bind(id)
        .bind(PUBLIC_PRESET)
        .bind(status)
        .bind(format!("p2-t1-historical-{id}"))
        .bind(now)
        .bind(now)
        .execute(&daemon.pool)
        .await
        .expect("seed historical row");
        // No core_context_versions row is created — the historical shape.
        let seed_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM core_context_versions WHERE schedule_id = ?")
                .bind(id)
                .fetch_one(&daemon.pool)
                .await
                .expect("count seeds");
        assert_eq!(
            seed_count, 0,
            "historical row {id} must have no version-0 seed"
        );
    }

    // Inertness before the explicit start: boot resume, tick, and cron
    // must leave every historical row unchanged and unowned.
    let supervisor = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired");
    let paused = supervisor
        .resume_running_as_paused("daemon_restart")
        .await
        .expect("resume running as paused");
    assert_eq!(paused, 0, "no driven_v1 running rows to pause");
    supervisor.tick().await.expect("tick succeeds");
    nexus_daemon_runtime::cron_supervisor::run_one_tick(
        &daemon.pool,
        std::path::Path::new(""),
        &supervisor,
        Some(MOCK_PROVIDER),
    )
    .await;
    for (id, status) in [
        ("SCHLEGACY-PENDING", "pending"),
        ("SCHLEGACY-PAUSED", "paused"),
        ("SCHLEGACY-RUNNING", "running"),
    ] {
        let row = load_drive_row(&daemon, id).await;
        assert_eq!(
            row.status, status,
            "historical row {id} must keep its status before explicit start"
        );
        assert_eq!(
            row.execution_policy, "legacy_inert",
            "historical row {id} must stay legacy_inert before explicit start"
        );
        assert!(
            row.current_session_id.is_none(),
            "historical row {id} must stay unowned before explicit start"
        );
    }

    // Explicit public start for each row: the coordinator prepares the
    // missing frozen payload (descriptor + version-0 seed + sanctioned
    // bindings) and atomically cuts the row over with one owned session.
    for (id, status) in [
        ("SCHLEGACY-PENDING", "pending"),
        ("SCHLEGACY-PAUSED", "paused"),
        ("SCHLEGACY-RUNNING", "running"),
    ] {
        let (status_code, body) = post_json(
            &daemon,
            &format!("/v1/daemon/orchestration/schedules/{id}/signal"),
            json!({ "signal": "start" }),
        )
        .await;
        assert!(
            status_code.is_success(),
            "historical {status} row {id} start failed: status={status_code} body={body}"
        );

        let row = load_drive_row(&daemon, id).await;
        assert_eq!(
            row.execution_policy, "driven_v1",
            "historical row {id} must be cut over to driven_v1"
        );
        assert!(
            row.current_session_id
                .as_deref()
                .is_some_and(|s| !s.is_empty()),
            "historical row {id} must own exactly one session: status={} session={:?}",
            row.status,
            row.current_session_id
        );

        // The frozen descriptor and version-0 seed were created by the
        // preparation step.
        let descriptor: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT execution_descriptor_json FROM creator_schedules WHERE schedule_id = ?",
        )
        .bind(id)
        .fetch_one(&daemon.pool)
        .await
        .expect("descriptor");
        assert!(
            descriptor.is_some(),
            "historical row {id} must have a frozen descriptor after explicit start"
        );
        let seed_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM core_context_versions WHERE schedule_id = ? AND version = 0",
        )
        .bind(id)
        .fetch_one(&daemon.pool)
        .await
        .expect("count seeds");
        assert_eq!(
            seed_count, 1,
            "historical row {id} must have exactly one version-0 seed after explicit start"
        );

        // Exactly one owned session row.
        let sid = row.current_session_id.expect("session id");
        let session_count: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?")
                .bind(&sid)
                .fetch_one(&daemon.pool)
                .await
                .expect("count sessions");
        assert_eq!(
            session_count, 1,
            "historical row {id} must own exactly one session"
        );
    }
}

#[tokio::test]
async fn future_scheduled_add_stays_pending() {
    let daemon = LiveDaemon::start().await;
    let future = chrono::Utc::now().timestamp() + 2 * 60 * 60;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-gated-clock",
            "scheduled_at": future.to_string(),
            "seed": "p2-t1-topic",
            "input": {
                "keyword": "p2-t1",
                "topic": "p2-t1-topic"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    assert_eq!(
        body["status"].as_str(),
        Some("pending"),
        "future scheduled_at must defer admission: {body}"
    );
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    let row = load_drive_row(&daemon, schedule_id).await;
    assert_eq!(row.status, "pending");
    assert!(row.current_session_id.is_none());
    assert_eq!(row.execution_policy, "driven_v1");
}

/// N-2/N-2b: lazy Profile attach publishes ONE immutable aggregate bundle
/// (engine + pool-backed capability registry + Host prompt executor +
/// coordinator + supervisor) and a lazy-attached schedule drives a REAL
/// Host prompt through the production executor — never a Tier-0
/// pool-less/executor-less registry.
#[tokio::test]
async fn admission_lazy_attach_drives_host_prompt() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_lazy_attach(host.clone()).await;

    // The aggregate bundle is published and every accessor reads it.
    let bundle = daemon
        .state
        .runtime_bundle()
        .expect("lazy attach publishes the aggregate bundle");
    assert!(
        daemon.state.engine().is_some(),
        "engine accessor must read the aggregate"
    );
    assert!(
        daemon.state.schedule_supervisor().is_some(),
        "supervisor accessor must read the aggregate"
    );
    assert!(
        daemon.state.capability_registry().is_some(),
        "capability registry accessor must read the aggregate"
    );
    let _ = bundle;

    // A lazy-attached schedule drives a real Host prompt (non-echo).
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-lazy-attach",
            "seed": "p2-t1-lazy-topic",
            "input": {
                "keyword": "p2-t1",
                "topic": "p2-t1-lazy-topic"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "lazy-attached add must succeed: {body}"
    );
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    let prompts = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let prompts = host.prompts();
            if !prompts.is_empty() {
                return prompts;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("lazy-attached run must reach the Host");
    assert!(
        prompts.iter().any(|p| p.contains("p2-t1-lazy-topic")),
        "lazy-attached run prompt must carry the frozen input: {prompts:?}"
    );
    assert!(
        host.execs.load(Ordering::SeqCst) >= 1,
        "at least one Host exec must run through the lazy-attached bundle"
    );
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert!(
        row.current_session_id
            .as_deref()
            .is_some_and(|s| !s.is_empty()),
        "lazy-attached schedule must own a session"
    );
}

/// N-13: the NORMAL boot publication path (`publish_boot_runtime_bundle`)
/// publishes a pool-backed capability registry — `kb.extract_work` on the
/// boot aggregate carries the Creator DB dependency and executes real DB
/// work, never a pool-less placeholder. The lazy-attach assertion above is
/// a SEPARATE path; this fixture exercises the boot publisher directly.
#[tokio::test]
async fn admission_normal_boot_bundle_pool_backed_capability() {
    let daemon = LiveDaemon::start_normal_boot().await;

    // The boot aggregate is published and every accessor reads it.
    let bundle = daemon
        .state
        .runtime_bundle()
        .expect("normal boot publishes the aggregate bundle");
    assert!(
        daemon.state.engine().is_some(),
        "engine accessor must read the boot aggregate"
    );
    assert!(
        daemon.state.schedule_supervisor().is_some(),
        "supervisor accessor must read the boot aggregate"
    );
    assert!(
        daemon.state.run_coordinator().is_some(),
        "coordinator accessor must read the boot aggregate"
    );
    let registry = daemon
        .state
        .capability_registry()
        .expect("capability registry accessor must read the boot aggregate");
    let _ = bundle;

    // N-13: `kb.extract_work` on the BOOT aggregate is pool-backed — it
    // must NOT return WorkerUnavailable (the pool-less placeholder mode).
    // A worldless input is a success no-op in the pool-backed pipeline
    // (never WorkerUnavailable), proving the Creator DB dependency is
    // wired into the normal-boot registry.
    let kb = registry
        .get("kb.extract_work")
        .expect("kb.extract_work must be registered on the boot aggregate");
    let out = kb
        .run(serde_json::json!({ "creator_id": "test_creator" }))
        .await
        .expect("pool-backed kb.extract_work must execute, not return WorkerUnavailable");
    assert_eq!(
        out.get("status").and_then(Value::as_str),
        Some("skipped"),
        "worldless no-op must be the pool-backed pipeline's response: {out}"
    );
}

/// N-7/N-14: legacy never-started rows (pending/running-without-session,
/// `legacy_inert`) stay inert across boot/tick/cron — the supervisor tick
/// never admits them, never flips them to Running, and the cron admission
/// tick leaves them untouched. The boot path (`resume_running_as_paused`)
/// only pauses `driven_v1` Running rows, so a legacy `running` row keeps
/// its historical status.
#[tokio::test]
async fn admission_legacy_rows_inert_across_tick() {
    let daemon = LiveDaemon::start().await;
    let now = chrono::Utc::now().timestamp();
    for (id, status) in [("SCHLEGACY1", "pending"), ("SCHLEGACY2", "running")] {
        sqlx::query(
            "INSERT INTO creator_schedules
               (schedule_id, creator_id, preset_id, preset_version, status,
                concurrency_kind, current_core_context_version, label,
                created_at, updated_at, work_id, execution_policy)
               VALUES (?, 'test_creator', ?, 1, ?, 'serial', 0, ?, ?, ?, NULL, 'legacy_inert')",
        )
        .bind(id)
        .bind(PUBLIC_PRESET)
        .bind(status)
        .bind(format!("p2-t1-legacy-{id}"))
        .bind(now)
        .bind(now)
        .execute(&daemon.pool)
        .await
        .expect("seed legacy row");
    }

    // Boot path: `resume_running_as_paused` must NOT touch legacy rows —
    // only `driven_v1` Running rows are paused on restart.
    let supervisor = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired");
    let paused = supervisor
        .resume_running_as_paused("daemon_restart")
        .await
        .expect("resume running as paused");
    assert_eq!(paused, 0, "no driven_v1 running rows to pause");
    for id in ["SCHLEGACY1", "SCHLEGACY2"] {
        let row = load_drive_row(&daemon, id).await;
        let expected = if id == "SCHLEGACY1" {
            "pending"
        } else {
            "running"
        };
        assert_eq!(
            row.status, expected,
            "legacy row {id} must keep its historical status after boot resume"
        );
        assert!(
            row.current_session_id.is_none(),
            "legacy row {id} must stay unowned after boot resume"
        );
    }

    // Supervisor tick (the same admission path boot/tick use).
    supervisor.tick().await.expect("tick succeeds");

    // Cron admission tick (the cron supervisor's step 2 path).
    nexus_daemon_runtime::cron_supervisor::run_one_tick(
        &daemon.pool,
        std::path::Path::new(""),
        &supervisor,
        Some(MOCK_PROVIDER),
    )
    .await;

    for id in ["SCHLEGACY1", "SCHLEGACY2"] {
        let row = load_drive_row(&daemon, id).await;
        assert_eq!(
            row.execution_policy, "legacy_inert",
            "legacy row {id} must stay legacy_inert after tick/cron"
        );
        assert!(
            row.current_session_id.is_none(),
            "legacy row {id} must stay unowned after tick/cron"
        );
        // A3: operator-visible historical status is preserved — a legacy
        // `running` row stays `running` (never driven, never re-flipped);
        // a legacy `pending` row stays `pending` (never flipped to running).
        let expected = if id == "SCHLEGACY1" {
            "pending"
        } else {
            "running"
        };
        assert_eq!(
            row.status, expected,
            "legacy row {id} must keep its historical status after tick/cron"
        );
    }
}

/// N-16: `resume_schedule` counts only `driven_v1` rows toward execution
/// capacity — a historical `legacy_inert` `Running` row must not block an
/// eligible driven paused row's explicit resume. The driven row is admitted
/// through the production starter while the legacy row stays unchanged and
/// unowned.
#[tokio::test]
async fn admission_resume_ignores_legacy_running_capacity() {
    let daemon = LiveDaemon::start().await;
    let now = chrono::Utc::now().timestamp();

    // Historical legacy `Running` row (no owned session, migration default).
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy)
           VALUES ('SCHLEGACY-RUN', 'test_creator', ?, 1, 'running',
                   'serial', 0, 'p2-t1-legacy-run', ?, ?, NULL, 'legacy_inert')",
    )
    .bind(PUBLIC_PRESET)
    .bind(now)
    .bind(now)
    .execute(&daemon.pool)
    .await
    .expect("seed legacy running row");

    // Driven paused row (same creator, serial) with a frozen descriptor and
    // version-0 seed — eligible for explicit resume.
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy,
            execution_descriptor_json)
           VALUES ('SCHDRIVEN-PAUSED', 'test_creator', ?, 1, 'paused',
                   'serial', 0, 'p2-t1-driven-paused', ?, ?, NULL, 'driven_v1', ?)",
    )
    .bind(PUBLIC_PRESET)
    .bind(now)
    .bind(now)
    .bind(seeded_descriptor_json())
    .execute(&daemon.pool)
    .await
    .expect("seed driven paused row");
    seed_core_context_record(&daemon, "SCHDRIVEN-PAUSED").await;

    let supervisor = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired");

    // Explicit resume: the driven row is admitted through the production
    // starter (the legacy Running row is excluded from capacity).
    let new_status = supervisor
        .resume_schedule("SCHDRIVEN-PAUSED")
        .await
        .expect("resume driven paused row");
    assert_eq!(new_status, "running");

    let driven = load_drive_row(&daemon, "SCHDRIVEN-PAUSED").await;
    assert_eq!(driven.status, "running");
    assert!(
        driven
            .current_session_id
            .as_deref()
            .is_some_and(|s| !s.is_empty()),
        "driven row must own a session after resume"
    );

    // The legacy row stays unchanged and unowned.
    let legacy = load_drive_row(&daemon, "SCHLEGACY-RUN").await;
    assert_eq!(
        legacy.status, "running",
        "legacy row keeps its historical status"
    );
    assert_eq!(legacy.execution_policy, "legacy_inert");
    assert!(
        legacy.current_session_id.is_none(),
        "legacy row stays unowned"
    );
}

/// N-10: auto-chain, cron, and review-master insertion branches are driven
/// through the PRODUCTION coordinator/starter with a deterministic Host
/// fixture — each branch asserts an owned session identity, real provider
/// progress, and absence of durable driver failure. Starter/provider
/// failures fail the test; durability-only row assertions are not
/// sufficient.
#[tokio::test]
async fn admission_internal_insertion_branches_durable() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Auto-chain branch (research stage).
    let work = nexus_local_db::works::WorkRecord {
        work_id: "wrk_chain".to_string(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Chain Work".to_string(),
        long_term_goal: "goal".to_string(),
        initial_idea: "idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-09T10:00:00Z".to_string(),
        updated_at: "2026-06-09T10:00:00Z".to_string(),
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        work_ref: Some("chain-ref".to_string()),
        work_profile: Some("novel".to_string()),
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
    };
    nexus_local_db::works::create_work(&daemon.pool, &work)
        .await
        .expect("create chain work");

    // The production starter the supervisor tick uses — the SAME
    // coordinator ownership path every insertion branch must route through.
    let starter = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired")
        .schedule_starter_clone()
        .expect("production starter injected");

    // ── Auto-chain branch ──────────────────────────────────────────────
    let chain_id = nexus_orchestration::auto_chain::enqueue_auto_chain_schedule(
        &daemon.pool,
        "test_creator",
        "wrk_chain",
        "research",
        None,
        None,
        &work,
        nexus_orchestration::preset::default_bindings_for_preset("research", MOCK_PROVIDER)
            .expect("research preset resolves"),
        None,
    )
    .await
    .expect("enqueue auto-chain schedule");
    let chain_row = load_drive_row(&daemon, &chain_id).await;
    assert_eq!(chain_row.execution_policy, "driven_v1");
    let chain_cc: i64 = sqlx::query_scalar(
        "SELECT current_core_context_version FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(&chain_id)
    .fetch_one(&daemon.pool)
    .await
    .expect("chain core version");
    assert_eq!(
        chain_cc, 0,
        "auto-chain row must carry a durable seed pointer"
    );
    let chain_cc_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM core_context_versions WHERE schedule_id = ?")
            .bind(&chain_id)
            .fetch_one(&daemon.pool)
            .await
            .expect("chain cc rows");
    assert_eq!(
        chain_cc_rows, 1,
        "auto-chain row must have a durable seed record"
    );
    let chain_desc: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT execution_descriptor_json FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(&chain_id)
    .fetch_one(&daemon.pool)
    .await
    .expect("chain descriptor");
    let chain_desc: nexus_orchestration::run_state::RunDescriptorV1 =
        serde_json::from_slice(chain_desc.as_deref().expect("chain descriptor present"))
            .expect("chain descriptor parses");
    assert!(
        chain_desc.source
            != nexus_orchestration::run_state::PresetSourceIdentity::Embedded {
                preset_id: "research".to_string(),
                content_hash: [0; 32],
            },
        "auto-chain descriptor must carry the REAL content hash, never zero"
    );

    // N-10: drive through the production starter — owned session, real
    // Host progress, no durable driver failure.
    let chain_sid = starter
        .start(&chain_id)
        .await
        .expect("auto-chain admitted through the production starter");
    let chain_prompts = if let Ok(prompts) =
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let prompts = host.prompts();
                if !prompts.is_empty() {
                    return prompts;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
    {
        prompts
    } else {
        let (status, state) = load_run_record(&daemon, &chain_sid.0).await;
        let row = load_drive_row(&daemon, &chain_id).await;
        let ctx: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT context_json FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&chain_sid.0)
        .fetch_one(&daemon.pool)
        .await
        .expect("load context");
        let ctx_text = ctx
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        let run_error = serde_json::from_str::<Value>(&ctx_text)
            .ok()
            .and_then(|v| v.get("data").cloned())
            .and_then(|d| d.get("_run_error").cloned())
            .unwrap_or(Value::Null);
        panic!(
            "auto-chain host prompt not recorded; session={} status={} state={:?} \
             schedule_status={} schedule_session={:?} run_error={:?}",
            chain_sid.0, status, state, row.status, row.current_session_id, run_error
        );
    };
    assert!(
        chain_prompts
            .iter()
            .all(|p| !p.contains("transformed:mock-output")),
        "auto-chain fixture output must never be an echo of the prompt"
    );
    let (chain_status, chain_state) =
        wait_for_run_status(&daemon, &chain_sid.0, "waiting_for_input", 15).await;
    assert_eq!(chain_status, "waiting_for_input");
    assert!(
        chain_state.get("failure").is_none_or(Value::is_null),
        "no durable failure after auto-chain provider progress: {chain_state}"
    );
    let chain_owned = load_drive_row(&daemon, &chain_id).await;
    assert_eq!(
        chain_owned.current_session_id.as_deref(),
        Some(chain_sid.0.as_str()),
        "auto-chain schedule must own the driven session"
    );

    // ── Cron branch ─────────────────────────────────────────────────────
    // Distinct creator: the auto-chain row is still `running` for
    // `test_creator` (parked at a manual wait), and the serial
    // per-creator gate must block a second driven row for the same
    // creator — each branch is proven independently.
    let cron_id = nexus_orchestration::auto_chain::enqueue_cron_schedule(
        &daemon.pool,
        "cron_creator",
        "wrk_chain",
        "novel-brainstorm",
        "brainstorm",
        nexus_orchestration::preset::default_bindings_for_preset("novel-brainstorm", MOCK_PROVIDER)
            .expect("novel-brainstorm preset resolves"),
    )
    .await
    .expect("enqueue cron schedule");
    let cron_row = load_drive_row(&daemon, &cron_id).await;
    assert_eq!(cron_row.execution_policy, "driven_v1");
    let cron_cc_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM core_context_versions WHERE schedule_id = ?")
            .bind(&cron_id)
            .fetch_one(&daemon.pool)
            .await
            .expect("cron cc rows");
    assert_eq!(cron_cc_rows, 1, "cron row must have a durable seed record");

    let cron_sid = starter
        .start(&cron_id)
        .await
        .expect("cron admitted through the production starter");
    let cron_prompts = wait_for_prompts(&host, 15).await;
    assert!(
        cron_prompts
            .iter()
            .all(|p| !p.contains("transformed:mock-output")),
        "cron fixture output must never be an echo of the prompt"
    );
    let (cron_status, cron_state) =
        wait_for_run_status(&daemon, &cron_sid.0, "waiting_for_input", 15).await;
    assert_eq!(cron_status, "waiting_for_input");
    assert!(
        cron_state.get("failure").is_none_or(Value::is_null),
        "no durable failure after cron provider progress: {cron_state}"
    );
    let cron_owned = load_drive_row(&daemon, &cron_id).await;
    assert_eq!(
        cron_owned.current_session_id.as_deref(),
        Some(cron_sid.0.as_str()),
        "cron schedule must own the driven session"
    );

    // ── Review-master branch ───────────────────────────────────────────
    let rvm_id = nexus_orchestration::auto_chain::enqueue_review_master_schedule(
        &daemon.pool,
        "rvm_creator",
        "wrk_chain",
        nexus_orchestration::preset::default_bindings_for_preset(
            "novel-review-master",
            MOCK_PROVIDER,
        )
        .expect("novel-review-master preset resolves"),
    )
    .await
    .expect("enqueue review-master schedule");
    let rvm_row = load_drive_row(&daemon, &rvm_id).await;
    assert_eq!(rvm_row.execution_policy, "driven_v1");
    let rvm_cc_rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM core_context_versions WHERE schedule_id = ?")
            .bind(&rvm_id)
            .fetch_one(&daemon.pool)
            .await
            .expect("rvm cc rows");
    assert_eq!(
        rvm_cc_rows, 1,
        "review-master row must have a durable seed record"
    );

    let rvm_sid = starter
        .start(&rvm_id)
        .await
        .expect("review-master admitted through the production starter");
    // review-master parks at the `present` manual wait; the enter action
    // (`creator.inject_prompt`) completed and the run is durably parked
    // with no failure — the observable acceptance contract for an
    // interactive preset.
    let (rvm_status, rvm_state) =
        wait_for_run_status(&daemon, &rvm_sid.0, "waiting_for_input", 15).await;
    assert_eq!(rvm_status, "waiting_for_input");
    assert!(
        rvm_state.get("failure").is_none_or(Value::is_null),
        "no durable failure after review-master progress: {rvm_state}"
    );
    let rvm_owned = load_drive_row(&daemon, &rvm_id).await;
    assert_eq!(
        rvm_owned.current_session_id.as_deref(),
        Some(rvm_sid.0.as_str()),
        "review-master schedule must own the driven session"
    );

    assert!(
        host.execs.load(Ordering::SeqCst) >= 2,
        "auto-chain + cron must each reach the Host (review-master parks at a manual wait)"
    );
}

/// N-4b: a schedule add with NO bindings for a preset whose graphs issue
/// prompts refuses loudly (400 `invalid_input`) before any run row is created
/// — the completeness gate, not the prompt executor, is the admission
/// validator.
#[tokio::test]
async fn admission_missing_default_binding_refuses() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-missing-binding",
            "seed": "p2-t1-topic",
            "input": { "keyword": "p2-t1", "topic": "p2-t1-topic" }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "missing default binding must refuse before enqueue: {body}"
    );
    let text = body.to_string();
    assert!(
        text.contains("missing agent binding"),
        "refusal must name the missing binding: {text}"
    );
    let count: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions")
        .fetch_one(&daemon.pool)
        .await
        .expect("count sessions");
    assert_eq!(count, 0, "no run row may be created for a refused add");
}

/// N-14: force-gates insertion — a gated preset with `force_gates=true`
/// and a non-empty reason bypasses gate evaluation (audited), inserts a
/// `driven_v1` row, and is admitted through the production coordinator
/// with real Host progress.
#[tokio::test]
async fn admission_force_gates_insertion_driven() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // A gated preset: `novel-brainstorm` requires work_profile=novel and
    // work_ref. The Work below satisfies the gates, but force-gates must
    // still bypass evaluation (audited) and drive.
    let work = nexus_local_db::works::WorkRecord {
        work_id: "wrk_force".to_string(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Force Work".to_string(),
        long_term_goal: "goal".to_string(),
        initial_idea: "idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-09T10:00:00Z".to_string(),
        updated_at: "2026-06-09T10:00:00Z".to_string(),
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        work_ref: Some("force-ref".to_string()),
        work_profile: Some("novel".to_string()),
        total_planned_chapters: Some(3),
        current_chapter: 0,
        auto_chain_enabled: false,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };
    nexus_local_db::works::create_work(&daemon.pool, &work)
        .await
        .expect("create force work");

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": "novel-brainstorm",
            "label": "p2-t1-force-gates",
            "force_gates": true,
            "reason": "p2-t1 acceptance: force-gates bypass",
            "input": {
                "work_id": "wrk_force",
                "work_ref": "force-ref",
                "topic": "p2-t1-force-topic",
                "open_findings": "[]"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "force-gates add must succeed: {body}"
    );
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    assert_eq!(
        body["status"].as_str(),
        Some("running"),
        "force-gates row must be admitted immediately: {body}"
    );

    // Audit row written.
    let audit: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM force_gates_audit WHERE preset_id = 'novel-brainstorm'",
    )
    .fetch_one(&daemon.pool)
    .await
    .expect("audit count");
    assert_eq!(audit, 1, "force-gates must write exactly one audit row");

    // Driven with real Host progress: the `gather` enter action
    // (`creator.inject_prompt`) enqueues the durable prompt injection; the
    // exit judge (`judge.llm` → acp.prompt) reaches the Host.
    let prompts = if let Ok(prompts) =
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let prompts = host.prompts();
                if !prompts.is_empty() {
                    return prompts;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
    {
        prompts
    } else {
        let row = load_drive_row(&daemon, &schedule_id).await;
        let sid = row.current_session_id.clone().unwrap_or_default();
        let (status, state) = load_run_record(&daemon, &sid).await;
        let ctx: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT context_json FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&sid)
        .fetch_one(&daemon.pool)
        .await
        .expect("load context");
        let ctx_text = ctx
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        let run_error = serde_json::from_str::<Value>(&ctx_text)
            .ok()
            .and_then(|v| v.get("data").cloned())
            .and_then(|d| d.get("_run_error").cloned())
            .unwrap_or(Value::Null);
        panic!(
            "force-gates host prompt not recorded; schedule={schedule_id} \
             schedule_status={} schedule_session={:?} session={sid} \
             status={status} state={state:?} run_error={run_error:?}",
            row.status, row.current_session_id,
        );
    };
    assert!(
        prompts
            .iter()
            .any(|p| p.contains("Gather Phase Exit Check")),
        "force-gates judge prompt must reach the Host: {prompts:?}"
    );
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_eq!(row.execution_policy, "driven_v1");
    let sid = row
        .current_session_id
        .as_deref()
        .expect("force-gates schedule must own a session")
        .to_string();
    let (run_status, run_state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    assert!(
        run_state.get("failure").is_none_or(Value::is_null),
        "no durable failure after force-gates progress: {run_state}"
    );
}

/// N-14: gated-work insertion — a gated preset whose Work satisfies the
/// gates is admitted through the production coordinator with real provider
/// progress; a Work that fails the gates refuses loudly (422) before any
/// run row is created.
#[tokio::test]
async fn admission_gated_work_insertion_driven() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Satisfying Work: work_profile=novel + work_ref present.
    let work = nexus_local_db::works::WorkRecord {
        work_id: "wrk_gated".to_string(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Gated Work".to_string(),
        long_term_goal: "goal".to_string(),
        initial_idea: "idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-09T10:00:00Z".to_string(),
        updated_at: "2026-06-09T10:00:00Z".to_string(),
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        work_ref: Some("gated-ref".to_string()),
        work_profile: Some("novel".to_string()),
        total_planned_chapters: Some(3),
        current_chapter: 0,
        auto_chain_enabled: false,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };
    nexus_local_db::works::create_work(&daemon.pool, &work)
        .await
        .expect("create gated work");

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": "novel-brainstorm",
            "label": "p2-t1-gated-work",
            "input": {
                "work_id": "wrk_gated",
                "work_ref": "gated-ref",
                "topic": "p2-t1-gated-topic",
                "open_findings": "[]"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CREATED,
        "gated add with satisfying Work must succeed: {body}"
    );
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    assert_eq!(
        body["status"].as_str(),
        Some("running"),
        "gated row must be admitted immediately: {body}"
    );
    let prompts = if let Ok(prompts) =
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let prompts = host.prompts();
                if !prompts.is_empty() {
                    return prompts;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
    {
        prompts
    } else {
        let row = load_drive_row(&daemon, &schedule_id).await;
        let sid = row.current_session_id.clone().unwrap_or_default();
        let (status, state) = load_run_record(&daemon, &sid).await;
        let ctx: Option<Vec<u8>> = sqlx::query_scalar(
            "SELECT context_json FROM orchestration_sessions WHERE session_id = ?",
        )
        .bind(&sid)
        .fetch_one(&daemon.pool)
        .await
        .expect("load context");
        let ctx_text = ctx
            .map(|b| String::from_utf8_lossy(&b).to_string())
            .unwrap_or_default();
        let run_error = serde_json::from_str::<Value>(&ctx_text)
            .ok()
            .and_then(|v| v.get("data").cloned())
            .and_then(|d| d.get("_run_error").cloned())
            .unwrap_or(Value::Null);
        panic!(
            "gated host prompt not recorded; schedule={schedule_id} \
             schedule_status={} schedule_session={:?} session={sid} \
             status={status} state={state:?} run_error={run_error:?}",
            row.status, row.current_session_id,
        );
    };
    assert!(
        prompts
            .iter()
            .any(|p| p.contains("Gather Phase Exit Check")),
        "gated judge prompt must reach the Host: {prompts:?}"
    );
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_eq!(row.execution_policy, "driven_v1");
    let sid = row
        .current_session_id
        .as_deref()
        .expect("gated schedule must own a session")
        .to_string();
    let (run_status, run_state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    assert!(
        run_state.get("failure").is_none_or(Value::is_null),
        "no durable failure after gated progress: {run_state}"
    );

    // Failing Work: work_profile=essay (not novel) → gate fails, 422, no row.
    let bad_work = nexus_local_db::works::WorkRecord {
        work_id: "wrk_gated_bad".to_string(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Bad Gated Work".to_string(),
        long_term_goal: "goal".to_string(),
        initial_idea: "idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-09T10:00:00Z".to_string(),
        updated_at: "2026-06-09T10:00:00Z".to_string(),
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        work_ref: Some("bad-ref".to_string()),
        work_profile: Some("essay".to_string()),
        total_planned_chapters: None,
        current_chapter: 0,
        auto_chain_enabled: false,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };
    nexus_local_db::works::create_work(&daemon.pool, &bad_work)
        .await
        .expect("create bad gated work");

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": "novel-brainstorm",
            "label": "p2-t1-gated-bad",
            "input": {
                "work_id": "wrk_gated_bad",
                "work_ref": "bad-ref",
                "topic": "p2-t1-gated-bad",
                "open_findings": "[]"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::UNPROCESSABLE_ENTITY,
        "gated add with failing Work must refuse (422): {body}"
    );
    let text = body.to_string();
    assert!(
        text.contains("preset_gates_failed"),
        "refusal must name the gate failure: {text}"
    );
    let bad_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM creator_schedules WHERE label = 'p2-t1-gated-bad'",
    )
    .fetch_one(&daemon.pool)
    .await
    .expect("bad schedule count");
    assert_eq!(
        bad_count, 0,
        "no schedule row may be created for a gate failure"
    );
}

/// N-14: work-linked vs general insertion — a work-linked add freezes the
/// `work_id` into the schedule row and descriptor; a general add (no
/// `work_id`) stays work-less. Both are driven through the production
/// coordinator.
#[tokio::test]
async fn admission_work_linked_vs_general_insertion() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Work-linked: memory-augmented with input.work_id.
    let work = nexus_local_db::works::WorkRecord {
        work_id: "wrk_linked".to_string(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Linked Work".to_string(),
        long_term_goal: "goal".to_string(),
        initial_idea: "idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-09T10:00:00Z".to_string(),
        updated_at: "2026-06-09T10:00:00Z".to_string(),
        current_stage: "research".to_string(),
        stage_status: "active".to_string(),
        work_ref: Some("linked-ref".to_string()),
        work_profile: Some("novel".to_string()),
        total_planned_chapters: Some(3),
        current_chapter: 0,
        auto_chain_enabled: false,
        driver_schedule_id: None,
        auto_chain_interrupted: false,
        auto_review_master_on_timeout: false,
        runtime_lock_holder: None,
        runtime_lock_acquired_at: None,
        completion_locked_at: None,
        novel_completion_status: None,
        lineage_from_work_id: None,
    };
    nexus_local_db::works::create_work(&daemon.pool, &work)
        .await
        .expect("create linked work");

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-work-linked",
            "seed": "p2-t1-linked-topic",
            "input": {
                "keyword": "p2-t1",
                "topic": "p2-t1-linked-topic",
                "work_id": "wrk_linked"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let linked_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    let linked_work: Option<String> =
        sqlx::query_scalar("SELECT work_id FROM creator_schedules WHERE schedule_id = ?")
            .bind(&linked_id)
            .fetch_one(&daemon.pool)
            .await
            .expect("linked work_id");
    assert_eq!(
        linked_work.as_deref(),
        Some("wrk_linked"),
        "work-linked add must freeze work_id into the schedule row"
    );
    let linked_desc: Option<Vec<u8>> = sqlx::query_scalar(
        "SELECT execution_descriptor_json FROM creator_schedules WHERE schedule_id = ?",
    )
    .bind(&linked_id)
    .fetch_one(&daemon.pool)
    .await
    .expect("linked descriptor");
    let linked_desc: nexus_orchestration::run_state::RunDescriptorV1 =
        serde_json::from_slice(linked_desc.as_deref().expect("linked descriptor present"))
            .expect("linked descriptor parses");
    assert_eq!(
        linked_desc.work_id.as_deref(),
        Some("wrk_linked"),
        "work-linked descriptor must carry the frozen work_id"
    );

    // General: no work_id — distinct creator so the serial per-creator
    // gate does not block it behind the still-running linked row.
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "general_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t1-general",
            "seed": "p2-t1-general-topic",
            "input": {
                "keyword": "p2-t1",
                "topic": "p2-t1-general-topic"
            },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let general_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    let general_work: Option<String> =
        sqlx::query_scalar("SELECT work_id FROM creator_schedules WHERE schedule_id = ?")
            .bind(&general_id)
            .fetch_one(&daemon.pool)
            .await
            .expect("general work_id");
    assert!(
        general_work.is_none() || general_work.as_deref() == Some(""),
        "general add must stay work-less: {general_work:?}"
    );

    // Both driven with real Host progress: wait until BOTH prompts are
    // recorded (the linked run is admitted first, the general run second).
    let prompts = if let Ok(prompts) =
        tokio::time::timeout(std::time::Duration::from_secs(15), async {
            loop {
                let prompts = host.prompts();
                if prompts.iter().any(|p| p.contains("p2-t1-linked-topic"))
                    && prompts.iter().any(|p| p.contains("p2-t1-general-topic"))
                {
                    return prompts;
                }
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        })
        .await
    {
        prompts
    } else {
        let prompts = host.prompts();
        let mut diag = String::new();
        for id in [&linked_id, &general_id] {
            let row = load_drive_row(&daemon, id).await;
            let sid = row.current_session_id.clone().unwrap_or_default();
            let (status, state) = load_run_record(&daemon, &sid).await;
            let ctx: Option<Vec<u8>> = sqlx::query_scalar(
                "SELECT context_json FROM orchestration_sessions WHERE session_id = ?",
            )
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("load context");
            let ctx_text = ctx
                .map(|b| String::from_utf8_lossy(&b).to_string())
                .unwrap_or_default();
            let run_error = serde_json::from_str::<Value>(&ctx_text)
                .ok()
                .and_then(|v| v.get("data").cloned())
                .and_then(|d| d.get("_run_error").cloned())
                .unwrap_or(Value::Null);
            diag.push_str(&format!(
                "schedule={id} schedule_status={} schedule_session={:?} \
                 session={sid} status={status} state={state:?} run_error={run_error:?}; ",
                row.status, row.current_session_id,
            ));
        }
        panic!("work-linked/general prompts not both recorded; prompts={prompts:?} {diag}");
    };
    assert!(
        prompts.iter().any(|p| p.contains("p2-t1-linked-topic")),
        "work-linked run prompt must carry the frozen input: {prompts:?}"
    );
    assert!(
        prompts.iter().any(|p| p.contains("p2-t1-general-topic")),
        "general run prompt must carry the frozen input: {prompts:?}"
    );
    for id in [&linked_id, &general_id] {
        let row = load_drive_row(&daemon, id).await;
        assert_eq!(row.execution_policy, "driven_v1");
        assert!(
            row.current_session_id
                .as_deref()
                .is_some_and(|s| !s.is_empty()),
            "schedule {id} must own a session"
        );
    }
}

/// N-14 (end-to-end): a failed drive over the REAL daemon settles its
/// durable failure record and is never re-driven — the public admission path
/// mints exactly one session and every later re-entry is refused.
///
/// The storage-fault half of this contract (a commit/cleanup write that
/// FAILS and fences the owner) is proven at the real store boundary in
/// `nexus-daemon-runtime`'s `preset_run` fixtures with a delegating
/// real-store wrapper; here the store is healthy, so the refusal comes from
/// the durable terminal state instead.
#[tokio::test]
async fn admission_failed_driver_transition_refuses_reentry() {
    let daemon = LiveDaemon::start().await;
    let now = chrono::Utc::now().timestamp();

    // This fixture omits the required world_id task-template input, so
    // `combat-engine` fails before invoking its first capability. The
    // healthy store commits the witnessed failure and terminal settlement;
    // durable state then refuses re-entry.
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy,
            execution_descriptor_json)
           VALUES ('SCHDRVFAIL', 'test_creator', 'combat-engine', 1, 'pending',
                   'serial', 0, 'p2-t1-driver-fail', ?, ?, NULL, 'driven_v1', ?)",
    )
    .bind(now)
    .bind(now)
    .bind(seeded_combat_descriptor_json())
    .execute(&daemon.pool)
    .await
    .expect("seed driver-fail row");
    seed_core_context_record(&daemon, "SCHDRVFAIL").await;

    let starter = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired")
        .schedule_starter_clone()
        .expect("production starter injected");
    let coordinator = daemon.state.run_coordinator().expect("coordinator wired");

    // First admission succeeds; strict task-template rendering fails inside
    // the drive. The healthy store records the authoritative `RunFailure`
    // and settles the run as Failed.
    let first = starter.start("SCHDRVFAIL").await;
    assert!(
        first.is_ok(),
        "first admission must succeed (the failure happens inside the drive)"
    );
    let sid = first.expect("session id").0;

    // Failed execution stays Failed after cleanup; it is not cancellation.
    let (status, state) = wait_for_run_status(&daemon, &sid, "failed", 15).await;
    assert_eq!(status, "failed");
    assert!(
        state
            .get("cancel_requested")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "durable cancel/failure record must be present: {state}"
    );
    assert_eq!(
        state
            .get("failure")
            .and_then(|f| f.get("code"))
            .and_then(Value::as_str),
        Some("driver_failed"),
        "the authoritative RunFailure must be durable: {state}"
    );
    assert!(
        !coordinator
            .is_fenced(&nexus_orchestration::SessionId(sid.clone()))
            .await,
        "a healthy-store failure settles terminal; the owner fence is only for a FAILED write"
    );

    // Re-entry: the durable terminal gate refuses a fresh drive —
    // `ensure_driving` returns `NotDriving`.
    let disposition = coordinator
        .ensure_driving(&nexus_orchestration::SessionId(sid.clone()))
        .await
        .expect("ensure_driving must not error");
    assert_eq!(
        disposition,
        nexus_daemon_runtime::preset_run::DriveDisposition::NotDriving,
        "a terminal failed run must refuse re-entry"
    );

    // Admission refuses the terminal failed run without minting another
    // session. Assert the refusal and durable effects, not error wording.
    starter
        .start("SCHDRVFAIL")
        .await
        .expect_err("terminal failed run must refuse re-entry");

    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?")
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("count sessions");
    assert_eq!(count, 1, "exactly one session for the failed run");
    let row = load_drive_row(&daemon, "SCHDRVFAIL").await;
    assert_eq!(
        row.current_session_id.as_deref(),
        Some(sid.as_str()),
        "schedule must keep its owned session identity"
    );
    // The matching schedule retains the same failed terminal cause.
    assert_eq!(
        row.status, "failed",
        "a terminal run settles its schedule to the matching terminal status"
    );
}

// ---------------------------------------------------------------------------
// Task 2 — control acceptance: wait continue, cancellation, nested waits
// ---------------------------------------------------------------------------

/// Deterministic blocking Host fixture (T2): the first prompt BLOCKS until
/// `release()` is called (or the operation is cancelled), then emits a
/// transformed delta + `EndTurn`. Every prompt increments the effect counter.
/// `cancel` records the cancelled op id AND releases any blocked stream so
/// the prompt executor's drain loop can observe cancellation. `shutdown_session`
/// increments the reap counter. This is the deterministic production-daemon
/// blocking ACP fixture the T2 acceptance requires.
struct BlockingHost {
    prompts: Mutex<Vec<String>>,
    effects: AtomicU64,
    cancels: AtomicU64,
    reaps: AtomicU64,
    block_first: AtomicU64,
    /// Incremented when a blocked stream's first event is actually being
    /// awaited (the prompt executor's drain loop is polling) — the
    /// deterministic signal that a cancel will reach the Host operation.
    streams_started: std::sync::Arc<AtomicU64>,
    release: tokio::sync::watch::Sender<bool>,
    /// Kept alive so `send` never fails (the channel stays open even when
    /// no stream is currently subscribed).
    _keep_alive: tokio::sync::watch::Receiver<bool>,
    /// Non-cooperative mode: `cancel` never completes (bounded by the
    /// executor's `shutdown_ms` timeout) and never releases the blocked
    /// stream, and `shutdown_session` fails — the A5 unconfirmed-cleanup
    /// shape that must persist `interrupted`, never `cancelled`.
    cancel_blocks: bool,
    shutdown_fails: bool,
}

impl BlockingHost {
    fn new() -> Arc<Self> {
        Self::build(false, false)
    }

    /// A5 unconfirmed-cleanup fixture: `cancel` never completes and
    /// `shutdown_session` fails, so the engine cannot confirm the stop.
    fn non_cooperative() -> Arc<Self> {
        Self::build(true, true)
    }

    fn build(cancel_blocks: bool, shutdown_fails: bool) -> Arc<Self> {
        let (release, keep_alive) = tokio::sync::watch::channel(false);
        Arc::new(Self {
            prompts: Mutex::new(Vec::new()),
            effects: AtomicU64::new(0),
            cancels: AtomicU64::new(0),
            reaps: AtomicU64::new(0),
            block_first: AtomicU64::new(1),
            streams_started: std::sync::Arc::new(AtomicU64::new(0)),
            release,
            _keep_alive: keep_alive,
            cancel_blocks,
            shutdown_fails,
        })
    }

    fn release(&self) {
        let _ = self.release.send(true);
    }

    fn effects(&self) -> u64 {
        self.effects.load(Ordering::SeqCst)
    }

    fn streams_started(&self) -> u64 {
        self.streams_started.load(Ordering::SeqCst)
    }

    fn cancels(&self) -> u64 {
        self.cancels.load(Ordering::SeqCst)
    }

    fn reaps(&self) -> u64 {
        self.reaps.load(Ordering::SeqCst)
    }
}

#[async_trait::async_trait]
impl HostFacade for BlockingHost {
    async fn start(&self, _config: HostStartConfig) -> HostResult<()> {
        Ok(())
    }

    async fn create_session(
        &self,
        request: nexus_agent_host::capability::CreateSessionRequest,
    ) -> HostResult<HostSession> {
        Ok(HostSession {
            id: HostSessionId::new(),
            provider_id: request.provider_id,
            state: SessionState::Ready,
            created_at: chrono::Utc::now(),
            active_op_id: None,
            negotiated_capabilities: CapabilityDescriptor::native_cli_limited(),
            owner: request.owner,
            process_identity: None,
        })
    }

    async fn exec(
        &self,
        session_id: HostSessionId,
        op: HostOperation,
    ) -> HostResult<HostEventStream> {
        self.effects.fetch_add(1, Ordering::SeqCst);
        let op_id = match op {
            HostOperation::Prompt { op_id, content, .. } => {
                let text = match content.as_slice() {
                    [HostContentBlock::Text { text }] => text.clone(),
                    other => format!("unexpected content {other:?}"),
                };
                self.prompts.lock().expect("prompts").push(text);
                op_id
            }
            other => {
                return Err(HostError::internal(format!(
                    "unexpected operation {other:?}"
                )));
            }
        };

        // Block the FIRST prompt's first event until released or cancelled.
        // The stream is returned immediately so the prompt executor's drain
        // loop can observe the coordinator token concurrently.
        let block = self.block_first.swap(0, Ordering::SeqCst) == 1;
        let release_rx = self.release.subscribe();

        let started = HostEvent::OpStarted(OperationStartedEvent {
            op_id: op_id.clone(),
            session_id: session_id.clone(),
        });
        let delta = HostEvent::MessageDelta(TextDeltaEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            text: "transformed:blocking-output".to_string(),
        });
        let finished = HostEvent::OpFinished(OperationFinishedEvent {
            session_id,
            op_id,
            reason: FinishReason::EndTurn,
        });
        let streams_started = std::sync::Arc::clone(&self.streams_started);

        Ok(Box::pin(futures_util::stream::unfold(
            (block, release_rx, started, delta, finished, 0u8),
            move |(block, mut release_rx, started, delta, finished, state)| {
                let streams_started = std::sync::Arc::clone(&streams_started);
                async move {
                    if state == 0 {
                        if block {
                            // Signal that the drain loop is now polling this
                            // stream (deterministic cancel-reachability),
                            // then wait until released (or cancelled via the
                            // same watch flag — `cancel` sends `true`).
                            streams_started.fetch_add(1, Ordering::SeqCst);
                            while !*release_rx.borrow() {
                                if release_rx.changed().await.is_err() {
                                    break;
                                }
                            }
                        }
                        Some((
                            Ok(started.clone()),
                            (false, release_rx, started, delta, finished, 1),
                        ))
                    } else if state == 1 {
                        Some((
                            Ok(delta.clone()),
                            (false, release_rx, started, delta, finished, 2),
                        ))
                    } else if state == 2 {
                        Some((
                            Ok(finished.clone()),
                            (false, release_rx, started, delta, finished, 3),
                        ))
                    } else {
                        None
                    }
                }
            },
        )))
    }

    async fn cancel(&self, _op_id: HostOperationId) -> HostResult<()> {
        self.cancels.fetch_add(1, Ordering::SeqCst);
        if self.cancel_blocks {
            // Non-cooperative provider: the cancel request never completes.
            // The executor bounds this with `shutdown_ms` and proceeds to
            // unconfirmed cleanup (A5).
            std::future::pending::<()>().await;
        }
        // Release any blocked stream so the drain loop can observe
        // cancellation and terminate the operation.
        let _ = self.release.send(true);
        Ok(())
    }

    async fn health(&self) -> HostResult<HostHealth> {
        Ok(HostHealth {
            running: true,
            active_sessions: 0,
            active_operations: 0,
        })
    }

    async fn shutdown(&self) -> HostResult<()> {
        Ok(())
    }

    async fn shutdown_session(&self, _session_id: HostSessionId) -> HostResult<()> {
        self.reaps.fetch_add(1, Ordering::SeqCst);
        if self.shutdown_fails {
            return Err(HostError::internal(
                "non-cooperative provider: shutdown_session failed",
            ));
        }
        Ok(())
    }

    async fn list_sessions(&self) -> HostResult<Vec<HostSession>> {
        Ok(Vec::new())
    }

    async fn provider_catalog(&self) -> HostResult<ProviderCatalog> {
        Ok(ProviderCatalog {
            entries: vec![ProviderCatalogEntry {
                provider_id: nexus_agent_host::ProviderId::new(MOCK_PROVIDER),
                display_name: MOCK_PROVIDER.to_string(),
                protocol_kind: ProtocolKind::NativeCli,
                launch: LaunchStrategy::NativeCli {
                    command: "mock".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                },
                source: DiscoverySource::Config,
                trust: TrustLevel::Explicit,
                capabilities: CapabilityDescriptor::native_cli_limited(),
                health: ProviderHealth {
                    provider_id: nexus_agent_host::ProviderId::new(MOCK_PROVIDER),
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
        let (events, _) = tokio::sync::broadcast::channel(64);
        events.subscribe()
    }
}

/// T2: authorized continue with the exact durable wait token resumes the
/// run — the effect counter advances past the wait and the run completes.
/// A stale/duplicate continue refuses with the A4 409 envelope and never
/// starts a second driver.
#[tokio::test]
async fn control_authorized_continue_resumes_and_stale_refuses() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Admit a public schedule; the drive loop parks at the manual wait
    // after the generate step (the blocking Host releases the first prompt).
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t2-continue",
            "seed": "p2-t2-topic",
            "input": { "keyword": "p2-t2", "topic": "p2-t2-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    // Release the first prompt so the run reaches the manual wait.
    host.release();
    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("durable wait id")
        .to_string();
    assert!(!wait_id.is_empty(), "wait id must be a fresh UUID");
    let effects_before = host.effects();

    // Stale/duplicate continue: wrong token → 409 workflow_wait_conflict
    // with the exact envelope; no mutation, no second driver.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": "stale-token" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("workflow_wait_conflict"),
        "{body}"
    );
    assert_eq!(
        body["error"]["details"]["session_id"].as_str(),
        Some(sid.as_str()),
        "A4 envelope must carry the session id: {body}"
    );
    assert_eq!(
        body["error"]["details"]["status"].as_str(),
        Some("waiting_for_input"),
        "A4 envelope must carry the current status: {body}"
    );
    assert_eq!(
        body["error"]["details"]["current_wait_id"].as_str(),
        Some(wait_id.as_str()),
        "A4 envelope must carry the current wait id: {body}"
    );
    // No mutation: still waiting, same wait id, no new effects.
    let (run_status, state) = load_run_record(&daemon, &sid).await;
    assert_eq!(run_status, "waiting_for_input");
    assert_eq!(
        state["wait"]["wait_id"].as_str(),
        Some(wait_id.as_str()),
        "stale continue must not consume the wait"
    );
    assert_eq!(
        host.effects(),
        effects_before,
        "no new effects after stale continue"
    );

    // Missing wait_id → 422 invalid_input.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("invalid_input"),
        "{body}"
    );
    assert_eq!(
        body["error"]["details"]["field"].as_str(),
        Some("wait_id"),
        "{body}"
    );

    // Authorized continue with the exact token: the run resumes and
    // completes. (The Host exec happens at the generate step, before the
    // wait; after continue the run only steps to the terminal `done` state,
    // so the effect counter is unchanged — completion is the proof.)
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": wait_id }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(body["status"].as_str(), Some("running"), "{body}");

    // The run completes (persist → done).
    let (run_status, _) = wait_for_run_status(&daemon, &sid, "completed", 15).await;
    assert_eq!(run_status, "completed");
    assert_eq!(
        host.effects(),
        effects_before,
        "continue must not spawn a second Host exec (single-flight)"
    );

    // Duplicate continue after completion → 409 workflow_state_conflict
    // (the run is terminal; no second driver).
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": "any-token" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("workflow_state_conflict"),
        "{body}"
    );
}

/// T2: cancel reaches the active Host/ACP work — the coordinator token
/// fires, the Host operation is cancelled, the owned session is reaped, and
/// the run persists `cancelled` (never a false success). Later effects are
/// suppressed by the fence.
#[tokio::test]
async fn control_cancel_reaches_host_and_persists_cancelled() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Admit a public schedule; the first prompt BLOCKS in the Host.
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t2-cancel",
            "seed": "p2-t2-topic",
            "input": { "keyword": "p2-t2", "topic": "p2-t2-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    // Wait for the prompt to be in flight AND its stream to be polled by
    // the executor's drain loop (deterministic cancel-reachability: a
    // cancel issued now is observed by the drain loop and reaches the Host
    // operation).
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if host.streams_started() >= 1 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("prompt stream polled");

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();
    let effects_before = host.effects();

    // Cancel the schedule: the coordinator routes to the engine's A5
    // cancel path — durable cancel-intent fence, token fire, bounded
    // owned-Host teardown, then terminal Cancelled.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "cancel" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(body["status"].as_str(), Some("cancelled"), "{body}");

    // The run persists cancelled (terminal, never auto-driven).
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "cancelled", 15).await;
    assert_eq!(run_status, "cancelled");
    assert_eq!(
        state["cancel_requested"].as_bool(),
        Some(true),
        "cancelled run must carry cancel_requested"
    );

    // The Host observed the cancellation: the operation was cancelled and
    // the owned session was reaped.
    assert!(
        host.cancels() >= 1,
        "cancel must reach the Host operation (cancels={})",
        host.cancels()
    );
    assert!(
        host.reaps() >= 1,
        "cancel must reap the owned Host session (reaps={})",
        host.reaps()
    );

    // No later effects: the fence suppressed the persist step.
    assert_eq!(
        host.effects(),
        effects_before,
        "no downstream effects after the cancel fence"
    );

    // Repeat cancel is idempotent: the schedule is already cancelled.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "cancel" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
}

/// qc2 F-002: an unconfirmed cancel/cleanup persists `interrupted` on the
/// run and MUST NOT be projected as a successful schedule `cancelled`.
/// `creator_schedules.status` has no `interrupted` variant (and an unmapped
/// string projects as `Pending`, i.e. re-runnable), so the row stays
/// non-terminal and the response/inspect report the interrupted run.
#[tokio::test]
async fn control_unconfirmed_cancel_keeps_schedule_non_cancelled() {
    let host = BlockingHost::non_cooperative();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-fix-unconfirmed-cancel",
            "seed": "p2-fix-topic",
            "input": { "keyword": "p2-fix", "topic": "p2-fix-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    // Wait for the prompt to be in flight AND its stream to be polled by
    // the executor's drain loop, then release it and let the run park at
    // the durable manual wait.
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if host.streams_started() >= 1 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("prompt stream polled");
    host.release();

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();

    // Deterministic cancel point: poll the DURABLE run record until the run
    // is parked at the manual wait — provably non-terminal and signalable,
    // with the drive loop exited. Cancelling the in-flight prompt instead is
    // a structural race: the cancel token fails the blocked prompt step, the
    // drive loop's failure boundary re-enters the engine cancel path, and
    // whichever of the two racing `interrupted` commits loses the revision
    // fence surfaces a spurious 409 "terminal or not in a signalable state".
    let (parked_status, _parked_state) =
        wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(parked_status, "waiting_for_input");

    // Cancel reaches a non-cooperative provider: the parked run's owned Host
    // session cannot be reaped (`shutdown_session` fails), so the stop
    // cannot be confirmed.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "cancel" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(
        body["status"].as_str(),
        Some("interrupted"),
        "unconfirmed cancel must report interrupted, never cancelled: {body}"
    );

    // The durable run is interrupted (uncertain), not a successful cancel.
    let (run_status, _state) = wait_for_run_status(&daemon, &sid, "interrupted", 15).await;
    assert_eq!(run_status, "interrupted");

    // The schedule row is NOT cancelled and NOT pending (an unmapped
    // `interrupted` string would project as Pending → re-runnable).
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_ne!(
        row.status, "cancelled",
        "unconfirmed cancel must not fabricate a successful schedule cancel"
    );
    assert_ne!(
        row.status, "pending",
        "an unmapped status string must never make the schedule re-runnable"
    );
    assert_eq!(
        row.current_session_id.as_deref(),
        Some(sid.as_str()),
        "the schedule keeps its owned (interrupted) run identity"
    );
}

/// Greptile follow-up (A5): cancelling a run whose prompt is IN FLIGHT
/// against a non-cooperative host must report the unconfirmed outcome —
/// `interrupted` with the durable cancel intent, never a false `cancelled`,
/// and never a spurious 409. The cancel token fails the blocked prompt
/// step, so the drive loop's failure boundary re-enters the engine cancel
/// path concurrently with this signal's own A5 teardown; whichever
/// `interrupted` commit loses the revision fence must resolve to the
/// persisted outcome (`cancel_fence_loss_accomplished`), not a conflict.
#[tokio::test]
async fn control_in_flight_unconfirmed_cancel_reports_interrupted() {
    let host = BlockingHost::non_cooperative();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-fix-in-flight-unconfirmed-cancel",
            "seed": "p2-fix-in-flight-topic",
            "input": { "keyword": "p2-fix-in-flight", "topic": "p2-fix-in-flight-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    // Deterministic IN-FLIGHT cancel point: the first prompt's stream is
    // being polled by the executor's drain loop, so the prompt is provably
    // in flight and blocked. Do NOT release it — the cancel must interrupt
    // active Host work, not a parked wait.
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if host.streams_started() >= 1 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("prompt stream polled");

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();

    // Cancel reaches the in-flight prompt of a non-cooperative provider:
    // `cancel` never completes (bounded by the executor's shutdown timeout)
    // and `shutdown_session` fails, so the stop cannot be confirmed.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "cancel" }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::OK,
        "an in-flight unconfirmed cancel must never surface a spurious conflict: {body}"
    );
    assert_eq!(
        body["status"].as_str(),
        Some("interrupted"),
        "unconfirmed cancel must report interrupted, never cancelled: {body}"
    );

    // The cancel reached the active Host operation.
    assert!(
        host.cancels() >= 1,
        "cancel must reach the in-flight Host operation (cancels={})",
        host.cancels()
    );

    // The durable run is interrupted (uncertain) WITH the cancel intent —
    // never a false successful cancel.
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "interrupted", 15).await;
    assert_eq!(run_status, "interrupted");
    assert_eq!(
        state["cancel_requested"].as_bool(),
        Some(true),
        "interrupted run must carry the durable cancel intent: {state}"
    );
    assert_eq!(
        state["failure"]["code"].as_str(),
        Some("cancel_cleanup_unconfirmed"),
        "the actionable unconfirmed-cleanup reason must be durable: {state}"
    );

    // The schedule row is NOT cancelled and NOT pending (an unmapped
    // `interrupted` string would project as Pending → re-runnable).
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_ne!(
        row.status, "cancelled",
        "unconfirmed cancel must not fabricate a successful schedule cancel"
    );
    assert_ne!(
        row.status, "pending",
        "an unmapped status string must never make the schedule re-runnable"
    );
    assert_eq!(
        row.current_session_id.as_deref(),
        Some(sid.as_str()),
        "the schedule keeps its owned (interrupted) run identity"
    );
}

/// qc1 F-001: an explicit legacy start with no configured default binding
/// provider MUST refuse — the Host catalog's first entry is never selected
/// as a fallback — and MUST NOT create a session.
#[tokio::test]
async fn admission_legacy_start_without_configured_provider_refuses() {
    // `start_with_agent_host` leaves `providers` empty (no configured
    // default binding provider) while the MockHost catalog still has one
    // entry — exactly the shape that used to fall back to the first entry.
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let now = chrono::Utc::now().timestamp();

    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy,
            execution_descriptor_json)
           VALUES ('SCHNOBIND', 'test_creator', ?, 1, 'pending', 'parallel_any', 0, ?,
                   ?, ?, NULL, 'legacy_inert', NULL)",
    )
    .bind(PUBLIC_PRESET)
    .bind("p2-fix-no-binding")
    .bind(now)
    .bind(now)
    .execute(&daemon.pool)
    .await
    .expect("seed legacy row");

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules/SCHNOBIND/signal",
        json!({ "signal": "start" }),
    )
    .await;
    assert_eq!(
        status,
        reqwest::StatusCode::CONFLICT,
        "legacy start without a configured provider must refuse: {body}"
    );
    let text = body.to_string();
    assert!(
        text.contains("binding provider"),
        "refusal must name the missing binding provider: {text}"
    );

    let sessions: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions")
        .fetch_one(&daemon.pool)
        .await
        .expect("count sessions");
    assert_eq!(sessions, 0, "a refused legacy start must create no session");
    let row = load_drive_row(&daemon, "SCHNOBIND").await;
    assert_eq!(row.status, "pending", "the row must stay unstarted");
    assert!(
        row.current_session_id.is_none(),
        "no run identity may be minted for a refused legacy start"
    );
}

/// T2: completed-v-cancel race — a run that already completed stays
/// completed; cancel cannot erase committed work.
#[tokio::test]
async fn control_completed_vs_cancel_race_completed_wins() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Admit a public schedule; release the first prompt so the run parks
    // at the manual wait.
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t2-race",
            "seed": "p2-t2-topic",
            "input": { "keyword": "p2-t2", "topic": "p2-t2-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    host.release();

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("durable wait id")
        .to_string();

    // Continue to completion.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": wait_id }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let (run_status, _) = wait_for_run_status(&daemon, &sid, "completed", 15).await;
    assert_eq!(run_status, "completed");

    // Cancel after completion: the run stays completed (revision-fenced).
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "cancel" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
    let (run_status, _) = load_run_record(&daemon, &sid).await;
    assert_eq!(run_status, "completed", "completed remains completed");
}

/// T2: restart/hydration leaves a `HumanWait` blocked — the durable wait
/// token survives and the run is never auto-advanced at boot.
#[tokio::test]
async fn control_restart_wait_stays_blocked() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Admit a public schedule; release the first prompt so the run parks
    // at the manual wait.
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t2-restart",
            "seed": "p2-t2-topic",
            "input": { "keyword": "p2-t2", "topic": "p2-t2-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    host.release();

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("durable wait id")
        .to_string();

    // Simulate restart: the durable record is still waiting_for_input with
    // the same token. The recovery classifier must NOT auto-advance it.
    let class = nexus_orchestration::resume_rules::classify_recovery(
        &nexus_orchestration::engine::SessionStatus::WaitingForInput,
        Some(&nexus_orchestration::run_state::RunStateV1 {
            wait: Some(nexus_orchestration::run_state::WaitRecord {
                wait_id: wait_id.clone(),
                task_id: "persist".to_string(),
                child_session_id: None,
                child_task_id: None,
                kind: nexus_orchestration::run_state::WaitKind::Manual,
            }),
            ..nexus_orchestration::run_state::RunStateV1::default()
        }),
        false,
    );
    assert_eq!(
        class,
        nexus_orchestration::resume_rules::RecoveryClass::HumanWait,
        "a durable human wait must classify HumanWait (never auto-advanced)"
    );

    // The wait token is retained through restart: a matching continue still
    // works after the "restart" (the durable record is unchanged).
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": wait_id }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let (run_status, _) = wait_for_run_status(&daemon, &sid, "completed", 15).await;
    assert_eq!(run_status, "completed");
}

/// T2: advance/resume/force-transition cannot bypass a human wait — the
/// engine's non-continue signals are fenced against waiting runs.
#[tokio::test]
async fn control_advance_cannot_bypass_human_wait() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    // Admit a public schedule; release the first prompt so the run parks
    // at the manual wait.
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t2-advance",
            "seed": "p2-t2-topic",
            "input": { "keyword": "p2-t2", "topic": "p2-t2-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    host.release();

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("durable wait id")
        .to_string();
    let effects_before = host.effects();

    // Advance on a waiting run must refuse (409 workflow_state_conflict) —
    // it cannot bypass the human wait.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "advance" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");
    assert_eq!(
        body["error"]["code"].as_str(),
        Some("workflow_state_conflict"),
        "{body}"
    );

    // Resume on a waiting run must also refuse (the schedule is running,
    // so the supervisor refuses the resume transition).
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "resume" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CONFLICT, "{body}");

    // The wait is untouched and no effects ran.
    let (run_status, state) = load_run_record(&daemon, &sid).await;
    assert_eq!(run_status, "waiting_for_input");
    assert_eq!(
        state["wait"]["wait_id"].as_str(),
        Some(wait_id.as_str()),
        "bypass attempts must not consume the wait"
    );
    assert_eq!(
        host.effects(),
        effects_before,
        "no effects after bypass attempts"
    );
}

// ---------------------------------------------------------------------------
// Task 3 — settlement acceptance: terminal agreement, idempotent auto-chain,
// boot reconciliation, no re-enqueue
// ---------------------------------------------------------------------------

/// T3: a completed driven run settles the matching schedule from the durable
/// session status — schedule and session inspect agree (status, policy,
/// current run), and a later tick never re-enqueues the terminal row.
#[tokio::test]
async fn settlement_completed_inspect_agrees() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t3-completed",
            "seed": "p2-t3-topic",
            "input": { "keyword": "p2-t3", "topic": "p2-t3-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    host.release();

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("wait id")
        .to_string();

    // Shared projection: the durable human wait is actionable and named.
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/v1/daemon/orchestration/schedules/{schedule_id}",
            daemon.http_url
        ))
        .send()
        .await
        .expect("GET inspect (waiting)");
    let inspect: Value = resp.json().await.expect("inspect json");
    assert_eq!(
        inspect["schedule"]["execution"]["recovery_class"].as_str(),
        Some("human_wait"),
        "a parked run must project human_wait: {inspect}"
    );
    assert_eq!(
        inspect["schedule"]["execution"]["wait"]["wait_id"].as_str(),
        Some(wait_id.as_str())
    );
    let actions = inspect["schedule"]["execution"]["allowed_actions"]
        .as_array()
        .expect("allowed_actions array");
    assert!(actions.iter().any(|a| a.as_str() == Some("continue")));
    assert!(actions.iter().any(|a| a.as_str() == Some("cancel")));

    // Continue to completion.
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": wait_id }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let (run_status, _) = wait_for_run_status(&daemon, &sid, "completed", 15).await;
    assert_eq!(run_status, "completed");

    // The schedule settles to completed from the durable session status.
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_eq!(row.status, "completed", "schedule must settle to completed");
    assert_eq!(
        row.current_session_id.as_deref(),
        Some(sid.as_str()),
        "schedule keeps its owned run identity"
    );

    // Public inspect agrees: status, policy, current run.
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/v1/daemon/orchestration/schedules/{schedule_id}",
            daemon.http_url
        ))
        .send()
        .await
        .expect("GET inspect");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let inspect: Value = resp.json().await.expect("inspect json");
    assert_eq!(inspect["schedule"]["status"].as_str(), Some("completed"));
    assert_eq!(
        inspect["schedule"]["execution_policy"].as_str(),
        Some("driven_v1")
    );
    assert_eq!(
        inspect["schedule"]["current_session_id"].as_str(),
        Some(sid.as_str())
    );
    // Shared projection: a terminal run is `terminal` and offers only a new run.
    assert_eq!(
        inspect["schedule"]["execution"]["recovery_class"].as_str(),
        Some("terminal"),
        "settled schedule must project terminal: {inspect}"
    );
    assert_eq!(
        inspect["schedule"]["execution"]["allowed_actions"]
            .as_array()
            .map(|a| a.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()),
        Some(vec!["new_run"]),
        "terminal rows offer only new_run: {inspect}"
    );

    // Session inspect agrees.
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/v1/daemon/orchestration/sessions/{sid}",
            daemon.http_url
        ))
        .send()
        .await
        .expect("GET session");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let session: Value = resp.json().await.expect("session json");
    assert_eq!(session["session"]["status"].as_str(), Some("completed"));

    // Terminal rows are never re-enqueued: a tick leaves the row terminal
    // and does not mint a second session.
    let supervisor = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired");
    supervisor.tick().await.expect("tick succeeds");
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_eq!(
        row.status, "completed",
        "terminal row must not be re-enqueued"
    );
    let count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?")
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("count sessions");
    assert_eq!(count, 1, "no second session for a terminal row");
}

/// T3: a cancelled driven run settles the matching schedule to cancelled —
/// schedule and session inspect agree, and the terminal row is not
/// re-enqueued.
#[tokio::test]
async fn settlement_cancelled_inspect_agrees() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t3-cancelled",
            "seed": "p2-t3-topic",
            "input": { "keyword": "p2-t3", "topic": "p2-t3-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();

    // Wait for the prompt stream to be polled (deterministic cancel-reachability).
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            if host.streams_started() >= 1 {
                return;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("prompt stream polled");

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();

    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "cancel" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(body["status"].as_str(), Some("cancelled"), "{body}");

    let (run_status, _) = wait_for_run_status(&daemon, &sid, "cancelled", 15).await;
    assert_eq!(run_status, "cancelled");

    // The schedule settles to cancelled (durable session status is truth).
    // Settlement is async — the drive task settles after the cancel signal
    // returns — so poll the schedule row (the handler's raw UPDATE and the
    // drive task's settlement both converge on "cancelled").
    let row = tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let row = load_drive_row(&daemon, &schedule_id).await;
            if row.status == "cancelled" {
                return row;
            }
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("schedule must settle to cancelled");
    assert_eq!(row.status, "cancelled", "schedule must settle to cancelled");

    // Public inspect agrees.
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/v1/daemon/orchestration/schedules/{schedule_id}",
            daemon.http_url
        ))
        .send()
        .await
        .expect("GET inspect");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let inspect: Value = resp.json().await.expect("inspect json");
    assert_eq!(inspect["schedule"]["status"].as_str(), Some("cancelled"));
    assert_eq!(
        inspect["schedule"]["current_session_id"].as_str(),
        Some(sid.as_str())
    );

    // Terminal rows are never re-enqueued.
    let supervisor = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired");
    supervisor.tick().await.expect("tick succeeds");
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_eq!(
        row.status, "cancelled",
        "terminal row must not be re-enqueued"
    );
}

/// T3: a failed driven run settles the matching schedule from the durable
/// session status — schedule and session inspect agree (the durable record
/// is truth, whatever terminal status the engine committed).
#[tokio::test]
async fn settlement_failed_inspect_agrees() {
    let daemon = LiveDaemon::start().await;
    let now = chrono::Utc::now().timestamp();

    // Missing required world_id task-template input triggers a witnessed
    // engine failure before capability execution (the T1 fixture).
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy,
            execution_descriptor_json)
           VALUES ('SCHSETTLEFAIL', 'test_creator', 'combat-engine', 1, 'pending',
                   'serial', 0, 'p2-t3-failed', ?, ?, NULL, 'driven_v1', ?)",
    )
    .bind(now)
    .bind(now)
    .bind(seeded_combat_descriptor_json())
    .execute(&daemon.pool)
    .await
    .expect("seed failed row");
    seed_core_context_record(&daemon, "SCHSETTLEFAIL").await;

    let starter = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired")
        .schedule_starter_clone()
        .expect("production starter injected");
    let sid = starter
        .start("SCHSETTLEFAIL")
        .await
        .expect("admit failed run")
        .0;

    // Resource cleanup must preserve Failed in both the durable run and
    // schedule, so inspect cannot report a satisfied cancellation.
    let (run_status, _) = wait_for_run_status(&daemon, &sid, "failed", 15).await;
    assert_eq!(run_status, "failed");

    let row = load_drive_row(&daemon, "SCHSETTLEFAIL").await;
    assert_eq!(
        row.status, run_status,
        "schedule must settle to the durable session status"
    );
    assert_eq!(
        row.current_session_id.as_deref(),
        Some(sid.as_str()),
        "schedule keeps its owned run identity"
    );

    // Public inspect agrees.
    let resp = reqwest::Client::new()
        .get(format!(
            "{}/v1/daemon/orchestration/schedules/SCHSETTLEFAIL",
            daemon.http_url
        ))
        .send()
        .await
        .expect("GET inspect");
    assert_eq!(resp.status(), reqwest::StatusCode::OK);
    let inspect: Value = resp.json().await.expect("inspect json");
    assert_eq!(inspect["schedule"]["status"].as_str(), Some("failed"));
    assert_eq!(
        inspect["schedule"]["execution_policy"].as_str(),
        Some("driven_v1")
    );
    assert_eq!(
        inspect["schedule"]["current_session_id"].as_str(),
        Some(sid.as_str())
    );
}

/// T3: duplicate terminal callbacks never create a second auto-chain child —
/// the child INSERT is keyed by `source_run_id`; a duplicate loads the
/// already-created child.
#[tokio::test]
async fn settlement_duplicate_callback_no_second_child() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host_and_provider(host.clone()).await;

    // Work at the review stage (complete) — the next chain step is persist
    // (kb-extract, a preset with no prompt roles, so no binding provider is
    // needed for the child enqueue).
    let work = nexus_local_db::works::WorkRecord {
        work_id: "wrk_settle_dup".to_string(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Settle Dup Work".to_string(),
        long_term_goal: "goal".to_string(),
        initial_idea: "idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-09T10:00:00Z".to_string(),
        updated_at: "2026-06-09T10:00:00Z".to_string(),
        current_stage: "review".to_string(),
        stage_status: "complete".to_string(),
        work_ref: Some("settle-dup-ref".to_string()),
        work_profile: Some("novel".to_string()),
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
    };
    nexus_local_db::works::create_work(&daemon.pool, &work)
        .await
        .expect("create settle work");

    // Drive a public schedule to completion (the source run).
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t3-dup-callback",
            "seed": "p2-t3-topic",
            "input": { "keyword": "p2-t3", "topic": "p2-t3-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    host.release();

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("wait id")
        .to_string();
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": wait_id }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let (run_status, _) = wait_for_run_status(&daemon, &sid, "completed", 15).await;
    assert_eq!(run_status, "completed");

    // The schedule settled to completed (no child yet — the Work had no
    // driver pointer at settlement time).
    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_eq!(row.status, "completed");

    // Attach the driver pointer, then fire the terminal callback again.
    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::works::patch_work(
        &daemon.pool,
        "test_creator",
        "wrk_settle_dup",
        &nexus_local_db::works::WorkPatch {
            driver_schedule_id: Some(Some(schedule_id.clone())),
            ..Default::default()
        },
        &now,
    )
    .await
    .expect("attach driver pointer");

    let supervisor = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired");
    // First callback: creates the child keyed by source_run_id.
    supervisor
        .on_schedule_terminal(
            &schedule_id,
            nexus_contracts::local::schedule::ScheduleStatus::Completed,
        )
        .await
        .expect("first terminal callback");
    let child_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM creator_schedules WHERE source_run_id = ?")
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("child count");
    assert_eq!(
        child_count, 1,
        "first callback must create exactly one child"
    );

    // Duplicate callback: loads the existing child, never mints a second.
    supervisor
        .on_schedule_terminal(
            &schedule_id,
            nexus_contracts::local::schedule::ScheduleStatus::Completed,
        )
        .await
        .expect("duplicate terminal callback");
    let child_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM creator_schedules WHERE source_run_id = ?")
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("child count");
    assert_eq!(
        child_count, 1,
        "duplicate callback must not mint a second child"
    );
    let child_id: Option<String> =
        sqlx::query_scalar("SELECT schedule_id FROM creator_schedules WHERE source_run_id = ?")
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("child id");
    assert!(child_id.is_some(), "the existing child must be loaded");
}

/// T3: boot reconciliation settles a durable terminal session whose schedule
/// row is still `running` (checkpoint-before-settlement loss) — no second
/// session, no second auto-chain child.
#[tokio::test]
async fn settlement_restart_reconciles_no_second_child() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host_and_provider(host.clone()).await;

    // Work at the review stage (complete) — next chain step is persist
    // (kb-extract, no prompt roles).
    let work = nexus_local_db::works::WorkRecord {
        work_id: "wrk_settle_restart".to_string(),
        creator_id: "test_creator".to_string(),
        workspace_slug: "default".to_string(),
        status: "active".to_string(),
        title: "Settle Restart Work".to_string(),
        long_term_goal: "goal".to_string(),
        initial_idea: "idea".to_string(),
        creative_brief: None,
        intake_status: "complete".to_string(),
        world_id: Some("wld_test_world".to_string()),
        story_ref: None,
        inspiration_log: "[]".to_string(),
        primary_preset_id: "novel-writing".to_string(),
        schedule_ids: "[]".to_string(),
        created_at: "2026-06-09T10:00:00Z".to_string(),
        updated_at: "2026-06-09T10:00:00Z".to_string(),
        current_stage: "review".to_string(),
        stage_status: "complete".to_string(),
        work_ref: Some("settle-restart-ref".to_string()),
        work_profile: Some("novel".to_string()),
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
    };
    nexus_local_db::works::create_work(&daemon.pool, &work)
        .await
        .expect("create settle work");

    // Drive a public schedule to completion.
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p2-t3-restart",
            "seed": "p2-t3-topic",
            "input": { "keyword": "p2-t3", "topic": "p2-t3-topic" },
            "agent_bindings": {
                "default": { "provider_id": MOCK_PROVIDER }
            }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"]
        .as_str()
        .expect("schedule_id")
        .to_string();
    host.release();

    let row = load_drive_row(&daemon, &schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("owned session")
        .to_string();
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(run_status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("wait id")
        .to_string();
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": wait_id }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let (run_status, _) = wait_for_run_status(&daemon, &sid, "completed", 15).await;
    assert_eq!(run_status, "completed");

    // Attach the driver pointer and create the child via a terminal callback.
    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::works::patch_work(
        &daemon.pool,
        "test_creator",
        "wrk_settle_restart",
        &nexus_local_db::works::WorkPatch {
            driver_schedule_id: Some(Some(schedule_id.clone())),
            ..Default::default()
        },
        &now,
    )
    .await
    .expect("attach driver pointer");
    let supervisor = daemon
        .state
        .schedule_supervisor()
        .expect("supervisor wired");
    supervisor
        .on_schedule_terminal(
            &schedule_id,
            nexus_contracts::local::schedule::ScheduleStatus::Completed,
        )
        .await
        .expect("terminal callback");
    let child_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM creator_schedules WHERE source_run_id = ?")
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("child count");
    assert_eq!(
        child_count, 1,
        "exactly one child after the first settlement"
    );

    // Simulate checkpoint-before-settlement loss: the durable session is
    // terminal but the schedule row is `running` again (the settlement
    // transaction did not commit before the crash).
    let now_ts = chrono::Utc::now().timestamp();
    sqlx::query(
        "UPDATE creator_schedules SET status = 'running', terminated_at = NULL, updated_at = ?
         WHERE schedule_id = ?",
    )
    .bind(now_ts)
    .bind(&schedule_id)
    .execute(&daemon.pool)
    .await
    .expect("rewind schedule to running");

    // Boot reconciliation settles the schedule from the durable terminal
    // session — no second session, no second child. The child (kb-extract)
    // was admitted and driven by the first settlement; with no queued KB
    // jobs its own session is terminal too, so reconcile settles BOTH the
    // rewound source and the child — the source-specific assertions below
    // are the acceptance criteria.
    let reconciled = supervisor
        .reconcile_terminal_schedules()
        .await
        .expect("reconcile");
    assert!(
        reconciled >= 1,
        "at least the rewound source schedule must be reconciled"
    );

    let row = load_drive_row(&daemon, &schedule_id).await;
    assert_eq!(row.status, "completed", "schedule settled after reconcile");
    let child_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM creator_schedules WHERE source_run_id = ?")
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("child count");
    assert_eq!(child_count, 1, "reconcile must not mint a second child");
    let session_count: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?")
            .bind(&sid)
            .fetch_one(&daemon.pool)
            .await
            .expect("session count");
    assert_eq!(
        session_count, 1,
        "reconcile must not create a second session"
    );
}




#[derive(Debug, Clone)]
struct SseFrameParsed {
    id: String,
    event: String,
    data: Value,
    sequence: u64,
}

fn parse_sse_frames(body: &str) -> Vec<SseFrameParsed> {
    let mut out = Vec::new();
    for block in body.split("\n\n") {
        let mut id = None;
        let mut event = None;
        let mut data = None;
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("id: ") {
                id = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("event: ") {
                event = Some(rest.trim().to_string());
            } else if let Some(rest) = line.strip_prefix("data: ") {
                data = Some(rest);
            }
        }
        if let (Some(id), Some(event), Some(data)) = (id, event, data) {
            let json = serde_json::from_str::<Value>(data).unwrap_or(Value::Null);
            let sequence = json
                .get("sequence")
                .and_then(|v| v.as_u64())
                .or_else(|| {
                    id.split(':')
                        .nth(1)
                        .and_then(|s| s.parse().ok())
                })
                .unwrap_or(0);
            out.push(SseFrameParsed {
                id,
                event,
                data: json,
                sequence,
            });
        }
    }
    out
}

fn assert_monotonic_sse_frames(frames: &[SseFrameParsed]) {
    let mut last = 0u64;
    let mut seen = std::collections::HashSet::new();
    for frame in frames {
        assert!(frame.sequence > last, "sequence must increase: {} after {}", frame.sequence, last);
        assert!(seen.insert(frame.id.clone()), "duplicate id {}", frame.id);
        last = frame.sequence;
    }
}

fn host_event_kind(data: &Value) -> Option<String> {
    data.get("host_event")
        .and_then(|v| v.as_object())
        .and_then(|obj| obj.keys().next().map(|k| k.to_string()))
}

async fn fetch_session_events(daemon: &LiveDaemon, sid: &str, last_event_id: Option<&str>) -> (reqwest::StatusCode, String) {
    let client = reqwest::Client::new();
    let mut req = client.get(format!(
        "{}/v1/daemon/orchestration/sessions/{}/events",
        daemon.http_url,
        sid
    ));
    if let Some(cursor) = last_event_id {
        req = req.header("Last-Event-ID", cursor);
    }
    let resp = req.send().await.expect("events");
    (resp.status(), resp.text().await.expect("events body"))
}

fn assert_host_lifecycle_before_terminal(frames: &[SseFrameParsed], terminal_status: &str) {
    assert!(!frames.is_empty(), "expected SSE frames");
    assert_monotonic_sse_frames(frames);
    let terminal = frames
        .iter()
        .rfind(|f| {
            f.event == "run_state" && f.data["status"].as_str() == Some(terminal_status)
        })
        .expect("terminal run_state");
    let terminal_seq = terminal.sequence;
    let host_frames: Vec<_> = frames
        .iter()
        .filter(|f| f.event == "host_event" && f.sequence < terminal_seq)
        .collect();
    let mut last_started = None;
    for (idx, frame) in host_frames.iter().enumerate() {
        if host_event_kind(&frame.data).as_deref() == Some("OpStarted") {
            last_started = Some(idx);
        }
    }
    let start_idx = last_started.expect("OpStarted before terminal");
    let tail = &host_frames[start_idx..];
    let kinds: Vec<String> = tail
        .iter()
        .map(|f| host_event_kind(&f.data).expect("host_event kind"))
        .collect();
    let started = kinds.iter().position(|k| k == "OpStarted").expect("OpStarted");
    let content = kinds
        .iter()
        .position(|k| k == "MessageDelta")
        .expect("MessageDelta after OpStarted");
    let finished = kinds
        .iter()
        .position(|k| k == "OpFinished")
        .expect("OpFinished after content");
    assert!(
        tail[started].sequence < tail[content].sequence,
        "OpStarted -> content ordering"
    );
    assert!(
        tail[content].sequence < tail[finished].sequence,
        "content -> OpFinished ordering"
    );
    assert!(
        tail[finished].sequence < terminal_seq,
        "OpFinished -> terminal run_state ordering"
    );
}

async fn fetch_session_inspect(daemon: &LiveDaemon, sid: &str) -> Value {
    reqwest::Client::new()
        .get(format!(
            "{}/v1/daemon/orchestration/sessions/{}",
            daemon.http_url,
            sid
        ))
        .send()
        .await
        .expect("inspect")
        .json::<Value>()
        .await
        .expect("inspect json")
}

fn parse_sse_run_states(body: &str) -> Vec<Value> {
    let mut out = Vec::new();
    for block in body.split("\n\n") {
        let mut event = None;
        let mut data = None;
        for line in block.lines() {
            if let Some(rest) = line.strip_prefix("event: ") {
                event = Some(rest.trim());
            } else if let Some(rest) = line.strip_prefix("data: ") {
                data = Some(rest);
            }
        }
        if event == Some("run_state") {
            if let Some(d) = data {
                if let Ok(v) = serde_json::from_str::<Value>(d) {
                    out.push(v);
                }
            }
        }
    }
    out
}

fn assert_monotonic_run_state_frames(frames: &[Value], expected_terminal_status: &str) {
    assert!(
        !frames.is_empty(),
        "expected at least one run_state SSE frame"
    );
    let mut last_seq = 0u64;
    let mut seen = std::collections::HashSet::new();
    for frame in frames {
        let seq = frame["sequence"]
            .as_u64()
            .expect("run_state frame must carry numeric sequence");
        assert!(seq > last_seq, "sequences must increase: {seq} after {last_seq}");
        assert!(seen.insert(seq), "duplicate sequence {seq}");
        last_seq = seq;
    }
    let terminal = frames.last().expect("terminal frame");
    assert_eq!(
        terminal["status"].as_str(),
        Some(expected_terminal_status),
        "terminal run_state must match durable status: {terminal}"
    );
}

/// P4 T4: cancel/inspect/events/DB agreement on the same run through the live daemon.
#[tokio::test]
async fn p4_cancel_inspect_events_db_journey() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p4-cancel-journey",
            "seed": "p4-topic",
            "input": { "keyword": "p4", "topic": "p4-topic" },
            "agent_bindings": { "default": { "provider_id": MOCK_PROVIDER } }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while host.streams_started() < 1 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("stream started");
    let row = load_drive_row(&daemon, schedule_id).await;
    let sid = row.current_session_id.as_deref().expect("session");
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "cancel" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(body["status"].as_str(), Some("cancelled"));
    let (db_status, db_state) = wait_for_run_status(&daemon, sid, "cancelled", 15).await;
    let inspect = fetch_session_inspect(&daemon, sid).await;
    assert_eq!(inspect["session"]["status"].as_str(), Some("cancelled"));
    assert_eq!(db_status, "cancelled");
    assert_eq!(db_state["cancel_requested"].as_bool(), Some(true));
    let events = reqwest::Client::new()
        .get(format!(
            "{}/v1/daemon/orchestration/sessions/{}/events",
            daemon.http_url,
            sid
        ))
        .send()
        .await
        .expect("events");
    assert!(events.status().is_success(), "events route must succeed");
    let text = events.text().await.expect("events body");
    let run_states = parse_sse_run_states(&text);
    assert_monotonic_run_state_frames(&run_states, "cancelled");
}

/// P4 T4: late subscribe after a terminal run replays retained run_state.
#[tokio::test]
async fn p4_late_subscribe_replays_terminal_run_state() {
    let host = BlockingHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p4-late-replay",
            "seed": "p4-topic",
            "input": { "keyword": "p4", "topic": "p4-topic" },
            "agent_bindings": { "default": { "provider_id": MOCK_PROVIDER } }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    host.release();
    let row = load_drive_row(&daemon, schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("session")
        .to_string();
    let (parked_status, state) =
        wait_for_run_status(&daemon, &sid, "waiting_for_input", 15).await;
    assert_eq!(parked_status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("durable wait id")
        .to_string();
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": wait_id }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    let (run_status, _) = wait_for_run_status(&daemon, &sid, "completed", 15).await;
    assert_eq!(run_status, "completed");
    let events = reqwest::Client::new()
        .get(format!(
            "{}/v1/daemon/orchestration/sessions/{}/events",
            daemon.http_url,
            sid
        ))
        .send()
        .await
        .expect("events");
    assert!(events.status().is_success(), "{}", events.text().await.unwrap_or_default());
    let text = events.text().await.expect("events body");
    let frames = parse_sse_frames(&text);
    assert_host_lifecycle_before_terminal(&frames, "completed");
}

/// P4 T4: unconfirmed cleanup stays actionable Interrupted on inspect/DB.
#[tokio::test]
async fn p4_cleanup_failure_recovery_projection() {
    let host = BlockingHost::non_cooperative();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules",
        json!({
            "creator_id": "test_creator",
            "preset_id": PUBLIC_PRESET,
            "label": "p4-interrupted",
            "seed": "p4-topic",
            "input": { "keyword": "p4", "topic": "p4-topic" },
            "agent_bindings": { "default": { "provider_id": MOCK_PROVIDER } }
        }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        while host.streams_started() < 1 {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
    })
    .await
    .expect("stream started");
    let row = load_drive_row(&daemon, schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("session")
        .to_string();
    let (status, body) = post_json(
        &daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "cancel" }),
    )
    .await;
    assert_eq!(status, reqwest::StatusCode::OK, "{body}");
    assert_eq!(
        body["status"].as_str(),
        Some("interrupted"),
        "unconfirmed cancel must report interrupted: {body}"
    );
    let (run_status, state) = wait_for_run_status(&daemon, &sid, "interrupted", 20).await;
    assert_eq!(run_status, "interrupted");
    assert_eq!(
        state["failure"]["code"].as_str(),
        Some("cancel_cleanup_unconfirmed"),
        "actionable cleanup reason must be durable: {state}"
    );
    let inspect = fetch_session_inspect(&daemon, &sid).await;
    assert_eq!(inspect["session"]["status"].as_str(), Some("interrupted"));
    assert!(
        inspect["session"]["failureReason"]
            .as_str()
            .is_some_and(|r| r.contains("cancel cleanup unconfirmed")),
        "inspect must carry actionable cleanup reason: {inspect}"
    );
}


/// Drive a freshly admitted public run to its durable terminal `completed`.
///
/// The `memory-augmented` preset parks at the `persist` state
/// (`exit_when: kind: manual`), so a terminal — and therefore a retained
/// terminal SSE ring — is only reachable by consuming the durable wait token
/// with an authorized `continue`, exactly as a real operator would. Tests that
/// assert on a TERMINAL ring must go through this; asserting `completed`
/// straight after admission can only ever observe `waiting_for_input`.
async fn drive_public_run_to_terminal(daemon: &LiveDaemon, schedule_id: &str, sid: &str) {
    let (status, state) = wait_for_run_status(daemon, sid, "waiting_for_input", 20).await;
    assert_eq!(status, "waiting_for_input");
    let wait_id = state["wait"]["wait_id"]
        .as_str()
        .expect("durable wait id")
        .to_string();
    let (code, body) = post_json(
        daemon,
        &format!("/v1/daemon/orchestration/schedules/{schedule_id}/signal"),
        json!({ "signal": "continue", "wait_id": wait_id }),
    )
    .await;
    assert_eq!(code, reqwest::StatusCode::OK, "{body}");
    let (run_status, _) = wait_for_run_status(daemon, sid, "completed", 20).await;
    assert_eq!(run_status, "completed");
}

/// P4 T4: Last-Event-ID reconnect returns strictly-later frames without duplicates.
#[tokio::test]
async fn p4_public_sse_last_event_id_strict_later_dedupe() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = add_public(&daemon, "p4-last-event-id").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    let _ = wait_for_prompts(&host, 15).await;
    let row = load_drive_row(&daemon, schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("session")
        .to_string();
    drive_public_run_to_terminal(&daemon, schedule_id, &sid).await;
    let (status, full_body) = fetch_session_events(&daemon, &sid, None).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{full_body}");
    let all = parse_sse_frames(&full_body);
    assert_monotonic_sse_frames(&all);
    let cursor = all
        .iter()
        .find(|f| f.event == "host_event")
        .expect("host_event frame")
        .id
        .clone();
    let cursor_seq = all
        .iter()
        .find(|f| f.id == cursor)
        .expect("cursor frame")
        .sequence;
    let (status, tail_body) = fetch_session_events(&daemon, &sid, Some(&cursor)).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{tail_body}");
    let tail = parse_sse_frames(&tail_body);
    assert_monotonic_sse_frames(&tail);
    for frame in &tail {
        assert!(
            frame.sequence > cursor_seq,
            "Last-Event-ID must replay strictly later than cursor: {} <= {}",
            frame.sequence,
            cursor_seq
        );
        assert!(
            !all.iter().any(|f| f.id == frame.id && f.sequence <= cursor_seq),
            "duplicate or non-later frame id {}",
            frame.id
        );
    }
}

/// P4 T4: foreign-owned durable run is denied on the public events route.
#[tokio::test]
async fn p4_public_sse_foreign_owner_denied() {
    let host = MockHost::new();
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = add_public(&daemon, "p4-foreign-owner").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    let _ = wait_for_prompts(&host, 15).await;
    let row = load_drive_row(&daemon, schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("session")
        .to_string();
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, cached_at, data)
         VALUES ('other_creator', 'Other', 'active', datetime('now'), '{}')",
    )
    .execute(&daemon.pool)
    .await
    .expect("seed foreign creator");
    sqlx::query("UPDATE orchestration_sessions SET creator_id = 'other_creator' WHERE session_id = ?")
        .bind(&sid)
        .execute(&daemon.pool)
        .await
        .expect("reassign owner");
    let (status, body) = fetch_session_events(&daemon, &sid, None).await;
    assert_eq!(
        status,
        reqwest::StatusCode::NOT_FOUND,
        "foreign owner must be denied via session owner path: {body}"
    );
}


/// Emits OpStarted + many MessageDelta frames + OpFinished in one operation.
struct EventFloodHost {
    deltas: usize,
    prompts: Mutex<Vec<String>>,
}

impl EventFloodHost {
    fn with_deltas(deltas: usize) -> Arc<Self> {
        Arc::new(Self {
            deltas,
            prompts: Mutex::new(Vec::new()),
        })
    }
}

#[async_trait::async_trait]
impl HostFacade for EventFloodHost {
    async fn start(&self, _config: HostStartConfig) -> HostResult<()> {
        Ok(())
    }

    async fn create_session(
        &self,
        request: nexus_agent_host::capability::CreateSessionRequest,
    ) -> HostResult<HostSession> {
        Ok(HostSession {
            id: HostSessionId::new(),
            provider_id: request.provider_id,
            state: SessionState::Ready,
            created_at: chrono::Utc::now(),
            active_op_id: None,
            negotiated_capabilities: CapabilityDescriptor::native_cli_limited(),
            owner: request.owner,
            process_identity: None,
        })
    }

    async fn exec(
        &self,
        session_id: HostSessionId,
        op: HostOperation,
    ) -> HostResult<HostEventStream> {
        let op_id = match op {
            HostOperation::Prompt { op_id, content, .. } => {
                let text = match content.as_slice() {
                    [HostContentBlock::Text { text }] => text.clone(),
                    other => format!("unexpected content {other:?}"),
                };
                self.prompts.lock().expect("prompts").push(text);
                op_id
            }
            other => {
                return Err(HostError::internal(format!(
                    "unexpected operation {other:?}"
                )));
            }
        };
        let started = HostEvent::OpStarted(OperationStartedEvent {
            op_id: op_id.clone(),
            session_id: session_id.clone(),
        });
        let mut events = vec![Ok(started.clone())];
        for i in 0..self.deltas {
            events.push(Ok(HostEvent::MessageDelta(TextDeltaEvent {
                session_id: session_id.clone(),
                op_id: op_id.clone(),
                text: format!("delta-{i}"),
            })));
        }
        let finished = HostEvent::OpFinished(OperationFinishedEvent {
            session_id,
            op_id,
            reason: FinishReason::EndTurn,
        });
        events.push(Ok(finished.clone()));
        Ok(Box::pin(futures_util::stream::iter(events)))
    }

    async fn cancel(&self, _op_id: HostOperationId) -> HostResult<()> {
        Ok(())
    }

    async fn health(&self) -> HostResult<HostHealth> {
        Ok(HostHealth {
            running: true,
            active_sessions: 0,
            active_operations: 0,
        })
    }

    async fn shutdown(&self) -> HostResult<()> {
        Ok(())
    }

    async fn shutdown_session(&self, _session_id: HostSessionId) -> HostResult<()> {
        Ok(())
    }

    async fn list_sessions(&self) -> HostResult<Vec<HostSession>> {
        Ok(Vec::new())
    }

    async fn provider_catalog(&self) -> HostResult<ProviderCatalog> {
        Ok(ProviderCatalog {
            entries: vec![ProviderCatalogEntry {
                provider_id: nexus_agent_host::ProviderId::new(MOCK_PROVIDER),
                display_name: MOCK_PROVIDER.to_string(),
                protocol_kind: ProtocolKind::NativeCli,
                launch: LaunchStrategy::NativeCli {
                    command: "mock".to_string(),
                    args: vec![],
                    env: HashMap::new(),
                },
                source: DiscoverySource::Config,
                trust: TrustLevel::Explicit,
                capabilities: CapabilityDescriptor::native_cli_limited(),
                health: ProviderHealth {
                    provider_id: nexus_agent_host::ProviderId::new(MOCK_PROVIDER),
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
        let (_, rx) = tokio::sync::broadcast::channel(1);
        rx
    }
}

/// P4 T4: reconnect after ring eviction surfaces explicit gap then retained tail.
#[tokio::test]
async fn p4_public_sse_gap_on_evicted_cursor() {
    let host = EventFloodHost::with_deltas(300);
    let daemon = LiveDaemon::start_with_agent_host(host.clone()).await;
    let (status, body) = add_public(&daemon, "p4-gap-cursor").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id");
    let row = load_drive_row(&daemon, schedule_id).await;
    let sid = row
        .current_session_id
        .as_deref()
        .expect("session")
        .to_string();
    drive_public_run_to_terminal(&daemon, schedule_id, &sid).await;
    let (status, full_body) = fetch_session_events(&daemon, &sid, None).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{full_body}");
    let all = parse_sse_frames(&full_body);
    assert_monotonic_sse_frames(&all);
    let epoch = all
        .first()
        .expect("frame")
        .id
        .split(':')
        .next()
        .expect("epoch");
    let (status, replay_body) =
        fetch_session_events(&daemon, &sid, Some(&format!("{epoch}:1"))).await;
    assert_eq!(status, reqwest::StatusCode::OK, "{replay_body}");
    let replay = parse_sse_frames(&replay_body);
    assert!(
        replay.iter().any(|f| f.event == "gap"),
        "evicted cursor must surface explicit gap: {:?}",
        replay.iter().map(|f| &f.event).collect::<Vec<_>>()
    );
    assert_monotonic_sse_frames(&replay);
}
