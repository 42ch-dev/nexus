//! Nexus KB — World-scoped narrative KB graph: `KeyBlock` + `SourceAnchor`.
//!
//! This crate owns the `KeyBlock` aggregate (structured knowledge units in
//! world timelines), the `SourceAnchor` value object, and the `KbStore` trait
//! for World-scoped KB graph insertion and query.
//!
//! # Crate scope
//!
//! Per the entity scope model, `nexus-kb` owns narrative knowledge assets
//! under a **World** entity — not generic Creator or User knowledge.
//!
//! # Uniqueness constraint
//!
//! Under the same `world_id`, at most one **active** `KeyBlock` may exist
//! for a given `(canonical_name, block_type)` pair.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod errors;
pub mod extract_finalize;
pub mod key_block;
pub mod query;
pub mod source_anchor;
pub mod store;
pub mod validation;

pub use errors::{KbError, ValidationError, ValidationKind};
pub use extract_finalize::{finalize_extract, ExtractFinalizeInput};
pub use query::{KbInsertResult, KbQuery, KbQueryResult};
pub use store::{InMemoryKbStore, KbStore, KbStoreError};
pub use validation::{validate_body, validate_canonical_name, ValidationMode, NOVEL_CATEGORIES};
