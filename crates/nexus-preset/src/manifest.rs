//! Preset manifest re-exports from `nexus-contracts`.
//!
//! The hand-written preset types live in `nexus-contracts::local::orchestration::preset`
//! per `schemas-boundary.md` §3. This module re-exports them for convenience.
// note: the types are re-exported verbatim; no preset type is defined in this crate.

pub use nexus_contracts::local::orchestration::preset::*;
