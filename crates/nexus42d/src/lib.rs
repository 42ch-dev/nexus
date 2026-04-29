//! nexus42d — Nexus Daemon Library
//!
//! This library exposes the daemon's internal modules for testing
//! and future embedding. The binary entry point is in `main.rs`.

#![deny(clippy::unwrap_used)]

pub mod api;
pub mod auth;
pub mod cli_config;
pub mod db;
pub mod lifecycle;
pub mod workspace;

/// Helpers for integration tests (also used by `tests/*.rs` in this crate).
pub mod test_utils;

/// Helpers for building Axum apps with ephemeral engines for integration tests.
/// Gated behind `#[cfg(test)]` because it uses dev-dependencies.
#[cfg(test)]
pub mod test_support;
