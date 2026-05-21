//! Nexus Narrative — World, timeline, fork, story, manuscript, consistency.
//!
//! This crate owns the narrative graph aggregates: World, `TimelineEvent`,
//! `ForkBranch`, `StoryManifest`, `ManuscriptState`, `WorldMembership`, and
//! cross-aggregate consistency rules. Uses `nexus-kb` for `KeyBlock` types.
//!
//! # Read Model API (V1.23)
//!
//! The `gateway` module exposes the `NarrativeGateway` trait — a read-only
//! API for querying narrative state (`NarrativeContext`, `WorldState`,
//! `TimelinePosition`, `EventSnapshot`). This is the primary entry point for
//! `nexus-moment-context-assembly` and CLI/daemon local APIs.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_closure_for_method_calls)]

pub mod consistency;
pub mod errors;
pub mod fork_branch;
pub mod gateway;
pub mod manuscript_state;
pub mod narrative_context;
pub mod narrative_query;
pub mod story_manifest;
pub mod timeline_event;
pub mod world;
pub mod world_membership;

pub use errors::NarrativeError;
pub use gateway::{InMemoryNarrativeGateway, NarrativeGateway};
pub use narrative_context::{EventSnapshot, NarrativeContext, TimelinePosition, WorldState};
pub use narrative_query::NarrativeQuery;
