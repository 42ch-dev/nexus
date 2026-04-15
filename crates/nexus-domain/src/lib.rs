//! Nexus Domain Logic
//!
//! Domain layer implementing business logic for all Nexus aggregates.
//! Builds on top of `nexus-contracts` generated types.
//!
//! # Architecture
//!
//! - Each aggregate is a separate module with domain logic methods
//! - Domain types embed or convert to/from contract types
//! - All validation follows consistency-rules-v1.md
//!
//! # Integration
//!
//! Domain types are designed for serde roundtrip compatibility with
//! `nexus-contracts` generated types. See `contract_assertions` module.

pub mod consistency;
pub mod context_assembly;
#[cfg(test)]
pub mod contract_assertions;
pub mod creator;
pub mod degradation;
pub mod errors;
pub mod experience_aggregation;
pub mod fork_branch;
pub mod key_block;
pub mod local_identity;
pub mod long_term_memory;
pub mod manuscript_state;
pub mod memory_io;
pub mod memory_item;
pub mod pairing;
pub mod personality_sync;
pub mod reference_source;
pub mod review;
pub mod runtime_guard;
pub mod runtime_mode;
pub mod soul;
pub mod soul_io;
pub mod source_anchor;
pub mod story_manifest;
pub mod timeline_event;
pub mod user;
pub mod world;
pub mod world_membership;

// Re-export error types
pub use errors::DomainError;

// Re-export validation helpers
pub use local_identity::is_valid_creator_id;

// Re-export domain types
pub use context_assembly::Stage0Assembly;
pub use context_assembly::TwoStageAssembly;
pub use context_assembly::{
    AssembleMetadata, AssembleResponse, KbEntry, MemoryItemRef, TimelineEventRef,
};
pub use degradation::{
    DegradationGuard, DegradationPolicy, DegradationSnapshot, HealthCheckSnapshot,
};
pub use long_term_memory::LongTermMemory;
pub use review::{check_session_already_promoted, promote_to_long_term, SessionDigestSummarizer};
pub use runtime_mode::DomainRuntimeMode;
pub use soul::SoulDocument;

// Re-export common types from nexus-contracts
pub use nexus_contracts::{
    BlockType, BundleType, CreatorId, KeyBlockId, ManuscriptPhase, MemoryType, TimePolicy,
    TimelineEventId, Timestamp, UserId, Visibility, WorkspaceId, WorldId,
};
