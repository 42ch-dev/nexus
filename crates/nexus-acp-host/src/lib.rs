//! nexus-acp-host — ACP client host library.
//!
//! This crate is intentionally **worker-only** linkable. See
//! `.agents/plans/knowledge/acp-client-tech-spec-v2.md` §11 for the rules.

#![deny(clippy::unwrap_used)]

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
pub use client::{AcpSdkAdapter, NexusAcpClient};
#[allow(unused_imports)]
pub use error::{AcpError, AcpResult};

// Re-export policy types for permission management.
#[allow(unused_imports)]
pub use policy::{DefaultPolicy, PermissionDecision, PermissionPolicy};

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
pub use transport::{AcpSession, AgentSpawner, Platform};

// Re-export session management types.
#[allow(unused_imports)]
pub use session_manager::{SessionEntry, SessionManager};
