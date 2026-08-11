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
pub mod acp;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod acp_trace;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod acp_worker;
// V1.148 P3 N-C0 (DF-72): Connect Host facade — compiled only with the
// opt-in `connect-host` feature (`nexus42 connect start`).
#[cfg(feature = "connect-host")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod connect;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod creator;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod daemon;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod daemon_run;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod desktop;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod host_call;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod platform;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod sync;
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod system;
