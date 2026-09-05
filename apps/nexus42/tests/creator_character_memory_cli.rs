//! Process-level `nexus42 creator character memory|soul` and the P3
//! SOUL/Memory slot projection through `creator character run` (v1.184 P3 T3).
//!
//! Uses the live daemon router + deterministic mock host. Asserts:
//! - deterministic human output and `--json` DTO parity for the full
//!   capture → review → promote → reflect lifecycle;
//! - fail-closed denial (foreign/missing/inactive Character, foreign binding)
//!   with zero memory mutation and zero host launches;
//! - `character run` fills only the admitted Character SOUL/Memory slots with
//!   the executing Character's bounded data (shared + selected binding scope).

mod common;

use common::rn_act4::{seed, stderr, stdout};
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

/// Deterministic marker strings seeded through public operations only.
const SOUL_MARKER: &str = "AVASOULMARKER keeps a ledger of every debt owed to the river.";
const SHARED_MEMORY_MARKER: &str = "SHAREDMEMORYMARKER the harbor accord holds because Ava keeps it";
const LOCAL_MEMORY_MARKER: &str = "LOCALMEMORYMARKER only W1 saw the lantern signal at dusk";

struct MockHost {
    sessions: Mutex<HashMap<HostSessionId, HostSession>>,
    prompts: Mutex<Vec<String>>,
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
            HostOperation::SetModel { .. } | HostOperation::SetMode { .. } => {
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

async fn cli_ok(d: &LiveDaemon, args: &[&str]) -> Output {
    let out = d.cli(args).await;
    assert!(out.status.success(), "cli {args:?}: {}", stderr(&out));
    out
}

/// >= 50 chars with research task kind → FragmentOnly.
fn fragment_digest(marker: &str) -> String {
    format!("{marker} — researched background detail for texture and continuity.")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_memory_lifecycle_json_and_human_parity() {
    let d = LiveDaemon::start().await;
    let g = seed(&d).await;
    let chr = g.character_a.as_str();

    // capture (json): returns the generated response DTO.
    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "capture",
            "--character-id",
            chr,
            "--pending-id",
            "pend_cli_1",
            "--session-id",
            "sess_cli_1",
            "--task-kind",
            "research",
            "--digest",
            &fragment_digest(SHARED_MEMORY_MARKER),
            "--json",
        ],
    )
    .await;
    let payload = json_out(&out);
    assert_eq!(payload["success"], true);
    assert_eq!(payload["pending_id"], "pend_cli_1");

    // capture (human): deterministic labeled lines, not JSON.
    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "capture",
            "--character-id",
            chr,
            "--pending-id",
            "pend_cli_2",
            "--session-id",
            "sess_cli_2",
            "--binding-id",
            &g.bind_a_w1,
            "--task-kind",
            "research",
            "--digest",
            &fragment_digest(LOCAL_MEMORY_MARKER),
        ],
    )
    .await;
    let human = stdout(&out);
    assert!(human.contains("pend_cli_2"));
    assert!(!human.trim_start().starts_with('{'));

    // pending-count, both scopes, both output modes.
    let out = cli_ok(
        &d,
        &[
            "creator", "character", "memory", "pending-count", "--character-id", chr, "--json",
        ],
    )
    .await;
    assert_eq!(json_out(&out)["count"], 1);
    let out = cli_ok(
        &d,
        &[
            "creator", "character", "memory", "pending-count", "--character-id", chr,
        ],
    )
    .await;
    assert!(stdout(&out).contains('1'));
    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "pending-count",
            "--character-id",
            chr,
            "--binding-id",
            &g.bind_a_w1,
            "--json",
        ],
    )
    .await;
    assert_eq!(json_out(&out)["count"], 1);

    // pending-list: shared scope shows pend_cli_1 only.
    let out = cli_ok(
        &d,
        &[
            "creator", "character", "memory", "pending-list", "--character-id", chr, "--json",
        ],
    )
    .await;
    let payload = json_out(&out);
    let items = payload["items"].as_array().unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0]["pending_id"], "pend_cli_1");

    // review (json): drains both scopes' rows when unscoped? No — unscoped
    // drains the shared scope only; the binding scope is drained explicitly.
    let out = cli_ok(
        &d,
        &[
            "creator", "character", "memory", "review", "--character-id", chr, "--json",
        ],
    )
    .await;
    let payload = json_out(&out);
    assert_eq!(payload["fragmented"], 1);
    assert_eq!(payload["has_more"], false);

    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "review",
            "--character-id",
            chr,
            "--binding-id",
            &g.bind_a_w1,
        ],
    )
    .await;
    assert!(stdout(&out).contains("fragmented=1"));

    // fragments: binding-local row carries revision 0 and the marker.
    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "fragments",
            "--character-id",
            chr,
            "--binding-id",
            &g.bind_a_w1,
            "--json",
        ],
    )
    .await;
    let payload = json_out(&out);
    let fragments = payload["fragments"].as_array().unwrap();
    assert_eq!(fragments.len(), 1);
    assert_eq!(fragments[0]["revision"], 0);
    assert!(fragments[0]["summary"]
        .as_str()
        .unwrap()
        .contains(LOCAL_MEMORY_MARKER));
    let fragment_id = fragments[0]["fragment_id"].as_str().unwrap().to_string();

    // promote: stale revision is a stable failure with no mutation.
    let out = d
        .cli(&[
            "creator",
            "character",
            "memory",
            "promote",
            "--character-id",
            chr,
            "--fragment-id",
            &fragment_id,
            "--expected-revision",
            "9",
        ])
        .await;
    assert!(!out.status.success(), "stale promote: {}", stdout(&out));
    assert!(
        stderr(&out).contains("version_mismatch"),
        "stale promote stderr: {}",
        stderr(&out)
    );

    // promote: correct revision clears provenance (shared scope gains it).
    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "promote",
            "--character-id",
            chr,
            "--fragment-id",
            &fragment_id,
            "--expected-revision",
            "0",
            "--json",
        ],
    )
    .await;
    let payload = json_out(&out);
    assert_eq!(payload["fragment"]["fragment_id"], fragment_id);
    assert_eq!(payload["fragment"]["revision"], 1);

    // soul reflect: deterministic states in both output modes.
    let out = cli_ok(
        &d,
        &[
            "creator", "character", "soul", "reflect", "--character-id", chr, "--json",
        ],
    )
    .await;
    let payload = json_out(&out);
    assert_eq!(payload["character_id"], chr);
    assert_eq!(payload["state"], "insufficient_data");
    let out = cli_ok(
        &d,
        &["creator", "character", "soul", "reflect", "--character-id", chr],
    )
    .await;
    let human = stdout(&out);
    assert!(human.contains("insufficient_data"));
    assert!(!human.trim_start().starts_with('{'));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_memory_fail_closed_no_mutation() {
    let d = LiveDaemon::start().await;
    let g = seed(&d).await;

    // Foreign (missing) character id: every memory verb fails.
    let missing = "chr_ffffffffffffffffffffffffffffffff";
    for args in [
        vec![
            "creator", "character", "memory", "capture", "--character-id", missing,
            "--pending-id", "pend_x", "--session-id", "sess_x", "--digest", "irrelevant digest text",
        ],
        vec!["creator", "character", "memory", "pending-count", "--character-id", missing],
        vec!["creator", "character", "memory", "review", "--character-id", missing],
        vec!["creator", "character", "soul", "reflect", "--character-id", missing],
    ] {
        let out = d.cli(&args).await;
        assert!(
            !out.status.success(),
            "{args:?} must fail: {}",
            stdout(&out)
        );
    }

    // Cross-character binding: A's memory verbs must not accept B's binding.
    let out = d
        .cli(&[
            "creator",
            "character",
            "memory",
            "capture",
            "--character-id",
            &g.character_a,
            "--pending-id",
            "pend_xb",
            "--session-id",
            "sess_xb",
            "--binding-id",
            &g.bind_b_w1,
            "--digest",
            "irrelevant digest text that is long enough to matter",
        ])
        .await;
    assert!(!out.status.success(), "cross-character binding accepted");

    // Inactive character: archive B, then every verb denies.
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(&g.character_b)
        .execute(&d.pool)
        .await
        .unwrap();
    let out = d
        .cli(&[
            "creator",
            "character",
            "memory",
            "pending-count",
            "--character-id",
            &g.character_b,
        ])
        .await;
    assert!(!out.status.success(), "inactive character must deny");

    // Zero mutation proof: A's queues stay empty, B has no rows at all.
    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "memory",
            "pending-count",
            "--character-id",
            &g.character_a,
            "--json",
        ],
    )
    .await;
    assert_eq!(json_out(&out)["count"], 0);
    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM character_memory_pending_review")
        .fetch_one(&d.pool)
        .await
        .unwrap();
    assert_eq!(count.0, 0, "deny matrix must not write pending rows");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn character_run_projects_only_admitted_soul_and_memory() {
    let host = MockHost::new();
    let d = LiveDaemon::start_with_agent_host(host.clone()).await;
    let g = seed(&d).await;
    let chr = g.character_a.as_str();

    // Seed SOUL.md at the canonical Character path inside the hermetic home.
    // The daemon passes `state.nexus_home()` (= `<raw home>/.nexus42`) to the
    // bearer layout helpers, which also join `.nexus42` — so the SOUL.md the
    // daemon reads lives under `<raw home>/.nexus42/.nexus42/creators/...`.
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
        format!("# Ava\n\n{SOUL_MARKER}\n"),
    )
    .unwrap();

    // One shared fragment and one W1-binding-local fragment via public CLI.
    for (pending, binding, marker) in [
        ("pend_shared", None, SHARED_MEMORY_MARKER),
        ("pend_local", Some(g.bind_a_w1.as_str()), LOCAL_MEMORY_MARKER),
    ] {
        let session_id = format!("sess_{pending}");
        let digest = fragment_digest(marker);
        let mut args = vec![
            "creator",
            "character",
            "memory",
            "capture",
            "--character-id",
            chr,
            "--pending-id",
            pending,
            "--session-id",
            &session_id,
            "--task-kind",
            "research",
            "--digest",
            &digest,
        ];
        if let Some(b) = binding {
            args.push("--binding-id");
            args.push(b);
        }
        cli_ok(&d, &args).await;
    }
    cli_ok(
        &d,
        &["creator", "character", "memory", "review", "--character-id", chr],
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
            "--binding-id",
            &g.bind_a_w1,
        ],
    )
    .await;

    // Run A in W1: SOUL + shared + W1-local memory fill the slots.
    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "run",
            "--character-id",
            chr,
            "--world-id",
            &g.world_w1,
            "--binding-id",
            &g.bind_a_w1,
            "--prompt",
            "Act now.",
        ],
    )
    .await;
    assert!(stdout(&out).contains(MOCK_RESULT));
    let prompt = host.last_prompt();
    assert!(prompt.contains("## Character SOUL"), "{prompt}");
    assert!(prompt.contains(SOUL_MARKER), "soul slot: {prompt}");
    assert!(prompt.contains("## Character Memory"), "{prompt}");
    assert!(prompt.contains(SHARED_MEMORY_MARKER), "shared: {prompt}");
    assert!(prompt.contains(LOCAL_MEMORY_MARKER), "w1 local: {prompt}");
    assert!(prompt.contains("## Character ToM — L1"));
    assert!(prompt.contains("## Character ToM — L2"));

    // Run A in W2: shared memory visible, W1-local memory absent.
    let out = cli_ok(
        &d,
        &[
            "creator",
            "character",
            "run",
            "--character-id",
            chr,
            "--world-id",
            &g.world_w2,
            "--binding-id",
            &g.bind_a_w2,
            "--prompt",
            "Act now.",
        ],
    )
    .await;
    assert!(out.status.success(), "w2 run: {}", stderr(&out));
    let prompt = host.last_prompt();
    assert!(prompt.contains(SOUL_MARKER), "soul persists: {prompt}");
    assert!(prompt.contains(SHARED_MEMORY_MARKER), "shared: {prompt}");
    assert!(
        !prompt.contains(LOCAL_MEMORY_MARKER),
        "w1-local must not leak into w2 run: {prompt}"
    );

    // No Creator/B data bleeds into the Character slots.
    assert!(!prompt.contains("## Personality"));

    // Inactive Character run: rejected before any launch/mutation.
    sqlx::query("UPDATE characters SET status = 'archived' WHERE character_id = ?")
        .bind(&g.character_b)
        .execute(&d.pool)
        .await
        .unwrap();
    let before_create = host.create_sessions.load(Ordering::SeqCst);
    let before_exec = host.execs.load(Ordering::SeqCst);
    let out = d
        .cli(&[
            "creator",
            "character",
            "run",
            "--character-id",
            &g.character_b,
            "--world-id",
            &g.world_w1,
            "--binding-id",
            &g.bind_b_w1,
            "--prompt",
            "Act now.",
        ])
        .await;
    assert!(!out.status.success(), "inactive run must fail");
    assert_eq!(host.create_sessions.load(Ordering::SeqCst), before_create);
    assert_eq!(host.execs.load(Ordering::SeqCst), before_exec);
}
