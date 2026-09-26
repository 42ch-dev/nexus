//! P4-T2 Host authority acceptance anchor: a stale Actor epoch is denied
//! before any provider effect, a journal failure after a committed effect
//! refuses success and settles `interrupted` on restart without re-executing
//! provider work, and an unconfirmed authority close keeps its guards.
//!
//! v1.193 P2-T11 disposition of the retired daemon boot fixtures
//! (`boot_native_providers.rs`, `daemon_boot_llm_wiring.rs`): those cases
//! asserted provider REGISTRATION and prompt-executor wiring inside
//! `run_daemon`, observed through the daemon's HTTP `/agent-host/providers`
//! route — a boot composition that retires with the host. The retained
//! provider-effect assertions are already owned here: the admitted port is
//! the only effect path, so a denied authority and a restart settlement both
//! register ZERO provider calls
//! ([`stale_actor_and_journal_failure_never_redispatch`]), and the host stays
//! a single owned slot ([`second_open_host_is_typed_rejected_until_confirmed_close`]).
//! Provider discovery/catalog semantics are owned by the `nexus-agent-host`
//! discovery units (`discovery::path_scan`, `discovery::catalog`); the LLM
//! capability's executor behaviour is owned by
//! `nexus-orchestration::capability::builtins::llm_extract` and the migrated
//! executor-failure case in `capability_compute.rs`. No daemon boot
//! expectation is re-pinned here.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use nexus_agent_host::capability::model::HostStartConfig;
use nexus_agent_host::capability::model::{
    CapabilityDescriptor, FinishReason, HostEvent, HostEventStream, HostOperation, LaunchSpec,
    ManagedSessionHandle, OperationFailedEvent, OperationFinishedEvent, ProbeRequest, ProtocolKind,
    ProviderDescriptor, ProviderHealth, SessionOwner, SessionStopReason, SessionStoppedEvent,
    StatusEvent, StatusLevel,
};
use nexus_agent_host::providers::port::ProviderEventReader;
use nexus_agent_host::{
    HostFacade, HostManager, HostOperationId, HostSessionId, LaunchStrategy, ProviderAdapter,
    ProviderId,
};
use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::{
    CharacterOperationResult, CharacterOperationResultFinishReason,
    CharacterOperationResultRunStatus, NexusCharacterRunCaptureOutcomeStatus,
};
use nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest;
use nexus_contracts::{
    CoreCloseReportState, ProviderCall, ProviderEventBatch, ProviderEventBatchGapReason,
    ProviderReply,
};
use nexus_core::{
    ActorSessionKey, ActorSessionRegistry, ActorViewpoint, AdmittedActor, AdmittedKnowledgeContext,
    CharacterOperationSnapshot, CoreAccess, CoreActorAdmission, CoreError, CoreOpenOptions,
    CoreService, HostHandle,
};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{ensure_creator_row, CreateCharacterParams};
use nexus_provider_ports::{ProviderPort, ProviderResult};
use tempfile::TempDir;
use uuid::Uuid;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
const WORLD: &str = "wld_worldA";

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
    db_path: PathBuf,
    character_id: String,
    binding_id: String,
}

async fn seed_env() -> Env {
    seed_env_as(CREATOR).await
}

/// Seed an environment bound to an explicit creator, so a foreign principal
/// really carries a different creator identity.
async fn seed_env_as(creator: &str) -> Env {
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home, creator, "default",
    ))
    .unwrap();
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{creator}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{creator}\" = \"default\""
        ),
    )
    .unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, creator, "default");
    let (character_id, binding_id) = {
        let guarded = init_engine_pool(&db_path, creator, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        ensure_creator_row(&pool, creator, "Owner").await.unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'wrk', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(WORLD)
        .bind(creator)
        .bind(WORLD)
        .bind(WORLD)
        .execute(&pool)
        .await
        .unwrap();
        let created = nexus_local_db::create_character_with_initial_binding(
            &pool,
            CreateCharacterParams {
                owner_creator_id: creator,
                display_name: "Ada",
                image_uri: None,
                persona_json: "{}",
                world_id: WORLD,
                world_sheet_entry_id: None,
            },
        )
        .await
        .unwrap();
        (created.character.character_id, created.binding.binding_id)
    };
    Env {
        _tmp: tmp,
        user_home,
        db_path,
        character_id,
        binding_id,
    }
}

async fn open_core(env: &Env) -> (CoreService, nexus_core::Principal) {
    let core = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::EngineOwner,
    })
    .await
    .unwrap();
    let principal = core.active_principal().await.unwrap();
    (core, principal)
}

async fn plain_pool(env: &Env) -> sqlx::SqlitePool {
    nexus_local_db::open_pool(&env.db_path).await.unwrap()
}

/// Deterministic provider peer: counts every admitted call so a denial can be
/// proven to have produced no provider effect. `next` never carries events.
struct CountingPort {
    calls: AtomicUsize,
}

impl CountingPort {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
        })
    }

    fn call_count(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

#[async_trait]
impl ProviderPort for CountingPort {
    async fn call(&self, request: ProviderCall) -> ProviderResult<ProviderReply> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Ok(ProviderReply {
            request_id: request.request_id,
            ok: true,
            session_id: Some("sess-counting".to_string()),
            operation_id: Some("op-counting".to_string()),
            health: None,
            error: None,
        })
    }

    async fn next(
        &self,
        operation_id: String,
        _max_events: u32,
        _max_bytes: u32,
    ) -> ProviderResult<ProviderEventBatch> {
        Ok(ProviderEventBatch {
            events: vec![],
            gap: None,
            has_more: false,
            operation_id,
        })
    }
}

async fn admit_character(
    _core: &CoreService,
    principal: &nexus_core::Principal,
    env: &Env,
) -> nexus_core::AdmittedActorContext {
    let pool = plain_pool(env).await;
    let admission = CoreActorAdmission::new(pool);
    admission
        .admit(
            principal.creator_id(),
            AdmittedActor::Character {
                character_id: env.character_id.clone(),
            },
            ActorViewpoint {
                world_id: WORLD.to_string(),
                binding_id: Some(env.binding_id.clone()),
                branch_id: None,
                event_id: None,
            },
        )
        .await
        .unwrap()
}

fn registry_key(
    _registry: &ActorSessionRegistry,
    ctx: &nexus_core::AdmittedActorContext,
    knowledge: nexus_core::ActorKnowledgeIdentity,
    home: &Path,
) -> ActorSessionKey {
    ActorSessionRegistry::key_for("mock-acp", home, None, None, ctx, knowledge).unwrap()
}

/// Old Actor epoch denies BEFORE the provider effect: after a material
/// transition bumps the stored `lifecycle_epoch`, a prompt against the
/// indexed old-epoch session is `actor_conflict actor_session_stale` and the
/// counting provider peer observes zero calls.
#[tokio::test]
async fn stale_actor_and_journal_failure_never_redispatch() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    // Index one old-epoch Actor session without a live host create (the
    // documented integration seam of the registry).
    let ctx = admit_character(&core, &principal, &env).await;
    let knowledge = admitted_knowledge(&core, &principal, &env).await.identity();
    let key = registry_key(handle.actor_sessions(), &ctx, knowledge, &env.user_home);
    let session_id = Uuid::new_v4();
    handle.actor_sessions().insert_indexed_entry(
        key,
        ctx,
        nexus_agent_host::HostSessionId(session_id),
    );

    // Material transition: bump the stored epoch behind the registry.
    {
        let pool = plain_pool(&env).await;
        sqlx::query(
            "UPDATE characters SET lifecycle_epoch = lifecycle_epoch + 1 WHERE character_id = ?1",
        )
        .bind(&env.character_id)
        .execute(&pool)
        .await
        .unwrap();
    }

    let request = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
    >(serde_json::json!({ "kind": "prompt", "content": "hello" }))
    .unwrap();
    let err = handle
        .execute(&principal, session_id.to_string(), request)
        .await
        .expect_err("a stale-epoch prompt must be denied");
    match &err {
        CoreError::ActorConflict { code, .. } => {
            assert_eq!(code, "actor_session_stale", "stale epoch conflict code");
        }
        other => panic!("expected actor_session_stale, got {other:?}"),
    }
    assert_eq!(
        port.call_count(),
        0,
        "the stale denial happens before any provider effect"
    );

    // --- Journal failure after a committed effect: success is refused, the
    // restart settles `interrupted`, and no provider work re-executes.
    core.journal_provider_write_internal("op-r", "sess-r", "mock-acp", "running")
        .await
        .expect("the running write-through lands while the core is open");
    core.close().await.unwrap();
    assert!(
        core.journal_provider_write_internal("op-r", "sess-r", "mock-acp", "finished")
            .await
            .is_err(),
        "a journal write after the service closed must fail, never fake success"
    );

    // Restart: the orphan settles as interrupted and stays queryable without
    // re-executing provider work.
    let (restarted, principal2) = open_core(&env).await;
    let settled = restarted.settle_provider_orphans().await.unwrap();
    assert_eq!(settled, 1, "the active op settles exactly once");
    let operation = restarted
        .provider_operation(&principal2, "op-r".to_string())
        .await
        .unwrap()
        .expect("the journaled op stays queryable after restart");
    assert_eq!(
        operation.status,
        nexus_contracts::CoreProviderOperationStatus::Interrupted
    );
    assert_eq!(
        port.call_count(),
        0,
        "restart settlement re-executes no provider work"
    );

    // --- Close unconfirmed keeps guards: the authority drains its retired
    // ids against the Host, which cannot confirm an unknown id; the close
    // then reports interrupted/unconfirmed and the registry tombstone guard
    // survives the close.
    let h2 = restarted.open_host(CountingPort::new()).await.unwrap();
    let ctx2 = admit_character(&restarted, &principal2, &env).await;
    let knowledge2 = admitted_knowledge(&restarted, &principal2, &env)
        .await
        .identity();
    let key2 = registry_key(h2.actor_sessions(), &ctx2, knowledge2, &env.user_home);
    let sid2 = Uuid::new_v4();
    h2.actor_sessions()
        .insert_indexed_entry(key2, ctx2, nexus_agent_host::HostSessionId(sid2));
    // Retire the id so the close drain attempts a Host shutdown the Host
    // cannot confirm.
    let _retired = h2
        .actor_sessions()
        .retire_character_sessions(&env.character_id);
    let report = h2.close().await.expect("close returns a report");
    assert!(
        !report.cleanup_confirmed,
        "an unconfirmed authority close never reports confirmed cleanup"
    );
    assert!(
        h2.actor_sessions()
            .is_actor_session(&nexus_agent_host::HostSessionId(sid2)),
        "the closed registry keeps the tombstone guard observable"
    );
}

/// C1: a principal minted against a different creator/workspace is denied on
/// every `HostHandle` method, and a mid-session selection drift is denied by
/// the disk re-read — with zero observable Host effect.
#[tokio::test]
async fn host_authority_admits_only_verified_principals() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle = core.open_host(port.clone()).await.unwrap();

    // A foreign principal: a core opened against a genuinely different
    // creator identity, so verify_principal must reject it.
    let foreign = seed_env_as("ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").await;
    let (_, foreign_principal) = open_core(&foreign).await;
    let request = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
    >(serde_json::json!({ "kind": "prompt", "content": "hello" }))
    .unwrap();
    let mut denials = Vec::new();
    denials.push(
        handle
            .create_session(
                &foreign_principal,
                serde_json::from_value(serde_json::json!({ "provider_id": "mock-acp" })).unwrap(),
            )
            .await
            .unwrap_err(),
    );
    denials.push(
        handle
            .execute(&foreign_principal, Uuid::new_v4().to_string(), request)
            .await
            .unwrap_err(),
    );
    denials.push(
        handle
            .query(
                &foreign_principal,
                serde_json::from_value(serde_json::json!({ "query": "list_sessions" })).unwrap(),
            )
            .await
            .unwrap_err(),
    );
    // A foreign principal querying an operation owned by the open's creator
    // is denied, never leaked through the live Host branch.
    denials.push(
        handle
            .query(&foreign_principal, serde_json::from_value(
                serde_json::json!({ "query": "get_operation", "operation_id": Uuid::new_v4().to_string() }),
            ).unwrap())
            .await
            .unwrap_err(),
    );
    for err in denials {
        assert!(
            matches!(err, CoreError::AuthRequired),
            "foreign principals are auth-required, got {err:?}"
        );
    }

    // Selection drift: the on-disk active slug no longer matches the open.
    let nexus_home = env.user_home.join(".nexus42");
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"other\""
        ),
    )
    .unwrap();
    let request = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
    >(serde_json::json!({ "kind": "prompt", "content": "hello" }))
    .unwrap();
    let err = handle
        .execute(&principal, Uuid::new_v4().to_string(), request)
        .await
        .unwrap_err();
    assert!(
        matches!(err, CoreError::AuthRequired),
        "selection drift is auth-required, got {err:?}"
    );

    // Zero observable Host effect: no session was ever created.
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"default\""
        ),
    )
    .unwrap();
    let list = handle
        .query(
            &principal,
            serde_json::from_value(serde_json::json!({ "query": "list_sessions" })).unwrap(),
        )
        .await
        .unwrap();
    assert!(
        list.sessions.is_none_or(|s| s.items.is_empty()),
        "denied admissions created no Host session"
    );
}

/// C2: the established-owner slot admits exactly one `open_host` per open
/// service; a second start is a typed busy rejection, and a confirmed close
/// frees the slot.
#[tokio::test]
async fn second_open_host_is_typed_rejected_until_confirmed_close() {
    let env = seed_env().await;
    let (core, _principal) = open_core(&env).await;
    let first = core.open_host(CountingPort::new()).await.unwrap();
    let err = core
        .open_host(CountingPort::new())
        .await
        .expect_err("a second start must be rejected");
    assert!(
        matches!(err, CoreError::OwnerBusy),
        "second open is typed OwnerBusy, got {err:?}"
    );
    // A confirmed close frees the slot; the first handle still closes cleanly.
    let report = first.close().await.unwrap();
    assert!(report.cleanup_confirmed);
    let _second = core.open_host(CountingPort::new()).await.unwrap();
}

/// I3: a status write never re-attributes the operation — the stored
/// `session_id/provider_id` are write-once at the first insert.
#[tokio::test]
async fn journal_status_update_preserves_stored_identity() {
    let env = seed_env().await;
    let (core, _principal) = open_core(&env).await;
    core.journal_provider_write_internal("op-i", "sess-i", "mock-acp", "running")
        .await
        .unwrap();
    // A later status write carrying mismatched identity must not clobber the
    // stored owner fields.
    core.journal_provider_write_internal("op-i", "sess-OTHER", "mock-OTHER", "cancelled")
        .await
        .unwrap();
    let row = core
        .provider_operation_row_internal("op-i")
        .await
        .unwrap()
        .expect("row exists");
    assert_eq!(row.0, "op-i");
    assert_eq!(row.1, "sess-i", "stored session_id is write-once");
    assert_eq!(row.2, "mock-acp", "stored provider_id is write-once");
    assert_eq!(row.3, "cancelled", "the status update still lands");
}

// ── v1.191 P1 T5 — knowledge fences and session-key revision binding ──────
//
// Durable contract: `.mstar/specs/holder-governance.md` §4.3. The Actor session
// key carries the read-policy kind and the stored World/Character
// `knowledge_revision` pair; a session whose admitted fingerprint no longer
// matches stored state is retired through the existing tombstone machinery
// instead of being reused, and a no-op leaves the key usable.

/// The exact fingerprint the host records for one admission, read from the
/// stored revisions under the knowledge leases.
async fn admitted_knowledge(
    core: &CoreService,
    principal: &nexus_core::Principal,
    env: &Env,
) -> nexus_core::AdmittedKnowledgeContext {
    core.admit_actor_knowledge_view(
        principal,
        &AdmittedActor::Character {
            character_id: env.character_id.clone(),
        },
        ActorViewpoint {
            world_id: WORLD.to_string(),
            binding_id: Some(env.binding_id.clone()),
            branch_id: None,
            event_id: None,
        },
    )
    .await
    .expect("knowledge context admits")
}

/// A running effect holds the World/Character shared knowledge leases: the
/// disclosure edits (exclusive governance leases) refuse busy and only land
/// after the effect drains.
#[tokio::test]
async fn v1191_knowledge_fence_active_effect_blocks_governance_and_drains() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let actor = AdmittedActor::Character {
        character_id: env.character_id.clone(),
    };

    // The admitted knowledge context is what a running stream holds: the
    // Character activity lease plus the World-then-Character shared knowledge
    // leases.
    let effect = admitted_knowledge(&core, &principal, &env).await;
    let refused = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::World,
            WORLD.to_string(),
        )
        .await
        .expect_err("World disclosure edit blocks while the stream runs");
    match &refused {
        CoreError::ActorConflict { code, .. } => assert_eq!(code, "world_busy"),
        other => panic!("expected world_busy, got {other:?}"),
    }
    let refused = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::Character,
            env.character_id.clone(),
        )
        .await
        .expect_err("Character disclosure edit blocks while the stream runs");
    match &refused {
        CoreError::ActorConflict { code, .. } => assert_eq!(code, "character_busy"),
        other => panic!("expected character_busy, got {other:?}"),
    }
    drop(effect);

    // Drained: the same edit lands on both subjects and is then releasable.
    let world_lease = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::World,
            WORLD.to_string(),
        )
        .await
        .expect("the edit acquires once the effect drains");
    let character_lease = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::Character,
            env.character_id.clone(),
        )
        .await
        .expect("the Character edit acquires once the effect drains");
    drop(world_lease);
    drop(character_lease);
    assert_eq!(
        core.admit_actor_knowledge_view(
            &principal,
            &actor,
            ActorViewpoint {
                world_id: WORLD.to_string(),
                binding_id: Some(env.binding_id.clone()),
                branch_id: None,
                event_id: None,
            },
        )
        .await
        .expect("a fresh admission works after the edits drain")
        .identity()
        .policy(),
        nexus_knowledge::world_kb::store::KnowledgeReadPolicy::ActorView
    );
}

/// Two independently opened cores compute the same fingerprint from stored
/// state; a stored knowledge-revision bump (the governance edit) makes them
/// diverge, and a stale session retires on its next use with the retained
/// `actor_session_stale` refusal before any provider effect.
#[allow(clippy::too_many_lines)] // one reuse round-trip asserted across cores
#[tokio::test]
async fn v1191_knowledge_fence_stale_revision_retires_the_session() {
    let env = seed_env().await;
    let (core_a, principal_a) = open_core(&env).await;
    let port = CountingPort::new();
    let handle = core_a.open_host(port.clone()).await.unwrap();

    // The session is indexed with the fingerprint admitted from stored state.
    let ctx = admit_character(&core_a, &principal_a, &env).await;
    let admitted = admitted_knowledge(&core_a, &principal_a, &env).await;
    let identity = admitted.identity();
    assert_eq!(identity.world_revision(), 0);
    assert_eq!(identity.character_revision(), Some(0));
    drop(admitted);
    let key = registry_key(handle.actor_sessions(), &ctx, identity, &env.user_home);
    assert_eq!(key.knowledge, identity, "the key carries the fingerprint");
    let session_id = Uuid::new_v4();
    handle.actor_sessions().insert_indexed_entry(
        key,
        ctx.clone(),
        nexus_agent_host::HostSessionId(session_id),
    );

    // A no-op reuse keeps the key usable, and a second, independently opened
    // core derives the same fingerprint from the same stored state.
    let (core_b, principal_b) = open_core(&env).await;
    let same = admitted_knowledge(&core_b, &principal_b, &env).await;
    assert_eq!(same.identity(), identity, "no-op admissions agree");
    drop(same);
    let reuse = handle
        .actor_sessions()
        .revalidate_knowledge(&nexus_agent_host::HostSessionId(session_id), &identity)
        .expect("an indexed session is revalidated");
    assert!(
        matches!(reuse, nexus_core::KnowledgeReuse::Reusable),
        "no-op admissions leave the reuse key usable, got {reuse:?}"
    );
    assert!(
        handle
            .actor_sessions()
            .context_for(&nexus_agent_host::HostSessionId(session_id))
            .is_some(),
        "a reusable session stays indexed"
    );

    // Governance edit: the stored Character knowledge revision moves.
    {
        let pool = plain_pool(&env).await;
        sqlx::query(
            "UPDATE characters SET knowledge_revision = knowledge_revision + 1 \
             WHERE character_id = ?1",
        )
        .bind(&env.character_id)
        .execute(&pool)
        .await
        .unwrap();
    }
    let bumped = admitted_knowledge(&core_b, &principal_b, &env).await;
    assert_ne!(
        bumped.identity(),
        identity,
        "an independently opened core observes the stored revision change"
    );
    drop(bumped);

    // Next use of the stale session retires it and refuses before any provider
    // effect.
    let request = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
    >(serde_json::json!({ "kind": "prompt", "content": "hello" }))
    .unwrap();
    let err = handle
        .execute(&principal_a, session_id.to_string(), request)
        .await
        .expect_err("a stale knowledge context must be refused on reuse");
    match &err {
        CoreError::ActorConflict { code, .. } => assert_eq!(code, "actor_session_stale"),
        other => panic!("expected actor_session_stale, got {other:?}"),
    }
    assert_eq!(
        port.call_count(),
        0,
        "the stale denial happens before any provider effect"
    );
    let retired = nexus_agent_host::HostSessionId(session_id);
    assert!(
        handle.actor_sessions().context_for(&retired).is_none(),
        "the stale session left the live index"
    );
    assert!(
        handle.actor_sessions().is_actor_session(&retired),
        "the retired id stays recognizably Actor-mode"
    );
    assert_eq!(
        handle
            .actor_sessions()
            .stored_session_owner(&retired)
            .map(|(owner, _, retired)| (owner, retired)),
        Some((CREATOR.to_string(), true)),
        "the tombstone retains its owner for later authorization"
    );

    // The retired id stays an Actor session: a second use is the same stale
    // refusal, never a legacy raw-prompt fallback.
    let request = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
    >(serde_json::json!({ "kind": "prompt", "content": "hello again" }))
    .unwrap();
    let err = handle
        .execute(&principal_a, session_id.to_string(), request)
        .await
        .expect_err("a retired id is never re-admitted as a legacy session");
    match &err {
        CoreError::ActorConflict { code, .. } => assert_eq!(code, "actor_session_stale"),
        other => panic!("expected actor_session_stale, got {other:?}"),
    }
    assert_eq!(
        port.call_count(),
        0,
        "no provider effect for a retired Actor session"
    );
}

/// Migrated from the retired daemon `characters_api.rs`
/// (`restore_same_state_cas_no_op_keeps_session_executable_without_shutdown`):
/// a same-state lifecycle transition is a no-op — it moves neither the
/// revision nor the stored `lifecycle_epoch`, so a session indexed under the
/// current epoch stays reusable, and the post-restore prompt is still admitted
/// and dispatched to the Host instead of being retired as stale.
#[allow(clippy::too_many_lines)] // no-op invariants + the post-no-op operation are one path
#[tokio::test]
async fn retained_same_state_transition_no_op_keeps_the_indexed_session_reusable() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    let ctx = admit_character(&core, &principal, &env).await;
    let knowledge = admitted_knowledge(&core, &principal, &env).await.identity();
    let key = registry_key(handle.actor_sessions(), &ctx, knowledge, &env.user_home);
    let session_id = Uuid::new_v4();
    handle.actor_sessions().insert_indexed_entry(
        key,
        ctx,
        nexus_agent_host::HostSessionId(session_id),
    );

    let (revision, epoch) = {
        let pool = plain_pool(&env).await;
        let row: (i64, i64) = sqlx::query_as(
            "SELECT revision, lifecycle_epoch FROM characters WHERE character_id = ?",
        )
        .bind(&env.character_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        pool.close().await;
        row
    };

    // Restore-to-active on an already active Character: the retained no-op.
    let request = nexus_contracts::generated::core::CoreCharacterTransitionRequest::builder()
        .character_id(env.character_id.clone())
        .expected_revision(revision)
        .target_status(
            nexus_contracts::generated::core::CoreCharacterTransitionRequestTargetStatus::Active,
        )
        .try_into()
        .expect("transition request is wire-valid");
    let response = core
        .transition_character(&principal, request)
        .await
        .expect("a same-state transition admits");
    assert_eq!(response.character.status.to_string(), "active");
    assert_eq!(
        response.character.revision, revision,
        "a no-op transition never bumps the revision"
    );
    let after_epoch: i64 = {
        let pool = plain_pool(&env).await;
        let value: i64 =
            sqlx::query_scalar("SELECT lifecycle_epoch FROM characters WHERE character_id = ?")
                .bind(&env.character_id)
                .fetch_one(&pool)
                .await
                .unwrap();
        pool.close().await;
        value
    };
    assert_eq!(
        after_epoch, epoch,
        "a no-op transition must not retire the current session generation"
    );

    // The indexed session is still live and its reuse key still admits: the
    // epoch-guarded staleness path (proved by
    // `stale_actor_and_journal_failure_never_redispatch`) can only fire once
    // the stored epoch actually moves, so nothing fabricates a shutdown for a
    // no-op.
    let indexed = nexus_agent_host::HostSessionId(session_id);
    assert!(
        handle.actor_sessions().context_for(&indexed).is_some(),
        "a no-op transition never retires the current session generation"
    );
    let reuse = handle
        .actor_sessions()
        .revalidate_knowledge(&indexed, &knowledge)
        .expect("an indexed session revalidates");
    assert!(
        matches!(reuse, nexus_core::KnowledgeReuse::Reusable),
        "a no-op transition leaves the reuse key usable, got {reuse:?}"
    );

    // ── The operation half of the source assertion ────────────────────────
    // The retired daemon case performed the post-restore prompt and asserted
    // exactly one execution (`host.execs == 1`) with a `200`. A no-op restore
    // must leave the session *executable*, so the post-restore prompt is
    // admitted through every authority gate — stored owner, indexed context,
    // stored epoch, knowledge revalidation, Character re-admission — and
    // dispatched to the Host. A generation the no-op had retired would have
    // produced `actor_session_stale` before the Host was ever reached.
    let prompt = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
    >(serde_json::json!({ "kind": "prompt", "content": "ping after the no-op" }))
    .unwrap();
    let dispatched = handle
        .execute(&principal, session_id.to_string(), prompt)
        .await;
    // Observation point: this fixture indexes the session into the registry
    // without a Host create, so the single dispatch lands on the authority's
    // own Host plane and is refused there for the fixture's missing provider
    // mapping — the Host boundary, never an Actor refusal. The composed
    // transport port (`provider_port`) is not the authority's execution path,
    // so it observes no call (the same zero-effect reading the refusal cases
    // above rely on).
    let Err(refusal) = &dispatched else {
        panic!("the post-restore prompt is dispatched to the Host, got {dispatched:?}");
    };
    assert!(
        matches!(refusal, CoreError::Internal { category } if category.starts_with("agent_host:")),
        "the post-restore prompt reaches the Host boundary, got {refusal:?}"
    );
    assert_eq!(
        port.call_count(),
        0,
        "the authority dispatches through its own Host plane, not the composed port"
    );

    // The dispatched prompt leaves no shadow: the authority released its
    // Character operation reservation and its effect fences rather than
    // retiring the generation, so the id is neither tombstoned nor stale and
    // both shared knowledge leases are free again for an exclusive edit.
    assert_eq!(
        handle
            .actor_sessions()
            .stored_session_owner(&indexed)
            .map(|(owner, _, retired)| (owner, retired)),
        Some((CREATOR.to_string(), false)),
        "a dispatched post-restore prompt never tombstones the session"
    );
    assert!(
        handle.actor_sessions().context_for(&indexed).is_some(),
        "the dispatched post-restore prompt keeps the session indexed"
    );
    let world_lease = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::World,
            WORLD.to_string(),
        )
        .await
        .expect("the post-restore prompt returns its World knowledge fence");
    let character_lease = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::Character,
            env.character_id.clone(),
        )
        .await
        .expect("the post-restore prompt returns its Character knowledge fence");
    drop(world_lease);
    drop(character_lease);
}

// ── v1.196 P0-T2 — authoritative Character terminal outcomes ─────────────
//
// Durable contract: `.mstar/iterations/v1.196/specs/current-host-actor-contract.md`
// §5 (D8/D9). The core drain of the ORIGINAL `HostFacade::exec` stream settles
// the FIRST matching terminal/fault of the exact `(session_id, operation_id)`,
// releases that operation's knowledge fences at settlement — not at unrelated
// trailing producer events — and never infers success from EOF, a generic
// session state or a coarse provider journal row. Character `remember:true` is
// a typed pre-effect refusal because this Host has no run-capture writer.

/// One item of a host exec stream.
type HostItem = nexus_agent_host::HostResult<HostEvent>;

/// The matching observations contract §5 classifies, as they reach the drain
/// from a real provider stream.
enum CharacterObservation {
    /// `OpFinished` carrying the provider's stop reason.
    Finished(FinishReason),
    /// `OpFailed` carrying an error category (`max_tokens`, `max_turn_requests`,
    /// `refusal`, `provider_error`, `stream_closed`, …). Contract §5 classifies
    /// every one of them as the fault row; the category is carried so the case
    /// mirrors a real provider stream, never so the drain may reinterpret it.
    Failed(String),
    /// A stream error item.
    StreamError,
    /// A session stop for this operation's session, before any terminal.
    SessionStopped,
    /// EOF with no terminal at all.
    Eof,
}

impl CharacterObservation {
    fn events(&self, session_id: &HostSessionId, operation_id: &HostOperationId) -> Vec<HostItem> {
        match self {
            Self::Finished(reason) => vec![Ok(HostEvent::OpFinished(OperationFinishedEvent {
                session_id: session_id.clone(),
                op_id: operation_id.clone(),
                reason: reason.clone(),
            }))],
            Self::Failed(category) => vec![Ok(HostEvent::OpFailed(OperationFailedEvent {
                session_id: session_id.clone(),
                op_id: operation_id.clone(),
                error_category: category.clone(),
                error_message: format!("{category} from the provider"),
            }))],
            Self::StreamError => vec![Err(nexus_agent_host::HostError::internal("stream fault"))],
            Self::SessionStopped => vec![Ok(HostEvent::SessionStopped(SessionStoppedEvent {
                session_id: session_id.clone(),
                reason: SessionStopReason::ProviderExit,
            }))],
            Self::Eof => vec![],
        }
    }
}

/// A deterministic exec-stream stand-in.
///
/// Once it has yielded an item, a poll that finds the stream exhausted PANICS:
/// reaching EOF after a terminal means the drain waited for unrelated trailing
/// producer events instead of settling on the first match (contract §5). An
/// empty item list is a genuine empty stream, whose EOF must instead settle the
/// operation as failed.
fn exec_stream(items: Vec<HostItem>) -> HostEventStream {
    let mut inner = futures_util::stream::iter(items);
    let mut yielded = false;
    Box::pin(futures_util::stream::poll_fn(
        move |cx| match std::pin::Pin::new(&mut inner).poll_next(cx) {
            std::task::Poll::Ready(Some(item)) => {
                yielded = true;
                std::task::Poll::Ready(Some(item))
            }
            std::task::Poll::Ready(None) if yielded => {
                panic!("the Character drain read past its settled terminal to EOF")
            }
            std::task::Poll::Ready(None) => std::task::Poll::Ready(None),
            std::task::Poll::Pending => std::task::Poll::Pending,
        },
    ))
}

/// Reserve one Character operation the way `execute` does, drive the
/// authority-owned drain over the events `build` produces for that exact
/// `(session_id, operation_id)`, and return the settled owner-scoped outcome.
///
/// The seam it drives (`HostHandle::settle_character_stream`) is
/// `test-hooks`-gated, so it does not exist in a production build: production
/// settlement consumes only the stream `execute` owns. The drain under test is
/// the same production `drain_character_operation`, not a reimplementation.
async fn settle_character_operation(
    core: &CoreService,
    handle: &HostHandle,
    principal: &nexus_core::Principal,
    env: &Env,
    build: impl FnOnce(&HostSessionId, &HostOperationId) -> Vec<HostItem>,
    fenced: Option<AdmittedKnowledgeContext>,
) -> (CharacterOperationResult, HostOperationId) {
    // The snapshot carries only the identity fields the record consumes, so the
    // admitted context is this fixture's own admission step (a plain data
    // read-back, no lease: the fences travel in `fenced`).
    let _admitted = admit_character(core, principal, env).await;
    let operation_id = HostOperationId(Uuid::new_v4());
    let session_id = HostSessionId(Uuid::new_v4());
    let snapshot = CharacterOperationSnapshot::new(
        principal.creator_id().to_string(),
        session_id.clone(),
        operation_id.clone(),
    );
    handle
        .reserve_character_operation(&snapshot)
        .expect("a Character operation reserves an outcome");
    let events = build(&session_id, &operation_id);
    handle
        .settle_character_stream(snapshot, exec_stream(events), fenced)
        .await;
    let outcome = handle
        .character_operation(principal, operation_id.to_string())
        .await
        .expect("the settled outcome is owner-readable");
    (outcome, operation_id)
}

fn finished(
    session_id: &HostSessionId,
    operation_id: &HostOperationId,
    reason: FinishReason,
) -> HostEvent {
    HostEvent::OpFinished(OperationFinishedEvent {
        session_id: session_id.clone(),
        op_id: operation_id.clone(),
        reason,
    })
}

/// A generated execute request from its JSON body.
fn execute_request(body: serde_json::Value) -> ExecuteOperationRequest {
    serde_json::from_value(body).expect("the request body is wire-valid")
}

/// Contract §5 table: every matching observation settles the run status and
/// finish reason the table promises, every Character result stays uncaptured,
/// and an unrelated observation never decides the outcome.
#[allow(clippy::too_many_lines)] // one exhaustive terminal table + the filtering round
#[tokio::test]
async fn character_terminal_contract_table_settles_every_run_status() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    let cases: Vec<(
        &str,
        CharacterObservation,
        CharacterOperationResultRunStatus,
        Option<CharacterOperationResultFinishReason>,
    )> = vec![
        (
            "end_turn",
            CharacterObservation::Finished(FinishReason::EndTurn),
            CharacterOperationResultRunStatus::Succeeded,
            Some(CharacterOperationResultFinishReason::EndTurn),
        ),
        // The `incomplete` row is reachable ONLY through an explicit
        // `OpFinished(MaxTokens | MaxTurnRequests | Refusal)` — the provider
        // named the stop reason in the finished event itself.
        (
            "opfinished max_tokens",
            CharacterObservation::Finished(FinishReason::MaxTokens),
            CharacterOperationResultRunStatus::Incomplete,
            Some(CharacterOperationResultFinishReason::MaxTokens),
        ),
        (
            "opfinished max_turn_requests",
            CharacterObservation::Finished(FinishReason::MaxTurnRequests),
            CharacterOperationResultRunStatus::Incomplete,
            Some(CharacterOperationResultFinishReason::MaxTurnRequests),
        ),
        (
            "opfinished refusal",
            CharacterObservation::Finished(FinishReason::Refusal),
            CharacterOperationResultRunStatus::Incomplete,
            Some(CharacterOperationResultFinishReason::Refusal),
        ),
        (
            "cancelled",
            CharacterObservation::Finished(FinishReason::Cancelled),
            CharacterOperationResultRunStatus::Cancelled,
            Some(CharacterOperationResultFinishReason::Cancelled),
        ),
        // Contract §5 gives `OpFailed` ONE row — failed, finish_reason=null —
        // with no error-category exception. The three categories the adapters
        // use for the non-`EndTurn` stop reasons (`providers/acp.rs`) are
        // therefore faults, NOT the `incomplete` rows above: an adapter's
        // representation cannot amend the terminal table. These three cases are
        // what keeps the strict row asserted for the second event form.
        (
            "acp max_tokens",
            CharacterObservation::Failed("max_tokens".to_string()),
            CharacterOperationResultRunStatus::Failed,
            None,
        ),
        (
            "acp max_turn_requests",
            CharacterObservation::Failed("max_turn_requests".to_string()),
            CharacterOperationResultRunStatus::Failed,
            None,
        ),
        (
            "acp refusal",
            CharacterObservation::Failed("refusal".to_string()),
            CharacterOperationResultRunStatus::Failed,
            None,
        ),
        (
            "provider_error",
            CharacterObservation::Failed("provider_error".to_string()),
            CharacterOperationResultRunStatus::Failed,
            None,
        ),
        (
            "stream_closed",
            CharacterObservation::Failed("stream_closed".to_string()),
            CharacterOperationResultRunStatus::Failed,
            None,
        ),
        (
            "stream error",
            CharacterObservation::StreamError,
            CharacterOperationResultRunStatus::Failed,
            None,
        ),
        (
            "session stopped before terminal",
            CharacterObservation::SessionStopped,
            CharacterOperationResultRunStatus::Failed,
            None,
        ),
        (
            "eof without terminal",
            CharacterObservation::Eof,
            CharacterOperationResultRunStatus::Failed,
            None,
        ),
    ];

    for (label, observation, expected_status, expected_finish) in cases {
        let (outcome, operation_id) = settle_character_operation(
            &core,
            &handle,
            &principal,
            &env,
            |session_id, operation_id| observation.events(session_id, operation_id),
            None,
        )
        .await;
        assert_eq!(outcome.run_status, expected_status, "{label}: run status");
        assert_eq!(
            outcome.finish_reason, expected_finish,
            "{label}: finish reason"
        );
        assert_eq!(
            outcome.capture.status,
            NexusCharacterRunCaptureOutcomeStatus::Disabled,
            "{label}: this batch never captures a Character run"
        );
        assert!(
            outcome.capture.pending_id.is_none() && outcome.capture.code.is_none(),
            "{label}: no fabricated capture id or code"
        );
        assert!(
            outcome.operation_id == operation_id.to_string(),
            "{label}: the outcome names the operation it settled"
        );
    }
    assert_eq!(port.call_count(), 0, "the drain settles no provider effect");

    // An unrelated operation's terminal, another session's fault and a stop of
    // a different session never decide this operation's outcome.
    let other_op = HostOperationId(Uuid::new_v4());
    let other_session = HostSessionId(Uuid::new_v4());
    let (outcome, _) = settle_character_operation(
        &core,
        &handle,
        &principal,
        &env,
        move |session_id, operation_id| {
            vec![
                Ok(finished(session_id, &other_op, FinishReason::Cancelled)),
                Ok(HostEvent::OpFailed(OperationFailedEvent {
                    session_id: session_id.clone(),
                    op_id: other_op.clone(),
                    error_category: "provider_error".to_string(),
                    error_message: "another operation failed".to_string(),
                })),
                Ok(finished(
                    &other_session,
                    operation_id,
                    FinishReason::Refusal,
                )),
                Ok(HostEvent::SessionStopped(SessionStoppedEvent {
                    session_id: other_session.clone(),
                    reason: SessionStopReason::GracefulShutdown,
                })),
                Ok(finished(session_id, operation_id, FinishReason::EndTurn)),
            ]
        },
        None,
    )
    .await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Succeeded,
        "only this (session, operation) decides the outcome"
    );
    assert_eq!(
        outcome.finish_reason,
        Some(CharacterOperationResultFinishReason::EndTurn)
    );
}

/// Contract §5: the first matching terminal/fault is immutable — a trailing
/// duplicate fault and a later success cannot replace it — and the drain
/// settles on that first match instead of waiting for the producer's trailing
/// events (`exec_stream` panics if it reads to EOF).
#[tokio::test]
async fn character_terminal_first_match_is_immutable_and_stops_the_drain() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    let (outcome, operation_id) = settle_character_operation(
        &core,
        &handle,
        &principal,
        &env,
        |session_id, operation_id| {
            let other_op = HostOperationId(Uuid::new_v4());
            vec![
                // Unrelated operations first: neither matches.
                Ok(finished(session_id, &other_op, FinishReason::Refusal)),
                // The first MATCHING observation is this operation's truth.
                Ok(finished(session_id, operation_id, FinishReason::MaxTokens)),
                // Trailing duplicates/faults must not replace it, and must not
                // even be consumed.
                Ok(HostEvent::OpFailed(OperationFailedEvent {
                    session_id: session_id.clone(),
                    op_id: operation_id.clone(),
                    error_category: "provider_error".to_string(),
                    error_message: "a duplicate trailing fault".to_string(),
                })),
                Ok(finished(session_id, operation_id, FinishReason::EndTurn)),
            ]
        },
        None,
    )
    .await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Incomplete,
        "the first matching terminal is the truth"
    );
    assert_eq!(
        outcome.finish_reason,
        Some(CharacterOperationResultFinishReason::MaxTokens)
    );

    // A later settlement of the same operation (a trailing producer fault or a
    // cancel that lost the phase race) is a no-op: the settlement is immutable
    // and never re-enters the retention FIFO. The registry's own writer is
    // crate-visible, so this test build drives it through the authority's gated
    // seam.
    handle.settle_character_terminal(
        &operation_id,
        CharacterOperationResultRunStatus::Succeeded,
        Some(CharacterOperationResultFinishReason::EndTurn),
    );
    let reread = handle
        .character_operation(&principal, operation_id.to_string())
        .await
        .expect("the settled outcome stays owner-readable");
    assert_eq!(
        reread.run_status,
        CharacterOperationResultRunStatus::Incomplete,
        "a duplicate settlement cannot replace the recorded truth"
    );
    assert_eq!(
        reread.finish_reason,
        Some(CharacterOperationResultFinishReason::MaxTokens)
    );
    assert_eq!(
        handle.actor_sessions().character_operation_count(),
        1,
        "the duplicate settlement was not retained a second time"
    );
}

/// Contract §5: an accepted cancellation that won the phase race is the
/// operation's truth — the provider terminal that follows cannot turn it back
/// into a success — and a later cancel is the typed 409 conflict.
#[tokio::test]
async fn character_terminal_accepted_cancel_beats_the_provider_terminal() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    let operation_id = HostOperationId(Uuid::new_v4());
    let session_id = HostSessionId(Uuid::new_v4());
    let snapshot = CharacterOperationSnapshot::new(
        principal.creator_id().to_string(),
        session_id.clone(),
        operation_id.clone(),
    );
    handle
        .reserve_character_operation(&snapshot)
        .expect("a Character operation reserves an outcome");
    handle
        .actor_sessions()
        .request_operation_cancel(principal.creator_id(), &operation_id)
        .expect("the cancel intent latches before the drain finalizes");

    // The provider then reports a clean end of turn; the accepted local cancel
    // already won the race and stays the recorded outcome.
    handle
        .settle_character_stream(
            snapshot,
            exec_stream(vec![Ok(finished(
                &session_id,
                &operation_id,
                FinishReason::EndTurn,
            ))]),
            None,
        )
        .await;
    let outcome = handle
        .character_operation(&principal, operation_id.to_string())
        .await
        .expect("the settled outcome is owner-readable");
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Cancelled,
        "an accepted cancel is never rewritten into a provider success"
    );
    assert_eq!(
        outcome.finish_reason,
        Some(CharacterOperationResultFinishReason::Cancelled)
    );

    // Once terminal, a later cancel is the 409 conflict, never an override.
    let err = handle
        .actor_sessions()
        .request_operation_cancel(principal.creator_id(), &operation_id)
        .expect_err("a finished operation refuses cancel");
    match &err {
        CoreError::ActorConflict { code, .. } => {
            assert_eq!(code, "actor_operation_finished", "finished cancel code");
        }
        other => panic!("expected actor_operation_finished, got {other:?}"),
    }
}

/// Contract §5: the operation's activity guard and shared knowledge leases are
/// released AT settlement, not after unrelated trailing producer events. The
/// stream here stops at the terminal (it panics on a later poll), and the
/// exclusive World/Character disclosure edits land immediately after.
#[tokio::test]
async fn character_terminal_settlement_releases_the_knowledge_fences() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    // The admitted context a running Character effect holds.
    let knowledge = admitted_knowledge(&core, &principal, &env).await;
    let busy = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::World,
            WORLD.to_string(),
        )
        .await
        .expect_err("a held effect blocks the disclosure edit");
    match &busy {
        CoreError::ActorConflict { code, .. } => assert_eq!(code, "world_busy"),
        other => panic!("expected world_busy, got {other:?}"),
    }

    let (outcome, _) = settle_character_operation(
        &core,
        &handle,
        &principal,
        &env,
        |session_id, operation_id| {
            vec![Ok(finished(
                session_id,
                operation_id,
                FinishReason::EndTurn,
            ))]
        },
        Some(knowledge),
    )
    .await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Succeeded
    );

    let world_lease = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::World,
            WORLD.to_string(),
        )
        .await
        .expect("the settlement released the World knowledge fence");
    let character_lease = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::Character,
            env.character_id.clone(),
        )
        .await
        .expect("the settlement released the Character knowledge fence");
    drop(world_lease);
    drop(character_lease);
}

/// Contract §5: detailed outcomes are process-lifetime and bounded — an evicted
/// outcome and an outcome of a reopened authority are MISSING (`not_found`),
/// never an inferred success — and they stay owner-scoped.
#[tokio::test]
async fn character_terminal_eviction_and_reopen_have_no_detailed_outcome() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    // 1025 terminal settlements: the retention window keeps the newest 1024 and
    // drops the oldest instead of answering an evicted id from memory.
    let mut oldest = None;
    let mut newest = None;
    for _ in 0..1025 {
        let operation_id = HostOperationId(Uuid::new_v4());
        handle
            .reserve_character_operation(&CharacterOperationSnapshot::new(
                principal.creator_id().to_string(),
                HostSessionId(Uuid::new_v4()),
                operation_id.clone(),
            ))
            .expect("a Character operation reserves an outcome");
        handle.settle_character_terminal(
            &operation_id,
            CharacterOperationResultRunStatus::Succeeded,
            Some(CharacterOperationResultFinishReason::EndTurn),
        );
        oldest.get_or_insert_with(|| operation_id.clone());
        newest = Some(operation_id);
    }
    assert_eq!(
        handle.actor_sessions().character_operation_count(),
        1024,
        "the terminal retention window stays bounded"
    );
    let oldest = oldest.expect("the first settled operation");
    let newest = newest.expect("the last settled operation");
    let err = handle
        .character_operation(&principal, oldest.to_string())
        .await
        .expect_err("an evicted detailed outcome is missing");
    assert!(
        matches!(err, CoreError::NotFound { .. }),
        "eviction is not_found, got {err:?}"
    );
    let retained = handle
        .character_operation(&principal, newest.to_string())
        .await
        .expect("a retained outcome stays readable");
    assert_eq!(
        retained.run_status,
        CharacterOperationResultRunStatus::Succeeded
    );

    // A fresh authority over the same open service has no detailed outcome for
    // a previous operation: nothing is replayed or inferred after reopen.
    let report = handle.close().await.expect("close returns a report");
    assert!(report.cleanup_confirmed, "{report:?}");
    let reopened = core.open_host(CountingPort::new()).await.unwrap();
    let err = reopened
        .character_operation(&principal, newest.to_string())
        .await
        .expect_err("a reopened authority never replays a detailed outcome");
    assert!(
        matches!(err, CoreError::NotFound { .. }),
        "reopen is not_found, got {err:?}"
    );

    // Owner-scoped: a foreign principal cannot read an outcome at all.
    let foreign = seed_env_as("ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").await;
    let (_, foreign_principal) = open_core(&foreign).await;
    let err = reopened
        .character_operation(&foreign_principal, newest.to_string())
        .await
        .expect_err("a foreign principal is refused");
    assert!(
        matches!(err, CoreError::AuthRequired),
        "foreign read is auth_required, got {err:?}"
    );
}

/// Contract §5 / D9: a Character `remember:true` prompt is refused with the
/// typed `not_supported` envelope BEFORE the operation reserves an outcome,
/// admits an activity/knowledge context or reaches the provider — no capture
/// writer exists to fulfil it. The legacy/Creator refusal is preserved.
#[tokio::test]
async fn character_terminal_remember_refusal_precedes_reservation_and_effects() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    let ctx = admit_character(&core, &principal, &env).await;
    let knowledge = admitted_knowledge(&core, &principal, &env).await.identity();
    let key = registry_key(handle.actor_sessions(), &ctx, knowledge, &env.user_home);
    let session_id = Uuid::new_v4();
    handle
        .actor_sessions()
        .insert_indexed_entry(key, ctx, HostSessionId(session_id));

    let err = handle
        .execute(
            &principal,
            session_id.to_string(),
            execute_request(
                serde_json::json!({ "kind": "prompt", "content": "hello", "remember": true }),
            ),
        )
        .await
        .expect_err("a Character capture request is refused");
    match &err {
        CoreError::Coded { code, .. } => assert_eq!(code, "not_supported"),
        other => panic!("expected the not_supported envelope, got {other:?}"),
    }
    assert_eq!(
        port.call_count(),
        0,
        "the refusal precedes any provider effect"
    );
    assert_eq!(
        handle.actor_sessions().character_operation_count(),
        0,
        "the refusal precedes the operation reservation"
    );
    // Zero memory effects: no activity or knowledge context was admitted, so
    // the exclusive disclosure edit lands at once.
    let lease = core
        .acquire_knowledge_governance(
            &principal,
            nexus_core::ActorFenceKind::World,
            WORLD.to_string(),
        )
        .await
        .expect("the refused prompt admitted no knowledge context");
    drop(lease);

    // The non-Character refusal is unchanged: `remember` on a never-indexed
    // session stays the 422 `invalid_input` refusal.
    let err = handle
        .execute(
            &principal,
            Uuid::new_v4().to_string(),
            execute_request(
                serde_json::json!({ "kind": "prompt", "content": "hello", "remember": true }),
            ),
        )
        .await
        .expect_err("remember on a non-Character session stays invalid_input");
    match &err {
        CoreError::InvalidInput { field, .. } => assert_eq!(field, "remember"),
        other => panic!("expected the invalid `remember` refusal, got {other:?}"),
    }
    assert_eq!(port.call_count(), 0, "the legacy refusal has no effects");
}

/// Contract §5 / §4: a native list/get row keeps its Actor pair — the live
/// admitted context first, then the owner-retaining tombstone — so a native or
/// cached read never downgrades an Actor session into a provider-only one. The
/// `HostHandle::query` rows render exactly the pair this overlay returns.
#[tokio::test]
async fn actor_echo_keeps_the_actor_pair_for_live_and_retired_sessions() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let handle: HostHandle = core.open_host(CountingPort::new()).await.unwrap();
    let ctx = admit_character(&core, &principal, &env).await;
    let knowledge = admitted_knowledge(&core, &principal, &env).await.identity();
    let key = registry_key(handle.actor_sessions(), &ctx, knowledge, &env.user_home);
    let session_id = HostSessionId(Uuid::new_v4());
    handle
        .actor_sessions()
        .insert_indexed_entry(key, ctx, session_id.clone());

    // A never-indexed (legacy/provider-only) row carries no Actor pair.
    assert!(
        matches!(
            handle
                .actor_sessions()
                .echo_actor_pair_for_session(&HostSessionId(Uuid::new_v4()))
                .expect("an unknown id echoes nothing"),
            (None, None)
        ),
        "a provider-only row is not Actor-shaped"
    );

    for (label, expected_retired) in [("live indexed context", false), ("retired tombstone", true)]
    {
        let (actor_ref, viewpoint) = handle
            .actor_sessions()
            .echo_actor_pair_for_session(&session_id)
            .expect("the row echoes its Actor pair");
        match actor_ref.expect("an Actor row echoes its identity") {
            nexus_contracts::core_host_query_response::NexusActorRef::CharacterActorRef {
                character_id,
                ..
            } => assert!(
                character_id.as_str() == env.character_id,
                "{label}: the Character bearer id survives"
            ),
            ref other @ nexus_contracts::core_host_query_response::NexusActorRef::CreatorActorRef {
                ..
            } => panic!("{label}: expected a Character actor ref, got {other:?}"),
        }
        let viewpoint = viewpoint.expect("an Actor row echoes its viewpoint");
        assert!(viewpoint.world_id.as_str() == WORLD, "{label}: world id");
        assert!(
            viewpoint
                .binding_id
                .as_ref()
                .map(|id| id.as_str().to_string())
                == Some(env.binding_id.clone()),
            "{label}: binding id"
        );
        if !expected_retired {
            let retired = handle
                .actor_sessions()
                .retire_character_sessions(&env.character_id);
            assert!(retired == vec![session_id.clone()], "the id retires once");
        }
    }
    assert!(
        handle
            .actor_sessions()
            .stored_session_owner(&session_id)
            .map(|(owner, _, retired)| (owner, retired))
            == Some((CREATOR.to_string(), true)),
        "the echoed pair comes from the owner-retaining tombstone"
    );
}

// ── v1.196 P0-T1 — attached core authority lifetime ──────────────────────
//
// Durable contract: `.mstar/iterations/v1.196/specs/current-host-actor-contract.md`
// §2/§3 (D8). The core authority attaches ONCE to an already-started native
// `HostManager` (the manager the native open owns) instead of constructing a
// second Host. Both constructors share the single established-owner slot, the
// exact supplied manager/port are retained, every authority carries its own
// closed identity, and the Actor drains it mints stay owned instead of being
// detached and forgotten.

/// The `max_sessions` the attach fixtures bake into their manager: a manager
/// started by `open_host_inner` would carry the on-disk default instead, so a
/// matching value proves the supplied instance — not a fresh Host — is the one
/// the authority acts on.
const ATTACHED_MANAGER_MAX_SESSIONS: usize = 4242;

/// The canonical agent-host config path the standalone `open_host` reads for
/// this env: the Nexus root plus `agent-host/config.toml`, i.e. exactly what
/// `agent_host_config_path` resolves from the raw user home the native boot
/// passes it. The helper takes the HOME, so it is handed `env.user_home` once.
fn host_config_path(env: &Env) -> PathBuf {
    nexus_agent_host::config::agent_host_config_path(&env.user_home)
}

/// Start a native manager the way the native open composes one — the
/// already-started manager an `attach_host` call must adopt rather than start
/// again.
async fn start_host_manager(env: &Env) -> Arc<HostManager> {
    use nexus_agent_host::config::{load_config_from_path, validate_workspace_path};
    use nexus_agent_host::core::readiness::discover_provider_catalog;

    let workspace_root = nexus_home_layout::nexus_root_from_home(&env.user_home);
    validate_workspace_path(&workspace_root).unwrap();
    let config_path = host_config_path(env);
    let mut host_config = load_config_from_path(&config_path).unwrap();
    host_config.max_sessions = ATTACHED_MANAGER_MAX_SESSIONS;
    let admitted_catalog = discover_provider_catalog(&host_config).unwrap();
    let host = Arc::new(HostManager::new());
    host.start(HostStartConfig {
        config_path,
        workspace_root,
        max_sessions: host_config.max_sessions,
        max_ops_per_session: host_config.max_ops_per_session,
        timeouts: host_config.timeouts.clone(),
        host_config: Some(host_config),
        admitted_catalog: Some(admitted_catalog),
        probe_owner: None,
    })
    .await
    .unwrap();
    host
}

/// Contract §2 `attach_host`: the authority retains the EXACT supplied
/// already-started manager and port (no second Host start), and the standalone
/// and attaching constructors share one established-owner slot, so both
/// reject a second owner identically — while a rejected attempt never
/// disturbs the incumbent.
#[tokio::test]
async fn attached_host_adopts_the_supplied_manager_and_shares_owner_admission() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let manager = start_host_manager(&env).await;
    let port = CountingPort::new();

    let attached = core
        .attach_host(manager.clone(), port.clone())
        .expect("an already-started manager attaches");
    // The exact supplied instance is retained: same allocation, and the
    // distinct configuration it was started with (not the on-disk default a
    // fresh `HostManager::start` would have loaded).
    assert!(
        Arc::ptr_eq(&attached.manager(), &manager),
        "the authority retains the supplied manager"
    );
    assert_eq!(
        attached.manager().agent_config().await.max_sessions,
        ATTACHED_MANAGER_MAX_SESSIONS,
        "the authority acts on the supplied, distinctly configured manager"
    );
    assert!(
        attached.manager().health().await.unwrap().running,
        "the supplied manager was already started; attach starts no second Host"
    );

    // One authority per open: a second attach and a standalone open are the
    // identical typed busy rejection.
    let err = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect_err("a second attach must be rejected");
    assert!(
        matches!(err, CoreError::OwnerBusy),
        "second attach: {err:?}"
    );
    let err = core
        .open_host(CountingPort::new())
        .await
        .expect_err("a standalone open is rejected identically");
    assert!(matches!(err, CoreError::OwnerBusy), "second open: {err:?}");

    // Neither rejected attempt freed the incumbent's claim: the slot is still
    // held and the attached authority still answers over the supplied manager.
    let health = attached
        .query(
            &principal,
            serde_json::from_value(serde_json::json!({ "query": "health" })).unwrap(),
        )
        .await
        .expect("the attached authority stays usable");
    assert!(health.health.expect("health payload").running);
    let err = core
        .open_host(CountingPort::new())
        .await
        .expect_err("the failed attempts never released the slot");
    assert!(
        matches!(err, CoreError::OwnerBusy),
        "slot retained: {err:?}"
    );
}

/// A construction that fails after claiming the shared admission releases it:
/// the standalone path's failed `open_host` leaves the slot free, so the
/// attaching path still admits, and only a confirmed close frees it again.
#[tokio::test]
async fn attached_host_failed_open_releases_the_shared_admission() {
    let env = seed_env().await;
    let (core, _principal) = open_core(&env).await;
    let manager = start_host_manager(&env).await;

    // A malformed agent-host config fails `open_host_inner` AFTER the slot was
    // claimed.
    let config_path = host_config_path(&env);
    std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
    std::fs::write(&config_path, "max_sessions = \"not-a-number\"\n").unwrap();
    let err = core
        .open_host(CountingPort::new())
        .await
        .expect_err("a malformed host config fails the open");
    assert!(
        matches!(err, CoreError::Internal { .. }),
        "the failed start is an internal refusal, got {err:?}"
    );

    // The claim did not survive the failure: attach still admits.
    let attached = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect("a failed open releases the shared admission");
    assert!(Arc::ptr_eq(&attached.manager(), &manager));
    let err = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect_err("the successful attach now owns the slot");
    assert!(matches!(err, CoreError::OwnerBusy), "single owner: {err:?}");
}

/// The standalone `open_host` reads the ONE canonical agent-host config
/// (`<nexus root>/agent-host/config.toml`) once, at open: the Host's own
/// manager carries that file's value, a conflicting config at the
/// `.nexus42`-nested path no writer uses cannot win, a later edit never
/// re-loads into the running authority, and a malformed canonical file fails
/// the open without retaining the shared admission.
#[tokio::test]
async fn standalone_host_reads_canonical_config_once() {
    let env = seed_env().await;
    let (core, _principal) = open_core(&env).await;

    let canonical = host_config_path(&env);
    // The path the standalone lookup must NOT resolve: applying the layout
    // helper to the already-canonical Nexus root nests `.nexus42` twice, and
    // nothing ever writes a config there.
    let nested = nexus_agent_host::config::agent_host_config_path(
        &nexus_home_layout::nexus_root_from_home(&env.user_home),
    );
    assert_ne!(
        canonical, nested,
        "the canonical and the nested lookup are different files"
    );
    for (path, max_sessions) in [(&canonical, 7), (&nested, 999)] {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, format!("max_sessions = {max_sessions}\n")).unwrap();
    }

    let handle = core
        .open_host(CountingPort::new())
        .await
        .expect("the standalone authority opens over the canonical config");
    // Observed through the manager the Host itself uses — not by comparing
    // path strings.
    assert_eq!(
        handle.manager().agent_config().await.max_sessions,
        7,
        "the canonical file is the one read; the nested decoy cannot win"
    );

    // One read, at open: a later edit of the canonical file is not picked up
    // by the running authority.
    std::fs::write(&canonical, "max_sessions = 11\n").unwrap();
    assert_eq!(
        handle.manager().agent_config().await.max_sessions,
        7,
        "the config was read once, when the authority opened"
    );

    // A confirmed close frees the slot; a malformed canonical file then fails
    // the next open AFTER the claim, and a corrected file admits the retry —
    // so the failed open released the shared admission.
    handle.close().await.expect("close returns a report");
    std::fs::write(&canonical, "max_sessions = \"not-a-number\"\n").unwrap();
    let err = core
        .open_host(CountingPort::new())
        .await
        .expect_err("a malformed canonical config fails the open");
    assert!(
        matches!(err, CoreError::Internal { .. }),
        "the failed start is an internal refusal, got {err:?}"
    );
    std::fs::write(&canonical, "max_sessions = 7\n").unwrap();
    let retried = core
        .open_host(CountingPort::new())
        .await
        .expect("the failed open released the shared admission");
    assert_eq!(
        retried.manager().agent_config().await.max_sessions,
        7,
        "the corrected canonical file is the one read on retry"
    );
}

/// An attaching open on a service that is already closing is refused BEFORE
/// any claim: attach has no `ensure_open` bypass, so it can never take the
/// established-owner slot of a closing service.
#[tokio::test]
async fn attached_host_closing_service_refuses_before_claiming_the_slot() {
    let env = seed_env().await;
    let manager = start_host_manager(&env).await;
    let (core, _principal) = open_core(&env).await;
    core.close().await.expect("the service closes");

    let err = core
        .attach_host(manager, CountingPort::new())
        .expect_err("a closing service never attaches");
    assert!(
        matches!(err, CoreError::Closing),
        "attach on a closing service is typed: {err:?}"
    );
}

/// Contract §3 close identity: a confirmed close frees the slot for a later
/// authority, and the closed handle is refused on every method afterwards —
/// including after that later authority opens over the same service.
#[tokio::test]
async fn attached_host_confirmed_close_releases_slot_and_refuses_the_old_handle() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let manager = start_host_manager(&env).await;
    let port = CountingPort::new();
    let attached = core
        .attach_host(manager.clone(), port.clone())
        .expect("an already-started manager attaches");

    let report = attached.close().await.expect("close returns a report");
    assert!(
        report.cleanup_confirmed,
        "a settled close confirms cleanup: {report:?}"
    );
    assert_eq!(report.state, CoreCloseReportState::Closed);
    // Idempotent: a repeated close reports the retained confirmed cleanup
    // rather than shutting the manager down a second time.
    let repeated = attached.close().await.expect("repeated close returns");
    assert!(repeated.cleanup_confirmed);
    assert_eq!(repeated.pending_operations, report.pending_operations);

    // Refused on every authority method, with zero provider effects.
    let create = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::CreateSessionRequest,
    >(serde_json::json!({ "provider_id": "mock-acp" }))
    .unwrap();
    let err = attached
        .create_session(&principal, create)
        .await
        .expect_err("a closed handle never creates a session");
    assert!(matches!(err, CoreError::Closing), "closed create: {err:?}");
    let prompt = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
    >(serde_json::json!({ "kind": "prompt", "content": "hello" }))
    .unwrap();
    let err = attached
        .execute(&principal, Uuid::new_v4().to_string(), prompt)
        .await
        .expect_err("a closed handle never executes");
    assert!(matches!(err, CoreError::Closing), "closed execute: {err:?}");
    let err = attached
        .query(
            &principal,
            serde_json::from_value(serde_json::json!({ "query": "health" })).unwrap(),
        )
        .await
        .expect_err("a closed handle never observes");
    assert!(matches!(err, CoreError::Closing), "closed query: {err:?}");
    assert_eq!(port.call_count(), 0, "a closed handle has zero effects");

    // A later authority over the same service is a DIFFERENT identity: it
    // admits, while the old handle stays refused.
    let reopened = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect("a confirmed close frees the slot");
    assert!(
        reopened
            .query(
                &principal,
                serde_json::from_value(serde_json::json!({ "query": "health" })).unwrap(),
            )
            .await
            .is_ok(),
        "the new authority is usable"
    );
    let err = attached
        .query(
            &principal,
            serde_json::from_value(serde_json::json!({ "query": "health" })).unwrap(),
        )
        .await
        .expect_err("a closed handle cannot act after another authority opens");
    assert!(matches!(err, CoreError::Closing), "old handle: {err:?}");
}

/// Contract §3 drain ownership: an authority close that cannot settle a live
/// Actor drain reports it and RETAINS the authority (slot + drain handle)
/// instead of detaching live work and claiming a cleanup it cannot confirm.
#[tokio::test]
async fn attached_host_unconfirmed_close_retains_slot_and_owned_drains() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let manager = start_host_manager(&env).await;
    let attached = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect("an already-started manager attaches");

    // A live drain minted through the same authority-owned spawn path
    // `execute` uses (it holds the admitted knowledge leases for its whole
    // run).
    attached
        .actor_sessions()
        .spawn_actor_drain(HostSessionId::new(), std::future::pending::<()>());

    let report = attached.close().await.expect("close returns a report");
    assert!(
        !report.cleanup_confirmed,
        "an unsettled drain is never a confirmed cleanup: {report:?}"
    );
    assert_eq!(report.state, CoreCloseReportState::Interrupted);
    assert!(
        report
            .pending_operations
            .iter()
            .any(|pending| pending.starts_with("actor-drains:")),
        "the close reports the drain it still owns: {report:?}"
    );
    // Retained, not forgotten: the authority still owns the live drain.
    assert_eq!(
        attached.actor_sessions().prune_settled_drains(),
        1,
        "the unsettled drain stays owned by the authority"
    );
    // Ownership retained ⇒ the slot is never freed for another authority.
    let err = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect_err("an unconfirmed close retains the authority slot");
    assert!(
        matches!(err, CoreError::OwnerBusy),
        "slot retained: {err:?}"
    );
    // And the handle that could not settle is itself closing.
    let err = attached
        .query(
            &principal,
            serde_json::from_value(serde_json::json!({ "query": "health" })).unwrap(),
        )
        .await
        .expect_err("the closing handle is refused");
    assert!(matches!(err, CoreError::Closing), "closing handle: {err:?}");
}

/// Contract §3 close/admission barrier: a `close` that races an ADMITTED
/// `execute` may not confirm a cleanup nor release the established-owner slot
/// while that operation has not registered its drain yet. The registered
/// drain handles are not that proof — `execute` crosses asynchronous
/// admission and Host execution before it registers one — so the authority
/// counts the operation itself and reports it instead of claiming a cleanup
/// it cannot confirm. Once the operation has settled (here: it fails before
/// creating a drain) and a drain has transferred to the registry, the
/// repeated close owns that drain and still refuses.
#[allow(clippy::too_many_lines)] // one admission/transfer/close lifecycle
#[tokio::test]
async fn attached_host_close_accounts_for_an_admitted_execute() {
    use std::future::Future;
    use std::task::{Context, Poll, Waker};

    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let manager = start_host_manager(&env).await;
    let port = CountingPort::new();
    let attached = core
        .attach_host(manager.clone(), port.clone())
        .expect("an already-started manager attaches");

    // An indexed Character session, so `execute` takes the admitted Actor path
    // (knowledge re-admission before any Host effect) whose drain registration
    // is the transfer under test.
    let ctx = admit_character(&core, &principal, &env).await;
    let knowledge = admitted_knowledge(&core, &principal, &env).await.identity();
    let key = registry_key(attached.actor_sessions(), &ctx, knowledge, &env.user_home);
    let session_id = Uuid::new_v4();
    attached.actor_sessions().insert_indexed_entry(
        key,
        ctx,
        nexus_agent_host::HostSessionId(session_id),
    );
    let prompt =
        serde_json::from_value::<
            nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
        >(serde_json::json!({ "kind": "prompt", "content": "close races the admission" }))
        .unwrap();

    // Poll the operation exactly once: it is admitted (the barrier is taken
    // before the first await, so no close can slip in ahead of it) and
    // suspended at its first await, i.e. the registered-drain view is still
    // empty — the exact window the finding describes.
    let mut execute = std::pin::pin!(attached.execute(&principal, session_id.to_string(), prompt));
    let first = {
        let mut cx = Context::from_waker(Waker::noop());
        execute.as_mut().poll(&mut cx)
    };
    assert!(
        matches!(first, Poll::Pending),
        "the admitted operation must be suspended before it registers its drain, got {first:?}"
    );
    assert_eq!(
        attached.actor_sessions().prune_settled_drains(),
        0,
        "the admitted operation has registered no drain handle yet"
    );

    // The concurrent close. An unaccounted admitted operation keeps BOTH the
    // cleanup ownership and the slot.
    let report = attached.close().await.expect("close returns a report");
    assert!(
        !report.cleanup_confirmed,
        "a close that raced an admitted operation never confirms cleanup: {report:?}"
    );
    assert_eq!(report.state, CoreCloseReportState::Interrupted);
    assert!(
        report
            .pending_operations
            .iter()
            .any(|pending| pending.starts_with("actor-admissions:")),
        "the close reports the admitted operation it still owns: {report:?}"
    );
    let err = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect_err("an unaccounted admission retains the authority slot");
    assert!(
        matches!(err, CoreError::OwnerBusy),
        "slot retained: {err:?}"
    );
    assert_eq!(port.call_count(), 0, "the race produced no provider effect");

    // The admitted operation settles. Here it fails BEFORE creating a drain:
    // this fixture indexes the session without a Host create, so the Host
    // plane refuses it — the accounted-for outcome the barrier must accept
    // instead of a fabricated success.
    let settled = execute.await;
    assert!(
        settled.is_err(),
        "the admitted operation settles against the closed authority: {settled:?}"
    );

    // The transfer half: a drain registered through the same authority-owned
    // spawn path `execute` uses, landing AFTER the closing latch. It is
    // retained, so it still owns the cleanup and still blocks the slot.
    attached
        .actor_sessions()
        .spawn_actor_drain(HostSessionId::new(), std::future::pending::<()>());

    let retried = attached.close().await.expect("the repeated close returns");
    assert!(
        !retried.cleanup_confirmed,
        "a late drain transfer keeps the cleanup unconfirmed: {retried:?}"
    );
    assert!(
        !retried
            .pending_operations
            .iter()
            .any(|pending| pending.starts_with("actor-admissions:")),
        "the settled admission is no longer pending: {retried:?}"
    );
    assert!(
        retried
            .pending_operations
            .iter()
            .any(|pending| pending.starts_with("actor-drains:")),
        "the drain transferred to the registry is reported: {retried:?}"
    );
    assert_eq!(
        attached.actor_sessions().prune_settled_drains(),
        1,
        "the authority still owns the drain it retained"
    );
    let err = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect_err("retained cleanup keeps the authority slot");
    assert!(
        matches!(err, CoreError::OwnerBusy),
        "slot retained: {err:?}"
    );
}

// ── v1.196 P0-T3 — close observation, cancel and shutdown ─────────────────
//
// Durable contract: `.mstar/iterations/v1.196/specs/current-host-actor-contract.md`
// §2 (control methods), §3 (one lifecycle owner, including the close order) and
// §5 (delivery versus authority). The authority observes the SAME Host it
// executes on (its own drain keeps the original exec stream), cancels through
// that same manager only after the stored owner and the session/operation
// association are verified and the provider's negotiated capability allows it,
// and settles the manager/LocalSet exactly once in the ordered close — which
// stays legal after the core service closed.

const CONTROL_PROVIDER: &str = "control-fixture";

/// One in-process provider adapter for the control cases: a real manager
/// session plane whose events the test drives, so the cancel/terminal races are
/// OBSERVED instead of timed.
struct ControlProvider {
    /// Negotiated cancellation capability (`false` is the DSH shape).
    cancellation: bool,
    /// Advertises cancellation but fails the `cancel` call.
    cancel_fails: bool,
    /// Events the provider publishes by itself at exec time (the observation-lag
    /// case); always the same session-scoped status event.
    burst: usize,
    /// Executions that reached the provider, i.e. that passed the manager's own
    /// session lookup. The session-shutdown race parks one here: the operation
    /// is admitted and the manager resolved its session, while the authority
    /// has not registered a drain yet.
    executions: Arc<AtomicUsize>,
    /// Park executing provider calls until [`Self::release_parked_executions`].
    park: Arc<AtomicBool>,
    /// Fail executing provider calls instead of producing a stream: the shape of
    /// an execution that reaches the provider and registers no drain at all.
    exec_fails: Arc<AtomicBool>,
    /// Publish the operation's own clean terminal while a `cancel` call is in
    /// flight, then refuse the cancel: the drain-settles-midflight race a
    /// refused cancel must survive (PR #335 Greptile P1).
    cancel_publishes_terminal: Arc<AtomicBool>,
    /// The release signal for a parked execution.
    exec_gate: Arc<tokio::sync::Notify>,
    /// Every `LaunchSpec.cwd` a launch actually received, in call order: the
    /// observable proof of WHICH root a session was launched at (P0-T2).
    launched_cwds: parking_lot::Mutex<Vec<PathBuf>>,
    cancels: Arc<AtomicUsize>,
    shutdowns: Arc<AtomicUsize>,
    polled: Arc<AtomicUsize>,
    outbox: Mutex<Option<tokio::sync::mpsc::UnboundedSender<HostItem>>>,
}

fn control_provider(cancellation: bool, burst: usize, cancel_fails: bool) -> Arc<ControlProvider> {
    Arc::new(ControlProvider {
        cancellation,
        cancel_fails,
        burst,
        executions: Arc::new(AtomicUsize::new(0)),
        park: Arc::new(AtomicBool::new(false)),
        exec_fails: Arc::new(AtomicBool::new(false)),
        cancel_publishes_terminal: Arc::new(AtomicBool::new(false)),
        exec_gate: Arc::new(tokio::sync::Notify::new()),
        launched_cwds: parking_lot::Mutex::new(Vec::new()),
        cancels: Arc::new(AtomicUsize::new(0)),
        shutdowns: Arc::new(AtomicUsize::new(0)),
        polled: Arc::new(AtomicUsize::new(0)),
        outbox: Mutex::new(None),
    })
}

impl ControlProvider {
    const fn negotiated_capabilities(&self) -> CapabilityDescriptor {
        CapabilityDescriptor {
            cancellation: self.cancellation,
            ..CapabilityDescriptor::acp_full()
        }
    }

    fn provider_id() -> ProviderId {
        ProviderId::new(CONTROL_PROVIDER)
    }

    /// Publish one event on the executing operation's stream.
    fn push(&self, event: HostEvent) {
        self.outbox
            .lock()
            .unwrap()
            .as_ref()
            .expect("the provider is executing")
            .send(Ok(event))
            .expect("the drain is alive");
    }

    /// End the executing stream without a terminal (stream loss).
    fn close_stream(&self) {
        drop(self.outbox.lock().unwrap().take());
    }

    fn cancels(&self) -> usize {
        self.cancels.load(Ordering::SeqCst)
    }

    /// Park executing provider calls until [`Self::release_parked_executions`].
    fn park_executions(&self) {
        self.park.store(true, Ordering::SeqCst);
    }

    /// Release the parked execution (later calls run unparked).
    fn release_parked_executions(&self) {
        self.park.store(false, Ordering::SeqCst);
        self.exec_gate.notify_one();
    }

    /// Make executing provider calls fail instead of producing a stream, so an
    /// admitted execution retires its admission while registering no drain.
    fn fail_executions(&self) {
        self.exec_fails.store(true, Ordering::SeqCst);
    }

    /// Publish the operation's own clean terminal while the `cancel` call is
    /// in flight, then refuse the cancel: the drain-settles-midflight race.
    fn publish_terminal_before_refusing_cancel(&self) {
        self.cancel_publishes_terminal.store(true, Ordering::SeqCst);
    }

    /// Executions that reached the provider.
    fn executions(&self) -> usize {
        self.executions.load(Ordering::SeqCst)
    }

    fn shutdowns(&self) -> usize {
        self.shutdowns.load(Ordering::SeqCst)
    }

    /// Host events the provider actually produced (polled off its stream).
    fn polled(&self) -> usize {
        self.polled.load(Ordering::SeqCst)
    }

    /// The `cwd` every launch received, in call order.
    fn launched_cwds(&self) -> Vec<PathBuf> {
        self.launched_cwds.lock().clone()
    }
}

#[async_trait]
impl ProviderAdapter for ControlProvider {
    fn descriptor(&self) -> ProviderDescriptor {
        ProviderDescriptor {
            provider_id: Self::provider_id(),
            display_name: "Control fixture".to_string(),
            protocol_kind: ProtocolKind::NativeCli,
            capabilities: self.negotiated_capabilities(),
        }
    }

    async fn probe(&self, _request: ProbeRequest) -> nexus_agent_host::HostResult<ProviderHealth> {
        Ok(ProviderHealth {
            provider_id: Self::provider_id(),
            available: true,
            latency_ms: None,
            message: Some("control fixture".to_string()),
        })
    }

    async fn launch(&self, spec: LaunchSpec) -> nexus_agent_host::HostResult<ManagedSessionHandle> {
        self.launched_cwds.lock().push(spec.cwd.clone());
        Ok(ManagedSessionHandle {
            provider_id: Self::provider_id(),
            session_id: HostSessionId::new(),
            capabilities: self.negotiated_capabilities(),
            process_identity: None,
        })
    }

    async fn execute(
        &self,
        session: &ManagedSessionHandle,
        _op: HostOperation,
    ) -> nexus_agent_host::HostResult<HostEventStream> {
        self.executions.fetch_add(1, Ordering::SeqCst);
        // The session-shutdown race parks here: the manager has resolved this
        // session for the operation, and the authority has not returned from
        // `exec` yet, so no drain is registered.
        if self.park.load(Ordering::SeqCst) {
            self.exec_gate.notified().await;
        }
        if self.exec_fails.load(Ordering::SeqCst) {
            return Err(nexus_agent_host::HostError::internal(
                "the control fixture refuses every execution",
            ));
        }
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<HostItem>();
        *self.outbox.lock().unwrap() = Some(tx);
        let sid = session.session_id.clone();
        let burst: Vec<HostItem> = (0..self.burst)
            .map(|index| {
                Ok(HostEvent::Status(StatusEvent {
                    session_id: Some(sid.clone()),
                    level: StatusLevel::Info,
                    message: format!("burst {index}"),
                }))
            })
            .collect();
        let polled = Arc::clone(&self.polled);
        let stream = futures_util::stream::iter(burst)
            .chain(futures_util::stream::unfold(rx, |mut rx| async move {
                rx.recv().await.map(|item| (item, rx))
            }))
            .inspect(move |_| {
                polled.fetch_add(1, Ordering::SeqCst);
            });
        Ok(Box::pin(stream))
    }

    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
    ) -> nexus_agent_host::HostResult<()> {
        self.cancels.fetch_add(1, Ordering::SeqCst);
        if self.cancel_publishes_terminal.load(Ordering::SeqCst) {
            // The drain-settles-midflight race (PR #335): the provider settles
            // the operation with its own clean end of turn WHILE the cancel
            // call is in flight, then refuses the cancel. The drain settles in
            // the same poll that pulls the terminal off the stream (no await
            // between the pull and the settlement), so one polled item proves
            // the settlement has run before the refusal returns.
            self.push(finished(&session.session_id, &op_id, FinishReason::EndTurn));
            wait_until(|| self.polled() >= 1, "the drain to settle the terminal").await;
        }
        if self.cancel_fails {
            return Err(nexus_agent_host::HostError::internal(
                "cancel refused by the control fixture",
            ));
        }
        Ok(())
    }

    async fn shutdown(&self, _session: ManagedSessionHandle) -> nexus_agent_host::HostResult<()> {
        self.shutdowns.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn capabilities(&self) -> CapabilityDescriptor {
        self.negotiated_capabilities()
    }
}

/// Poll a condition until it holds, bounded by a generous deadline: the fixture
/// waits for the observable effect the next assertion depends on instead of
/// guessing with a sleep.
async fn wait_until(mut condition: impl FnMut() -> bool, what: &str) {
    tokio::time::timeout(std::time::Duration::from_secs(20), async {
        while !condition() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {what}"));
}

/// Start a real manager whose single provider is the test-driven control
/// adapter, the way the native open composes one (registered before start, so
/// the registered candidate is admitted and probed available).
async fn control_manager(env: &Env, provider: Arc<ControlProvider>) -> Arc<HostManager> {
    use nexus_agent_host::config::{load_config_from_path, validate_workspace_path};

    let workspace_root = nexus_home_layout::nexus_root_from_home(&env.user_home);
    validate_workspace_path(&workspace_root).unwrap();
    let config_path = host_config_path(env);
    let mut host_config = load_config_from_path(&config_path).unwrap();
    host_config.max_sessions = ATTACHED_MANAGER_MAX_SESSIONS;
    let host = Arc::new(HostManager::new());
    host.register_provider(
        provider,
        LaunchStrategy::NativeCli {
            command: "control-fixture".to_string(),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
        },
    )
    .await;
    host.start(HostStartConfig {
        config_path,
        workspace_root: workspace_root.clone(),
        max_sessions: host_config.max_sessions,
        max_ops_per_session: host_config.max_ops_per_session,
        timeouts: host_config.timeouts.clone(),
        host_config: Some(host_config),
        admitted_catalog: None,
        probe_owner: Some(SessionOwner {
            creator_id: CREATOR.to_string(),
            workspace_root,
            orchestration_run_id: None,
        }),
    })
    .await
    .unwrap();
    host
}

/// Create the one control session through the manager's own create path.
async fn control_session(manager: &Arc<HostManager>, env: &Env) -> HostSessionId {
    let workspace_root = nexus_home_layout::nexus_root_from_home(&env.user_home);
    manager
        .create_session(nexus_agent_host::capability::model::CreateSessionRequest {
            provider_id: ControlProvider::provider_id(),
            cwd: workspace_root.clone(),
            model: None,
            mode: None,
            mcp_servers: vec![],
            metadata: serde_json::Value::Null,
            owner: SessionOwner {
                creator_id: CREATOR.to_string(),
                workspace_root,
                orchestration_run_id: None,
            },
        })
        .await
        .expect("the control session is created")
        .id
}

/// The full control fixture: one core authority attached to a real started
/// manager with one control provider, one created session on that same manager
/// and that session indexed as an Actor Character session.
async fn control_fixture(
    env: &Env,
    provider: Arc<ControlProvider>,
) -> (
    CoreService,
    nexus_core::Principal,
    Arc<HostManager>,
    HostHandle,
    HostSessionId,
) {
    let (core, principal) = open_core(env).await;
    let manager = control_manager(env, Arc::clone(&provider)).await;
    let handle = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect("the supplied manager attaches");
    let session_id = control_session(&manager, env).await;
    let ctx = admit_character(&core, &principal, env).await;
    let knowledge = admitted_knowledge(&core, &principal, env).await.identity();
    let key = ActorSessionRegistry::key_for(
        CONTROL_PROVIDER,
        &env.user_home,
        None,
        None,
        &ctx,
        knowledge,
    )
    .unwrap();
    handle
        .actor_sessions()
        .insert_indexed_entry(key, ctx, session_id.clone());
    (core, principal, manager, handle, session_id)
}

/// Prompt the control session and return the operation the authority started.
async fn control_prompt(
    handle: &HostHandle,
    principal: &nexus_core::Principal,
    session_id: &HostSessionId,
) -> HostOperationId {
    let response = handle
        .execute(
            principal,
            session_id.to_string(),
            execute_request(serde_json::json!({ "kind": "prompt", "content": "control prompt" })),
        )
        .await
        .expect("the control prompt is dispatched to the Host");
    HostOperationId(Uuid::parse_str(&response.operation_id).expect("the operation id is a UUID"))
}

async fn control_status(
    handle: &HostHandle,
    principal: &nexus_core::Principal,
    operation_id: &HostOperationId,
) -> CharacterOperationResult {
    handle
        .character_operation(principal, operation_id.to_string())
        .await
        .expect("the recorded outcome is owner-readable")
}

/// The accepted cancel is the operation's truth (§5) and it is recorded ONCE:
/// the provider's later clean terminal cannot rewrite it, and a later cancel is
/// the typed 409 conflict.
#[tokio::test]
async fn actor_control_cancel_wins_the_phase_race_and_settles_once() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;

    // The cancel is accepted through the SAME manager the prompt ran on.
    let cancel = handle
        .cancel_operation(&principal, operation_id.to_string())
        .await
        .expect("an owner-authorized cancel is accepted");
    assert!(
        cancel.operation_id == operation_id.to_string(),
        "the cancel names the operation it cancelled"
    );
    assert_eq!(cancel.status, "cancelled");
    assert_eq!(
        provider.cancels(),
        1,
        "the manager's own cancel was invoked"
    );
    let recorded = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        recorded.run_status,
        CharacterOperationResultRunStatus::Cancelled
    );
    assert_eq!(
        recorded.finish_reason,
        Some(CharacterOperationResultFinishReason::Cancelled)
    );

    // The provider then reports its own clean end of turn — and then ends the
    // stream. The accepted cancel already won the phase race, so the drain
    // settlement is a no-op rather than a second terminal.
    provider.push(finished(&session_id, &operation_id, FinishReason::EndTurn));
    provider.close_stream();
    wait_until(
        || handle.actor_sessions().unsettled_drain_count() == 0,
        "the operation drain to settle",
    )
    .await;
    let after_drain = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        after_drain.run_status,
        CharacterOperationResultRunStatus::Cancelled,
        "a provider success after an accepted cancel is never the truth"
    );
    assert_eq!(
        handle.actor_sessions().character_operation_count(),
        1,
        "the cancel and the drain settled the operation exactly once"
    );

    // A later cancel of a finished operation is the typed 409 conflict.
    let err = handle
        .cancel_operation(&principal, operation_id.to_string())
        .await
        .expect_err("a finished operation refuses cancel");
    match &err {
        CoreError::ActorConflict { code, .. } => {
            assert_eq!(code, "actor_operation_finished", "finished cancel code");
        }
        other => panic!("expected actor_operation_finished, got {other:?}"),
    }
}

/// DSH: a provider whose session does not negotiate cancellation is refused
/// `not_supported` BEFORE any intent — the provider is never asked, no cancel
/// intent is latched, and the run's own terminal decides the outcome.
#[tokio::test]
async fn actor_control_unsupported_cancel_never_latches_intent() {
    let env = seed_env().await;
    let provider = control_provider(false, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;

    let err = handle
        .cancel_operation(&principal, operation_id.to_string())
        .await
        .expect_err("an unsupported provider is refused");
    match &err {
        CoreError::Coded { code, .. } => assert_eq!(code, "not_supported", "DSH refusal code"),
        other => panic!("expected not_supported, got {other:?}"),
    }
    assert_eq!(
        provider.cancels(),
        0,
        "an unsupported request never reaches the provider"
    );

    // No intent was latched: the provider's own terminal is the truth, not a
    // fabricated cancellation.
    provider.push(finished(&session_id, &operation_id, FinishReason::EndTurn));
    provider.close_stream();
    wait_until(
        || handle.actor_sessions().unsettled_drain_count() == 0,
        "the operation drain to settle",
    )
    .await;
    let outcome = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Succeeded,
        "an unsupported cancel never latches cancellation intent"
    );
    assert_eq!(
        outcome.finish_reason,
        Some(CharacterOperationResultFinishReason::EndTurn)
    );
}

/// A provider that advertises cancellation but REFUSES the call: the refusal is
/// reported honestly, the latched intent is rolled back, and the operation's
/// own terminal still decides.
#[tokio::test]
async fn actor_control_provider_refusal_is_never_recorded_as_cancelled() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, true);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;

    let err = handle
        .cancel_operation(&principal, operation_id.to_string())
        .await
        .expect_err("a provider refusal is reported, never fabricated as success");
    assert!(
        matches!(err, CoreError::Internal { .. }),
        "the refusal surfaces as an error, got {err:?}"
    );
    assert_eq!(provider.cancels(), 1, "the provider was asked once");
    let rolled_back = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        rolled_back.run_status,
        CharacterOperationResultRunStatus::Running,
        "a refused cancel leaves no latched intent behind"
    );

    provider.push(finished(&session_id, &operation_id, FinishReason::EndTurn));
    provider.close_stream();
    wait_until(
        || handle.actor_sessions().unsettled_drain_count() == 0,
        "the operation drain to settle",
    )
    .await;
    let outcome = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Succeeded,
        "the provider's own terminal decides after a refused cancel"
    );
}

/// PR #335 (Greptile P1): the drain settles the operation WHILE the provider's
/// cancel call is still in flight, and the provider then refuses the cancel.
/// The refusal is reported, and the refusal must NOT be recorded as an
/// accepted cancellation: the drain's settlement on the provider's own clean
/// end of turn stays the recorded truth.
#[tokio::test]
async fn actor_control_refused_cancel_survives_the_drain_settling_midflight() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, true);
    provider.publish_terminal_before_refusing_cancel();
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;

    let err = handle
        .cancel_operation(&principal, operation_id.to_string())
        .await
        .expect_err("a refused cancel is reported even when the drain settled mid-flight");
    assert!(
        matches!(err, CoreError::Internal { .. }),
        "the refusal surfaces as an error, got {err:?}"
    );
    assert_eq!(provider.cancels(), 1, "the provider was asked exactly once");

    // The drain settled on the provider's own clean end of turn BEFORE the
    // refusal arrived, and the rollback did not erase it into a cancellation.
    let outcome = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Succeeded,
        "a refused cancel is never recorded as an accepted cancellation"
    );
    assert_eq!(
        outcome.finish_reason,
        Some(CharacterOperationResultFinishReason::EndTurn)
    );
}

/// Owner scoping: a foreign principal is `auth_required` on every control
/// method, and a cross-session or unknown operation is `not_found` — never a
/// leak of another session's work.
#[tokio::test]
async fn actor_control_foreign_and_cross_session_requests_are_denied() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;
    let other_session = control_session(&manager, &env).await;

    let foreign = seed_env_as("ctr_bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb").await;
    let (_, foreign_principal) = open_core(&foreign).await;
    for err in [
        handle
            .cancel_operation(&foreign_principal, operation_id.to_string())
            .await
            .unwrap_err(),
        handle
            .shutdown_session(&foreign_principal, session_id.to_string())
            .await
            .unwrap_err(),
        handle
            .next_events(
                &foreign_principal,
                session_id.to_string(),
                operation_id.to_string(),
                16,
                4096,
            )
            .await
            .unwrap_err(),
    ] {
        assert!(
            matches!(err, CoreError::AuthRequired),
            "a foreign principal is auth_required, got {err:?}"
        );
    }

    // Cross-session: the operation exists, but not for the session the caller
    // claims, so observation is refused indistinguishably from unknown.
    let err = handle
        .next_events(
            &principal,
            other_session.to_string(),
            operation_id.to_string(),
            16,
            4096,
        )
        .await
        .expect_err("a cross-session observation is refused");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");

    // Unknown operation, in both directions.
    let unknown = HostOperationId(Uuid::new_v4());
    let err = handle
        .next_events(
            &principal,
            session_id.to_string(),
            unknown.to_string(),
            16,
            4096,
        )
        .await
        .expect_err("an unknown operation is refused");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");
    let err = handle
        .cancel_operation(&principal, unknown.to_string())
        .await
        .expect_err("an unknown operation refuses cancel");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");
    let err = handle
        .shutdown_session(&principal, Uuid::new_v4().to_string())
        .await
        .expect_err("a session unknown to the index and the manager is not_found");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");
    assert_eq!(
        provider.cancels(),
        0,
        "no denial reached the provider's cancel"
    );
}

/// Contract §5 "Delivery versus authority": the server-owned drain settles the
/// operation with ZERO subscribers, and the retained observation reader replays
/// from its own bounded buffer without ever touching the original exec stream.
#[tokio::test]
async fn actor_control_observation_settles_without_a_subscriber() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;
    assert_eq!(
        handle.actor_sessions().observation_count(),
        1,
        "one bounded observation reader per reserved Character operation"
    );

    // No `next_events` call at all: the drain still observes and settles.
    provider.push(finished(&session_id, &operation_id, FinishReason::EndTurn));
    provider.close_stream();
    wait_until(
        || handle.actor_sessions().unsettled_drain_count() == 0,
        "the operation drain to settle",
    )
    .await;
    let outcome = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Succeeded,
        "settlement never waits for an observation subscriber"
    );
    assert_eq!(
        provider.polled(),
        1,
        "the authority's own exec stream was consumed exactly once"
    );

    // The observation is a SEPARATE source: pulling it now still delivers this
    // operation's own terminal without re-reading the exec stream.
    let batch = handle
        .next_events(
            &principal,
            session_id.to_string(),
            operation_id.to_string(),
            16,
            64 * 1024,
        )
        .await
        .expect("the retained observation is readable");
    assert!(
        batch.operation_id == operation_id.to_string(),
        "the observation batch names the requested operation"
    );
    assert!(batch.gap.is_none(), "an intact observation is not a gap");
    assert!(!batch.has_more);
    assert!(
        batch.events.iter().any(|event| matches!(
            event,
            nexus_contracts::generated::core::provider_event_batch::NexusProviderHostEvent::OpFinished { .. }
        )),
        "the observation carries this operation's terminal"
    );
    assert_eq!(
        provider.polled(),
        1,
        "observation never consumes the authority's own stream"
    );
}

/// A cap overflow and a lost broadcast source are TYPED resync gaps: a consumer
/// re-reads authoritative status instead of reading either as completion.
#[tokio::test]
async fn actor_control_observation_gaps_are_typed_and_never_complete() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;

    // One session-scoped event, delivered to the observation source.
    provider.push(HostEvent::Status(StatusEvent {
        session_id: Some(session_id.clone()),
        level: StatusLevel::Info,
        message: "observation payload".to_string(),
    }));
    wait_until(
        || provider.polled() == 1,
        "the fixture event to be produced",
    )
    .await;

    // A byte cap too small for a single event is the typed overflow gap.
    let overflow = handle
        .next_events(
            &principal,
            session_id.to_string(),
            operation_id.to_string(),
            16,
            64,
        )
        .await
        .expect("an over-budget observation returns a gap batch");
    assert!(
        overflow.events.is_empty(),
        "nothing over budget is delivered"
    );
    assert_eq!(
        overflow.gap.map(|gap| (gap.reason, gap.resync_required)),
        Some((ProviderEventBatchGapReason::Oversized, true))
    );

    provider.close_stream();
    wait_until(
        || handle.actor_sessions().unsettled_drain_count() == 0,
        "the operation drain to settle",
    )
    .await;
    let _ = control_status(&handle, &principal, &operation_id).await;
}

/// A broadcast lag is the typed LAGGING gap: the observation never turns lost
/// data into a clean completion, and the authoritative status stays readable.
#[tokio::test]
async fn actor_control_observation_lag_is_a_resync_gap() {
    let env = seed_env().await;
    // More events than the manager's broadcast capacity (1024): the idle
    // observation subscriber falls behind and the channel reports the lag.
    let provider = control_provider(true, 2_100, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;
    wait_until(
        || provider.polled() >= 2_100,
        "the provider burst to be produced",
    )
    .await;

    let batch = handle
        .next_events(
            &principal,
            session_id.to_string(),
            operation_id.to_string(),
            16,
            256 * 1024,
        )
        .await
        .expect("a lagged observation returns a typed gap");
    assert!(!batch.has_more, "a lost observation never claims more data");
    let gap = batch
        .gap
        .expect("a lagged observation must report a gap, never an EOF");
    assert_eq!(gap.reason, ProviderEventBatchGapReason::Lagging);
    assert!(gap.resync_required);
    let outcome = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Running,
        "the authoritative status stays readable through the observation gap"
    );

    provider.push(finished(&session_id, &operation_id, FinishReason::EndTurn));
    provider.close_stream();
    wait_until(
        || handle.actor_sessions().unsettled_drain_count() == 0,
        "the operation drain to settle",
    )
    .await;
}

/// A-1: the observation reader is retained BY its reserved Character operation
/// record, so retention cannot escape the operation registry — an id with no
/// reservation has no observation slot, the readers stay in lockstep with the
/// records through every retirement path, and the terminal retention window
/// drops each reader with the record it evicts.
#[tokio::test]
async fn actor_control_observation_retention_is_bounded_by_its_reserved_operation() {
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let handle: HostHandle = core.open_host(CountingPort::new()).await.unwrap();
    let registry = handle.actor_sessions();

    // An id with no reserved Character operation has no observation slot: an
    // arbitrary caller-supplied id can no longer mint an orphaned reader.
    let orphan = HostOperationId(Uuid::new_v4());
    let err = registry
        .retain_observation(&orphan, ProviderEventReader::observe(exec_stream(vec![])))
        .expect_err("an unreserved id cannot be observed");
    assert!(matches!(err, CoreError::NotFound { .. }), "got {err:?}");
    assert_eq!(registry.observation_count(), 0);
    assert!(registry.observation(&orphan).is_none());

    // A reserved Character operation takes one reader, one-for-one, and
    // removing the unstarted reservation drops the reader with its record.
    let operation_id = HostOperationId(Uuid::new_v4());
    handle
        .reserve_character_operation(&CharacterOperationSnapshot::new(
            principal.creator_id().to_string(),
            HostSessionId(Uuid::new_v4()),
            operation_id.clone(),
        ))
        .expect("a Character operation reserves an outcome");
    registry
        .retain_observation(
            &operation_id,
            ProviderEventReader::observe(exec_stream(vec![])),
        )
        .expect("the reserved record carries its observation");
    assert_eq!(registry.observation_count(), 1);
    assert_eq!(registry.character_operation_count(), 1);
    registry.remove_operation_reservation(&operation_id);
    assert_eq!(
        registry.observation_count(),
        0,
        "removing the reservation drops the reader with its record"
    );
    assert!(registry.observation(&operation_id).is_none());

    // The retention window is the records': past 1024 terminals the oldest
    // record is evicted with its reader, so readers never outnumber or outlive
    // the operations they belong to.
    let mut oldest = None;
    let mut newest = None;
    for _ in 0..1025 {
        let operation_id = HostOperationId(Uuid::new_v4());
        handle
            .reserve_character_operation(&CharacterOperationSnapshot::new(
                principal.creator_id().to_string(),
                HostSessionId(Uuid::new_v4()),
                operation_id.clone(),
            ))
            .expect("a Character operation reserves an outcome");
        registry
            .retain_observation(
                &operation_id,
                ProviderEventReader::observe(exec_stream(vec![])),
            )
            .expect("the reserved record carries its observation");
        handle.settle_character_terminal(
            &operation_id,
            CharacterOperationResultRunStatus::Succeeded,
            Some(CharacterOperationResultFinishReason::EndTurn),
        );
        oldest.get_or_insert_with(|| operation_id.clone());
        newest = Some(operation_id);
    }
    assert_eq!(
        registry.character_operation_count(),
        1024,
        "the terminal retention window stays bounded"
    );
    assert_eq!(
        registry.observation_count(),
        1024,
        "the readers stay in lockstep with the retained records"
    );
    let oldest = oldest.expect("the first settled operation");
    let newest = newest.expect("the last settled operation");
    assert!(
        registry.observation(&oldest).is_none(),
        "the evicted record took its observation with it"
    );
    assert!(
        registry.observation(&newest).is_some(),
        "a retained record keeps its observation"
    );
}

/// Session shutdown cancels the session's active work through the same manager,
/// releases the session, and retires the Actor reuse state — reporting success
/// only after the manager confirmed the release AND the session's own drain
/// settled.
#[tokio::test]
async fn actor_control_session_shutdown_cancels_then_releases() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;

    // The shutdown joins the session's own drain before it confirms, and this
    // fixture's provider `shutdown` deliberately returns without ending its
    // exec stream, so the run ends when the test ends the stream — after the
    // cancel, so the accepted cancel still wins the phase race.
    let shutdown = tokio::spawn({
        let handle = handle.clone();
        let principal = principal.clone();
        let session_id = session_id.to_string();
        async move { handle.shutdown_session(&principal, session_id).await }
    });
    wait_until(|| provider.cancels() == 1, "the session's cancel").await;
    provider.close_stream();
    let response = tokio::time::timeout(std::time::Duration::from_secs(20), shutdown)
        .await
        .expect("the session shutdown settles with its drain")
        .expect("the shutdown task joins")
        .expect("an owner-authorized shutdown is confirmed");
    assert!(
        response.session_id == session_id.to_string(),
        "the shutdown names the session it settled"
    );
    assert_eq!(response.status, "shutdown");
    assert_eq!(
        provider.cancels(),
        1,
        "the session's active operation was cancelled through the same manager"
    );
    assert_eq!(provider.shutdowns(), 1, "the session was released once");
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the shutdown confirmed only after its own drain settled"
    );
    let outcome = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        outcome.run_status,
        CharacterOperationResultRunStatus::Cancelled,
        "the shutdown's accepted cancel is the operation's truth"
    );
    assert!(
        handle
            .actor_sessions()
            .stored_session_owner(&session_id)
            .map(|(owner, _, retired)| (owner, retired))
            == Some((CREATOR.to_string(), true)),
        "the released session is retired, not reusable"
    );

    // The operation's session is gone from the manager: a later cancel is
    // refused instead of reaching a nonexistent session.
    let err = handle
        .cancel_operation(&principal, operation_id.to_string())
        .await
        .expect_err("a finished operation refuses cancel");
    assert!(
        matches!(
            err,
            CoreError::ActorConflict { .. } | CoreError::NotFound { .. }
        ),
        "got {err:?}"
    );
    assert_eq!(provider.cancels(), 1, "no second cancel was attempted");
}

/// B-1 regression: the manager release does NOT settle this authority's own
/// drain of the session's exec stream — this fixture's provider `shutdown`
/// returns `Ok` and leaves its stream open. A session shutdown must therefore
/// join that drain before it retires the indexed session and before it reports
/// success; confirming a teardown while the work it owns is still running is
/// the pre-fix behavior this pins.
#[tokio::test]
async fn actor_control_session_shutdown_joins_its_own_drain_before_success() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;

    let mut shutdown = tokio::spawn({
        let handle = handle.clone();
        let principal = principal.clone();
        let session_id = session_id.to_string();
        async move { handle.shutdown_session(&principal, session_id).await }
    });
    // The manager released the session and returned, while the authority's
    // drain of that same exec stream is still live (nothing ended the stream).
    wait_until(|| provider.shutdowns() == 1, "the manager session release").await;
    wait_until(
        || handle.actor_sessions().unsettled_drain_count() == 1,
        "the authority's own drain to stay live",
    )
    .await;
    assert!(
        handle
            .actor_sessions()
            .stored_session_owner(&session_id)
            .is_some_and(|(_, _, retired)| !retired),
        "the indexed session is retired before its own drain settled"
    );
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(500), &mut shutdown)
            .await
            .is_err(),
        "the shutdown confirmed a teardown while its session's drain was unsettled"
    );

    // The run ends: the drain settles, and only then is the shutdown confirmed.
    provider.close_stream();
    let response = tokio::time::timeout(std::time::Duration::from_secs(20), shutdown)
        .await
        .expect("the shutdown settles with its drain")
        .expect("the shutdown task joins")
        .expect("the confirmed shutdown");
    assert_eq!(response.status, "shutdown");
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the confirmed shutdown left no unsettled drain behind"
    );
    assert_eq!(
        handle
            .actor_sessions()
            .stored_session_owner(&session_id)
            .map(|(_, _, retired)| retired),
        Some(true),
        "the session is retired once its drain settled"
    );
    assert_eq!(
        control_status(&handle, &principal, &operation_id)
            .await
            .run_status,
        CharacterOperationResultRunStatus::Cancelled,
        "the shutdown's accepted cancel is still the operation's truth"
    );
}

/// B-1 (remaining race): the registered drains are not the whole proof a
/// session shutdown needs. `execute` crosses asynchronous admission and Host
/// execution BEFORE it registers its drain, so a shutdown that only joined the
/// drains it could already see would retire the session — and report success —
/// while an operation admitted for that session was still in flight and about
/// to register one. The session's live work must therefore be accounted for
/// from admission, not from registration: the join a shutdown runs waits on an
/// admitted execute exactly as it waits on a live drain.
#[tokio::test]
async fn actor_control_session_shutdown_waits_for_an_admitted_execute() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;

    // Park the admitted execute inside the provider's own `execute`: admitted,
    // past every registry gate and the manager's session lookup, with no drain
    // registered yet — the window the finding describes.
    provider.park_executions();
    let execute = tokio::spawn({
        let handle = handle.clone();
        let principal = principal.clone();
        let session_id = session_id.to_string();
        async move {
            handle
                .execute(
                    &principal,
                    session_id,
                    execute_request(serde_json::json!({
                        "kind": "prompt",
                        "content": "race the session shutdown",
                    })),
                )
                .await
        }
    });
    wait_until(
        || provider.executions() == 1,
        "the admitted execute to reach the provider",
    )
    .await;
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the admitted execute has registered no drain yet"
    );

    // The concurrent session shutdown releases the session at the manager while
    // that admitted execute is still in flight.
    let shutdown = tokio::spawn({
        let handle = handle.clone();
        let principal = principal.clone();
        let session_id = session_id.to_string();
        async move { handle.shutdown_session(&principal, session_id).await }
    });
    wait_until(|| provider.shutdowns() == 1, "the session release").await;
    assert!(
        !handle
            .actor_sessions()
            .stored_session_owner(&session_id)
            .is_some_and(|(_, _, retired)| retired),
        "the shutdown retired the session while an admitted execute for it was still in flight"
    );

    // The admitted execute registers its drain. The shutdown may confirm only
    // after that drain settles — it is the work this teardown owns.
    provider.release_parked_executions();
    let started = execute
        .await
        .expect("the execute task joins")
        .expect("the admitted execute is dispatched");
    assert!(
        started.session_id == session_id.to_string(),
        "the dispatch names the session it was admitted for"
    );
    wait_until(
        || handle.actor_sessions().unsettled_drain_count() == 1,
        "the admitted execute to register its drain",
    )
    .await;
    assert!(
        !shutdown.is_finished(),
        "the shutdown confirmed a teardown before the drain its session registered had settled"
    );

    // The run ends: the drain settles, and only then is the shutdown confirmed.
    provider.close_stream();
    let response = tokio::time::timeout(std::time::Duration::from_secs(20), shutdown)
        .await
        .expect("the shutdown settles with the late drain")
        .expect("the shutdown task joins")
        .expect("the confirmed shutdown");
    assert!(
        response.session_id == session_id.to_string(),
        "the confirmed shutdown names the same session"
    );
    assert_eq!(response.status, "shutdown");
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the confirmed shutdown left no unsettled drain behind"
    );
    assert_eq!(
        handle
            .actor_sessions()
            .stored_session_owner(&session_id)
            .map(|(_, _, retired)| retired),
        Some(true),
        "the session is retired only once the work it owned settled"
    );
    assert_eq!(provider.shutdowns(), 1, "the session was released once");
}

/// Contract §3: `quiesce_actor_sessions` closes the Actor side only — it
/// cancels active work, joins the retained drains and leaves the manager (and
/// the Host authority slot) alone for the ordered settlement that follows.
#[tokio::test]
async fn actor_control_quiesce_is_actor_only_and_joins_drains() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (core, principal, manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;

    // Quiesce blocks on the retained drain until it settles: end the stream
    // from the fixture while the quiesce is running.
    let quiescing = tokio::spawn({
        let handle = handle.clone();
        async move { handle.quiesce_actor_sessions().await }
    });
    wait_until(|| provider.cancels() == 1, "the quiesce cancel").await;
    provider.push(finished(
        &session_id,
        &operation_id,
        FinishReason::Cancelled,
    ));
    provider.close_stream();
    let report = tokio::time::timeout(std::time::Duration::from_secs(20), quiescing)
        .await
        .expect("the Actor-side join settles")
        .expect("the quiesce task joins")
        .expect("the Actor side settles");
    assert!(
        report.cleanup_confirmed,
        "Actor-only quiesce report: {report:?}"
    );
    assert_eq!(
        handle.actor_sessions().observation_count(),
        0,
        "quiesce closes Actor admission and drops the observation readers"
    );
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the retained drains were joined"
    );
    assert!(
        manager.health().await.unwrap().running,
        "quiesce never shuts the manager used by workflows"
    );
    let recorded = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        recorded.run_status,
        CharacterOperationResultRunStatus::Cancelled
    );
    // The Host authority slot is still held: the ordered close owns its
    // release, not the quiesce.
    let err = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect_err("the quiesce never releases the authority slot");
    assert!(
        matches!(err, CoreError::OwnerBusy),
        "slot retained: {err:?}"
    );
}

/// F-001 (plan QC): the quiesce's zero registered-drain count is NOT proof of
/// quiescence. `execute` takes this authority's admission before its first await
/// and retires it only after the drain has been transferred, so an operation
/// admitted before the quiesce latched is still about to own a drain while the
/// registry shows none. The quiesce must therefore join that admission before it
/// may confirm — otherwise the native close that trusts the confirmation closes
/// core storage ahead of the drain it never saw.
#[tokio::test]
async fn actor_control_quiesce_waits_for_an_admitted_execute_before_it_confirms() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;

    // Park the admitted execute inside the provider's own `execute`: admitted,
    // past every registry gate and the manager's session lookup, with no drain
    // registered yet — the window the finding describes.
    provider.park_executions();
    let execute = tokio::spawn({
        let handle = handle.clone();
        let principal = principal.clone();
        let session_id = session_id.to_string();
        async move {
            handle
                .execute(
                    &principal,
                    session_id,
                    execute_request(serde_json::json!({
                        "kind": "prompt",
                        "content": "race the Actor quiesce",
                    })),
                )
                .await
        }
    });
    wait_until(
        || provider.executions() == 1,
        "the admitted execute to reach the provider",
    )
    .await;
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the admitted execute has registered no drain yet"
    );

    // The quiesce meets that admission. A correct quiesce joins it and therefore
    // cannot finish on the zero drain count; an incorrect one confirms at once
    // and gets the whole observation window to prove it.
    let quiesce = tokio::spawn({
        let handle = handle.clone();
        async move { handle.quiesce_actor_sessions().await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert!(
        !quiesce.is_finished(),
        "the quiesce confirmed on a zero registered-drain count while an admitted execute had not transferred its drain"
    );

    // Release the parked execute: it registers the drain the join must account
    // for, the quiesce cancels the operation it can now see, and the Actor side
    // confirms only once that drain settles.
    provider.release_parked_executions();
    let started = execute
        .await
        .expect("the execute task joins")
        .expect("the admitted execute is dispatched");
    let operation_id = HostOperationId(
        Uuid::parse_str(&started.operation_id).expect("the operation id is a UUID"),
    );
    wait_until(|| provider.cancels() == 1, "the quiesce cancel").await;
    provider.push(finished(
        &session_id,
        &operation_id,
        FinishReason::Cancelled,
    ));
    provider.close_stream();
    let report = tokio::time::timeout(std::time::Duration::from_secs(20), quiesce)
        .await
        .expect("the Actor-side join settles")
        .expect("the quiesce task joins")
        .expect("the Actor side settles");
    assert!(report.cleanup_confirmed, "quiesce report: {report:?}");
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the drain the admitted execute transferred was joined"
    );
    let recorded = control_status(&handle, &principal, &operation_id).await;
    assert_eq!(
        recorded.run_status,
        CharacterOperationResultRunStatus::Cancelled
    );
}

/// W-005 (plan QC fix round 2): the authority-wide admission join is entered by
/// EVERY concurrent quiesce on one authority, so the final admission retirement
/// must wake all of them. A single-permit `notify_one` handoff wakes one joiner
/// to recheck zero while the other stays suspended — and no further retirement
/// will ever come, because the count only decreases once admission is frozen.
///
/// Two concurrent quiesces meet ONE admitted execution that fails before it can
/// register a drain, so the retirement they wait on is the last settlement in
/// play and the case stays on the admission join (the drain join is never
/// entered). Both joins must complete, and both must report the settled Actor
/// side: the refused execution left no recorded operation and no live drain.
#[tokio::test]
async fn actor_control_concurrent_quiesces_join_the_same_admitted_retirement() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;

    // Park the admitted execution inside the provider's own `execute`: admitted,
    // past the registry gates and the manager's session lookup, with no drain
    // registered yet. Releasing it then fails the call, so the admission retires
    // with nothing transferred.
    provider.park_executions();
    provider.fail_executions();
    let execute = tokio::spawn({
        let handle = handle.clone();
        let principal = principal.clone();
        let session_id = session_id.to_string();
        async move {
            handle
                .execute(
                    &principal,
                    session_id,
                    execute_request(serde_json::json!({
                        "kind": "prompt",
                        "content": "race two Actor quiesces",
                    })),
                )
                .await
        }
    });
    wait_until(
        || provider.executions() == 1,
        "the admitted execute to reach the provider",
    )
    .await;
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the admitted execute has registered no drain yet"
    );

    // Both quiesces meet that one admission and wait for it to retire.
    let first = tokio::spawn({
        let handle = handle.clone();
        async move { handle.quiesce_actor_sessions().await }
    });
    let second = tokio::spawn({
        let handle = handle.clone();
        async move { handle.quiesce_actor_sessions().await }
    });
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert!(
        !first.is_finished() && !second.is_finished(),
        "both quiesces wait on the one in-flight admission"
    );

    // Retire the admission: the refused execution has failed, so this
    // retirement is the only settlement either joiner will observe.
    provider.release_parked_executions();
    let failed = execute.await.expect("the execute task joins");
    assert!(
        failed.is_err(),
        "the control fixture refuses every execution: {failed:?}"
    );

    let first = tokio::time::timeout(std::time::Duration::from_secs(5), first)
        .await
        .expect("the first quiesce observes the retirement")
        .expect("the first quiesce task joins")
        .expect("the first quiesce settles");
    let second = tokio::time::timeout(std::time::Duration::from_secs(5), second)
        .await
        .expect(
            "a second concurrent quiesce must not be stranded on a retirement that already happened",
        )
        .expect("the second quiesce task joins")
        .expect("the second quiesce settles");
    assert!(
        first.cleanup_confirmed && second.cleanup_confirmed,
        "both joins observe the drained-and-retired admission: {first:?} {second:?}"
    );
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "the refused execution registered no drain to keep retained"
    );
    assert_eq!(
        provider.cancels(),
        0,
        "the refused execution left no recorded operation to cancel"
    );
}

/// W-005 (plan QC fix round 2), second join of the same quiesce: the
/// authority-wide DRAIN join is entered by every concurrent quiesce too, and a
/// settling drain is the last settlement in play. A single-permit handoff would
/// leave the second joiner waiting for a drain that already settled — the same
/// strand the admission join had, on the other half of the ordered close.
///
/// One live Character drain is joined by two concurrent quiesces; ending the
/// operation settles it, and BOTH joins must observe that settlement.
#[tokio::test]
async fn actor_control_concurrent_quiesces_join_the_same_actor_drain() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle, session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;
    let operation_id = control_prompt(&handle, &principal, &session_id).await;
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        1,
        "the live Character operation owns one drain"
    );

    // Both quiesces meet that ONE live drain. With no admission in flight the
    // admission join returns at once, so the drain join is the only place either
    // can wait — which is what the window below proves.
    let first = tokio::spawn({
        let handle = handle.clone();
        async move { handle.quiesce_actor_sessions().await }
    });
    let second = tokio::spawn({
        let handle = handle.clone();
        async move { handle.quiesce_actor_sessions().await }
    });
    wait_until(
        || provider.cancels() >= 1,
        "a quiesce to request cancellation",
    )
    .await;
    // The window lets the second quiesce finish its own cancel round-trip (or
    // read the recorded terminal) and register at the drain join.
    tokio::time::sleep(std::time::Duration::from_millis(250)).await;
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        1,
        "the joined drain is still the live one"
    );
    assert!(
        !first.is_finished() && !second.is_finished(),
        "both quiesces wait on the one live drain"
    );

    // End the operation: the drain settles, and it is the only settlement either
    // joiner will observe.
    provider.push(finished(
        &session_id,
        &operation_id,
        FinishReason::Cancelled,
    ));
    provider.close_stream();

    let first = tokio::time::timeout(std::time::Duration::from_secs(5), first)
        .await
        .expect("the first quiesce observes the drain settlement")
        .expect("the first quiesce task joins")
        .expect("the first quiesce settles");
    let second = tokio::time::timeout(std::time::Duration::from_secs(5), second)
        .await
        .expect("a second concurrent quiesce must not be stranded on a drain that already settled")
        .expect("the second quiesce task joins")
        .expect("the second quiesce settles");
    assert_eq!(
        handle.actor_sessions().unsettled_drain_count(),
        0,
        "both joins observed the settled drain"
    );
    assert!(
        first.cleanup_confirmed || second.cleanup_confirmed,
        "the drain join did not turn the ordered quiesce into a blanket failure: {first:?} {second:?}"
    );
}

/// F-002 (plan QC): the established-owner slot is released at most ONCE per
/// authority epoch. Two clones of one authority closing concurrently can both
/// pass the retained-report check before either publishes a report and both run a
/// confirmed settle; a second release then clears a successor's claim and lets
/// two authorities exist over one service, so the release — not just the report —
/// must be one-shot.
///
/// The two closes run on their OWN tasks (a multi-thread runtime): a session-less
/// close is otherwise short enough to finish inside one poll, which would hide the
/// window the finding describes.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn actor_control_concurrent_confirmed_closes_release_the_slot_once() {
    let env = seed_env().await;
    let (core, _principal) = open_core(&env).await;
    let holder: HostHandle = core.open_host(CountingPort::new()).await.unwrap();
    let first = holder.clone();
    let second = holder.clone();

    let a = tokio::spawn(async move { first.close().await });
    let b = tokio::spawn(async move { second.close().await });
    let a = a
        .await
        .expect("the first close task joins")
        .expect("the first close returns a report");
    let b = b
        .await
        .expect("the second close task joins")
        .expect("the second close returns a report");
    assert!(
        a.cleanup_confirmed && b.cleanup_confirmed,
        "both concurrent closes confirm: {a:?} {b:?}"
    );
    assert_eq!(
        holder.slot_release_count(),
        1,
        "the retired authority may free the established-owner slot at most once"
    );

    // The consequence the one-shot release protects: the freed slot admits
    // exactly one successor, whose claim the retired authority cannot clear.
    let _successor: HostHandle = core
        .open_host(CountingPort::new())
        .await
        .expect("the freed slot admits a successor");
    let refused = core.open_host(CountingPort::new()).await;
    assert!(
        matches!(refused, Err(CoreError::OwnerBusy)),
        "the successor's claim holds: {refused:?}"
    );
}

/// Contract §3: an unsettled Actor drain keeps the authority (slot + drain
/// ownership) across a `close_before`, and the retry stays legal after the core
/// service itself closed.
#[tokio::test]
async fn actor_control_retained_close_before_retries_after_the_service_closed() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (core, _principal, manager, handle, _session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;

    // A live drain the authority still owns.
    handle
        .actor_sessions()
        .spawn_actor_drain(HostSessionId::new(), std::future::pending::<()>());

    let report = handle
        .close_before(Instant::now() + std::time::Duration::from_secs(20))
        .await
        .expect("close_before returns a report");
    assert!(
        !report.cleanup_confirmed,
        "an unsettled drain is never a confirmed cleanup: {report:?}"
    );
    assert!(
        report
            .pending_operations
            .iter()
            .any(|pending| pending.starts_with("actor-drains:")),
        "the report names the drain it still owns: {report:?}"
    );
    let err = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect_err("retained work keeps the authority slot");
    assert!(
        matches!(err, CoreError::OwnerBusy),
        "slot retained: {err:?}"
    );

    // The native owner closes the core service BEFORE the Host settlement, so
    // the retry must not turn into a `closing` refusal.
    core.close().await.expect("the core service closes");
    let retried = handle
        .close_before(Instant::now() + std::time::Duration::from_secs(20))
        .await
        .expect("close_before stays legal on a closed service");
    assert!(
        !retried.cleanup_confirmed,
        "the retained drain still blocks confirmation: {retried:?}"
    );
}

/// Contract §3: `close_before` settles the shared manager and `LocalSet`, stays
/// legal after the core service closed, and releases the authority slot only on
/// the confirmed settlement — a repeated call then reports the retained report.
#[tokio::test]
async fn actor_control_close_before_settles_once_after_the_service_closed() {
    let env = seed_env().await;
    let provider = control_provider(true, 0, false);
    let (core, _principal, _manager, handle, _session_id) =
        control_fixture(&env, Arc::clone(&provider)).await;

    core.close().await.expect("the core service closes");
    let report = handle
        .close_before(Instant::now() + std::time::Duration::from_secs(20))
        .await
        .expect("close_before stays legal on a closed service");
    assert!(report.cleanup_confirmed, "settled close report: {report:?}");
    assert_eq!(report.state, CoreCloseReportState::Closed);
    let repeated = handle
        .close_before(Instant::now() + std::time::Duration::from_secs(20))
        .await
        .expect("a repeated close_before returns");
    assert!(repeated.cleanup_confirmed);
    assert_eq!(
        repeated.pending_operations, report.pending_operations,
        "a repeated close reports the retained settlement instead of settling twice"
    );
}

// ── v1.197 P0-T2 — a Character session's cwd is the admitted creative root ──
//
// §A2: the engine-owner admission pins the selected workspace's canonical
// creative root ONCE at open, and a Character Actor create may only run at
// that exact root. An omitted `cwd` uses the pin; an explicit `cwd` is
// accepted only when its canonical form (a symlink alias included) equals it.
// A descendant, a foreign/Nexus/home root, an absent or deleted pin and an
// unresolvable explicit path are `invalid_input` on `cwd` BEFORE any Host,
// provider or memory effect. The Creator and legacy lanes keep their
// pre-existing cwd boundary. The fixtures below use a REAL registered
// creative root (the operational `meta.json` the daemon/CLI writers emit) and
// a real manager bounded to that pin — no fake pin, no production setter.

/// Register `local_root` in the selected workspace's operational `meta.json`
/// BEFORE the core opens, so the engine-owner admission pins a real registered
/// creative root. Returns that root's canonical form.
async fn register_creative_root(env: &Env, local_root: &Path) -> PathBuf {
    let op_dir = nexus_home_layout::operational_workspace_dir(&env.user_home, CREATOR, "default");
    std::fs::create_dir_all(&op_dir).unwrap();
    std::fs::write(
        op_dir.join("meta.json"),
        serde_json::to_vec(&serde_json::json!({ "local_root": local_root })).unwrap(),
    )
    .unwrap();
    std::fs::canonicalize(local_root).unwrap()
}

/// A real manager whose workspace boundary IS the pin, with the test-driven
/// control provider registered before start (the native composition order), so
/// a Character create at the pin is a launch the provider actually receives.
async fn pin_control_manager(
    env: &Env,
    pin: &Path,
    provider: Arc<ControlProvider>,
) -> Arc<HostManager> {
    use nexus_agent_host::config::{load_config_from_path, validate_workspace_path};

    let workspace_root = validate_workspace_path(pin).unwrap();
    let config_path = host_config_path(env);
    let mut host_config = load_config_from_path(&config_path).unwrap();
    host_config.max_sessions = ATTACHED_MANAGER_MAX_SESSIONS;
    let host = Arc::new(HostManager::new());
    host.register_provider(
        provider,
        LaunchStrategy::NativeCli {
            command: "control-fixture".to_string(),
            args: Vec::new(),
            env: std::collections::HashMap::new(),
        },
    )
    .await;
    host.start(HostStartConfig {
        config_path,
        workspace_root: workspace_root.clone(),
        max_sessions: host_config.max_sessions,
        max_ops_per_session: host_config.max_ops_per_session,
        timeouts: host_config.timeouts.clone(),
        host_config: Some(host_config),
        admitted_catalog: None,
        probe_owner: Some(SessionOwner {
            creator_id: CREATOR.to_string(),
            workspace_root,
            orchestration_run_id: None,
        }),
    })
    .await
    .unwrap();
    host
}

/// The §A2 fixture: a real registered creative root already pinned at open, a
/// real manager bounded to that pin, and the core authority attached to it.
async fn pinned_character_fixture(
    env: &Env,
    pin: &Path,
    provider: Arc<ControlProvider>,
) -> (
    CoreService,
    nexus_core::Principal,
    Arc<HostManager>,
    HostHandle,
) {
    let (core, principal) = open_core(env).await;
    assert_eq!(
        core.admission_creative_root(),
        Some(pin),
        "the engine-owner admission pins the registered creative root"
    );
    let manager = pin_control_manager(env, pin, Arc::clone(&provider)).await;
    let handle = core
        .attach_host(Arc::clone(&manager), CountingPort::new())
        .expect("the pin-bound manager attaches");
    (core, principal, manager, handle)
}

/// The Character Actor create body a caller posts, with the optional explicit
/// `cwd` the case is about.
fn character_create(
    character_id: &str,
    binding_id: &str,
    model: &str,
    cwd: Option<&Path>,
) -> nexus_contracts::generated::daemon_api::agent_host::CreateSessionRequest {
    let mut body = serde_json::json!({
        "provider_id": CONTROL_PROVIDER,
        "model": model,
        "actor_ref": { "actor_kind": "character", "character_id": character_id },
        "viewpoint": { "world_id": WORLD, "binding_id": binding_id },
    });
    if let Some(cwd) = cwd {
        body["cwd"] = serde_json::Value::String(cwd.to_string_lossy().into_owned());
    }
    serde_json::from_value(body).expect("the Character create body is schema-valid")
}

/// The Creator Actor create body: no binding, because the Creator carries no
/// binding-local viewpoint.
fn creator_actor_create(
    model: &str,
    cwd: Option<&Path>,
) -> nexus_contracts::generated::daemon_api::agent_host::CreateSessionRequest {
    let mut body = serde_json::json!({
        "provider_id": CONTROL_PROVIDER,
        "model": model,
        "actor_ref": { "actor_kind": "creator", "creator_id": CREATOR },
        "viewpoint": { "world_id": WORLD },
    });
    if let Some(cwd) = cwd {
        body["cwd"] = serde_json::Value::String(cwd.to_string_lossy().into_owned());
    }
    serde_json::from_value(body).expect("the Creator Actor create body is schema-valid")
}

/// The legacy provider-only create body: no actor pair at all.
fn legacy_create(
    model: &str,
    cwd: Option<&Path>,
) -> nexus_contracts::generated::daemon_api::agent_host::CreateSessionRequest {
    let mut body = serde_json::json!({ "provider_id": CONTROL_PROVIDER, "model": model });
    if let Some(cwd) = cwd {
        body["cwd"] = serde_json::Value::String(cwd.to_string_lossy().into_owned());
    }
    serde_json::from_value(body).expect("the legacy create body is schema-valid")
}

/// Session-end capture-queue rows (memory candidates) the guarded store holds.
async fn memory_candidates(env: &Env) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM character_memory_pending_review")
        .fetch_one(&plain_pool(env).await)
        .await
        .unwrap()
}

/// A cwd refusal is the typed `invalid_input` naming `cwd`.
fn is_cwd_refusal(err: &CoreError) -> bool {
    matches!(err, CoreError::InvalidInput { field, .. } if field == "cwd")
}

/// A Character create at the admitted pin is admitted, and the root the
/// provider is actually launched at is that pin — for an omitted cwd, for the
/// pin spelled explicitly, and for a symlink alias of it. The non-Character
/// lanes keep their pre-existing cwd boundary.
#[tokio::test]
async fn character_cwd_uses_admitted_pin() {
    let env = seed_env().await;
    let creative = env.user_home.join("creative");
    std::fs::create_dir_all(&creative).unwrap();
    let pin = register_creative_root(&env, &creative).await;

    let provider = control_provider(true, 0, false);
    let (core, principal, manager, handle) =
        pinned_character_fixture(&env, &pin, Arc::clone(&provider)).await;

    // (1) An omitted cwd uses the pin: the launch observes exactly that root.
    let created = handle
        .create_session(
            &principal,
            character_create(&env.character_id, &env.binding_id, "omitted", None),
        )
        .await
        .expect("a Character create with an omitted cwd is admitted");
    assert_eq!(created.provider_id, CONTROL_PROVIDER);
    assert!(!created.session_id.is_empty());
    assert_eq!(
        provider.launched_cwds(),
        vec![pin.clone()],
        "an omitted cwd launches at the admitted pin"
    );

    // (2) The pin spelled explicitly is the SAME root (exact equality, not a
    // string alias).
    handle
        .create_session(
            &principal,
            character_create(&env.character_id, &env.binding_id, "explicit", Some(&pin)),
        )
        .await
        .expect("the canonical pin is an admissible explicit cwd");
    assert_eq!(provider.launched_cwds(), vec![pin.clone(), pin.clone()]);

    // (3) A symlink alias resolves to the pin, so it is the same root.
    let alias = env.user_home.join("creative-alias");
    std::os::unix::fs::symlink(&pin, &alias).unwrap();
    handle
        .create_session(
            &principal,
            character_create(&env.character_id, &env.binding_id, "alias", Some(&alias)),
        )
        .await
        .expect("a symlink alias of the pin is the same canonical root");
    assert_eq!(
        provider.launched_cwds(),
        vec![pin.clone(), pin.clone(), pin.clone()],
        "a symlink alias canonicalizes to the pin"
    );

    // (4) The Creator Actor and legacy lanes keep the pre-existing boundary: a
    // descendant root is accepted for both, so the pin rule is Character-only.
    let descendant = pin.join("sub");
    std::fs::create_dir_all(&descendant).unwrap();
    let descendant = std::fs::canonicalize(&descendant).unwrap();
    handle
        .create_session(
            &principal,
            creator_actor_create("creator", Some(&descendant)),
        )
        .await
        .expect("a Creator Actor create keeps the pre-existing cwd boundary");
    handle
        .create_session(&principal, legacy_create("legacy", Some(&descendant)))
        .await
        .expect("the legacy provider-only create keeps the pre-existing cwd boundary");
    let launched = provider.launched_cwds();
    assert_eq!(
        launched,
        vec![
            pin.clone(),
            pin.clone(),
            pin.clone(),
            descendant.clone(),
            descendant.clone()
        ],
        "the non-Character lanes still launch at the caller's own root"
    );

    // Each distinct create launched its own session; only the three Character
    // creates were pinned to the creative root.
    assert_eq!(
        handle.actor_sessions().len(),
        4,
        "three Character + one Creator Actor session"
    );
    assert_eq!(manager.list_sessions().await.unwrap().len(), 5);
    assert_eq!(core.admission_creative_root(), Some(pin.as_path()));
}

/// Every non-pin Character cwd is refused as `invalid_input` on `cwd`, before
/// any Host session, launch, registry entry or memory candidate exists; and
/// the stored Actor/binding admission keeps deciding identity refusals.
#[tokio::test]
async fn character_cwd_refuses_non_pin_before_effects() {
    let env = seed_env().await;
    let creative = env.user_home.join("creative");
    std::fs::create_dir_all(&creative).unwrap();
    let pin = register_creative_root(&env, &creative).await;

    let provider = control_provider(true, 0, false);
    let (_core, principal, manager, handle) =
        pinned_character_fixture(&env, &pin, Arc::clone(&provider)).await;

    // The baseline a refused create must leave untouched.
    let baseline_sessions = manager.list_sessions().await.unwrap().len();
    let baseline_indexed = handle.actor_sessions().len();
    let baseline_candidates = memory_candidates(&env).await;
    assert_eq!(
        provider.launched_cwds(),
        Vec::<PathBuf>::new(),
        "boot launches no session"
    );
    assert_eq!(baseline_sessions, 0);
    assert_eq!(baseline_indexed, 0);
    assert_eq!(baseline_candidates, 0);

    // A real descendant exists at the pin and is still not the pin.
    std::fs::create_dir_all(pin.join("sub")).unwrap();
    let sibling = env.user_home.join("sibling");
    std::fs::create_dir_all(&sibling).unwrap();
    let nexus_root = nexus_home_layout::nexus_root_from_home(&env.user_home);

    let refusals: Vec<(&str, PathBuf)> = vec![
        ("a descendant of the pin", pin.join("sub")),
        ("a sibling root", std::fs::canonicalize(&sibling).unwrap()),
        ("the Nexus root", nexus_root.clone()),
        ("the user home", env.user_home.clone()),
        ("a path that does not resolve", pin.join("missing-dir")),
        ("a traversal path", pin.join("..").join("creative")),
    ];
    for (label, cwd) in &refusals {
        let err = handle
            .create_session(
                &principal,
                character_create(&env.character_id, &env.binding_id, label, Some(cwd)),
            )
            .await
            .expect_err("a non-pin Character cwd is refused");
        assert!(
            is_cwd_refusal(&err),
            "{label}: the refusal names cwd, got {err:?}"
        );
    }

    // The cwd IS the pin here, so any refusal is the stored Actor/binding
    // admission still owning the decision — and still preceding every effect.
    let unowned_character = "chr_eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee";
    let missing_binding = "awb_ffffffffffffffffffffffffffffffff";
    for (label, body) in [
        (
            "a Character this creator does not own",
            character_create(unowned_character, &env.binding_id, "unowned", Some(&pin)),
        ),
        (
            "a binding that does not exist",
            character_create(&env.character_id, missing_binding, "no-binding", Some(&pin)),
        ),
    ] {
        let err = handle
            .create_session(&principal, body)
            .await
            .expect_err("an inadmissible Actor pair is refused");
        assert!(
            !is_cwd_refusal(&err),
            "{label}: the refusal is the stored admission, not cwd: {err:?}"
        );
    }

    // No refusal reached the Host plane, the registry, or the capture queue.
    assert_eq!(
        provider.launched_cwds(),
        Vec::<PathBuf>::new(),
        "no refused create launches"
    );
    assert_eq!(
        manager.list_sessions().await.unwrap().len(),
        baseline_sessions
    );
    assert_eq!(handle.actor_sessions().len(), baseline_indexed);
    assert_eq!(memory_candidates(&env).await, baseline_candidates);
}

/// A Character create requires an admitted pin: an unregistered workspace, a
/// pin whose directory is removed after open, and a selection that MOVES after
/// open are all refused or still bound to the ORIGINAL pin — never silently
/// rebound to whatever the metadata says now.
#[tokio::test]
async fn character_cwd_requires_admitted_pin() {
    // (A) A workspace registering no creative root pins nothing: both the
    // omitted and the explicit cwd are refused instead of falling back.
    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    assert!(
        core.admission_creative_root().is_none(),
        "an unregistered workspace pins no creative root"
    );
    let provider = control_provider(true, 0, false);
    let nexus_root = nexus_home_layout::nexus_root_from_home(&env.user_home);
    let manager = pin_control_manager(&env, &nexus_root, Arc::clone(&provider)).await;
    let handle = core
        .attach_host(manager.clone(), CountingPort::new())
        .expect("the unpinned manager attaches");
    for cwd in [None, Some(nexus_root.as_path())] {
        let err = handle
            .create_session(
                &principal,
                character_create(&env.character_id, &env.binding_id, "no-pin", cwd),
            )
            .await
            .expect_err("without an admitted pin a Character create is refused");
        assert!(
            is_cwd_refusal(&err),
            "no pin: the refusal names cwd, got {err:?}"
        );
    }
    assert_eq!(
        provider.launched_cwds(),
        Vec::<PathBuf>::new(),
        "no pin launches nothing"
    );
    assert_eq!(manager.list_sessions().await.unwrap().len(), 0);
    assert_eq!(handle.actor_sessions().len(), 0);

    // (B) A metadata selection that MOVES after open never silently rebinds
    // the session to the new root.
    let env = seed_env().await;
    let first = env.user_home.join("creative-first");
    let second = env.user_home.join("creative-second");
    std::fs::create_dir_all(&first).unwrap();
    std::fs::create_dir_all(&second).unwrap();
    let pin = register_creative_root(&env, &first).await;
    let provider = control_provider(true, 0, false);
    let (core, principal, _manager, handle) =
        pinned_character_fixture(&env, &pin, Arc::clone(&provider)).await;
    let moved = register_creative_root(&env, &second).await;
    assert_ne!(
        moved, pin,
        "the moved selection is a different canonical root"
    );
    assert_eq!(
        core.admission_creative_root(),
        Some(pin.as_path()),
        "a metadata write after open never moves the pinned admission root"
    );
    handle
        .create_session(
            &principal,
            character_create(&env.character_id, &env.binding_id, "moved-omitted", None),
        )
        .await
        .expect("the pinned root still admits an omitted cwd");
    assert_eq!(
        provider.launched_cwds(),
        vec![pin.clone()],
        "a moved selection never silently rebinds the session to the new root"
    );
    let err = handle
        .create_session(
            &principal,
            character_create(
                &env.character_id,
                &env.binding_id,
                "moved-explicit",
                Some(&moved),
            ),
        )
        .await
        .expect_err("the moved root is not the pin");
    assert!(
        is_cwd_refusal(&err),
        "moved root: the refusal names cwd, got {err:?}"
    );
    assert_eq!(
        provider.launched_cwds(),
        vec![pin.clone()],
        "the refused create launched nothing at the moved root"
    );

    // (C) A pin whose directory is removed after open is refused rather than
    // launched at a root that no longer exists.
    let env = seed_env().await;
    let third = env.user_home.join("creative-third");
    std::fs::create_dir_all(&third).unwrap();
    let pin = register_creative_root(&env, &third).await;
    let provider = control_provider(true, 0, false);
    let (_core, principal, _manager, handle) =
        pinned_character_fixture(&env, &pin, Arc::clone(&provider)).await;
    std::fs::remove_dir_all(&pin).unwrap();
    let err = handle
        .create_session(
            &principal,
            character_create(&env.character_id, &env.binding_id, "deleted-pin", None),
        )
        .await
        .expect_err("a deleted pin is refused");
    assert!(
        is_cwd_refusal(&err),
        "deleted pin: the refusal names cwd, got {err:?}"
    );
    assert_eq!(
        provider.launched_cwds(),
        Vec::<PathBuf>::new(),
        "a deleted pin launches nothing"
    );
}

/// A pin whose pathname has been retargeted is refused, not followed.
///
/// The stored pin is the canonical root resolved ONCE at admission, so
/// re-canonicalising it must reproduce the SAME path. Renaming the admitted
/// directory and leaving a symlink at its old pathname to a DIFFERENT existing
/// directory makes the old pathname resolve elsewhere: an omitted cwd must not
/// silently adopt that new root, an explicit cwd naming it must not pass, and
/// the renamed original directory is not the pin either. Every refusal happens
/// before any Host session, provider launch, registry entry or memory candidate.
#[tokio::test]
async fn character_cwd_refuses_a_retargeted_pin() {
    let env = seed_env().await;
    let creative = env.user_home.join("creative");
    std::fs::create_dir_all(&creative).unwrap();
    let pin = register_creative_root(&env, &creative).await;

    let provider = control_provider(true, 0, false);
    let (core, principal, manager, handle) =
        pinned_character_fixture(&env, &pin, Arc::clone(&provider)).await;
    assert_eq!(
        core.admission_creative_root(),
        Some(pin.as_path()),
        "the admission still holds the pin it resolved at open"
    );

    let baseline_sessions = manager.list_sessions().await.unwrap().len();
    let baseline_candidates = memory_candidates(&env).await;
    assert_eq!(
        provider.launched_cwds(),
        Vec::<PathBuf>::new(),
        "boot launches no session"
    );

    // Rename the admitted root away and leave a symlink in its place, pointing
    // at a DIFFERENT directory that really exists.
    let moved = env.user_home.join("creative-moved");
    std::fs::rename(&pin, &moved).unwrap();
    let target = env.user_home.join("creative-target");
    std::fs::create_dir_all(&target).unwrap();
    let target = std::fs::canonicalize(&target).unwrap();
    std::os::unix::fs::symlink(&target, &pin).unwrap();
    assert_ne!(
        target, pin,
        "the symlink target is a different canonical root"
    );
    assert_eq!(
        std::fs::canonicalize(&pin).unwrap(),
        target,
        "the old pathname now resolves to the new target"
    );

    // Only the stored pin pathname is the exact-match authority: the omitted
    // cwd, the new symlink target, the retargeted old pathname and the renamed
    // original directory are all refused.
    let cases: Vec<(&str, Option<&Path>)> = vec![
        ("an omitted cwd", None),
        ("the new symlink target", Some(target.as_path())),
        ("the retargeted old pathname", Some(pin.as_path())),
        ("the renamed admitted directory", Some(moved.as_path())),
    ];
    for (label, cwd) in cases {
        let err = handle
            .create_session(
                &principal,
                character_create(&env.character_id, &env.binding_id, label, cwd),
            )
            .await
            .expect_err("a retargeted pin is refused");
        assert!(
            is_cwd_refusal(&err),
            "{label}: the refusal names cwd, got {err:?}"
        );
    }

    // No refusal reached the Host plane, the registry, a provider launch or the
    // capture queue.
    assert_eq!(
        provider.launched_cwds(),
        Vec::<PathBuf>::new(),
        "a retargeted pin launches nothing"
    );
    assert_eq!(
        manager.list_sessions().await.unwrap().len(),
        baseline_sessions
    );
    assert_eq!(handle.actor_sessions().len(), 0);
    assert_eq!(memory_candidates(&env).await, baseline_candidates);
}
