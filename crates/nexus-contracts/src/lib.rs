//! Nexus Wire Contracts (Generated from JSON Schema)
//!
//! This crate contains type definitions generated from `schemas/` JSON Schema files.
//! All wire types are auto-generated - do not modify manually.
//!
//! Hand-written local types live in `local/` — see `schemas-boundary.md` §3.

pub mod enum_conversions;
pub mod generated;
pub mod local;

// Re-export all generated types at crate root (includes wire types only)
pub use generated::*;
