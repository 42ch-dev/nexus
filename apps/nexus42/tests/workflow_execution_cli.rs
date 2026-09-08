//! v1.186 P2 Task 1 — public admission through the production daemon.
//!
//! Fixture: hermetic `LiveDaemon` (real router) plus JSON HTTP against
//! `/v1/daemon/orchestration/schedules`. CLI stdout is human-formatted
//! and is not treated as JSON. No live omp.

#![allow(clippy::unwrap_used, clippy::expect_used)]

mod common;

use common::LiveDaemon;
use nexus_agent_host::capability::model::{
    CapabilityDescriptor, FinishReason, HostContentBlock, HostEvent, HostEventStream, HostHealth,
    HostOperation, HostStartConfig, OperationFinishedEvent, OperationStartedEvent, ProtocolKind,
    ProviderHealth, TextDeltaEvent,
};
use nexus_agent_host::{
    DiscoverySource, HostError, HostFacade, HostOperationId, HostResult, HostSession,
    HostSessionId, LaunchStrategy, ProviderCatalog, ProviderCatalogEntry, SessionState,
    TrustLevel,
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
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id").to_string();

    // The drive loop steps recall → generate (Host prompt) → persist, then
    // parks at the manual wait. Wait for the prompt to be recorded.
    let prompts = match tokio::time::timeout(std::time::Duration::from_secs(15), async {
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
        Ok(prompts) => prompts,
        Err(_) => {
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
        }
    };

    assert!(
        prompts.iter().any(|p| p.contains("p2-t1-topic")),
        "rendered prompt must carry the frozen preset.input.topic: {prompts:?}"
    );
    assert!(
        prompts.iter().all(|p| !p.contains("transformed:mock-output")),
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
    let (status, state) = match tokio::time::timeout(std::time::Duration::from_secs(15), async {
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
        Ok(parked) => parked,
        Err(_) => {
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
        }
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
            created_at, updated_at, work_id, execution_policy)
           VALUES ('SCHCONCURRENT', 'test_creator', ?, 1, 'pending',
                   'serial', 0, 'p2-t1-concurrent-pending', ?, ?, NULL, 'driven_v1')",
    )
    .bind(PUBLIC_PRESET)
    .bind(now)
    .bind(now)
    .execute(&daemon.pool)
    .await
    .expect("seed fresh pending driven_v1 row");

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
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(sid)
    .fetch_one(&daemon.pool)
    .await
    .expect("count sessions");
    assert_eq!(count, 1, "one pending row must mint exactly one session");
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
            "input": { "keyword": "p2-t1", "topic": "p2-t1-topic" }
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
    assert!(row.current_session_id.is_none(), "blocked row must be unowned");
}

/// N-7: serial concurrency blocking — a second serial schedule for the same
/// creator stays pending while the first driven run is running.
#[tokio::test]
async fn admission_serial_blocked_stays_pending() {
    let daemon = LiveDaemon::start().await;
    let (status, body) = add_public(&daemon, "p2-t1-serial-first").await;
    assert_eq!(status, reqwest::StatusCode::CREATED, "{body}");
    let first_id = body["schedule_id"].as_str().expect("schedule_id").to_string();
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
    assert!(row.current_session_id.is_none(), "serial-blocked row is unowned");
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

    // Unknown role: refused before enqueue (400 invalid_input).
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
        text.contains("unknown role"),
        "refusal must name the unknown role: {text}"
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM orchestration_sessions WHERE session_id = ?",
    )
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
        row.current_session_id.as_deref().is_some_and(|s| !s.is_empty()),
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
    let schedule_id = body["schedule_id"].as_str().expect("schedule_id").to_string();

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

#[tokio::test]
async fn explicit_legacy_running_without_session_starts() {
    let daemon = LiveDaemon::start().await;
    let now = chrono::Utc::now().timestamp();
    sqlx::query(
        "INSERT INTO creator_schedules
           (schedule_id, creator_id, preset_id, preset_version, status,
            concurrency_kind, current_core_context_version, label,
            created_at, updated_at, work_id, execution_policy)
           VALUES ('SCHLEGACYRUN', 'test_creator', ?, 1, 'running',
                   'serial', 0, 'p2-t1-legacy-running', ?, ?, NULL, 'legacy_inert')",
    )
    .bind(PUBLIC_PRESET)
    .bind(now)
    .bind(now)
    .execute(&daemon.pool)
    .await
    .expect("seed legacy running-without-session row");

    let (status, body) = post_json(
        &daemon,
        "/v1/daemon/orchestration/schedules/SCHLEGACYRUN/signal",
        json!({ "signal": "start" }),
    )
    .await;
    assert!(
        status.is_success(),
        "legacy start failed: status={status} body={body}"
    );

    let row = load_drive_row(&daemon, "SCHLEGACYRUN").await;
    assert_eq!(row.execution_policy, "driven_v1");
    assert!(
        row.current_session_id.as_deref().is_some_and(|s| !s.is_empty()),
        "legacy start must mint an owned session: status={} session={:?}",
        row.status,
        row.current_session_id
    );
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
