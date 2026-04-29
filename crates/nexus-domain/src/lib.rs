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

// Pedantic clippy lints that are mechanical/stylistic and don't affect correctness.
// These are either too noisy for the development stage or require significant
// refactoring that would change structure (pre-v1.0).
#![allow(clippy::use_self)] // 26: `Self` vs type name is purely stylistic
#![allow(clippy::missing_errors_doc)] // 23: `# Errors` sections for all Result fns
#![allow(clippy::must_use_candidate)] // 24: adding #[must_use] everywhere is noisy
#![allow(clippy::derive_partial_eq_without_eq)] // 21: Eq derives can be added later
#![allow(clippy::uninlined_format_args)] // 15: format args style preference
#![allow(clippy::redundant_closure_for_method_calls)] // 9: method ref style
#![allow(clippy::doc_markdown)] // 9 remaining: backtick style in docs
#![allow(clippy::cast_precision_loss)] // 6: usize→f32 precision in review metrics
#![allow(clippy::option_if_let_else)] // 5: if-let vs map_or_else style
#![allow(clippy::missing_const_for_fn)] // 3: const fn not always beneficial
#![allow(clippy::items_after_statements)] // 3: const placement in fn body
#![allow(clippy::needless_collect)] // 1: collect() usage in filtering
#![allow(clippy::float_cmp)] // test-only: f32 exact comparisons in assertions
#![allow(clippy::single_char_pattern)] // test-only: char pattern style
#![allow(clippy::redundant_clone)] // test-only: some tests use clone unnecessarily
#![allow(clippy::clone_on_copy)] // test-only: clone on Copy types in test setup
#![allow(clippy::cast_possible_truncation)] // 2: usize→u32 truncation in counters
#![allow(clippy::cast_possible_wrap)] // 2: u64→i64 wrap in duration casts
#![allow(clippy::single_match_else)] // 1: match vs if-let style preference
#![allow(clippy::manual_let_else)] // 1: let-else style preference
#![allow(clippy::if_not_else)] // 1: if !cond vs swapped branches
#![allow(clippy::match_same_arms)] // test-only: test match arm patterns
#![allow(clippy::similar_names)] // test-only: variable naming in tests
#![allow(clippy::manual_string_new)] // test-only: String::new() in test data
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
pub mod review_quality;
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
