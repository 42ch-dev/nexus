//! Cloud-line domain modules (runtime mode, degradation, guard).
//!
//! These modules belong to the cloud line (CLI) per architecture spec §8:
//! "`runtime_mode`, `degradation`, and platform health probing belong to
//! the cloud line (CLI / cloud-stage builds), not the daemon hot path."

pub mod degradation;
pub mod errors;
pub mod runtime_guard;
pub mod runtime_mode;

// Re-export primary types for convenience
pub use degradation::{
    DegradationGuard, DegradationPolicy, DegradationSnapshot, DegradationState,
    HealthCheckSnapshot,
};
pub use errors::DomainError;
pub use runtime_guard::{check_operation, classify_operation, require_platform};
pub use runtime_mode::DomainRuntimeMode;
