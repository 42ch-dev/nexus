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
use nexus_core::owner::ExecutionOpenError;
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService, ExecutionBuildObserver, RunnerDeps};
use nexus_execution::workflow::{RunControlError, RunSignal};
use nexus_orchestration::capability::{
    CapabilityError, PromptExecutor, PromptRequest, PromptResult,
};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use nexus_session_storage::run_state::WorkflowStateStore;
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
        memory: None,
    }
}

async fn open_engine_owner(f: &Fixture) -> CoreService {
    CoreService::open(open_options(f, CoreAccess::EngineOwner))
        .await
        .expect("engine-owner core open")
}

/// A fully retired owner releases both the home admission and the per-DB
/// execution reservation, allowing a new generation to start.
async fn assert_fresh_owner_can_start(f: &Fixture) {
    let fresh = open_engine_owner(f).await;
    fresh
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("the retired owner must not fence its DB");
    fresh.close().await.unwrap();
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
    state: nexus_session_storage::run_state::RunStateV1,
) {
    seed_run_at(db_path, session_id, creator, state, SEED_PRESET_INITIAL).await;
}

/// Same admission seam as [`seed_run`], at an explicit persisted position.
async fn seed_run_at(
    db_path: &Path,
    session_id: &str,
    creator: &str,
    state: nexus_session_storage::run_state::RunStateV1,
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
    let store = nexus_session_storage::storage::sqlite::SqliteSessionStorage::new(Arc::new(
        guarded.clone_pool(),
    ));

    let sid = nexus_orchestration::SessionId(session_id.to_string());
    let descriptor = nexus_session_storage::run_state::RunDescriptorV1 {
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
        applied: None,
        intake_mode: None,
    };
    // The persisted position is the preset's initial state, so reconstruction
    // rebuilds a runner whose position is a real task in the frozen graph.
    let root = graph_flow::Session::new_from_task(session_id.to_string(), initial);
    root.context
        .set("_session_id", session_id.to_string())
        .expect("seed session context");
    let checkpoint = nexus_session_storage::run_state::RunCheckpoint {
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
        nexus_session_storage::run_state::RunStateV1::default(),
    )
    .await;
}

/// Mark an in-flight step so A7 classifies the run `Interrupted`.
async fn seed_in_flight_run(db_path: &Path, session_id: &str, creator: &str) {
    seed_run(
        db_path,
        session_id,
        creator,
        nexus_session_storage::run_state::RunStateV1 {
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
    let store = nexus_session_storage::storage::sqlite::SqliteSessionStorage::new(Arc::new(
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
    assert_ne!(world.world_id, "");

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

/// R-V1186P3-T1-002 (A7 rule 6, P3 T1 rereview P1): the safe-boundary re-drive
/// regression must be driven by the real restart, never the coordinator seam
/// alone. The v1.186 case called `WorkflowRunCoordinator::recover_persisted`
/// directly on a live runtime; its legacy harness (`LiveDaemon` and the daemon
/// runtime it booted) was retired in v1.193, so the real restart is now
/// `CoreService::open(EngineOwner)` + `start_execution` — whose boot runs that
/// same production seam over the persisted DB.
///
/// A v1 row parked at a fully committed boundary (no wait, no in-flight
/// prompt, no unfinished step, no cancel request → `SafeBoundary`) must be
/// reconstructed from its frozen source identity and re-driven; the pre-fix
/// explicit safe-boundary skip stranded it. The seeded position is the
/// `generate` inner graph's `acp_prompt`, so the re-drive is observed as the
/// prompt executor being entered — a skip leaves the run stranded and the
/// wait times out.
#[tokio::test]
async fn daemon_restart_safe_boundary_is_reconstructed_and_redriven() {
    let f = fixture().await;
    seed_run_at(
        &f.db_path,
        "test-preset:safe-boundary",
        CREATOR,
        nexus_session_storage::run_state::RunStateV1::default(),
        "generate",
    )
    .await;

    let owner = open_engine_owner(&f).await;
    let gate = GatedPromptExecutor::new();
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps {
                prompt_executor: Some(Arc::new(gate.clone())),
                ..RunnerDeps::default()
            },
        )
        .await
        .expect("owner starts");

    // Deterministic hold: the safe-boundary run was reconstructed and
    // re-driven into its prompt step by the boot recovery.
    gate.wait_entered().await;
    gate.release();
    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Applied identity across a restart (`.mstar/plans/p4a-applied-identity.md`
// Task 2). The descriptor bundle and the applied document share the entry node
// and reach DIFFERENT nodes from it, so the recovered position — a node id —
// tells which graph the reconstructed runner holds.
// ─────────────────────────────────────────────────────────────────────────────

/// The user preset whose applied document the restart test installs.
const RESTART_PRESET: &str = "restart-applied-identity";
/// The same bundle with NO apply: the no-apply side of the same restart.
const RESTART_NO_APPLY_PRESET: &str = "restart-no-apply";
/// The entry node BOTH documents declare — the position the killed run held.
const SHARED_ENTRY_NODE: &str = "open_probe";
/// Node id that exists ONLY in the applied document.
const APPLIED_ONLY_NODE: &str = "applied_only_probe";
/// Node id that exists ONLY in the descriptor bundle.
const PRESET_ONLY_NODE: &str = "preset_only_probe";
/// Node id that exists ONLY in the superseding apply's document.
const SUPERSEDING_ONLY_NODE: &str = "superseding_only_probe";

/// The descriptor bundle: shared entry → the preset-only node, which is a
/// manual wait so the recovered position is durable instead of transient.
fn restart_preset_yaml(preset_id: &str) -> String {
    format!(
        r#"
preset:
  id: {preset_id}
  version: 1
  kind: creator
  description: "applied-identity restart fixture"
  requires_capabilities: []
  initial: {SHARED_ENTRY_NODE}
  terminal: done
states:
  - id: {SHARED_ENTRY_NODE}
    exit_when: {{ kind: rule }}
    next: {PRESET_ONLY_NODE}
  - id: {PRESET_ONLY_NODE}
    exit_when: {{ kind: manual }}
    next: done
  - id: done
    terminal: true
"#
    )
}

/// The applied document: the same entry node, and `probe` where the bundle has
/// `preset_only_probe`. No node id of the bundle's second state appears anywhere
/// in it.
fn restart_applied_document(preset_id: &str, probe: &str) -> String {
    format!(
        r#"{{
  "ir_version": 0,
  "id": "{preset_id}",
  "version": 1,
  "revision": 0,
  "state": "candidate",
  "meta": {{
    "description": "applied-identity restart fixture",
    "kind": "creator",
    "run_intents": [],
    "requires_capabilities": []
  }},
  "entry": "{SHARED_ENTRY_NODE}",
  "terminal": ["done"],
  "nodes": [
    {{"id": "{SHARED_ENTRY_NODE}", "kind": "outer_state", "effect_class": "pure",
     "exit_when": {{"kind": "rule"}}}},
    {{"id": "{probe}", "kind": "outer_state", "effect_class": "pure",
     "exit_when": {{"kind": "manual"}}}},
    {{"id": "done", "kind": "terminal", "effect_class": "pure"}}
  ],
  "edges": [
    {{"id": "{SHARED_ENTRY_NODE}->{probe}", "from": "{SHARED_ENTRY_NODE}",
     "to": "{probe}", "kind": "linear"}},
    {{"id": "{probe}->done", "from": "{probe}", "to": "done",
     "kind": "linear"}}
  ],
  "nests": [],
  "assets": [],
  "signals": [],
  "roles": [],
  "effect_summary": "pure",
  "provenance": {{"source": "compose"}}
}}"#
    )
}

/// The package the pinned restart fixtures nest, and the nest it is bound through.
const RESTART_PINNED_PACKAGE: &str = "restart-pinned-step";

/// [`restart_applied_document`] plus one exact pattern pin on the shared entry
/// node: the shape a real approval freezes when its document nests a pattern.
/// `pin_hash: None` is the hashless carrier — a nest whose body is decided at
/// Apply, so the document itself records nothing to compare.
fn restart_pinned_applied_document(preset_id: &str, pin_hash: Option<&str>) -> String {
    let mut document: serde_json::Value =
        serde_json::from_str(&restart_applied_document(preset_id, APPLIED_ONLY_NODE))
            .expect("the applied-side document decodes");
    document["nests"] = serde_json::json!([{
        "id": "restart-pinned-nest",
        "parent_node": SHARED_ENTRY_NODE,
        "expansion": "defer_until_apply",
        "pin_policy": "exact",
        "child": {
            "kind": "pattern",
            "registry_id": RESTART_PINNED_PACKAGE,
            "version": "1.0.0",
            "content_hash": pin_hash,
        },
    }]);
    serde_json::to_string(&document).expect("the pinned document encodes")
}

/// One package directory under `<nexus_home>/patterns`: the registry the
/// recovery-side pin check reads. Two calls with different bodies are the same
/// pin serving different bytes — the case the document hash cannot catch.
fn write_restart_package(nexus_home: &Path, description: &str) {
    write_restart_package_with(
        nexus_home,
        RESTART_PINNED_PACKAGE,
        description,
        &serde_json::json!([]),
    );
}

/// [`write_restart_package`] for any id, with the nests its own fragment carries.
fn write_restart_package_with(
    nexus_home: &Path,
    id: &str,
    description: &str,
    nests: &serde_json::Value,
) {
    let directory = nexus_home.join("patterns").join(id);
    std::fs::create_dir_all(&directory).expect("pattern directory");
    std::fs::write(
        directory.join("pattern.toml"),
        format!(
            "id = \"{id}\"\nversion = \"1.0.0\"\ndescription = \
             \"restart pin fixture\"\neffect_class = \"pure\"\nrequires_capabilities = []\n\
             slots = []\n\n[ir_compat]\nmin = 0\nmax = 0\n"
        ),
    )
    .expect("pattern manifest");
    std::fs::write(
        directory.join("workflow.ir.json"),
        serde_json::json!({
            "ir_version": 0,
            "id": id,
            "version": 1,
            "revision": 0,
            "state": "candidate",
            "meta": {
                "description": description,
                "kind": "creator",
                "run_intents": [],
                "requires_capabilities": [],
            },
            "entry": "slot",
            "terminal": ["slot"],
            "nodes": [{"id": "slot", "kind": "pattern_slot", "effect_class": "pure"}],
            "edges": [],
            "nests": nests,
            "assets": [],
            "signals": [],
            "roles": [],
            "effect_summary": "pure",
            "provenance": {"source": "compose"},
        })
        .to_string(),
    )
    .expect("pattern fragment");
}

/// The package [`RESTART_PINNED_PACKAGE`]'s own fragment nests: the pin a
/// hashless carrier hides from its document.
const RESTART_NESTED_PACKAGE: &str = "restart-nested-step";

/// Write the carrier's fragment so it pins [`RESTART_NESTED_PACKAGE`] by hash,
/// and return the nested hash it froze.
fn write_restart_nested_fixture(nexus_home: &Path, nested: &str, outer: &str) -> String {
    write_restart_package_with(
        nexus_home,
        RESTART_NESTED_PACKAGE,
        nested,
        &serde_json::json!([]),
    );
    let nested_hash = restart_package_hash(nexus_home, RESTART_NESTED_PACKAGE);
    write_restart_package_with(
        nexus_home,
        RESTART_PINNED_PACKAGE,
        outer,
        &serde_json::json!([{
            "id": "restart-nested-nest",
            "parent_node": "slot",
            "expansion": "defer_until_apply",
            "pin_policy": "exact",
            "child": {
                "kind": "pattern",
                "registry_id": RESTART_NESTED_PACKAGE,
                "version": "1.0.0",
                "content_hash": nested_hash,
            },
        }]),
    );
    nested_hash
}

/// The hash the registry serves for `id` right now.
fn restart_package_hash(nexus_home: &Path, id: &str) -> String {
    nexus_pattern::scan_pattern_packages(
        &nexus_home.join("patterns"),
        nexus_workflow_ir::IR_VERSION,
    )
    .expect("pattern registry scan")
    .into_iter()
    .find(|package| {
        package
            .manifest()
            .is_some_and(|manifest| manifest.id.as_str() == id)
    })
    .and_then(|package| package.content_hash().map(ToOwned::to_owned))
    .unwrap_or_else(|| panic!("the package `{id}` is readable"))
}

fn write_restart_bundle(nexus_home: &Path, preset_id: &str) -> std::path::PathBuf {
    let bundle = nexus_home.join("presets").join(preset_id);
    std::fs::create_dir_all(&bundle).expect("restart preset bundle");
    std::fs::write(bundle.join("preset.yaml"), restart_preset_yaml(preset_id))
        .expect("restart preset yaml");
    bundle
}

/// Open the admitted writer pool a seeding window writes through. [`fixture`]
/// already migrated this DB, so a test that seeds several rows opens the pool
/// once and hands the same `&SqlitePool` to each seed.
async fn open_seed_pool(db_path: &Path) -> nexus_local_db::GuardedPool {
    nexus_local_db::init_engine_pool(db_path)
        .await
        .expect("seed engine pool init")
}

/// Close a seeding window's pool and hand its retained writer guards back, so an
/// owner can boot on the same DB afterwards.
async fn close_seed_pool(db_path: &Path, guarded: nexus_local_db::GuardedPool) {
    guarded.pool().close().await;
    nexus_local_db::writer_protocol::release_retained_writer_guards(db_path);
}

/// Seed the current apply of `preset_id` through the caller's admitted writer
/// pool (`open_seed_pool`): the approval freeze, then the apply row naming it — the same rows, in
/// the same shapes, the approve and apply paths write (the
/// `pinned_approval_strategy_applies_insert` trigger requires exactly this
/// pairing, so a wrong row cannot be seeded at all). The real apply route is
/// proven by the intake tests; this fixture only needs the state a restart finds.
///
/// Returns the identity the seeded apply installed — the value admission freezes
/// on a run it admits under it.
async fn seed_restart_apply(
    pool: &sqlx::SqlitePool,
    preset_id: &str,
) -> nexus_session_storage::run_state::AppliedRunIdentity {
    let document = restart_applied_document(preset_id, APPLIED_ONLY_NODE);
    seed_apply(pool, preset_id, &document, "[]").await
}

/// The same seeded apply carrying ONE recorded pattern pin: the applied
/// document nests the package and `pins_json` freezes the hash the registry
/// served when the row was seeded.
async fn seed_pinned_restart_apply(
    pool: &sqlx::SqlitePool,
    preset_id: &str,
    pin_hash: &str,
) -> nexus_session_storage::run_state::AppliedRunIdentity {
    let document = restart_pinned_applied_document(preset_id, Some(pin_hash));
    let pins = serde_json::json!([{
        "id": RESTART_PINNED_PACKAGE,
        "version": "1.0.0",
        "content_hash": pin_hash,
    }]);
    seed_apply(pool, preset_id, &document, &pins.to_string()).await
}

/// The same seeded apply, with a hashless carrier and NOTHING recorded: the
/// shape an approval writes when the nest was never resolved.
async fn seed_hashless_restart_apply(
    pool: &sqlx::SqlitePool,
    preset_id: &str,
) -> nexus_session_storage::run_state::AppliedRunIdentity {
    let document = restart_pinned_applied_document(preset_id, None);
    seed_apply(pool, preset_id, &document, "[]").await
}

/// Write the approval, the apply row and the schedule precondition an apply
/// requires, all through the caller's admitted pool.
async fn seed_apply(
    pool: &sqlx::SqlitePool,
    preset_id: &str,
    document_json: &str,
    pins_json: &str,
) -> nexus_session_storage::run_state::AppliedRunIdentity {
    let document: nexus_workflow_ir::WorkflowDocument =
        serde_json::from_str(document_json).expect("the applied-side document decodes");
    let hash = nexus_workflow_ir::content_hash(&document).expect("the fixture document hashes");

    // An apply requires the strategy to own a schedule. A pre-seed row is inert
    // (it carries no version-0 core-context record), so nothing ever admits it.
    sqlx::query(
        "INSERT INTO creator_schedules \
         (schedule_id, creator_id, preset_id, preset_version, status, concurrency_kind, \
          current_core_context_version, created_at, updated_at) \
         VALUES (?, ?, ?, 1, 'pending', 'serial', 0, 0, 0)",
    )
    .bind(format!("SCH-precondition-{preset_id}"))
    .bind(CREATOR)
    .bind(preset_id)
    .execute(pool)
    .await
    .expect("the apply precondition row");

    sqlx::query(
        "INSERT INTO strategy_approvals \
         (approval_id, strategy_id, content_hash, candidate_revision, document_json, \
          trajectory_json, pins_json, approver, approved_at) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, '2026-11-15T00:00:00+00:00')",
    )
    .bind(format!("apr_{preset_id}"))
    .bind(preset_id)
    .bind(&hash)
    .bind(i64::try_from(document.revision).expect("revision fits"))
    .bind(serde_json::to_string(&document).expect("document json"))
    .bind(serde_json::json!({ "ir_content_hash": hash }).to_string())
    .bind(pins_json)
    .bind(CREATOR)
    .execute(pool)
    .await
    .expect("approval row");

    sqlx::query(
        "INSERT INTO strategy_applies \
         (apply_id, strategy_id, approval_id, content_hash, applied_at, applied_by, \
          supersedes_apply_id) \
         VALUES (?, ?, ?, ?, '2026-11-15T00:00:00+00:00', ?, NULL)",
    )
    .bind(format!("apl_{preset_id}"))
    .bind(preset_id)
    .bind(format!("apr_{preset_id}"))
    .bind(&hash)
    .bind(CREATOR)
    .execute(pool)
    .await
    .expect("apply row");

    nexus_session_storage::run_state::AppliedRunIdentity {
        apply_id: format!("apl_{preset_id}"),
        content_hash: hash,
    }
}

/// Seed a SECOND apply for `preset_id`, superseding the one [`seed_restart_apply`]
/// installed — the re-apply a restart after admission must notice (I-000464).
/// Its document reaches a different probe node, so a recovery that rebuilt from
/// it instead of refusing would land somewhere only this document defines.
async fn seed_superseding_apply(
    pool: &sqlx::SqlitePool,
    preset_id: &str,
    supersedes: &str,
) -> nexus_session_storage::run_state::AppliedRunIdentity {
    let document: nexus_workflow_ir::WorkflowDocument =
        serde_json::from_str(&restart_applied_document(preset_id, SUPERSEDING_ONLY_NODE))
            .expect("the superseding document decodes");
    let hash = nexus_workflow_ir::content_hash(&document).expect("the fixture document hashes");

    sqlx::query(
        "INSERT INTO strategy_approvals \
         (approval_id, strategy_id, content_hash, candidate_revision, document_json, \
          trajectory_json, pins_json, approver, approved_at) \
         VALUES (?, ?, ?, ?, ?, ?, '[]', ?, '2026-11-15T00:00:01+00:00')",
    )
    .bind(format!("apr2_{preset_id}"))
    .bind(preset_id)
    .bind(&hash)
    .bind(i64::try_from(document.revision).expect("revision fits"))
    .bind(serde_json::to_string(&document).expect("document json"))
    .bind(serde_json::json!({ "ir_content_hash": hash }).to_string())
    .bind(CREATOR)
    .execute(pool)
    .await
    .expect("superseding approval row");

    sqlx::query(
        "INSERT INTO strategy_applies \
         (apply_id, strategy_id, approval_id, content_hash, applied_at, applied_by, \
          supersedes_apply_id) \
         VALUES (?, ?, ?, ?, '2026-11-15T00:00:01+00:00', ?, ?)",
    )
    .bind(format!("apl2_{preset_id}"))
    .bind(preset_id)
    .bind(format!("apr2_{preset_id}"))
    .bind(&hash)
    .bind(CREATOR)
    .bind(supersedes)
    .execute(pool)
    .await
    .expect("superseding apply row");

    nexus_session_storage::run_state::AppliedRunIdentity {
        apply_id: format!("apl2_{preset_id}"),
        content_hash: hash,
    }
}

/// Seed the durable v1 row a killed daemon leaves behind: a non-terminal run of
/// `preset_id` parked at the shared entry node, written through the REAL store
/// API with the on-disk bundle's frozen source identity (exactly what admission
/// persists for this run — an applied run's descriptor names the strategy's
/// bundle, never the document).
///
/// `applied` is the applied identity admission freezes on a run it admits under
/// an apply (P4 Task 4); `None` is a descriptor that records none, which
/// recovery resolves against the strategy's current apply as it always has.
/// `intake_mode` is the mode arm the row was admitted under (`None` = a legacy
/// row predating the field); recovery follows the recorded value.
async fn seed_restart_run(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    preset_id: &str,
    bundle: &Path,
    applied: Option<nexus_session_storage::run_state::AppliedRunIdentity>,
    intake_mode: Option<nexus_home_layout::strategy_intake::StrategyIntakeMode>,
) {
    let loaded = nexus_preset::load_preset(
        bundle,
        &nexus_preset::capability_catalog::BuiltinCapabilityCatalog,
    )
    .expect("the restart fixture bundle loads");
    let source = loaded
        .source_identity
        .expect("a directory bundle carries an identity");
    let store =
        nexus_session_storage::storage::sqlite::SqliteSessionStorage::new(Arc::new(pool.clone()));
    let descriptor = nexus_session_storage::run_state::RunDescriptorV1 {
        creator_id: CREATOR.to_string(),
        work_id: None,
        workspace_root: std::path::PathBuf::from("/tmp/ws"),
        preset_id: preset_id.to_string(),
        preset_version: loaded.version,
        source,
        input: serde_json::Map::default(),
        agent_bindings: std::collections::HashMap::new(),
        parent_session_id: None,
        graph_name: None,
        applied,
        intake_mode,
    };
    let root = graph_flow::Session::new_from_task(session_id.to_string(), SHARED_ENTRY_NODE);
    root.context
        .set("_session_id", session_id.to_string())
        .expect("seed session context");
    let checkpoint = nexus_session_storage::run_state::RunCheckpoint {
        root: &root,
        children: &[],
    };
    store
        .start_run(
            &nexus_orchestration::SessionId(session_id.to_string()),
            &descriptor,
            checkpoint,
            &nexus_session_storage::run_state::RunStateV1::default(),
        )
        .await
        .expect("seed start_run");
}

/// The run's durable record, read through the owner's own admitted pool.
async fn read_restart_session(
    pool: &sqlx::SqlitePool,
    session_id: &str,
) -> Option<graph_flow::Session> {
    use graph_flow::SessionStorage as _;
    nexus_session_storage::storage::sqlite::SqliteSessionStorage::new(Arc::new(pool.clone()))
        .get(session_id)
        .await
        .expect("read the run row")
}

/// Wait until the run's durable position is `node`.
async fn wait_for_position(pool: &sqlx::SqlitePool, session_id: &str, node: &str, what: &str) {
    wait_until(what, || {
        let pool = pool.clone();
        let session_id = session_id.to_string();
        async move {
            read_restart_session(&pool, &session_id)
                .await
                .is_some_and(|session| session.current_task_id == node)
        }
    })
    .await;
}

/// The run's authoritative record: status, durable state, frozen descriptor.
async fn read_restart_record(
    pool: &sqlx::SqlitePool,
    session_id: &str,
) -> nexus_session_storage::run_state::RunRecord {
    nexus_session_storage::storage::sqlite::SqliteSessionStorage::new(Arc::new(pool.clone()))
        .load_run(&nexus_orchestration::SessionId(session_id.to_string()))
        .await
        .expect("read the run record")
        .expect("the run record exists")
}

/// Wait until the run's durable record satisfies `cond`, answering the record the
/// satisfied poll read: one read per tick, never a second one after the wait.
async fn wait_for_record<F>(
    pool: &sqlx::SqlitePool,
    session_id: &str,
    what: &str,
    mut cond: F,
) -> nexus_session_storage::run_state::RunRecord
where
    F: FnMut(&nexus_session_storage::run_state::RunRecord) -> bool,
{
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let record = read_restart_record(pool, session_id).await;
            if cond(&record) {
                return record;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"))
}

/// Wait until the admitted run parks on the applied document's manual wait and
/// answer the durable wait id that parking transition wrote.
async fn wait_for_applied_manual_wait(pool: &sqlx::SqlitePool, session_id: &str) -> String {
    wait_for_record(
        pool,
        session_id,
        "the run to park on the applied-only manual wait",
        |record| {
            record.status == nexus_orchestration::engine::SessionStatus::WaitingForInput
                && record
                    .state
                    .as_ref()
                    .and_then(|state| state.wait.as_ref())
                    .is_some_and(|wait| wait.task_id == APPLIED_ONLY_NODE)
        },
    )
    .await
    .state
    .and_then(|state| state.wait)
    .expect("the manual wait record exists")
    .wait_id
}

/// Boot recovery re-drives the seeded run past its shared entry node, then the
/// durable record names the node the reconstructed graph reached.
async fn recovered_position(owner: &CoreService, session_id: &str) -> graph_flow::Session {
    let pool: sqlx::SqlitePool = owner.pool().clone();
    wait_until(
        "boot recovery to re-drive the run past its shared entry node",
        || {
            let pool = pool.clone();
            async move {
                read_restart_session(&pool, session_id)
                    .await
                    .is_some_and(|session| session.current_task_id != SHARED_ENTRY_NODE)
            }
        },
    )
    .await;
    read_restart_session(&pool, session_id)
        .await
        .expect("the recovered run row exists")
}

/// Generation 1 of a restart test: admit `preset_id` through the `start_session`
/// seam on a fresh owner, check the identity its descriptor froze, park the run
/// on the applied document's manual wait, then retire the owner. Answers the run
/// and the durable wait admission left behind.
async fn admit_and_park(
    f: &Fixture,
    nexus_home: &Path,
    admitted: &nexus_session_storage::run_state::AppliedRunIdentity,
) -> (nexus_orchestration::SessionId, String) {
    let owner = open_engine_owner(f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    let caps = handle.capability_holder();
    let session_id = handle
        .coordinator()
        .start_session(
            RESTART_PRESET,
            CREATOR,
            None,
            std::collections::HashMap::new(),
            nexus_home,
            &caps,
            None,
            None,
        )
        .await
        .expect("an applied strategy admits through start_session");
    let pool: sqlx::SqlitePool = owner.pool().clone();
    let wait_id = wait_for_applied_manual_wait(&pool, &session_id.0).await;
    assert_eq!(
        read_restart_record(&pool, &session_id.0)
            .await
            .descriptor
            .expect("the frozen descriptor exists")
            .applied,
        Some(admitted.clone()),
        "the admitted descriptor must freeze the apply it runs"
    );
    handle.close().await.unwrap();
    owner.close().await.unwrap();
    (session_id, wait_id)
}

/// P4 Task 2: a restart during an in-flight applied run recovers into the
/// APPLIED graph.
///
/// The strategy has a current apply whose document reaches `applied_only_probe`
/// — a node id the YAML descriptor does not carry at all — while the descriptor
/// reaches `preset_only_probe`. Both share `open_probe`, so the same seeded
/// position is a real task in either graph and only the graph choice can move
/// the run onto the two different nodes.
///
/// Mutation this fails on: recovery reconstructing from the descriptor alone
/// (the pre-change behaviour) leaves the recovered position on
/// `preset_only_probe`, never on the applied-only node id.
///
/// The seeded descriptor records NO applied identity — the shape of a row
/// persisted before Task 4 froze it. The recorded-and-unchanged restart is
/// proven by `a_restart_of_an_unchanged_applied_run_still_recovers_the_applied_graph`.
#[tokio::test]
async fn a_restart_during_an_applied_run_recovers_into_the_applied_graph() {
    let f = fixture().await;
    let bundle = write_restart_bundle(&f.tmp.path().join(".nexus42"), RESTART_PRESET);
    let session_id = format!("{RESTART_PRESET}:in-flight");
    let seed = open_seed_pool(&f.db_path).await;
    seed_restart_apply(seed.pool(), RESTART_PRESET).await;
    seed_restart_run(
        seed.pool(),
        &session_id,
        RESTART_PRESET,
        &bundle,
        None,
        None,
    )
    .await;
    close_seed_pool(&f.db_path, seed).await;

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");

    let session = recovered_position(&owner, &session_id).await;
    assert_eq!(
        session.current_task_id, APPLIED_ONLY_NODE,
        "the recovered run must execute the applied document's node; got {:?}",
        session.current_task_id
    );
    assert_ne!(
        session.current_task_id, PRESET_ONLY_NODE,
        "the descriptor-only node must never run while the strategy has a current apply"
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

/// P4 Task 2: a strategy with NO apply still recovers exactly as before — the
/// descriptor bundle's graph — which is what the same seeded position proves
/// from the other side.
#[tokio::test]
async fn a_restart_with_no_apply_still_recovers_the_descriptor_graph() {
    let f = fixture().await;
    let bundle = write_restart_bundle(&f.tmp.path().join(".nexus42"), RESTART_NO_APPLY_PRESET);
    let session_id = format!("{RESTART_NO_APPLY_PRESET}:in-flight");
    let seed = open_seed_pool(&f.db_path).await;
    seed_restart_run(
        seed.pool(),
        &session_id,
        RESTART_NO_APPLY_PRESET,
        &bundle,
        None,
        None,
    )
    .await;
    close_seed_pool(&f.db_path, seed).await;

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");

    let session = recovered_position(&owner, &session_id).await;
    assert_eq!(
        session.current_task_id, PRESET_ONLY_NODE,
        "without an apply the descriptor graph is the run's graph, exactly as before"
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

// ── Applied identity frozen on the run (Task 4, I-000464) ──────────────────

/// P4 Task 4 (I-000464): a re-apply between admission and a restart is refused.
///
/// Recovery resolves the strategy's CURRENT apply, so without the identity
/// frozen on the descriptor it rebuilds this run from the superseding document —
/// a graph the run was never admitted under, carrying the old position, context
/// and bindings. The descriptor records apply A and the strategy's current apply
/// is apply B (A gained a successor): reconstruction must be the typed
/// `reconstruction_unavailable` refusal, and the durable position must stay
/// exactly where the killed daemon left it.
///
/// Mutation this fails on: dropping the recorded-identity comparison rebuilds
/// the run from B and the recovered position moves off the shared entry node.
#[tokio::test]
async fn a_restart_after_a_re_apply_refuses_instead_of_rebuilding_the_newer_document() {
    let f = fixture().await;
    let bundle = write_restart_bundle(&f.tmp.path().join(".nexus42"), RESTART_PRESET);
    let session_id = format!("{RESTART_PRESET}:in-flight");
    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_restart_apply(seed.pool(), RESTART_PRESET).await;
    seed_restart_run(
        seed.pool(),
        &session_id,
        RESTART_PRESET,
        &bundle,
        Some(admitted.clone()),
        None,
    )
    .await;
    // The re-apply lands AFTER the run was admitted, which is the whole window
    // the frozen identity exists to close.
    let superseding = seed_superseding_apply(seed.pool(), RESTART_PRESET, &admitted.apply_id).await;
    close_seed_pool(&f.db_path, seed).await;

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");

    // The A7 reattach seam — what Continue/Resume run before any transition —
    // refuses the run its admitted graph instead of rebuilding a newer one.
    let refusal = handle
        .coordinator()
        .signal_run(
            &nexus_orchestration::SessionId(session_id.clone()),
            RunSignal::Continue {
                wait_id: "irrelevant".to_string(),
            },
        )
        .await
        .expect_err("a run whose admitted apply is no longer current must not be recovered");
    let RunControlError::ReconstructionUnavailable { reason, .. } = refusal else {
        panic!("expected reconstruction_unavailable, got {refusal:?}");
    };
    assert!(
        reason.contains(&admitted.apply_id),
        "the refusal must name the apply the run was admitted under: {reason}"
    );
    assert!(
        reason.contains(&superseding.apply_id),
        "the refusal must name the apply that replaced it: {reason}"
    );

    // Not a recovered graph: the durable position never left the shared entry
    // node, so neither the superseding document's node nor the descriptor's ran.
    let pool: sqlx::SqlitePool = owner.pool().clone();
    let session = read_restart_session(&pool, &session_id)
        .await
        .expect("the run row exists");
    assert_eq!(
        session.current_task_id, SHARED_ENTRY_NODE,
        "a refused reconstruction must not advance the run; got {:?}",
        session.current_task_id
    );
    assert_ne!(session.current_task_id, SUPERSEDING_ONLY_NODE);

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

/// P4 Task 4: the recorded identity is not a prohibition — a run restarted while
/// its admitted apply is still the strategy's current one recovers into the
/// applied graph exactly as before.
#[tokio::test]
async fn a_restart_of_an_unchanged_applied_run_still_recovers_the_applied_graph() {
    let f = fixture().await;
    let bundle = write_restart_bundle(&f.tmp.path().join(".nexus42"), RESTART_PRESET);
    let session_id = format!("{RESTART_PRESET}:in-flight");
    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_restart_apply(seed.pool(), RESTART_PRESET).await;
    seed_restart_run(
        seed.pool(),
        &session_id,
        RESTART_PRESET,
        &bundle,
        Some(admitted),
        None,
    )
    .await;
    close_seed_pool(&f.db_path, seed).await;

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");

    let session = recovered_position(&owner, &session_id).await;
    assert_eq!(
        session.current_task_id, APPLIED_ONLY_NODE,
        "a matching recorded identity must recover the applied graph; got {:?}",
        session.current_task_id
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

// ── The frozen pin snapshot re-compared at recovery (P6) ──────────────────

/// P6: a pattern body edited between apply and restart is refused at recovery.
///
/// The applied document's own bytes still hash to the recorded identity — the
/// edit moved the package, not the document — so only the frozen `pins_json`
/// snapshot can catch it. Recovery must refuse the reconstruction with the
/// stable `applied_pin_changed` code and name the pinned package, and the
/// durable position must stay exactly where the killed daemon left it.
#[tokio::test]
async fn a_restart_after_a_pinned_body_moved_refuses_naming_the_pin() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");
    let bundle = write_restart_bundle(&nexus_home, RESTART_PRESET);
    write_restart_package(&nexus_home, "original bytes");
    let recorded = restart_package_hash(&nexus_home, RESTART_PINNED_PACKAGE);
    let session_id = format!("{RESTART_PRESET}:in-flight");
    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_pinned_restart_apply(seed.pool(), RESTART_PRESET, &recorded).await;
    seed_restart_run(
        seed.pool(),
        &session_id,
        RESTART_PRESET,
        &bundle,
        Some(admitted),
        None,
    )
    .await;
    close_seed_pool(&f.db_path, seed).await;

    // Same id and version, different bytes.
    write_restart_package(&nexus_home, "changed bytes");
    assert_ne!(
        restart_package_hash(&nexus_home, RESTART_PINNED_PACKAGE),
        recorded,
        "the fixture must actually move the pinned bytes"
    );

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    let refusal = handle
        .coordinator()
        .signal_run(
            &nexus_orchestration::SessionId(session_id.clone()),
            RunSignal::Continue {
                wait_id: "irrelevant".to_string(),
            },
        )
        .await
        .expect_err("a run whose pinned body moved must not be recovered");
    let RunControlError::ReconstructionUnavailable { reason, .. } = refusal else {
        panic!("expected reconstruction_unavailable, got {refusal:?}");
    };
    assert!(
        reason.contains("applied_pin_changed"),
        "the refusal keeps the coded reason: {reason}"
    );
    assert!(
        reason.contains(RESTART_PINNED_PACKAGE),
        "the refusal must name the pinned package: {reason}"
    );
    assert!(
        reason.contains(&recorded),
        "the refusal must name the hash the apply froze: {reason}"
    );

    let pool: sqlx::SqlitePool = owner.pool().clone();
    let session = read_restart_session(&pool, &session_id)
        .await
        .expect("the run row exists");
    assert_eq!(
        session.current_task_id, SHARED_ENTRY_NODE,
        "a refused reconstruction must not advance the run; got {:?}",
        session.current_task_id
    );
    assert_ne!(session.current_task_id, APPLIED_ONLY_NODE);

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

/// I-000506: BOTH routes compare the pins in the OWNER's home, never a caller's.
///
/// The pins are compared against `<nexus_home>/patterns`. Admission once took
/// the home from its caller while recovery could only read the home
/// `CreatorAppliedDocuments` was built with — two values kept equal by wiring
/// alone. The dangerous direction is the accepting one: a home whose pinned
/// body moved while the other's had not would let recovery clear a run that
/// the admitting home refuses as non-replayable.
///
/// `caller` is a second registry answering the same pin id with different
/// bytes, so any resolution that read it refuses before a run row exists.
/// Admission runs through it; recovery must read the owner home.
///
/// Mutation this fails on: admission resolving the pins against the home its
/// caller passed (refuses generation 1 with `applied_pin_changed`), and
/// recovery re-comparing against a home other than the owner's (accepts
/// generation 2, whose owner-home pin has moved).
#[tokio::test]
async fn a_restart_compares_pins_in_the_owners_home_not_the_callers() {
    let f = fixture().await;
    let owner_home = f.tmp.path().join(".nexus42");
    let caller_home = f.tmp.path().join("caller").join(".nexus42");
    write_restart_bundle(&owner_home, RESTART_PRESET);
    write_restart_bundle(&caller_home, RESTART_PRESET);
    write_restart_package(&owner_home, "owner bytes");
    write_restart_package(&caller_home, "caller bytes");
    let recorded = restart_package_hash(&owner_home, RESTART_PINNED_PACKAGE);
    let caller_hash = restart_package_hash(&caller_home, RESTART_PINNED_PACKAGE);
    assert_ne!(
        caller_hash, recorded,
        "the fixture must give the two homes different bytes under the same pin"
    );

    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_pinned_restart_apply(seed.pool(), RESTART_PRESET, &recorded).await;
    close_seed_pool(&f.db_path, seed).await;

    // ── Generation 1: admit with the OTHER home as the caller home. The pin
    //    still matches in the owner home, so the run admits. ──
    let (session_id, _wait_id) = admit_and_park(&f, &caller_home, &admitted).await;

    // ── Generation 2: the owner home's pinned body moved. Recovery must refuse
    //    naming the hash the apply froze, not the caller home's untouched one. ──
    write_restart_package(&owner_home, "owner bytes moved");
    assert_ne!(
        restart_package_hash(&owner_home, RESTART_PINNED_PACKAGE),
        recorded,
        "the fixture must actually move the owner home's pinned bytes"
    );
    let (refusal, position) = restart_pin_refusal(&f, &session_id).await;
    assert!(
        refusal.contains("applied_pin_changed"),
        "the refusal keeps the coded reason: {refusal}"
    );
    assert!(
        refusal.contains(&recorded),
        "the refusal must name the hash the apply froze: {refusal}"
    );
    assert!(
        refusal.contains(RESTART_PINNED_PACKAGE),
        "the refusal must name the pinned package: {refusal}"
    );
    assert!(
        !refusal.contains(&caller_hash),
        "the caller home's registry must not enter the decision: {refusal}"
    );
    assert_eq!(
        position.as_deref(),
        Some(APPLIED_ONLY_NODE),
        "a refused reconstruction must not advance the run past its applied wait"
    );

    // ── Generation 3: moving the CALLER home's pin changes nothing — the same
    //    run over a fresh owner is refused for the same reason. ──
    write_restart_package(&caller_home, "caller bytes moved");
    let (again, _) = restart_pin_refusal(&f, &session_id).await;
    assert_eq!(
        again, refusal,
        "a caller home's pin state must not change the recovery verdict"
    );
}

/// Boot one owner generation and signal a parked applied run at it: the
/// reconstruction refusal reason plus the position the run was left on.
async fn restart_pin_refusal(
    f: &Fixture,
    session_id: &nexus_orchestration::SessionId,
) -> (String, Option<String>) {
    let owner = open_engine_owner(f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    // Reconstruction refuses before the wait is resolved; the neighbouring
    // moved-pin test signals the same placeholder for that reason.
    let refusal = handle
        .coordinator()
        .signal_run(
            session_id,
            RunSignal::Continue {
                wait_id: "irrelevant".to_string(),
            },
        )
        .await
        .expect_err("a run whose pinned body moved must not be recovered");
    let RunControlError::ReconstructionUnavailable { reason, .. } = refusal else {
        panic!("expected reconstruction_unavailable, got {refusal:?}");
    };
    let pool: sqlx::SqlitePool = owner.pool().clone();
    let position = read_restart_session(&pool, &session_id.0)
        .await
        .map(|session| session.current_task_id);
    handle.close().await.unwrap();
    owner.close().await.unwrap();
    (reason, position)
}

/// P6: an empty pin snapshot cannot clear a carrier whose body it never recorded.
///
/// The applied document names the carrier by id and version with no
/// `content_hash`, and the approval recorded no pin at all. What the carrier
/// consumes is decided by its own fragment — which pins
/// [`RESTART_NESTED_PACKAGE`] by hash — so the recovery-side check must refuse
/// instead of clearing a document whose reachable pins it never looked at.
#[tokio::test]
async fn a_restart_with_a_hashless_carrier_and_no_recorded_pin_refuses() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");
    let bundle = write_restart_bundle(&nexus_home, RESTART_PRESET);
    let nested = write_restart_nested_fixture(&nexus_home, "nested bytes", "outer bytes");
    let session_id = format!("{RESTART_PRESET}:in-flight");
    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_hashless_restart_apply(seed.pool(), RESTART_PRESET).await;
    seed_restart_run(
        seed.pool(),
        &session_id,
        RESTART_PRESET,
        &bundle,
        Some(admitted),
        None,
    )
    .await;
    close_seed_pool(&f.db_path, seed).await;

    // The nested body moved after the freeze, and no row records what it was.
    write_restart_package_with(
        &nexus_home,
        RESTART_NESTED_PACKAGE,
        "nested bytes moved",
        &serde_json::json!([]),
    );
    assert_ne!(
        restart_package_hash(&nexus_home, RESTART_NESTED_PACKAGE),
        nested,
        "the fixture must actually move the nested bytes"
    );

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    let refusal = handle
        .coordinator()
        .signal_run(
            &nexus_orchestration::SessionId(session_id.clone()),
            RunSignal::Continue {
                wait_id: "irrelevant".to_string(),
            },
        )
        .await
        .expect_err("a hashless carrier under an empty snapshot must not be recovered");
    let RunControlError::ReconstructionUnavailable { reason, .. } = refusal else {
        panic!("expected reconstruction_unavailable, got {refusal:?}");
    };
    assert!(
        reason.contains("applied_pins_unverifiable"),
        "the refusal keeps the coded reason: {reason}"
    );
    assert!(
        reason.contains(RESTART_PINNED_PACKAGE),
        "the refusal must name the carrier it cannot compare: {reason}"
    );

    let pool: sqlx::SqlitePool = owner.pool().clone();
    let session = read_restart_session(&pool, &session_id)
        .await
        .expect("the run row exists");
    assert_eq!(
        session.current_task_id, SHARED_ENTRY_NODE,
        "a refused reconstruction must not advance the run; got {:?}",
        session.current_task_id
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

/// P6: unchanged pins recover the applied graph exactly as before — the new
/// check must admit every applied identity whose bodies still hash to the
/// frozen snapshot.
#[tokio::test]
async fn a_restart_with_unchanged_pins_recovers_the_applied_graph() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");
    let bundle = write_restart_bundle(&nexus_home, RESTART_PRESET);
    write_restart_package(&nexus_home, "original bytes");
    let recorded = restart_package_hash(&nexus_home, RESTART_PINNED_PACKAGE);
    let session_id = format!("{RESTART_PRESET}:in-flight");
    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_pinned_restart_apply(seed.pool(), RESTART_PRESET, &recorded).await;
    seed_restart_run(
        seed.pool(),
        &session_id,
        RESTART_PRESET,
        &bundle,
        Some(admitted),
        None,
    )
    .await;
    close_seed_pool(&f.db_path, seed).await;

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");

    let session = recovered_position(&owner, &session_id).await;
    assert_eq!(
        session.current_task_id, APPLIED_ONLY_NODE,
        "unchanged pins must recover the applied graph; got {:?}",
        session.current_task_id
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

// ── The YAML run seam freezes the apply it runs (P6, I-000443) ─────────────

/// P6: `start_session` resolves the strategy's current apply before it builds,
/// under the intake mode the home config selects.
///
/// An applied strategy under the default `ir-only` runs the applied document —
/// proven by the run's position on `applied_only_probe`, a node id the
/// descriptor bundle does not declare — and freezes that identity plus
/// `intake_mode: Some(IrOnly)` on the descriptor. Without the freeze,
/// recovery's `(None, current)` arm rebuilds from whatever the strategy has
/// applied by then: a graph this run never ran.
///
/// The default is also the behaviour change ADR P6 condition 3 names: a user
/// bundle with NO current apply refuses (`yaml_intake_disabled`) before any
/// graph is built — never a YAML fallback — and no durable run row is minted.
#[tokio::test]
async fn start_session_freezes_and_runs_the_strategy_apply() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");
    write_restart_bundle(&nexus_home, RESTART_PRESET);
    write_restart_bundle(&nexus_home, RESTART_NO_APPLY_PRESET);
    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_restart_apply(seed.pool(), RESTART_PRESET).await;
    close_seed_pool(&f.db_path, seed).await;

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    let caps = handle.capability_holder();
    let coordinator = handle.coordinator();
    let pool: sqlx::SqlitePool = owner.pool().clone();

    // Default (no config key): the applied strategy runs the applied document
    // and freezes the identity AND the intake mode.
    let applied_run = coordinator
        .start_session(
            RESTART_PRESET,
            CREATOR,
            None,
            std::collections::HashMap::new(),
            &nexus_home,
            &caps,
            None,
            None,
        )
        .await
        .expect("an applied strategy admits through start_session");
    wait_for_position(
        &pool,
        &applied_run.0,
        APPLIED_ONLY_NODE,
        "the applied run to reach the applied-only node",
    )
    .await;
    assert_ne!(
        read_restart_session(&pool, &applied_run.0)
            .await
            .expect("the started run row exists")
            .current_task_id,
        PRESET_ONLY_NODE,
        "the descriptor-only node must never run while the strategy has a current apply"
    );
    let applied_descriptor = read_restart_record(&pool, &applied_run.0)
        .await
        .descriptor
        .expect("the frozen descriptor exists");
    assert_eq!(
        applied_descriptor.applied,
        Some(admitted),
        "the descriptor must freeze the apply the run executes"
    );
    assert_eq!(
        applied_descriptor.intake_mode,
        Some(nexus_home_layout::strategy_intake::StrategyIntakeMode::IrOnly),
        "the default arm is ir-only and must be recorded"
    );

    // The default no-apply refusal: a user bundle with no current apply
    // refuses before graph construction, and no run row is minted.
    let refusal = coordinator
        .start_session(
            RESTART_NO_APPLY_PRESET,
            CREATOR,
            Some("intake"),
            std::collections::HashMap::new(),
            &nexus_home,
            &caps,
            None,
            None,
        )
        .await
        .expect_err("the default ir-only arm refuses a user bundle with no apply");
    let RunControlError::YamlIntakeDisabled { preset_id } = refusal else {
        panic!("expected yaml_intake_disabled, got {refusal:?}");
    };
    assert_eq!(preset_id, RESTART_NO_APPLY_PRESET);
    let minted: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM orchestration_sessions WHERE preset_id = ?")
            .bind(RESTART_NO_APPLY_PRESET)
            .fetch_one(&pool)
            .await
            .expect("count run rows");
    assert_eq!(
        minted, 0,
        "a refused admission must not leave a durable run row"
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

/// P6 Task 3: an `ir-only` descriptor that records no applied identity is the
/// no-apply admission refusal resurfaced at recovery: a typed, non-replayable
/// refusal — never a YAML rebuild, even though the strategy has since gained
/// a current apply (recovery under ir-only does not fall back).
#[tokio::test]
async fn an_ir_only_descriptor_without_an_apply_refuses_recovery() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");
    let bundle = write_restart_bundle(&nexus_home, RESTART_PRESET);
    let session_id = format!("{RESTART_PRESET}:in-flight");
    let seed = open_seed_pool(&f.db_path).await;
    // The strategy has a current apply NOW; the descriptor records none, the
    // state a tampered or partially-written ir-only row can reach.
    seed_restart_apply(seed.pool(), RESTART_PRESET).await;
    seed_restart_run(
        seed.pool(),
        &session_id,
        RESTART_PRESET,
        &bundle,
        None,
        Some(nexus_home_layout::strategy_intake::StrategyIntakeMode::IrOnly),
    )
    .await;
    close_seed_pool(&f.db_path, seed).await;

    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");

    // The A7 reattach seam refuses instead of rebuilding from YAML or from
    // the strategy's current apply.
    let refusal = handle
        .coordinator()
        .signal_run(
            &nexus_orchestration::SessionId(session_id.clone()),
            RunSignal::Continue {
                wait_id: "irrelevant".to_string(),
            },
        )
        .await
        .expect_err("an ir-only descriptor without an apply must not recover");
    let RunControlError::ReconstructionUnavailable { reason, .. } = refusal else {
        panic!("expected reconstruction_unavailable, got {refusal:?}");
    };
    assert!(
        reason.contains("yaml_intake_disabled"),
        "the refusal must carry the stable intake text: {reason}"
    );

    // Not a recovered graph: the durable position never left the shared
    // entry node.
    let pool: sqlx::SqlitePool = owner.pool().clone();
    let session = read_restart_session(&pool, &session_id)
        .await
        .expect("the run row exists");
    assert_eq!(
        session.current_task_id, SHARED_ENTRY_NODE,
        "a refused reconstruction must not advance the run; got {:?}",
        session.current_task_id
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

/// P6 Task 2: a run ADMITTED through `start_session` refuses to restart once the
/// strategy gained a successor apply.
///
/// Admission froze apply A on the descriptor; while the daemon was down the
/// strategy moved on to apply B. Recovery must keep the run non-replayable —
/// naming both apply ids — instead of rebuilding it from a document it never
/// ran (`engine.rs:4109-4134`), and the durable position plus the human wait
/// must stay exactly where admission left them.
///
/// Mutation this fails on: admitting without freezing the identity (descriptor
/// `applied: None`) hits recovery's `(None, current)` arm and rebuilds the run
/// from B, so the refusal never happens.
#[tokio::test]
async fn a_restart_after_a_successor_apply_refuses_the_admitted_run() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");
    write_restart_bundle(&nexus_home, RESTART_PRESET);
    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_restart_apply(seed.pool(), RESTART_PRESET).await;
    close_seed_pool(&f.db_path, seed).await;

    // ── Generation 1: admit through the seam, freeze A, park on the applied
    //    document's manual wait. ──
    let (session_id, wait_id) = admit_and_park(&f, &nexus_home, &admitted).await;

    // ── The successor lands while no daemon owns the DB: the re-apply window
    //    the frozen identity exists to close. ──
    let seed = open_seed_pool(&f.db_path).await;
    let superseding = seed_superseding_apply(seed.pool(), RESTART_PRESET, &admitted.apply_id).await;
    close_seed_pool(&f.db_path, seed).await;

    // ── Generation 2: boot recovery cannot rebuild this run, and the A7
    //    reattach seam surfaces that as the typed refusal. ──
    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    let refusal = handle
        .coordinator()
        .signal_run(
            &session_id,
            RunSignal::Continue {
                wait_id: wait_id.clone(),
            },
        )
        .await
        .expect_err("a run whose admitted apply gained a successor must not be recovered");
    let RunControlError::ReconstructionUnavailable { reason, .. } = refusal else {
        panic!("expected reconstruction_unavailable, got {refusal:?}");
    };
    assert!(
        reason.contains(&admitted.apply_id),
        "the refusal must name the apply the run was admitted under: {reason}"
    );
    assert!(
        reason.contains(&superseding.apply_id),
        "the refusal must name the apply that replaced it: {reason}"
    );

    // The refusal advances nothing: same node, same human wait, same identity.
    let pool: sqlx::SqlitePool = owner.pool().clone();
    let record = read_restart_record(&pool, &session_id.0).await;
    assert_eq!(
        record.status,
        nexus_orchestration::engine::SessionStatus::WaitingForInput,
        "a refused reconstruction must leave the run on its preserved wait"
    );
    assert_eq!(
        record
            .state
            .as_ref()
            .and_then(|state| state.wait.as_ref())
            .map(|wait| wait.wait_id.as_str()),
        Some(wait_id.as_str()),
        "the human wait must survive the refusal unconsumed"
    );
    let session = read_restart_session(&pool, &session_id.0)
        .await
        .expect("the run row exists");
    assert_eq!(
        session.current_task_id, APPLIED_ONLY_NODE,
        "a refused reconstruction must not advance the run; got {:?}",
        session.current_task_id
    );
    assert_eq!(
        record.descriptor.and_then(|descriptor| descriptor.applied),
        Some(admitted),
        "the frozen identity must survive the refused restart unchanged"
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

/// P6 Task 2: the same seam admitted with NO successor restarts into the
/// document it froze.
///
/// The kill left the run parked on `applied_only_probe` — a node id only the
/// applied document declares. After the restart the run must be rebuilt from
/// that document and driven off it: the manual wait is consumed and the run
/// settles `completed`, which a YAML-graph reconstruction could not do (its
/// graph has no `applied_only_probe` task to execute).
#[tokio::test]
async fn a_restart_of_an_admitted_applied_run_recovers_the_applied_document() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");
    write_restart_bundle(&nexus_home, RESTART_PRESET);
    let seed = open_seed_pool(&f.db_path).await;
    let admitted = seed_restart_apply(seed.pool(), RESTART_PRESET).await;
    close_seed_pool(&f.db_path, seed).await;

    // ── Generation 1: admit through the seam and park on the applied wait. ──
    let (session_id, wait_id) = admit_and_park(&f, &nexus_home, &admitted).await;

    // ── Generation 2: the current apply is still the admitted one, so the
    //    restart reconstructs the applied document and re-drives the run. ──
    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    handle
        .coordinator()
        .signal_run(&session_id, RunSignal::Continue { wait_id })
        .await
        .expect("an unchanged apply must reconstruct the admitted document");

    let pool: sqlx::SqlitePool = owner.pool().clone();
    let sid = session_id.0.clone();
    wait_until(
        "the reconstructed run to finish the applied document",
        || {
            let pool = pool.clone();
            let sid = sid.clone();
            async move {
                read_restart_record(&pool, &sid).await.status
                    == nexus_orchestration::engine::SessionStatus::Completed
            }
        },
    )
    .await;
    let record = read_restart_record(&pool, &session_id.0).await;
    let session = read_restart_session(&pool, &session_id.0)
        .await
        .expect("the run row exists");
    assert_eq!(
        record.status,
        nexus_orchestration::engine::SessionStatus::Completed,
        "the rebuilt run must execute the applied document to its terminal node, got {:?}",
        record.status
    );
    assert_eq!(
        session.current_task_id, "done",
        "the applied document's terminal node is where an applied recovery ends; got {:?}",
        session.current_task_id
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

// ─────────────────────────────────────────────────────────────────────────────
// Embedded execution: the run input is the document (compass D10).
// `.mstar/plans/embedded-ir-execution.md` Tasks 3–5. An embedded bundle resolves
// an applied document from its own compiled-in bytes, admission freezes that
// identity, and a restart re-enters through the ONE resolution route.
// ─────────────────────────────────────────────────────────────────────────────

/// An embedded preset with NO roles and a manual-wait ENTRY: a run admitted for
/// it parks immediately, so neither admission nor recovery performs a provider
/// effect.
const EMBEDDED_RUN_PRESET: &str = "novel-review-master";
/// [`EMBEDDED_RUN_PRESET`]'s entry state — a manual wait.
const EMBEDDED_ENTRY_NODE: &str = "present";
/// The manual wait [`EMBEDDED_ENTRY_NODE`] advances to once continued.
const EMBEDDED_NEXT_NODE: &str = "await_decision";
/// The input [`EMBEDDED_RUN_PRESET`]'s entry capabilities render (strict mode):
/// every `preset.input.*` the first two manual states read.
const EMBEDDED_RUN_INPUT: &str = r#"{"work_ref":"wrk-embedded-execution","topic":"embedded execution","work_id":"wrk_embedded_execution","open_findings":"[]"}"#;

/// The identity an embedded bundle resolves — `apply_id = "embedded:<preset_id>"`
/// and the canonical hash of the document `embedded_workflow_document`
/// constructs. Written here the way `resolve_applied_document` writes it: the
/// test pins the production value, it does not replace the production route.
fn embedded_run_identity(preset_id: &str) -> nexus_session_storage::run_state::AppliedRunIdentity {
    let caps = nexus_preset::capability_catalog::BuiltinCapabilityCatalog;
    let loaded =
        nexus_preset::load_embedded_preset(preset_id, &caps).expect("the embedded preset loads");
    let document =
        nexus_preset::embedded_workflow_document(&loaded).expect("the document constructs");
    nexus_session_storage::run_state::AppliedRunIdentity {
        apply_id: format!("embedded:{preset_id}"),
        content_hash: nexus_workflow_ir::content_hash(&document).expect("the document hashes"),
    }
}

/// Wait until an admitted run parks on a manual wait at `node`, answering that
/// wait's durable id.
async fn wait_for_manual_wait(pool: &sqlx::SqlitePool, session_id: &str, node: &str) -> String {
    wait_for_record(
        pool,
        session_id,
        "the embedded run to park on its entry manual wait",
        |record| {
            record.status == nexus_orchestration::engine::SessionStatus::WaitingForInput
                && record
                    .state
                    .as_ref()
                    .and_then(|state| state.wait.as_ref())
                    .is_some_and(|wait| wait.task_id == node)
        },
    )
    .await
    .state
    .and_then(|state| state.wait)
    .expect("the manual wait record exists")
    .wait_id
}

/// Admit `preset_id` through the production seam and park it on its entry manual
/// wait, asserting the frozen descriptor carries the embedded identity and the
/// run was positioned by the resolved document.
async fn admit_embedded_and_park(
    f: &Fixture,
    nexus_home: &Path,
    preset_id: &str,
) -> (nexus_orchestration::SessionId, String) {
    let owner = open_engine_owner(f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    let caps = handle.capability_holder();
    // The entry capability renders `preset.input.*` in strict mode, so the run is
    // admitted with exactly the input its document's first two manual states read.
    let session_id = handle
        .coordinator()
        .start_session(
            preset_id,
            CREATOR,
            Some(EMBEDDED_RUN_INPUT),
            std::collections::HashMap::new(),
            nexus_home,
            &caps,
            None,
            None,
        )
        .await
        .expect("an embedded preset admits through start_session");
    let pool: sqlx::SqlitePool = owner.pool().clone();
    let wait_id = wait_for_manual_wait(&pool, &session_id.0, EMBEDDED_ENTRY_NODE).await;

    let descriptor = read_restart_record(&pool, &session_id.0)
        .await
        .descriptor
        .expect("the frozen descriptor exists");
    assert_eq!(
        descriptor.applied,
        Some(embedded_run_identity(preset_id)),
        "admission must freeze the embedded document's D10 identity"
    );
    assert!(
        descriptor.intake_mode.is_none(),
        "the intake key governs user bundles only; an embedded run records no mode"
    );
    assert!(
        matches!(
            descriptor.source,
            nexus_preset::source_identity::PresetSourceIdentity::Embedded { .. }
        ),
        "an embedded run freezes its compiled-in source identity, got {:?}",
        descriptor.source
    );
    let session = read_restart_session(&pool, &session_id.0)
        .await
        .expect("the run row exists");
    assert_eq!(
        session.current_task_id, EMBEDDED_ENTRY_NODE,
        "the run's position must be a node of the resolved document"
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
    (session_id, wait_id)
}

/// The run input is the embedded document, and a restart resumes on the SAME
/// identity: recovery re-resolves the compiled-in bundle, re-constructs the
/// document and compares its identity against the frozen one before rebuilding.
///
/// Mutation this fails on: recovery passing `None` for an embedded applied run
/// (the pre-change arm) resolves no document, and the run keeps the descriptor
/// bundle. A resolver that recomputed the identity from a different source
/// (e.g. the twin bytes) would not equal the frozen hash and refuse here.
#[tokio::test]
async fn an_embedded_run_recovers_on_the_identity_admission_froze() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");

    // ── Generation 1: admit through the seam, park on the entry manual wait. ──
    let (session_id, wait_id) = admit_embedded_and_park(&f, &nexus_home, EMBEDDED_RUN_PRESET).await;

    // ── Generation 2: restart, then continue past the entry wait. ──
    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    handle
        .coordinator()
        .signal_run(
            &session_id,
            RunSignal::Continue {
                wait_id: wait_id.clone(),
            },
        )
        .await
        .expect("an unchanged embedded document must reconstruct and continue");

    let pool: sqlx::SqlitePool = owner.pool().clone();
    let record = wait_for_record(
        &pool,
        &session_id.0,
        "the recovered run to park on the document's next manual wait",
        |record| {
            record.status == nexus_orchestration::engine::SessionStatus::WaitingForInput
                && record
                    .state
                    .as_ref()
                    .and_then(|state| state.wait.as_ref())
                    .is_some_and(|wait| wait.task_id == EMBEDDED_NEXT_NODE)
        },
    )
    .await;
    assert_eq!(
        record.descriptor.and_then(|descriptor| descriptor.applied),
        Some(embedded_run_identity(EMBEDDED_RUN_PRESET)),
        "recovery must not rewrite the identity admission froze"
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
}

/// A changed embedded byte flips the document hash and the run stays
/// non-replayable — never rebuilt from the YAML descriptor.
///
/// The frozen identity is rewritten to a hash the compiled-in document no longer
/// produces, which is exactly what a binary upgrade between run and resume looks
/// like to recovery. Only the document identity is moved: the frozen SOURCE
/// identity still reloads, so the refusal must come from the applied-key
/// comparison (the harder direction), not from the source reload.
#[tokio::test]
async fn a_changed_embedded_document_refuses_recovery_instead_of_the_yaml_graph() {
    let f = fixture().await;
    let nexus_home = f.tmp.path().join(".nexus42");
    let (session_id, wait_id) = admit_embedded_and_park(&f, &nexus_home, EMBEDDED_RUN_PRESET).await;
    let frozen = embedded_run_identity(EMBEDDED_RUN_PRESET);

    // ── The bytes moved while no daemon owned the DB. ──
    let seed = open_seed_pool(&f.db_path).await;
    let bytes: Vec<u8> = sqlx::query_scalar(
        "SELECT run_descriptor_json FROM orchestration_sessions WHERE session_id = ?",
    )
    .bind(&session_id.0)
    .fetch_one(seed.pool())
    .await
    .expect("read the frozen descriptor bytes");
    let mut descriptor: serde_json::Value =
        serde_json::from_slice(&bytes).expect("the descriptor decodes");
    descriptor["applied"]["content_hash"] = serde_json::json!("blake3:changed-after-admission");
    let rewritten = serde_json::to_vec(&descriptor).expect("the descriptor encodes");
    sqlx::query("UPDATE orchestration_sessions SET run_descriptor_json = ? WHERE session_id = ?")
        .bind(rewritten)
        .bind(&session_id.0)
        .execute(seed.pool())
        .await
        .expect("rewrite the frozen descriptor");
    close_seed_pool(&f.db_path, seed).await;

    // ── Generation 2: the re-resolved identity no longer matches. ──
    let owner = open_engine_owner(&f).await;
    let handle = owner
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("owner starts");
    let refusal = handle
        .coordinator()
        .signal_run(
            &session_id,
            RunSignal::Continue {
                wait_id: wait_id.clone(),
            },
        )
        .await
        .expect_err("a changed embedded document must not be recovered from YAML");
    let RunControlError::ReconstructionUnavailable { reason, .. } = refusal else {
        panic!("expected reconstruction_unavailable, got {refusal:?}");
    };
    assert!(
        reason.contains(&frozen.apply_id),
        "the refusal names the embedded identity the run was admitted under: {reason}"
    );
    assert!(
        reason.contains(&frozen.content_hash),
        "the refusal names the document hash admission froze: {reason}"
    );
    assert!(
        reason.contains("blake3:changed-after-admission"),
        "the refusal names the identity recovery re-resolved: {reason}"
    );

    // The refusal advances nothing: same node, same human wait, same descriptor.
    let pool: sqlx::SqlitePool = owner.pool().clone();
    let record = read_restart_record(&pool, &session_id.0).await;
    assert_eq!(
        record.status,
        nexus_orchestration::engine::SessionStatus::WaitingForInput,
        "a refused reconstruction must leave the run on its preserved wait"
    );
    let session = read_restart_session(&pool, &session_id.0)
        .await
        .expect("the run row exists");
    assert_eq!(
        session.current_task_id, EMBEDDED_ENTRY_NODE,
        "a refused reconstruction must not advance the run; got {:?}",
        session.current_task_id
    );

    handle.close().await.unwrap();
    owner.close().await.unwrap();
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
        nexus_session_storage::run_state::RunStateV1::default(),
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
        nexus_session_storage::run_state::RunStateV1::default(),
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
///
/// It also pins the WAIT itself: while the registered build is held, the close
/// may not report a settled cleanup and may not release the pool — the drain is
/// fenced behind the start, so `cleanup_confirmed` can never precede the
/// builder's own shutdown.
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
        // `closing` is raised by the close in the caller's own step, so seeing
        // it proves this close has begun. Finishing, though, requires the drain
        // — and the drain takes the start fence exclusively, which this
        // registered build still holds. So "not finished" is not a race here:
        // it is the invariant under test.
        let closing_probe = Arc::clone(&owner);
        wait_until("close begins", move || {
            let closing_probe = Arc::clone(&closing_probe);
            async move { closing_probe.is_closing() }
        })
        .await;
        assert!(
            !close.is_finished(),
            "round {round}: close reported a settled cleanup while the registered build was \
             still in flight"
        );
        assert!(
            !owner.pool().is_closed(),
            "round {round}: close released the SQL pool under a registered build"
        );
        gate.release();

        let report = close.await.expect("close task joins");
        let started = start.await.expect("start task joins");
        assert!(
            report.unwrap().cleanup_confirmed,
            "round {round}: close must report a confirmed cleanup"
        );
        // The build was refused by the install-time double check — it may not
        // have installed anything into the closed service.
        assert!(
            matches!(started, Err(ExecutionOpenError::Closing)),
            "round {round}: a start racing a completed close must be refused as Closing"
        );
        assert!(
            owner.execution().is_none(),
            "round {round}: an owner survived concurrent close — orphan effect owner"
        );
        owner.close().await.unwrap();
    }
}

/// C3 (canceled build): a start whose caller is cancelled must not leave that
/// build's LIVE drives behind a confirmed close.
///
/// `build_execution` spawns the hosted clock and the drives `recovery`
/// re-drives, and dropping an `ExecutionHandle` stops none of them. The start
/// therefore runs on a retained task: the caller's cancellation only stops
/// waiting, and the task settles the owner it built — under the per-service
/// fence — when nobody is left to receive it.
///
/// The test drives that chain: it parks a recovered drive inside the gated
/// prompt executor, cancels the caller, then releases the build barrier so the
/// task's own settlement is the ONLY thing left. The close must stay
/// unconfirmed and the per-DB reservation held until the parked drive is
/// released — i.e. until the abandoned owner's drives really joined.
#[tokio::test]
async fn close_waits_for_a_canceled_builds_live_drives() {
    let f = fixture().await;
    // A run parked inside the `generate` LLM step: recovery re-drives it and
    // the gated executor holds that drive open.
    seed_run_at(
        &f.db_path,
        "test-preset:canceled",
        CREATOR,
        nexus_session_storage::run_state::RunStateV1::default(),
        "generate",
    )
    .await;

    let owner = Arc::new(open_engine_owner(&f).await);
    // A second core over the same DB (the daemon's co-host shape): it JOINS the
    // live engine admission, so it can observe the per-DB owner reservation
    // without disturbing the owner under test.
    let competitor = open_engine_owner(&f).await;
    let gate = GatedPromptExecutor::new();
    let build_gate = BuildGate::new();
    let start = tokio::spawn({
        let owner = Arc::clone(&owner);
        let gate = gate.clone();
        let build_gate = build_gate.clone();
        async move {
            owner
                .start_execution(
                    Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
                    RunnerDeps {
                        prompt_executor: Some(Arc::new(gate)),
                        build_observer: Some(Arc::new(build_gate)),
                        ..RunnerDeps::default()
                    },
                )
                .await
        }
    });

    // The build is complete (recovery ran, the drive is parked) and nothing has
    // been published into the per-service slot yet.
    build_gate.wait_entered().await;
    gate.wait_entered().await;

    let close = tokio::spawn({
        let owner = Arc::clone(&owner);
        async move { owner.close().await }
    });
    let closing_probe = Arc::clone(&owner);
    wait_until("close begins", move || {
        let closing_probe = Arc::clone(&closing_probe);
        async move { closing_probe.is_closing() }
    })
    .await;
    assert!(
        !close.is_finished(),
        "close must stay fenced behind the registered build"
    );

    // Cancel the caller: it stops waiting, and the retained start task is now
    // the only owner of what the build produced.
    start.abort();
    let _ = start.await;
    assert!(
        owner.execution().is_none(),
        "an abandoned build must not publish an owner into the closed service"
    );

    // Let the retained start finish its build: with no caller waiting, it
    // refuses the install and settles the owner it built — which joins the
    // parked drive, so its settlement is the ONLY thing left to finish.
    build_gate.release();
    // `wait_until` on a positive observable would need a probe inside the
    // settlement; the negative observation below is what the fence guarantees:
    // while the drive is parked, neither the close nor the DB may be released.
    assert!(
        !close.is_finished(),
        "close confirmed while the abandoned owner's drive was still parked"
    );
    let refused = competitor
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await;
    assert!(
        matches!(refused, Err(ExecutionOpenError::AlreadyOwned)),
        "the abandoned owner must hold the per-DB reservation until its drives join"
    );

    // Release the parked drive: NOW the abandoned owner settles, and only then
    // does the close confirm the cleanup and the DB open again.
    gate.release();
    let report = tokio::time::timeout(Duration::from_secs(10), close)
        .await
        .expect("close must confirm once the abandoned owner's drive joined")
        .expect("the close task joins")
        .expect("a close report");
    assert!(
        report.cleanup_confirmed,
        "the canceled build must not strand the close"
    );
    competitor.close().await.unwrap();
    // The DB is genuinely free: a fresh owner is admitted and can start.
    let fresh = open_engine_owner(&f).await;
    fresh
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await
        .expect("a canceled build must not fence its DB");
    fresh.close().await.unwrap();
    owner.close().await.unwrap();
}

/// C3 (unreceived hand-off): a caller dropped AFTER the owner was published but
/// BEFORE it ever polled the hand-off must leave no live owner behind.
///
/// A queued value is not a hand-over: the retained start publishes the owner and
/// queues it, but a caller dropped before its next poll takes that value with it
/// — and nobody would ever close that owner. The task therefore keeps the fence
/// until the caller ACKNOWLEDGES receipt, and settles the owner itself when the
/// acknowledgement can never come.
///
/// The ordering is forced deterministically with a caller future the runtime
/// does NOT own: it is polled exactly once (which spawns the retained start and
/// parks it on the hand-off), never polled again, and then dropped. A
/// current-thread runtime is pinned because the install and the queuing happen
/// in ONE poll of the retained task, and this test can only run once that poll
/// yielded (the task parks on the receipt) — so observing a published owner
/// while this caller is still unpolled PROVES the value is queued and
/// unreceived.
#[tokio::test(flavor = "current_thread")]
async fn a_caller_dropped_before_receiving_the_owner_settles_it() {
    let f = fixture().await;
    let owner = Arc::new(open_engine_owner(&f).await);
    // The caller side of the start: a local future, not a task, so this test
    // alone decides when — and whether — it is polled.
    let caller_core = (*owner).clone();
    let build_gate = BuildGate::new();
    let mut caller = Box::pin(caller_core.start_execution(
        Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
        RunnerDeps {
            build_observer: Some(Arc::new(build_gate.clone())),
            ..RunnerDeps::default()
        },
    ));

    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut caller)
            .await
            .is_err(),
        "the caller must park on the hand-off while the build is held"
    );
    build_gate.wait_entered().await;
    assert!(
        owner.execution().is_none(),
        "nothing may be published before the install"
    );

    build_gate.release();
    let published = Arc::clone(&owner);
    wait_until("the owner is published", move || {
        let published = Arc::clone(&published);
        async move { published.execution().is_some() }
    })
    .await;

    // Drop the caller without ever taking the owner the start wanted to hand it.
    drop(caller);
    let withdrawn = Arc::clone(&owner);
    wait_until("the unreceived owner is withdrawn and settled", move || {
        let withdrawn = Arc::clone(&withdrawn);
        async move { withdrawn.execution().is_none() }
    })
    .await;

    // The settlement is what the close waited for, and the DB is free again.
    assert!(
        owner.close().await.unwrap().cleanup_confirmed,
        "a settled unreceived owner must not block a confirmed close"
    );
    assert_fresh_owner_can_start(&f).await;
}

/// P1 (retired owner during the receipt wait): an owner RETIRED while the
/// retained start was still waiting for its receipt must keep the close fenced
/// until its drives really joined.
///
/// `retire_execution` takes the per-service slot and settles the handle it took,
/// and that slot never comes back. The receipt-failure settlement in the
/// retained start used to return the moment the slot no longer held ITS handle,
/// so the start fence dropped while the retirement was still joining drives — a
/// close that landed there drained an EMPTY slot and closed the pool over a
/// live owner whose per-DB reservation was still held. The retirement now holds
/// the SAME shared start fence for its whole duration and the receipt-failure
/// settlement JOINS that idempotent shutdown, so the close drain — exclusive
/// half of the fence — cannot report a confirmed cleanup before the drives are
/// gone.
///
/// Deterministic ordering, no timing assumption about who wins a race: the
/// caller is a local future polled exactly ONCE (so no receipt can ever
/// arrive), the recovered drive is parked inside the gated prompt executor, and
/// the test waits for the published owner to LEAVE the slot while the caller is
/// still unpolled — which proves the retirement, not the withdrawal, took it.
/// The pre-fix failure is then observable on a synchronous state: `Pool::close`
/// raises `is_closed` as its FIRST act, so an early drain shows a closed pool
/// without having waited for the parked drive.
#[tokio::test(flavor = "current_thread")]
async fn close_waits_for_a_retired_owner_during_the_receipt_wait() {
    let f = fixture().await;
    // A run parked inside the `generate` LLM step: recovery re-drives it and
    // the gated executor holds the drive open.
    seed_run_at(
        &f.db_path,
        "test-preset:retired",
        CREATOR,
        nexus_session_storage::run_state::RunStateV1::default(),
        "generate",
    )
    .await;

    let owner = Arc::new(open_engine_owner(&f).await);
    // A second core over the same DB (the daemon's co-host shape): it JOINS the
    // live engine admission, so it can probe the per-DB owner reservation
    // without disturbing the owner under test.
    let competitor = open_engine_owner(&f).await;
    let gate = GatedPromptExecutor::new();
    let build_gate = BuildGate::new();
    // The caller side of the start: a local future, not a task, so this test
    // alone decides when — and whether — it is polled. Its next poll would
    // acknowledge receipt, so it is never polled again.
    let caller_core = (*owner).clone();
    let mut caller = Box::pin(caller_core.start_execution(
        Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
        RunnerDeps {
            prompt_executor: Some(Arc::new(gate.clone())),
            build_observer: Some(Arc::new(build_gate.clone())),
            ..RunnerDeps::default()
        },
    ));

    assert!(
        tokio::time::timeout(Duration::from_millis(20), &mut caller)
            .await
            .is_err(),
        "the caller must park on the hand-off while the build is held"
    );
    build_gate.wait_entered().await;
    gate.wait_entered().await;

    // Install: release the build so the retained start publishes the owner and
    // parks on the receipt nobody will ever send.
    build_gate.release();
    let published = Arc::clone(&owner);
    wait_until("the owner is published", move || {
        let published = Arc::clone(&published);
        async move { published.execution().is_some() }
    })
    .await;

    // Retire it. The caller is still unpolled, so the ONLY taker of the slot is
    // the retirement — and its shutdown parks on the gated drive.
    let retire = tokio::spawn({
        let owner = Arc::clone(&owner);
        async move { owner.retire_execution().await }
    });
    let retired = Arc::clone(&owner);
    wait_until("the retirement takes the per-service slot", move || {
        let retired = Arc::clone(&retired);
        async move { retired.execution().is_none() }
    })
    .await;

    // Drop the caller: the receipt can never come, so the retained start
    // settles the owner it published — the very handle the retirement took.
    drop(caller);

    let close = tokio::spawn({
        let owner = Arc::clone(&owner);
        async move { owner.close().await }
    });
    let closing_probe = Arc::clone(&owner);
    wait_until("close begins", move || {
        let closing_probe = Arc::clone(&closing_probe);
        async move { closing_probe.is_closing() }
    })
    .await;

    // Outlast the interleaving this regression is about. Every task involved
    // runs on THIS runtime, so a drain that could still run (the pre-fix
    // retirement held no fence; the pre-fix withdrawal returned the fence
    // early) reaches `Pool::close` — and raises `is_closed` — well inside this
    // budget, without ever waiting for the parked drive.
    let budget = tokio::time::Instant::now() + Duration::from_secs(5);
    while !owner.pool().is_closed() && tokio::time::Instant::now() < budget {
        tokio::task::yield_now().await;
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    assert!(
        !owner.pool().is_closed(),
        "close released the SQL pool while the retired owner's drive was still parked"
    );
    assert!(
        !close.is_finished(),
        "close published a settled cleanup while the retired owner's drive was still parked"
    );
    assert!(
        !retire.is_finished(),
        "the retirement must still be joining the parked drive (deterministic hold)"
    );
    // The per-DB OWNER fence is still held: the retired owner is not settled.
    let refused = competitor
        .start_execution(
            Arc::clone(&NullProvider::new()) as Arc<dyn ProviderPort>,
            RunnerDeps::default(),
        )
        .await;
    assert!(
        matches!(refused, Err(ExecutionOpenError::AlreadyOwned)),
        "a retired-but-unsettled owner must keep the per-DB reservation"
    );

    // Release the parked drive: the retirement settles, the retained start's
    // settlement returns, and ONLY THEN may the close confirm.
    gate.release();
    let report = tokio::time::timeout(Duration::from_secs(10), close)
        .await
        .expect("close must confirm once the retired owner's drive joined")
        .expect("the close task joins")
        .expect("a close report");
    assert!(
        report.cleanup_confirmed,
        "the close must report a confirmed cleanup once the retired owner settled"
    );
    retire.await.expect("the retirement task joins");

    competitor.close().await.unwrap();
    owner.close().await.unwrap();
    // The fence is genuinely released: a fresh owner over the same home is
    // admitted and can start.
    assert_fresh_owner_can_start(&f).await;
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
