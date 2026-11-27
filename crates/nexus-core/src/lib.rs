//! Transport-neutral core service (v1.189 P1 → v1.190 P2).

mod actor_fence;
mod actor_knowledge;
#[cfg(feature = "provider-host")]
pub mod actor_sessions;
mod actors;
#[cfg(feature = "execution")]
mod apply;
#[cfg(feature = "strategy-authoring")]
mod approve;
mod atomic_write;
mod changes;
mod chronology;
mod compose;
mod config_lock;
mod content;
mod context;
mod creators;
mod error;
mod findings;
mod forks;
#[cfg(feature = "strategy-authoring")]
mod generate;
mod home;
#[cfg(feature = "provider-host")]
pub mod host;
mod knowledge;
mod memory;
mod memory_binding;
mod memory_credentials;
mod memory_pipeline;
pub(crate) mod memory_provider_error;
#[cfg(feature = "memory-remote")]
mod memory_remote;
mod memory_selection;
#[cfg(feature = "operation-id")]
pub mod operation_id;
mod operation_receipts;
mod outline;
#[cfg(feature = "execution")]
pub mod owner;
mod patterns;
mod presets;
mod principal;
mod provider_journal;
mod reading;
mod service;
#[cfg(feature = "strategy-authoring")]
mod simulate;
pub mod slug;
mod soul;
mod storage_status;
mod strategy_candidates;
mod sync;
mod timeline;
pub mod tool_catalog;
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
#[cfg(feature = "execution")]
pub use apply::{
    AppliedDocumentRefusal, AppliedStrategy, ApplyRefusal, ApplyRefusalClass,
    StrategyAppliedStatus, StrategyApproval, StrategyApprovals,
};
pub use chronology::CoreWorkChronology;
#[cfg(feature = "execution")]
pub use compose::document_bodies;
pub use compose::{
    BindSlotOp, ComposeDiagnostic, ComposeOp, ComposeOutcome, ComposeRefusal, NestOp,
    PatternPackageRef, ReplaceSubtreeOp, UnnestOp,
};
pub use config_lock::with_config_lock;
pub use content::{resolve_guarded_path, resolve_guarded_path_async, CoreChapterContentQuery};
pub use context::{LocalDirectiveStore, ReadOnlyDirectiveStore};
pub use creators::CREATOR_INTERNAL_CODES;
pub use error::{
    CoreError, CoreResult, MemoryProviderRefusal, StrategyAlreadyApprovedError,
    StrategyCandidateConflictError, MEMORY_INTERNAL_CODES,
};
pub use findings::{
    format_routing_hint, CreateFindingRequest, ListFindingsQuery, ListFindingsResponse,
    PruneFindingsOutcome, StaleFindingEntry, StaleFindingsResponse,
};
#[cfg(feature = "strategy-authoring")]
pub use generate::{Correction, GenerateExhaustion, GenerateOutcome, GenerateRequest};
pub use home::CoreHomeService;
#[cfg(feature = "provider-host")]
pub use host::HostHandle;
pub use knowledge::QueueKbExtractParams;
pub use memory::{
    CharacterTomBeliefRow, CharacterTomListQuery, CharacterTomPage, CharacterTomRecordInput,
    CharacterTomService, MemoryRevision,
};
/// The review-drain step-point a deterministic concurrency regression arms
/// (NXM-5 Task 2 fix round 2, finding F1). Test builds only.
#[cfg(feature = "test-hooks")]
pub use memory::{MemoryReviewPause, MemoryReviewRendezvous};
pub use memory_credentials::MemoryCredential;
pub use memory_selection::{MemoryProviderSelection, RemoteMemoryProviderSelection};
pub use nexus_home_layout::MemoryProviderConfig;
pub use nexus_memory_provider::{LegacyContentMigration, LegacyMigrationStatus};
#[cfg(feature = "execution")]
pub use owner::{ExecutionBuildObserver, ExecutionHandle, ExecutionOpenError, RunnerDeps};
pub use presets::PresetError;
pub use principal::Principal;
pub use service::{CoreAccess, CoreOpenOptions, CoreService};
pub use soul::CoreCharacterMind;
pub use spoke_operations::spoke_schemas::{ProfileDocument, RetainedRecord};
pub use storage_status::{CoreStorageStatus, CoreStorageVersions};
pub use strategy_candidates::StrategyCandidate;
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
