//! ACP integration for nexus42 CLI.
//!
//! Phase-1 compatibility layer: re-exports from `nexus-acp-host` so that
//! existing `crate::acp::*` paths continue to work. Remove in Phase 2 (WS2).
//!
//! The `session_capture` submodule stays here because it has deep coupling
//! to nexus42-internal modules (`api::daemon_client`, `config`, `errors`).

pub use nexus_acp_host::*;

pub mod session_capture;
