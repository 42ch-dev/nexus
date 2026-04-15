//! Nexus Wire Contracts (Generated from JSON Schema)
//!
//! This crate contains type definitions generated from `schemas/` JSON Schema files.
//! All wire types are auto-generated - do not modify manually.

pub mod enum_conversions;
pub mod generated;

// Re-export all generated types at crate root (includes RuntimeMode from runtime-mode.schema.json)
pub use generated::*;
