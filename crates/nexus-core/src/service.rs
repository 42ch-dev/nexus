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
};
use nexus_local_db::open_pool_read_only;
use nexus_local_db::writer_protocol::{
    init_engine_pool, open_admitted_pool, GuardedPoolOptions, WriterMode,
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
    nexus_home: PathBuf,
    user_home: PathBuf,
    creator_id: String,
    workspace_slug: String,
    db_path: PathBuf,
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
        let creator_id = read_active_creator_id(&nexus_home).ok_or(CoreError::AuthRequired)?;
        let workspace_slug = read_active_workspace_slug(&nexus_home, &creator_id)
            .filter(|s| !s.trim().is_empty())
            .ok_or(CoreError::AuthRequired)?;
        let db_path = try_resolve_state_db_path(&options.user_home, &nexus_home)
            .ok_or(CoreError::Uninitialized)?;
        if !db_path.exists() && options.access == CoreAccess::ReadOnly {
            return Err(CoreError::Uninitialized);
        }

        let pool = match options.access {
            CoreAccess::ReadOnly => open_pool_read_only(&db_path).await.map_err(map_db)?,
            CoreAccess::DirectWriter => {
                open_admitted_pool(&db_path, &creator_id, WriterMode::Direct)
                    .await
                    .map_err(map_db)?
            }
            CoreAccess::EngineOwner => {
                init_engine_pool(
                    &db_path,
                    &creator_id,
                    GuardedPoolOptions::default(),
                )
                .await
                .map_err(map_db)?
                .clone_pool()
            }
        };

        Ok(Self {
            inner: Arc::new(CoreInner {
                pool,
                nexus_home,
                user_home: options.user_home,
                creator_id,
                workspace_slug,
                db_path,
                generation: AtomicU64::new(1),
                access: options.access,
                closing: AtomicBool::new(false),
            }),
        })
    }

    /// Attach an already-admitted engine/direct pool (daemon boot path).
    pub async fn attach_pool(
        options: CoreOpenOptions,
        creator_id: String,
        workspace_slug: String,
        db_path: PathBuf,
        pool: SqlitePool,
    ) -> CoreResult<Self> {
        let nexus_home = nexus_home_layout::nexus_root_from_home(&options.user_home);
        Ok(Self {
            inner: Arc::new(CoreInner {
                pool,
                nexus_home,
                user_home: options.user_home,
                creator_id,
                workspace_slug,
                db_path,
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

    fn verify_principal(&self, principal: &Principal) -> CoreResult<()> {
        self.ensure_open()?;
        if !principal.verify_generation(self.inner.generation.load(Ordering::SeqCst)) {
            return Err(CoreError::AuthRequired);
        }
        Ok(())
    }

    pub async fn active_principal(&self) -> CoreResult<Principal> {
        self.ensure_open()?;
        if read_active_creator_id(&self.inner.nexus_home).as_deref()
            != Some(self.inner.creator_id.as_str())
        {
            return Err(CoreError::AuthRequired);
        }
        let slug = read_active_workspace_slug(&self.inner.nexus_home, &self.inner.creator_id);
        if slug.as_deref() != Some(self.inner.workspace_slug.as_str()) {
            return Err(CoreError::AuthRequired);
        }
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
                resource: "world_kb_patch".to_string(),
                reason: "read-only core access".to_string(),
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
        Ok(CoreCloseReport {
            state: nexus_contracts::CoreCloseReportState::Closed,
            cleanup_confirmed: true,
            pending_operations: vec![],
            reason: None,
        })
    }

    #[must_use]
    pub(crate) fn pool(&self) -> &SqlitePool {
        &self.inner.pool
    }
}

fn map_db(e: nexus_local_db::LocalDbError) -> CoreError {
    let msg = e.to_string();
    if msg.contains("WRITER_FENCED") || msg.contains("writer_fenced") {
        return CoreError::WriterFenced;
    }
    if msg.contains("OWNER_BUSY") || msg.contains("owner_busy") {
        return CoreError::OwnerBusy;
    }
    if msg.contains("database is locked") || msg.contains("SQLITE_BUSY") {
        return CoreError::Busy;
    }
    CoreError::Internal {
        category: format!("database_error: {msg}"),
    }
}
