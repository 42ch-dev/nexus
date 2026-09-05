//! Process-level `nexus42 creator character run` against a live daemon + mock host.

mod common;

use common::rn_act4::{
    seed, stderr, stdout, NAME_A_SHARE, NAME_A_W1_LOCAL, NAME_B_SHARE, NAME_W1_PUBLIC,
    NAME_W1_SECRET, NAME_W2_PUBLIC,
};
use common::LiveDaemon;
use nexus_agent_host::capability::model::{
    FinishReason, HostContentBlock, HostEvent, HostEventStream, HostHealth, HostOperation,
    HostStartConfig, OperationFinishedEvent, OperationStartedEvent, TextDeltaEvent,
};
use nexus_agent_host::{
    HostError, HostFacade, HostOperationId, HostResult, HostSession, HostSessionId, ProviderCatalog,
    SessionState,
};
use serde_json::Value;
use std::collections::HashMap;
use std::process::Output;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MOCK_RESULT: &str = "mock-host-result";

struct MockHost {
    sessions: Mutex<HashMap<HostSessionId, HostSession>>,
    prompts: Mutex<Vec<String>>,
    last_create_metadata: Mutex<Option<serde_json::Value>>,
    create_sessions: AtomicU64,
    execs: AtomicU64,
    events: tokio::sync::broadcast::Sender<HostEvent>,
}

impl MockHost {
    fn new() -> Arc<Self> {
        let (events, _) = tokio::sync::broadcast::channel(64);
        Arc::new(Self {
            sessions: Mutex::new(HashMap::new()),
            prompts: Mutex::new(Vec::new()),
            last_create_metadata: Mutex::new(None),
            create_sessions: AtomicU64::new(0),
            execs: AtomicU64::new(0),
            events,
        })
    }

    fn last_prompt(&self) -> String {
        self.prompts
            .lock()
            .expect("prompts")
            .last()
            .cloned()
            .unwrap_or_default()
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
        self.create_sessions.fetch_add(1, Ordering::SeqCst);
        *self.last_create_metadata.lock().expect("metadata") = Some(request.metadata.clone());
        let session = HostSession {
            id: HostSessionId::new(),
            provider_id: request.provider_id,
            state: SessionState::Ready,
            created_at: chrono::Utc::now(),
            active_op_id: None,
            negotiated_capabilities:
                nexus_agent_host::capability::model::CapabilityDescriptor::native_cli_limited(),
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
        self.execs.fetch_add(1, Ordering::SeqCst);
        let op_id = match op {
            HostOperation::Prompt { op_id, content } => {
                let text = match content.as_slice() {
                    [HostContentBlock::Text { text }] => text.clone(),
                    other => format!("unexpected content {other:?}"),
                };
                self.prompts.lock().expect("prompts").push(text);
                op_id
            }
            HostOperation::SetModel { model } => {
                self.prompts
                    .lock()
                    .expect("prompts")
                    .push(format!("set-model:{model}"));
                HostOperationId::new()
            }
            HostOperation::SetMode { mode } => {
                self.prompts
                    .lock()
                    .expect("prompts")
                    .push(format!("set-mode:{mode}"));
                HostOperationId::new()
            }
        };
        let started = HostEvent::OpStarted(OperationStartedEvent {
            op_id: op_id.clone(),
            session_id: session_id.clone(),
        });
        let delta = HostEvent::MessageDelta(TextDeltaEvent {
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            text: MOCK_RESULT.to_string(),
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
            active_sessions: self.sessions.lock().expect("sessions").len(),
            active_operations: 0,
        })
    }

    async fn shutdown(&self) -> HostResult<()> {
        Ok(())
    }

    async fn shutdown_session(&self, session_id: HostSessionId) -> HostResult<()> {
        self.sessions
            .lock()
            .expect("sessions")
            .remove(&session_id)
            .ok_or_else(|| HostError::internal("session"))?;
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
        self.events.subscribe()
    }
}

fn json_out(out: &Output) -> Value {
    serde_json::from_str(&stdout(out)).unwrap_or_else(|_| panic!("json: {}", stdout(out)))
}

fn run_args<'a>(
    character_id: &'a str,
    world_id: &'a str,
    binding_id: &'a str,
    extra: &'a [&'a str],
) -> Vec<&'a str> {
    let mut args = vec![
        "creator",
        "character",
        "run",
        "--character-id",
        character_id,
        "--world-id",
        world_id,
        "--binding-id",
        binding_id,
        "--prompt",
        "Act now.",
        "--json",
    ];
    args.extend_from_slice(extra);
    args
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_authorized_view_empty_headings_human_json() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;

    let json_run = d
        .cli(&run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &[],
        ))
        .await;
    assert!(
        json_run.status.success(),
        "json run: {}",
        stderr(&json_run)
    );
    let payload = json_out(&json_run);
    assert_eq!(payload["result"], MOCK_RESULT);
    assert_eq!(payload["session"]["provider_id"], "mock-provider");
    assert_eq!(
        payload["session"]["actor_ref"]["character_id"],
        g.character_a
    );
    assert_eq!(payload["session"]["viewpoint"]["world_id"], g.world_w1);
    assert_eq!(payload["session"]["viewpoint"]["binding_id"], g.bind_a_w1);
    assert!(payload["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e.get("OpFinished").is_some()));

    let human = d
        .cli(&[
            "creator",
            "character",
            "run",
            "--character-id",
            &g.character_a,
            "--world-id",
            &g.world_w1,
            "--binding-id",
            &g.bind_a_w1,
            "--prompt",
            "Act now.",
        ])
        .await;
    assert!(human.status.success(), "human run: {}", stderr(&human));
    let human_out = stdout(&human);
    assert!(human_out.contains(&g.character_a));
    assert!(human_out.contains(MOCK_RESULT));
    assert!(!human_out.trim_start().starts_with('{'));

    let prompt = host.last_prompt();
    assert!(prompt.contains("Act now."));
    assert!(prompt.contains("## Character SOUL"));
    assert!(prompt.contains("## Character Memory"));
    assert!(prompt.contains("## Character ToM — L1"));
    assert!(prompt.contains("## Character ToM — L2"));
    assert!(!prompt.contains("## Personality"));
    assert!(prompt.contains(NAME_W1_PUBLIC));
    assert!(prompt.contains(NAME_A_SHARE));
    assert!(prompt.contains(NAME_A_W1_LOCAL));
    assert!(!prompt.contains(NAME_W1_SECRET));
    assert!(!prompt.contains(NAME_W2_PUBLIC));
    assert!(!prompt.contains(NAME_B_SHARE));
    assert_eq!(host.execs.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_deny_matrix_moves_no_host_counters() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;

    let missing = d
        .cli(&run_args(
            "chr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
            &g.world_w1,
            &g.bind_a_w1,
            &[],
        ))
        .await;
    assert!(!missing.status.success());
    assert!(
        stderr(&missing).contains("not_found") || stderr(&missing).contains("404"),
        "{}",
        stderr(&missing)
    );

    let bad_binding = d
        .cli(&run_args(
            &g.character_a,
            &g.world_w1,
            "awb_dddddddddddddddddddddddddddddddd",
            &[],
        ))
        .await;
    assert!(!bad_binding.status.success());

    let cross_world = d
        .cli(&run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w2,
            &[],
        ))
        .await;
    assert!(!cross_world.status.success());

    assert_eq!(host.create_sessions.load(Ordering::SeqCst), 0);
    assert_eq!(host.execs.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_isolation_and_legacy_outside_lookup() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;
    let cwd_a = d.home.path().join("cwd-a");
    let cwd_b = d.home.path().join("cwd-b");
    std::fs::create_dir_all(&cwd_a).unwrap();
    std::fs::create_dir_all(&cwd_b).unwrap();
    let cwd_a = cwd_a.to_string_lossy().into_owned();
    let cwd_b = cwd_b.to_string_lossy().into_owned();

    let first_out = d
        .cli(&run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &["--cwd", &cwd_a],
        ))
        .await;
    assert!(first_out.status.success(), "{}", stderr(&first_out));
    let first = json_out(&first_out);
    assert_eq!(first["result"], MOCK_RESULT);
    let first_id = first["session"]["session_id"].as_str().unwrap().to_string();

    let reuse_out = d
        .cli(&run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &["--cwd", &cwd_a],
        ))
        .await;
    assert!(reuse_out.status.success(), "{}", stderr(&reuse_out));
    let reuse = json_out(&reuse_out);
    assert_eq!(reuse["session"]["session_id"], first_id);

    async fn isolated(d: &LiveDaemon, args: &[&str], first_id: &str, seen: &mut Vec<String>) {
        let out = d.cli(args).await;
        assert!(out.status.success(), "isolation run: {}", stderr(&out));
        let id = json_out(&out)["session"]["session_id"]
            .as_str()
            .unwrap()
            .to_string();
        assert_ne!(id, first_id);
        assert!(!seen.contains(&id), "duplicate isolated session {id}");
        seen.push(id);
    }
    let mut other_ids = Vec::new();
    isolated(
        &d,
        &run_args(&g.character_b, &g.world_w1, &g.bind_b_w1, &["--cwd", &cwd_a]),
        &first_id,
        &mut other_ids,
    )
    .await;
    isolated(
        &d,
        &run_args(&g.character_a, &g.world_w2, &g.bind_a_w2, &["--cwd", &cwd_a]),
        &first_id,
        &mut other_ids,
    )
    .await;
    isolated(
        &d,
        &run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &["--cwd", &cwd_b]),
        &first_id,
        &mut other_ids,
    )
    .await;
    isolated(
        &d,
        &run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &["--cwd", &cwd_a, "--provider-id", "other-provider"],
        ),
        &first_id,
        &mut other_ids,
    )
    .await;
    isolated(
        &d,
        &run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &["--cwd", &cwd_a, "--model", "m1"],
        ),
        &first_id,
        &mut other_ids,
    )
    .await;
    isolated(
        &d,
        &run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &["--cwd", &cwd_a, "--mode", "ask"],
        ),
        &first_id,
        &mut other_ids,
    )
    .await;
    isolated(
        &d,
        &run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &["--cwd", &cwd_a, "--branch-id", "fbk_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
        ),
        &first_id,
        &mut other_ids,
    )
    .await;
    isolated(
        &d,
        &run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &["--cwd", &cwd_a, "--event-id", "evt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"],
        ),
        &first_id,
        &mut other_ids,
    )
    .await;

    let client = reqwest::Client::new();
    let legacy = client
        .post(format!("{}/v1/daemon/agent-host/sessions", d.http_url))
        .json(&serde_json::json!({
            "provider_id": "mock-provider",
            "cwd": cwd_a,
        }))
        .send()
        .await
        .unwrap();
    assert!(legacy.status().is_success(), "legacy create");
    let legacy_json: Value = legacy.json().await.unwrap();
    let legacy_id = legacy_json["session_id"].as_str().unwrap();
    assert!(!legacy_json.as_object().unwrap().contains_key("actor_ref"));
    assert!(!legacy_json.as_object().unwrap().contains_key("viewpoint"));
    assert_ne!(legacy_id, first_id);
    assert_eq!(
        host.last_create_metadata.lock().expect("metadata").as_ref(),
        Some(&serde_json::Value::Null)
    );

    let still = d
        .cli(&run_args(
            &g.character_a,
            &g.world_w1,
            &g.bind_a_w1,
            &["--cwd", &cwd_a],
        ))
        .await;
    assert!(still.status.success(), "{}", stderr(&still));
    let still_actor = json_out(&still);
    assert_eq!(still_actor["session"]["session_id"], first_id);
    assert_ne!(still_actor["session"]["session_id"], legacy_id);
}

const TOM_L1_MARKER: &str = "TOMRUNL1MARKER dock safety";
const TOM_L2_MARKER: &str = "TOMRUNL2MARKER models Ben";

async fn seed_tom_carrier_run(d: &LiveDaemon, character_id: &str) -> String {
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
    use nexus_knowledge::world_kb::store::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;
    use serde_json::json;
    let store = SqliteKbStore::new(d.pool.clone());
    let mut kb = KnowledgeEntryRecord::for_character(character_id, BlockType::Character, "TomRunCarrier");
    kb.modules = Some(json!({ "belief": [] }));
    let id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    id
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_includes_tom_l1_l2_after_cli_record() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;
    let carrier = seed_tom_carrier_run(&d, &g.character_a).await;
    let before = host.execs.load(Ordering::SeqCst);

    for (holder, order, rev, prop) in [
        (&g.character_a, "1", "0", TOM_L1_MARKER),
        (&g.character_b, "2", "1", TOM_L2_MARKER),
    ] {
        let out = d.cli(&[
            "creator", "character", "tom", "record",
            "--character-id", &g.character_a,
            "--world-id", &g.world_w1,
            "--binding-id", &g.bind_a_w1,
            "--carrier-entry-id", &carrier,
            "--expected-revision", rev,
            "--holder", holder,
            "--proposition", prop,
            "--order", order,
            "--truth", "True",
            "--access", "Private",
            "--representation", "Explicit",
            "--content-type", "Location",
            "--source", "Perception",
            "--context", "Neutral",
        ]).await;
        assert!(out.status.success(), "record: {}", stderr(&out));
    }
    assert_eq!(host.execs.load(Ordering::SeqCst), before);

    let out = d.cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &[])).await;
    assert!(out.status.success(), "run: {}", stderr(&out));
    assert_eq!(host.execs.load(Ordering::SeqCst), before + 1);
    let prompt = host.last_prompt();
    assert!(prompt.contains(TOM_L1_MARKER), "{prompt}");
    assert!(prompt.contains(TOM_L2_MARKER), "{prompt}");
    assert!(prompt.contains("## Character ToM — L1"));
    assert!(prompt.contains("## Character ToM — L2"));
}

