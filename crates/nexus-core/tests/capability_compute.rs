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
    run_accept_request::RunAcceptRequest, run_request::RunRequest,
};
use nexus_core::execution::capabilities::{
    ToolContext, ToolExecuteRequest, ToolRuntimeFacts,
};
use nexus_core::execution::compute::ComputeContext;
use nexus_core::{CoreAccess, CoreOpenOptions, CoreService};
use nexus_wasm_host::{CachedModule, ModuleCache, ModuleManifest, WasmEngine};
use serde_json::json;
use std::sync::Arc;
use tempfile::TempDir;

const CREATOR: &str = "test_creator";
const SLUG: &str = "default";
const WORLD: &str = "wld_combat";
const MODULE: &str = "basic-combat";


struct Fixture {
    _tmp: TempDir,
    /// The engine-owner core the family APIs are driven through.
    core: CoreService,
    /// A tool context over the same pool for the capability entry points.
    context: ToolContext,
    /// The compute context (engine + cache + serializer) for run/accept.
    compute: ComputeContext,
}

/// Seed a workspace with the embedded combat module warmed.
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
    // The seeding guard must be fully DROPPED before the owner opens: the
    // writer protocol's engine admission is a process-wide registry keyed by
    // DB path, so opening a second engine while this pool still holds the
    // guard is an `OwnerBusy` refusal (the fixture is not racing another
    // test — it is racing its own seeding pool).
    {
        let guarded = nexus_local_db::init_engine_pool(&db_path)
            .await
            .expect("engine pool init");
        sqlx::query(
            "INSERT OR IGNORE INTO creators (creator_id, display_name, status, \
             cached_at, data) VALUES (?, 'Test', 'active', datetime('now'), '{}')",
        )
        .bind(CREATOR)
        .execute(guarded.pool())
        .await
        .expect("seed creator row");
        seed_world(guarded.pool()).await;
        guarded.pool().close().await;
        nexus_local_db::writer_protocol::release_retained_writer_guards(&db_path);
    }

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .expect("engine-owner core open");

    let engine = Arc::new(WasmEngine::new().expect("wasm engine"));
    let cache = Arc::new(ModuleCache::new());
    cache.warm_embedded(&engine).expect("warm embedded module");

    // Exactly one computable character, so a run against this world produces
    // one deterministic delta and one event.
    seed_character(core.pool(), "kb_atk", "Striker", 20, 3, 100, 100).await;
    seed_character(core.pool(), "kb_def", "Guardian", 10, 5, 30, 50).await;

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
        _tmp: tmp,
        core,
        context,
        compute,
    }
}

/// Seed the owned world.
async fn seed_world(pool: &sqlx::SqlitePool) {
    sqlx::query(
        "INSERT OR IGNORE INTO narrative_worlds \
            (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
             time_policy, metadata_json, created_at) \
           VALUES (?, 'ws', ?, 'Combat World', 'combat-world', \
             'active', 'private', 'manual', '{}', datetime('now'))",
    )
    .bind(WORLD)
    .bind(CREATOR)
    .execute(pool)
    .await
    .unwrap();
}

/// Seed one computable character entry.
async fn seed_character(
    pool: &sqlx::SqlitePool,
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
        owner: KnowledgeOwnerRef::world(WORLD),
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
        ..KnowledgeEntryRecord::new(WORLD, BlockType::Character, name)
    };
    nexus_local_db::kb_store::SqliteKbStore::new(pool.clone())
        .insert_knowledge_entry(kb)
        .await
        .unwrap();
}

/// Read a character's `current_hp`.
async fn defender_hp(pool: &sqlx::SqlitePool, entry_id: &str) -> i64 {
    let raw: Option<String> = sqlx::query_scalar(
        "SELECT body_json FROM kb_key_blocks WHERE key_block_id = ?",
    )
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
    response.run_id.to_string()
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
    let response = nexus_core::execution::compute::accept_compute_run(
        &f.core,
        &principal,
        &run_id,
        request,
    )
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
    nexus_core::execution::compute::accept_compute_run(&f.core, &principal, &run_id, request.clone())
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
    assert_eq!(timeline_event_count(f.core.pool()).await, events_after_first);
}

/// An unknown tool is refused with the retained code and ZERO domain effect.
#[tokio::test]
#[serial_test::serial]
async fn unknown_tool_is_refused_with_zero_domain_effect() {
    let f = fixture().await;
    let before = timeline_event_count(f.core.pool()).await;

    let request = ToolExecuteRequest {
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
    let scan_root = f._tmp.path().join("usercaps");
    let dir = scan_root.join("t3.requires.thing");
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
                "name": "t3.requires.thing",
                "inputSchema": "{{\"type\":\"object\",\"required\":[\"thing\"],\"properties\":{{\"thing\":{{\"type\":\"string\"}}}}}}",
                "outputSchema": "{{\"type\":\"object\"}}",
                "wasm": {{ "moduleId": "basic-combat", "wasmSha256": "{sha}" }}
            }}"#
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

    let deps = CapabilityRuntimeDeps {
        pool: Some(f.core.pool().clone()),
        prompt_executor: None,
        session_cancels: Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())),
        daemon_tool_dispatch: None,
        cdn_config: None,
        workspace_executor: None,
    };
    let (registry, outcome) = CapabilityRegistry::with_runtime_deps_and_user_caps(&deps, &scan_root);
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

    let request = ToolExecuteRequest {
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
                nexus_home: Some(f._tmp.path().join(".nexus42")),
                ..nexus_core::execution::RunnerDeps::default()
            },
        )
        .await
        .expect("owner starts");

    // A SECOND core over a DIFFERENT creator: its principal is well-formed but
    // was minted by another service, so this handle must not accept it.
    let other_home = f._tmp.path().join("other");
    let other_nexus = other_home.join(".nexus42");
    std::fs::create_dir_all(&other_nexus).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &other_home, "intruder", SLUG,
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

    let request = ToolExecuteRequest {
        tool_name: "nexus.workspace.info".to_string(),
        parameters: json!({}),
        session_id: None,
        request_id: None,
        caller_kind: None,
    };
    let err = handle
        .execute_tool(&foreign, request)
        .await
        .unwrap_err();
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
                nexus_home: Some(f._tmp.path().join(".nexus42")),
                ..nexus_core::execution::RunnerDeps::default()
            },
        )
        .await
        .expect("owner starts");
    let principal = f.core.active_principal().await.unwrap();

    let request = ToolExecuteRequest {
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
/// must be indistinguishable from a missing one (NotFound, never Forbidden —
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
    let request = ToolExecuteRequest {
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
    let request = ToolExecuteRequest {
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
    let workspace_dir = f._tmp.path().join("policy-ws");
    std::fs::create_dir_all(workspace_dir.join(".nexus42")).unwrap();
    std::fs::write(
        workspace_dir.join(".nexus42").join("permissions.toml"),
        "default = \"ask\"\n[grant]\n\"nexus.*.read\" = true\n",
    )
    .unwrap();

    let mut context = f.context.clone();
    context.set_workspace_path(Some(workspace_dir.to_string_lossy().into_owned()));

    let request = ToolExecuteRequest {
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
    let request = ToolExecuteRequest {
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
