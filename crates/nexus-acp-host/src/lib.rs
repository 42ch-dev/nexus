//! nexus-acp-host — ACP client host library.
//!
//! Linked by the provider-host cohort: `nexus-agent-host` (unconditional
//! normal edge) and `nexus-core` / `nexus-core-node` boot through the
//! `provider-host` feature. The retired per-creator worker composition no
//! longer defines the linkage boundary; see
//! `.mstar/specs/active/agents/acp-client-tech-spec.md` §1.1 for the retained
//! dependency and SDK boundary.

#![deny(clippy::unwrap_used)]
// Test modules use ergonomic `.unwrap()`/`.expect()`; production keeps the strict deny.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]

pub mod client;
pub mod error;
pub mod localset_bridge;
pub mod policy;
pub mod registry;
pub mod session_manager;
pub mod skills;
pub mod transport;

// Re-export the primary types at module level for convenience.
#[allow(unused_imports)]
pub use client::{AcpPermissionOutcome, AcpSdkAdapter, AcpStreamUpdate, NexusAcpClient};
#[allow(unused_imports)]
pub use error::{AcpError, AcpResult};

// Re-export policy types for permission management.
#[allow(unused_imports)]
pub use policy::{AgentRules, DefaultPolicy, PermissionDecision, PermissionPolicy};

// Re-export registry types for commands and other consumers.
#[allow(unused_imports)]
pub use registry::{
    AgentEntry, BinaryDistribution, CacheMeta, Distribution, NpxDistribution, PlatformBinary,
    Registry, RegistryClient, REGISTRY_URL,
};

// Re-export capability IDs for direct access from commands/transport.
#[allow(unused_imports)]
pub use skills::{build_v1_0_capabilities, capabilities};

// Re-export transport types for subprocess management.
#[allow(unused_imports)]
pub use transport::{AcpSession, AgentSpawner, ManagedAcpProcess, Platform, ProcessBirthToken};

#[allow(unused_imports)]
pub use localset_bridge::LocalSetBridge;

// Re-export session management types.
#[allow(unused_imports)]
pub use session_manager::{SessionEntry, SessionManager};
