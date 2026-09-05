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
fn wire_orchestration_engine(
    state: &mut WorkspaceState,
    pool: &sqlx::SqlitePool,
) -> (
    Arc<dyn nexus_orchestration::OrchestrationEngine>,
    Arc<dyn graph_flow::SessionStorage>,
) {
    let storage: Arc<dyn graph_flow::SessionStorage> = Arc::new(
        nexus_orchestration::storage::sqlite::SqliteSessionStorage::new(Arc::new(pool.clone())),
    );
    let holder = nexus_orchestration::CapabilityRegistryHolder::with_registry(Arc::new(
        nexus_orchestration::CapabilityRegistry::with_builtins(),
    ));
    let engine = Arc::new(nexus_orchestration::GraphFlowEngine::new_with_storage(
        storage.clone(),
        holder,
    ));
    state.set_engine(engine.clone() as Arc<dyn nexus_orchestration::OrchestrationEngine>);
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
        let (engine, session_storage) = wire_orchestration_engine(&mut state, &pool);

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

        let mut state = WorkspaceState::new_for_testing(
            nexus_home,
            db_path,
            Some(workspace_dir.to_string_lossy().to_string()),
        )
        .await;
        let pool = state.pool().expect("pool").clone();
        test_utils::seed_test_creator_and_world(&pool).await;
        let (engine, session_storage) = wire_orchestration_engine(&mut state, &pool);

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
