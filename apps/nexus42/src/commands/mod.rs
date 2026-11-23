//! Command modules for nexus42 CLI
//!
//! Deny `.unwrap()` in production command code to encourage proper error
//! propagation. Individual modules may opt out with `#[allow(clippy::unwrap_used)]`
//! on specific items where justified.
//!
//! v1.193 P2-T13 cohort gates: the authoring/operator modules belong to
//! `cli`; `connect` belongs to `connect-host` (it never implies `cli`).

// Each sub-module inherits this deny via the module-level attribute below.
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod creator;

#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod acp;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod acp_trace;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod capability;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod compute;
#[cfg(feature = "connect-host")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod connect;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod desktop;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod local_creator_bootstrap;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod ops;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod platform;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod preset;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod sync;
#[cfg(feature = "cli")]
#[deny(clippy::unwrap_used)]
#[cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]
pub mod system;
