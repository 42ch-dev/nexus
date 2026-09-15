//! Transport-neutral core service (v1.189 P1 → v1.190 P2).

mod actor_fence;
mod actor_knowledge;
mod actors;
mod changes;
mod creators;
mod error;
mod forks;
mod home;
mod memory;
mod memory_pipeline;
mod principal;
mod provider_journal;
mod service;
mod soul;
mod storage_status;
mod sync;
mod timeline;
mod world_kb;
mod world_pack;
mod world_rules;
mod worlds;

pub use actor_fence::{ActorActivityLease, CharacterTransitionLease};
pub use actor_knowledge::{
    ActorKnowledgePage, ActorKnowledgeViewQuery, ActorKnowledgeViewService,
    KNOWLEDGE_INSERT_FAILED_PREFIX, KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX,
    KNOWLEDGE_WIRE_INVALID_PREFIX,
};
pub use actors::{
    ActorPairMode, ActorViewpoint, AdmittedActor, AdmittedActorContext,
    CHARACTER_WIRE_INVALID_PREFIX, CoreActorAdmission, classify_pair,
};
pub use creators::CREATOR_INTERNAL_CODES;
pub use error::{CoreError, CoreResult, MEMORY_INTERNAL_CODES};
pub use home::CoreHomeService;
pub use memory::{
    CharacterTomBeliefRow, CharacterTomListQuery, CharacterTomPage, CharacterTomRecordInput,
    CharacterTomService,
};
pub use principal::Principal;
pub use service::{CoreAccess, CoreOpenOptions, CoreService};
pub use soul::CoreCharacterMind;
pub use storage_status::{CoreStorageStatus, CoreStorageVersions};
pub use timeline::{CoreTimelineEventsQuery, CoreTimelineOverviewQuery};
pub use worlds::DELETE_WORLD_BLOCKED_BY_BINDINGS;
