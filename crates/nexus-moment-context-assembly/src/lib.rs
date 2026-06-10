//! Per-moment context assembly for ACP sessions.
//!
//! Three assembly strategies:
//!
//! **Stage-0** (`local_only` mode):
//! Assembles context from local sources only: SOUL sections, long-term memories,
//! fragment keywords, and the user prompt.
//!
//! **Moment assembly** (default, local-only):
//! Aggregates from all four local domains: creator memory (SOUL), narrative state,
//! World KB, and User knowledge. This is the primary entry point for V1.23+.
//!
//! **Two-Stage** (`local_first` / `cloud_enhanced` modes, requires `cloud-stage` feature):
//! Stage-1 calls platform `context/assemble` API; Stage-2 merges with local data.
//!
//! # Features
//!
//! - `cloud-stage` — enables `TwoStageAssembly` and platform API types
//!   (adds dependency on `nexus-cloud-sync`).
//!
//! # Domain dependencies
//!
//! - `nexus-creator-memory` — SOUL sections, long-term memories, fragment keywords
//! - `nexus-narrative` — World state, timeline, events (via `NarrativeGateway`)
//! - `nexus-kb` — World-scoped key blocks (via `KbStore`)
//! - `nexus-knowledge` — User-scoped knowledge entries (via `KnowledgeStore`)

pub mod moment;
pub mod stage0;
pub mod world_context;

// Cloud-stage types gated behind feature flag
#[cfg(feature = "cloud-stage")]
pub mod cloud_stage;

// Re-export primary types
#[cfg(feature = "cloud-stage")]
pub use cloud_stage::TwoStageAssembly;
pub use moment::{assemble_moment, MomentContext, MomentRequest};
pub use stage0::Stage0Assembly;
pub use world_context::{
    build_chapter_kb_block, ChapterKbBlockParams, WorldContextBlock, WorldContextItem,
    WorldKbQueryBuilder, DEFAULT_WORLD_CONTEXT_TOKEN_BUDGET,
};
