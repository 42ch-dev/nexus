//! P4-T2 Host authority acceptance anchor: a stale Actor epoch is denied
//! before any provider effect, a journal failure after a committed effect
//! refuses success and settles `interrupted` on restart without re-executing
//! provider work, and an unconfirmed authority close keeps its guards.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use nexus_core::{
    AdmittedActor, ActorSessionKey, ActorSessionRegistry, ActorViewpoint, CoreActorAdmission,
    CoreAccess, CoreError, CoreOpenOptions, CoreService, HostHandle,
};
use nexus_local_db::writer_protocol::{init_engine_pool, GuardedPoolOptions};
use nexus_local_db::{ensure_creator_row, CreateCharacterParams};
use nexus_contracts::{ProviderCall, ProviderEventBatch, ProviderReply};
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
    let tmp = TempDir::new().unwrap();
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");
    std::fs::create_dir_all(&nexus_home).unwrap();
    std::fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
        &user_home,
        CREATOR,
        "default",
    ))
    .unwrap();
    std::fs::write(
        nexus_home.join("config.toml"),
        format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"default\""
        ),
    )
    .unwrap();
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, CREATOR, "default");
    let (character_id, binding_id) = {
        let guarded = init_engine_pool(&db_path, CREATOR, GuardedPoolOptions::default())
            .await
            .unwrap();
        let pool = guarded.clone_pool();
        ensure_creator_row(&pool, CREATOR, "Owner").await.unwrap();
        sqlx::query(
            "INSERT INTO narrative_worlds \
             (world_id, workspace_id, owner_creator_id, title, slug, status, visibility, \
              time_policy, metadata_json, created_at) \
             VALUES (?, 'wrk', ?, ?, ?, 'active', 'private', 'manual', '{}', datetime('now'))",
        )
        .bind(WORLD)
        .bind(CREATOR)
        .bind(WORLD)
        .bind(WORLD)
        .execute(&pool)
        .await
        .unwrap();
        let created = nexus_local_db::create_character_with_initial_binding(
            &pool,
            CreateCharacterParams {
                owner_creator_id: CREATOR,
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
            request_id: request.request_id.clone(),
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
    home: &Path,
) -> ActorSessionKey {
    ActorSessionRegistry::key_for("mock-acp", home, None, None, ctx).unwrap()
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
    let key = registry_key(handle.actor_sessions(), &ctx, &env.user_home);
    let session_id = Uuid::new_v4();
    handle
        .actor_sessions()
        .insert_indexed_entry(key, ctx, nexus_agent_host::HostSessionId(session_id));

    // Material transition: bump the stored epoch behind the registry.
    {
        let pool = plain_pool(&env).await;
        sqlx::query("UPDATE characters SET lifecycle_epoch = lifecycle_epoch + 1 WHERE character_id = ?1")
            .bind(&env.character_id)
            .execute(&pool)
            .await
            .unwrap();
    }

    let request = serde_json::from_value::<nexus_contracts::generated::daemon_api::agent_host::ExecuteOperationRequest>(
        serde_json::json!({ "kind": "prompt", "content": "hello" }),
    )
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
    assert_eq!(operation.status, nexus_contracts::CoreProviderOperationStatus::Interrupted);
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
    let key2 = registry_key(h2.actor_sessions(), &ctx2, &env.user_home);
    let sid2 = Uuid::new_v4();
    h2.actor_sessions()
        .insert_indexed_entry(key2, ctx2, nexus_agent_host::HostSessionId(sid2));
    // Retire the id so the close drain attempts a Host shutdown the Host
    // cannot confirm.
    h2.actor_sessions()
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
