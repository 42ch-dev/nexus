//! The single execution owner (v1.190 P3-T1).
//!
//! [`CoreService::start_execution`] builds the one engine/effect owner for an
//! engine-owner core and returns an [`ExecutionHandle`]. The handle owns the
//! task set, the engine epoch and every cleanup path; a second
//! `start_execution` on the same service refuses rather than building a
//! second engine.
//!
//! This module deliberately excludes logging, HTTP binding, OS signals and
//! the SPA. A domain-only [`CoreService`] open starts none of these tasks —
//! execution requires an explicit `start_execution` under
//! [`CoreAccess::EngineOwner`].

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use nexus_contracts::{CoreCloseReport, CoreCloseReportState};
use nexus_orchestration::capability::{
    CapabilityRegistry, CapabilityRegistryHolder, CapabilityRuntimeDeps, DaemonToolDispatch,
    PromptExecutor, WorkspaceExecutor, WorkspaceStateProvider,
};
use nexus_orchestration::run_state::WorkflowStateStore;
use nexus_orchestration::storage::sqlite::SqliteSessionStorage;
use nexus_orchestration::{GraphFlowEngine, OrchestrationEngine};
use nexus_provider_ports::ProviderPort;
use sqlx::SqlitePool;

use crate::error::CoreResult;
use crate::service::{CoreAccess, CoreService};
use crate::execution::workflow::{
    ProviderCatalogPort, RunEventPort, WorkflowRunCoordinator,
};

/// Why an execution handle could not be established.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ExecutionOpenError {
    /// The service was not opened as [`CoreAccess::EngineOwner`]. Continuing
    /// would create a second effect owner beside the live one.
    #[error("execution requires an engine-owner core (opened as {0:?})")]
    NotEngineOwner(CoreAccess),
    /// An execution owner already exists for this service.
    ///
    /// The handle is single-owner per service: a duplicate start must return
    /// the established owner (or refuse) and never construct a second engine.
    #[error("an execution owner is already established for this core")]
    AlreadyOwned,
    /// The service is closing.
    #[error("core service is closing")]
    Closing,
}

/// Optional collaborators the daemon composes onto the execution handle.
///
/// Every field is a port or an already-constructed collaborator: the core
/// never reaches into a transport, a host manager or a global.
#[derive(Default)]
pub struct RunnerDeps {
    /// Prompt executor for `acp_prompt`-style capability nodes.
    pub prompt_executor: Option<Arc<dyn PromptExecutor>>,
    /// Daemon-side `nexus.*` tool dispatch for schedule ticks.
    pub daemon_tool_dispatch: Option<Arc<dyn DaemonToolDispatch>>,
    /// Production workspace executor for `workspace.open`/`workspace.commit`.
    pub workspace_executor: Option<Arc<dyn WorkspaceExecutor>>,
    /// The transport's live capability registry holder.
    ///
    /// The daemon owns the WASM singleton, the user-capability scan and the
    /// hot-reload watcher; the engine must read the SAME holder so a registry
    /// swap is visible to a running graph. When absent (core-only callers,
    /// tests) a bare builtin registry is constructed.
    pub capability_holder: Option<CapabilityRegistryHolder>,
    /// The transport's per-run cancellation map.
    ///
    /// The engine, the coordinator and every transport caller that fires a
    /// run's token must share ONE map. The daemon already publishes this map
    /// on its state and builds preset graphs with it, so it must be supplied
    /// here rather than re-created — a split map would let a token the
    /// engine registered be invisible to the transport's cancel path.
    pub session_cancels: Option<
        Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        >,
    >,
    /// Resolver for `_context.workspace.*` preset conditions, so conditional
    /// edges observe the same durable workspace authority the commit
    /// executor writes through.
    pub workspace_state_provider: Option<Arc<dyn WorkspaceStateProvider>>,
    /// Provider catalog port used to validate agent bindings before enqueue.
    pub provider_catalog: Option<Arc<dyn ProviderCatalogPort>>,
    /// Sanctioned default binding provider for explicit legacy starts (C-3),
    /// resolved by the transport from its own enabling config.
    pub binding_provider: Option<String>,
    /// Per-run live-ring registry for the run SSE surface.
    pub run_events: Option<Arc<dyn RunEventPort>>,
    /// Frozen workspace root written into every v1 run descriptor.
    pub workspace_root: Option<std::path::PathBuf>,
    /// Nexus home used to resolve directory presets for source identity.
    pub nexus_home: Option<std::path::PathBuf>,
    /// Cancels the bounded recovery re-drive when the transport shuts down.
    pub shutdown_notify: Option<Arc<tokio::sync::Notify>>,
}

/// The single owner of the execution task set and engine epoch.
///
/// Dropping the handle does not silently stop work — call
/// [`ExecutionHandle::close`] so the owned drives are aborted and the engine
/// is released in the documented order.
pub struct ExecutionHandle {
    /// The concrete engine. Retained so the transport can still drive the
    /// inherent (non-trait) surface the outer-graph builder and system-preset
    /// startup use; the trait view is [`Self::engine`].
    engine: Arc<GraphFlowEngine>,
    coordinator: Arc<WorkflowRunCoordinator>,
    /// The live capability registry the engine reads through. The transport
    /// keeps its own handle to the same holder (hot-reload swaps are visible
    /// to both), so the daemon can publish it to its runtime bundle.
    capability_holder: CapabilityRegistryHolder,
    session_cancels: Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    >,
    /// The engine epoch this owner was admitted with, read from the durable
    /// workspace gate at establishment. It identifies the ownership
    /// generation of every run this handle drives.
    engine_epoch: i64,
    closed: AtomicBool,
}

impl ExecutionHandle {
    /// The engine this handle owns, as the transport-neutral trait surface.
    #[must_use]
    pub fn engine(&self) -> Arc<dyn OrchestrationEngine> {
        Arc::clone(&self.engine) as Arc<dyn OrchestrationEngine>
    }

    /// The concrete engine, for callers that need the inherent surface
    /// (e.g. the transport's outer-graph builder / system-preset startup).
    #[must_use]
    pub fn engine_concrete(&self) -> Arc<GraphFlowEngine> {
        Arc::clone(&self.engine)
    }

    /// The owned run coordinator.
    #[must_use]
    pub fn coordinator(&self) -> Arc<WorkflowRunCoordinator> {
        Arc::clone(&self.coordinator)
    }

    /// The live capability registry holder the engine reads through.
    /// Identical to the transport's own handle when it supplied one.
    #[must_use]
    pub fn capability_holder(&self) -> CapabilityRegistryHolder {
        self.capability_holder.clone()
    }

    /// Attach the schedule supervisor once it exists.
    ///
    /// Ordering: the supervisor needs the coordinator's `ScheduleRunStarter`,
    /// so it is always constructed after the handle. Settlement of terminal
    /// runs routes through it from then on.
    pub fn set_schedule_supervisor(
        &self,
        supervisor: Arc<nexus_orchestration::schedule::supervisor::ScheduleSupervisor>,
    ) {
        self.coordinator.set_schedule_supervisor(supervisor);
    }

    /// The durable engine epoch this owner was admitted with.
    #[must_use]
    pub fn engine_epoch(&self) -> i64 {
        self.engine_epoch
    }

    /// The shared per-run cancellation map (also held by the engine).
    #[must_use]
    pub fn session_cancels(
        &self,
    ) -> Arc<
        std::sync::RwLock<std::collections::HashMap<String, tokio_util::sync::CancellationToken>>,
    > {
        Arc::clone(&self.session_cancels)
    }

    /// Whether this owner has already been closed.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    /// Abort every owned drive and settle the handle's cleanup in the
    /// documented order.
    ///
    /// Ordering: stop admitting new drives (the close flag), fire every
    /// owned cancellation token and join the drive loops, then release the
    /// handle's own references. Repeated calls report the already-closed
    /// state.
    ///
    /// # Errors
    /// Currently infallible: the report always describes a settled close.
    pub async fn close(&self) -> CoreResult<CoreCloseReport> {
        if self
            .closed
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(closed_report());
        }
        self.coordinator.abort_all_drives().await;
        Ok(closed_report())
    }

    /// Close without owning the report (used by [`CoreService::close`]).
    pub(crate) async fn shutdown(&self) {
        let _ = self.close().await;
    }
}

fn closed_report() -> CoreCloseReport {
    CoreCloseReport {
        state: CoreCloseReportState::Closed,
        cleanup_confirmed: true,
        pending_operations: vec![],
        reason: None,
    }
}

impl CoreService {
    /// Establish the single execution owner for this engine-owner core.
    ///
    /// Builds the engine over the core's own durable pool, wires the
    /// coordinator and recovery, and returns the handle that owns the task
    /// set, engine epoch and cleanup. The providers port is injected (P4
    /// supplies the implementation); it is required so a handle can never be
    /// created without a provider seam.
    ///
    /// # Errors
    /// Returns [`ExecutionOpenError::NotEngineOwner`] when the core was not
    /// opened under [`CoreAccess::EngineOwner`],
    /// [`ExecutionOpenError::AlreadyOwned`] when an owner already exists, and
    /// [`ExecutionOpenError::Closing`] when the service is closing.
    pub async fn start_execution(
        &self,
        _providers: Arc<dyn ProviderPort>,
        deps: RunnerDeps,
    ) -> Result<Arc<ExecutionHandle>, ExecutionOpenError> {
        self.ensure_open().map_err(|_| ExecutionOpenError::Closing)?;
        if self.inner.access != CoreAccess::EngineOwner {
            return Err(ExecutionOpenError::NotEngineOwner(self.inner.access));
        }
        // Single owner: the first successful start wins; a duplicate refuses.
        let mut slot = self
            .inner
            .execution
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(existing) = slot.as_ref() {
            if !existing.is_closed() {
                return Err(ExecutionOpenError::AlreadyOwned);
            }
        }
        let handle = self.build_execution(deps).await?;
        *slot = Some(Arc::clone(&handle));
        Ok(handle)
    }

    /// The established execution handle, if any.
    #[must_use]
    pub fn execution(&self) -> Option<Arc<ExecutionHandle>> {
        self.inner
            .execution
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clone()
    }

    async fn build_execution(
        &self,
        deps: RunnerDeps,
    ) -> Result<Arc<ExecutionHandle>, ExecutionOpenError> {
        let pool = self.inner.pool.clone();
        // The transport's map wins: the engine, the coordinator and the
        // transport's cancel path must all fire the same per-run token.
        let session_cancels: Arc<
            std::sync::RwLock<
                std::collections::HashMap<String, tokio_util::sync::CancellationToken>,
            >,
        > = deps
            .session_cancels
            .unwrap_or_else(|| Arc::new(std::sync::RwLock::new(std::collections::HashMap::new())));

        let sqlite_storage = Arc::new(SqliteSessionStorage::new(Arc::new(pool.clone())));
        let session_storage: Arc<dyn graph_flow::SessionStorage> = sqlite_storage.clone();

        // The daemon owns the live holder (WASM singleton, user-cap scan,
        // hot-reload watcher). A core-only caller gets a bare builtin
        // registry so the engine still has a capability surface.
        let capability_holder = match deps.capability_holder {
            Some(holder) => holder,
            None => {
                let capabilities = Arc::new(CapabilityRegistry::with_runtime_deps(
                    &CapabilityRuntimeDeps {
                        pool: Some(pool.clone()),
                        prompt_executor: deps.prompt_executor.clone(),
                        session_cancels: Arc::clone(&session_cancels),
                        daemon_tool_dispatch: deps.daemon_tool_dispatch.clone(),
                        cdn_config: None,
                        workspace_executor: deps.workspace_executor.clone(),
                    },
                ));
                let holder = CapabilityRegistryHolder::new();
                holder.swap(capabilities);
                holder
            }
        };

        let workflow_store: Arc<dyn WorkflowStateStore> = sqlite_storage.clone();
        let workspace_root = deps.workspace_root.clone().unwrap_or_default();
        let mut engine = GraphFlowEngine::new_with_storage_and_workflow_store_and_workspace(
            session_storage.clone(),
            workflow_store,
            capability_holder.clone(),
            workspace_root,
        );
        if let Some(dispatch) = &deps.daemon_tool_dispatch {
            engine.set_daemon_tool_dispatch(dispatch.clone());
        }
        if let Some(executor) = &deps.prompt_executor {
            engine.set_prompt_executor(executor.clone(), Arc::clone(&session_cancels));
        }
        if let Some(home) = &deps.nexus_home {
            engine.set_nexus_home(home.clone());
        }
        if let Some(provider) = &deps.workspace_state_provider {
            engine.set_workspace_state_provider(Arc::clone(provider));
        }
        let engine = Arc::new(engine);

        let mut coordinator = WorkflowRunCoordinator::new(
            Arc::clone(&engine),
            session_storage.clone(),
            Arc::new(pool.clone()),
            Arc::clone(&session_cancels),
        );
        if let Some(catalog) = deps.provider_catalog {
            coordinator = coordinator.with_provider_catalog(catalog);
        }
        if let Some(provider_id) = deps.binding_provider {
            coordinator = coordinator.with_binding_provider(provider_id);
        }
        if let Some(registry) = deps.run_events {
            coordinator = coordinator.with_run_events(registry);
        }
        let coordinator = Arc::new(coordinator);

        // A7 recovery: reconstruct runners from the frozen source identity and
        // re-drive only the eligible converge/merge class through this owner.
        let decisions = coordinator
            .recover_persisted(&sqlite_storage, deps.shutdown_notify)
            .await;
        for d in &decisions {
            tracing::info!(decision = ?d, "execution start: recovery re-drive decision");
        }

        let engine_epoch = read_engine_epoch(&pool).await;

        Ok(Arc::new(ExecutionHandle {
            engine,
            coordinator,
            capability_holder,
            session_cancels,
            engine_epoch,
            closed: AtomicBool::new(false),
        }))
    }
}

/// Read the durable engine epoch recorded by this connection's admission.
///
/// The writer protocol installs `nexus_engine_epoch()` per connection from
/// the admitting context, so this reports the epoch THIS owner was admitted
/// with — the ownership generation every run it drives belongs to.
async fn read_engine_epoch(pool: &SqlitePool) -> i64 {
    let epoch: Option<i64> = sqlx::query_scalar("SELECT nexus_engine_epoch()")
        .fetch_one(pool)
        .await
        .unwrap_or(None);
    epoch.unwrap_or(0)
}
