//! nexus42d — Nexus Daemon Library
//!
//! This library re-exports modules from `nexus-daemon-runtime` for backward
//! compatibility. The binary entry point is in `main.rs`.
//!
//! **TEMPORARY**: This crate will be removed in Batch 3 once `nexus42` CLI
//! wires directly to `nexus-daemon-runtime`.

#![deny(clippy::unwrap_used)]

// Re-export all daemon runtime modules
pub use nexus_daemon_runtime::api;
pub use nexus_daemon_runtime::auth;
pub use nexus_daemon_runtime::config as cli_config;
pub use nexus_daemon_runtime::db;
pub use nexus_daemon_runtime::lifecycle;
pub use nexus_daemon_runtime::workspace;

/// Helpers for integration tests (also used by `tests/*.rs` in this crate).
pub use nexus_daemon_runtime::test_utils;
