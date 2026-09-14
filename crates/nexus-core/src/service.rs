//! `CoreService` lifecycle and public API surface.

use std::path::PathBuf;
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

use crate::changes::read_changes;
use crate::error::{CoreError, CoreResult};
use crate::principal::Principal;
use crate::world_kb::{candidates, graph, patch};

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

struct CoreInner {
    pool: SqlitePool,
    db_path: PathBuf,
    _guarded: Option<GuardedPool>,
    nexus_home: PathBuf,
    creator_id: String,
    workspace_slug: String,
    generation: AtomicU64,
    access: CoreAccess,
    closing: AtomicBool,
}

pub struct CoreService {
    inner: Arc<CoreInner>,
}

impl CoreService {
    pub async fn open(options: CoreOpenOptions) -> CoreResult<Self> {
        let nexus_home = nexus_home_layout::nexus_root_from_home(&options.user_home);
        let cfg = CliConfigSnapshot::load(&nexus_home).map_err(|e| CoreError::Internal {
            category: format!("config_load: {e}"),
        })?;
        let creator_id = cfg.active_creator_id.clone().ok_or(CoreError::AuthRequired)?;
        let workspace_slug = cfg.workspace_slug_for_creator(&creator_id);
        if workspace_slug.trim().is_empty() {
            return Err(CoreError::AuthRequired);
        }
        let db_path = try_resolve_state_db_path(&options.user_home, &nexus_home)
            .ok_or(CoreError::Uninitialized)?;
        if !db_path.exists() && options.access == CoreAccess::ReadOnly {
            return Err(CoreError::Uninitialized);
        }

        let (pool, guarded) = match options.access {
            CoreAccess::ReadOnly => {
                let pool = open_pool_read_only(&db_path).await.map_err(map_db)?;
                (pool, None)
            }
            CoreAccess::DirectWriter => {
                let guarded = init_guarded_pool(&db_path, &creator_id)
                    .await
                    .map_err(map_db)?;
                let pool = guarded.clone_pool();
                (pool, Some(guarded))
            }
            CoreAccess::EngineOwner => {
                let options = GuardedPoolOptions::default();
                let guarded = match nexus_local_db::writer_protocol::join_live_engine_pool(
                    &db_path,
                    options,
                )
                .await
                .map_err(map_db)?
                {
                    Some(guarded) => guarded,
                    None => init_engine_pool(&db_path, &creator_id, options)
                        .await
                        .map_err(map_db)?,
                };
                let pool = guarded.clone_pool();
                (pool, Some(guarded))
            }
        };

        Ok(Self {
            inner: Arc::new(CoreInner {
                pool,
                db_path,
                _guarded: guarded,
                nexus_home,
                creator_id,
                workspace_slug,
                generation: AtomicU64::new(1),
                access: options.access,
                closing: AtomicBool::new(false),
            }),
        })
    }

    fn ensure_open(&self) -> CoreResult<()> {
        if self.inner.closing.load(Ordering::SeqCst) {
            return Err(CoreError::Closing);
        }
        Ok(())
    }

    fn verify_selected_context(&self) -> CoreResult<()> {
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

    fn verify_principal(&self, principal: &Principal) -> CoreResult<()> {
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

    pub async fn active_principal(&self) -> CoreResult<Principal> {
        self.ensure_open()?;
        self.verify_selected_context()?;
        Ok(Principal::new(
            self.inner.creator_id.clone(),
            self.inner.workspace_slug.clone(),
            self.inner.generation.load(Ordering::SeqCst),
        ))
    }

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
        patch::patch_entity(
            &self.inner.pool,
            principal.creator_id(),
            &world_id,
            request,
        )
        .await
    }

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

    pub async fn changes(
        &self,
        principal: &Principal,
        request: CoreChangesRequest,
    ) -> CoreResult<CoreChangesResponse> {
        self.verify_principal(principal)?;
        read_changes(&self.inner.pool, request).await
    }

    /// Durable JS-provider operation journal access (LIFE-3). The pool is
    /// exposed read/write for the native provider-callback bridge and the
    /// `hostQuery` fallback; it is not a second business truth.
    #[must_use]
    pub fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }

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

fn is_sqlite_busy(err: &sqlx::Error) -> bool {
    match err {
        sqlx::Error::Database(db) => {
            db.code().as_deref() == Some("5")
                || db.message().contains("database is locked")
                || db.message().contains("SQLITE_BUSY")
        }
        _ => false,
    }
}

fn map_db(e: nexus_local_db::LocalDbError) -> CoreError {
    match e {
        nexus_local_db::LocalDbError::OwnerBusy { .. } => CoreError::OwnerBusy,
        nexus_local_db::LocalDbError::WriterFenced { .. } => CoreError::WriterFenced,
        nexus_local_db::LocalDbError::SchemaMismatch { .. } => CoreError::SchemaMismatch,
        nexus_local_db::LocalDbError::Sqlx(err) if is_sqlite_busy(&err) => CoreError::Busy,
        nexus_local_db::LocalDbError::Sqlx(sqlx::Error::PoolTimedOut) => CoreError::OwnerBusy,
        other => CoreError::Internal {
            category: format!("database_error: {other}"),
        },
    }
}
