//! Thin Workflow IR contract.
//!
//! `schemas/daemon-api/orchestration/workflow-document.schema.json` is the single source of
//! truth; this crate re-exports the generated document types and adds the Q2 structural checks
//! and the effect rollup that schema cannot express. It is deliberately dependency-thin:
//! `nexus-contracts` is the only dependency, so generators, Canvas and validators share the
//! contract without pulling preset loading or orchestration.

pub use nexus_contracts::generated::daemon_api::orchestration::workflow_document::*;

/// The workflow IR version this build composes, validates and resolves pattern packages against.
pub const IR_VERSION: u64 = 0;

pub mod canonical;
pub mod effect;
pub mod structure;
pub use canonical::{content_hash, CanonicalHashError};
pub use effect::{document_summary, effect_summary, join};
pub use structure::{validate, PatternBody, PatternBodyKind, Violation, MAX_NEST_DEPTH};
