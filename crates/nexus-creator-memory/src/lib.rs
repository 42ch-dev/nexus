//! Nexus memory pipeline — SOUL, LTM, review, personality IO.
//!
//! This crate owns the bearer-scoped memory operations: SOUL document
//! parsing, long-term memory management, review/promotion, personality sync,
//! and experience aggregation. Every stage dispatches through the closed
//! [`MemoryBearerRef`] (Creator | Character); the Creator arm reproduces the
//! legacy layout and bytes exactly.

pub mod bearer;
pub mod errors;
pub mod experience_aggregation;
pub mod long_term_memory;
pub mod memory_io;
pub mod memory_item;
pub mod personality_sync;
pub mod review;
pub mod review_quality;
pub mod soul;
pub mod soul_io;
pub mod soul_narrative;

pub use bearer::{BearerIdentity, MemoryBearerRef};
pub use errors::MemoryError;
pub use long_term_memory::LongTermMemory;
pub use review::{check_session_already_promoted, promote_to_long_term, SessionDigestSummarizer};
pub use soul::SoulDocument;
