//! Stored-data Actor admission for agent-host session and prompt paths,
//! now a thin translation over the core admission service (v1.190 P2-T1).
//!
//! Stored ownership validation and the bounded `KnowledgeView` live in
//! [`nexus_core::CoreActorAdmission`]; this shim only re-sends errors at the
//! HTTP boundary. Callers never treat request-body ownership claims as
//! trusted context: any deny returns before MCA, `HostFacade` session work,
//! registry insertion, or provider calls. [`AdmittedActor`] is an opaque
//! core token with no serde impls, so a serialized admission context can
//! never be replayed as authority.

use crate::api::errors::NexusApiError;
use sqlx::SqlitePool;

pub use nexus_core::{ActorPairMode, ActorViewpoint, AdmittedActor, AdmittedActorContext};

/// One reusable stored-data admission service (core-backed).
#[must_use]
pub struct ActorAdmissionService {
    admission: nexus_core::CoreActorAdmission,
}

impl ActorAdmissionService {
    /// Bind admission to a workspace pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            admission: nexus_core::CoreActorAdmission::new(pool),
        }
    }

    /// Classify the optional pair. Partial pairs are a stable 400.
    ///
    /// # Errors
    ///
    /// Returns `invalid_input` when exactly one of the pair is present.
    pub fn classify_pair(
        actor_present: bool,
        viewpoint_present: bool,
    ) -> Result<ActorPairMode, NexusApiError> {
        nexus_core::classify_pair(actor_present, viewpoint_present).map_err(NexusApiError::from)
    }

    /// Admit stored Creator/Character/World/binding ownership and load the
    /// bounded view.
    ///
    /// # Errors
    /// Auth, ownership, status, or view-composition failures. No host/MCA side effects.
    pub async fn admit(
        &self,
        caller_creator_id: &str,
        actor: AdmittedActor,
        viewpoint: ActorViewpoint,
    ) -> Result<AdmittedActorContext, NexusApiError> {
        self.admission
            .admit(caller_creator_id, actor, viewpoint)
            .await
            .map_err(NexusApiError::from)
    }
}
