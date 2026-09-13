//! Cloud-line domain modules (runtime mode, degradation, guard).
//!
//! These modules belong to the cloud line (CLI) per architecture spec §8:
//! "`runtime_mode`, `degradation`, and platform health probing belong to
//! the cloud line (CLI / cloud-stage builds), not the daemon hot path."

#[cfg(feature = "legacy-cli")]
pub mod degradation;
pub mod errors;
pub mod runtime_guard;
pub mod runtime_mode;

// Re-export primary types for convenience.
//
// `degradation` is `legacy-cli`-only (v1.189 P1 fix round 2): it is the sole
// module in the binary that pulls in `chrono`, and §2.1 keeps `chrono` out of
// the basic cohort. Nothing on the basic command surface reads degradation
// state, so gating here removes the dependency without changing the contract.
#[cfg(feature = "legacy-cli")]
pub use degradation::{
    DegradationGuard, DegradationPolicy, DegradationSnapshot, DegradationState, HealthCheckSnapshot,
};
pub use errors::DomainError;
pub use runtime_guard::{check_operation, classify_operation, require_platform};
pub use runtime_mode::DomainRuntimeMode;
