//! Nexus Wire Contracts (Generated from JSON Schema)
//!
//! This crate contains type definitions generated from `schemas/` JSON Schema files.
//! All wire types are auto-generated - do not modify manually.

pub mod enum_conversions;
pub mod generated;

// Re-export all generated types at crate root
pub use generated::*;

// Re-export hand-maintained enum types from enum_conversions
pub use enum_conversions::RuntimeMode;
