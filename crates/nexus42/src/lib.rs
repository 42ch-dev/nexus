//! Nexus42 CLI Library
//!
//! This library provides the core functionality for the nexus42 CLI,
//! including registry management and agent transport.

pub mod api;
pub mod auth;
pub mod challenge;
pub mod cli;
pub mod commands;
pub mod config;
pub mod context;
pub mod db;
pub mod errors;
pub mod paths;
pub mod session_capture;

// Re-export commonly used types for convenience
pub use config::CliConfig;
pub use errors::{CliError, Result};
