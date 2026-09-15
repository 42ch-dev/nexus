//! Thin daemon translation over the core KnowledgeView service (v1.190 P2-T1).
//!
//! The stored-owner composition, keyset pagination, cursor codec and
//! admission predicates live in [`nexus_core`]; this wrapper only re-sends
//! errors at the HTTP boundary. The type re-exports keep the
//! `actor_sessions`/`agent_host`/`actor_run_capture` imports compiling
//! until their owning tasks (P4-T2) consume the core types directly.

use crate::api::errors::NexusApiError;
use sqlx::SqlitePool;

pub use nexus_core::{ActorKnowledgePage, ActorKnowledgeViewQuery, AdmittedActor};

/// One reusable `KnowledgeView` composer (core-backed).
#[must_use]
pub struct ActorKnowledgeViewService(nexus_core::ActorKnowledgeViewService);

impl ActorKnowledgeViewService {
    /// Bind the service to a workspace pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self(nexus_core::ActorKnowledgeViewService::new(pool))
    }

    /// Resolve limit (1..=100, default 50).
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when `limit` is out of range.
    pub fn resolve_limit(raw: Option<i64>) -> Result<u32, NexusApiError> {
        nexus_core::ActorKnowledgeViewService::resolve_limit(raw).map_err(NexusApiError::from)
    }

    /// Admit `actor_ref` from stored rows and compose the locked view.
    ///
    /// # Errors
    ///
    /// Auth/ownership failures, invalid cursors, or any failed component query.
    pub async fn view(
        &self,
        caller_creator_id: &str,
        actor: &AdmittedActor,
        query: ActorKnowledgeViewQuery,
    ) -> Result<ActorKnowledgePage, NexusApiError> {
        self.0
            .view(caller_creator_id, actor, query)
            .await
            .map_err(NexusApiError::from)
    }

    /// Character-owned listing without a World filter.
    ///
    /// # Errors
    ///
    /// Missing Character or a failed owner query.
    pub async fn list_character_owned(
        &self,
        caller_creator_id: &str,
        character_id: &str,
        limit: u32,
        cursor: Option<String>,
    ) -> Result<ActorKnowledgePage, NexusApiError> {
        self.0
            .list_character_owned(caller_creator_id, character_id, limit, cursor)
            .await
            .map_err(NexusApiError::from)
    }

    /// Active stored binding tuple (write admission).
    pub(crate) async fn require_active_binding(
        &self,
        character_id: &str,
        binding_id: &str,
        world_id: &str,
    ) -> Result<(), NexusApiError> {
        self.0
            .require_active_binding(character_id, binding_id, world_id)
            .await
            .map_err(NexusApiError::from)
    }

    /// Stored binding tuple with no status requirement (retained reads).
    pub(crate) async fn require_stored_binding_tuple(
        &self,
        character_id: &str,
        binding_id: &str,
        world_id: &str,
    ) -> Result<(), NexusApiError> {
        self.0
            .require_stored_binding_tuple(character_id, binding_id, world_id)
            .await
            .map_err(NexusApiError::from)
    }
}
