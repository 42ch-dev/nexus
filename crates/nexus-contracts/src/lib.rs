//! Nexus Wire Contracts (Generated from JSON Schema)
//!
//! This crate contains type definitions generated from `schemas/` JSON Schema files.
//! All wire types are auto-generated - do not modify manually.
//!
//! Hand-written local types live in `local/` — see `schemas-boundary.md` §3.

pub mod common_types;
pub mod enum_conversions;
pub mod generated;
pub mod local;

// Re-export all generated types at crate root (includes wire types only)
pub use generated::*;

// Re-export SourceAnchor at the crate root for drift-test discoverability
// (`use nexus_contracts::*`). The hand-maintained `common_types` module is
// NOT glob-re-exported here to avoid ambiguity with typify-generated
// inlined copies of the same enums in domain/daemon-api modules.
pub use common_types::SourceAnchor;
