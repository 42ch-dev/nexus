//! Shared live-daemon fixture for hermetic CLI integration tests (AR-83 #6).
//!
//! Boots the REAL daemon router (`nexus-daemon-runtime::api::create_router`,
//! keyless) over a real `axum::serve` TCP listener on `127.0.0.1:0`, with a
//! hermetic `$HOME` whose `.nexus42/config.toml` points `daemon_url` at that
//! listener and seeds an active creator + workspace. CLI invocations spawn
//! the real `nexus42` binary with `HOME` set to the same hermetic dir —
//! nothing touches the developer's real `~/.nexus42` (per
//! `nexus42-cli-home-resolution-hermetic`).

#![allow(clippy::unwrap_used, clippy::expect_used)]

use nexus_agent_host::HostFacade;
use nexus_daemon_runtime::api;
use nexus_daemon_runtime::api::auth_middleware::DaemonApiConfig;
use nexus_daemon_runtime::test_utils;
use nexus_daemon_runtime::test_utils::TestTempRoot;
use nexus_daemon_runtime::workspace::WorkspaceState;
use std::path::Path;
use std::process::Output;
use std::sync::Arc;
use tokio::net::TcpListener;

pub mod rn_act4;

/// A live in-process daemon + hermetic HOME pair.
///
/// The shared fixture is compiled into each integration-test crate, which
/// uses a different subset of fields — allow the unused ones per crate.
#[allow(dead_code)]
pub struct LiveDaemon {
    /// Hermetic HOME (parent of `.nexus42`). Kept alive for the whole test.
    pub home: TestTempRoot,
    /// The workspace SQLite pool (for direct test seeding).
    pub pool: sqlx::SqlitePool,
    /// A clone of the daemon `WorkspaceState` (for handler-level seeding).
    pub state: WorkspaceState,
    /// Bound HTTP base URL of the live router (for HTTP-observability tests,
    /// e.g. `GET /v1/daemon/orchestration/sessions/:id`).
    pub http_url: String,
    /// The daemon engine (wired BEFORE `create_router`, like boot does), so
    /// orchestration routes serve the SAME engine a test drives.
    pub engine: Arc<dyn nexus_orchestration::OrchestrationEngine>,
    /// The engine's session storage over `pool` — the daemon's real
    /// `orchestration_sessions` persistence (e.g. for failure records).
    pub session_storage: Arc<dyn graph_flow::SessionStorage>,
    http_task: tokio::task::JoinHandle<()>,
}

/// Wire a production-shaped orchestration engine into the daemon state
/// (mirrors `boot.rs`: `SqliteSessionStorage` over the daemon pool +
/// `GraphFlowEngine`). MUST run before `create_router` so the router's
/// `WorkspaceState` clone shares the engine slot.
async fn wire_orchestration_engine(
    state: &WorkspaceState,
    pool: &sqlx::SqlitePool,
) -> (
    Arc<dyn nexus_orchestration::OrchestrationEngine>,
    Arc<dyn graph_flow::SessionStorage>,
) {
    state
        .publish_creator_runtime_bundle()
        .await
        .expect("publish Creator-DB runtime bundle");
    let engine = state.engine().expect("engine after runtime bundle");
    let storage: Arc<dyn graph_flow::SessionStorage> = Arc::new(
        nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(Arc::new(pool.clone())),
    );
    (engine, storage)
}

#[allow(dead_code)]
impl LiveDaemon {
    /// Boot the daemon and write the hermetic HOME config.
    pub async fn start() -> Self {
        Self::start_with_optional_host(None).await
    }

    /// Boot the daemon with a deterministic `HostFacade` (Character run E2E).
    pub async fn start_with_agent_host(host: Arc<dyn HostFacade>) -> Self {
        Self::start_with_optional_host(Some(host)).await
    }

    /// Boot the daemon with a `HostFacade` AND an enabled default provider
    /// in the agent-host config (T3 settlement tests).
    ///
    /// A real daemon derives its supervisor binding provider from the first
    /// enabled `agent_host_config().providers` entry. The default fixture
    /// leaves `providers` empty, so the supervisor's internal insertion
    /// paths (auto-chain child enqueue) refuse presets that require prompt
    /// roles — e.g. `kb-extract` (persist stage) whose `acp_prompt` node
    /// has no `agent:` field and therefore requires the `default` role.
    /// This fixture mirrors the production config so the persist-stage
    /// child enqueue passes the N-9 binding gate.
    pub async fn start_with_agent_host_and_provider(host: Arc<dyn HostFacade>) -> Self {
        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind daemon port");
        let port = listener.local_addr().expect("local addr").port();
        let http_url = format!("http://127.0.0.1:{port}");

        let config_path = nexus_home.join("config.toml");
        let config = format!(
            "active_creator_id = \"test_creator\"\n\
             daemon_url = \"{http_url}\"\n\
             \n\
             [active_workspace_slug_by_creator]\n\
             \"test_creator\" = \"default\"\n"
        );
        std::fs::write(&config_path, config).expect("write config.toml");

        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        state.set_agent_host(host);
        // Mirror a production agent-host config: one enabled provider.
        state.set_agent_host_config(nexus_agent_host::config::AgentHostConfig {
            providers: vec![nexus_agent_host::config::ProviderConfig {
                id: "mock-provider".to_string(),
                protocol: "native_cli".to_string(),
                command: Some("mock".to_string()),
                args: vec![],
                env: std::collections::HashMap::new(),
                enabled: true,
            }],
            ..nexus_agent_host::config::AgentHostConfig::default()
        });
        let pool = state.pool().expect("pool").clone();
        test_utils::seed_test_creator_and_world(&pool).await;
        let (engine, session_storage) = wire_orchestration_engine(&state, &pool).await;

        let app = api::create_router(
            state.clone(),
            DaemonApiConfig::keyless().with_resolved_listen_addr(port, "127.0.0.1"),
        );
        let http_task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("daemon http serve");
        });

        Self {
            home: tmp,
            pool,
            state,
            http_url,
            engine,
            session_storage,
            http_task,
        }
    }

    /// Boot the daemon with a `HostFacade`, one enabled provider, AND a
    /// daemon-side tool dispatch (P3 restart matrix: converge/merge edge
    /// counting). The dispatch is wired BEFORE `publish_creator_runtime_bundle`
    /// so the bundle's engine wires it into every graph, exactly like boot.
    pub async fn start_with_host_provider_and_dispatch(
        host: Arc<dyn HostFacade>,
        dispatch: Arc<dyn nexus_orchestration::capability::DaemonToolDispatch>,
    ) -> Self {
        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind daemon port");
        let port = listener.local_addr().expect("local addr").port();
        let http_url = format!("http://127.0.0.1:{port}");

        let config_path = nexus_home.join("config.toml");
        let config = format!(
            "active_creator_id = \"test_creator\"\n\
             daemon_url = \"{http_url}\"\n\
             \n\
             [active_workspace_slug_by_creator]\n\
             \"test_creator\" = \"default\"\n"
        );
        std::fs::write(&config_path, config).expect("write config.toml");

        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        state.set_agent_host(host);
        state.set_agent_host_config(nexus_agent_host::config::AgentHostConfig {
            providers: vec![nexus_agent_host::config::ProviderConfig {
                id: "mock-provider".to_string(),
                protocol: "native_cli".to_string(),
                command: Some("mock".to_string()),
                args: vec![],
                env: std::collections::HashMap::new(),
                enabled: true,
            }],
            ..nexus_agent_host::config::AgentHostConfig::default()
        });
        state.set_daemon_tool_dispatch(dispatch.clone());
        let pool = state.pool().expect("pool").clone();
        test_utils::seed_test_creator_and_world(&pool).await;
        let (engine, session_storage) = wire_orchestration_engine(&state, &pool).await;

        let app = api::create_router(
            state.clone(),
            DaemonApiConfig::keyless().with_resolved_listen_addr(port, "127.0.0.1"),
        );
        let http_task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("daemon http serve");
        });

        Self {
            home: tmp,
            pool,
            state,
            http_url,
            engine,
            session_storage,
            http_task,
        }
    }

    /// Daemon-level restart over the SAME DB/HOME (P3 A7 restart matrix).
    ///
    /// Quiesces the current generation (aborts in-flight drives via their
    /// coordinator cancellation tokens, then the HTTP serve task), resets
    /// the published runtime bundle, and re-runs the PRODUCTION attach/boot
    /// path: `publish_creator_runtime_bundle` (fresh engine/coordinator/
    /// supervisor over the same pool + terminal-schedule reconciliation)
    /// followed by `run_boot_recovery` — the exact A7 recovery order real
    /// daemon boot uses. A fresh listener/router is bound and `daemon_url`
    /// in the hermetic `config.toml` is updated so CLI children resolve the
    /// new daemon.
    ///
    /// The bundled Host facade / agent-host config / tool dispatch are kept
    /// (the test owns them); every engine-side handle (runners, coordinator
    /// drives, prompt executor caches) is rebuilt from the durable records.
    pub async fn restart(&mut self) {
        // Quiesce the current generation BEFORE touching the bundle: the
        // drive-loop cancellation also releases blocked Host streams.
        if let Some(coord) = self.state.run_coordinator() {
            coord.abort_all_drives().await;
        }
        self.http_task.abort();
        // Let the old serve task die so its listener is fully released.
        tokio::task::yield_now().await;

        // Production attach path over the same DB/HOME.
        self.state.reset_runtime_bundle();
        self.state
            .publish_creator_runtime_bundle()
            .await
            .expect("daemon restart: republish Creator-DB runtime bundle");
        let engine = self.state.engine().expect("engine after restart");
        let sqlite = Arc::new(
            nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(Arc::new(
                self.pool.clone(),
            )),
        );
        nexus_daemon_runtime::boot::run_boot_recovery(
            &engine,
            &sqlite,
            self.state.run_coordinator().as_ref(),
            self.state.shutdown_notify(),
        )
        .await;

        // Rebind a fresh listener + router (the old listener was consumed by
        // the aborted serve task) and point CLI children at the new port.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("restart: bind daemon port");
        let port = listener.local_addr().expect("restart: local addr").port();
        self.http_url = format!("http://127.0.0.1:{port}");

        let config_path = self.home.path().join(".nexus42").join("config.toml");
        let content = std::fs::read_to_string(&config_path).expect("restart: read config");
        let mut lines: Vec<String> = content.lines().map(str::to_string).collect();
        for line in &mut lines {
            if line.starts_with("daemon_url") {
                *line = format!("daemon_url = \"{}\"", self.http_url);
            }
        }
        std::fs::write(&config_path, lines.join("\n")).expect("restart: rewrite config");

        let app = api::create_router(
            self.state.clone(),
            DaemonApiConfig::keyless().with_resolved_listen_addr(port, "127.0.0.1"),
        );
        self.http_task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("daemon http serve");
        });
        self.engine = self.state.engine().expect("engine after restart");
        self.session_storage = Arc::new(
            nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(Arc::new(
                self.pool.clone(),
            )),
        );
    }

    /// Resolve the durable `RunDescriptorV1`-equivalent DB layout: the
    /// workspace `state.db` path this herd serves.
    #[must_use]
    pub fn db_path(&self) -> std::path::PathBuf {
        nexus_home_layout::workspace_state_db_path(self.home.path(), "test_creator", "default")
    }

    /// Boot the daemon WITHOUT publishing the Creator-DB runtime bundle
    /// (N-2/N-2b lazy-attach path): the pool is open (test fixture) but no
    /// engine/coordinator/supervisor bundle exists until the test calls
    /// `state.ensure_creator_pool()`, exactly like a Tier-0 boot followed by
    /// Profile attach. The router is created after the bundle is published
    /// so routes observe the complete aggregate.
    pub async fn start_lazy_attach(host: Arc<dyn HostFacade>) -> Self {
        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind daemon port");
        let port = listener.local_addr().expect("local addr").port();
        let http_url = format!("http://127.0.0.1:{port}");

        let config_path = nexus_home.join("config.toml");
        let config = format!(
            "active_creator_id = \"test_creator\"\n\
             daemon_url = \"{http_url}\"\n\
             \n\
             [active_workspace_slug_by_creator]\n\
             \"test_creator\" = \"default\"\n"
        );
        std::fs::write(&config_path, config).expect("write config.toml");

        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        state.set_agent_host(host);
        let pool = state.pool().expect("pool").clone();
        test_utils::seed_test_creator_and_world(&pool).await;

        // No bundle yet — the lazy-attach path publishes it via
        // `ensure_creator_pool()` (the Profile-attach middleware seam).
        assert!(
            state.run_coordinator().is_none(),
            "lazy-attach fixture must start without a runtime bundle"
        );
        state
            .ensure_creator_pool()
            .await
            .expect("lazy attach publishes the runtime bundle");
        assert!(
            state.runtime_bundle().is_some(),
            "lazy attach must publish the immutable aggregate bundle"
        );

        let engine = state.engine().expect("engine after lazy attach");
        let session_storage: Arc<dyn graph_flow::SessionStorage> = Arc::new(
            nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(Arc::new(pool.clone())),
        );
        let app = api::create_router(
            state.clone(),
            DaemonApiConfig::keyless().with_resolved_listen_addr(port, "127.0.0.1"),
        );
        let http_task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("daemon http serve");
        });

        Self {
            home: tmp,
            pool,
            state,
            http_url,
            engine,
            session_storage,
            http_task,
        }
    }

    /// Boot the daemon through the NORMAL boot publication path (N-13):
    /// wire the boot slots (engine + pool-backed capability registry +
    /// coordinator + supervisor) exactly like `boot.rs`, then call
    /// `publish_boot_runtime_bundle()` BEFORE creating the router. This is
    /// the production boot aggregate — distinct from the lazy-attach
    /// bundle `start_lazy_attach` exercises.
    pub async fn start_normal_boot() -> Self {
        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind daemon port");
        let port = listener.local_addr().expect("local addr").port();
        let http_url = format!("http://127.0.0.1:{port}");

        let config_path = nexus_home.join("config.toml");
        let config = format!(
            "active_creator_id = \"test_creator\"\n\
             daemon_url = \"{http_url}\"\n\
             \n\
             [active_workspace_slug_by_creator]\n\
             \"test_creator\" = \"default\"\n"
        );
        std::fs::write(&config_path, config).expect("write config.toml");

        let state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        let pool = state.pool().expect("pool").clone();
        test_utils::seed_test_creator_and_world(&pool).await;

        // N-13: normal-boot wiring — mirrors `boot.rs`: durable storage +
        // workflow store over the Creator DB pool, a POOL-BACKED capability
        // registry (never a pool-less placeholder), the single run
        // coordinator, and the schedule supervisor. The boot bundle is
        // published from these slots before route readiness.
        let pool_arc = Arc::new(pool.clone());
        let sqlite_storage = Arc::new(
            nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(pool_arc.clone()),
        );
        let storage: Arc<dyn graph_flow::SessionStorage> = sqlite_storage.clone();
        let workflow_store: Arc<dyn nexus_orchestration::run_state::WorkflowStateStore> =
            sqlite_storage.clone();

        let deps = nexus_orchestration::capability::CapabilityRuntimeDeps {
            pool: Some(pool.clone()),
            prompt_executor: None,
            session_cancels: state.session_cancels(),
            daemon_tool_dispatch: None,
            cdn_config: None,
        workspace_executor: None,
        };
        let scan_dir = state
            .nexus_home()
            .parent()
            .expect("nexus home parent")
            .join("capabilities");
        let (registry, _outcome) =
            nexus_orchestration::capability::CapabilityRegistry::with_runtime_deps_and_user_caps(
                &deps, &scan_dir,
            );
        let holder =
            nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(registry));

        let mut engine =
            nexus_orchestration::GraphFlowEngine::new_with_storage_and_workflow_store_and_workspace(
                storage.clone(),
                workflow_store.clone(),
                holder.clone(),
                std::path::PathBuf::new(),
            );
        engine.set_nexus_home(state.nexus_home().clone());
        let engine_arc = Arc::new(engine);
        state.set_engine(engine_arc.clone() as Arc<dyn nexus_orchestration::OrchestrationEngine>);
        state.set_capability_registry(holder.clone());

        let coordinator = Arc::new(
            nexus_daemon_runtime::preset_run::WorkflowRunCoordinator::new(
                engine_arc.clone(),
                storage.clone(),
                pool_arc.clone(),
                state.session_cancels(),
            ),
        );
        state.set_run_coordinator(coordinator.clone());

        let mut supervisor_builder =
            nexus_orchestration::schedule::supervisor::ScheduleSupervisor::new_with_workspace(
                pool_arc.clone(),
                None,
            );
        if let Some(reg) = holder.get() {
            supervisor_builder = supervisor_builder.with_capability_registry(reg);
        }
        let supervisor = Arc::new(supervisor_builder);
        state.set_schedule_supervisor(supervisor.clone());
        // T3 (A3): the coordinator settles terminal runs through the
        // supervisor (mirrors boot.rs wiring).
        coordinator.set_schedule_supervisor(supervisor.clone());

        // N-13: the BOOT publisher (not the lazy-attach publisher) — the
        // production boot aggregate is published before the router exists.
        state
            .publish_boot_runtime_bundle()
            .await
            .expect("publish boot runtime bundle");

        let app = api::create_router(
            state.clone(),
            DaemonApiConfig::keyless().with_resolved_listen_addr(port, "127.0.0.1"),
        );
        let http_task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("daemon http serve");
        });

        Self {
            home: tmp,
            pool,
            state,
            http_url,
            engine: engine_arc.clone() as Arc<dyn nexus_orchestration::OrchestrationEngine>,
            session_storage: storage,
            http_task,
        }
    }

    async fn start_with_optional_host(host: Option<Arc<dyn HostFacade>>) -> Self {
        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;

        // Bind the HTTP listener BEFORE writing `daemon_url` into config.
        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind daemon port");
        let port = listener.local_addr().expect("local addr").port();
        let http_url = format!("http://127.0.0.1:{port}");

        // The daemon's active-scope reads AND the CLI's DaemonClient both
        // resolve from this same config file. `daemon_url` must be a
        // top-level key — written BEFORE the table header (appending after
        // `[active_workspace_slug_by_creator]` would make it a table key).
        let config_path = nexus_home.join("config.toml");
        let config = format!(
            "active_creator_id = \"test_creator\"\n\
             daemon_url = \"{http_url}\"\n\
             \n\
             [active_workspace_slug_by_creator]\n\
             \"test_creator\" = \"default\"\n"
        );
        std::fs::write(&config_path, config).expect("write config.toml");

        let mut state = WorkspaceState::new_for_testing(nexus_home, db_path, None).await;
        if let Some(host) = host {
            state.set_agent_host(host);
        }
        let pool = state.pool().expect("pool").clone();
        test_utils::seed_test_creator_and_world(&pool).await;
        let (engine, session_storage) = wire_orchestration_engine(&state, &pool).await;

        let app = api::create_router(
            state.clone(),
            DaemonApiConfig::keyless().with_resolved_listen_addr(port, "127.0.0.1"),
        );
        let http_task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("daemon http serve");
        });

        Self {
            home: tmp,
            pool,
            state,
            http_url,
            engine,
            session_storage,
            http_task,
        }
    }

    /// Boot the daemon with a real workspace directory on disk (needed by
    /// routes that read/write workspace files, e.g. the V1.72 outline
    /// canvas). The workspace root is `$HOME/workspace`.
    pub async fn start_with_workspace() -> Self {
        let (tmp, nexus_home, db_path) = test_utils::create_test_workspace().await;
        let workspace_dir = tmp.path().join("workspace");
        std::fs::create_dir_all(&workspace_dir).expect("create workspace dir");

        let listener = TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind daemon port");
        let port = listener.local_addr().expect("local addr").port();
        let http_url = format!("http://127.0.0.1:{port}");

        let config_path = nexus_home.join("config.toml");
        let config = format!(
            "active_creator_id = \"test_creator\"\n\
             daemon_url = \"{http_url}\"\n\
             \n\
             [active_workspace_slug_by_creator]\n\
             \"test_creator\" = \"default\"\n"
        );
        std::fs::write(&config_path, config).expect("write config.toml");

        let state = WorkspaceState::new_for_testing(
            nexus_home,
            db_path,
            Some(workspace_dir.to_string_lossy().to_string()),
        )
        .await;
        let pool = state.pool().expect("pool").clone();
        test_utils::seed_test_creator_and_world(&pool).await;
        let (engine, session_storage) = wire_orchestration_engine(&state, &pool).await;

        let app = api::create_router(
            state.clone(),
            DaemonApiConfig::keyless().with_resolved_listen_addr(port, "127.0.0.1"),
        );
        let http_task = tokio::spawn(async move {
            axum::serve(listener, app).await.expect("daemon http serve");
        });

        Self {
            home: tmp,
            pool,
            state,
            http_url,
            engine,
            session_storage,
            http_task,
        }
    }

    /// Run the real `nexus42` binary against the hermetic HOME.
    ///
    /// # Panics
    ///
    /// Panics if the binary cannot be spawned.
    pub async fn cli(&self, args: &[&str]) -> Output {
        self.cli_in_home(self.home.path(), args).await
    }

    /// Run the real `nexus42` binary with an explicit hermetic HOME.
    ///
    /// # Panics
    ///
    /// Panics if the binary cannot be spawned.
    pub async fn cli_in_home(&self, home: &Path, args: &[&str]) -> Output {
        tokio::process::Command::new(env!("CARGO_BIN_EXE_nexus42"))
            .args(args)
            .env("HOME", home)
            .env("RUST_LOG", "off")
            .output()
            .await
            .expect("spawn nexus42")
    }
}

impl Drop for LiveDaemon {
    fn drop(&mut self) {
        self.http_task.abort();
    }
}
