//! `CoreService` lifecycle and public API surface.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use nexus_contracts::{
    CoreChangesRequest, CoreChangesResponse, CoreCloseReport, WorldKbCandidatesResponse,
    WorldKbGraphResponse, WorldKbPatchEntityRequest, WorldKbPatchEntityResponse,
};
use nexus_home_layout::active_context::{
    read_active_creator_id, read_active_workspace_slug, try_resolve_state_db_path,
    CliConfigSnapshot,
};
use nexus_local_db::open_pool_read_only;
use nexus_local_db::writer_protocol::{
    init_engine_pool, init_guarded_pool, release_retained_writer_guards, GuardedPool,
    GuardedPoolOptions,
};
use sqlx::SqlitePool;

use crate::actor_fence::ActorFenceTable;
use crate::changes::read_changes;
use crate::error::{local_db_err, CoreError, CoreResult};
use crate::principal::Principal;
use crate::world_kb::{candidates, graph, patch};

/// Refuse an open whose config-resolved database is not the caller's bound one.
///
/// The host's creator-DB slot and this open each resolve the workspace path, so
/// a selection that moved between them would silently point this service at a
/// *second* database while the caller still treats it as the first. Comparing
/// the two paths here — before any pool is initialized or joined — keeps a
/// fail-closed error from becoming wrong-database reads and writes.
///
/// The error is [`CoreError::AuthRequired`], matching what
/// [`CoreService::verify_selected_context`] reports for a selection that moved
/// *after* open, so the status a caller sees does not depend on whether the
/// drift was observed before or after the pool was taken. The paths are logged
/// rather than carried on the wire.
fn verify_expected_binding(expected: &Path, resolved: &Path) -> CoreResult<()> {
    if expected == resolved {
        return Ok(());
    }
    tracing::warn!(
        expected = %expected.display(),
        resolved = %resolved.display(),
        "workspace database resolved from config is not the one bound by the caller; \
         refusing to open a second database"
    );
    Err(CoreError::AuthRequired)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreAccess {
    ReadOnly,
    DirectWriter,
    EngineOwner,
}

#[derive(Debug, Clone)]
pub struct CoreOpenOptions {
    pub user_home: PathBuf,
    pub access: CoreAccess,
}

pub struct CoreInner {
    pub(crate) pool: SqlitePool,
    /// The workspace `state.db` this service serves. The execution owner
    /// registry is keyed by it, so the single-owner fence spans every
    /// `CoreService` opened over the same file in this process.
    pub(crate) db_path: PathBuf,
    /// The admitted pool handle this open holds.
    ///
    /// Behind a mutex so a CONFIRMED [`CoreService::close`] can give it up:
    /// a closed service value that is still referenced must not keep the
    /// pooled connection — or the OS admission lock its `WorkspaceWriterGuard`
    /// carries — alive after the close reported the service released.
    guarded: Mutex<Option<GuardedPool>>,
    /// Whether THIS open CREATED the process's engine admission over
    /// `db_path`, rather than joining the one a live in-process engine owner
    /// already holds.
    ///
    /// Only the creator releases the admission on a confirmed close: a
    /// co-hosted joiner (the daemon's transport pool and its execution core
    /// over one DB) must never release an admission its creator still owns.
    owns_engine_admission: bool,
    /// Active creator this service was opened against (open-scoped).
    pub(crate) nexus_home: PathBuf,
    creator_id: String,
    workspace_slug: String,
    /// Open-scoped generation: minted as `1` at [`CoreService::open`] and
    /// never bumped for the service lifetime. It only proves a principal was
    /// minted from THIS open — staleness of the on-disk selection is
    /// enforced solely by `verify_selected_context`'s disk re-read.
    /// Any future check MUST NOT test the generation alone.
    generation: AtomicU64,
    pub(crate) access: CoreAccess,
    closing: AtomicBool,
    /// Set by the FIRST [`CoreService::close`]: the ONE retained close drain
    /// was started.
    ///
    /// The drain runs on its own task (see [`CoreService::close`]) so an
    /// interrupted caller cannot abandon a half-drained service, and a later
    /// close observes the real settlement instead of reporting a close that
    /// never happened.
    close_drain_started: AtomicBool,
    /// The retained drain's REAL result — `None` until it finishes.
    close_report: Mutex<Option<CoreCloseReport>>,
    /// Notified when [`Self::close_report`] is published.
    close_settled: tokio::sync::Notify,
    /// Per-Character activity/transition fences (v1.190 P2-T1), Host-free
    /// and separate from any process session registry.
    pub(crate) character_fences: ActorFenceTable,
    /// Established-owner slot for the Host authority (P4-T2): at most one
    /// `open_host` manager per open service — a second start is a typed busy
    /// rejection, never a second engine. Reset only by a confirmed close.
    #[cfg(feature = "provider-host")]
    pub(crate) host_authority_established: Mutex<bool>,
    /// The canonical creative root this ENGINE-OWNER admission is PINNED to.
    ///
    /// Resolved ONCE here at open, from the selected workspace's own
    /// `meta.json`, so the native boot's Host probe boundary and every
    /// workspace port the hosted factory composes (`RunnerDeps.workspace_root`,
    /// the commit executor, the `_context.workspace.*` state provider, the
    /// durable commit authority and its startup-recovery root filter) all
    /// derive from ONE immutable value. A metadata write after open can then
    /// never make the probed root and the execution/commit authority disagree.
    ///
    /// The whole OUTCOME is stored, not just the path: a metadata fault must
    /// stay an `internal` refusal for the hosted factory instead of silently
    /// degrading into the `uninitialized` "no owner" answer. `Ok(None)` means
    /// the selected workspace registers no usable canonical root, so no probe
    /// owner and no execution owner are admissible.
    ///
    /// Only an engine-owner open resolves it; every other access mode pins "no
    /// root" because it composes no execution owner.
    #[cfg(feature = "execution")]
    pub(crate) admission_root: CoreResult<Option<PathBuf>>,
    /// The execution owner slot for THIS service (v1.190 P3-T1). Empty until
    /// `start_execution` succeeds.
    ///
    /// The authoritative single-owner fence is NOT this slot — it is the
    /// process-wide registry in `execution::lifecycle`, keyed by the
    /// workspace DB, because `CoreService::open` under `EngineOwner` joins
    /// this process's retained engine admission and would otherwise let a
    /// second core over the same file build a second engine.
    #[cfg(feature = "execution")]
    pub(crate) execution: std::sync::Mutex<Option<Arc<crate::execution::ExecutionHandle>>>,
    /// Per-service start fence (v1.195 P1-T2): the retained start task holds a
    /// SHARED guard for its whole start, and the close drain takes the EXCLUSIVE
    /// guard before it takes the execution slot and releases the pool, the
    /// writer admission and the per-DB owner fence.
    ///
    /// The install-time double check alone cannot close this window: a close
    /// that lands mid-build finds the per-service slot EMPTY, so it used to
    /// release the pool/admission and publish `cleanup_confirmed` while the
    /// registered build (and the recovery pass it ran) was still in flight —
    /// and the next open over the same home could re-admit beside it. The fence
    /// makes the drain WAIT for every registered start to settle (finish or
    /// abandon) first, so a confirmed report can never precede a registered
    /// builder's shutdown.
    ///
    /// The lock is behind an `Arc` because it uses OWNED guards: the retained
    /// start task (see `CoreService::start_execution`) holds the shared half for
    /// its whole duration — reservation, build, recovery, install, and the
    /// settlement of an owner whose caller went away.
    ///
    /// Cancellation-safe by construction: the guard is held by that retained
    /// task, so a caller that is cancelled stops waiting but cannot interrupt
    /// the start, and can neither strand the close nor release the DB under
    /// drives that are still running.
    #[cfg(feature = "execution")]
    pub(crate) start_fence: Arc<tokio::sync::RwLock<()>>,
}

/// Cloneable handle: `inner` is already shared, so a clone is the same open
/// service (P4-T2: `open_host` hands a clone to the `HostHandle`).
#[derive(Clone)]
pub struct CoreService {
    pub(crate) inner: Arc<CoreInner>,
}

impl CoreService {
    /// Open a [`CoreService`] against the active workspace state DB under
    /// `options.user_home`.
    ///
    /// The active creator and workspace are resolved from the nexus home
    /// config, then the pool is opened in the requested [`CoreAccess`] mode:
    /// read-only opens require the DB to exist, `DirectWriter` takes the
    /// guarded writer pool, and `EngineOwner` joins a live engine's pool or
    /// initializes one.
    ///
    /// Use [`CoreService::open_with_expected_binding`] when the caller already
    /// holds a bound workspace database.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when no active creator/workspace is
    /// configured, [`CoreError::Uninitialized`] when the state DB path cannot
    /// be resolved (or is missing under read-only access), and the mapped
    /// pool-initialization error otherwise.
    pub async fn open(options: CoreOpenOptions) -> CoreResult<Self> {
        Self::open_impl(options, None).await
    }

    /// Open a [`CoreService`], requiring the resolved workspace database to be
    /// the `expected` one the caller has already bound.
    ///
    /// The daemon's creator-DB slot resolves the workspace path independently of
    /// this open, so the two can disagree when the active selection moves. Such
    /// an open is refused with [`CoreError::AuthRequired`] **before** any pool is
    /// initialized or joined, instead of silently opening a second database and
    /// serving the selected identity out of it.
    ///
    /// # Errors
    /// As [`CoreService::open`], plus [`CoreError::AuthRequired`] when the
    /// resolved path is not `expected`.
    pub async fn open_with_expected_binding(
        options: CoreOpenOptions,
        expected: &Path,
    ) -> CoreResult<Self> {
        Self::open_impl(options, Some(expected)).await
    }

    async fn open_impl(options: CoreOpenOptions, expected: Option<&Path>) -> CoreResult<Self> {
        let nexus_home = nexus_home_layout::nexus_root_from_home(&options.user_home);
        let cfg = CliConfigSnapshot::load(&nexus_home).map_err(|e| CoreError::Internal {
            category: format!("config_load: {e}"),
        })?;
        let creator_id = cfg
            .active_creator_id
            .clone()
            .ok_or(CoreError::AuthRequired)?;
        let workspace_slug = cfg.workspace_slug_for_creator(&creator_id);
        if workspace_slug.trim().is_empty() {
            return Err(CoreError::AuthRequired);
        }
        let db_path = try_resolve_state_db_path(&options.user_home, &nexus_home)
            .ok_or(CoreError::Uninitialized)?;
        if let Some(expected) = expected {
            verify_expected_binding(expected, &db_path)?;
        }
        if !db_path.exists() && options.access == CoreAccess::ReadOnly {
            return Err(CoreError::Uninitialized);
        }

        // Pin the admission's canonical creative root HERE, once, before any
        // pool or owner exists: the native boot's Host probe boundary and the
        // hosted factory's workspace ports both bind this exact value, so a
        // selected-metadata write that lands between them cannot publish a
        // Host bound to one root beside execution authority bound to another.
        #[cfg(feature = "execution")]
        let admission_root = if options.access == CoreAccess::EngineOwner {
            crate::works::canonical_selected_workspace_root(
                &nexus_home,
                &creator_id,
                &workspace_slug,
            )
        } else {
            Ok(None)
        };

        let mut owns_engine_admission = false;
        let (pool, guarded) = match options.access {
            CoreAccess::ReadOnly => {
                let pool = open_pool_read_only(&db_path).await.map_err(local_db_err)?;
                (pool, None)
            }
            CoreAccess::DirectWriter => {
                let guarded = init_guarded_pool(&db_path, &creator_id)
                    .await
                    .map_err(local_db_err)?;
                let pool = guarded.clone_pool();
                (pool, Some(guarded))
            }
            CoreAccess::EngineOwner => {
                let options = GuardedPoolOptions::default();
                let joined = nexus_local_db::writer_protocol::join_live_engine_pool(
                    &db_path, options,
                )
                .await
                .map_err(local_db_err)?;
                owns_engine_admission = joined.is_none();
                let guarded = match joined {
                    Some(guarded) => guarded,
                    None => init_engine_pool(&db_path, &creator_id, options)
                        .await
                        .map_err(local_db_err)?,
                };
                let pool = guarded.clone_pool();
                (pool, Some(guarded))
            }
        };

        Ok(Self {
            inner: Arc::new(CoreInner {
                pool,
                db_path: db_path.clone(),
                guarded: Mutex::new(guarded),
                owns_engine_admission,
                nexus_home,
                creator_id,
                workspace_slug,
                generation: AtomicU64::new(1),
                access: options.access,
                closing: AtomicBool::new(false),
                close_drain_started: AtomicBool::new(false),
                close_report: Mutex::new(None),
                close_settled: tokio::sync::Notify::new(),
                #[cfg(feature = "provider-host")]
                host_authority_established: Mutex::new(false),
                #[cfg(feature = "execution")]
                admission_root,
                character_fences: ActorFenceTable::new(&db_path),
                #[cfg(feature = "execution")]
                execution: std::sync::Mutex::new(None),
                #[cfg(feature = "execution")]
                start_fence: Arc::new(tokio::sync::RwLock::new(())),
            }),
        })
    }

    /// The nexus root this service was opened against (`<user_home>/.nexus42`).
    /// Core family modules resolve bearer file paths (SOUL.md, long-term
    /// memory) through it; the field stays private to this module.
    #[must_use]
    /// The Creator DB pool, for crate-external integration tests only.
    ///
    /// Architecture §Cohorts keeps the pool out of the product API: this
    /// accessor is compiled solely for test builds (`test-hooks` / `cfg(test)`)
    /// and exists so the `crates/nexus-core/tests/*` acceptance targets can
    /// seed and inspect the same guarded store the service writes. It is not a
    /// second business truth and must not acquire production callers.
    #[cfg(any(test, feature = "test-hooks"))]
    pub fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }

    #[must_use]
    pub fn nexus_home(&self) -> &std::path::Path {
        &self.inner.nexus_home
    }

    /// The canonical creative root this engine-owner admission is pinned to.
    ///
    /// The value resolved ONCE at open from the selected workspace's own
    /// `meta.json`, exposed so the native boot binds its Host probe boundary to
    /// the very root the core's hosted factory will compose its workspace ports
    /// from — instead of resolving the selection a second time and racing it.
    ///
    /// `None` means this admission registers no usable canonical root (absent,
    /// blank, gone, unreadable, or a non-engine-owner open), so no probe owner
    /// is admissible: the Host then keeps its pre-existing open boundary and
    /// marks every selected candidate `probe_context_unavailable`.
    #[must_use]
    #[cfg(feature = "execution")]
    pub fn admission_creative_root(&self) -> Option<&Path> {
        self.inner
            .admission_root
            .as_ref()
            .ok()
            .and_then(Option::as_deref)
    }

    pub(crate) fn ensure_open(&self) -> CoreResult<()> {
        if self.inner.closing.load(Ordering::SeqCst) {
            return Err(CoreError::Closing);
        }
        Ok(())
    }

    /// Read-only probe: whether this service has begun closing.
    ///
    /// The same predicate [`Self::ensure_open`] enforces, exposed so a caller
    /// can OBSERVE the close barrier instead of inferring it from a later
    /// refusal. Used by the concurrent close/start contract to force a close
    /// into the mid-build window deterministically.
    #[must_use]
    pub fn is_closing(&self) -> bool {
        self.inner.closing.load(Ordering::SeqCst)
    }

    /// Re-read the on-disk active creator/workspace and require it to still
    /// match the context this service was opened against.
    ///
    /// Invariant (QC v1.190 P0): this disk re-read is the ONLY staleness
    /// enforcement — [`CoreInner::generation`] is open-scoped and never
    /// bumps, so a principal minted from this open always passes the
    /// generation check. Selection changes after open are caught here, not
    /// by generation invalidation.
    pub(crate) fn verify_selected_context(&self) -> CoreResult<()> {
        if read_active_creator_id(&self.inner.nexus_home).as_deref()
            != Some(self.inner.creator_id.as_str())
        {
            return Err(CoreError::AuthRequired);
        }
        let slug = read_active_workspace_slug(&self.inner.nexus_home, &self.inner.creator_id);
        if slug.as_deref() != Some(self.inner.workspace_slug.as_str()) {
            return Err(CoreError::AuthRequired);
        }
        Ok(())
    }

    pub(crate) fn verify_principal(&self, principal: &Principal) -> CoreResult<()> {
        self.ensure_open()?;
        if !principal.verify_generation(self.inner.generation.load(Ordering::SeqCst)) {
            return Err(CoreError::AuthRequired);
        }
        if principal.creator_id() != self.inner.creator_id.as_str()
            || principal.workspace_slug() != self.inner.workspace_slug.as_str()
        {
            return Err(CoreError::AuthRequired);
        }
        self.verify_selected_context()?;
        Ok(())
    }

    /// Snapshot the [`Principal`] for the currently selected creator and
    /// workspace.
    ///
    /// The returned future is immediately ready: the check only reads
    /// process-local state (closing flag, config files, in-memory fields), so
    /// there is no await point.
    ///
    /// # Errors
    /// Returns [`CoreError::Closing`] when the service is closing and
    /// [`CoreError::AuthRequired`] when the on-disk active creator/workspace
    /// no longer matches the context this service was opened against.
    pub fn active_principal(&self) -> impl Future<Output = CoreResult<Principal>> {
        std::future::ready(self.active_principal_snapshot())
    }

    fn active_principal_snapshot(&self) -> CoreResult<Principal> {
        self.ensure_open()?;
        self.verify_selected_context()?;
        Ok(Principal::new(
            self.inner.creator_id.clone(),
            self.inner.workspace_slug.clone(),
            self.inner.generation.load(Ordering::SeqCst),
        ))
    }

    /// Project the entity graph for a World.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal or the on-disk
    /// selection fails verification, and the storage error mapped by the
    /// graph reader otherwise (missing world → [`CoreError::NotFound`],
    /// foreign world → [`CoreError::Forbidden`]).
    pub async fn world_kb_graph(
        &self,
        principal: &Principal,
        world_id: String,
        include_suggested: bool,
    ) -> CoreResult<WorldKbGraphResponse> {
        self.verify_principal(principal)?;
        graph::get_graph(
            &self.inner.pool,
            self.inner.access,
            principal.creator_id(),
            &world_id,
            include_suggested,
        )
        .await
    }

    /// Apply an OCC-guarded entity patch and outbox the change event.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::Forbidden`] under read-only access,
    /// [`CoreError::WorldKbValidation`] for invalid patches,
    /// [`CoreError::ActorConflict`] `invalid_world_sheet` when a linked
    /// `WorldSheet` would lose its eligibility, and [`CoreError::WorldKbConflict`]
    /// carrying the committed revision when the expected version is stale.
    #[allow(clippy::collection_is_never_read, clippy::significant_drop_tightening)] // held-only: the governance leases fence this call (durable §4.3) and are never read
    pub async fn patch_world_kb_entity(
        &self,
        principal: &Principal,
        world_id: String,
        request: WorldKbPatchEntityRequest,
    ) -> CoreResult<WorldKbPatchEntityResponse> {
        self.verify_principal(principal)?;
        if self.inner.access == CoreAccess::ReadOnly {
            return Err(CoreError::Forbidden {
                resource: "world_kb_patch: read-only core access".to_string(),
            });
        }
        // v1.191 P1 T7 (durable §4.3): a World governance edit takes the World
        // **exclusive** knowledge lease before the authoring transaction opens
        // `BEGIN IMMEDIATE`, and a `character-private` audience additionally
        // takes the named Character's exclusive lease in the durable
        // multi-scope order (World ids first, then Character ids). The
        // audience permission is then re-resolved inside that transaction, so
        // an archive/unbind racing this call cannot land a row whose holder
        // was no longer admitted. Both leases are released when this returns.
        let mut governance_leases: Vec<crate::actor_fence::KnowledgeGovernanceLease> = Vec::new();
        if let Some(audience) = request.patch.audience.as_ref() {
            governance_leases.push(
                self.acquire_knowledge_governance(
                    principal,
                    crate::actor_fence::ActorFenceKind::World,
                    world_id.clone(),
                )
                .await?,
            );
            if let nexus_contracts::world_kb_patch_entity_request::NexusWorldKbEntityPatchAudience::CharacterPrivate(
                character_id,
            ) = audience
            {
                governance_leases.push(
                    self.acquire_knowledge_governance(
                        principal,
                        crate::actor_fence::ActorFenceKind::Character,
                        character_id.to_string(),
                    )
                    .await?,
                );
            }
        }
        // Retained route texture (daemon world-KB family): a foreign World is
        // refused by the typed ownership guard as 403 before any other
        // admission runs, so adding the read selection below cannot turn that
        // answer into an existence-revealing 404.
        crate::world_kb::guards::require_world_owner(
            &self.inner.pool,
            &world_id,
            principal.creator_id(),
        )
        .await?;
        // Durable §5.1: World-KB authoring is Creator **management** authoring
        // on owned containers, so it reads and writes under the management
        // selection (owned World/Character/binding containers plus the
        // known-governance holder set) — server-chosen from stored state, never
        // from the request.
        let read_scope = self
            .creator_management_read_scope(principal, &world_id)
            .await?;
        patch::patch_entity(
            &self.inner.pool,
            principal.creator_id(),
            &world_id,
            &read_scope,
            request,
        )
        .await
    }

    /// List pending World KB candidates with keyset pagination.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::InvalidInput`] when the cursor is
    /// malformed, and [`CoreError::Forbidden`] when the caller does not own
    /// the world.
    pub async fn world_kb_candidates(
        &self,
        principal: &Principal,
        world_id: String,
        limit: Option<i64>,
        cursor: Option<String>,
    ) -> CoreResult<WorldKbCandidatesResponse> {
        self.verify_principal(principal)?;
        candidates::get_candidates(
            &self.inner.pool,
            principal.creator_id(),
            &world_id,
            limit,
            cursor,
        )
        .await
    }

    /// Read the `core_changes` outbox page after `request.after_sequence`.
    ///
    /// # Errors
    /// Returns [`CoreError::AuthRequired`] when the principal fails
    /// verification, [`CoreError::InvalidInput`] when `after_sequence` is
    /// not a non-negative decimal string, and the mapped database error
    /// otherwise (`resync_required` is set when the requested tail has been
    /// retention-trimmed).
    pub async fn changes(
        &self,
        principal: &Principal,
        request: CoreChangesRequest,
    ) -> CoreResult<CoreChangesResponse> {
        self.verify_principal(principal)?;
        read_changes(&self.inner.pool, request).await
    }

    /// Close the pool and release writer guards exactly once.
    ///
    /// The drain runs as a RETAINED task, not on the caller's future: the
    /// native cleanup budget cancels this future mid-drain, and a later close
    /// must observe (and wait for) the REAL drain instead of reporting a close
    /// that never happened. Every caller — including a retry after an
    /// interrupted close — waits for that task's result, so a confirmed report
    /// is only ever published by a close that actually settled, and an
    /// interrupted caller retains the pool, the writer admission and the
    /// execution owner it did not settle.
    ///
    /// A close also FENCES new `start_execution` calls and waits for the
    /// registered in-flight ones to settle before it takes the execution slot
    /// and releases the pool, the writer admission and the per-DB owner fence
    /// (see [`CoreInner::start_fence`]) — so `cleanup_confirmed` can never
    /// precede a registered builder's shutdown.
    ///
    /// # Errors
    /// Currently infallible: the report describes a settled close, or the
    /// unconfirmed report of a drain that could not run.
    pub async fn close(&self) -> CoreResult<CoreCloseReport> {
        self.start_close_drain();
        loop {
            let notified = self.inner.close_settled.notified();
            tokio::pin!(notified);
            // Register BEFORE the check so a settlement that lands in between
            // cannot be missed.
            notified.as_mut().enable();
            let report = self
                .inner
                .close_report
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .clone();
            if let Some(report) = report {
                return Ok(report);
            }
            notified.await;
        }
    }

    /// Start the ONE close drain, if it has not been started yet.
    ///
    /// The fence rises in the caller's own step, before the drain task first
    /// runs, so an operation that arrives after `close` began is refused even
    /// while the drain is still being scheduled.
    fn start_close_drain(&self) {
        if self
            .inner
            .close_drain_started
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return;
        }
        self.inner.closing.store(true, Ordering::SeqCst);
        let inner = Arc::clone(&self.inner);
        tokio::spawn(async move {
            // A drain that panicked must not strand every later close on a
            // report that never comes.
            if tokio::spawn(close_drain(Arc::clone(&inner))).await.is_err() {
                publish_close_report(&inner, interrupted_close_report());
            }
        });
    }
}

/// The retained close drain: settle the execution owner, then the SQL pool and
/// every admission this open took over the home, then publish the ONE report.
///
/// I4 ordering is load-bearing: the execution owner is settled BEFORE the SQL
/// pool closes, because drive cancellation, the bounded join and every final
/// durable settlement run against a LIVE pool.
async fn close_drain(inner: Arc<CoreInner>) {
    // Close the start barrier BEFORE taking the slot. The retained start task
    // holds the shared half for the whole start (engine construction, recovery,
    // the install, or the settlement of an owner whose caller went away), so
    // this waits out every registered start instead of releasing the pool, the
    // writer admission and the per-DB owner fence under a registered builder.
    // `closing` was raised in the caller's step, so a start arriving from here
    // on is refused by `ensure_open` and never builds at all.
    //
    // Lock order (fence, then slot) matches `start_execution`'s install, which
    // holds the fence while taking the slot mutex — so the two cannot
    // deadlock, and the drain only ever takes the slot of a start that has
    // already published it or abandoned it.
    #[cfg(feature = "execution")]
    let _starts_settled = Arc::clone(&inner.start_fence).write_owned().await;
    #[cfg(feature = "execution")]
    let execution_handle = inner
        .execution
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    #[cfg(feature = "execution")]
    if let Some(handle) = &execution_handle {
        handle.shutdown().await;
    }
    inner.pool.close().await;
    // A confirmed close ends the admission THIS open took over the home.
    // Both halves are needed for the OS lock to actually go away:
    //
    // 1. the retained guard is what a later open would otherwise JOIN — and
    //    joining a settled generation would republish its epoch while
    //    claiming a fresh owner, so the next owner must take a new
    //    admission (a new epoch) instead;
    // 2. this open's own admitted-pool handle is given up, so a closed
    //    service value that is still referenced cannot keep the guard
    //    behind `state.db.engine.lock` alive after the close reported the
    //    service released.
    //
    // `owns_engine_admission` keeps the two accesses apart: an
    // `EngineOwner` that merely JOINED a live in-process admission (the
    // daemon's co-host shape) releases nothing, because the creator still
    // owns it. The `DirectWriter` arm is the pre-existing cooperative
    // rule (`init_guarded_pool` always creates its own admission).
    if inner.access == CoreAccess::DirectWriter || inner.owns_engine_admission {
        release_retained_writer_guards(&inner.db_path);
    }
    // EVERY closing core gives up its OWN admitted-pool handle, joiner
    // included: a cooperative joiner holds a clone of the CREATOR's guard, so
    // a closed-but-still-referenced joiner would otherwise keep
    // `state.db.engine.lock` open after the creator released the retained
    // entry — and a fresh open over the same home would fail `OwnerBusy` with
    // no live owner at all.
    inner
        .guarded
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .take();
    // `take` returned the handle, so ownership is settled; release the
    // per-DB fence so a later open of the same DB can claim it.
    #[cfg(feature = "execution")]
    if let Some(handle) = execution_handle {
        crate::execution::lifecycle::release_owner_slot(&inner.db_path, &handle);
    }
    publish_close_report(&inner, settled_close_report());
}

/// Publish the drain's result exactly ONCE and wake every waiter.
fn publish_close_report(inner: &CoreInner, report: CoreCloseReport) {
    let mut slot = inner
        .close_report
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if slot.is_none() {
        *slot = Some(report);
    }
    drop(slot);
    inner.close_settled.notify_waiters();
}

const fn settled_close_report() -> CoreCloseReport {
    CoreCloseReport {
        state: nexus_contracts::CoreCloseReportState::Closed,
        cleanup_confirmed: true,
        pending_operations: Vec::new(),
        reason: None,
    }
}

/// The report of a drain that could not run to completion: nothing was
/// released and no caller may treat it as a confirmation.
const fn interrupted_close_report() -> CoreCloseReport {
    CoreCloseReport {
        state: nexus_contracts::CoreCloseReportState::Interrupted,
        cleanup_confirmed: false,
        pending_operations: Vec::new(),
        reason: None,
    }
}
