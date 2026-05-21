//! Nexus KB — Narrative `KeyBlock` + `SourceAnchor` types and logic.
//!
//! This crate owns the `KeyBlock` aggregate (structured knowledge units in
//! world timelines) and the `SourceAnchor` value object. Types are from
//! `nexus-contracts`; domain logic (validation, state transitions, conversions)
//! lives here.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod errors;
pub mod key_block;
pub mod source_anchor;

pub use errors::KbError;
