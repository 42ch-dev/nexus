//! Nexus Knowledge — the World KB knowledge family.
//!
//! `world_kb` is the former `nexus-kb` domain (relocated by the V1.139 P1 T1
//! merger): narrative KB entries (`KnowledgeEntryRecord` + `SourceAnchor`,
//! `KbStore`) tied to a World entity. `KnowledgeEntryRecord` converts to/from
//! the spoke standard `KnowledgeEntry` at the wire boundary (V1.139 P1 T2).
//!
//! # Storage
//!
//! **Production persistence is owned by [`nexus-local-db`]** (see
//! `nexus_local_db::kb_store::SqliteKbStore`). This crate provides domain
//! types, traits, and adapter seams only — it **does not** introduce its own
//! SQLite/file-backed production truth source. (DF-43 closure: V1.55 P0)

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]

pub mod world_kb;
