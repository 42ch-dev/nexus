//! Policy and admission gates.
//!
//! ACP provider permissions delegate to `nexus-acp-host::PermissionPolicy` (R-003).
//! Native CLI providers use host-level risk classification only.

pub mod admission;
pub mod permission;

pub use admission::AdmissionPolicy;
pub use permission::{HostPermissionResolver, PermissionOutcome};
