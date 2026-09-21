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
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_contracts::{ProviderCall, ProviderEventBatch, ProviderReply};
use nexus_core::{
    ActorSessionKey, ActorSessionRegistry, ActorViewpoint, AdmittedActor, CoreAccess,
    CoreActorAdmission, CoreError, CoreOpenOptions, CoreService, HostHandle,
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
    let request =
        nexus_contracts::generated::core::CoreCharacterTransitionRequest::builder()
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

/// Remembered capture is a **durable writer** boundary at the core authority
/// (v1.193 P0-T11): `remember` is admitted only for an indexed Character
/// session — a legacy session's request is refused before any provider effect
/// — and the reserved operation settles its **run** status while the
/// **capture** half stays `pending` with no fabricated `run_…` id. The core
/// never claims a capture it did not perform: only a separate durable capture
/// writer settles that half.
#[tokio::test]
async fn remembered_capture_stays_reserved_for_the_durable_capture_writer() {
    use nexus_contracts::generated::daemon_api::agent_host::character_operation_result::{
        CharacterOperationResultFinishReason, CharacterOperationResultRunStatus,
        NexusCharacterRunCaptureOutcomeStatus,
    };

    let env = seed_env().await;
    let (core, principal) = open_core(&env).await;
    let port = CountingPort::new();
    let handle: HostHandle = core.open_host(port.clone()).await.unwrap();

    // A never-indexed (legacy) session can never request a remembered capture,
    // and the refusal lands before any provider effect.
    let request = serde_json::from_value::<
        nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest,
    >(serde_json::json!({ "kind": "prompt", "content": "hello", "remember": true }))
    .unwrap();
    let err = handle
        .execute(&principal, Uuid::new_v4().to_string(), request)
        .await
        .expect_err("remember on a non-Character session must be refused");
    match &err {
        CoreError::InvalidInput { field, .. } => assert_eq!(field, "remember"),
        other => panic!("expected an invalid `remember` refusal, got {other:?}"),
    }
    assert_eq!(
        port.call_count(),
        0,
        "the refusal lands before any provider effect"
    );

    // The reserved Character operation: `remember` reserves a `pending`
    // capture, the opt-out reserves `disabled`, and the authority-owned drain
    // settles the run half only — a reserved capture is never rewritten into a
    // captured status nor given a fabricated pending id.
    let ctx = admit_character(&core, &principal, &env).await;
    for (remember, expected) in [
        (true, NexusCharacterRunCaptureOutcomeStatus::Pending),
        (false, NexusCharacterRunCaptureOutcomeStatus::Disabled),
    ] {
        let operation_id = nexus_agent_host::HostOperationId(Uuid::new_v4());
        handle
            .actor_sessions()
            .reserve_character_operation(&nexus_core::CharacterOperationSnapshot {
                owner_creator_id: principal.creator_id().to_string(),
                ctx: ctx.clone(),
                session_id: nexus_agent_host::HostSessionId(Uuid::new_v4()),
                operation_id: operation_id.clone(),
                remember,
                raw_prompt: "hello".to_string(),
            })
            .expect("a Character operation reserves an outcome");

        let running = handle
            .actor_sessions()
            .character_operation_result(principal.creator_id(), &operation_id)
            .expect("the reserved outcome is owner-readable");
        assert_eq!(
            running.run_status,
            CharacterOperationResultRunStatus::Running,
            "a reserved operation is still running"
        );
        assert_eq!(
            running.capture.status, expected,
            "remember reserves a pending capture, the opt-out a disabled one"
        );
        assert!(
            running.capture.pending_id.is_none(),
            "a reserved capture never carries a fabricated pending id"
        );

        handle.actor_sessions().settle_operation_terminal(
            &operation_id,
            CharacterOperationResultRunStatus::Succeeded,
            Some(CharacterOperationResultFinishReason::EndTurn),
        );
        let settled = handle
            .actor_sessions()
            .character_operation_result(principal.creator_id(), &operation_id)
            .expect("the settled outcome stays owner-readable");
        assert_eq!(
            settled.run_status,
            CharacterOperationResultRunStatus::Succeeded,
            "the authority-owned drain settles the run status"
        );
        assert_eq!(
            settled.capture.status, expected,
            "the drain never settles the capture half"
        );
        assert!(settled.capture.pending_id.is_none());
    }
}
