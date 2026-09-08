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
    HostError, HostFacade, HostOperationId, HostResult, HostSession, HostSessionId,
    ProviderCatalog, SessionState,
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
            owner: request.owner,
            process_identity: None,
        };
        self.sessions
            .lock()
            .expect("sessions")
            .insert(session.id.clone(), session.clone());
        Ok(session)
    }

    #[allow(
        clippy::redundant_clone,
        clippy::if_not_else,
        clippy::branches_sharing_code
    )]
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
        .cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &[]))
        .await;
    assert!(json_run.status.success(), "json run: {}", stderr(&json_run));
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
        .cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w2, &[]))
        .await;
    assert!(!cross_world.status.success());

    assert_eq!(host.create_sessions.load(Ordering::SeqCst), 0);
    assert_eq!(host.execs.load(Ordering::SeqCst), 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(
    clippy::too_many_lines,
    clippy::items_after_statements, // nested fixture fn follows seed statements
    clippy::similar_names // A/B roles
)]
async fn character_run_isolation_and_legacy_outside_lookup() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;
    let cwd_a = d.home.path().join("cwd-a");

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

    let mut other_ids = Vec::new();
    isolated(
        &d,
        &run_args(
            &g.character_b,
            &g.world_w1,
            &g.bind_b_w1,
            &["--cwd", &cwd_a],
        ),
        &first_id,
        &mut other_ids,
    )
    .await;
    isolated(
        &d,
        &run_args(
            &g.character_a,
            &g.world_w2,
            &g.bind_a_w2,
            &["--cwd", &cwd_a],
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
            &["--cwd", &cwd_b],
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
            &[
                "--cwd",
                &cwd_a,
                "--branch-id",
                "fbk_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ],
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
            &[
                "--cwd",
                &cwd_a,
                "--event-id",
                "evt_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            ],
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
const TOM_SOUL_MARKER: &str = "TOMSOULMARKER keeps a ledger of every debt owed to the river";
const TOM_MEM_MARKER: &str = "TOMMEMMARKER the harbor accord holds because Ava keeps it";
const TOM_W1_BINDING_MARKER: &str = "TOMW1BINDMARKER only W1 binding carrier belief";

async fn seed_tom_carrier_run(d: &LiveDaemon, character_id: &str) -> String {
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
    use nexus_knowledge::world_kb::store::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;
    use serde_json::json;
    let store = SqliteKbStore::new(d.pool.clone());
    let mut kb =
        KnowledgeEntryRecord::for_character(character_id, BlockType::Character, "TomRunCarrier");
    kb.modules = Some(json!({ "belief": [] }));
    let id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    id
}

async fn seed_binding_tom_carrier(d: &LiveDaemon, binding_id: &str) -> String {
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::KnowledgeEntryRecord;
    use nexus_knowledge::world_kb::store::KbStore;
    use nexus_local_db::kb_store::SqliteKbStore;
    use serde_json::json;
    let store = SqliteKbStore::new(d.pool.clone());
    let mut kb =
        KnowledgeEntryRecord::for_binding(binding_id, BlockType::Character, "TomBindingCarrier");
    kb.modules = Some(json!({ "belief": [] }));
    let id = kb.entry_id.clone();
    store.insert_knowledge_entry(kb).await.unwrap();
    id
}

fn fragment_digest(marker: &str) -> String {
    format!("{marker} — researched background detail for texture and continuity.")
}

async fn cli_ok(d: &LiveDaemon, args: &[&str]) -> Output {
    let out = d.cli(args).await;
    assert!(out.status.success(), "cli {args:?}: {}", stderr(&out));
    out
}

/// RN-ACT-4 + P3 memory + P4 `ToM` full-mind dogfood (v1.184 P4 Task 3).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)] // single P0-P4 full-mind dogfood proof
async fn character_tom_full_mind_p0_p4_dogfood() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;
    let chr = g.character_a.as_str();
    let carrier = seed_tom_carrier_run(&d, chr).await;
    let before_execs = host.execs.load(Ordering::SeqCst);

    let soul_dir = d
        .home
        .path()
        .join(".nexus42")
        .join(".nexus42")
        .join("creators")
        .join(&g.creator_id)
        .join("characters")
        .join(chr);
    std::fs::create_dir_all(&soul_dir).unwrap();
    std::fs::write(
        soul_dir.join("SOUL.md"),
        format!("# Ava\n\n{TOM_SOUL_MARKER}\n"),
    )
    .unwrap();
    cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "capture",
            "--character-id",
            chr,
            "--pending-id",
            "pend_tom_dog",
            "--session-id",
            "sess_tom_dog",
            "--task-kind",
            "research",
            "--digest",
            &fragment_digest(TOM_MEM_MARKER),
        ],
    )
    .await;
    cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "review",
            "--character-id",
            chr,
        ],
    )
    .await;

    for (holder, order, rev, prop) in [
        (chr, "1", "0", TOM_L1_MARKER),
        (&g.character_b, "2", "1", TOM_L2_MARKER),
    ] {
        cli_ok(
            &d,
            &[
                "creator",
                "character",
                "tom",
                "record",
                "--character-id",
                chr,
                "--world-id",
                &g.world_w1,
                "--binding-id",
                &g.bind_a_w1,
                "--carrier-entry-id",
                &carrier,
                "--expected-revision",
                rev,
                "--holder",
                holder,
                "--proposition",
                prop,
                "--order",
                order,
                "--truth",
                "True",
                "--access",
                "Private",
                "--representation",
                "Explicit",
                "--content-type",
                "Location",
                "--source",
                "Perception",
                "--context",
                "Neutral",
            ],
        )
        .await;
    }
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        before_execs,
        "tom record must not call host"
    );

    let show_human = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "tom",
            "show",
            "--character-id",
            chr,
            "--world-id",
            &g.world_w1,
            "--binding-id",
            &g.bind_a_w1,
        ],
    )
    .await;
    let show_text = stdout(&show_human);
    assert!(show_text.contains(TOM_L1_MARKER));
    assert!(show_text.contains(TOM_L2_MARKER));
    let show_json = json_out(
        &cli_ok(
            &d,
            &[
                "creator",
                "character",
                "tom",
                "show",
                "--character-id",
                chr,
                "--world-id",
                &g.world_w1,
                "--binding-id",
                &g.bind_a_w1,
                "--json",
            ],
        )
        .await,
    );
    let orders: Vec<i64> = show_json["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["order"].as_i64().unwrap())
        .collect();
    assert_eq!(orders, vec![1, 2]);

    // Binding-owned carrier: visible in W1 binding scope only (not W2).
    let bind_carrier = seed_binding_tom_carrier(&d, &g.bind_a_w1).await;
    cli_ok(
        &d,
        &[
            "creator",
            "character",
            "tom",
            "record",
            "--character-id",
            chr,
            "--world-id",
            &g.world_w1,
            "--binding-id",
            &g.bind_a_w1,
            "--carrier-entry-id",
            &bind_carrier,
            "--expected-revision",
            "0",
            "--holder",
            chr,
            "--proposition",
            TOM_W1_BINDING_MARKER,
            "--order",
            "1",
            "--truth",
            "True",
            "--access",
            "Private",
            "--representation",
            "Explicit",
            "--content-type",
            "Location",
            "--source",
            "Perception",
            "--context",
            "Neutral",
        ],
    )
    .await;

    let w1_show = stdout(
        &cli_ok(
            &d,
            &[
                "creator",
                "character",
                "tom",
                "show",
                "--character-id",
                chr,
                "--world-id",
                &g.world_w1,
                "--binding-id",
                &g.bind_a_w1,
            ],
        )
        .await,
    );
    assert!(w1_show.contains(TOM_W1_BINDING_MARKER));

    let w2_show = stdout(
        &cli_ok(
            &d,
            &[
                "creator",
                "character",
                "tom",
                "show",
                "--character-id",
                chr,
                "--world-id",
                &g.world_w2,
                "--binding-id",
                &g.bind_a_w2,
            ],
        )
        .await,
    );
    assert!(
        !w2_show.contains(TOM_W1_BINDING_MARKER),
        "binding-local ToM must not leak into W2: {w2_show}"
    );
    // Character-owned L1/L2 remain visible in W2 (same viewer carriers).
    assert!(w2_show.contains(TOM_L1_MARKER) && w2_show.contains(TOM_L2_MARKER));

    let b_show = stdout(
        &cli_ok(
            &d,
            &[
                "creator",
                "character",
                "tom",
                "show",
                "--character-id",
                &g.character_b,
                "--world-id",
                &g.world_w1,
                "--binding-id",
                &g.bind_b_w1,
            ],
        )
        .await,
    );
    assert!(
        !b_show.contains(TOM_L1_MARKER) && !b_show.contains(TOM_L2_MARKER),
        "B must not see A's carrier beliefs: {b_show}"
    );
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        before_execs,
        "tom show must not call host"
    );

    let cwd = d.home.path().join("tom-dog-cwd");
    std::fs::create_dir_all(&cwd).unwrap();
    let cwd = cwd.to_string_lossy().into_owned();
    let a_run = json_out(
        &cli_ok(
            &d,
            &run_args(chr, &g.world_w1, &g.bind_a_w1, &["--cwd", &cwd]),
        )
        .await,
    );
    let a_id = a_run["session"]["session_id"].as_str().unwrap();
    assert_eq!(
        host.execs.load(Ordering::SeqCst),
        before_execs + 1,
        "one host prompt for A"
    );
    let a_prompt = host.last_prompt();
    assert!(a_prompt.contains(TOM_SOUL_MARKER), "SOUL: {a_prompt}");
    assert!(a_prompt.contains(TOM_MEM_MARKER), "memory: {a_prompt}");
    assert!(a_prompt.contains(TOM_L1_MARKER), "L1: {a_prompt}");
    assert!(a_prompt.contains(TOM_L2_MARKER), "L2: {a_prompt}");
    assert!(
        a_prompt.contains(TOM_W1_BINDING_MARKER),
        "binding tom: {a_prompt}"
    );
    assert!(a_prompt.contains("## Character ToM — L1"));
    assert!(a_prompt.contains("## Character ToM — L2"));
    assert!(a_prompt.contains(NAME_W1_PUBLIC));
    assert!(a_prompt.contains(NAME_A_SHARE));
    assert!(!a_prompt.contains(NAME_W1_SECRET));
    assert!(!a_prompt.contains(NAME_B_SHARE));
    assert!(!a_prompt.contains("## Personality"));

    let b_run = json_out(
        &cli_ok(
            &d,
            &run_args(&g.character_b, &g.world_w1, &g.bind_b_w1, &["--cwd", &cwd]),
        )
        .await,
    );
    let b_id = b_run["session"]["session_id"].as_str().unwrap();
    assert_ne!(a_id, b_id, "Character session isolation");
    assert_eq!(host.execs.load(Ordering::SeqCst), before_execs + 2);
    let b_prompt = host.last_prompt();
    assert!(
        !b_prompt.contains(TOM_L1_MARKER),
        "B prompt must not include A ToM"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn restored_character_run_mints_fresh_session_after_archive() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;

    let pre = json_out(
        &d.cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &[]))
            .await,
    );
    assert!(pre.get("session").is_some());
    let pre_session = pre["session"]["session_id"].as_str().unwrap().to_string();

    let show = json_out(
        &d.cli(&["creator", "character", "show", &g.character_a, "--json"])
            .await,
    );
    let revision = show["character"]["revision"].as_i64().unwrap();

    assert!(d
        .cli(&[
            "creator",
            "character",
            "archive",
            &g.character_a,
            "--expected-revision",
            &revision.to_string(),
            "--json",
        ])
        .await
        .status
        .success());

    let archived_show = json_out(
        &d.cli(&["creator", "character", "show", &g.character_a, "--json"])
            .await,
    );
    let archived_revision = archived_show["character"]["revision"].as_i64().unwrap();

    let denied = d
        .cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &[]))
        .await;
    assert!(!denied.status.success(), "archived run must fail");
    assert!(stderr(&denied).contains("character_inactive"));

    assert!(d
        .cli(&[
            "creator",
            "character",
            "restore",
            &g.character_a,
            "--expected-revision",
            &archived_revision.to_string(),
            "--json",
        ])
        .await
        .status
        .success());

    let post = json_out(
        &d.cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &[]))
            .await,
    );
    let post_session = post["session"]["session_id"].as_str().unwrap();
    assert_ne!(
        pre_session, post_session,
        "restore must mint a fresh session"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_includes_edited_summary_and_shared_ke_across_worlds() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;

    let edited = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "edit",
            "--character-id",
            &g.character_a,
            "--entry-id",
            &g.ke_a_share,
            "--expected-revision",
            "0",
            "--summary",
            "AShare edited summary for MCA",
            "--json",
        ])
        .await;
    assert!(edited.status.success(), "edit share: {}", stderr(&edited));

    let w1 = d
        .cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &[]))
        .await;
    assert!(w1.status.success(), "w1 run: {}", stderr(&w1));
    let prompt_w1 = host.last_prompt();
    assert!(prompt_w1.contains("AShare edited summary for MCA"));
    assert!(prompt_w1.contains(NAME_A_SHARE));
    assert!(!prompt_w1.contains(NAME_B_SHARE));

    let w2 = d
        .cli(&run_args(&g.character_a, &g.world_w2, &g.bind_a_w2, &[]))
        .await;
    assert!(w2.status.success(), "w2 run: {}", stderr(&w2));
    let prompt_w2 = host.last_prompt();
    assert!(prompt_w2.contains("AShare edited summary for MCA"));
    assert!(prompt_w2.contains(NAME_A_SHARE));
    assert!(!prompt_w2.contains(NAME_B_SHARE));

    let shown = d
        .cli(&[
            "creator",
            "character",
            "knowledge",
            "show",
            "--character-id",
            &g.character_a,
            "--entry-id",
            &g.ke_a_share,
            "--json",
        ])
        .await;
    assert!(shown.status.success(), "show shared ke: {}", stderr(&shown));
    let detail: Value = json_out(&shown);
    assert_eq!(detail["item"]["entry_id"], g.ke_a_share);
    assert_eq!(detail["summary"], "AShare edited summary for MCA");
}

// ─── v1.185 P3 run observation + --remember ─────────────────────────────────

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum P3RunScript {
    Standard = 0,
    WrongOpTerminalFirst = 1,
    MaxTokens = 2,
    MissedSseTerminal = 3,
}

struct P3MockHost {
    inner: MockHost,
    script: std::sync::atomic::AtomicU8,
}

impl P3MockHost {
    fn new(script: P3RunScript) -> Arc<Self> {
        Arc::new(Self {
            inner: MockHost {
                sessions: Mutex::new(HashMap::new()),
                prompts: Mutex::new(Vec::new()),
                last_create_metadata: Mutex::new(None),
                create_sessions: AtomicU64::new(0),
                execs: AtomicU64::new(0),
                events: tokio::sync::broadcast::channel(64).0,
            },
            script: std::sync::atomic::AtomicU8::new(script as u8),
        })
    }
}

#[async_trait::async_trait]
impl HostFacade for P3MockHost {
    async fn start(&self, config: HostStartConfig) -> HostResult<()> {
        self.inner.start(config).await
    }

    async fn create_session(
        &self,
        request: nexus_agent_host::capability::CreateSessionRequest,
    ) -> HostResult<HostSession> {
        self.inner.create_session(request).await
    }

    #[allow(
        clippy::redundant_clone,
        clippy::if_not_else,
        clippy::branches_sharing_code
    )]
    async fn exec(
        &self,
        session_id: HostSessionId,
        op: HostOperation,
    ) -> HostResult<HostEventStream> {
        self.inner.execs.fetch_add(1, Ordering::SeqCst);
        let script = self.script.load(Ordering::SeqCst);
        let op_id = match op {
            HostOperation::Prompt { op_id, content, .. } => {
                let text = match content.as_slice() {
                    [HostContentBlock::Text { text }] => text.clone(),
                    other => format!("unexpected content {other:?}"),
                };
                self.inner.prompts.lock().expect("prompts").push(text);
                op_id
            }
            HostOperation::SetModel { model } => {
                self.inner
                    .prompts
                    .lock()
                    .expect("prompts")
                    .push(format!("set-model:{model}"));
                HostOperationId::new()
            }
            HostOperation::SetMode { mode } => {
                self.inner
                    .prompts
                    .lock()
                    .expect("prompts")
                    .push(format!("set-mode:{mode}"));
                HostOperationId::new()
            }
        };
        let reason = if script == P3RunScript::MaxTokens as u8 {
            FinishReason::MaxTokens
        } else {
            FinishReason::EndTurn
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
            session_id: session_id.clone(),
            op_id: op_id.clone(),
            reason,
        });
        let mut stream_events = vec![Ok(started.clone()), Ok(delta.clone())];
        if script == P3RunScript::WrongOpTerminalFirst as u8 {
            let wrong = HostOperationId::new();
            let wrong_finished = HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session_id.clone(),
                op_id: wrong,
                reason: FinishReason::EndTurn,
            });
            let _ = self.inner.events.send(wrong_finished.clone());
            stream_events.push(Ok(wrong_finished));
        }
        stream_events.push(Ok(finished.clone()));
        let _ = self.inner.events.send(started.clone());
        let _ = self.inner.events.send(delta.clone());
        if script != P3RunScript::MissedSseTerminal as u8 {
            let _ = self.inner.events.send(finished);
        }
        Ok(Box::pin(futures_util::stream::iter(stream_events)))
    }

    async fn cancel(&self, op_id: HostOperationId) -> HostResult<()> {
        self.inner.cancel(op_id).await
    }

    async fn health(&self) -> HostResult<HostHealth> {
        self.inner.health().await
    }

    async fn shutdown(&self) -> HostResult<()> {
        self.inner.shutdown().await
    }

    async fn shutdown_session(&self, session_id: HostSessionId) -> HostResult<()> {
        self.inner.shutdown_session(session_id).await
    }

    async fn list_sessions(&self) -> HostResult<Vec<HostSession>> {
        self.inner.list_sessions().await
    }

    async fn provider_catalog(&self) -> HostResult<ProviderCatalog> {
        self.inner.provider_catalog().await
    }

    fn subscribe_events(
        &self,
        session_id: HostSessionId,
    ) -> tokio::sync::broadcast::Receiver<HostEvent> {
        self.inner.subscribe_events(session_id)
    }
}

async fn cli_run_fast_grace(d: &LiveDaemon, args: &[&str]) -> Output {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_nexus42"))
        .args(args)
        .env("HOME", d.home.path())
        .env("RUST_LOG", "off")
        .env("NEXUS42_RUN_OBSERVATION_GRACE_SECS", "1")
        .output()
        .await
        .expect("spawn nexus42")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_remember_captures_pending_json() {
    let host = P3MockHost::new(P3RunScript::Standard);
    let d = LiveDaemon::start_with_agent_host(host).await;
    let g = seed(&d).await;

    let out = cli_run_fast_grace(
        &d,
        &run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &["--remember"]),
    )
    .await;
    assert!(out.status.success(), "remember run: {}", stderr(&out));
    let payload = json_out(&out);
    assert_eq!(payload["result"], MOCK_RESULT);
    assert_eq!(payload["outcome"]["run_status"], "succeeded");
    assert_eq!(payload["outcome"]["capture"]["status"], "captured");
    assert!(payload["outcome"]["capture"]["pending_id"]
        .as_str()
        .unwrap()
        .starts_with("run_"));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_opt_out_capture_disabled() {
    let host = P3MockHost::new(P3RunScript::Standard);
    let d = LiveDaemon::start_with_agent_host(host).await;
    let g = seed(&d).await;

    let out = d
        .cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &[]))
        .await;
    assert!(out.status.success(), "default run: {}", stderr(&out));
    let payload = json_out(&out);
    assert_eq!(payload["outcome"]["capture"]["status"], "disabled");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_wrong_op_terminal_ignored() {
    let host = P3MockHost::new(P3RunScript::WrongOpTerminalFirst);
    let d = LiveDaemon::start_with_agent_host(host).await;
    let g = seed(&d).await;

    let out = d
        .cli(&run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &[]))
        .await;
    assert!(out.status.success(), "wrong-op run: {}", stderr(&out));
    let payload = json_out(&out);
    assert_eq!(payload["result"], MOCK_RESULT);
    assert_eq!(payload["outcome"]["run_status"], "succeeded");
    let op_id = payload["operation"]["operation_id"].as_str().unwrap();
    let session_id = payload["session"]["session_id"].as_str().unwrap();
    for event in payload["events"].as_array().unwrap() {
        let sid = event
            .as_object()
            .and_then(|o| o.values().next())
            .and_then(|v| v.get("session_id"))
            .and_then(|v| v.as_str());
        let oid = event
            .as_object()
            .and_then(|o| o.values().next())
            .and_then(|v| v.get("op_id"))
            .and_then(|v| v.as_str());
        assert_eq!(sid, Some(session_id));
        assert_eq!(oid, Some(op_id));
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_max_tokens_remember_nonzero() {
    let host = P3MockHost::new(P3RunScript::MaxTokens);
    let d = LiveDaemon::start_with_agent_host(host).await;
    let g = seed(&d).await;

    let out = cli_run_fast_grace(
        &d,
        &run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &["--remember"]),
    )
    .await;
    assert!(!out.status.success(), "max_tokens remember must fail exit");
    let payload = json_out(&out);
    assert_eq!(payload["result"], MOCK_RESULT);
    assert_eq!(payload["outcome"]["run_status"], "incomplete");
    assert_eq!(payload["outcome"]["capture"]["status"], "skipped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_missed_sse_preserves_outcome() {
    let host = P3MockHost::new(P3RunScript::MissedSseTerminal);
    let d = LiveDaemon::start_with_agent_host(host).await;
    let g = seed(&d).await;

    let out = cli_run_fast_grace(
        &d,
        &run_args(&g.character_a, &g.world_w1, &g.bind_a_w1, &["--remember"]),
    )
    .await;
    assert!(!out.status.success(), "missed sse: {}", stderr(&out));
    let payload = json_out(&out);
    assert_eq!(payload["result"], MOCK_RESULT);
    assert_eq!(payload["outcome"]["capture"]["status"], "captured");
    assert_eq!(payload["output_observation"], "incomplete");
}
