//! Nexus42 CLI Library
//!
//! This library provides the core functionality for the nexus42 CLI,
//! including registry management and agent transport.

pub mod cli;
pub mod commands;
pub mod config;
pub(crate) mod core;
pub mod domain;
pub mod errors;
pub mod paths;

#[cfg(feature = "legacy-cli")]
pub mod api;
#[cfg(feature = "legacy-cli")]
pub mod auth;
#[cfg(feature = "legacy-cli")]
pub mod challenge;
#[cfg(feature = "legacy-cli")]
pub mod context;
#[cfg(feature = "legacy-cli")]
pub mod creator_identity;
#[cfg(feature = "legacy-cli")]
pub mod db;

// Test-only helper: `tempfile` is a dev-dependency, so it is available in
// every test build regardless of feature cohort. Gating this on `legacy-cli`
// alone broke `cargo test -p nexus42 --no-default-features --features
// basic-cli --lib` (shared `config` tests call it → E0433).
#[cfg(test)]
mod testutil;

// Re-export commonly used types for convenience
pub use config::CliConfig;
pub use errors::{CliError, Result};
