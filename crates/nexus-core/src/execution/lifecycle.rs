//! The single execution owner (v1.190 P3-T1).
//!
//! [`CoreService::start_execution`] builds the one engine/effect owner for an
//! engine-owner core and returns an [`ExecutionHandle`]. The handle owns the
//! task set, the engine epoch and every cleanup path; a second
//! `start_execution` for the SAME workspace DB refuses rather than building a
//! second engine — even from a different [`CoreService`] in this process.
//!
//! This module deliberately excludes logging, HTTP binding, OS signals and
//! the SPA. A domain-only [`CoreService`] open starts none of these tasks —
//! execution requires an explicit `start_execution` under
//! [`CoreAccess::EngineOwner`].

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex, PoisonError, Weak};

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
    /// An execution owner already exists for this workspace DB.
    ///
    /// The handle is single-owner per workspace DB, process-wide: a duplicate
    /// start must refuse (or return the established owner) and never construct
    /// a second engine — including a start from a different [`CoreService`]
    /// opened over the same file.
    #[error("an execution owner is already established for this workspace")]
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
    /// The durable workspace commit authority (P3-T2).
    ///
    /// Supplied by the transport (or a test) with the shared session manager
    /// bound to its active canonical root. When absent the handle's
    /// `commit_workspace` reports `NotFound` rather than committing through an
    /// unbound root.
    pub workspace_commit: Option<crate::execution::workspace::WorkspaceCommitAuthority>,
    /// Frozen workspace root written into every v1 run descriptor.
    pub workspace_root: Option<std::path::PathBuf>,
    /// Nexus home used to resolve directory presets for source identity.
    pub nexus_home: Option<std::path::PathBuf>,
    /// Cancels the bounded recovery re-drive when the transport shuts down.
    pub shutdown_notify: Option<Arc<tokio::sync::Notify>>,
    /// Optional barrier invoked once the owner is fully built but BEFORE
    /// [`CoreService::start_execution`] publishes it into the per-service slot.
    ///
    /// Production supplies `None`. It exists so a test can hold the exact
    /// window the C3 race lives in — `start_execution` awaiting its build
    /// while a concurrent `CoreService::close` runs — and force that
    /// interleaving deterministically rather than hoping for it. The build
    /// has already completed engine construction and A7 recovery when this
    /// fires, so a close racing here observes exactly the split the
    /// install-time double check must resolve.
    pub build_observer: Option<Arc<dyn ExecutionBuildObserver>>,
}

/// Observable point in [`CoreService::start_execution`]'s build phase.
///
/// A single await where the caller may hold the build open (see
/// [`RunnerDeps::build_observer`]). It is a diagnostic seam, not a
/// collaborator: it observes no state and its only power is to delay the
/// caller's own build.
#[async_trait::async_trait]
pub trait ExecutionBuildObserver: Send + Sync {
    /// Called once, after the build settles and before the install.
    async fn built(&self);
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
    /// The durable workspace commit authority this owner commits through.
    ///
    /// Bound at establishment so `commit_workspace` cannot be routed to a
    /// root the owner was not admitted for.
    workspace_commit: Option<crate::execution::workspace::WorkspaceCommitAuthority>,
    /// The engine epoch this owner was admitted with, read from the durable
    /// workspace gate at establishment. It identifies the ownership
    /// generation of every run this handle drives.
    engine_epoch: i64,
    /// Set when `close()` begins: drives admission is fenced (C2) and
    /// repeated closes are no-ops. Replaced by `settled` when the drain
    /// finishes.
    closing: AtomicBool,
    /// Set when `close()` has finished: every owned drive was cancelled and
    /// joined (C1). Only a SETTLED owner may be superseded in the registry.
    settled: AtomicBool,
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

    /// Whether this owner's close has begun: drives admission is fenced
    /// and repeated closes are no-ops.
    #[must_use]
    pub fn is_closed(&self) -> bool {
        self.closing.load(Ordering::SeqCst)
    }

    /// Whether this owner's close has COMPLETED: every owned drive was
    /// cancelled and joined (C1). Only a settled owner may be superseded in
    /// the per-DB owner registry — a closing-but-draining owner keeps the
    /// fence until its drives are gone.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.settled.load(Ordering::SeqCst)
    }

    /// Abort every owned drive and settle the handle's cleanup in the
    /// documented order.
    ///
    /// Ordering: fence new drive admission (C2), fire every owned
    /// cancellation token and join the drive loops, THEN mark the handle
    /// settled (C1) so the per-DB registry admits a replacement only after
    /// every owned drive has joined. Repeated calls report the
    /// already-closed state.
    ///
    /// # Errors
    /// Currently infallible: the report always describes a settled close.
    pub async fn close(&self) -> CoreResult<CoreCloseReport> {
        if self
            .closing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            // Another close owns the drain (C2). It reports `confirmed` only
            // AFTER the drives have joined — wait for that settle rather than
            // returning a false confirmed report from a close that did
            // nothing.
            while !self.is_settled() {
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            return Ok(closed_report());
        }
        self.coordinator.begin_shutdown();
        self.coordinator.abort_all_drives().await;
        self.settled.store(true, Ordering::SeqCst);
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

/// The registered execution owner for one workspace DB.
///
/// The `Building` reservation is what makes the claim race-safe without
/// holding a lock across the build await: a `std::sync::MutexGuard` is not
/// `Send`, and `build_execution` awaits engine construction and recovery. A
/// reservation installed BEFORE the await serializes concurrent starts, so
/// the loser refuses immediately and never builds a second engine.
enum OwnerSlot {
    /// A `start_execution` holds the slot and is building. Transient.
    Building,
    /// An established owner. `Weak` so the registry never keeps a dropped
    /// handle — or its engine — alive.
    Established(Weak<ExecutionHandle>),
}

/// The established execution owner per workspace DB, process-wide.
///
/// The per-service slot alone cannot fence this: `CoreService::open` under
/// [`CoreAccess::EngineOwner`] deliberately JOINS this process's retained
/// engine admission (the daemon's transport pool and its execution core
/// co-host one DB), so a second core over the same file would find its own
/// per-service slot empty and build a second engine. The OS `engine.lock`
/// fences cross-PROCESS owners only — same-process contenders share the
/// retained guard. This registry is the in-process fence.
static OWNERS: LazyLock<Mutex<HashMap<PathBuf, OwnerSlot>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Normalize a workspace DB path so two opens of the same file collide.
///
/// Canonicalization matters (e.g. macOS `/var` → `/private/var`), but the
/// resolved path is authoritative even when it cannot be canonicalized.
fn owner_key(db_path: &Path) -> PathBuf {
    std::fs::canonicalize(db_path).unwrap_or_else(|_| db_path.to_path_buf())
}

fn owners_lock() -> std::sync::MutexGuard<'static, HashMap<PathBuf, OwnerSlot>> {
    OWNERS.lock().unwrap_or_else(PoisonError::into_inner)
}

/// A held-but-not-yet-installed owner reservation.
///
/// RAII: dropping the guard without [`Self::install`] frees the slot, so a
/// failed — or panicking — build never fences its own workspace DB.
struct OwnerReservation {
    key: PathBuf,
    armed: bool,
}

impl OwnerReservation {
    /// Reserve the single owner slot for `db_path`, or refuse when a live
    /// owner already holds it.
    fn claim(db_path: &Path) -> Result<Self, ExecutionOpenError> {
        let key = owner_key(db_path);
        let mut owners = owners_lock();
        // A dropped or closed owner no longer fences its DB.
        owners.retain(|_, slot| match slot {
            OwnerSlot::Building => true,
            OwnerSlot::Established(weak) => weak.upgrade().is_some_and(|h| !h.is_settled()),
        });
        if owners.contains_key(&key) {
            return Err(ExecutionOpenError::AlreadyOwned);
        }
        owners.insert(key.clone(), OwnerSlot::Building);
        Ok(Self { key, armed: true })
    }

    /// Publish `handle` as the established owner and stop owning the slot.
    fn install(mut self, handle: &Arc<ExecutionHandle>) {
        let mut owners = owners_lock();
        owners.insert(
            self.key.clone(),
            OwnerSlot::Established(Arc::downgrade(handle)),
        );
        self.armed = false;
    }
}

impl Drop for OwnerReservation {
    fn drop(&mut self) {
        if self.armed {
            let mut owners = owners_lock();
            if matches!(owners.get(&self.key), Some(OwnerSlot::Building)) {
                owners.remove(&self.key);
            }
        }
    }
}

/// Release the registry slot for `db_path` when it still names `handle`.
///
/// `Arc::ptr_eq` guards against a stale release evicting a NEWER owner that
/// claimed the same DB after this one closed. This is the eager path: the
/// next [`OwnerReservation::claim`] also prunes a closed handle's slot, so a
/// missed release can never strand a DB.
pub(crate) fn release_owner_slot(db_path: &Path, handle: &Arc<ExecutionHandle>) {
    let key = owner_key(db_path);
    let mut owners = owners_lock();
    let names_this_handle = matches!(
        owners.get(&key),
        Some(OwnerSlot::Established(weak))
            if weak.upgrade().is_some_and(|current| Arc::ptr_eq(&current, handle))
    );
    if names_this_handle {
        owners.remove(&key);
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
        mut deps: RunnerDeps,
    ) -> Result<Arc<ExecutionHandle>, ExecutionOpenError> {
        self.ensure_open().map_err(|_| ExecutionOpenError::Closing)?;
        if self.inner.access != CoreAccess::EngineOwner {
            return Err(ExecutionOpenError::NotEngineOwner(self.inner.access));
        }
        // Single owner. The fence cannot be the per-service slot alone:
        // `CoreService::open` under `EngineOwner` deliberately JOINS this
        // process's retained engine admission (the daemon's transport pool and
        // its execution core co-host one DB file), so a second core over the
        // same file would find its OWN slot empty and build a second engine.
        // The OS `engine.lock` fences cross-PROCESS owners only — same-process
        // contenders share the retained guard.
        //
        // The claim is therefore the process-wide, per-DB registry, reserved
        // BEFORE the build await: `build_execution` awaits engine construction
        // and recovery, and a `std::sync::MutexGuard` is not `Send`, so the
        // reservation is a registry entry rather than a held lock. It
        // serializes competing starts (the loser refuses immediately, never
        // building a second engine) and is dropped on any failure, so a failed
        // build cannot fence its own DB. A closed or dropped owner frees its
        // slot on the next claim.
        let reservation = OwnerReservation::claim(&self.inner.db_path)?;
        // The build-phase barrier is consumed HERE, not by the builder: it
        // gates the install, so it must outlive `build_execution`.
        let build_observer = deps.build_observer.take();
        let handle = self.build_execution(deps).await?;
        // C3 barrier (diagnostic seam): the build is complete (engine +
        // recovery) but nothing is published yet. A test holds this exact
        // window to force a close to arrive mid-start; production supplies
        // no observer and skips straight to the install below.
        if let Some(observer) = build_observer {
            observer.built().await;
        }
        // C3: install-time double check. Close sets `closing` BEFORE it takes
        // the per-service slot, and this check+install is atomic under the
        // SAME slot mutex — so either close observes the installed handle
        // and settles it, or the build abandons it. Neither path leaves an
        // owner behind after close returns.
        let installing = {
            let mut slot = self
                .inner
                .execution
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            if self.ensure_open().is_err() {
                false
            } else {
                *slot = Some(Arc::clone(&handle));
                true
            }
        };
        if !installing {
            // The service began closing while this build ran. The build ran
            // recovery (which spawns drives), so settle the freshly built
            // owner before abandoning it — a dropped handle must not leak
            // live drives.
            handle.shutdown().await;
            return Err(ExecutionOpenError::Closing);
        }
        reservation.install(&handle);
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

    /// Retire the established execution owner so a fresh generation can be
    /// established over the same DB (the daemon-level restart seam, A7).
    ///
    /// A restart republishes a FRESH engine/coordinator over the SAME Creator
    /// DB, so the prior owner must be settled and its per-DB fence released
    /// before the new generation claims it. The handle is settled FIRST and
    /// the slot released after, so the fence is never dropped while a build
    /// that supersedes this owner is still in flight.
    ///
    /// A service that never established an owner (domain-only cores) is a
    /// no-op.
    pub async fn retire_execution(&self) {
        let handle = self
            .inner
            .execution
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .take();
        if let Some(handle) = handle {
            handle.shutdown().await;
            release_owner_slot(&self.inner.db_path, &handle);
        }
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

        let workspace_commit = deps.workspace_commit;
        let engine_epoch = read_engine_epoch(&pool).await;

        Ok(Arc::new(ExecutionHandle {
            engine,
            coordinator,
            capability_holder,
            session_cancels,
            workspace_commit,
            engine_epoch,
            closing: AtomicBool::new(false),
            settled: AtomicBool::new(false),
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
