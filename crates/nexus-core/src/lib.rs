//! Transport-neutral core service (v1.189 P1 → v1.190 P2).

mod actor_fence;
mod actor_knowledge;
#[cfg(feature = "provider-host")]
pub mod actor_sessions;
mod actors;
mod changes;
mod chronology;
#[cfg(feature = "connect-client")]
pub mod connect;
mod content;
mod context;
mod creators;
mod error;
#[cfg(feature = "execution")]
pub mod execution;
mod findings;
mod forks;
mod home;
#[cfg(feature = "provider-host")]
pub mod host;
mod knowledge;
mod memory;
mod memory_pipeline;
mod operation_receipts;
// v1.207 P3-T1 (spec §B.1): the stable durable operation id every receipt
// consumer resolves through. Root-level and gated on its own tiny feature so
// the Connect host (which links no `execution`) reuses the ONE derivation.
#[cfg(feature = "operation-id")]
pub mod operation_id;
mod outline;
mod presets;
mod principal;
mod provider_journal;
mod reading;
mod references;
mod service;
mod soul;
mod storage_status;
mod sync;
mod timeline;
mod works;
mod world_kb;
mod world_pack;
mod world_rules;
mod worlds;

pub use actor_fence::{
    ActorActivityLease, ActorFenceKind, CharacterTransitionLease, KnowledgeEffectLeases,
    KnowledgeGovernanceLease,
};
pub use actor_knowledge::{
    authored_patch_audience, ActorKnowledgeIdentity, ActorKnowledgePage, ActorKnowledgeViewQuery,
    ActorKnowledgeViewService, AdmittedKnowledgeContext, KnowledgeRevisions,
    KNOWLEDGE_INSERT_FAILED_PREFIX, KNOWLEDGE_VIEW_COMPONENT_FAILED_PREFIX,
    KNOWLEDGE_WIRE_INVALID_PREFIX,
};
#[cfg(feature = "provider-host")]
pub use actor_sessions::{
    echo_actor_pair, ActorSessionKey, ActorSessionKind, ActorSessionRegistry,
    CharacterOperationSnapshot, KnowledgeReuse,
};
pub use actors::{
    classify_pair, require_active_owned_character, require_active_owned_world,
    require_actor_holder, ActorPairMode, ActorViewpoint, AdmittedActor, AdmittedActorContext,
    CoreActorAdmission, CHARACTER_WIRE_INVALID_PREFIX,
};
pub use chronology::CoreWorkChronology;
pub use content::{resolve_guarded_path, resolve_guarded_path_async, CoreChapterContentQuery};
pub use context::{LocalDirectiveStore, ReadOnlyDirectiveStore};
pub use creators::CREATOR_INTERNAL_CODES;
pub use error::{CoreError, CoreResult, MEMORY_INTERNAL_CODES};
#[cfg(feature = "execution")]
pub use execution::{
    CancelOutcome, DriveDisposition, ExecutionBuildObserver, ExecutionHandle, ExecutionOpenError,
    PresetRunConfig, PresetRunOutcome, ResumeDecision, RunControlError, RunControlResult,
    RunSignal, RunnerDeps, WorkflowRunCoordinator,
};
pub use findings::{
    format_routing_hint, CreateFindingRequest, ListFindingsQuery, ListFindingsResponse,
    PruneFindingsOutcome, StaleFindingEntry, StaleFindingsResponse,
};
pub use home::CoreHomeService;
#[cfg(feature = "provider-host")]
pub use host::HostHandle;
pub use knowledge::QueueKbExtractParams;
pub use memory::{
    CharacterTomBeliefRow, CharacterTomListQuery, CharacterTomPage, CharacterTomRecordInput,
    CharacterTomService,
};
pub use presets::PresetError;
pub use principal::Principal;
pub use references::{RegisterReferenceParams, REFERENCE_SOURCE_TYPES};
// Reference read envelopes are schema-owned (core lane); re-exported here so
// direct-core callers keep naming them through `nexus_core`.
pub use nexus_contracts::core::references::{
    ReferenceGetResponse, ReferenceListResponse, ReferenceSourceInfo,
};
pub use service::{CoreAccess, CoreOpenOptions, CoreService};
pub use soul::CoreCharacterMind;
pub use storage_status::{CoreStorageStatus, CoreStorageVersions};
pub use timeline::{CoreTimelineEventsQuery, CoreTimelineOverviewQuery};
pub use works::{
    AddInspirationRequest, AddInspirationResponse, ArchiveInspirationRequest, ArchivePoolRequest,
    ListInspirationQuery, ListInspirationResponse, ListPoolQuery, ListPoolResponse,
    PromoteInspirationRequest, PromoteInspirationResponse, PromotePoolRequest,
    ReconcileDryRunQuery, SetPoolActiveRequest, WorkDetails, WorkInspirationItem, WorkPatchRequest,
    WorkPoolEntry, WorkReconcileReport,
};
pub use world_pack::{
    AtomCounts, HolderMapping, HolderMappingSelector, ImportAtomKind, ImportDetail, ImportOutcome,
    ImportQuarantineReview, ImportSummary, QuarantineReason, QuarantinedAtomReport,
    REVIEW_IMPORT_MAX_ATOMS,
};
pub use world_rules::RulePatchPresence;
/// Test-only write-lock rendezvous for the rule write path (`test-hooks`).
#[cfg(any(test, feature = "test-hooks"))]
pub use world_rules::{set_rule_write_gate, RuleWriteGate};
pub use worlds::DELETE_WORLD_BLOCKED_BY_BINDINGS;
