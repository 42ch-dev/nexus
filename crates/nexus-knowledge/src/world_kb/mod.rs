//! World KB — World-scoped narrative KB graph: `KnowledgeEntryRecord` + `SourceAnchor`.
//!
//! This module owns the `KnowledgeEntryRecord` aggregate (structured knowledge units in
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
//! Under the same `world_id`, at most one **active** `KnowledgeEntryRecord` may exist
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
pub use extract_finalize::{
    persist_prepared_extract, prepare_extract, ExtractPrepareInput, PreparedExtractCandidate,
};
pub use extract_sync::{compute_kb_diff, diff_and_apply, KbSyncDiff, KbSyncUpdate};
pub use knowledge_entry::{
    is_character_subject_id, reject_reserved_authoring_keys, resolve_authored_governance,
    validate_character_tom_belief_row, validate_native_governance, BeliefPropositionRaw,
    KnowledgeAudience, KnowledgeAuthoringOp, KnowledgeEntryBody, KnowledgeEntryRecord,
    KnowledgeGovernance, KnowledgeOwnerRef, MentalFieldsRaw, ReservedAuthoringKey,
    DISCLOSURE_OWNER_PRIVATE, LEGACY_CREATOR_ONLY_KEY, LEGACY_CREATOR_ONLY_UNSUPPORTED,
    LEGACY_MENTAL_BELIEF_MODULES_FIXTURE,
};
pub use query::{KbInsertResult, KbQuery, KbQueryResult};
pub use store::{InMemoryKbStore, KbStore, KbStoreError, KnowledgeReadPolicy, KnowledgeReadScope};
pub use validation::{
    block_type_state_key, validate_body, validate_canonical_name, ValidationMode, NOVEL_CATEGORIES,
};

// V1.139 P1 T2 — wire-boundary re-export. `spoke_schemas::KnowledgeEntry` is the
// spoke standard type; `KnowledgeEntryRecord` converts to/from it at the seam (see
// `knowledge_entry.rs`). spoke-operations receive the spoke type only (spec §7).
pub use spoke_schemas::KnowledgeEntry;
