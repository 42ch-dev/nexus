//! Transport-neutral core service (v1.189 P1 → v1.190 P2).

mod actor_fence;
mod actor_knowledge;
mod actors;
mod changes;
mod error;
mod home;
mod principal;
mod provider_journal;
mod service;
mod storage_status;
mod sync;
mod worlds;
mod world_kb;
mod creators;
mod timeline;
mod forks;
mod world_pack;
mod world_rules;

pub use actor_fence::{ActorActivityLease, CharacterTransitionLease};
pub use actor_knowledge::{
    ActorKnowledgePage, ActorKnowledgeViewQuery, ActorKnowledgeViewService,
    KNOWLEDGE_INSERT_FAILED_PREFIX, KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX,
    KNOWLEDGE_WIRE_INVALID_PREFIX,
};
pub use actors::{
    classify_pair, AdmittedActor, AdmittedActorContext, ActorPairMode, ActorViewpoint,
    CHARACTER_WIRE_INVALID_PREFIX, CoreActorAdmission,
};
pub use creators::CREATOR_INTERNAL_CODES;
pub use error::{CoreError, CoreResult};
pub use home::CoreHomeService;
pub use principal::Principal;
pub use service::{CoreAccess, CoreOpenOptions, CoreService};
pub use storage_status::{CoreStorageStatus, CoreStorageVersions};
pub use worlds::DELETE_WORLD_BLOCKED_BY_BINDINGS;
pub use timeline::{CoreTimelineEventsQuery, CoreTimelineOverviewQuery};
