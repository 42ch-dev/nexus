//! Nexus42 CLI Library
//!
//! This library provides the core functionality for the nexus42 CLI,
//! including registry management and agent transport.
//!
//! # Cohorts (v1.193 P2-T13)
//!
//! Two real product cohorts share this crate and its `config`/`errors`/`db`
//! modules:
//!
//! - `cli` (crate default) — the ordinary one-shot authoring/operator/cloud
//!   product: `cli`/`commands`/`core`/`paths`/`db` plus the authoring
//!   command modules.
//! - `connect-host` — the independent Connect cohort (headless
//!   `nexus-runtime`): `commands::connect` over the shared config/error/
//!   local-db modules. It never implies `cli`, so no authoring module,
//!   Host/orchestration scheduler or HTTP router is reachable from it.
//!
//! `cli,connect-host` is the optional CLI Connect build (same parser plus the
//! `connect` group).
//!
//! The HTTP-only `api` module (hand-written daemon wire DTOs + `DaemonClient`)
//! was deleted with the daemon composition — no consumer remained.

#[cfg(feature = "cli")]
pub mod auth;
#[cfg(feature = "cli")]
pub mod challenge;
#[cfg(feature = "cli")]
pub mod cli;
pub mod commands;
pub mod config;
#[cfg(feature = "cli")]
pub mod context;
#[cfg(feature = "cli")]
pub(crate) mod core;
#[cfg(feature = "cli")]
pub mod creator_identity;
// `db` (the `nexus-local-db` schema initializer) and `domain` (runtime mode +
// error vocabulary) are shared: `config.rs` resolves the state-db path and the
// Connect runtime opens the workspace pool through them.
pub mod db;
pub mod domain;
pub mod errors;
pub mod paths;

// Test-only helper: `tempfile` is a dev-dependency, so it is available in
// every test build regardless of feature cohort.
#[cfg(test)]
mod testutil;

// Re-export commonly used types for convenience
pub use config::CliConfig;
pub use errors::{CliError, Result};
