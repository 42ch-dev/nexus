//! Nexus Knowledge — Global references and future general KB.
//!
//! This crate owns `ReferenceSource` (local-only research/reference registration)
//! and future general knowledge base entries. Indexed per creator/workspace but
//! NOT narrative KeyBlocks (those live in `nexus-kb`).

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod errors;
pub mod reference_source;

pub use errors::KnowledgeError;
