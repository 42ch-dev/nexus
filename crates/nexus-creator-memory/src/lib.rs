//! Nexus Creator Memory — SOUL, LTM, review, personality IO.
//!
//! This crate owns creator-scoped memory operations: SOUL document parsing,
//! long-term memory management, review/promotion, personality sync, and
//! experience aggregation. Depends on `nexus-creator` for Creator types.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_clone)]
#![allow(clippy::clone_on_copy)]
#![allow(clippy::cast_precision_loss)]
#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_possible_wrap)]
#![allow(clippy::needless_collect)]
#![allow(clippy::single_char_pattern)]
#![allow(clippy::similar_names)]
#![allow(clippy::manual_string_new)]
#![allow(clippy::single_match_else)]
#![allow(clippy::manual_let_else)]
#![allow(clippy::if_not_else)]
#![allow(clippy::match_same_arms)]
#![allow(clippy::items_after_statements)]
#![allow(clippy::missing_const_for_fn)]
#![allow(clippy::option_if_let_else)]
#![allow(clippy::doc_markdown)]
#![allow(clippy::float_cmp)]
#![allow(clippy::redundant_closure_for_method_calls)]
#![allow(clippy::derive_partial_eq_without_eq)]

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

pub use errors::MemoryError;
pub use long_term_memory::LongTermMemory;
pub use review::{check_session_already_promoted, promote_to_long_term, SessionDigestSummarizer};
pub use soul::SoulDocument;
