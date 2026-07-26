//! Nexus Knowledge — Knowledge entries (World + User) + reference sources.
//!
//! This crate consolidates three knowledge tiers in one crate (V1.139 P1 T1
//! merger of the former `nexus-kb` into `nexus-knowledge`):
//!
//! - **World KB** (`world_kb` module): the former `nexus-kb`'s domain —
//!   narrative KB entries (`WorldKbEntry` + `SourceAnchor`, `KbStore`) tied to
//!   a World entity. Relocated here from the deleted `nexus-kb` crate.
//!   `WorldKbEntry` converts to/from the spoke standard `KnowledgeEntry` at the
//!   wire boundary (V1.139 P1 T2); User-scoped entries are a separate domain
//!   (`UserKnowledgeEntry` in the `knowledge` module).
//!
//! - **User-scoped knowledge** (`knowledge` module): tag-driven global knowledge
//!   entries indexed per `user_id`. These may be pulled into Moment context
//!   assembly. Not Creator-scoped.
//!
//! - **Reference sources** (`reference_source` module): local-only
//!   research/reference registration. Indexed per creator/workspace.
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
pub mod world_kb;

pub use errors::KnowledgeError;
pub use knowledge::{KnowledgeQuery, KnowledgeResult, KnowledgeTag, UserKnowledgeEntry};
pub use store::{InMemoryKnowledgeStore, KnowledgeStore};
