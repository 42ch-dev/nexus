//! `CoreService` lifecycle and public API surface.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

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

pub(crate) struct CoreInner {
    pub(crate) pool: SqlitePool,
    pub(crate) db_path: PathBuf,
    _guarded: Option<GuardedPool>,
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
    /// Per-Character activity/transition fences (v1.190 P2-T1), Host-free
    /// and separate from any process session registry.
    pub(crate) character_fences: ActorFenceTable,
}

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
                let guarded =
                    match nexus_local_db::writer_protocol::join_live_engine_pool(&db_path, options)
                        .await
                        .map_err(local_db_err)?
                    {
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
                _guarded: guarded,
                nexus_home,
                creator_id,
                workspace_slug,
                generation: AtomicU64::new(1),
                access: options.access,
                closing: AtomicBool::new(false),
                character_fences: ActorFenceTable::new(&db_path),
            }),
        })
    }

    /// The nexus root this service was opened against (`<user_home>/.nexus42`).
    /// Core family modules resolve bearer file paths (SOUL.md, long-term
    /// memory) through it; the field stays private to this module.
    pub(crate) fn nexus_home(&self) -> &std::path::Path {
        &self.inner.nexus_home
    }

    pub(crate) fn ensure_open(&self) -> CoreResult<()> {
        if self.inner.closing.load(Ordering::SeqCst) {
            return Err(CoreError::Closing);
        }
        Ok(())
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
    /// [`CoreError::WorldKbValidation`] for invalid patches, and
    /// [`CoreError::WorldKbConflict`] carrying the committed revision when
    /// the expected version is stale.
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
        patch::patch_entity(&self.inner.pool, principal.creator_id(), &world_id, request).await
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

    /// Transitional pool access for the native provider-callback bridge and
    /// the `hostQuery` fallback (LIFE-3); it is not a second business truth.
    /// Deletion owner: P4-T2, once bridge consumers use the owned journal
    /// methods (`provider_operation`, `journal_provider_operation`,
    /// `settle_provider_orphans`). This escape hatch must not acquire new
    /// callers.
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }

    /// Close the pool and release writer guards exactly once; repeated calls
    /// report the already-closed state.
    ///
    /// # Errors
    /// Currently infallible: the report always describes a fully settled
    /// close (`cleanup_confirmed`), including for repeat callers.
    pub async fn close(&self) -> CoreResult<CoreCloseReport> {
        if self
            .inner
            .closing
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .is_err()
        {
            return Ok(CoreCloseReport {
                state: nexus_contracts::CoreCloseReportState::Closed,
                cleanup_confirmed: true,
                pending_operations: vec![],
                reason: None,
            });
        }
        self.inner.pool.close().await;
        if self.inner.access == CoreAccess::DirectWriter {
            release_retained_writer_guards(&self.inner.db_path);
        }
        Ok(CoreCloseReport {
            state: nexus_contracts::CoreCloseReportState::Closed,
            cleanup_confirmed: true,
            pending_operations: vec![],
            reason: None,
        })
    }
}

