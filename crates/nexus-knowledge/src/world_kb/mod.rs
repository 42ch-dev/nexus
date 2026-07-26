//! World KB — World-scoped narrative KB graph: `WorldKbEntry` + `SourceAnchor`.
//!
//! This module owns the `WorldKbEntry` aggregate (structured knowledge units in
//! world timelines), the `SourceAnchor` value object, and the `KbStore` trait
//! for World-scoped KB graph insertion and query.
//!
//! # Module scope
//!
//! Per the entity scope model, this module owns narrative knowledge assets
//! under a **World** entity — not generic Creator or User knowledge. It was
//! relocated (V1.139 P1 T1) from the former `nexus-kb` crate, which has been
//! merged into `nexus-knowledge` alongside the existing User-scoped knowledge
//! and reference-source domains.
//!
//! # Uniqueness constraint
//!
//! Under the same `world_id`, at most one **active** `WorldKbEntry` may exist
//! for a given `(canonical_name, block_type)` pair.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod errors;
pub mod extract_finalize;
pub mod extract_sync;
pub mod knowledge_entry;
pub mod query;
pub mod source_anchor;
pub mod store;
pub mod validation;

pub use errors::{KbError, ValidationError, ValidationKind};
pub use extract_finalize::{finalize_extract, ExtractFinalizeInput};
pub use extract_sync::{compute_kb_diff, diff_and_apply, KbSyncDiff, KbSyncUpdate};
pub use knowledge_entry::{WorldKbBody, WorldKbEntry};
pub use query::{KbInsertResult, KbQuery, KbQueryResult};
pub use store::{InMemoryKbStore, KbStore, KbStoreError};
pub use validation::{
    block_type_state_key, validate_body, validate_canonical_name, ValidationMode, NOVEL_CATEGORIES,
};

// V1.139 P1 T2 — wire-boundary re-export. `spoke_schemas::KnowledgeEntry` is the
// spoke standard type; `WorldKbEntry` converts to/from it at the seam (see
// `knowledge_entry.rs`). spoke-operations receive the spoke type only (spec §7).
pub use spoke_schemas::KnowledgeEntry;
