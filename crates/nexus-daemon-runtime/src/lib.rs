//! nexus-daemon-runtime — Nexus Daemon Runtime
//!
//! Reusable runtime layer for daemon lifecycle, subsystem composition,
//! and local API transport. Extracted from the former standalone daemon binary.

#![deny(clippy::unwrap_used)]

pub mod api;
pub mod auth;
pub mod boot;
pub mod config;
pub mod db;
pub mod lifecycle;
pub mod workspace;

/// Helpers for integration tests (also used by `tests/*.rs` in this crate).
pub mod test_utils;

/// Helpers for building Axum apps with ephemeral engines for integration tests.
/// Gated behind `#[cfg(test)]` because it uses dev-dependencies.
#[cfg(test)]
pub mod test_support;

/// Architecture dependency assertions — compile-time and runtime checks
/// for the intended dependency graph of this crate.
#[cfg(test)]
mod architecture_assertions;
