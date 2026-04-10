//! Command modules for nexus42 CLI
//!
//! Deny `.unwrap()` in production command code to encourage proper error
//! propagation. Individual modules may opt out with `#[allow(clippy::unwrap_used)]`
//! on specific items where justified.

// Enforce no `.unwrap()` in production command code.
// Each sub-module inherits this deny via the module-level attribute below.
#[deny(clippy::unwrap_used)]
pub mod agent;
#[deny(clippy::unwrap_used)]
pub mod auth;
#[deny(clippy::unwrap_used)]
pub mod context;
#[deny(clippy::unwrap_used)]
pub mod creator;
#[deny(clippy::unwrap_used)]
pub mod daemon;
#[deny(clippy::unwrap_used)]
pub mod db;
#[deny(clippy::unwrap_used)]
pub mod explore;
#[deny(clippy::unwrap_used)]
pub mod init;
#[deny(clippy::unwrap_used)]
pub mod manuscript;
#[deny(clippy::unwrap_used)]
pub mod policy;
#[deny(clippy::unwrap_used)]
pub mod publish;
#[deny(clippy::unwrap_used)]
pub mod research;
#[deny(clippy::unwrap_used)]
pub mod session;
#[deny(clippy::unwrap_used)]
pub mod sync;
#[deny(clippy::unwrap_used)]
pub mod world;
