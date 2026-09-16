//! P2-T0 home-control contract tests: a temporary empty home can register
//! and select a Creator and initialize only the chosen workspace (no
//! provider/engine starts); invalid paths and foreign workspaces are denied;
//! and a stale old selection cannot mutate.

use nexus_cloud_sync::outbox::Outbox;
use nexus_contracts::{
    CoreOutboxResolveRequest, CoreRegisterCreatorRequest, ListWorkspacesQuery,
    SetActiveWorkspaceRequest,
};
use nexus_core::{CoreAccess, CoreError, CoreHomeService, CoreOpenOptions, CoreService, Principal};
use sqlx::Row;
use tempfile::TempDir;

#[allow(clippy::unused_async)] // the test helper mirrors the async service signature
async fn home() -> (TempDir, CoreHomeService) {
    let tmp = tempfile::tempdir().unwrap();
    let svc = CoreHomeService::open(tmp.path().to_path_buf()).expect("open home");
    (tmp, svc)
}

/// Materialize an operational workspace registration (creative root +
/// `meta.json`) the way the daemon create path lays it out — this is the
/// on-disk existence the selection path validates; the guarded state-DB
/// initialization itself is under test.
fn materialize_operational_dir(user_home: &std::path::Path, creator: &str, slug: &str) {
    let op_dir = nexus_home_layout::operational_workspace_dir(user_home, creator, slug);
    std::fs::create_dir_all(&op_dir).unwrap();
    let meta = serde_json::json!({
        "schema_version": 1,
        "creator_id": creator,
        "workspace_slug": slug,
        "local_root": user_home.join("creative").join(creator).join(slug),
        "workspace_id": null,
        "created_at": "2020-01-01T00:00:00Z",
    });
    std::fs::write(
        op_dir.join("meta.json"),
        serde_json::to_string_pretty(&meta).unwrap(),
    )
    .unwrap();
}

async fn register_named(home: &CoreHomeService, name: &str) -> String {
    let request = CoreRegisterCreatorRequest {
        display_name: Some(name.parse().unwrap()),
        platform_creator_id: None,
    };
    let detail = home.register_creator(request).await.expect("register");
    assert!(detail.is_active);
    detail.creator_id
}
/// One mutation probe through the service — the observable "principal still
/// works" contract used by the stale-selection regressions.
async fn probe_changes(
    core: &CoreService,
    principal: &Principal,
) -> Result<nexus_contracts::CoreChangesResponse, CoreError> {
    core.changes(
        principal,
        nexus_contracts::CoreChangesRequest {
            after_sequence: "0".parse().unwrap(),
            limit: std::num::NonZeroU64::MIN,
        },
    )
    .await
}

/// AC-P2-T0: `uninitialized_home_can_register_and_select_without_engine`
#[tokio::test]
async fn uninitialized_home_can_register_and_select_without_engine() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let nexus_home = user_home.join(".nexus42");

    // Nothing exists yet: registration happens with no selected workspace.
    assert!(!nexus_home.exists());

    // Register — minted persistent identity becomes the active creator.
    let detail = svc
        .register_creator(CoreRegisterCreatorRequest {
            display_name: Some("P2T0 Author".parse().unwrap()),
            platform_creator_id: None,
        })
        .await
        .expect("register creator");
    let creator_id = detail.creator_id.clone();
    assert!(creator_id.starts_with("ctr_local"));
    assert!(detail.is_active);
    assert_eq!(detail.display_name.as_deref(), Some("P2T0 Author"));

    // Stores after registration: global identity DB + config.toml only.
    assert!(nexus_home.join("state.db").exists());
    let config = std::fs::read_to_string(nexus_home.join("config.toml")).unwrap();
    assert!(config.contains(&format!("active_creator_id = \"{creator_id}\"")));
    // No workspace state DB was initialized by registration.
    let creators_root = nexus_home.join("creators");
    assert!(!creators_root.exists());

    // Discovery is empty before any workspace exists.
    let listed = svc.list_workspaces().await.expect("list");
    assert!(listed.items.is_empty());

    // Configuration reflects the selected creator, pre-workspace.
    let config = svc.configuration().await.expect("configuration");
    assert_eq!(
        config.active_creator_id.as_deref(),
        Some(creator_id.as_str())
    );
    assert_eq!(config.active_workspace_slug.as_deref(), Some("default"));

    // Select the chosen workspace: initialize only it.
    materialize_operational_dir(&user_home, &creator_id, "default");
    let selected = svc
        .select_workspace(SetActiveWorkspaceRequest {
            creator_id: None,
            workspace_slug: "default".to_string(),
        })
        .await
        .expect("select");
    assert_eq!(selected.creator_id, creator_id);
    assert_eq!(selected.workspace_slug, "default");

    // Only the chosen workspace was initialized: guarded state DB + creators row.
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, &creator_id, "default");
    assert!(db_path.exists());
    let ro = nexus_local_db::open_pool_read_only(&db_path).await.unwrap();
    let creators: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM creators")
        .fetch_one(&ro)
        .await
        .unwrap();
    let creator_rows: (String,) = sqlx::query_as("SELECT creator_id FROM creators LIMIT 1")
        .fetch_one(&ro)
        .await
        .unwrap();
    ro.close().await;
    assert_eq!(creators, 1);
    assert_eq!(creator_rows.0, creator_id);

    // No provider/engine started: the engine lock file for this DB is absent.
    let engine_lock =
        nexus_home_layout::workspace_state_db_path(&user_home, &creator_id, "default");
    let engine_lock = std::path::PathBuf::from(format!("{}.engine.lock", engine_lock.display()));
    assert!(!engine_lock.exists());

    // Selection persisted per creator.
    let config = svc.configuration().await.expect("configuration after");
    assert_eq!(config.active_workspace_slug.as_deref(), Some("default"));
    let _ = engine_lock; // silence unused on non-test cfg paths
}

#[tokio::test]
async fn register_display_name_is_validated_and_collision_checked() {
    let (tmp, svc) = home().await;

    // Empty and over-long names are rejected at the front door.
    let request = CoreRegisterCreatorRequest {
        display_name: Some("   ".parse().unwrap()),
        platform_creator_id: None,
    };
    assert!(matches!(
        svc.register_creator(request).await,
        Err(CoreError::InvalidInput { field, .. }) if field == "display_name"
    ));

    let long = "x".repeat(65);
    let request = CoreRegisterCreatorRequest {
        display_name: Some(long.parse().unwrap()),
        platform_creator_id: None,
    };
    assert!(matches!(
        svc.register_creator(request).await,
        Err(CoreError::InvalidInput { field, .. }) if field == "display_name"
    ));

    // Convergence: registering the same display name twice converges on the
    // same persistent identity instead of minting a second one.
    let first = register_named(&svc, "Duplicate Author").await;
    let second = register_named(&svc, "Duplicate Author").await;
    assert_eq!(first, second);
    assert!(tmp.path().join(".nexus42/state.db").exists());
}

#[tokio::test]
async fn select_denies_invalid_path_and_foreign_workspace() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_a = register_named(&svc, "Owner Author").await;
    materialize_operational_dir(&user_home, &creator_a, "mine");

    // Path traversal / non-segment slugs are invalid input.
    for slug in ["a/b", "..", "."] {
        let request = SetActiveWorkspaceRequest {
            creator_id: Some(creator_a.clone()),
            workspace_slug: slug.to_string(),
        };
        assert!(
            matches!(
                svc.select_workspace(request).await,
                Err(CoreError::InvalidInput { field, .. }) if field == "workspace_slug"
            ),
            "slug {slug} must be denied"
        );
    }

    // A workspace registered to another creator is foreign: denied even
    // though a directory exists on disk.
    let creator_b = register_named(&svc, "Second Author").await;
    let foreign = SetActiveWorkspaceRequest {
        creator_id: Some(creator_b.clone()),
        workspace_slug: "mine".to_string(),
    };
    match svc.select_workspace(foreign).await {
        Err(CoreError::NotFound { resource }) => {
            assert!(resource.contains("mine"), "denial mentions the workspace");
        }
        other => panic!("foreign workspace must be NotFound, got {other:?}"),
    }

    // A nonexistent workspace stays NotFound (retained wording).
    let missing = SetActiveWorkspaceRequest {
        creator_id: Some(creator_a.clone()),
        workspace_slug: "nonexistent_ws".to_string(),
    };
    match svc.select_workspace(missing).await {
        Err(CoreError::NotFound { resource }) => {
            assert!(resource.contains("nonexistent_ws"));
        }
        other => panic!("missing workspace must be NotFound, got {other:?}"),
    }
}

#[tokio::test]
async fn stale_old_selection_cannot_mutate() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_a = register_named(&svc, "First Author").await;
    materialize_operational_dir(&user_home, &creator_a, "default");
    svc.select_workspace(SetActiveWorkspaceRequest {
        creator_id: Some(creator_a.clone()),
        workspace_slug: "default".to_string(),
    })
    .await
    .expect("select default");

    // A CoreService opened against creator A's selection holds a working principal.
    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("open core");
    let principal = core.active_principal().await.expect("principal");
    assert!(core.active_principal().await.is_ok());

    // A successful identity change through the home entry invalidates the
    // old selection on disk.
    let _creator_b = register_named(&svc, "Second Author").await;

    // The stale principal can no longer mutate (disk re-read rejects it).
    let result = core
        .changes(
            &principal,
            nexus_contracts::CoreChangesRequest {
                after_sequence: "0".parse().unwrap(),
                limit: std::num::NonZeroU64::MIN,
            },
        )
        .await;
    assert!(
        matches!(result, Err(CoreError::AuthRequired)),
        "stale selection must be AuthRequired, got {result:?}"
    );
    core.close().await.expect("close");
}

#[allow(clippy::too_many_lines)] // one end-to-end home-service scenario
#[tokio::test]
async fn outbox_status_and_resolve_operate_on_the_workspace_outbox() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator = register_named(&svc, "Outbox Author").await;
    materialize_operational_dir(&user_home, &creator, "default");
    svc.select_workspace(SetActiveWorkspaceRequest {
        creator_id: Some(creator.clone()),
        workspace_slug: "default".to_string(),
    })
    .await
    .expect("select");

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("open core");
    let principal = core.active_principal().await.expect("principal");

    // Empty outbox reads as an empty entry list (not a faked zero-count).
    let status = core.outbox_status(&principal).await.expect("status");
    assert!(status.entries.is_empty());

    // Seed two stuck entries directly (fixture-level, mirroring a real
    // conflicted/failed queue left behind by an interrupted push).
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, &creator, "default");
    let pool = nexus_cloud_sync::pool::OutboxPool::new(
        &db_path,
        nexus_cloud_sync::pool::DEFAULT_POOL_SIZE,
    )
    .await
    .expect("outbox pool");
    let exec = pool.inner().clone();
    let outbox = Outbox::with_pool(pool).await.expect("outbox");
    sqlx::query(
        "INSERT INTO outbox_entries (outbox_entry_id, bundle_id, idempotency_key, delivery_state, retry_count, last_error, next_retry_at, created_at)
         VALUES ('obx_conflicted', 'bdl_1', 'key_1', 'conflicted', 1, 'conflict: version_mismatch', '2020-01-01T00:00:00+00:00', '2020-01-01T00:00:00+00:00')",
    )
    .execute(&exec)
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO outbox_entries (outbox_entry_id, bundle_id, idempotency_key, delivery_state, retry_count, last_error, created_at)
         VALUES ('obx_failed', 'bdl_2', 'key_2', 'failed', 0, 'network down', '2020-01-01T00:00:00+00:00')",
    )
    .execute(&exec)
    .await
    .unwrap();

    // Status surfaces the stuck entries with their delivery state and error.
    let status = core.outbox_status(&principal).await.expect("status");
    assert_eq!(status.entries.len(), 2);
    let conflicted = status
        .entries
        .iter()
        .find(|e| e.outbox_entry_id.as_str() == "obx_conflicted")
        .expect("conflicted entry");
    assert_eq!(conflicted.delivery_state.to_string(), "conflicted");
    assert_eq!(
        conflicted.last_error.as_deref(),
        Some("conflict: version_mismatch")
    );

    // Retry re-queues a stuck entry for delivery (ready, no retry time).
    core.resolve_outbox(
        &principal,
        CoreOutboxResolveRequest {
            outbox_entry_id: "obx_conflicted".parse().unwrap(),
            action: nexus_contracts::CoreOutboxResolveRequestAction::Retry,
        },
    )
    .await
    .expect("retry");
    let row = sqlx::query(
        "SELECT delivery_state, next_retry_at FROM outbox_entries WHERE outbox_entry_id = 'obx_conflicted'",
    )
    .fetch_one(&exec)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>(0), "ready");
    assert!(row.get::<Option<String>, _>(1).is_none());

    // Discard drops a stuck entry from the send queue (permanent, no retry).
    core.resolve_outbox(
        &principal,
        CoreOutboxResolveRequest {
            outbox_entry_id: "obx_failed".parse().unwrap(),
            action: nexus_contracts::CoreOutboxResolveRequestAction::Discard,
        },
    )
    .await
    .expect("discard");
    let row = sqlx::query(
        "SELECT delivery_state, next_retry_at, last_error FROM outbox_entries WHERE outbox_entry_id = 'obx_failed'",
    )
    .fetch_one(&exec)
    .await
    .unwrap();
    assert_eq!(row.get::<String, _>(0), "failed");
    assert!(row.get::<Option<String>, _>(1).is_none());
    assert!(row.get::<String, _>(2).contains("discarded"));

    // Discarded entries never re-enter replay (dropped from the send queue).
    let pending = outbox.replay().await.expect("replay");
    assert!(pending.iter().all(|e| e.outbox_entry_id != "obx_failed"));

    // Non-stuck entries are not resolvable: the now-`ready` entry is denied.
    core.resolve_outbox(
        &principal,
        CoreOutboxResolveRequest {
            outbox_entry_id: "obx_conflicted".parse().unwrap(),
            action: nexus_contracts::CoreOutboxResolveRequestAction::Discard,
        },
    )
    .await
    .expect_err("ready entry must not be resolvable");
    core.close().await.expect("close");

    // The daemon-side query shape used by the CLI status stays available.
    let query = ListWorkspacesQuery::default();
    assert!(query.creator_id.is_none());
}

/// Concurrent double-registration of the same display name: the loser of the
/// mint race (TOCTOU unique violation) must surface the byte-exact collision
/// error — never an internal storage failure — and the store must end with
/// exactly one persistent identity for that name.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_same_name_registration_surfaces_collision_not_internal_failure() {
    for _round in 0..3 {
        let (tmp, svc) = home().await;
        // Warm the guarded init first: a single-threaded registration
        // migrates the global store, so the storm below races only the
        // display-name mint (the TOCTOU branch under test), not the
        // fresh-database migration bookkeeping.
        let _warmup = register_named(&svc, "Warmup Author").await;
        let svc = std::sync::Arc::new(svc);
        let display = "Race Author";

        let mut handles = Vec::new();
        for _ in 0..8 {
            let svc = svc.clone();
            handles.push(tokio::spawn(async move {
                svc.register_creator(CoreRegisterCreatorRequest {
                    display_name: Some(display.parse().unwrap()),
                    platform_creator_id: None,
                })
                .await
            }));
        }

        let mut winner_ids = std::collections::HashSet::new();
        for handle in handles {
            match handle.await.expect("task join") {
                Ok(detail) => {
                    assert!(winner_ids.insert(detail.creator_id));
                }
                Err(CoreError::InvalidInput { field, reason }) => {
                    assert_eq!(field, "display_name");
                    assert!(
                        reason.starts_with("creator name collision"),
                        "byte-exact collision surface required, got: {reason}"
                    );
                }
                // Guarded-initialization contention is a pre-existing,
                // unrelated outcome of racing inits.
                Err(CoreError::OwnerBusy | CoreError::Busy) => {}
                Err(other) => panic!(
                    "concurrent registration must not surface an internal failure: {other:?}"
                ),
            }
        }

        // Exactly one persistent identity won the race.
        assert_eq!(
            winner_ids.len(),
            1,
            "registrations must converge to a single identity, got {winner_ids:?}"
        );
        let global_db = tmp.path().join(".nexus42/state.db");
        let ro = nexus_local_db::open_pool_read_only(&global_db)
            .await
            .unwrap();
        let rows = nexus_local_db::list_local_identities(&ro).await.unwrap();
        ro.close().await;
        let persistent_named = rows
            .iter()
            .filter(|r| {
                r.identity_type == "persistent" && r.display_name.as_deref() == Some(display)
            })
            .count();
        assert_eq!(
            persistent_named, 1,
            "exactly one persistent row for {display}"
        );
    }
}

/// Failure-atomic selection: when the chosen workspace's guarded
/// initialization fails, the previous selection stays committed and a
/// principal minted under it keeps working.
#[tokio::test]
async fn select_workspace_failure_keeps_previous_selection_and_principal() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator = register_named(&svc, "Atomic Select Author").await;
    materialize_operational_dir(&user_home, &creator, "default");
    svc.select_workspace(SetActiveWorkspaceRequest {
        creator_id: Some(creator.clone()),
        workspace_slug: "default".to_string(),
    })
    .await
    .expect("select default");

    // A principal minted under the old selection.
    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("open core");
    let principal = core.active_principal().await.expect("principal");

    // The candidate workspace exists on disk but its state-DB path is a
    // directory, so the guarded initialization cannot open it.
    materialize_operational_dir(&user_home, &creator, "broken");
    let broken_db = nexus_home_layout::workspace_state_db_path(&user_home, &creator, "broken");
    std::fs::create_dir_all(&broken_db).unwrap();

    let request = SetActiveWorkspaceRequest {
        creator_id: Some(creator.clone()),
        workspace_slug: "broken".to_string(),
    };
    assert!(
        matches!(
            svc.select_workspace(request).await,
            Err(CoreError::Internal { .. } | CoreError::Busy | CoreError::OwnerBusy)
        ),
        "broken initialization must fail as a mapped storage error"
    );

    // The previous selection is still committed ...
    let config = svc.configuration().await.expect("configuration");
    assert_eq!(config.active_creator_id.as_deref(), Some(creator.as_str()));
    assert_eq!(config.active_workspace_slug.as_deref(), Some("default"));

    // ... and the old principal still mutates (no AuthRequired).
    let result = core
        .changes(
            &principal,
            nexus_contracts::CoreChangesRequest {
                after_sequence: "0".parse().unwrap(),
                limit: std::num::NonZeroU64::MIN,
            },
        )
        .await;
    assert!(
        result.is_ok(),
        "old principal must survive a failed selection"
    );
    core.close().await.expect("close");
}

/// Stale-selection contract across BOTH switch kinds: a principal minted
/// before a workspace switch or an identity switch is `AuthRequired` (disk
/// re-read), and a fresh reopen against the new selection recovers.
#[tokio::test]
async fn stale_principal_after_workspace_and_identity_switch_recovers_on_reopen() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator_a = register_named(&svc, "Switch Author").await;
    materialize_operational_dir(&user_home, &creator_a, "default");
    materialize_operational_dir(&user_home, &creator_a, "second");
    svc.select_workspace(SetActiveWorkspaceRequest {
        creator_id: Some(creator_a.clone()),
        workspace_slug: "default".to_string(),
    })
    .await
    .expect("select default");

    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("open core");
    let principal = core.active_principal().await.expect("principal");
    assert!(probe_changes(&core, &principal).await.is_ok());

    // Workspace switch: the old principal is AuthRequired on its next call.
    svc.select_workspace(SetActiveWorkspaceRequest {
        creator_id: Some(creator_a.clone()),
        workspace_slug: "second".to_string(),
    })
    .await
    .expect("select second");
    assert!(
        matches!(
            probe_changes(&core, &principal).await,
            Err(CoreError::AuthRequired)
        ),
        "workspace switch must invalidate the old principal"
    );
    core.close().await.expect("close");

    // Reopen against the new selection: a working principal again.
    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("reopen core");
    let principal = core
        .active_principal()
        .await
        .expect("principal after reopen");
    assert!(probe_changes(&core, &principal).await.is_ok());

    // Identity switch: the old principal is AuthRequired again.
    let _creator_b = register_named(&svc, "Switch Successor").await;
    assert!(
        matches!(
            probe_changes(&core, &principal).await,
            Err(CoreError::AuthRequired)
        ),
        "identity switch must invalidate the old principal"
    );
    core.close().await.expect("close");

    // Recovery on reopen needs the new identity to hold a workspace first.
    let creator_b = svc
        .configuration()
        .await
        .expect("configuration")
        .active_creator_id
        .expect("active creator b");
    materialize_operational_dir(&user_home, &creator_b, "default");
    svc.select_workspace(SetActiveWorkspaceRequest {
        creator_id: Some(creator_b.clone()),
        workspace_slug: "default".to_string(),
    })
    .await
    .expect("select for b");
    let core = CoreService::open(CoreOpenOptions {
        user_home: user_home.clone(),
        access: CoreAccess::DirectWriter,
    })
    .await
    .expect("reopen core as b");
    let principal = core.active_principal().await.expect("principal as b");
    assert!(probe_changes(&core, &principal).await.is_ok());
    core.close().await.expect("close");
}

/// `storage_status()` regressions: read-only diagnostics never initialize or
/// migrate the store, and faults are reported instead of faking health.
#[tokio::test]
async fn storage_status_never_initializes_and_reports_faults_honestly() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();

    // No selection: Uninitialized, not a faked empty status.
    assert!(matches!(
        svc.storage_status().await,
        Err(CoreError::Uninitialized)
    ));

    let creator = register_named(&svc, "Diagnostics Author").await;
    materialize_operational_dir(&user_home, &creator, "default");
    svc.select_workspace(SetActiveWorkspaceRequest {
        creator_id: Some(creator.clone()),
        workspace_slug: "default".to_string(),
    })
    .await
    .expect("select");
    let db_path = nexus_home_layout::workspace_state_db_path(&user_home, &creator, "default");

    // Selection resolves but the DB is missing: Uninitialized, and the
    // diagnostics must NOT initialize/migrate the store into existence.
    std::fs::remove_file(&db_path).unwrap();
    assert!(matches!(
        svc.storage_status().await,
        Err(CoreError::Uninitialized)
    ));
    assert!(
        !db_path.exists(),
        "storage_status must never initialize the DB"
    );

    // A valid but never-migrated store (schema absent) degrades to an
    // unhealthy status carrying the reason — versions are None, never a
    // faked zero-version healthy result.
    let empty = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            sqlx::sqlite::SqliteConnectOptions::new()
                .filename(&db_path)
                .create_if_missing(true),
        )
        .await
        .expect("create empty store");
    empty.close().await;
    let status = svc.storage_status().await.expect("empty store status");
    assert!(
        !status.healthy,
        "store without schema must not read healthy"
    );
    assert!(status.health_error.is_some(), "fault must carry a reason");
    assert!(
        status.versions.is_none(),
        "unmigrated store has no versions"
    );

    // A corrupt (non-sqlite) file is a hard diagnostic error, not a faked
    // healthy/empty report.
    std::fs::write(&db_path, b"definitely not a sqlite database").unwrap();
    assert!(
        matches!(svc.storage_status().await, Err(CoreError::Internal { .. })),
        "corrupt store must be diagnosed, not faked"
    );
}

/// `storage_status()` on an initialized store: real versions, real health,
/// real tables.
#[tokio::test]
async fn storage_status_reports_healthy_initialized_store() {
    let (tmp, svc) = home().await;
    let user_home = tmp.path().to_path_buf();
    let creator = register_named(&svc, "Healthy Author").await;
    materialize_operational_dir(&user_home, &creator, "default");
    svc.select_workspace(SetActiveWorkspaceRequest {
        creator_id: Some(creator.clone()),
        workspace_slug: "default".to_string(),
    })
    .await
    .expect("select");

    let status = svc.storage_status().await.expect("status");
    assert_eq!(
        status.db_path,
        nexus_home_layout::workspace_state_db_path(&user_home, &creator, "default")
    );
    assert!(status.healthy);
    assert_eq!(status.health_error, None);
    let versions = status.versions.expect("versions");
    assert!(versions.db_schema_version >= 1);
    assert!(versions.schema_version >= 1);
    assert!(status.tables.iter().any(|t| t == "creators"));
    assert!(status.tables.iter().any(|t| t == "outbox_entries"));
    assert!(status.journal_mode.is_some());
    assert!(status.foreign_keys.is_some());
}
