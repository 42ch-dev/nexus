//! Nexus Narrative — World, timeline, fork, story, manuscript, consistency.
//!
//! This crate owns the narrative graph aggregates: World, `TimelineEvent`,
//! `ForkBranch`, `StoryManifest`, `ManuscriptState`, `WorldMembership`, and
//! cross-aggregate consistency rules. Uses `nexus-kb` for `KeyBlock` types.

#![allow(clippy::missing_errors_doc)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::uninlined_format_args)]
#![allow(clippy::redundant_closure_for_method_calls)]

pub mod consistency;
pub mod errors;
pub mod fork_branch;
pub mod manuscript_state;
pub mod story_manifest;
pub mod timeline_event;
pub mod world;
pub mod world_membership;

pub use errors::NarrativeError;
