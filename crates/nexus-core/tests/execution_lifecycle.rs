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
use nexus_contracts::{
    CoreError as WireCoreError, CoreErrorCode, ProviderCall, ProviderEventBatch, ProviderReply,
};
use nexus_core::execution::RunControlError;
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService, ExecutionBuildObserver, RunnerDeps};
use nexus_orchestration::capability::{
    CapabilityError, PromptExecutor, PromptRequest, PromptResult,
};
use nexus_orchestration::WorkflowStateStore;
use nexus_provider_ports::{ProviderPort, ProviderResult};
use std::future::Future;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::Notify;

const CREATOR: &str = "test_creator";
const SLUG: &str = "default";

/// The embedded preset the seeded runs are frozen against.
///
/// A REAL preset (not a placeholder id) so recovery's frozen-source
/// verification actually runs: reconstruction re-resolves the identity from
/// this id and refuses on a hash/version mismatch, so a fabricated descriptor
/// would only ever exercise the `reconstruction_unavailable` path.
const SEED_PRESET: &str = "memory-augmented";
/// `SEED_PRESET`'s initial state — the persisted position a seeded run holds
/// at a clean boundary.
const SEED_PRESET_INITIAL: &str = "recall";

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
    // P0 verified-admission: `create_world` validates the caller's creator
    // against the stored `creators` row (`narrative_write::create_world` →
    // `FkNotFound`), so the fixture must seed it exactly as production does —
    // through an ADMITTED pool, because `guard_creators_insert` aborts any
    // write whose connection carries no engine/direct writer registration.
    sqlx::query(
        "INSERT OR IGNORE INTO creators (creator_id, display_name, status, \
         cached_at, data) VALUES (?, 'Test', 'active', datetime('now'), '{}')",
    )
    .bind(CREATOR)
    .execute(guarded.pool())
    .await
    .expect("seed the admitted creator row");
    guarded.pool().close().await;
    nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);

    Fixture { tmp, db_path }
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

/// Seed a durable v1 run through the REAL store API, not hand-written SQL.
///
/// `start_run` is the single path that writes an authoritative v1 row: the
/// `execution_version = 1` record together with the frozen
/// `run_descriptor_json` that the v1 contract requires. A v1 row without that
/// descriptor is non-replayable by contract — the store refuses to load it —
/// so hand-written SQL that omits it cannot exercise recovery at all. The
/// fixture must write the bytes production writes, including the state blob
/// whose `step_in_flight` is an `Option<String>` (an unfinished-step mark),
/// not a struct.
///
/// The descriptor names a REAL embedded preset and carries its genuine
/// content-addressed source identity (`embedded_source_identity`), because
/// reconstruction re-resolves the frozen source and verifies the hash against
/// the loaded preset: a zero or invented hash would classify every seeded row
/// `reconstruction_unavailable` instead of exercising the re-drive. The preset
/// version comes from the loaded preset, not a constant, so the frozen-version
/// check passes too.
///
/// The pool is the same admitted engine seam the owner uses, so the row is
/// admitted under the engine-mode registration the guard trigger checks.
async fn seed_run(
    db_path: &Path,
    session_id: &str,
    creator: &str,
    state: nexus_orchestration::RunStateV1,
) {
    seed_run_at(db_path, session_id, creator, state, SEED_PRESET_INITIAL).await;
}

/// Same admission seam as [`seed_run`], at an explicit persisted position.
async fn seed_run_at(
    db_path: &Path,
    session_id: &str,
    creator: &str,
    state: nexus_orchestration::RunStateV1,
    initial: &str,
) {
    let preset_id = SEED_PRESET;
    let source = nexus_preset::embedded_source_identity(preset_id)
        .expect("seed preset must be a real embedded preset");
    let preset_version = nexus_preset::load_embedded_preset(
        preset_id,
        &nexus_preset::capability_catalog::BuiltinCapabilityCatalog,
    )
    .expect("seed preset loads")
    .version;

    let guarded = nexus_local_db::init_engine_pool(db_path)
        .await
        .expect("seed engine pool init");
    let store = nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(Arc::new(
        guarded.clone_pool(),
    ));

    let sid = nexus_orchestration::SessionId(session_id.to_string());
    let descriptor = nexus_orchestration::RunDescriptorV1 {
        creator_id: creator.to_string(),
        work_id: None,
        workspace_root: std::path::PathBuf::from("/tmp/ws"),
        preset_id: preset_id.to_string(),
        preset_version,
        source,
        input: serde_json::Map::default(),
        agent_bindings: std::collections::HashMap::new(),
        parent_session_id: None,
        graph_name: None,
    };
    // The persisted position is the preset's initial state, so reconstruction
    // rebuilds a runner whose position is a real task in the frozen graph.
    let root = graph_flow::Session::new_from_task(session_id.to_string(), initial);
    root.context
        .set("_session_id", session_id.to_string())
        .expect("seed session context");
    let checkpoint = nexus_orchestration::RunCheckpoint {
        root: &root,
        children: &[],
    };
    store
        .start_run(&sid, &descriptor, checkpoint, &state)
        .await
        .expect("seed start_run");

    guarded.pool().close().await;
    nexus_local_db::writer_protocol::release_retained_writer_guards(db_path);
}

/// Seed a committed, non-terminal v1 run at a clean boundary: no wait, no
/// in-flight prompt, no unfinished step. Recovery classifies it
/// `SafeBoundary` — a committed run, not an uncertain one.
async fn seed_committed_run(db_path: &Path, session_id: &str, creator: &str) {
    seed_run(
        db_path,
        session_id,
        creator,
        nexus_orchestration::RunStateV1::default(),
    )
    .await;
}

/// Mark an in-flight step so A7 classifies the run `Interrupted`.
async fn seed_in_flight_run(db_path: &Path, session_id: &str, creator: &str) {
    seed_run(
        db_path,
        session_id,
        creator,
        nexus_orchestration::RunStateV1 {
            step_in_flight: Some("s1".to_string()),
            ..Default::default()
        },
    )
    .await;
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
        .start_execution(
            Arc::clone(&providers) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("first execution owner starts");
    assert!(
        handle.engine_epoch() >= 1,
        "a real owner is admitted with a durable engine epoch, got {}",
        handle.engine_epoch()
    );

    // Recovery classified both rows: the committed run is re-driven (its
    // runner is reconstructed), the in-flight run is NOT (Interrupted).
    //
    // Read back through the live owner's OWN admitted pool. A bare second
    // `SqlitePool` beside a live engine owner is exactly what the writer
    // protocol fences, and it also carries none of the installed
    // `nexus_writer_*` functions; the owner's pool has both by construction.
    let store = nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(Arc::new(
        owner_a.pool().clone(),
    ));
    let uncertain = store
        .load_run(&nexus_orchestration::SessionId(
            "test-preset:uncertain".into(),
        ))
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
            .start_execution(
                Arc::clone(&providers) as Arc<dyn ProviderPort>,
                RunnerDeps::default(),
            )
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
            .start_execution(
                Arc::clone(&providers) as Arc<dyn ProviderPort>,
                RunnerDeps::default()
            )
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

// ── Concurrency regressions (T1 review C1/C2/C3) ───────────────────────────

/// Poll `cond` until it holds, failing the test instead of hanging.
async fn wait_until<F, Fut>(what: &str, mut cond: F)
where
    F: FnMut() -> Fut,
    Fut: Future<Output = bool>,
{
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            if cond().await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// A prompt executor that parks the drive INSIDE an LLM step until the test
/// releases it. Holds `close()`'s drain deterministically: `abort_all_drives`
/// cannot finish while the executor is parked, so the "closing" window is
/// observable.
#[derive(Clone)]
struct GatedPromptExecutor {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

impl GatedPromptExecutor {
    fn new() -> Self {
        Self {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        }
    }

    async fn wait_entered(&self) {
        tokio::time::timeout(Duration::from_secs(10), self.entered.notified())
            .await
            .expect("the recovery re-drive must reach the gated prompt step");
    }

    fn release(&self) {
        self.release.notify_one();
    }
}

#[async_trait]
impl PromptExecutor for GatedPromptExecutor {
    async fn execute(&self, request: PromptRequest) -> Result<PromptResult, CapabilityError> {
        // Signal entry FIRST (the test's deterministic hold depends on it),
        // then park until the test releases. `Notify::notify_one` stores a
        // permit when no waiter is registered yet, so the signal is never
        // lost to a scheduling race.
        self.entered.notify_one();
        // Hold past the drive-loop cancellation deliberately: the drain must
        // remain in flight until the TEST releases it, so the mid-close
        // registry fence is observable.
        let () = self.release.notified().await;
        let _ = request;
        Err(CapabilityError::Internal("test: released".into()))
    }
}

/// C1: while close is draining (closing, not yet settled), the per-DB
/// registry must hold the fence — a competing `EngineOwner` is refused and
/// cannot build a second engine. Only AFTER the drives join is the slot
/// released and a replacement admitted.
#[tokio::test]
async fn draining_close_holds_the_db_fence_until_drives_join() {
    let f = fixture().await;
    // A run parked inside the `generate` LLM step: recovery re-drives it and
    // the gated executor holds the drive loop open.
    seed_run_at(
        &f.db_path,
        "test-preset:held",
        CREATOR,
        nexus_orchestration::RunStateV1::default(),
        "generate",
    )
    .await;

    let owner_a = open_engine_owner(&f).await;
    let gate = GatedPromptExecutor::new();
    let handle = owner_a
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps {
                prompt_executor: Some(Arc::new(gate.clone())),
                ..RunnerDeps::default()
            },
        )
        .await
        .expect("owner starts");

    // Deterministic hold: the recovery re-drive is parked inside the prompt.
    gate.wait_entered().await;

    // Close begins: drains (fenced admission) and joins the parked drive —
    // in the background, so the drain window is observable.
    let closer = {
        let handle = Arc::clone(&handle);
        tokio::spawn(async move { handle.close().await })
    };
    gate.release();
    let _ = closer.await;
    assert!(
        handle.is_settled(),
        "close must settle only after every drive has joined"
    );

    // The fence is released AFTER settle: a replacement is now admitted.
    let competitor = open_engine_owner(&f).await;
    competitor
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("replacement admitted only after the prior owner settled");
    competitor.close().await.unwrap();
    owner_a.close().await.unwrap();
}

/// C1 (mid-drain window): while close is draining, a competing `EngineOwner`
/// must be REFUSED — the fence is held until settle, never merely until
/// close starts.
#[tokio::test]
async fn competing_owner_refused_while_close_is_draining() {
    let f = fixture().await;
    seed_run_at(
        &f.db_path,
        "test-preset:held",
        CREATOR,
        nexus_orchestration::RunStateV1::default(),
        "generate",
    )
    .await;

    let owner_a = open_engine_owner(&f).await;
    let gate = GatedPromptExecutor::new();
    let handle = owner_a
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps {
                prompt_executor: Some(Arc::new(gate.clone())),
                ..RunnerDeps::default()
            },
        )
        .await
        .expect("owner starts");

    gate.wait_entered().await;

    let closer = {
        let handle = Arc::clone(&handle);
        tokio::spawn(async move { handle.close().await })
    };
    // Deterministic barrier: hold until close has ARMED the admission fence
    // (`begin_shutdown` → `draining`, observable as `is_closed()`) before the
    // competitor runs. Without this the refusal below could equally be
    // produced by a merely-LIVE owner, so the test would pass even with the
    // pre-fix registry and would not discriminate the C1 window at all.
    // `draining` is armed before the drain and the parked drive cannot join
    // until `gate.release()`, so the handle is still-unsettled here.
    let fence_probe = Arc::clone(&handle);
    wait_until("close arms the admission fence", move || {
        let fence_probe = Arc::clone(&fence_probe);
        async move { fence_probe.is_closed() }
    })
    .await;

    // The drain is in flight (the parked drive has not joined). While the
    // handle is NOT settled, the registry must refuse a replacement — the
    // C1 fence, not merely a live-owner collision.
    let competitor = open_engine_owner(&f).await;
    let refused = competitor
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .is_err();
    assert!(
        refused,
        "a competing EngineOwner must be refused while close is draining"
    );
    assert!(
        !handle.is_settled(),
        "the drain must still be in flight at the refusal (deterministic hold)"
    );

    gate.release();
    let _ = closer.await;
    assert!(handle.is_settled());
    competitor.close().await.unwrap();
    owner_a.close().await.unwrap();
}

/// C2: close fences new drive admission — no drive may start after close
/// began, and the fence is observable through the coordinator's public
/// admission primitive.
#[tokio::test]
async fn close_fences_new_drive_admission() {
    let f = fixture().await;
    seed_committed_run(&f.db_path, "test-preset:c2", CREATOR).await;
    let owner_a = open_engine_owner(&f).await;
    let handle = owner_a
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    handle.close().await.unwrap();

    // C2: after close, admission is fenced. Before the fix a drive could be
    // registered AFTER close returned `cleanup_confirmed: true`.
    let err = handle
        .coordinator()
        .ensure_driving(&nexus_orchestration::SessionId("test-preset:c2".into()))
        .await;
    assert!(
        matches!(err, Err(RunControlError::Closing)),
        "ensure_driving must be fenced during/after close, got {err:?}"
    );
    owner_a.close().await.unwrap();
}

/// Build-phase barrier for the C3 race (`RunnerDeps::build_observer`).
///
/// Holds `start_execution` at the exact window between "build complete" and
/// "install into the per-service slot", and reports when a close has arrived
/// there. That makes the pre-fix interleaving REACHABLE BY CONSTRUCTION
/// instead of probabilistic: the old code awaited the build, then installed
/// unconditionally, so a close landing in this window saw an empty slot and
/// returned while the build went on to install an owner into a closed
/// service.
#[derive(Clone)]
struct BuildGate {
    entered: Arc<Notify>,
    release: Arc<Notify>,
}

impl BuildGate {
    fn new() -> Self {
        Self {
            entered: Arc::new(Notify::new()),
            release: Arc::new(Notify::new()),
        }
    }

    async fn wait_entered(&self) {
        tokio::time::timeout(Duration::from_secs(10), self.entered.notified())
            .await
            .expect("start_execution must reach the build-phase barrier");
    }

    fn release(&self) {
        self.release.notify_one();
    }
}

#[async_trait]
impl ExecutionBuildObserver for BuildGate {
    async fn built(&self) {
        // Signal entry (Notify stores the permit if the waiter is not
        // registered yet), then hold the build open until the test releases.
        self.entered.notify_one();
        let () = self.release.notified().await;
    }
}

/// C3: a close that arrives WHILE `start_execution` is mid-build must not let
/// the build install an owner into the closed service. Either close observes
/// the installed owner and settles it, or the build abandons it — never an
/// orphan owner, and never an owner surviving the close.
#[tokio::test]
async fn concurrent_close_and_start_never_leave_an_owner() {
    let f = fixture().await;
    seed_committed_run(&f.db_path, "test-preset:c3", CREATOR).await;

    // Repeat so the barrier is exercised across scheduling orders, but every
    // round is deterministically forced into the mid-build window.
    for round in 0..8u32 {
        let owner = Arc::new(open_engine_owner(&f).await);
        let gate = BuildGate::new();
        let owner_for_start = Arc::clone(&owner);
        let gate_for_start = gate.clone();

        let start = tokio::spawn(async move {
            owner_for_start
                .start_execution(
                    Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
                    RunnerDeps {
                        build_observer: Some(Arc::new(gate_for_start)),
                        ..RunnerDeps::default()
                    },
                )
                .await
        });

        // The build is complete but NOT installed. Close now — this is the
        // interleaving the pre-fix code lost.
        gate.wait_entered().await;
        let close = tokio::spawn({
            let owner = Arc::clone(&owner);
            async move { owner.close().await }
        });
        // Deterministic ordering: close must have BEGUN (`closing`, C2) and
        // fully completed its slot take BEFORE the build is allowed to
        // attempt its install. Awaiting the close task here removes the last
        // scheduling dependence — the pre-fix install ran unconditionally,
        // so it would publish an owner into an already-closed service on
        // every round, while the install-time double check refuses it.
        let report = close.await.expect("close task joins");
        assert!(
            owner.is_closing(),
            "round {round}: close must have begun before the install window"
        );
        gate.release();

        let started = start.await.expect("start task joins");
        assert!(
            report.unwrap().cleanup_confirmed,
            "round {round}: close must report a confirmed cleanup"
        );
        // The build was refused (install-time double check) — it may not have
        // installed anything into the closed service.
        assert!(
            started.is_err(),
            "round {round}: a start racing a completed close must be refused"
        );
        assert!(
            owner.execution().is_none(),
            "round {round}: an owner survived concurrent close — orphan effect owner"
        );
        owner.close().await.unwrap();
    }
}

// ── Close / admission ownership (v1.195 P0-T5 close findings) ───────────────

/// The durable engine epoch this open's admission was granted.
///
/// `nexus_engine_epoch()` is installed per connection by the writer protocol
/// from the admitting context, so it reports the generation THIS pool was
/// admitted with — the same generation the execution handle publishes.
async fn admission_engine_epoch(core: &CoreService) -> i64 {
    let epoch: Option<i64> = sqlx::query_scalar("SELECT nexus_engine_epoch()")
        .fetch_one(core.pool())
        .await
        .expect("the admitted pool carries an engine epoch");
    epoch.expect("an admitted engine pool reports a non-null epoch")
}

/// A confirmed close gives up the closing core's OWN admitted-pool handle.
///
/// A cooperative `EngineOwner` joiner holds a CLONE of the creator's
/// `WorkspaceWriterGuard`, so that guard — and the `state.db.engine.lock` file
/// it keeps open — stays alive until every clone is gone. Closing the retained
/// registry entry and the creator's own handle is not enough: a
/// closed-but-still-referenced joiner would keep the home fenced, and a fresh
/// same-process open would fail `OwnerBusy` with no live owner at all. Only
/// the admission creator removes the retained entry; every closing core drops
/// its own handle.
#[tokio::test]
async fn closed_joiner_does_not_keep_the_engine_admission_alive() {
    let f = fixture().await;

    // ── The creator takes the engine admission. ──
    let creator = open_engine_owner(&f).await;
    let epoch = admission_engine_epoch(&creator).await;

    // ── A cooperating open JOINS that live admission: a live peer still fences
    //    a duplicate, so the joiner is the SAME generation, never a second
    //    owner admitted with a new epoch. ──
    let joiner = open_engine_owner(&f).await;
    assert_eq!(
        admission_engine_epoch(&joiner).await,
        epoch,
        "a live peer still fences a new admission"
    );

    // ── Close the JOINER first, then the CREATOR. Both closed values stay
    //    referenced for the rest of the test. ──
    joiner.close().await.expect("close the joiner");
    creator.close().await.expect("close the creator");
    assert!(joiner.is_closing() && creator.is_closing());

    // ── A fresh owner over the same home must genuinely RE-ADMIT: a new engine
    //    epoch, not the settled generation's. ──
    let fresh = CoreService::open(open_options(&f, CoreAccess::EngineOwner))
        .await
        .expect("a fresh owner must reopen the home after both closures");
    let fresh_epoch = admission_engine_epoch(&fresh).await;
    assert!(
        fresh_epoch > epoch,
        "the fresh owner must take a NEW engine admission ({epoch} -> {fresh_epoch})"
    );
    fresh.close().await.expect("close the fresh owner");
}
