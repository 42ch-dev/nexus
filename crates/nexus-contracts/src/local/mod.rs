//! Hand-written local types per `.agents/plans/knowledge/schemas-boundary-v1.md` §3.
//! These types are NOT codegen'd; edit by hand.
//!
//! Local types are those that `nexus-platform` never observes over any wire channel.
//! They live exclusively in the nexus OSS Rust codebase.

pub mod acp_runtime;
pub mod domain;
pub mod meta;

pub use meta::*;
