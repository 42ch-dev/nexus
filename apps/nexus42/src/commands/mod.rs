//! Command modules for nexus42 CLI
//!
//! Deny `.unwrap()` in production command code to encourage proper error
//! propagation. Individual modules may opt out with `#[allow(clippy::unwrap_used)]`
//! on specific items where justified.

// Enforce no `.unwrap()` in production command code.
// Each sub-module inherits this deny via the module-level attribute below.
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod creator;

#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod acp;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod acp_trace;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod capability;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod compute;
#[cfg(all(feature = "legacy-cli", feature = "connect-host"))]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod connect;
// v1.193 P2-T2: the `daemon` and `daemon_run` command modules (and their
// parser routing) are deleted with the legacy daemon composition — Electron/
// TS owns service lifecycle.
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod desktop;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod local_creator_bootstrap;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod ops;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod platform;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod preset;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod sync;
#[cfg(feature = "legacy-cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod system;
