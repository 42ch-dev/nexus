//! Hand-written local types per `.mstar/specs/active/architecture/schemas-external-consumer-boundary.md`.
//! These types are NOT codegen'd; edit by hand.
//!
//! Local types are those that `nexus-platform` never observes over any wire channel.
//! They live exclusively in the nexus OSS Rust codebase.

pub mod acp;
pub mod acp_runtime;
pub mod domain;
pub mod meta;
pub mod orchestration;
pub mod schedule;

pub use meta::*;
