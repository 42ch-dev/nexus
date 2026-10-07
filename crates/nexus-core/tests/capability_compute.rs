//! P3-T3 capability/compute/peer contract.
//!
//! The acceptance anchor is
//! [`accept_happy_path_applies_atomically_and_creates_events`]: a compute run's
//! proposals are applied under the owner's authority and the CAS, in ONE
//! transaction, and a concurrent accept can never double-apply.
//!
//! The other cases protect the two refusals the extraction could most easily
//! have broken:
//!
//! - **Zero domain effect on an unauthorized call.** An unknown tool, or a
//!   tool whose arguments fail its declared schema, must leave the World
//!   EXACTLY as it was — no delta, no key block, no timeline event.
//! - **A real error for an unknown or unavailable operation.** A pool-less or
//!   engine-less context reports a typed refusal; it never silently succeeds
//!   and never reports a fabricated result.

#![cfg(all(feature = "execution", feature = "compute"))]
#![allow(clippy::unwrap_used)]

use nexus_contracts::generated::daemon_api::compute::{
    clear_runs_query::{ClearRunsQuery, ClearRunsQueryStatus},
    list_runs_query::{ListRunsQuery, ListRunsQueryStatus},
    run_accept_request::RunAcceptRequest,
    run_request::RunRequest,
};
use nexus_contracts::CreateWorkRequest;
use nexus_contracts::ToolsApi;
use nexus_core::execution::capabilities::{execute_tool, ToolContext, ToolRuntimeFacts};
use nexus_core::execution::compute::{
    accept_compute_run, clear_compute_runs, compute_run, discard_compute_run, get_compute_run,
    list_compute_runs, recover_stuck_compute_runs, ComputeContext, ComputeRunRecoveryOutcome,
};
use nexus_core::{CoreAccess, CoreError, CoreOpenOptions, CoreService, WorkPatchRequest};
use nexus_wasm_host::{CachedModule, ModuleCache, ModuleManifest, SandboxConfig, WasmEngine};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

const CREATOR: &str = "test_creator";
const SLUG: &str = "default";
const WORLD: &str = "wld_combat";
const FOREIGN_WORLD: &str = "wld_other";
const MODULE: &str = "basic-combat";

struct Fixture {
    tmp: TempDir,
    /// The engine-owner core the family APIs are driven through.
    core: CoreService,
    /// A tool context over the same pool for the capability entry points.
    context: ToolContext,
    /// The compute context (engine + cache + serializer) for run/accept.
    compute: ComputeContext,
}

/// An engine + cache with the embedded module warmed, plus any extra modules
/// the sandbox-limit fixtures need. `sandbox` overrides the engine defaults
/// (the wall-time watchdog cases).
fn engine_with(
    extra: &[(&str, ModuleManifest, Vec<u8>)],
    sandbox: Option<SandboxConfig>,
) -> (Arc<WasmEngine>, Arc<ModuleCache>) {
    let engine = Arc::new(sandbox.map_or_else(
        || WasmEngine::new().expect("wasm engine"),
        |config| WasmEngine::with_config(config).expect("wasm engine"),
    ));
    let cache = Arc::new(ModuleCache::new());
    cache.warm_embedded(&engine).expect("warm embedded module");
    for (id, manifest, bytes) in extra {
        let module = engine.load_module(bytes).expect("extra module compiles");
        cache.insert(
            *id,
            Arc::new(CachedModule {
                module,
                manifest: manifest.clone(),
                // The cache identity is (id, bytes_hash, manifest_hash) —
                // the entry must record the exact artifacts it was compiled
                // from.
                bytes_hash: nexus_wasm_host::hash_module_bytes(bytes),
                manifest_hash: nexus_wasm_host::hash_module_bytes(
                    &serde_json::to_vec(manifest).expect("manifest serializes"),
                ),
            }),
        );
    }
    (engine, cache)
}

/// Seed a workspace with the embedded combat module warmed.
async fn fixture() -> Fixture {
    fixture_with_compute(&[], None).await
}

/// The same fixture with extra cached modules and/or a sandbox override.
async fn fixture_with_compute(
    extra: &[(&str, ModuleManifest, Vec<u8>)],
    sandbox: Option<SandboxConfig>,
) -> Fixture {
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
    // The seeding guard must be fully DROPPED before the owner opens: the
    // writer protocol's engine admission is a process-wide registry keyed by
    // DB path, so opening a second engine while this pool still holds the
    // guard is an `OwnerBusy` refusal (the fixture is not racing another
    // test — it is racing its own seeding pool).
    {
        let guarded = nexus_local_db::init_engine_pool(&db_path)
            .await
            .expect("engine pool init");
        // The PRODUCTION creation path, which materializes the Creator's holder
        // registry row in the same transaction. The compute lane reads its input
        // through the admitted Creator `ActorView`, and that selection resolves
        // the stable holder (`require_creator_holder`, fail-closed), so a
        // hand-rolled `creators` insert leaves the registry row missing and
        // admission refuses with `holder_state_invalid` before any compute runs.
        nexus_local_db::ensure_creator_row(guarded.pool(), CREATOR, "Test")
            .await
            .expect("seed the admitted creator and its holder");
        seed_world(guarded.pool(), WORLD, CREATOR).await;
        guarded.pool().close().await;
        nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);
    }

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .expect("engine-owner core open");

    let (engine, cache) = engine_with(extra, sandbox);

    // Exactly one computable character, so a run against this world produces
    // one deterministic delta and one event.
    seed_character(core.pool(), WORLD, "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(core.pool(), WORLD, "kb_def", "Guardian", 10, 5, 30, 50).await;

    let context = ToolContext::new(
        core.pool().clone(),
        home,
        None,
        ToolRuntimeFacts {
            is_initialized: true,
            lifecycle_state: "Running".to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            ..ToolRuntimeFacts::default()
        },
        None,
        None,
    );

    let compute = ComputeContext {
        creator_id: CREATOR.to_string(),
        engine: Some(Arc::clone(&engine)),
        cache: Some(Arc::clone(&cache)),
        serializer: Arc::new(tokio::sync::Semaphore::new(1)),
    };

    Fixture {
        tmp,
        core,
        context,
        compute,
    }
}

/// Seed a world owned by `owner`.
async fn seed_world(pool: &sqlx::SqlitePool, world_id: &str, owner: &str) {
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES (?, 'ws', ?, 'Combat World', 'combat-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(world_id)
    .bind(owner)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a second world owned by a DIFFERENT creator, plus a KB entry in it —
/// the ownership-gate and foreign-delta-target fixture.
async fn seed_foreign_world(pool: &sqlx::SqlitePool) {
    nexus_local_db::ensure_creator_row(pool, "intruder", "Other")
        .await
        .unwrap();
    seed_world(pool, FOREIGN_WORLD, "intruder").await;
    seed_character(
        pool,
        FOREIGN_WORLD,
        "kb_foreign",
        "Stranger",
        99,
        99,
        777,
        777,
    )
    .await;
}

/// Seed one computable character entry in `world_id`.
#[allow(clippy::too_many_arguments)] // fixture helper: the four combat stats mirror the seeded schema columns
async fn seed_character(
    pool: &sqlx::SqlitePool,
    world_id: &str,
    entry_id: &str,
    name: &str,
    base_atk: i64,
    base_def: i64,
    current_hp: i64,
    max_hp: i64,
) {
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{
        KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeOwnerRef,
    };
    use nexus_knowledge::world_kb::KbStore;

    let kb = KnowledgeEntryRecord {
        entry_id: entry_id.to_string(),
        owner: KnowledgeOwnerRef::world(world_id),
        block_type: BlockType::Character,
        canonical_name: name.to_string(),
        body: Some(KnowledgeEntryBody {
            summary: Some(format!("{name} combatant")),
            attributes: Some(json!({
                "max_hp": max_hp,
                "base_atk": base_atk,
                "base_def": base_def,
            })),
            computable: Some(true),
            state: Some(json!({
                "character": { "current_hp": current_hp, "is_alive": true, "status_effects": [] }
            })),
            ..Default::default()
        }),
        ..KnowledgeEntryRecord::new(world_id, BlockType::Character, name)
    };
    nexus_local_db::kb_store::SqliteKbStore::new(pool.clone())
        .insert_knowledge_entry(kb)
        .await
        .unwrap();
}

/// Read a character's `current_hp`.
async fn defender_hp(pool: &sqlx::SqlitePool, entry_id: &str) -> i64 {
    let raw: Option<String> =
        sqlx::query_scalar("SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?")
            .bind(entry_id)
            .fetch_optional(pool)
            .await
            .unwrap()
            .flatten();
    let body: serde_json::Value = serde_json::from_str(&raw.expect("body present")).unwrap();
    body["state"]["character"]["current_hp"].as_i64().unwrap()
}

/// Count timeline events in the world.
async fn timeline_event_count(pool: &sqlx::SqlitePool) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM narrative_timeline_events WHERE world_id = ?")
        .bind(WORLD)
        .fetch_one(pool)
        .await
        .unwrap()
}

/// Run a successful compute and return its run id.
async fn run_succeeded(f: &Fixture, context: &ComputeContext) -> String {
    let request: RunRequest = serde_json::from_value(json!({
        "world_id": WORLD,
        "module_id": MODULE,
        "invocation_params": { "attacker_id": "kb_atk", "defender_id": "kb_def" },
    }))
    .unwrap();
    let response = nexus_core::execution::compute::compute_run(&f.core, context, request)
        .await
        .expect("compute run succeeds");
    // The generated status enum carries Display but not PartialEq.
    assert_eq!(response.status.to_string(), "succeeded");
    response.run_id
}

/// Deterministic provider that never performs an effect.
struct NullProvider;

#[async_trait::async_trait]
impl nexus_provider_ports::ProviderPort for NullProvider {
    async fn call(
        &self,
        _request: nexus_contracts::ProviderCall,
    ) -> nexus_provider_ports::ProviderResult<nexus_contracts::ProviderReply> {
        Err(nexus_contracts::CoreError {
            code: nexus_contracts::CoreErrorCode::Internal,
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
    ) -> nexus_provider_ports::ProviderResult<nexus_contracts::ProviderEventBatch> {
        Ok(nexus_contracts::ProviderEventBatch {
            operation_id,
            events: vec![],
            has_more: false,
            gap: None,
        })
    }
}

/// The acceptance anchor: accept applies the proposal AND creates its events,
/// atomically, under the owner's authority.
#[tokio::test]
#[serial_test::serial]
async fn accept_happy_path_applies_atomically_and_creates_events() {
    let f = fixture().await;
    let run_id = run_succeeded(&f, &f.compute).await;
    let principal = f.core.active_principal().await.unwrap();

    // Proposals alone changed nothing.
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 30);
    assert_eq!(timeline_event_count(f.core.pool()).await, 0);

    let request: RunAcceptRequest = serde_json::from_value(json!({})).unwrap();
    let response =
        nexus_core::execution::compute::accept_compute_run(&f.core, &principal, &run_id, request)
            .await
            .expect("accept succeeds");

    // The typed response reports exactly what the transaction did.
    assert_eq!(response.applied.state_delta_count, 1);
    assert_eq!(response.applied.events_created, 1);
    assert_eq!(response.timeline_event_ids.len(), 1);

    // And the domain really changed: damage = max(0, 20 − 5) = 15.
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 15);
    assert_eq!(defender_hp(f.core.pool(), "kb_atk").await, 100);
    assert_eq!(timeline_event_count(f.core.pool()).await, 1);
}

/// A second accept of the same run is refused, and the domain is NOT applied
/// twice — the CAS makes the write idempotent at the domain level.
#[tokio::test]
#[serial_test::serial]
async fn accept_is_refused_twice_and_double_apply_is_impossible() {
    let f = fixture().await;
    let run_id = run_succeeded(&f, &f.compute).await;
    let principal = f.core.active_principal().await.unwrap();

    let request: RunAcceptRequest = serde_json::from_value(json!({})).unwrap();
    nexus_core::execution::compute::accept_compute_run(
        &f.core,
        &principal,
        &run_id,
        request.clone(),
    )
    .await
    .expect("first accept succeeds");
    let hp_after_first = defender_hp(f.core.pool(), "kb_def").await;
    let events_after_first = timeline_event_count(f.core.pool()).await;

    let second =
        nexus_core::execution::compute::accept_compute_run(&f.core, &principal, &run_id, request)
            .await
            .unwrap_err();
    assert!(
        matches!(second, nexus_core::CoreError::Coded { ref code, .. } if code == "conflict"),
        "a second accept must be a coded conflict, got {second:?}"
    );

    // The domain is untouched by the refused accept.
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, hp_after_first);
    assert_eq!(
        timeline_event_count(f.core.pool()).await,
        events_after_first
    );
}

/// An unknown tool is refused with the retained code and ZERO domain effect.
#[tokio::test]
#[serial_test::serial]
async fn unknown_tool_is_refused_with_zero_domain_effect() {
    let f = fixture().await;
    let before = timeline_event_count(f.core.pool()).await;

    let request = ToolsApi {
        tool_name: "nexus.does.not.exist".to_string(),
        parameters: json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let err = nexus_core::execution::capabilities::execute_tool(&f.context, &request)
        .await
        .unwrap_err();
    assert!(
        matches!(err, nexus_core::CoreError::Coded { ref code, .. } if code == "not_supported"),
        "an unknown tool must report the retained not_supported code, got {err:?}"
    );

    assert_eq!(
        timeline_event_count(f.core.pool()).await,
        before,
        "a refused dispatch must have zero domain effect"
    );
}

/// Stage an admitted user-capability trio (AR-35 layout) at
/// `<scan_root>/<name>/`: a hash-consistent `capability.json` +
/// `manifest.json` + `<module-id>.wasm` bundle so the AR-43 admission gates
/// pass inside the scan — the ONLY user-capability admission path. The bytes
/// are not real wasm, so an engine-less registry serves the AR-44
/// `WorkerUnavailable` stub for the name.
fn write_user_capability_trio(scan_root: &Path, name: &str, input_schema: &str) {
    let dir = scan_root.join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let wasm = b"fake module bytes";
    let sha = {
        use sha2::{Digest, Sha256};
        use std::fmt::Write as _;
        let mut hex = String::with_capacity(64);
        for b in Sha256::digest(wasm) {
            let _ = write!(hex, "{b:02x}");
        }
        hex
    };
    std::fs::write(
        dir.join("capability.json"),
        format!(
            r#"{{
                "name": "{name}",
                "inputSchema": {input_schema},
                "outputSchema": "{{\"type\":\"object\"}}",
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
            }}"#,
            input_schema = serde_json::to_string(input_schema).unwrap(),
        ),
    )
    .unwrap();
    std::fs::write(
        dir.join("manifest.json"),
        format!(
            r#"{{
                "module_id": "basic-combat",
                "name": "Basic Combat",
                "version": "1.0.0",
                "nexus_abi_version": 1,
                "required_key_block_types": [],
                "compute_export": "compute",
                "init_export": "",
                "wasm_sha256": "{sha}"
            }}"#
        ),
    )
    .unwrap();
    std::fs::write(dir.join("basic-combat.wasm"), wasm).unwrap();
}

/// A user capability's declared schema gates its arguments: a call missing a
/// required property is refused BEFORE the capability runs.
///
/// The capability is written to a scan directory (the only admission path —
/// user capabilities are never injected directly), so this also proves the
/// registry the dispatch spine reads is the SCANNED one.
#[tokio::test]
#[serial_test::serial]
async fn schema_invalid_arguments_never_reach_the_capability() {
    use nexus_orchestration::capability::{CapabilityRegistry, CapabilityRuntimeDeps};

    let f = fixture().await;
    // The descriptor contract (AR-34) requires a `wasm` module ref: without it
    // the scanner skips the directory rather than admitting a capability with
    // no executor. The scan is also the ONLY admission path — user
    // capabilities are never injected directly — so this fixture writes the
    // same bundle shape the daemon's boot scan consumes.
    // The scanner's ROOT is the directory that CONTAINS the capability dirs
    // (the daemon passes `nexus_home_layout::user_capabilities_dir`, which is
    // already `<home>/.nexus42/capabilities`), so the bundle is written
    // directly under it — an extra `capabilities/` level makes the scanner
    // look for `<root>/<name>/capability.json` and find nothing.
    let scan_root = f.tmp.path().join("usercaps");
    write_user_capability_trio(
        &scan_root,
        "t3.requires.thing",
        r#"{"type":"object","required":["thing"],"properties":{"thing":{"type":"string"}}}"#,
    );

    let deps = CapabilityRuntimeDeps {
        pool: Some(f.core.pool().clone()),
        prompt_executor: None,
        session_cancels: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: None,
    };
    let (registry, outcome) =
        CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_root);
    {
        use nexus_orchestration::capability::Capability as _;
        assert!(
            outcome
                .admitted
                .iter()
                .any(|c| c.name() == "t3.requires.thing"),
            "the fixture capability must be admitted: skipped={:?}",
            outcome.skipped
        );
    }

    let mut context = f.context.clone();
    context.set_user_capabilities(Some(
        nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(registry)),
    ));

    let request = ToolsApi {
        tool_name: "t3.requires.thing".to_string(),
        // `thing` is missing, so the declared schema refuses the call before
        // the capability's `run` is ever reached.
        parameters: json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let err = nexus_core::execution::capabilities::execute_tool(&context, &request)
        .await
        .unwrap_err();
    assert!(
        matches!(err, nexus_core::CoreError::Coded { ref code, .. } if code == "invalid_input"),
        "a schema-invalid call must be an invalid_input refusal, got {err:?}"
    );
}

/// The dispatch spine resolves through the LIVE published registry: after a
/// hot reload drops a capability from the holder, the next dispatch of that
/// name is refused exactly like an unknown id (the retained `not_supported`
/// code) with zero domain effect, while its admitted sibling keeps
/// dispatching on the same context.
///
/// MIGRATED (v1.193 P2-T11) from
/// `crates/nexus-daemon-runtime/tests/capability_hot_reload_journey.rs`
/// `catalog_routes_reflect_add_and_remove_within_bounded_interval` — the
/// core-observable half of the retired journey. The old case drove the two
/// daemon HTTP catalog routes over a `WorkspaceState`; the routes and the
/// daemon-owned holder retire with the host, and the watcher's merge/removal
/// rules stay in `nexus-orchestration::capability::watch`
/// (`deleted_scan_dir_drops_user_cap_names`,
/// `merge_carries_last_good_on_skipped_and_drops_absent`) — not duplicated
/// here. What is asserted is the consumer contract that made the journey
/// observable: `ToolContext::user_capabilities()` re-reads the holder on
/// EVERY dispatch, so a removal is honored by the very next call and a fresh
/// dispatch is refused on the spine-refusal basis.
#[tokio::test]
#[serial_test::serial]
async fn hot_removed_user_capability_is_refused_with_zero_domain_effect() {
    use nexus_orchestration::capability::{CapabilityRegistry, CapabilityRuntimeDeps};
    use nexus_orchestration::CapabilityRegistryHolder;

    let f = fixture().await;
    let scan_root = f.tmp.path().join("hotcaps");
    write_user_capability_trio(&scan_root, "hot.kept", r#"{"type":"object"}"#);
    write_user_capability_trio(&scan_root, "hot.removed", r#"{"type":"object"}"#);

    let deps = CapabilityRuntimeDeps {
        pool: Some(f.core.pool().clone()),
        prompt_executor: None,
        session_cancels: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: None,
    };
    let (registry, outcome) =
        CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_root);
    assert!(
        outcome.skipped.is_empty(),
        "no skips: {:?}",
        outcome.skipped
    );
    assert_eq!(outcome.admitted.len(), 2, "both trio dirs are admitted");

    let holder = CapabilityRegistryHolder::with_registry(Arc::new(registry));
    let mut context = f.context.clone();
    context.set_user_capabilities(Some(holder.clone()));

    let before = timeline_event_count(f.core.pool()).await;

    // Both admitted names resolve through the published holder: the
    // engine-less stub reports its honest no-executor failure, never the
    // unknown-id `not_supported`.
    for name in ["hot.kept", "hot.removed"] {
        let err = execute_tool(&context, &tool_request(name, json!({})))
            .await
            .unwrap_err();
        assert!(
            !matches!(&err, CoreError::Coded { code, .. } if code == "not_supported"),
            "{name} is admitted, so it must resolve before the reload, got {err:?}"
        );
    }

    // The hot reload itself: the directory is deleted and a fresh registry
    // generation is swapped into the SAME holder — what the retired watcher
    // did on a changed digest.
    std::fs::remove_dir_all(scan_root.join("hot.removed")).unwrap();
    let (rebuilt, rebuilt_outcome) =
        CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_root);
    assert!(
        rebuilt_outcome.skipped.is_empty(),
        "no skips: {:?}",
        rebuilt_outcome.skipped
    );
    assert_eq!(
        rebuilt_outcome.admitted.len(),
        1,
        "only the surviving trio is admitted after the reload"
    );
    holder.swap(Arc::new(rebuilt));

    // The removed name is refused like any unknown id, with the retained code
    // and zero domain effect…
    let err = execute_tool(&context, &tool_request("hot.removed", json!({})))
        .await
        .unwrap_err();
    assert!(
        matches!(&err, CoreError::Coded { code, .. } if code == "not_supported"),
        "a hot-removed capability must be refused as not_supported, got {err:?}"
    );
    // …while the admitted sibling keeps dispatching through the same context.
    let sibling = execute_tool(&context, &tool_request("hot.kept", json!({})))
        .await
        .unwrap_err();
    assert!(
        !matches!(&sibling, CoreError::Coded { code, .. } if code == "not_supported"),
        "the admitted sibling must keep resolving after the swap, got {sibling:?}"
    );

    assert_eq!(
        timeline_event_count(f.core.pool()).await,
        before,
        "a refused dispatch must have zero domain effect"
    );
}

/// A capability never fabricates success when its prompt executor fails: the
/// executor's typed error is surfaced out of `run()` verbatim.
///
/// MIGRATED (v1.193 P2-T11) from
/// `crates/nexus-daemon-runtime/tests/daemon_boot_llm_wiring.rs`
/// `executor_failure_stays_typed_failure` — the one retained real-failure
/// assertion of that retired boot-wiring fixture. Its siblings pinned the
/// boot composition shape (`CapabilityRuntimeDeps.prompt_executor` →
/// `CapabilityRegistry::with_runtime_deps`), whose no-executor and mock-success
/// halves are already owned by
/// `nexus-orchestration::capability::builtins::llm_extract`
/// (`llm_extract_standalone_returns_unavailable`,
/// `llm_extract_with_mock_executor_returns_candidates`). The production
/// consumer of the preserved contract is `quality_loop::run_llm_extract`,
/// which turns any non-`WorkerUnavailable` capability failure into a refusal
/// rather than standing in with heuristic candidates.
#[tokio::test]
async fn failing_prompt_executor_is_a_typed_failure_not_a_fabricated_success() {
    use nexus_orchestration::capability::{
        CapabilityError, CapabilityRegistry, CapabilityRuntimeDeps, PromptExecutor, PromptRequest,
        PromptResult,
    };

    struct FailingExecutor;

    #[async_trait::async_trait]
    impl PromptExecutor for FailingExecutor {
        async fn execute(&self, _request: PromptRequest) -> Result<PromptResult, CapabilityError> {
            Err(CapabilityError::TransientExternal(
                "agent refused the request".to_string(),
            ))
        }
    }

    // The run identity must have a registered coordinator token (the
    // fail-closed contract); a fresh token would be uncancellable.
    let session_cancels = Arc::new(std::sync::RwLock::new(std::collections::HashMap::from([(
        "extract-run".to_string(),
        tokio_util::sync::CancellationToken::new(),
    )])));
    let deps = CapabilityRuntimeDeps {
        pool: None,
        prompt_executor: Some(Arc::new(FailingExecutor) as Arc<dyn PromptExecutor>),
        session_cancels,
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: None,
    };
    let registry = CapabilityRegistry::with_runtime_deps(&deps);
    let cap = registry
        .get("nexus.llm.extract")
        .expect("nexus.llm.extract is a builtin");

    let err = cap
        .run(json!({
            "prompt": "extract entities",
            "chapter_prose": "Lin Xia drew her blade.",
            "_creator_id": "test_creator",
            "_session_id": "extract-run",
            // The trusted target/source the orchestration caller resolves
            // from stored state before the model runs.
            "_extract_target": {
                "world_id": "wld_wiring",
                "holder_entry_id": null,
                "disclosure": null,
            },
            "_extract_source_id": "ch02",
        }))
        .await
        .unwrap_err();
    match err {
        CapabilityError::TransientExternal(msg) => {
            assert!(msg.contains("refused"), "the executor error is kept: {msg}");
        }
        other => panic!("a failing executor must stay a typed failure, got: {other:?}"),
    }
}

/// An engine-less compute context reports a real error, never a fabricated
/// success.
#[tokio::test]
#[serial_test::serial]
async fn compute_without_an_engine_reports_a_real_error() {
    let f = fixture().await;
    let request: RunRequest = serde_json::from_value(json!({
        "world_id": WORLD,
        "module_id": MODULE,
        "invocation_params": { "attacker_id": "kb_atk", "defender_id": "kb_def" },
    }))
    .unwrap();

    // A context with no engine: the run must refuse, not pretend.
    let bare = ComputeContext {
        creator_id: CREATOR.to_string(),
        engine: None,
        cache: None,
        serializer: Arc::new(tokio::sync::Semaphore::new(1)),
    };
    let err = nexus_core::execution::compute::compute_run(&f.core, &bare, request)
        .await
        .unwrap_err();
    assert!(
        matches!(err, nexus_core::CoreError::Internal { .. }),
        "compute without an engine must be a real internal refusal, got {err:?}"
    );

    // And the world is untouched.
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 30);
    assert_eq!(timeline_event_count(f.core.pool()).await, 0);
}

// ---------------------------------------------------------------------------
// L2 review regressions (C1/C2/C3)
// ---------------------------------------------------------------------------

/// C1: a principal from ANOTHER core must be refused before any dispatch
/// effect.
///
/// The two halves of the authority (a principal, and a handle) are
/// independently obtainable, so a handle must prove the principal belongs to
/// the service that established it. The refusal must land BEFORE the handler
/// runs — so the audit row stays unwritten and the domain is untouched.
#[tokio::test]
#[serial_test::serial]
async fn execute_tool_refuses_a_principal_from_another_core() {
    let f = fixture().await;
    let handle = f
        .core
        .start_execution(
            Arc::new(NullProvider) as Arc<dyn nexus_provider_ports::ProviderPort>,
            nexus_core::execution::RunnerDeps {
                nexus_home: Some(f.tmp.path().join(".nexus42")),
                ..nexus_core::execution::RunnerDeps::default()
            },
        )
        .await
        .expect("owner starts");

    // A SECOND core over a DIFFERENT creator: its principal is well-formed but
    // was minted by another service, so this handle must not accept it.
    let other_home = f.tmp.path().join("other");
    let other_nexus = other_home.join(".nexus42");
    std::fs::create_dir_all(&other_nexus).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &other_home,
        "intruder",
        SLUG,
    ))
    .unwrap();
    std::fs::write(
        other_nexus.join("config.toml"),
        "active_creator_id = \"intruder\"\n[active_workspace_slug_by_creator]\n\"intruder\" = \"default\"\n",
    )
    .unwrap();
    let other_db = nexus_home_layout::workspace_state_db_path(&other_home, "intruder", SLUG);
    {
        let guarded = nexus_local_db::init_engine_pool(&other_db).await.unwrap();
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, \
             cached_at, data) VALUES ('intruder', 'Other', 'active', datetime('now'), '{}')",
        )
        .execute(guarded.pool())
        .await
        .unwrap();
        guarded.pool().close().await;
        nexus_local_db::writer_protocol::release_retained_writer_guards(&other_db);
    }
    let other_core = CoreService::open(CoreOpenOptions {
        user_home: other_home,
        access: CoreAccess::EngineOwner,
    })
    .await
    .expect("second core opens");
    let foreign = other_core.active_principal().await.unwrap();

    let request = ToolsApi {
        tool_name: "nexus.workspace.info".to_string(),
        parameters: json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let err = handle.execute_tool(&foreign, request).await.unwrap_err();
    assert!(
        matches!(err, nexus_core::CoreError::AuthRequired),
        "a foreign principal must be refused as AuthRequired, got {err:?}"
    );

    // The refusal precedes every effect: no audit row was written.
    let audits: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM acp_tool_audit_log")
        .fetch_one(f.core.pool())
        .await
        .unwrap();
    assert_eq!(audits, 0, "a refused principal must leave no audit row");
}

/// The owner's OWN principal still dispatches (the binding must not be a
/// blanket refusal).
#[tokio::test]
#[serial_test::serial]
async fn execute_tool_accepts_the_owners_own_principal() {
    let f = fixture().await;
    let handle = f
        .core
        .start_execution(
            Arc::new(NullProvider) as Arc<dyn nexus_provider_ports::ProviderPort>,
            nexus_core::execution::RunnerDeps {
                nexus_home: Some(f.tmp.path().join(".nexus42")),
                ..nexus_core::execution::RunnerDeps::default()
            },
        )
        .await
        .expect("owner starts");
    let principal = f.core.active_principal().await.unwrap();

    let request = ToolsApi {
        tool_name: "nexus.workspace.info".to_string(),
        parameters: json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let response = handle
        .execute_tool(&principal, request)
        .await
        .expect("the owner's own principal dispatches");
    assert!(response.success);
}

/// C2: `nexus.research.query` is creator-scoped.
///
/// Both the direct-id lookup and the list must be scoped, and a foreign row
/// must be indistinguishable from a missing one (`NotFound`, never Forbidden —
/// a 403 would confirm the row exists).
#[tokio::test]
#[serial_test::serial]
async fn research_query_is_scoped_to_the_creator() {
    let f = fixture().await;

    // Seed a reference source owned by ANOTHER creator, straight into the
    // durable store the fixture's pool serves.
    sqlx::query(
        "INSERT OR IGNORE INTO reference_sources \
         (reference_source_id, creator_id, workspace_id, title, uri, source_type, \
          tags, scan_status, created_at, updated_at) \
         VALUES ('ref_foreign', 'intruder', 'local', 'Foreign Source', \
                 'https://example.invalid/f', 'web', '', 'pending', \
                 datetime('now'), datetime('now'))",
    )
    .execute(f.core.pool())
    .await
    .unwrap();

    // Direct id lookup of a FOREIGN row: NotFound, and no existence leak.
    let request = ToolsApi {
        tool_name: "nexus.research.query".to_string(),
        parameters: json!({ "reference_source_id": "ref_foreign" }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let err = nexus_core::execution::capabilities::execute_tool(&f.context, &request)
        .await
        .unwrap_err();
    assert!(
        matches!(err, nexus_core::CoreError::NotFound { .. }),
        "a foreign reference source must be NotFound, got {err:?}"
    );

    // The LIST must not surface the foreign row either.
    let request = ToolsApi {
        tool_name: "nexus.research.query".to_string(),
        parameters: json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let value = nexus_core::execution::capabilities::execute_tool(&f.context, &request)
        .await
        .expect("list succeeds");
    let ids: Vec<String> = value["results"]
        .as_array()
        .expect("results array")
        .iter()
        .filter_map(|r| r["reference_source_id"].as_str().map(ToString::to_string))
        .collect();
    assert!(
        !ids.contains(&"ref_foreign".to_string()),
        "an unscoped list would leak a foreign row, got {ids:?}"
    );
}

/// C3: `nexus.reference.refresh` is a WRITE tool.
///
/// It writes DB state, rewrites body.md and performs a network fetch, so a
/// policy that grants reads only must refuse it — and the refusal must land
/// before any of those effects.
#[tokio::test]
#[serial_test::serial]
async fn reference_refresh_is_refused_under_a_read_only_policy() {
    let f = fixture().await;

    // A policy that grants READS (via `nexus.*.read`) but not writes. With
    // `default = "ask"` the two paths differ observably: the read predicate
    // consults `nexus.*.read` and grants, while the write predicate consults
    // only the tool name and `nexus.*` and refuses. Under `default = "deny"`
    // BOTH would refuse, so this test could not tell the fix from the bug.
    let workspace_dir = f.tmp.path().join("policy-ws");
    std::fs::create_dir_all(workspace_dir.join(".nexus42")).unwrap();
    std::fs::write(
        workspace_dir.join(".nexus42").join("permissions.toml"),
        "default = \"ask\"\n[grant]\n\"nexus.*.read\" = true\n",
    )
    .unwrap();

    let mut context = f.context.clone();
    context.set_workspace_path(Some(workspace_dir.to_string_lossy().into_owned()));

    let request = ToolsApi {
        tool_name: "nexus.reference.refresh".to_string(),
        parameters: json!({ "reference_source_id": "ref_any" }),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let err = nexus_core::execution::capabilities::execute_tool(&context, &request)
        .await
        .unwrap_err();
    assert!(
        matches!(err, nexus_core::CoreError::Coded { ref code, .. } if code == "policy_blocked"),
        "a read-only policy must refuse a write tool, got {err:?}"
    );

    // The refusal precedes the effects: the seeded row is untouched.
    let status: Option<String> = sqlx::query_scalar(
        "SELECT scan_status FROM reference_sources WHERE reference_source_id = 'ref_any'",
    )
    .fetch_optional(f.core.pool())
    .await
    .unwrap();
    assert!(status.is_none(), "no refresh effect may have run");

    // CONTROL: the same policy still ADMITS a read tool, so the refusal above
    // is the write classification and not the policy denying everything.
    let request = ToolsApi {
        tool_name: "nexus.workspace.info".to_string(),
        parameters: json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    nexus_core::execution::capabilities::execute_tool(&context, &request)
        .await
        .expect("a read tool still passes under the same policy");
}

// ---------------------------------------------------------------------------
// v1.193 P2-T9 — MIGRATED from `nexus-daemon-runtime/tests/agent_tool_api.rs`
// and `agent_tool_production_wiring.rs`.
//
// The daemon's `HostToolExecutor` was a thin adapter: it built this crate's
// `ToolContext` from its `WorkspaceState` and called the core
// `execute_tool` spine, which is where the admission gates, the handlers, the
// read/write policy and the audit row actually live. These cases therefore
// drive the core entry the daemon wrapped; the adapter's own HTTP
// composition retires with the host.
// ---------------------------------------------------------------------------

/// A `nexus.*` dispatch request with no session/request identity.
fn tool_request(tool_name: &str, parameters: serde_json::Value) -> ToolsApi {
    ToolsApi {
        tool_name: tool_name.to_string(),
        parameters,
        session_id: None,
        request_id: None,
        caller_kind: None,
    }
}

/// A tool context over the fixture pool that also carries the owner core —
/// the `nexus.work.patch` handler needs the family authority.
fn context_with_core(f: &Fixture) -> ToolContext {
    ToolContext::new(
        f.core.pool().clone(),
        f.tmp.path().join(".nexus42"),
        None,
        ToolRuntimeFacts {
            is_initialized: true,
            lifecycle_state: "Running".to_string(),
            started_at: chrono::Utc::now().to_rfc3339(),
            ..ToolRuntimeFacts::default()
        },
        Some(Arc::new(f.core.clone())),
        None,
    )
}

/// Create a Work owned by the fixture creator, bound to the fixture world.
async fn seed_work(f: &Fixture) -> String {
    let principal = f.core.active_principal().await.unwrap();
    let request: CreateWorkRequest = serde_json::from_value(json!({
        "title": "Test Work",
        "long_term_goal": "Goal",
        "initial_idea": "Idea",
        "world_id": WORLD,
    }))
    .unwrap();
    f.core
        .create_work(&principal, request)
        .await
        .expect("create work")
        .work_id
}

/// Audit outcomes for one tool, oldest first.
async fn audit_outcomes(pool: &sqlx::SqlitePool, tool_name: &str) -> Vec<String> {
    sqlx::query_scalar("SELECT outcome FROM acp_tool_audit_log WHERE tool_name = ? ORDER BY id")
        .bind(tool_name)
        .fetch_all(pool)
        .await
        .unwrap()
}

/// `nexus.context.whoami` and `nexus.workspace.info` report the active
/// creator/workspace and the honest runtime mode.
///
/// MIGRATED from `agent_tool_api.rs::whoami_returns_active_creator_id` and
/// `workspace_info_returns_workspace_slug`.
#[tokio::test]
#[serial_test::serial]
async fn nexus_tool_surface_reports_the_active_creator_and_workspace() {
    let f = fixture().await;

    let whoami = execute_tool(&f.context, &tool_request("nexus.context.whoami", json!({})))
        .await
        .expect("whoami succeeds");
    assert_eq!(whoami["creator_id"], CREATOR);
    assert_eq!(whoami["workspace_slug"], SLUG);

    let info = execute_tool(&f.context, &tool_request("nexus.workspace.info", json!({})))
        .await
        .expect("workspace info succeeds");
    assert_eq!(info["creator_id"], CREATOR);
    assert_eq!(info["workspace_slug"], SLUG);
    // The fixture supplies no more than `is_initialized`, so the honest mode is
    // the local-only default — never a fabricated running host.
    assert_eq!(info["runtime_mode"], "local_only");
}

/// `nexus.work.get` returns the stored stage fields for the caller's OWN work,
/// and collapses a foreign or unknown work into the SAME `forbidden` refusal
/// (existence is never leaked), while a missing parameter is `invalid_input`.
///
/// MIGRATED from `agent_tool_api.rs::work_get_happy_path_returns_work_stage_fields`,
/// `work_get_cross_creator_returns_forbidden`,
/// `error_code_forbidden_for_missing_work` and
/// `error_code_invalid_input_for_missing_params`.
#[tokio::test]
#[serial_test::serial]
async fn nexus_work_get_is_creator_scoped() {
    let f = fixture().await;
    let work_id = seed_work(&f).await;

    let value = execute_tool(
        &f.context,
        &tool_request("nexus.work.get", json!({ "work_id": work_id })),
    )
    .await
    .expect("the owner reads its own work");
    assert_eq!(value["work_id"], work_id);
    assert_eq!(value["title"], "Test Work");
    // The stored stage fields, not a fabricated default.
    assert_eq!(value["current_stage"], "intake");
    assert_eq!(value["stage_status"], "pending");
    // The Work DTO carries no `creator_id` (privacy filter).
    assert!(
        value.get("creator_id").is_none(),
        "the Work DTO must not leak creator_id: {value}"
    );

    // A row that EXISTS but belongs to another creator is indistinguishable
    // from a missing one — both are `forbidden`, never `not_found`.
    sqlx::query("UPDATE works SET creator_id = 'intruder' WHERE work_id = ?")
        .bind(&work_id)
        .execute(f.core.pool())
        .await
        .unwrap();
    let foreign = execute_tool(
        &f.context,
        &tool_request("nexus.work.get", json!({ "work_id": work_id })),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(foreign, CoreError::Forbidden { .. }),
        "a foreign work must be forbidden, got {foreign:?}"
    );

    let missing = execute_tool(
        &f.context,
        &tool_request("nexus.work.get", json!({ "work_id": "wrk_nonexistent" })),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(missing, CoreError::Forbidden { .. }),
        "an unknown work must be forbidden, got {missing:?}"
    );

    let unparametrized = execute_tool(&f.context, &tool_request("nexus.work.get", json!({}))).await;
    assert!(
        matches!(unparametrized, Err(CoreError::InvalidInput { .. })),
        "a missing work_id must be invalid_input, got {unparametrized:?}"
    );
}

/// `nexus.work.patch` appends inspiration, accepts only the declared field
/// allowlist, and refuses a stage-control field, an unknown field, an unknown
/// `stage_metadata` sub-key or a non-object `stage_metadata`.
///
/// MIGRATED from `agent_tool_api.rs::work_patch_append_inspiration_happy_path`,
/// `work_patch_rejects_current_stage_field`, `stage_metadata_accepts_allowed_keys`,
/// `stage_metadata_rejects_disallowed_sub_key`,
/// `stage_metadata_rejects_unknown_sub_key` and `stage_metadata_rejects_non_object`.
#[tokio::test]
#[serial_test::serial]
async fn nexus_work_patch_enforces_the_declared_field_allowlist() {
    let f = fixture().await;
    let work_id = seed_work(&f).await;
    let context = context_with_core(&f);

    let patched = execute_tool(
        &context,
        &tool_request(
            "nexus.work.patch",
            json!({
                "work_id": work_id,
                "inspiration_log": [{ "text": "Agent had an idea", "source": "acp_agent" }],
            }),
        ),
    )
    .await
    .expect("an inspiration append is admitted");
    assert_eq!(patched["work_id"], work_id);
    let inspiration = patched["inspiration_log"]
        .as_array()
        .expect("inspiration_log array");
    assert_eq!(
        inspiration.last().expect("appended entry")["note"],
        "Agent had an idea"
    );

    // The allowed `stage_metadata` sub-keys are stored as a metadata note.
    execute_tool(
        &context,
        &tool_request(
            "nexus.work.patch",
            json!({
                "work_id": work_id,
                "stage_metadata": {
                    "agent_notes": "some notes",
                    "research_summary_ref": "ref://123",
                },
            }),
        ),
    )
    .await
    .expect("allowed stage_metadata keys are admitted");

    // A stage-control field is refused as invalid_input — the work-patch lane
    // must never advance the stage.
    for refused in [
        json!({ "work_id": work_id, "current_stage": "writing" }),
        json!({ "work_id": work_id, "stage_metadata": { "current_stage": "writing" } }),
        json!({ "work_id": work_id, "stage_metadata": { "malicious_field": "evil" } }),
        json!({ "work_id": work_id, "stage_metadata": "not-an-object" }),
    ] {
        let err = execute_tool(&context, &tool_request("nexus.work.patch", refused.clone()))
            .await
            .unwrap_err();
        assert!(
            matches!(err, CoreError::Coded { ref code, .. } if code == "invalid_input")
                || matches!(err, CoreError::InvalidInput { .. }),
            "patch {refused} must be refused as invalid_input, got {err:?}"
        );
    }
}

/// `nexus.orchestration.schedule_status` returns the Work's linked schedules —
/// including while the Work is completion-locked, because that lock gates
/// WRITES and a read-only tool must not be blocked by it.
///
/// MIGRATED from `agent_tool_api.rs::schedule_status_happy_path` and
/// `agent_tool_production_wiring.rs::agent_tool_e2e_read_only_tool_succeeds_under_completion_lock`.
#[tokio::test]
#[serial_test::serial]
async fn nexus_schedule_status_reads_a_completion_locked_work() {
    let f = fixture().await;
    let work_id = seed_work(&f).await;
    let principal = f.core.active_principal().await.unwrap();

    f.core
        .patch_work(
            &principal,
            work_id.clone(),
            "test",
            WorkPatchRequest {
                schedule_ids: Some(vec!["SCH001".to_string()]),
                ..WorkPatchRequest::default()
            },
        )
        .await
        .expect("link one schedule to the work");

    // Completion-lock the Work at the store (the create path hardcodes NULL).
    let now = chrono::Utc::now().to_rfc3339();
    nexus_local_db::works::patch_work(
        f.core.pool(),
        CREATOR,
        &work_id,
        &nexus_local_db::works::WorkPatch {
            completion_locked_at: Some(Some(now.clone())),
            ..nexus_local_db::works::WorkPatch::default()
        },
        &now,
    )
    .await
    .expect("completion-lock the work");

    let status = execute_tool(
        &f.context,
        &tool_request(
            "nexus.orchestration.schedule_status",
            json!({ "work_id": work_id }),
        ),
    )
    .await
    .expect("a read-only tool still runs while the work is completion-locked");
    assert_eq!(status["work_id"], work_id);
    assert_eq!(status["count"], 1);
    assert_eq!(status["schedule_ids"][0], "SCH001");

    // A foreign work is refused, not silently reported as zero schedules.
    let err = execute_tool(
        &f.context,
        &tool_request(
            "nexus.orchestration.schedule_status",
            json!({ "work_id": "wrk_other_creator_work" }),
        ),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, CoreError::Forbidden { .. }),
        "a foreign work must be forbidden, got {err:?}"
    );
}

/// `nexus.context.assemble` is `policy_blocked` when the caller asks for a
/// platform-only assembly in local-only mode, and still assembles locally
/// without that flag.
///
/// MIGRATED from `agent_tool_api.rs::context_assemble_policy_blocked_when_platform_required`
/// and `error_code_policy_blocked_surface_in_assemble`.
#[tokio::test]
#[serial_test::serial]
async fn nexus_context_assemble_is_policy_blocked_in_local_only_mode() {
    let f = fixture().await;

    let err = execute_tool(
        &f.context,
        &tool_request(
            "nexus.context.assemble",
            json!({ "requires_platform": true }),
        ),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, CoreError::Coded { ref code, .. } if code == "policy_blocked"),
        "a platform-required assembly must be policy_blocked, got {err:?}"
    );

    let local = execute_tool(
        &f.context,
        &tool_request("nexus.context.assemble", json!({})),
    )
    .await
    .expect("the local-only subset still assembles");
    assert_eq!(local["mode"], "local");
    assert_eq!(local["creator_id"], CREATOR);
}

/// Every dispatch writes exactly ONE audit row — a success as `success`, and
/// every refusal as `denied:<code>` with the code the caller received.
///
/// MIGRATED from `agent_tool_api.rs::audit_log_written_on_success`,
/// `audit_log_written_on_unknown_tool_denial`,
/// `audit_log_written_on_cross_creator_denial`,
/// `audit_log_written_on_policy_blocked`,
/// `audit_log_written_on_invalid_input` and
/// `error_code_not_supported_for_unknown_tool`.
#[tokio::test]
#[serial_test::serial]
async fn tool_dispatch_audits_success_and_every_refusal() {
    let f = fixture().await;
    let work_id = seed_work(&f).await;

    execute_tool(&f.context, &tool_request("nexus.context.whoami", json!({})))
        .await
        .expect("whoami succeeds");
    assert_eq!(
        audit_outcomes(f.core.pool(), "nexus.context.whoami").await,
        ["success"]
    );

    // Unknown tool → `not_supported`, audited with that code.
    let unknown = execute_tool(&f.context, &tool_request("nexus.unknown.tool", json!({}))).await;
    assert!(
        matches!(unknown, Err(CoreError::Coded { ref code, .. }) if code == "not_supported"),
        "an unknown tool must report not_supported, got {unknown:?}"
    );
    assert_eq!(
        audit_outcomes(f.core.pool(), "nexus.unknown.tool").await,
        ["denied:not_supported"]
    );

    // Cross-creator read → `forbidden`; missing parameter → `invalid_input`.
    sqlx::query("UPDATE works SET creator_id = 'intruder' WHERE work_id = ?")
        .bind(&work_id)
        .execute(f.core.pool())
        .await
        .unwrap();
    let _ = execute_tool(
        &f.context,
        &tool_request("nexus.work.get", json!({ "work_id": work_id })),
    )
    .await
    .unwrap_err();
    let _ = execute_tool(&f.context, &tool_request("nexus.work.get", json!({}))).await;
    assert_eq!(
        audit_outcomes(f.core.pool(), "nexus.work.get").await,
        ["denied:forbidden", "denied:invalid_input"]
    );

    // Policy refusal → `policy_blocked`.
    let _ = execute_tool(
        &f.context,
        &tool_request(
            "nexus.context.assemble",
            json!({ "requires_platform": true }),
        ),
    )
    .await
    .unwrap_err();
    assert_eq!(
        audit_outcomes(f.core.pool(), "nexus.context.assemble").await,
        ["denied:policy_blocked"]
    );
}

// ---------------------------------------------------------------------------
// v1.193 P2-T9 — MIGRATED from `nexus-daemon-runtime/tests/compute_runs_api.rs`.
//
// The daemon's compute routes were HTTP envelopes over this crate's compute
// lane; the lane itself (ownership gate, sandbox taxonomy, atomic accept,
// discard, list/detail) is what these cases keep.
// ---------------------------------------------------------------------------

/// A run request against `world_id`/`module_id` with the fixture combatants.
fn run_request(world_id: &str, module_id: &str) -> RunRequest {
    serde_json::from_value(json!({
        "world_id": world_id,
        "module_id": module_id,
        "invocation_params": { "attacker_id": "kb_atk", "defender_id": "kb_def" },
    }))
    .unwrap()
}

/// A run request scoped to a named branch.
fn run_request_on_branch(world_id: &str, module_id: &str, branch_id: &str) -> RunRequest {
    serde_json::from_value(json!({
        "world_id": world_id,
        "module_id": module_id,
        "branch_id": branch_id,
        "invocation_params": { "attacker_id": "kb_atk", "defender_id": "kb_def" },
    }))
    .unwrap()
}

/// An accept request from its wire JSON.
fn accept_request(value: serde_json::Value) -> RunAcceptRequest {
    serde_json::from_value(value).unwrap()
}

/// `(run_id, status, error_json)` of the most recent run row of the world.
async fn latest_run_row(pool: &sqlx::SqlitePool) -> (String, String, Option<String>) {
    sqlx::query_as::<_, (String, String, Option<String>)>(
        "SELECT run_id, status, error_json FROM compute_sessions \
         WHERE world_id = ? AND run_id IS NOT NULL ORDER BY created_at DESC LIMIT 1",
    )
    .bind(WORLD)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// `(branch_id, event_type, status, provenance_json, affected_ids_json)` for
/// the fixture world's timeline, in sequence order.
type TimelineRow = (String, String, String, Option<String>, Option<String>);

async fn timeline_rows(pool: &sqlx::SqlitePool) -> Vec<TimelineRow> {
    sqlx::query_as::<_, TimelineRow>(
        "SELECT branch_id, event_type, status, extensions_nexus_json, \
                affected_key_block_ids_json \
         FROM narrative_timeline_events WHERE world_id = ? ORDER BY sequence_no",
    )
    .bind(WORLD)
    .fetch_all(pool)
    .await
    .unwrap()
}

/// A module that exports the V1 ABI but whose `compute` loops forever.
fn loop_wasm() -> Vec<u8> {
    wat::parse_str(
        r#"(module
            (memory (export "memory") 1)
            (global $heap (mut i32) (i32.const 1024))
            (func (export "alloc") (param $len i32) (result i32)
              (local $p i32)
              (local.set $p (global.get $heap))
              (global.set $heap (i32.add (global.get $heap) (local.get $len)))
              (local.get $p))
            (func (export "init"))
            (func (export "compute")
              (param i32 i32 i32 i32) (result i64)
              (loop $forever (br $forever))
              (i64.const 0)))
        "#,
    )
    .expect("valid wat")
}

/// The infinite-loop module manifest (`max_wall_time_ms` omitted).
fn loop_manifest() -> ModuleManifest {
    serde_json::from_str(
        r#"{"module_id":"loop","name":"Loop","version":"0.1.0","nexus_abi_version":1,
           "required_key_block_types":[],"compute_export":"compute","init_export":"init",
           "host_functions":[]}"#,
    )
    .unwrap()
}

/// Two staggered-budget loop manifests: the long run may execute for 1500 ms,
/// the short one only 300 ms.
fn staggered_loop_manifests() -> [(&'static str, ModuleManifest); 2] {
    let long = serde_json::from_str(
        r#"{"module_id":"loop_long","name":"Loop Long","version":"0.1.0","nexus_abi_version":1,
           "required_key_block_types":[],"compute_export":"compute","init_export":"init",
           "host_functions":[],"max_wall_time_ms":1500}"#,
    )
    .unwrap();
    let short = serde_json::from_str(
        r#"{"module_id":"loop_short","name":"Loop Short","version":"0.1.0","nexus_abi_version":1,
           "required_key_block_types":[],"compute_export":"compute","init_export":"init",
           "host_functions":[],"max_wall_time_ms":300}"#,
    )
    .unwrap();
    [("loop_long", long), ("loop_short", short)]
}

/// Insert a direct-lane run row and flip it to `succeeded` with crafted
/// proposals (bypasses the module, so the accept path can be driven with
/// arbitrary proposal payloads).
async fn craft_succeeded_run(pool: &sqlx::SqlitePool, proposals: serde_json::Value) -> String {
    let run_id = nexus_local_db::compute_runs::insert_run(
        pool,
        WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        Some(r"{}"),
    )
    .await
    .unwrap();
    nexus_local_db::compute_runs::set_run_succeeded(pool, &run_id, &proposals.to_string())
        .await
        .unwrap();
    run_id
}

/// Craft a `ComputeOutput` envelope with the given state deltas and event
/// titles; `affected_ids`, when `Some`, is stamped onto every event.
fn crafted_proposals(
    state_delta: &[serde_json::Value],
    event_titles: &[&str],
    affected_ids: Option<&[&str]>,
) -> serde_json::Value {
    let timeline_events: Vec<serde_json::Value> = event_titles
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let mut evt = json!({
                "schema_version": 1,
                "timeline_event_id": format!("evt_p{i}"),
                "world_id": WORLD,
                "branch_id": "fbk_root",
                "event_type": "story_advance",
                "status": "provisional",
                "sequence_no": i,
                "title": title,
                "summary": format!("summary {title}"),
                "created_at": "2026-07-31T12:00:00Z",
            });
            if let Some(ids) = affected_ids {
                evt["affected_key_block_ids"] = json!(ids);
            }
            evt
        })
        .collect();
    json!({
        "schema_version": 1,
        "state_delta": state_delta,
        "timeline_events": timeline_events,
        "new_key_blocks": [],
        "battle_report": {"kind": "combat"},
    })
}

/// Seed a timeline event on a named branch (the durable branch registry is the
/// set of `branch_id`s materialized on the timeline).
async fn seed_branch_event(pool: &sqlx::SqlitePool, world_id: &str, branch_id: &str) {
    sqlx::query(
        "INSERT INTO narrative_timeline_events \
            (timeline_event_id, world_id, branch_id, event_type, status, sequence_no, metadata_json) \
         VALUES (?, ?, ?, 'fork_marker', 'provisional', 0, '{}')",
    )
    .bind(format!("evt_bside{}", branch_id.replace('_', "")))
    .bind(world_id)
    .bind(branch_id)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed a computable character whose `state.character.current_hp` violates the
/// `basic-combat` manifest schema (it must be an integer) — the per-entry
/// validation case.
async fn seed_broken_character(pool: &sqlx::SqlitePool, entry_id: &str) {
    use nexus_contracts::BlockType;
    use nexus_knowledge::world_kb::knowledge_entry::{
        KnowledgeEntryBody, KnowledgeEntryRecord, KnowledgeOwnerRef,
    };
    use nexus_knowledge::world_kb::KbStore;

    let kb = KnowledgeEntryRecord {
        entry_id: entry_id.to_string(),
        owner: KnowledgeOwnerRef::world(WORLD),
        block_type: BlockType::Character,
        canonical_name: format!("Broken {entry_id}"),
        body: Some(KnowledgeEntryBody {
            summary: Some("Broken combatant".to_string()),
            attributes: Some(json!({ "max_hp": 100, "base_atk": 5, "base_def": 10 })),
            computable: Some(true),
            state: Some(json!({
                "character": {
                    "current_hp": "one-hundred",
                    "is_alive": true,
                    "status_effects": [],
                }
            })),
            ..Default::default()
        }),
        ..KnowledgeEntryRecord::new(WORLD, BlockType::Character, "Broken")
    };
    nexus_local_db::kb_store::SqliteKbStore::new(pool.clone())
        .insert_knowledge_entry(kb)
        .await
        .unwrap();
}

/// A run refuses a foreign World as an ownership denial, an unknown module as
/// `not_found`, and a World with no computable entry as `invalid_input` — and
/// none of the three leaves a run row or a timeline event behind.
///
/// MIGRATED from `compute_runs_api.rs::run_not_owner_returns_403`,
/// `run_invalid_module_returns_404` and `run_no_computable_entries_returns_422`.
#[tokio::test]
#[serial_test::serial]
async fn compute_run_refusals_are_typed_and_leave_no_state() {
    let f = fixture().await;
    seed_foreign_world(f.core.pool()).await;
    seed_world(f.core.pool(), "wld_empty", CREATOR).await;

    let foreign = compute_run(&f.core, &f.compute, run_request(FOREIGN_WORLD, MODULE))
        .await
        .unwrap_err();
    assert!(
        matches!(foreign, CoreError::WorldOwnerDenied { .. }),
        "a foreign world must be an ownership denial, got {foreign:?}"
    );

    // Ownership is checked BEFORE module resolution, so the unknown module is
    // only reached on an owned world.
    let unknown_module =
        compute_run(&f.core, &f.compute, run_request(WORLD, "no-such-module")).await;
    assert!(
        matches!(unknown_module, Err(CoreError::NotFound { .. })),
        "an unknown module must be not_found, got {unknown_module:?}"
    );

    let empty = compute_run(&f.core, &f.compute, run_request("wld_empty", MODULE)).await;
    assert!(
        matches!(empty, Err(CoreError::Coded { ref code, .. }) if code == "invalid_input"),
        "a world with no computable entry must be invalid_input, got {empty:?}"
    );

    let rows: i64 =
        sqlx::query_scalar("SELECT COUNT(*) FROM compute_sessions WHERE run_id IS NOT NULL")
            .fetch_one(f.core.pool())
            .await
            .unwrap();
    assert_eq!(rows, 0, "a refused run must not persist a row");
    assert_eq!(
        timeline_rows(f.core.pool()).await,
        [] as [(
            std::string::String,
            std::string::String,
            std::string::String,
            std::option::Option<std::string::String>,
            std::option::Option<std::string::String>
        ); 0]
    );
}

/// A sandbox trap is refused with its retained code, the failed run is
/// persisted with honest detail, and the direct lane writes NO timeline event.
///
/// MIGRATED from `compute_runs_api.rs::run_fuel_exhaustion_returns_422_with_honest_code`
/// and `failed_run_leaves_timeline_empty`.
#[tokio::test]
#[serial_test::serial]
async fn compute_run_fuel_exhaustion_is_persisted_as_a_failed_run() {
    let f = fixture_with_compute(&[("loop", loop_manifest(), loop_wasm())], None).await;

    let err = compute_run(&f.core, &f.compute, run_request(WORLD, "loop")).await;
    assert!(
        matches!(err, Err(CoreError::Coded { ref code, .. }) if code == "compute_fuel_exhausted"),
        "an exhausted-fuel loop must report compute_fuel_exhausted, got {err:?}"
    );

    let (run_id, status, error_json) = latest_run_row(f.core.pool()).await;
    assert!(!run_id.is_empty(), "the failed run is still persisted");
    assert_eq!(status, "failed");
    let error: serde_json::Value =
        serde_json::from_str(error_json.as_deref().expect("failed rows carry error_json")).unwrap();
    assert_eq!(error["code"], "compute_fuel_exhausted");
    assert_eq!(
        timeline_rows(f.core.pool()).await,
        [] as [(
            std::string::String,
            std::string::String,
            std::string::String,
            std::option::Option<std::string::String>,
            std::option::Option<std::string::String>
        ); 0]
    );
}

/// The wall-time watchdog traps first when fuel cannot be exhausted, and the
/// refusal keeps its own honest code.
///
/// MIGRATED from `compute_runs_api.rs::run_wall_time_exceeded_returns_422_with_honest_code`.
#[tokio::test]
#[serial_test::serial]
async fn compute_run_wall_time_exceeded_is_persisted_as_a_failed_run() {
    // Huge fuel so the loop cannot exhaust it; a 300 ms wall-time watchdog
    // traps first via epoch interruption.
    let config = SandboxConfig {
        fuel: 100_000_000_000,
        max_memory_bytes: 64 * 1024 * 1024,
        wall_time: Duration::from_millis(300),
    };
    let f = fixture_with_compute(&[("loop", loop_manifest(), loop_wasm())], Some(config)).await;

    let err = compute_run(&f.core, &f.compute, run_request(WORLD, "loop")).await;
    assert!(
        matches!(err, Err(CoreError::Coded { ref code, .. }) if code == "compute_wall_time_exceeded"),
        "a runaway loop must report compute_wall_time_exceeded, got {err:?}"
    );

    let (_run_id, status, error_json) = latest_run_row(f.core.pool()).await;
    assert_eq!(status, "failed");
    let error: serde_json::Value =
        serde_json::from_str(error_json.as_deref().expect("failed rows carry error_json")).unwrap();
    assert_eq!(error["code"], "compute_wall_time_exceeded");
    assert_eq!(
        timeline_rows(f.core.pool()).await,
        [] as [(
            std::string::String,
            std::string::String,
            std::string::String,
            std::option::Option<std::string::String>,
            std::option::Option<std::string::String>
        ); 0]
    );
}

/// Two concurrent runs against the shared engine execute SERIALLY through the
/// compute serializer, so the long run (1500 ms budget) survives the short
/// run's (300 ms) watchdog.
///
/// Without serialization the engine-global epoch counter trips BOTH
/// invocations at the shortest budget (~300 ms).
///
/// MIGRATED from `compute_runs_api.rs::concurrent_runs_serialize_compute_and_long_survives_short_watchdog`.
#[tokio::test]
#[serial_test::serial]
async fn concurrent_compute_runs_serialize_on_the_engine() {
    let manifests = staggered_loop_manifests();
    let config = SandboxConfig {
        fuel: 100_000_000_000, // the loops cannot exhaust fuel
        max_memory_bytes: 64 * 1024 * 1024,
        wall_time: Duration::from_secs(5), // host ceiling above both manifests
    };
    let f = fixture_with_compute(
        &[
            ("loop_long", manifests[0].1.clone(), loop_wasm()),
            ("loop_short", manifests[1].1.clone(), loop_wasm()),
        ],
        Some(config),
    )
    .await;

    let long = compute_run(&f.core, &f.compute, run_request(WORLD, "loop_long"));
    let short = compute_run(&f.core, &f.compute, run_request(WORLD, "loop_short"));
    let started = std::time::Instant::now();
    let (long_result, short_result) = tokio::join!(long, short);
    let elapsed = started.elapsed();

    for (label, result) in [("long", long_result), ("short", short_result)] {
        let Err(err) = result else {
            panic!("the {label} loop run must trap on wall time");
        };
        assert!(
            matches!(err, CoreError::Coded { ref code, .. } if code == "compute_wall_time_exceeded"),
            "the {label} run must report compute_wall_time_exceeded, got {err:?}"
        );
    }

    // Serialization proof: back-to-back execution is 300 ms + 1500 ms in either
    // order, so both complete in >= 1500 ms; cross-tripping would finish in
    // ~300 ms.
    assert!(
        elapsed >= Duration::from_millis(1200),
        "concurrent runs must serialize (long survives short watchdog), took {elapsed:?}"
    );

    let runs: Vec<(String, String)> = sqlx::query_as(
        "SELECT run_id, status FROM compute_sessions \
         WHERE world_id = ? AND run_id IS NOT NULL ORDER BY created_at DESC LIMIT 2",
    )
    .bind(WORLD)
    .fetch_all(f.core.pool())
    .await
    .unwrap();
    assert_eq!(runs.len(), 2, "both staggered runs are persisted: {runs:?}");
    for (_, status) in &runs {
        assert_eq!(status, "failed", "every loop run is persisted failed");
    }
}

/// An entry that violates the manifest schema poisons the run: the refusal
/// carries per-entry detail, the row is persisted `failed` with the SAME
/// structured detail, no event is written, and the detail read still serves the
/// failed run instead of failing to serialize it.
///
/// MIGRATED from `compute_runs_api.rs::run_with_invalid_entry_returns_422_with_per_entry_detail`.
#[tokio::test]
#[serial_test::serial]
async fn compute_run_reports_per_entry_detail_for_an_invalid_entry() {
    let f = fixture().await;
    seed_broken_character(f.core.pool(), "kb_broken").await;
    let principal = f.core.active_principal().await.unwrap();

    let details = match compute_run(&f.core, &f.compute, run_request(WORLD, MODULE)).await {
        Err(CoreError::InputValidation { details }) => details,
        other => panic!("a poisoned entry must be an input-validation refusal, got {other:?}"),
    };
    let entries = details["invalid_entries"]
        .as_array()
        .unwrap_or_else(|| panic!("invalid_entries must be an array: {details}"));
    assert!(!entries.is_empty(), "details={details}");
    assert_eq!(entries[0]["entry_id"], "kb_broken");
    let reason = entries[0]["reason"].as_str().expect("per-entry reason");
    assert!(
        reason.contains("current_hp"),
        "reason names the field: {reason}"
    );
    assert!(
        reason.contains("expected type integer"),
        "reason explains the violation: {reason}"
    );

    // The persisted row and the refusal carry the same detail by construction.
    let (run_id, status, error_json) = latest_run_row(f.core.pool()).await;
    assert_eq!(status, "failed");
    let error: serde_json::Value =
        serde_json::from_str(error_json.as_deref().expect("failed rows carry error_json")).unwrap();
    assert_eq!(error["code"], "invalid_input");
    assert_eq!(
        error["details"]["invalid_entries"][0]["entry_id"],
        "kb_broken"
    );
    assert_eq!(
        timeline_rows(f.core.pool()).await,
        [] as [(
            std::string::String,
            std::string::String,
            std::string::String,
            std::option::Option<std::string::String>,
            std::option::Option<std::string::String>
        ); 0]
    );

    // The detail read must serve this failed run (a top-level `invalid_entries`
    // key would break its deserialization).
    let detail = get_compute_run(&f.core, &principal, &run_id)
        .await
        .expect("a failed run is still readable");
    let detail = serde_json::to_value(&detail).expect("detail serializes");
    assert_eq!(detail["status"], "failed");
    assert_eq!(detail["error"]["code"], "invalid_input");
    assert_eq!(
        detail["error"]["details"]["invalid_entries"][0]["entry_id"],
        "kb_broken"
    );
}

/// Accept persists the proposals' `affected_key_block_ids` onto the appended
/// events — the column the compute inspector resolves "Affected knowledge"
/// from.
///
/// MIGRATED from `compute_runs_api.rs::accept_persists_affected_key_block_ids`.
#[tokio::test]
#[serial_test::serial]
async fn accept_persists_affected_key_block_ids() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();

    let run_id = craft_succeeded_run(
        f.core.pool(),
        crafted_proposals(
            &[json!({
                "op": "sub",
                "path": "character.current_hp",
                "target_key_block_id": "kb_def",
                "value": 15,
            })],
            &["Combat resolved"],
            Some(&["kb_def", "kb_atk"]),
        ),
    )
    .await;

    accept_compute_run(&f.core, &principal, &run_id, accept_request(json!({})))
        .await
        .expect("accept succeeds");

    let rows = timeline_rows(f.core.pool()).await;
    assert_eq!(rows.len(), 1);
    let affected: Vec<String> = serde_json::from_str(
        rows[0]
            .4
            .as_deref()
            .expect("affected ids must be persisted"),
    )
    .unwrap();
    assert_eq!(affected, vec!["kb_def".to_string(), "kb_atk".to_string()]);
}

/// Discard drops the proposals and flips the row: the World is untouched and
/// the detail reflects the discarded status.
///
/// MIGRATED from `compute_runs_api.rs::discard_marks_run_discarded_and_leaves_world_unchanged`.
#[tokio::test]
#[serial_test::serial]
async fn discard_marks_the_run_discarded_and_leaves_the_world_unchanged() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    let run_id = run_succeeded(&f, &f.compute).await;

    discard_compute_run(&f.core, &principal, &run_id)
        .await
        .expect("discard succeeds");

    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 30);
    assert_eq!(
        timeline_rows(f.core.pool()).await,
        [] as [(
            std::string::String,
            std::string::String,
            std::string::String,
            std::option::Option<std::string::String>,
            std::option::Option<std::string::String>
        ); 0]
    );

    let detail = get_compute_run(&f.core, &principal, &run_id)
        .await
        .expect("detail read");
    let detail = serde_json::to_value(&detail).unwrap();
    assert_eq!(detail["status"], "discarded");
}

/// Accept refuses a run that never succeeded, at the status guard.
///
/// MIGRATED from `compute_runs_api.rs::accept_on_failed_run_returns_422_invalid_state`.
#[tokio::test]
#[serial_test::serial]
async fn accept_refuses_a_failed_run_as_invalid_state() {
    let f = fixture_with_compute(&[("loop", loop_manifest(), loop_wasm())], None).await;
    let principal = f.core.active_principal().await.unwrap();

    let _ = compute_run(&f.core, &f.compute, run_request(WORLD, "loop")).await;
    let (run_id, status, _) = latest_run_row(f.core.pool()).await;
    assert_eq!(status, "failed");

    let err = accept_compute_run(&f.core, &principal, &run_id, accept_request(json!({})))
        .await
        .unwrap_err();
    assert!(
        matches!(err, CoreError::Coded { ref code, .. } if code == "invalid_state"),
        "a failed run must refuse as invalid_state, got {err:?}"
    );
}

/// Accept and discard both refuse a run that is already terminal, and the
/// terminal transitions are exclusive: an applied run cannot be discarded and
/// a discarded run cannot be accepted.
///
/// MIGRATED from `compute_runs_api.rs::accept_on_discarded_run_returns_409` and
/// `discard_on_applied_run_returns_409`.
#[tokio::test]
#[serial_test::serial]
async fn accept_and_discard_refuse_an_already_terminal_run() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();

    // Applied → a second accept AND a discard are both conflicts.
    let applied = run_succeeded(&f, &f.compute).await;
    accept_compute_run(&f.core, &principal, &applied, accept_request(json!({})))
        .await
        .expect("first accept succeeds");
    let second = accept_compute_run(&f.core, &principal, &applied, accept_request(json!({})))
        .await
        .unwrap_err();
    assert!(
        matches!(second, CoreError::Coded { ref code, .. } if code == "conflict"),
        "a second accept must conflict, got {second:?}"
    );
    let discard_applied = discard_compute_run(&f.core, &principal, &applied).await;
    assert!(
        matches!(discard_applied, Err(CoreError::Coded { ref code, .. }) if code == "conflict"),
        "discarding an applied run must conflict, got {discard_applied:?}"
    );

    // Discarded → accept is a conflict.
    let discarded = run_succeeded(&f, &f.compute).await;
    discard_compute_run(&f.core, &principal, &discarded)
        .await
        .expect("discard succeeds");
    let accept_discarded =
        accept_compute_run(&f.core, &principal, &discarded, accept_request(json!({})))
            .await
            .unwrap_err();
    assert!(
        matches!(accept_discarded, CoreError::Coded { ref code, .. } if code == "conflict"),
        "accepting a discarded run must conflict, got {accept_discarded:?}"
    );
}

/// The run list is scoped to the creator's OWNED worlds and cursor-paginated;
/// a run on a foreign world never appears.
///
/// MIGRATED from `compute_runs_api.rs::list_paginates_and_scopes_to_owned_worlds`.
#[tokio::test]
#[serial_test::serial]
async fn list_compute_runs_pages_and_scopes_to_owned_worlds() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    let first = run_succeeded(&f, &f.compute).await;
    let second = run_succeeded(&f, &f.compute).await;
    let third = run_succeeded(&f, &f.compute).await;

    seed_foreign_world(f.core.pool()).await;
    let foreign = nexus_local_db::compute_runs::insert_run(
        f.core.pool(),
        FOREIGN_WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let page1 = list_compute_runs(
        &f.core,
        &principal,
        ListRunsQuery {
            limit: Some(2),
            ..ListRunsQuery::default()
        },
    )
    .await
    .expect("first page");
    let page1 = serde_json::to_value(&page1).unwrap();
    assert_eq!(page1["items"].as_array().unwrap().len(), 2);
    assert_eq!(page1["has_more"], true);
    let cursor = page1["next_cursor"].as_str().expect("cursor").to_string();
    assert!(
        !page1["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["run_id"].as_str() == Some(foreign.as_str())),
        "a foreign-world run must not be listed: {page1}"
    );

    let page2 = list_compute_runs(
        &f.core,
        &principal,
        ListRunsQuery {
            limit: Some(2),
            cursor: Some(cursor),
            ..ListRunsQuery::default()
        },
    )
    .await
    .expect("second page");
    let page2 = serde_json::to_value(&page2).unwrap();
    assert_eq!(page2["items"].as_array().unwrap().len(), 1);
    assert_eq!(page2["has_more"], false);
    assert!(page2["next_cursor"].is_null(), "no further page: {page2}");

    let listed: Vec<&str> = page1["items"]
        .as_array()
        .unwrap()
        .iter()
        .chain(page2["items"].as_array().unwrap().iter())
        .filter_map(|i| i["run_id"].as_str())
        .collect();
    for expected in [&first, &second, &third] {
        assert!(
            listed.contains(&expected.as_str()),
            "{expected} must be listed: {listed:?}"
        );
    }
}

/// The list walks newest-first, and the cursor continues that order.
///
/// MIGRATED from `compute_runs_api.rs::list_orders_newest_first`.
#[tokio::test]
#[serial_test::serial]
async fn list_compute_runs_orders_newest_first() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    let a = run_succeeded(&f, &f.compute).await;
    let b = run_succeeded(&f, &f.compute).await;
    let c = run_succeeded(&f, &f.compute).await;

    // Pin distinct created_at values so ordering does not depend on clock
    // precision within the same second.
    for (run_id, ts) in [
        (&a, "2026-07-31T10:00:00.000Z"),
        (&b, "2026-07-31T10:00:01.000Z"),
        (&c, "2026-07-31T10:00:02.000Z"),
    ] {
        sqlx::query("UPDATE compute_sessions SET created_at = ? WHERE run_id = ?")
            .bind(ts)
            .bind(run_id)
            .execute(f.core.pool())
            .await
            .unwrap();
    }

    let page1 = list_compute_runs(
        &f.core,
        &principal,
        ListRunsQuery {
            limit: Some(2),
            ..ListRunsQuery::default()
        },
    )
    .await
    .expect("first page");
    let page1 = serde_json::to_value(&page1).unwrap();
    let ids1: Vec<&str> = page1["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["run_id"].as_str())
        .collect();
    assert_eq!(ids1, vec![c.as_str(), b.as_str()], "newest first");
    let cursor = page1["next_cursor"].as_str().expect("cursor").to_string();

    let page2 = list_compute_runs(
        &f.core,
        &principal,
        ListRunsQuery {
            limit: Some(2),
            cursor: Some(cursor),
            ..ListRunsQuery::default()
        },
    )
    .await
    .expect("second page");
    let page2 = serde_json::to_value(&page2).unwrap();
    let ids2: Vec<&str> = page2["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| i["run_id"].as_str())
        .collect();
    assert_eq!(ids2, vec![a.as_str()], "the cursor continues the order");
    assert_eq!(page2["has_more"], false);
}

/// Extract the `resource` of a `NotFound` refusal — the shared-shape assertion
/// used by the run-ID opacity pins (V1.206 P2 site 4).
fn not_found_resource(err: &CoreError) -> &str {
    match err {
        CoreError::NotFound { resource } => resource,
        other => panic!("a foreign or unknown run must close as NotFound, got {other:?}"),
    }
}

/// The detail read returns the proposals and invocation params of a succeeded
/// run, refuses an unknown run as `not_found`, and closes EVERY operation on a
/// foreign-world run with that SAME `not_found` shape (existence opacity,
/// V1.206 D4), so no lifecycle state and no run/World ownership fact leaks.
///
/// MIGRATED from `compute_runs_api.rs::get_run_detail_returns_proposals_and_invocation_params`,
/// `get_run_detail_unknown_run_returns_404` and
/// `foreign_run_accept_detail_discard_return_403`.
#[tokio::test]
#[serial_test::serial]
async fn run_detail_and_foreign_run_refusals() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    let run_id = run_succeeded(&f, &f.compute).await;

    let detail = get_compute_run(&f.core, &principal, &run_id)
        .await
        .expect("detail read");
    let detail = serde_json::to_value(&detail).unwrap();
    assert_eq!(detail["run_id"], run_id);
    assert_eq!(detail["world_id"], WORLD);
    assert_eq!(detail["module_id"], MODULE);
    assert_eq!(detail["module_version"], "1.0.0");
    assert_eq!(detail["status"], "succeeded");
    assert_eq!(detail["invocation_params"]["attacker_id"], "kb_atk");
    assert_eq!(detail["invocation_params"]["defender_id"], "kb_def");
    assert_eq!(detail["proposals"]["battle_report"]["kind"], "combat");
    assert!(detail["created_at"].is_string());

    let unknown = get_compute_run(&f.core, &principal, "run_does_not_exist").await;
    assert!(
        matches!(unknown, Err(CoreError::NotFound { .. })),
        "an unknown run must be not_found, got {unknown:?}"
    );

    seed_foreign_world(f.core.pool()).await;
    let foreign = nexus_local_db::compute_runs::insert_run(
        f.core.pool(),
        FOREIGN_WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    let foreign_detail = get_compute_run(&f.core, &principal, &foreign).await;
    let foreign_accept =
        accept_compute_run(&f.core, &principal, &foreign, accept_request(json!({}))).await;
    let foreign_discard = discard_compute_run(&f.core, &principal, &foreign).await;

    // Existence opacity (V1.206 D4 / compass D4): an unknown run id and a run
    // of a foreign World close with the SAME 404 `not_found` shape, so a probe
    // cannot split run existence from World ownership. `WorldOwnerDenied` stays
    // reserved for the world-scoped routes (invoke / list / clear).
    assert_eq!(
        not_found_resource(unknown.as_ref().unwrap_err()),
        "run run_does_not_exist not found"
    );
    assert_eq!(
        not_found_resource(foreign_detail.as_ref().unwrap_err()),
        format!("run {foreign} not found"),
        "an unknown id and a foreign id must close with the same refusal"
    );
    assert_eq!(
        not_found_resource(foreign_accept.as_ref().unwrap_err()),
        format!("run {foreign} not found"),
        "an unknown id and a foreign id must close with the same refusal"
    );
    assert_eq!(
        not_found_resource(foreign_discard.as_ref().unwrap_err()),
        format!("run {foreign} not found"),
        "an unknown id and a foreign id must close with the same refusal"
    );

    // QC F-001: the internal Absent / ForeignDenied / Visible distinction is
    // pinned inside the crate (`execution::compute::tests`) — keeping the
    // helper (and its SQL pool) off the public surface is the point of that
    // move, so this test asserts only the outward opacity shape above.

    // The foreign row survived every refused operation.
    assert!(
        nexus_local_db::compute_runs::get_run(f.core.pool(), &foreign)
            .await
            .unwrap()
            .is_some()
    );
}

/// A delta targeting another World rejects the WHOLE accept and rolls back
/// everything already applied in that transaction — including a valid first
/// delta.
///
/// MIGRATED from `compute_runs_api.rs::accept_foreign_delta_target_rejects_with_422_and_rolls_back`
/// and `accept_mid_loop_failure_rolls_back_entire_tx`.
#[tokio::test]
#[serial_test::serial]
async fn accept_rolls_back_when_a_delta_targets_another_world() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    seed_foreign_world(f.core.pool()).await;

    let run_id = craft_succeeded_run(
        f.core.pool(),
        crafted_proposals(
            &[
                // 1st delta: valid, applies inside the TX (def 30 → 15).
                json!({
                    "op": "sub",
                    "path": "character.current_hp",
                    "target_key_block_id": "kb_def",
                    "value": 15,
                }),
                // 2nd delta: foreign target → InputInvalid mid-loop.
                json!({
                    "op": "sub",
                    "path": "character.current_hp",
                    "target_key_block_id": "kb_foreign",
                    "value": 1,
                }),
            ],
            &["Battle"],
            None,
        ),
    )
    .await;

    let err = accept_compute_run(&f.core, &principal, &run_id, accept_request(json!({})))
        .await
        .unwrap_err();
    assert!(
        matches!(err, CoreError::Coded { ref code, .. } if code == "invalid_input"),
        "a foreign delta target must refuse as invalid_input, got {err:?}"
    );

    // FULL rollback: the first delta is not visible, no event was appended, and
    // the run is still `succeeded` (nothing was partially applied).
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 30);
    assert_eq!(defender_hp(f.core.pool(), "kb_foreign").await, 777);
    assert_eq!(
        timeline_rows(f.core.pool()).await,
        [] as [(
            std::string::String,
            std::string::String,
            std::string::String,
            std::option::Option<std::string::String>,
            std::option::Option<std::string::String>
        ); 0]
    );
    let row = nexus_local_db::compute_runs::get_run(f.core.pool(), &run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "succeeded");
}

/// A run scoped to a named branch snapshots that branch, accept lands its
/// events on the SNAPSHOT, and an unknown or other-world branch is refused at
/// run time.
///
/// MIGRATED from `compute_runs_api.rs::run_on_named_branch_snapshots_and_accept_lands_events_there`,
/// `run_with_unknown_branch_returns_422` and `run_with_other_world_branch_returns_422`.
#[tokio::test]
#[serial_test::serial]
async fn compute_run_branches_are_world_scoped() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    seed_branch_event(f.core.pool(), WORLD, "fbk_side1").await;

    let response = compute_run(
        &f.core,
        &f.compute,
        run_request_on_branch(WORLD, MODULE, "fbk_side1"),
    )
    .await
    .expect("a run on a known branch succeeds");
    let row = nexus_local_db::compute_runs::get_run(f.core.pool(), &response.run_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        row.branch_id.as_deref(),
        Some("fbk_side1"),
        "the run row must snapshot the requested branch"
    );

    accept_compute_run(
        &f.core,
        &principal,
        &response.run_id,
        accept_request(json!({})),
    )
    .await
    .expect("accept succeeds");

    let computed: Vec<String> = timeline_rows(f.core.pool())
        .await
        .into_iter()
        .filter(|row| row.1 == "compute_result")
        .map(|row| row.0)
        .collect();
    assert_eq!(
        computed,
        vec!["fbk_side1".to_string()],
        "compute_result must land on the snapshotted branch"
    );

    // An unknown branch is refused.
    let unknown = compute_run(
        &f.core,
        &f.compute,
        run_request_on_branch(WORLD, MODULE, "fbk_nonexistent"),
    )
    .await;
    assert!(
        matches!(unknown, Err(CoreError::Coded { ref code, .. }) if code == "invalid_input"),
        "an unknown branch must be invalid_input, got {unknown:?}"
    );

    // Membership is WORLD-scoped: a branch that is event-bearing in ANOTHER
    // world is still unreachable here.
    seed_foreign_world(f.core.pool()).await;
    seed_branch_event(f.core.pool(), FOREIGN_WORLD, "fbk_other").await;
    let other_world = compute_run(
        &f.core,
        &f.compute,
        run_request_on_branch(WORLD, MODULE, "fbk_other"),
    )
    .await;
    assert!(
        matches!(other_world, Err(CoreError::Coded { ref code, .. }) if code == "invalid_input"),
        "an other-world branch must be invalid_input, got {other_world:?}"
    );
}

/// `timeline_event_ids_to_accept` appends ONLY the referenced proposed events
/// (the state delta stays all-or-nothing), an unknown id refuses the whole
/// accept before any write, and an explicit JSON `null` accepts all.
///
/// MIGRATED from `compute_runs_api.rs::accept_subset_appends_only_listed_events`,
/// `accept_subset_with_unknown_id_returns_422_and_writes_nothing` and
/// `accept_with_explicit_null_timeline_ids_accepts_all`.
#[tokio::test]
#[serial_test::serial]
async fn accept_subsets_events_and_treats_explicit_null_as_all() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    let delta = || {
        vec![json!({
            "op": "sub",
            "path": "character.current_hp",
            "target_key_block_id": "kb_def",
            "value": 15,
        })]
    };

    // Subset: only `evt_0` is appended, the state delta still applies.
    let subset = craft_succeeded_run(
        f.core.pool(),
        crafted_proposals(&delta(), &["First", "Second"], None),
    )
    .await;
    let response = accept_compute_run(
        &f.core,
        &principal,
        &subset,
        accept_request(json!({ "timeline_event_ids_to_accept": ["evt_0"] })),
    )
    .await
    .expect("a known subset id is admitted");
    assert_eq!(response.applied.events_created, 1);
    assert_eq!(response.timeline_event_ids.len(), 1);
    let rows = timeline_rows(f.core.pool()).await;
    assert_eq!(rows.len(), 1, "only the referenced event is appended");
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 15);

    // Unknown id: the whole accept is refused BEFORE any write.
    let unknown =
        craft_succeeded_run(f.core.pool(), crafted_proposals(&delta(), &["Only"], None)).await;
    let err = accept_compute_run(
        &f.core,
        &principal,
        &unknown,
        accept_request(json!({ "timeline_event_ids_to_accept": ["evt_9"] })),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, CoreError::Coded { ref code, .. } if code == "invalid_input"),
        "an unknown event id must be invalid_input, got {err:?}"
    );
    let row = nexus_local_db::compute_runs::get_run(f.core.pool(), &unknown)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(row.status, "succeeded", "the refused accept wrote nothing");

    // Explicit null behaves exactly like an absent field: accept all.
    let all = craft_succeeded_run(
        f.core.pool(),
        crafted_proposals(&delta(), &["First", "Second"], None),
    )
    .await;
    let before_null_accept = timeline_rows(f.core.pool()).await.len();
    let response = accept_compute_run(
        &f.core,
        &principal,
        &all,
        accept_request(json!({ "timeline_event_ids_to_accept": null })),
    )
    .await
    .expect("explicit null accepts all events");
    assert_eq!(response.applied.events_created, 2);
    assert_eq!(response.timeline_event_ids.len(), 2);
    assert_eq!(
        timeline_rows(f.core.pool()).await.len() - before_null_accept,
        2,
        "explicit null must append every proposed event"
    );
}

/// The installed-module registry lists the embedded module, returns its
/// detail, and reports an unknown module as `not_found`.
///
/// MIGRATED from `compute_modules.rs::list_modules_includes_basic_combat`,
/// `get_basic_combat_returns_detail` and `get_unknown_module_returns_404`
/// (the daemon's `compute_modules` handler was a thin wrapper over these two
/// registry reads; its HTTP envelope and the router tests retire with the
/// host).
#[tokio::test]
#[serial_test::serial]
async fn compute_module_registry_lists_embedded_modules() {
    let modules = nexus_core::execution::compute::list_compute_modules()
        .expect("the compiled-in registry is readable");
    assert!(
        modules.iter().any(|m| m.module_id == MODULE),
        "{MODULE} must appear in the registry list: {modules:?}"
    );

    let detail = nexus_core::execution::compute::get_compute_module(MODULE)
        .expect("the embedded module has detail");
    assert_eq!(detail.module_id, MODULE);
    assert_eq!(detail.name, "Basic Combat");
    assert_eq!(detail.nexus_abi_version, 1);

    let unknown = nexus_core::execution::compute::get_compute_module("no-such-module");
    assert!(
        matches!(unknown, Err(CoreError::NotFound { .. })),
        "an unknown module must be not_found, got {unknown:?}"
    );
}

// ---------------------------------------------------------------------------
// v1.195 P2-T2 — terminal history clear (C8 / S2-5)
// ---------------------------------------------------------------------------

/// Whether a direct-lane run row is still durable.
///
/// Clear DELETES rows, so an absence claim is read back through the
/// authority's own point lookup instead of being inferred from a list page.
async fn run_row_survives(pool: &sqlx::SqlitePool, run_id: &str) -> bool {
    nexus_local_db::compute_runs::get_run(pool, run_id)
        .await
        .expect("run row read")
        .is_some()
}

/// Clear removes a World's TERMINAL run rows — and nothing else.
///
/// The three ways a clear can go wrong are each asserted:
///
/// - **It eats work that is not history.** A `succeeded` run still awaits
///   review and a `running` run is not terminal: both must survive, by the
///   storage predicate rather than by the caller's discipline.
/// - **It reaches past the World.** Another creator's TERMINAL row is never
///   touched, and clearing that World is refused as an ownership denial.
/// - **It undoes an accepted effect.** The applied state delta and the CANON
///   `compute_result` event are World truth, not run-row state, so they are
///   still there after the row that produced them is gone.
///
/// `query.status` narrows Clear to ONE terminal state and never widens it, and
/// the owner door (`ExecutionHandle::clear_compute_runs`) consumes the SAME
/// authority.
#[tokio::test]
#[serial_test::serial]
async fn clear_owned_terminal_history_preserves_effects_and_pending() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    seed_foreign_world(f.core.pool()).await;

    // ACCEPTED: terminal, and its effect is committed World truth.
    let applied = run_succeeded(&f, &f.compute).await;
    accept_compute_run(&f.core, &principal, &applied, accept_request(json!({})))
        .await
        .expect("accept succeeds");
    // Damage = max(0, 20 − 5); the event is canon.
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 15);
    let effects_before = timeline_rows(f.core.pool()).await;
    assert_eq!(effects_before.len(), 1);

    // DISCARDED: terminal and clearable, but it never had an effect.
    let discarded = run_succeeded(&f, &f.compute).await;
    discard_compute_run(&f.core, &principal, &discarded)
        .await
        .expect("discard succeeds");

    // SUCCEEDED: terminal-capable only after review — never clearable yet.
    let pending = run_succeeded(&f, &f.compute).await;

    // RUNNING: not terminal.
    let running = nexus_local_db::compute_runs::insert_run(
        f.core.pool(),
        WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();

    // Another creator's World, holding a TERMINAL row of its own.
    let foreign = nexus_local_db::compute_runs::insert_run(
        f.core.pool(),
        FOREIGN_WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    nexus_local_db::compute_runs::set_run_succeeded(f.core.pool(), &foreign, "{}")
        .await
        .unwrap();
    nexus_local_db::compute_runs::set_run_discarded(f.core.pool(), &foreign)
        .await
        .unwrap();

    // A terminal filter matches only its own state: nothing here is `failed`,
    // and the applied/discarded rows are still standing afterwards.
    let none_failed = clear_compute_runs(
        &f.core,
        &principal,
        ClearRunsQuery {
            status: Some(ClearRunsQueryStatus::Failed),
            world_id: WORLD.to_string(),
        },
    )
    .await
    .expect("clear with a terminal filter succeeds");
    assert_eq!(none_failed.deleted, 0, "no run of this World is failed");
    assert!(run_row_survives(f.core.pool(), &applied).await);
    assert!(run_row_survives(f.core.pool(), &discarded).await);

    // The `discarded` filter takes exactly that row.
    let cleared_discarded = clear_compute_runs(
        &f.core,
        &principal,
        ClearRunsQuery {
            status: Some(ClearRunsQueryStatus::Discarded),
            world_id: WORLD.to_string(),
        },
    )
    .await
    .expect("clear succeeds");
    assert_eq!(cleared_discarded.deleted, 1);
    assert!(!run_row_survives(f.core.pool(), &discarded).await);
    assert!(run_row_survives(f.core.pool(), &applied).await);

    // Unfiltered: every terminal row of the World goes — and only those.
    let cleared = clear_compute_runs(
        &f.core,
        &principal,
        ClearRunsQuery {
            status: None,
            world_id: WORLD.to_string(),
        },
    )
    .await
    .expect("clear succeeds");
    assert_eq!(
        cleared.deleted, 1,
        "the applied row was the last terminal one"
    );
    assert!(!run_row_survives(f.core.pool(), &applied).await);
    assert!(
        run_row_survives(f.core.pool(), &pending).await,
        "a succeeded run still needs review"
    );
    assert!(
        run_row_survives(f.core.pool(), &running).await,
        "a running run is not history"
    );
    assert!(
        run_row_survives(f.core.pool(), &foreign).await,
        "another World's terminal row is out of scope"
    );

    // The accepted effect outlived the run row that produced it.
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 15);
    assert_eq!(timeline_rows(f.core.pool()).await, effects_before);

    // The pending run is still readable AS the succeeded run it was, with its
    // proposals intact.
    let detail = get_compute_run(&f.core, &principal, &pending)
        .await
        .expect("pending detail read");
    let detail = serde_json::to_value(&detail).unwrap();
    assert_eq!(detail["status"], "succeeded");
    assert!(detail["proposals"].is_object());

    // The owner door reaches the same authority: nothing terminal is left for
    // this World, and the pending/foreign rows still survive it.
    let handle = open_compute_handle(&f).await;
    let door = handle
        .clear_compute_runs(
            &principal,
            ClearRunsQuery {
                status: None,
                world_id: WORLD.to_string(),
            },
        )
        .await
        .expect("facade clear");
    assert_eq!(door.deleted, 0);
    assert!(run_row_survives(f.core.pool(), &pending).await);
    assert!(run_row_survives(f.core.pool(), &foreign).await);

    // ... and is refused for a World the creator does not own; that World's
    // terminal row is untouched.
    let refused = handle
        .clear_compute_runs(
            &principal,
            ClearRunsQuery {
                status: None,
                world_id: FOREIGN_WORLD.to_string(),
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(refused, CoreError::WorldOwnerDenied { .. }),
        "a foreign World must be refused as ownership, got {refused:?}"
    );
    assert!(run_row_survives(f.core.pool(), &foreign).await);
}

// ---------------------------------------------------------------------------
// Generated-DTO facade on the execution owner (v1.195 P2-T1)
// ---------------------------------------------------------------------------

/// Establish the real execution owner over the fixture's core, with compute
/// wired from the SAME engine, cache and serializer the free authority uses —
/// so a facade call and a direct authority call are two doors into ONE owner.
async fn open_compute_handle(f: &Fixture) -> Arc<nexus_core::execution::ExecutionHandle> {
    f.core
        .start_execution(
            Arc::new(NullProvider) as Arc<dyn nexus_provider_ports::ProviderPort>,
            nexus_core::execution::RunnerDeps {
                nexus_home: Some(f.tmp.path().join(".nexus42")),
                compute_engine: f.compute.engine.clone(),
                compute_cache: f.compute.cache.clone(),
                compute_serializer: Some(Arc::clone(&f.compute.serializer)),
                ..nexus_core::execution::RunnerDeps::default()
            },
        )
        .await
        .expect("execution owner starts")
}

/// The facade's discovery surface serves the REAL registry: the same module
/// rows, the same manifest detail (including the invocation schema Run Studio
/// renders) and the same `not_found` for an unknown module.
///
/// An empty or placeholder catalog is the failure this guards: the assertion
/// is against the authority's own listing, so a facade that returned a stub
/// would disagree with it rather than merely look plausible.
#[tokio::test]
#[serial_test::serial]
async fn compute_facade_discovery_serves_the_real_module_registry() {
    let f = fixture().await;
    let handle = open_compute_handle(&f).await;
    let principal = f.core.active_principal().await.unwrap();

    let listed = handle
        .list_compute_modules(&principal)
        .expect("the facade lists the registry");
    assert!(
        !listed.has_more,
        "the embedded registry is one complete page, never a truncated one"
    );

    let authority = nexus_core::execution::compute::list_compute_modules().expect("registry");
    let row = listed
        .items
        .iter()
        .find(|m| m.module_id == MODULE)
        .unwrap_or_else(|| panic!("{MODULE} must be listed by the facade: {listed:?}"));
    let expected = authority
        .iter()
        .find(|m| m.module_id == MODULE)
        .expect("the authority lists the embedded module");
    assert_eq!(row.name, expected.name);
    assert_eq!(row.version, expected.version);
    assert_eq!(row.description, expected.description);
    assert_eq!(row.battle_report_kind, expected.battle_report_kind);
    assert_eq!(
        row.required_key_block_types,
        expected.required_key_block_types
    );
    assert_eq!(row.status.to_string(), expected.status.to_string());

    let detail = handle
        .get_compute_module(&principal, MODULE)
        .expect("the facade returns the module detail");
    let expected_detail =
        nexus_core::execution::compute::get_compute_module(MODULE).expect("authority detail");
    assert_eq!(detail.module_id, expected_detail.module_id);
    assert_eq!(detail.nexus_abi_version, expected_detail.nexus_abi_version);
    assert_eq!(
        serde_json::to_value(&detail.schemas).unwrap(),
        serde_json::to_value(&expected_detail.schemas).unwrap(),
        "the detail must carry the real manifest schemas, not a placeholder"
    );
    assert!(
        detail
            .schemas
            .as_ref()
            .is_some_and(|schemas| !schemas.invocation.is_empty()),
        "the invocation schema Run Studio renders must be present: {detail:?}"
    );

    let unknown = handle
        .get_compute_module(&principal, "no-such-module")
        .unwrap_err();
    assert!(
        matches!(unknown, CoreError::NotFound { .. }),
        "an unknown module must stay not_found through the facade, got {unknown:?}"
    );
}

/// The facade's run detail is the authority's own row: the run produced
/// through the owner is returned with the SAME id, status and stored
/// proposals, and it is listed under its World/module/status filters.
#[tokio::test]
#[serial_test::serial]
async fn compute_facade_run_detail_and_history_are_the_authority_rows() {
    let f = fixture().await;
    let handle = open_compute_handle(&f).await;
    let principal = f.core.active_principal().await.unwrap();

    let run_id = handle
        .compute_run(&principal, run_request(WORLD, MODULE))
        .await
        .expect("the owner runs the module")
        .run_id;

    // Running is not accepting: the proposals are persisted, the World is not
    // touched.
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 30);
    assert_eq!(timeline_event_count(f.core.pool()).await, 0);

    let detail = handle
        .get_compute_run(&principal, run_id.clone())
        .await
        .expect("facade run detail");
    let authority = get_compute_run(&f.core, &principal, &run_id)
        .await
        .expect("authority run detail");
    assert_eq!(detail.run_id, authority.run_id, "same run id");
    assert_eq!(detail.module_id, authority.module_id);
    assert_eq!(detail.status.to_string(), authority.status.to_string());
    assert_eq!(
        serde_json::to_value(&detail.proposals).unwrap(),
        serde_json::to_value(&authority.proposals).unwrap(),
        "the facade detail must be the authority's stored proposals"
    );
    assert!(
        detail.proposals.is_some(),
        "a succeeded run's detail carries its proposals: {detail:?}"
    );

    let page = handle
        .list_compute_runs(
            &principal,
            ListRunsQuery {
                world_id: Some(WORLD.to_string()),
                module_id: Some(MODULE.to_string()),
                status: Some(ListRunsQueryStatus::Succeeded),
                ..ListRunsQuery::default()
            },
        )
        .await
        .expect("facade history");
    assert!(
        page.items.iter().any(|r| r.run_id == run_id),
        "the run must be listed under its World/module/status filters: {page:?}"
    );
}

/// History scope and query validation: another creator's World is neither
/// readable nor listable through the facade, and the generated query's integer
/// bound is validated rather than coerced into a page size nobody asked for.
#[tokio::test]
#[serial_test::serial]
async fn compute_facade_history_scopes_to_owned_worlds_and_validates_limit() {
    let f = fixture().await;
    let handle = open_compute_handle(&f).await;
    let principal = f.core.active_principal().await.unwrap();
    let run_id = run_succeeded(&f, &f.compute).await;

    seed_foreign_world(f.core.pool()).await;
    let foreign = nexus_local_db::compute_runs::insert_run(
        f.core.pool(),
        FOREIGN_WORLD,
        MODULE,
        Some("1.0.0"),
        None,
        None,
        None,
    )
    .await
    .unwrap();
    let refused = handle
        .get_compute_run(&principal, foreign.clone())
        .await
        .unwrap_err();
    // Existence opacity (V1.206 D4): a foreign-World run closes with the same
    // `not_found` shape as an unknown id, never a distinguishable ownership
    // denial.
    match refused {
        CoreError::NotFound { resource } => assert_eq!(
            resource,
            format!("run {foreign} not found"),
            "a foreign run must close like an unknown id"
        ),
        other => panic!("a foreign run must be refused as not_found, got {other:?}"),
    }

    let unscoped = handle
        .list_compute_runs(
            &principal,
            ListRunsQuery {
                limit: Some(100),
                ..ListRunsQuery::default()
            },
        )
        .await
        .expect("facade history without a World filter");
    assert!(
        !unscoped.items.iter().any(|r| r.run_id == foreign),
        "a foreign-world run must never be listed: {unscoped:?}"
    );
    assert!(unscoped.items.iter().any(|r| r.run_id == run_id));

    let negative = handle
        .list_compute_runs(
            &principal,
            ListRunsQuery {
                limit: Some(-1),
                ..ListRunsQuery::default()
            },
        )
        .await
        .unwrap_err();
    assert!(
        matches!(negative, CoreError::Coded { ref code, .. } if code == "invalid_input"),
        "a negative limit must be invalid_input, got {negative:?}"
    );
}

/// The facade's discard returns the generated response, consumes the SAME CAS
/// the authority uses, and leaves the World exactly as it was.
#[tokio::test]
#[serial_test::serial]
async fn compute_facade_discard_uses_the_shared_cas_and_touches_no_world_state() {
    let f = fixture().await;
    let handle = open_compute_handle(&f).await;
    let principal = f.core.active_principal().await.unwrap();
    let run_id = run_succeeded(&f, &f.compute).await;

    let response = handle
        .discard_compute_run(&principal, run_id.clone())
        .await
        .expect("facade discard");
    assert_eq!(response.run_id, run_id);
    assert_eq!(response.status.to_string(), "discarded");

    // The World keeps the pre-run state and gains no timeline event.
    assert_eq!(defender_hp(f.core.pool(), "kb_def").await, 30);
    assert_eq!(timeline_event_count(f.core.pool()).await, 0);

    // The durable row is the authority's row, flipped by the shared CAS.
    let detail = handle
        .get_compute_run(&principal, run_id.clone())
        .await
        .expect("facade detail after discard");
    assert_eq!(detail.status.to_string(), "discarded");
    let second = handle
        .discard_compute_run(&principal, run_id)
        .await
        .unwrap_err();
    assert!(
        matches!(second, CoreError::Coded { ref code, .. } if code == "conflict"),
        "a second discard must lose the CAS, got {second:?}"
    );
}

/// The facade honours the owner fence: a draining owner refuses every entry
/// point instead of reading beside its own drain.
#[tokio::test]
#[serial_test::serial]
async fn compute_facade_is_fenced_once_the_owner_closes() {
    let f = fixture().await;
    let handle = open_compute_handle(&f).await;
    let principal = f.core.active_principal().await.unwrap();

    handle.close().await.expect("close");
    assert!(handle.is_draining(), "close must set the draining barrier");

    let discovery = handle.list_compute_modules(&principal).unwrap_err();
    assert!(
        matches!(discovery, CoreError::Closing),
        "module discovery must be fenced after close, got {discovery:?}"
    );
    let detail = handle.get_compute_module(&principal, MODULE).unwrap_err();
    assert!(
        matches!(detail, CoreError::Closing),
        "module detail must be fenced after close, got {detail:?}"
    );
    let history = handle
        .list_compute_runs(&principal, ListRunsQuery::default())
        .await
        .unwrap_err();
    assert!(
        matches!(history, CoreError::Closing),
        "run history must be fenced after close, got {history:?}"
    );
    let run = handle
        .get_compute_run(&principal, "run-any".to_string())
        .await
        .unwrap_err();
    assert!(
        matches!(run, CoreError::Closing),
        "run detail must be fenced after close, got {run:?}"
    );
    let discard = handle
        .discard_compute_run(&principal, "run-any".to_string())
        .await
        .unwrap_err();
    assert!(
        matches!(discard, CoreError::Closing),
        "discard must be fenced after close, got {discard:?}"
    );
}

// ---------------------------------------------------------------------------
// v1.207 P3 — durable operation receipts on the compute Run write path
// ---------------------------------------------------------------------------

/// The receipt shadowing a stuck run row is reachable from the run row alone
/// (spec §B.2 `subject_id`), which is the handle boot recovery asks with.
async fn receipt_for_run(f: &Fixture, run_id: &str) -> Option<nexus_local_db::OperationReceipt> {
    nexus_local_db::operation_receipts::get_operation_receipt_by_subject(
        f.core.pool(),
        nexus_local_db::operation_receipts::CONSUMER_COMPUTE_RUN,
        run_id,
    )
    .await
    .expect("receipt read")
}

/// Seed a direct-lane run row left `running` (the crash trace boot recovery
/// scans for), and optionally its receipt.
async fn seed_stuck_run(f: &Fixture, with_receipt: Option<&str>) -> String {
    let run_id = nexus_local_db::compute_runs::insert_run(
        f.core.pool(),
        WORLD,
        MODULE,
        None,
        None,
        None,
        None,
    )
    .await
    .expect("insert a stuck run row");
    if let Some(operation_id) = with_receipt {
        nexus_local_db::operation_receipts::begin_operation(
            f.core.pool(),
            operation_id,
            nexus_local_db::operation_receipts::CONSUMER_COMPUTE_RUN,
            &run_id,
            "fp-stuck",
        )
        .await
        .expect("seed the run's receipt");
    }
    run_id
}

/// The run's durable receipt shadows the `compute_sessions` transitions: the
/// `running` receipt is written before the effect and settled terminally with
/// the run, carrying the SAME payload the run row does (spec §B.2).
#[tokio::test]
#[serial_test::serial]
async fn compute_run_receipt_shadows_the_run_transitions() {
    let f = fixture_with_compute(&[("loop", loop_manifest(), loop_wasm())], None).await;
    let succeeded = run_succeeded(&f, &f.compute).await;

    let receipt = receipt_for_run(&f, &succeeded)
        .await
        .expect("a succeeded run has a durable receipt");
    assert_eq!(
        receipt.status,
        nexus_local_db::operation_receipts::STATUS_FINISHED
    );
    assert_eq!(receipt.consumer, "compute_run");
    assert_eq!(receipt.subject_id, succeeded);
    assert!(receipt.terminal_at.is_some());
    assert!(
        receipt.error_json.is_none(),
        "a finished receipt carries a result, never an error: {receipt:?}"
    );
    nexus_core::execution::operation_id::validate_operation_id(&receipt.operation_id)
        .expect("the shadowed receipt carries a wire-shaped operation id");
    let row = nexus_local_db::compute_runs::get_run(f.core.pool(), &succeeded)
        .await
        .unwrap()
        .expect("the run row");
    // The finished receipt stores the run's EXACT answer (a `RunResponse`), so a
    // replay is reconstructable from the receipt alone — it must survive Clear
    // deleting the run row. Its proposals are the run row's own proposals.
    let stored: serde_json::Value = serde_json::from_str(
        receipt
            .result_json
            .as_deref()
            .expect("a finished receipt carries its answer"),
    )
    .expect("the stored payload is the run response JSON");
    assert_eq!(stored["run_id"], json!(succeeded));
    assert_eq!(stored["status"], json!("succeeded"));
    let row_proposals: serde_json::Value = serde_json::from_str(
        row.proposals_json
            .as_deref()
            .expect("the succeeded row carries proposals"),
    )
    .unwrap();
    assert_eq!(
        stored["proposals"], row_proposals,
        "the receipt's answer carries the run's own proposals"
    );

    // A failed run shadows the other terminal transition.
    let _ = compute_run(&f.core, &f.compute, run_request(WORLD, "loop"))
        .await
        .expect_err("an exhausted-fuel module fails the run");
    let (failed_run_id, status, error_json): (String, String, Option<String>) = sqlx::query_as(
        "SELECT run_id, status, error_json FROM compute_sessions \
          WHERE module_id = 'loop' AND run_id IS NOT NULL \
          ORDER BY created_at DESC LIMIT 1",
    )
    .fetch_one(f.core.pool())
    .await
    .expect("the failed run is persisted");
    assert_eq!(status, "failed");
    let failed_receipt = receipt_for_run(&f, &failed_run_id)
        .await
        .expect("a failed run has a durable receipt");
    assert_eq!(
        failed_receipt.status,
        nexus_local_db::operation_receipts::STATUS_FAILED
    );
    assert_eq!(
        failed_receipt.error_json, error_json,
        "the failed receipt carries the run row's own error payload"
    );
    assert!(failed_receipt.result_json.is_none());
}

/// §B.3 item 4, the guard: a run stuck `running` with no terminal receipt is
/// refused as the typed `uncertain` answer — never re-applied, and no terminal
/// is fabricated for it. Both ambiguous shapes are covered: a receipt-less run
/// and a `running` receipt whose owner is gone.
#[tokio::test]
#[serial_test::serial]
async fn boot_recovery_refuses_stuck_runs_uncertain_and_never_reapplies() {
    let f = fixture().await;
    let no_receipt = seed_stuck_run(&f, None).await;
    let with_receipt_operation = format!("op_{}", "c".repeat(32));
    let with_receipt = seed_stuck_run(&f, Some(&with_receipt_operation)).await;

    let recovered = recover_stuck_compute_runs(f.core.pool())
        .await
        .expect("recovery classification");

    let receipt_less = recovered
        .iter()
        .find(|entry| entry.run_id == no_receipt)
        .expect("the receipt-less stuck run is classified");
    assert!(receipt_less.operation_id.is_none());
    let live_receipt = recovered
        .iter()
        .find(|entry| entry.run_id == with_receipt)
        .expect("the stuck run with a running receipt is classified");
    assert_eq!(
        live_receipt.operation_id.as_deref(),
        Some(with_receipt_operation.as_str())
    );

    for entry in [receipt_less, live_receipt] {
        match &entry.outcome {
            ComputeRunRecoveryOutcome::Uncertain(error) => {
                assert!(
                    matches!(error, CoreError::Coded { code, .. } if code == "uncertain"),
                    "the refusal must carry the frozen `uncertain` code, got {error:?}"
                );
            }
            other @ ComputeRunRecoveryOutcome::AnsweredFromReceipt { .. } => {
                panic!("a stuck run must be refused uncertain, got {other:?}")
            }
        }
    }

    // Nothing was re-applied and no terminal was fabricated: both rows are
    // byte-identical to the state the crash left behind.
    for run_id in [&no_receipt, &with_receipt] {
        let row = nexus_local_db::compute_runs::get_run(f.core.pool(), run_id)
            .await
            .unwrap()
            .expect("the stuck row survives recovery");
        assert_eq!(row.status, "running", "recovery never settles a stuck row");
        assert!(row.error_json.is_none());
    }
    // The orphaned receipt is untouched too — an orphan is never settled.
    let stored = nexus_local_db::operation_receipts::get_operation_receipt(
        f.core.pool(),
        &with_receipt_operation,
    )
    .await
    .unwrap()
    .expect("the orphaned receipt");
    assert_eq!(
        stored.status,
        nexus_local_db::operation_receipts::STATUS_RUNNING
    );
}

/// §B.3 step 3's other half: when the stuck run's receipt is TERMINAL the
/// replay is answered **from the receipt** — the complete stored answer
/// travels out with the decision (success payload and non-success reason
/// verbatim), and the effect is never re-applied.
#[tokio::test]
#[serial_test::serial]
async fn boot_recovery_answers_a_terminal_receipt_without_reapplying() {
    let f = fixture().await;
    let finished_operation = format!("op_{}", "d".repeat(32));
    let finished_run = seed_stuck_run(&f, Some(&finished_operation)).await;
    nexus_local_db::operation_receipts::settle_operation(
        f.core.pool(),
        &finished_operation,
        nexus_local_db::operation_receipts::STATUS_FINISHED,
        r#"{"state_delta":[],"new_key_blocks":[]}"#,
    )
    .await
    .expect("settle the receipt terminally");

    let failed_operation = format!("op_{}", "e".repeat(32));
    let failed_run = seed_stuck_run(&f, Some(&failed_operation)).await;
    nexus_local_db::operation_receipts::settle_operation(
        f.core.pool(),
        &failed_operation,
        nexus_local_db::operation_receipts::STATUS_FAILED,
        r#"{"code":"compute_fuel_exhausted","message":"out of fuel"}"#,
    )
    .await
    .expect("settle the receipt as failed");

    let recovered = recover_stuck_compute_runs(f.core.pool())
        .await
        .expect("recovery classification");

    for (run_id, operation_id, expected_status, expected_result, expected_error) in [
        (
            &finished_run,
            &finished_operation,
            "finished",
            Some(r#"{"state_delta":[],"new_key_blocks":[]}"#),
            None,
        ),
        (
            &failed_run,
            &failed_operation,
            "failed",
            None,
            Some(r#"{"code":"compute_fuel_exhausted","message":"out of fuel"}"#),
        ),
    ] {
        let entry = recovered
            .iter()
            .find(|entry| entry.run_id == *run_id)
            .expect("the stuck run is classified");
        assert_eq!(entry.operation_id.as_deref(), Some(operation_id.as_str()));
        match &entry.outcome {
            ComputeRunRecoveryOutcome::AnsweredFromReceipt { receipt } => {
                assert_eq!(
                    receipt.status.to_string(),
                    expected_status,
                    "the answer carries the stored terminal status"
                );
                assert_eq!(receipt.consumer.to_string(), "compute_run");
                assert_eq!(receipt.subject_id.to_string(), *run_id);
                assert_eq!(
                    receipt.operation_id.to_string(),
                    *operation_id,
                    "the answer carries the stored receipt's own id"
                );
                assert!(receipt.terminal_at.is_some());
                assert_eq!(
                    receipt.result_json.as_deref(),
                    expected_result,
                    "the success payload travels verbatim"
                );
                assert_eq!(
                    receipt.error_json.as_deref(),
                    expected_error,
                    "the non-success reason travels verbatim"
                );
            }
            other @ ComputeRunRecoveryOutcome::Uncertain(_) => {
                panic!("a terminal receipt must answer the replay, got {other:?}")
            }
        }
    }

    // The replay was answered, not applied: both stuck rows are exactly as the
    // crash left them — recovery settles nothing and runs no module.
    for run_id in [&finished_run, &failed_run] {
        let row = nexus_local_db::compute_runs::get_run(f.core.pool(), run_id)
            .await
            .unwrap()
            .expect("the stuck row survives recovery");
        assert_eq!(row.status, "running");
        assert!(row.proposals_json.is_none());
        assert!(row.error_json.is_none());
    }
}

/// The boot wiring is real, not just a callable: an execution start over a
/// workspace holding a stuck run records the receipt-first decision on the
/// owner, and the handle reports it. Recovery never re-applies, so a decided
/// run is still exactly as the crash left it.
#[tokio::test]
#[serial_test::serial]
async fn execution_start_runs_the_compute_receipt_recovery_pass() {
    let f = fixture().await;
    let run_id = seed_stuck_run(&f, None).await;
    // A second stuck run whose receipt already settled terminally: its replay
    // answer must be served from the receipt through the startup surface.
    let settled_operation = format!("op_{}", "f".repeat(32));
    let settled_run = seed_stuck_run(&f, Some(&settled_operation)).await;
    nexus_local_db::operation_receipts::settle_operation(
        f.core.pool(),
        &settled_operation,
        nexus_local_db::operation_receipts::STATUS_FINISHED,
        r#"{"state_delta":[{"op":"set"}]}"#,
    )
    .await
    .expect("settle the receipt terminally");

    let handle = open_compute_handle(&f).await;
    let recovered = handle.compute_run_recoveries();
    let entry = recovered
        .iter()
        .find(|entry| entry.run_id == run_id)
        .expect("the boot pass classified the stuck run");
    assert!(
        matches!(
            &entry.outcome,
            ComputeRunRecoveryOutcome::Uncertain(error)
                if matches!(error, CoreError::Coded { code, .. } if code == "uncertain")
        ),
        "the boot pass must refuse the stuck run uncertain, got {:?}",
        entry.outcome
    );
    let row = nexus_local_db::compute_runs::get_run(f.core.pool(), &run_id)
        .await
        .unwrap()
        .expect("the stuck row survives the boot pass");
    assert_eq!(row.status, "running");

    let settled = recovered
        .iter()
        .find(|entry| entry.run_id == settled_run)
        .expect("the boot pass classified the settled run");
    match &settled.outcome {
        ComputeRunRecoveryOutcome::AnsweredFromReceipt { receipt } => {
            assert_eq!(
                receipt.result_json.as_deref(),
                Some(r#"{"state_delta":[{"op":"set"}]}"#),
                "the startup surface carries the receipt's terminal answer verbatim"
            );
            assert_eq!(receipt.status.to_string(), "finished");
            assert_eq!(receipt.subject_id.to_string(), settled_run);
        }
        other @ ComputeRunRecoveryOutcome::Uncertain(_) => {
            panic!("a terminal receipt must answer the replay, got {other:?}")
        }
    }
    let settled_row = nexus_local_db::compute_runs::get_run(f.core.pool(), &settled_run)
        .await
        .unwrap()
        .expect("the settled row survives the boot pass");
    assert_eq!(settled_row.status, "running");
    assert!(settled_row.proposals_json.is_none());
}

/// Number of direct-lane compute runs the world holds.
///
/// The module is executed only after its run row is inserted, so this count is
/// the observable proxy for "how many times did the module run": a retry that
/// re-ran the module would necessarily add a second row.
async fn compute_run_rows(pool: &sqlx::SqlitePool, world_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM compute_sessions WHERE world_id = ? AND run_id IS NOT NULL",
    )
    .bind(world_id)
    .fetch_one(pool)
    .await
    .unwrap()
}

/// v1.207 P3 (Greptile #3): a caller-supplied `operation_id` stabilizes retry
/// identity.
///
/// A retry after a lost response is answered **from the receipt** — the module
/// is NOT executed a second time (exactly one run row, and the replay carries
/// the stored run id). A *different* request under the same id is the typed
/// `operation_id_conflict` refusal, never a silent re-run.
#[tokio::test]
async fn caller_supplied_operation_id_dedupes_a_retry_and_conflicts_on_a_new_request() {
    let f = fixture().await;
    let op = format!("op_{}", "a".repeat(32));

    let mut request = run_request(WORLD, MODULE);
    request.operation_id = Some(op.clone());

    let first = compute_run(&f.core, &f.compute, request.clone())
        .await
        .expect("the first call runs the module");
    assert_eq!(first.status.to_string(), "succeeded");
    assert_eq!(
        compute_run_rows(f.core.pool(), WORLD).await,
        1,
        "the first call runs the module exactly once"
    );

    // Simulated lost response: the caller retries the IDENTICAL request (same
    // caller-supplied id). The receipt answers it; the module does not re-run.
    let replay = compute_run(&f.core, &f.compute, request)
        .await
        .expect("the retry is answered from the receipt");
    assert_eq!(
        replay.run_id, first.run_id,
        "the replay answers with the stored run id"
    );
    assert_eq!(replay.status.to_string(), "succeeded");
    assert_eq!(
        compute_run_rows(f.core.pool(), WORLD).await,
        1,
        "the retry must not execute the module again"
    );

    // A DIFFERENT logical request under the same id is a first-writer-wins
    // conflict, not a replay.
    let mut conflicting = run_request(WORLD, MODULE);
    conflicting.operation_id = Some(op);
    conflicting.invocation_params = serde_json::Map::from_iter([(
        "attacker_id".to_string(),
        serde_json::Value::String("kb_other".to_string()),
    )]);
    let err = compute_run(&f.core, &f.compute, conflicting)
        .await
        .expect_err("a different request under the same id is refused");
    match err {
        CoreError::Coded { code, .. } => assert_eq!(code, "operation_id_conflict"),
        other => panic!("expected operation_id_conflict, got {other:?}"),
    }
    assert_eq!(
        compute_run_rows(f.core.pool(), WORLD).await,
        1,
        "a conflicting request never runs the module"
    );
}

/// v1.207 P3 (Greptile #3, C1): a finished receipt's replay must survive Clear
/// deleting the run row.
///
/// The receipt is the durable answer for its whole lifetime; Clear history can
/// delete the terminal `compute_sessions` row. The identical retry must still be
/// answered FROM THE RECEIPT — no `NotFound`, no re-execution.
#[tokio::test]
async fn finished_receipt_replay_survives_clearing_the_run_history() {
    let f = fixture().await;
    let principal = f.core.active_principal().await.unwrap();
    let op = format!("op_{}", "c".repeat(32));

    let mut request = run_request(WORLD, MODULE);
    request.operation_id = Some(op);
    let first = compute_run(&f.core, &f.compute, request.clone())
        .await
        .expect("the first call runs the module");
    assert_eq!(first.status.to_string(), "succeeded");

    // Accept (the row becomes `applied`, terminal) then Clear the World's
    // terminal history: the run row is deleted, the receipt is not.
    accept_compute_run(
        &f.core,
        &principal,
        &first.run_id,
        accept_request(json!({})),
    )
    .await
    .expect("accept succeeds");
    let cleared = clear_compute_runs(
        &f.core,
        &principal,
        ClearRunsQuery {
            status: None,
            world_id: WORLD.to_string(),
        },
    )
    .await
    .expect("clear succeeds");
    assert!(cleared.deleted >= 1);
    assert!(
        !run_row_survives(f.core.pool(), &first.run_id).await,
        "only the durable receipt remains after Clear"
    );

    let replay = compute_run(&f.core, &f.compute, request)
        .await
        .expect("the receipt answers the replay with no run row");
    assert_eq!(replay.run_id, first.run_id);
    assert_eq!(replay.status.to_string(), "succeeded");
    assert_eq!(
        replay.created_at, first.created_at,
        "the stored answer is byte-identical, timestamp included"
    );
    assert_eq!(
        serde_json::to_value(&replay).unwrap(),
        serde_json::to_value(&first).unwrap(),
        "the replay returns the exact stored first-success answer"
    );
    assert_eq!(
        compute_run_rows(f.core.pool(), WORLD).await,
        0,
        "the replay never re-executes the module"
    );
}

/// v1.207 P3 (Greptile #3, I1): a concurrent first use under one id with
/// DIFFERENT args yields one typed conflict and leaves no permanent `running`
/// duplicate.
#[tokio::test]
async fn concurrent_conflicting_first_use_leaves_no_running_loser() {
    let f = fixture().await;
    let op = format!("op_{}", "b".repeat(32));

    let mut first = run_request(WORLD, MODULE);
    first.operation_id = Some(op.clone());
    let mut second = run_request(WORLD, MODULE);
    second.operation_id = Some(op);
    // A different logical request => a different fingerprint => a genuine
    // first-writer-wins conflict (not a replay).
    second.invocation_params = serde_json::Map::from_iter([
        ("attacker_id".to_string(), json!("kb_atk")),
        ("defender_id".to_string(), json!("kb_def")),
        ("variant".to_string(), json!(2)),
    ]);

    // Polled concurrently: both pass the receipt pre-check (neither has written
    // a receipt yet), both insert a run row, and the receipt decides the single
    // effect owner. The loser must settle its own never-executed run row.
    let (a, b) = tokio::join!(
        compute_run(&f.core, &f.compute, first),
        compute_run(&f.core, &f.compute, second),
    );
    assert_eq!(
        usize::from(a.is_ok()) + usize::from(b.is_ok()),
        1,
        "exactly one attempt owns the effect"
    );
    for outcome in [&a, &b] {
        if let Err(err) = outcome {
            match err {
                CoreError::Coded { code, .. } => assert_eq!(code, "operation_id_conflict"),
                other => panic!("expected operation_id_conflict, got {other:?}"),
            }
        }
    }

    let (succeeded, running): (i64, i64) = sqlx::query_as(
        "SELECT SUM(status = 'succeeded'), SUM(status = 'running') \
           FROM compute_sessions WHERE world_id = ? AND run_id IS NOT NULL",
    )
    .bind(WORLD)
    .fetch_one(f.core.pool())
    .await
    .unwrap();
    assert_eq!(succeeded, 1, "the module executed exactly once");
    assert_eq!(
        running, 0,
        "the conflicting loser was not left as a permanent `running` row"
    );
}
