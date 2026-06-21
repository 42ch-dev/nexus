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
//!
//! **Production persistence is owned by [`nexus-local-db`]** (see
//! `nexus_local_db::knowledge_store::SqliteKnowledgeStore` and
//! `nexus_local_db::reference_source`). This crate provides domain types,
//! traits, and adapter seams only — it **does not** introduce its own
//! SQLite/file-backed production truth source. (DF-43 closure: V1.55 P0)

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
