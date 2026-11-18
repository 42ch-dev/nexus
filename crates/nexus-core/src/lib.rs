//! Transport-neutral core service (v1.189 P1 → v1.190 P2).

mod actor_fence;
mod actor_knowledge;
#[cfg(feature = "provider-host")]
pub mod actor_sessions;
mod actors;
// v1.241 P3.5 Task 2: the apply write. Gated with approve — both are the P3 approval/apply
// surface the daemon route exposes, and neither links without `execution`.
#[cfg(feature = "execution")]
mod apply;
#[cfg(feature = "execution")]
mod approve;
mod changes;
mod chronology;
// The Connect protocol stack lives in `nexus-connect` (v1.244 extraction):
// the dependency direction is `nexus-core → nexus-connect`, never back, and
// callers name the crate directly.
mod compose;
mod config_lock;
mod content;
mod context;
mod creators;
mod error;
mod findings;
mod forks;
// Generate produces a whole candidate through the three shared lint layers; layer 3 is the
// orchestration buildability predicate apply gates on, so the module is execution-gated like
// apply/approve/simulate rather than always compiled like compose.
#[cfg(feature = "execution")]
mod generate;
mod home;
#[cfg(feature = "provider-host")]
pub mod host;
mod knowledge;
mod memory;
#[cfg(feature = "execution")]
pub mod owner;
// v1.229 N3 Task 3: the one provider authority an open pins.
mod memory_binding;
mod memory_credentials;
mod memory_pipeline;
// v1.245 extraction: the provider's own error taxonomy projected onto the
// core's. The provider crate names no core type, so this `From` impl is the
// whole boundary between them.
mod memory_provider_error;
// v1.229 N3 Task 1: the signed Connect open of a selected remote provider.
#[cfg(feature = "memory-remote")]
mod memory_remote;
// v1.229 NXM-9: the validated `[memory_provider]` selection an open resolves.
mod memory_selection;
mod operation_receipts;
pub mod slug;
// v1.207 P3-T1 (spec §B.1): the stable durable operation id every receipt
// consumer resolves through. Root-level and gated on its own tiny feature so
// the Connect host (which links no `execution`) reuses the ONE derivation.
#[cfg(feature = "operation-id")]
pub mod operation_id;
mod outline;
mod patterns;
mod presets;
mod principal;
mod provider_journal;
mod reading;
mod service;
#[cfg(feature = "execution")]
mod simulate;
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
    AppliedDocumentRefusal, AppliedStrategy, StrategyAppliedStatus, StrategyApproval,
    StrategyApprovals,
};
pub use chronology::CoreWorkChronology;
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
// The migration report the legacy pass returns; the provider crate owns both
// types, and `nexus_core` keeps the path its callers already name.
pub use nexus_memory_provider::{LegacyContentMigration, LegacyMigrationStatus};
#[cfg(feature = "execution")]
pub use owner::{ExecutionBuildObserver, ExecutionHandle, ExecutionOpenError, RunnerDeps};
pub use presets::PresetError;
pub use principal::Principal;
// Provider-owned memory contract objects (spoke §6): the E1/E2 direct-core
// callers name the Creator record and SOUL profile documents through
// `nexus_core` instead of a second DTO set.
#[cfg(feature = "execution")]
pub use compose::document_bodies;
pub use compose::{
    BindSlotOp, ComposeDiagnostic, ComposeOp, ComposeOutcome, ComposeRefusal, NestOp,
    PatternPackageRef, ReplaceSubtreeOp, UnnestOp,
};
// Generate shares the compose registry and the CAS commit with Compose and needs the
// orchestration buildability predicate, so it rides the same execution edge as apply/approve.
#[cfg(feature = "execution")]
pub use generate::{Correction, GenerateExhaustion, GenerateOutcome, GenerateRequest};
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
