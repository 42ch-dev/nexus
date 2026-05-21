//! Nexus Knowledge — User-scoped global knowledge and reference sources.
//!
//! This crate owns two domains:
//!
//! - **User-scoped knowledge** (`knowledge` module): tag-driven global knowledge entries
//!   indexed per `user_id`. These may be pulled into Moment context assembly.
//!   Not Creator-scoped; does not own narrative `KeyBlocks`.
//!
//! - **Reference sources** (`reference_source` module): local-only research/reference
//!   registration. Indexed per creator/workspace but NOT narrative `KeyBlocks`
//!   (those live in `nexus-kb`).
//!
//! # Storage
//!
//! Knowledge persistence is abstracted behind the [`KnowledgeStore`] trait.
//! A default [`InMemoryKnowledgeStore`] is provided for testing and prototyping.
//! SQLite-backed storage will be added when migrations can be extended.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod errors;
pub mod knowledge;
pub mod reference_source;
pub mod store;

pub use errors::KnowledgeError;
pub use knowledge::{KnowledgeEntry, KnowledgeQuery, KnowledgeResult, KnowledgeTag};
pub use store::{InMemoryKnowledgeStore, KnowledgeStore};
