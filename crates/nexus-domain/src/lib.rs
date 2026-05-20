//! Nexus Domain Logic (facade)
//!
//! This crate is a **facade** that re-exports domain modules from their
//! focused application crates. New code should depend on the specific
//! crate directly; this crate exists for backward compatibility during
//! the migration period (Batch B7→B8).
//!
//! # Architecture
//!
//! - `nexus-creator` — Creator aggregate + local identity
//! - `nexus-creator-memory` — Memory pipeline, review, SOUL I/O
//! - `nexus-kb` — Key blocks + source anchors
//! - `nexus-knowledge` — Reference sources
//! - `nexus-narrative` — Worlds, forks, timelines, manuscripts
//! - `nexus-cloud-domain` — User + pairing (cloud sync domain)
//!
//! Modules still owned by this crate:
//! - `degradation` — Degradation guard + policy
//! - `runtime_guard` — Runtime mode guard
//! - `runtime_mode` — Domain runtime mode enum
//! - `contract_assertions` — Test-only schema validation helpers

// Pedantic clippy lints that are mechanical/stylistic and don't affect correctness.
#![allow(clippy::use_self)]
#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::needless_collect)]
#![allow(clippy::float_cmp)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_string_new)]

// ── Modules still owned by this crate ──────────────────────────────
pub mod consistency {
    pub use nexus_narrative::consistency::*;
}
#[cfg(test)]
pub mod contract_assertions;
pub mod creator {
    pub use nexus_creator::creator::*;
}
pub mod degradation;
pub mod errors;
pub mod experience_aggregation {
    pub use nexus_creator_memory::experience_aggregation::*;
}
pub mod fork_branch {
    pub use nexus_narrative::fork_branch::*;
}
pub mod key_block {
    pub use nexus_kb::key_block::*;
}
pub mod local_identity {
    pub use nexus_creator::local_identity::*;
}
pub mod long_term_memory {
    pub use nexus_creator_memory::long_term_memory::*;
}
pub mod manuscript_state {
    pub use nexus_narrative::manuscript_state::*;
}
pub mod memory_io {
    pub use nexus_creator_memory::memory_io::*;
}
pub mod memory_item {
    pub use nexus_creator_memory::memory_item::*;
}
pub mod pairing {
    pub use nexus_cloud_domain::pairing::*;
}
pub mod personality_sync {
    pub use nexus_creator_memory::personality_sync::*;
}
pub mod reference_source {
    pub use nexus_knowledge::reference_source::*;
}
pub mod review {
    pub use nexus_creator_memory::review::*;
}
pub mod review_quality {
    pub use nexus_creator_memory::review_quality::*;
}
pub mod runtime_guard;
pub mod runtime_mode;
pub mod soul {
    pub use nexus_creator_memory::soul::*;
}
pub mod soul_io {
    pub use nexus_creator_memory::soul_io::*;
}
pub mod source_anchor {
    pub use nexus_kb::source_anchor::*;
}
pub mod story_manifest {
    pub use nexus_narrative::story_manifest::*;
}
pub mod timeline_event {
    pub use nexus_narrative::timeline_event::*;
}
pub mod user {
    pub use nexus_cloud_domain::user::*;
}
pub mod world {
    pub use nexus_narrative::world::*;
}
pub mod world_membership {
    pub use nexus_narrative::world_membership::*;
}

// ── Re-export error types ──────────────────────────────────────────
pub use errors::DomainError;

// ── Re-export validation helpers ───────────────────────────────────
pub use local_identity::is_valid_creator_id;

// ── Re-export domain types ─────────────────────────────────────────
pub use degradation::{
    DegradationGuard, DegradationPolicy, DegradationSnapshot, HealthCheckSnapshot,
};
pub use long_term_memory::LongTermMemory;
pub use review::{check_session_already_promoted, promote_to_long_term, SessionDigestSummarizer};
pub use runtime_mode::DomainRuntimeMode;
pub use soul::SoulDocument;

// ── Re-export common types from nexus-contracts ────────────────────
pub use nexus_contracts::{
    BlockType, BundleType, CreatorId, KeyBlockId, ManuscriptPhase, MemoryType, TimePolicy,
    TimelineEventId, Timestamp, UserId, Visibility, WorkspaceId, WorldId,
};
