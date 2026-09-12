//! Nexus42 CLI Library
//!
//! This library provides the core functionality for the nexus42 CLI,
//! including registry management and agent transport.

pub mod cli;
pub mod commands;
pub mod config;
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

#[cfg(all(test, feature = "legacy-cli"))]
mod testutil;

// Re-export commonly used types for convenience
pub use config::CliConfig;
pub use errors::{CliError, Result};
