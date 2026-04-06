//! Nexus42 CLI Library
//!
//! This library provides the core functionality for the nexus42 CLI,
//! including ACP integration, registry management, and agent transport.

pub mod acp;
pub mod api;
pub mod auth;
pub mod commands;
pub mod config;
pub mod context;
pub mod errors;

// Re-export commonly used types for convenience
pub use config::CliConfig;
pub use errors::{CliError, Result};
