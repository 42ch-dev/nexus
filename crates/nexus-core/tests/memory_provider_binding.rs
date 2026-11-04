//! N3 Task 3: the pinned `MemoryProviderBinding` and its open/close lifecycle.
//!
//! The binding is crate-private, so these regressions drive it through the
//! public open surface: the selection precedence (`Some` override, disk table
//! read once per open, absent table = local), the pin's stability for the whole
//! open, the real `memory_remote_disabled` refusal while the `memory-remote`
//! cohort does not exist, close idempotence, and the guard release a failed
//! open owes the next one.

use std::fs;
use std::path::PathBuf;

use nexus_core::memory_provider::{LocalMemoryProvider, DEFAULT_SPACE_ID};
use nexus_core::{
    CoreAccess, CoreError, CoreOpenOptions, CoreService, MemoryProviderSelection,
    RemoteMemoryProviderSelection,
};
use nexus_local_db::writer_protocol::{join_live_engine_pool, GuardedPoolOptions};
use tempfile::TempDir;

const CREATOR: &str = "ctr_aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn provider_table(kind: &str) -> String {
    format!("\n[memory_provider]\nkind = \"{kind}\"\n")
}

fn remote_table() -> String {
    format!(
        "\n[memory_provider]\n\
         kind = \"remote\"\n\
         space_id = \"spc_remote\"\n\
         credential_id = \"cred-alpha\"\n\
         connect_endpoint = \"wss://memory.example.test/\"\n\
         remote_pubkey_hex = \"{}\"\n\
         allowlist = [\"peer-alpha\"]\n",
        "ab".repeat(32)
    )
}

fn remote_selection() -> MemoryProviderSelection {
    MemoryProviderSelection::Remote(RemoteMemoryProviderSelection {
        space_id: "spc_remote".to_string(),
        credential_id: "cred-alpha".to_string(),
        connect_endpoint: "wss://memory.example.test/".to_string(),
        remote_pubkey_hex: "ab".repeat(32),
        allowlist: vec!["peer-alpha".to_string()],
    })
}

fn is_remote_disabled(error: &CoreError) -> bool {
    matches!(error, CoreError::Coded { code, .. } if code == "memory_remote_disabled")
}

struct Env {
    _tmp: TempDir,
    user_home: PathBuf,
}

impl Env {
    fn new(memory_table: Option<&str>) -> Self {
        let tmp = TempDir::new().expect("temp home");
        let user_home = tmp.path().to_path_buf();
        fs::create_dir_all(user_home.join(".nexus42")).expect("nexus home");
        fs::create_dir_all(nexus_home_layout::operational_workspace_dir(
            &user_home, CREATOR, "default",
        ))
        .expect("workspace dir");
        let env = Self {
            _tmp: tmp,
            user_home,
        };
        env.write_config(memory_table);
        env
    }

    fn write_config(&self, memory_table: Option<&str>) {
        let mut text = format!(
            "active_creator_id = \"{CREATOR}\"\n\
             [active_workspace_slug_by_creator]\n\
             \"{CREATOR}\" = \"default\"\n"
        );
        if let Some(table) = memory_table {
            text.push_str(table);
        }
        fs::write(self.user_home.join(".nexus42").join("config.toml"), text).expect("seed config");
    }

    fn options(&self, memory: Option<MemoryProviderSelection>) -> CoreOpenOptions {
        CoreOpenOptions {
            user_home: self.user_home.clone(),
            access: CoreAccess::EngineOwner,
            memory,
        }
    }

    fn db_path(&self) -> PathBuf {
        nexus_home_layout::workspace_state_db_path(&self.user_home, CREATOR, "default")
    }
}

fn local_provider(core: &CoreService) -> std::sync::Arc<LocalMemoryProvider> {
    core.memory_provider_for_test().expect("pinned local provider")
}

/// A home with no `[memory_provider]` table keeps the byte-equivalent local
/// default, and that local binding really serves the memory family.
#[tokio::test]
async fn absent_provider_table_pins_the_local_default() {
    let env = Env::new(None);
    let core = CoreService::open(env.options(None)).await.expect("local default");
    let principal = core.active_principal().await.expect("principal");

    let provider = local_provider(&core);
    assert_eq!(provider.space_id(), DEFAULT_SPACE_ID);
    assert!(provider.is_writable(), "an engine owner pins a writable local provider");
    assert_eq!(core.count_pending_reviews(&principal).await.expect("provider read").count, 0);

    let report = core.close().await.expect("close");
    assert!(report.cleanup_confirmed);
}

/// `Some(Local)` is an open-only override: it beats a remote disk selection for
/// this open and never writes configuration, so the next open still sees the
/// disk's real refusal.
#[tokio::test]
async fn explicit_local_overrides_a_remote_disk_selection() {
    let env = Env::new(Some(&remote_table()));
    let core = CoreService::open(env.options(Some(MemoryProviderSelection::Local)))
        .await
        .expect("explicit local override opens");
    assert_eq!(local_provider(&core).space_id(), DEFAULT_SPACE_ID);
    core.close().await.expect("close");

    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("the disk selection applies to the next open");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
}

/// `Some(...)` never reads the provider table, so a malformed one cannot change
/// an explicit open.
#[tokio::test]
async fn explicit_local_never_reads_the_provider_table() {
    let env = Env::new(Some(&provider_table("bogus")));
    let core = CoreService::open(env.options(Some(MemoryProviderSelection::Local)))
        .await
        .expect("an explicit selection does not read the malformed table");
    assert_eq!(local_provider(&core).space_id(), DEFAULT_SPACE_ID);
    core.close().await.expect("close");
}

/// The disk table is read once per open: a live binding never re-reads or
/// retargets, while the next open resolves the current table. A sibling open
/// that fails on that reread releases only its own admissions.
#[tokio::test]
async fn provider_selection_is_pinned_for_the_open_and_reread_between_opens() {
    let env = Env::new(None);
    let core = CoreService::open(env.options(None)).await.expect("local default");
    let principal = core.active_principal().await.expect("principal");
    assert_eq!(core.count_pending_reviews(&principal).await.expect("provider read").count, 0);

    env.write_config(Some(&remote_table()));
    assert_eq!(
        local_provider(&core).space_id(),
        DEFAULT_SPACE_ID,
        "a live open never re-reads or retargets its provider"
    );
    assert_eq!(
        core.count_pending_reviews(&principal).await.expect("provider read").count,
        0,
        "the pinned local binding keeps serving reads after the table changed"
    );

    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("the next open reads the changed table");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
    assert_eq!(
        core.count_pending_reviews(&principal).await.expect("provider read").count,
        0,
        "the failed sibling open released only its own admissions"
    );

    core.close().await.expect("close");

    env.write_config(None);
    let reopened = CoreService::open(env.options(None))
        .await
        .expect("a later open reads the restored table");
    assert_eq!(local_provider(&reopened).space_id(), DEFAULT_SPACE_ID);
    reopened.close().await.expect("close");

    env.write_config(Some(&remote_table()));
    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("the reread sees the remote table again");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
}

/// A remote selection is refused with the real `memory_remote_disabled` code
/// and the failed open releases the writer admission it took, so the next open
/// acquires fresh instead of joining a half-open service.
#[tokio::test]
async fn remote_selection_is_refused_and_releases_its_own_guards() {
    let env = Env::new(Some(&remote_table()));
    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("a remote selection is refused while the feature does not exist");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
    assert!(
        join_live_engine_pool(&env.db_path(), GuardedPoolOptions::default())
            .await
            .expect("retained guard probe")
            .is_none(),
        "the failed open must release its own retained engine admission"
    );

    env.write_config(None);
    let core = CoreService::open(env.options(None))
        .await
        .expect("the next open acquires the released admissions");
    assert_eq!(local_provider(&core).space_id(), DEFAULT_SPACE_ID);
    assert!(core.close().await.expect("close").cleanup_confirmed);

    let reopened = CoreService::open(env.options(None))
        .await
        .expect("a closed open released its admissions once more");
    assert!(reopened.close().await.expect("close").cleanup_confirmed);
}

/// Close is idempotent: every caller observes the ONE settled report, memory
/// access refuses through the closing barrier, and the home opens again.
#[tokio::test]
async fn close_is_idempotent_and_releases_the_binding() {
    let env = Env::new(None);
    let core = CoreService::open(env.options(None)).await.expect("local default");

    let first = core.close().await.expect("first close");
    let second = core.close().await.expect("second close");
    assert_eq!(first.state, second.state);
    assert_eq!(first.cleanup_confirmed, second.cleanup_confirmed);
    assert!(first.cleanup_confirmed, "the drain settled");
    assert!(matches!(
        core.memory_provider_for_test(),
        Err(CoreError::Closing)
    ));

    let reopened = CoreService::open(env.options(None))
        .await
        .expect("a closed service released its home");
    assert_eq!(local_provider(&reopened).space_id(), DEFAULT_SPACE_ID);
    reopened.close().await.expect("close");
}

/// The public `Some(Remote(..))` override is the same typed refusal as the disk
/// path, before any credential file exists: no fake connection and no silent
/// local fallback.
#[tokio::test]
async fn explicit_remote_selection_is_refused_without_reading_credentials() {
    let env = Env::new(None);
    let error = CoreService::open(env.options(Some(remote_selection())))
        .await
        .err()
        .expect("an explicit remote selection is refused");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );
}

/// The local provider opens its own direct admission, so a confirmed close must
/// release it with the binding: no shared writer lock may survive the close, and
/// repeated opens must not accumulate claimed admissions.
#[tokio::test]
async fn confirmed_close_releases_the_local_providers_direct_admission() {
    let env = Env::new(None);
    for _ in 0..3 {
        let core = CoreService::open(env.options(None))
            .await
            .expect("local default");
        let principal = core.active_principal().await.expect("principal");
        assert_eq!(
            core.count_pending_reviews(&principal).await.expect("read").count,
            0
        );
        assert!(core.close().await.expect("close").cleanup_confirmed);

        // The exclusive migration fence is the real OS-lock probe: it succeeds
        // only when no writer admission — the core's or the dropped provider's
        // — still holds the shared migration lock.
        let fence = nexus_local_db::writer_protocol::acquire_store_reset_fence(&env.db_path())
            .expect("a confirmed close releases every writer admission it opened");
        drop(fence);
    }
}

/// F10: the measured provider open is cancelled in flight after its admission
/// is registered and *its own* seed write really landed, but before the store is
/// delivered. Both stage events are attributable to this invocation alone: the
/// registration row carries a writer id that did not exist before the spawn, and
/// the seed write is the `core_changes` row that this exact writer id produced.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelled_provider_open_after_seed_releases_its_admission() {
    let env = Env::new(None);
    let db_path = env.db_path();
    let home = env.user_home.clone();

    // Bootstrap the schema once, then take the set of writer ids that already
    // exist: only an id outside it can belong to the measured open.
    let bootstrap = LocalMemoryProvider::open(home.clone(), CREATOR, "default")
        .await
        .expect("bootstrap provider");
    drop(bootstrap);
    let known_writers = writer_ids(&db_path).await;

    let opening_home = home.clone();
    let opening =
        tokio::spawn(async move { LocalMemoryProvider::open(opening_home, CREATOR, "default").await });

    // Step 1: the measured open registers a writer id that did not exist before
    // the spawn, so every following observation is about this invocation.
    let mut measured_writer = None;
    for _ in 0..200_000 {
        if let Some(writer_id) = writer_ids(&db_path)
            .await
            .into_iter()
            .find(|writer_id| !known_writers.contains(writer_id))
        {
            measured_writer = Some(writer_id);
            break;
        }
        if opening.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    let measured_writer = measured_writer.expect("the measured open registered itself");

    // Step 2: the seed write, proven by the change row that only this writer id
    // produces for workspace_meta.
    let mut seeded = false;
    for _ in 0..200_000 {
        if meta_changes_of(&db_path, &measured_writer).await > 0 {
            seeded = true;
            break;
        }
        if opening.is_finished() {
            break;
        }
        tokio::task::yield_now().await;
    }
    assert!(
        seeded,
        "the measured open reached its own seed write (writer {measured_writer})"
    );
    assert!(
        !opening.is_finished(),
        "the open is still in flight when its own seed write is observed"
    );
    opening.abort();
    let join_error = match opening.await {
        Ok(_) => panic!("the aborted task must not have completed"),
        Err(join_error) => join_error,
    };
    assert!(
        join_error.is_cancelled(),
        "the task was really cancelled, not completed"
    );

    // Only the cancelled open's own admission ended, and it ended for real.
    assert!(
        eventual_fence(&db_path).await,
        "the cancelled open released its own OS admission"
    );
}

/// Every writer id registered in this workspace.
async fn writer_ids(db_path: &std::path::Path) -> Vec<String> {
    let Ok(pool) = read_pool(db_path).await else {
        return Vec::new();
    };
    let rows: Vec<(String,)> = sqlx::query_as("SELECT writer_id FROM core_writer_registration")
        .fetch_all(&pool)
        .await
        .unwrap_or_default();
    pool.close().await;
    rows.into_iter().map(|row| row.0).collect()
}

/// The `workspace_meta` change rows a single writer id produced — the persisted
/// trace of that writer's own writes.
async fn meta_changes_of(db_path: &std::path::Path, writer_id: &str) -> i64 {
    let Ok(pool) = read_pool(db_path).await else {
        return 0;
    };
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM core_changes WHERE resource_kind = 'workspace_meta' AND writer_id = ?",
    )
    .bind(writer_id)
    .fetch_one(&pool)
    .await
    .unwrap_or((0,));
    pool.close().await;
    count.0
}

async fn read_pool(db_path: &std::path::Path) -> Result<sqlx::SqlitePool, sqlx::Error> {
    sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=ro", db_path.display())).await
}

async fn raw_pool(db_path: &std::path::Path) -> sqlx::SqlitePool {
    sqlx::SqlitePool::connect(&format!("sqlite://{}?mode=rwc", db_path.display()))
        .await
        .expect("raw pool")
}

/// Bounded wait for the store-reset fence; only the expected contention is
/// accepted, anything else is a real failure.
async fn eventual_fence(db_path: &std::path::Path) -> bool {
    for _ in 0..200_000 {
        match nexus_local_db::writer_protocol::acquire_store_reset_fence(db_path) {
            Ok(_) => return true,
            Err(error) => assert!(
                matches!(error, nexus_local_db::LocalDbError::OwnerBusy { .. }),
                "the only expected refusal is OwnerBusy, got {error:?}"
            ),
        }
        tokio::task::yield_now().await;
    }
    false
}

/// F10: a real vision of compose/provision failing after the pool and seed
/// exist — a valid migrated workspace whose provider grant insert is rejected
/// makes provisioning fail for real, and only its own undelivered claim ends.
#[tokio::test]
async fn failed_provisioning_keeps_an_adopted_user_and_releases_its_own_admission() {
    let env = Env::new(None);
    let db_path = env.db_path();

    // A live adopted user: the engine owner keeps the workspace and its memory.
    let engine = CoreService::open(env.options(None))
        .await
        .expect("live engine owner");
    let principal = engine.active_principal().await.expect("principal");
    assert_eq!(
        engine.count_pending_reviews(&principal).await.expect("read").count,
        0
    );

    // A valid SQLite fixture that makes the provisioning write fail while every
    // read still works: a trigger on the grant table rejects the bootstrap
    // grant insert. The workspace stays migrated and readable.
    let raw = raw_pool(&db_path).await;
    sqlx::query(
        "CREATE TRIGGER reject_provider_grants BEFORE INSERT ON memory_grants \
         BEGIN SELECT RAISE(ABORT, 'providing blocked by fixture'); END",
    )
    .execute(&raw)
    .await
    .expect("reject grants");
    raw.close().await;

    let error = match LocalMemoryProvider::open(env.user_home.clone(), CREATOR, "default").await {
        Ok(_) => panic!("provisioning must fail on the rejected grant insert"),
        Err(error) => error,
    };
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("providing blocked by fixture"),
        "the failure is the real provisioning write, got {rendered}"
    );

    // The failed open took nothing from the adopted user: it still reads and
    // still holds its own admission.
    assert_eq!(
        engine.count_pending_reviews(&principal).await.expect("read").count,
        0,
        "the adopted engine user is untouched"
    );
    assert!(
        nexus_local_db::writer_protocol::acquire_store_reset_fence(&db_path).is_err(),
        "the adopted engine admission is still held"
    );

    // Settle the real user, then the fence must become free: nothing of the
    // failed open may still hold the workspace.
    assert!(engine.close().await.expect("close").cleanup_confirmed);
    assert!(
        eventual_fence(&db_path).await,
        "after the adopted user ended, the failed open left no claim behind"
    );
}

/// A normal provider close settles the provider's own pool, so a genuinely
/// pending guarded migration runs afterwards instead of waiting on a pool no
/// one can use.
#[tokio::test]
async fn pending_guarded_migration_runs_after_a_normal_provider_close() {
    let env = Env::new(None);
    let core = CoreService::open(env.options(None))
        .await
        .expect("local default");
    let principal = core.active_principal().await.expect("principal");
    assert_eq!(
        core.count_pending_reviews(&principal).await.expect("read").count,
        0
    );
    assert!(core.close().await.expect("close").cleanup_confirmed);

    // Make a real migration pending: drop one applied row the embedded
    // migrator re-applies (its SQL is idempotent).
    let pool = sqlx::SqlitePool::connect(&format!(
        "sqlite://{}?mode=rwc",
        env.db_path().display()
    ))
    .await
    .expect("raw pool");
    let dropped = sqlx::query("DELETE FROM _sqlx_migrations WHERE version = ?")
        .bind(20_260_914_000_001_i64)
        .execute(&pool)
        .await
        .expect("drop the applied row")
        .rows_affected();
    assert_eq!(dropped, 1, "the pinned idempotent migration is applied");
    pool.close().await;

    nexus_local_db::writer_protocol::run_guarded_migrations(&env.db_path())
        .await
        .expect("a pending guarded migration runs after the provider closed");
}

/// A failed `DirectWriter` pin must release only what that open owned: the
/// live `EngineOwner`'s retained admission stays in the process registry, so
/// the creator remains joinable afterwards.
#[tokio::test]
async fn failed_direct_writer_pin_keeps_a_live_engine_owner_joinable() {
    let env = Env::new(None);
    let engine = CoreService::open(env.options(None))
        .await
        .expect("live engine owner");
    let principal = engine.active_principal().await.expect("principal");
    assert_eq!(engine.count_pending_reviews(&principal).await.expect("read").count, 0);

    env.write_config(Some(&remote_table()));
    let direct = CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::DirectWriter,
        memory: None,
    };
    let error = CoreService::open(direct)
        .await
        .err()
        .expect("the remote pin refuses the direct writer open");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );

    env.write_config(None);
    let joined = CoreService::open(env.options(None))
        .await
        .expect("the live engine owner is still joinable after the failed open");
    let joined_principal = joined.active_principal().await.expect("principal");
    assert_eq!(
        joined.count_pending_reviews(&joined_principal).await.expect("read").count,
        0
    );
    assert!(joined.close().await.expect("close").cleanup_confirmed);
    assert_eq!(
        engine.count_pending_reviews(&principal).await.expect("read").count,
        0,
        "the failed direct open did not disturb the live engine owner"
    );
    assert!(engine.close().await.expect("close").cleanup_confirmed);
}

/// A confirmed `DirectWriter` close ends only that open's claim: the live
/// `EngineOwner`'s retained admission (and a later creator join) survives.
#[tokio::test]
async fn successful_direct_writer_close_keeps_a_live_engine_owner_joinable() {
    let env = Env::new(None);
    let engine = CoreService::open(env.options(None))
        .await
        .expect("live engine owner");
    let principal = engine.active_principal().await.expect("principal");
    assert_eq!(engine.count_pending_reviews(&principal).await.expect("read").count, 0);

    let direct = CoreService::open(CoreOpenOptions {
        user_home: env.user_home.clone(),
        access: CoreAccess::DirectWriter,
        memory: None,
    })
    .await
    .expect("a direct writer opens beside the engine owner");
    assert!(direct.close().await.expect("close").cleanup_confirmed);

    let joined = CoreService::open(env.options(None))
        .await
        .expect("a confirmed direct close must not erase the live engine admission");
    let joined_principal = joined.active_principal().await.expect("principal");
    assert_eq!(
        joined.count_pending_reviews(&joined_principal).await.expect("read").count,
        0
    );
    assert!(joined.close().await.expect("close").cleanup_confirmed);
    assert_eq!(
        engine.count_pending_reviews(&principal).await.expect("read").count,
        0,
        "the live engine owner still serves memory after the direct close"
    );
    assert!(engine.close().await.expect("close").cleanup_confirmed);
}

/// An engine creator that goes away after another open joined must leave that
/// adopter's admission — and further joins — intact, and the last claim must
/// release the registry entry instead of leaking it.
#[tokio::test]
async fn creator_close_keeps_a_joined_adopter_and_a_third_join() {
    let env = Env::new(None);
    let creator = CoreService::open(env.options(None))
        .await
        .expect("engine creator");
    let adopter = CoreService::open(env.options(None))
        .await
        .expect("a second engine open joins the live admission");
    let adopter_principal = adopter.active_principal().await.expect("principal");
    assert_eq!(
        adopter.count_pending_reviews(&adopter_principal).await.expect("read").count,
        0
    );

    // The creator's open ends here — a failed pin takes the same release.
    assert!(creator.close().await.expect("close").cleanup_confirmed);

    let third = CoreService::open(env.options(None))
        .await
        .expect("a third open still joins the surviving admission");
    let third_principal = third.active_principal().await.expect("principal");
    assert_eq!(
        third.count_pending_reviews(&third_principal).await.expect("read").count,
        0
    );

    assert!(third.close().await.expect("close").cleanup_confirmed);
    assert!(adopter.close().await.expect("close").cleanup_confirmed);
    assert!(
        join_live_engine_pool(&env.db_path(), GuardedPoolOptions::default())
            .await
            .expect("retained guard probe")
            .is_none(),
        "the last claim released the retained admission"
    );
    let reopened = CoreService::open(env.options(None))
        .await
        .expect("a later open takes a fresh admission");
    assert!(reopened.close().await.expect("close").cleanup_confirmed);
}

/// A failed OPEN THAT JOINED the live engine admission (it owns no retained
/// entry) must leave the creator joinable as well.
#[tokio::test]
async fn failed_joined_engine_pin_keeps_the_creator_joinable() {
    let env = Env::new(None);
    let engine = CoreService::open(env.options(None))
        .await
        .expect("live engine owner");

    env.write_config(Some(&remote_table()));
    let error = CoreService::open(env.options(None))
        .await
        .err()
        .expect("the joining open refuses the remote pin");
    assert!(
        is_remote_disabled(&error),
        "expected the typed refusal, got {error:?}"
    );

    env.write_config(None);
    let joined = CoreService::open(env.options(None))
        .await
        .expect("the joined failure released nothing the creator owns");
    assert!(joined.close().await.expect("close").cleanup_confirmed);
    assert!(engine.close().await.expect("close").cleanup_confirmed);
}
