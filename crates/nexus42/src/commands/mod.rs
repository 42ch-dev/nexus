//! Command modules for nexus42 CLI
//!
//! Deny `.unwrap()` in production command code to encourage proper error
//! propagation. Individual modules may opt out with `#[allow(clippy::unwrap_used)]`
//! on specific items where justified.

// Enforce no `.unwrap()` in production command code.
// Each sub-module inherits this deny via the module-level attribute below.
#[deny(clippy::unwrap_used)]
pub mod acp;
#[deny(clippy::unwrap_used)]
pub mod acp_trace;
#[deny(clippy::unwrap_used)]
pub mod acp_worker;
#[deny(clippy::unwrap_used)]
pub mod creator;
#[deny(clippy::unwrap_used)]
pub mod daemon;
#[deny(clippy::unwrap_used)]
pub mod daemon_run;
#[deny(clippy::unwrap_used)]
pub mod platform;
#[deny(clippy::unwrap_used)]
pub mod sync;
#[deny(clippy::unwrap_used)]
pub mod system;
