//! ACP (Agent Client Protocol) integration for nexus42 CLI.
//!
//! This module provides the adapter layer between the nexus42 CLI and ACP
//! agents, following the architecture defined in the ACP Client tech spec.
//!
//! # Architecture
//!
//! ```text
//! Commands (agent/*) ──► ACP Module ──► NexusAcpClient (trait)
//!                                       │
//!                                       └─► AcpSdkAdapter (SDK wrapper)
//!                                               │
//!                                               └─► agent-client-protocol SDK
//!                                                       │
//!                                                       └─► stdio (JSON-RPC 2.0)
//!                                                               │
//!                                                               └─► Agent Subprocess
//! ```
//!
//! # Module Layout
//!
//! - [`client`] — `NexusAcpClient` trait + `AcpSdkAdapter` implementation
//! - [`error`] — `AcpError` enum covering all ACP failure modes
//! - [`localset_bridge`] — Bridge between async tokio and `!Send` LocalSet futures
//! - [`registry`] — ACP registry manifest fetcher + local cache
//! - [`skills`] — Frozen capability IDs + capability set construction
//! - [`transport`] — Subprocess spawn + stdio pipe management + lifecycle

pub mod client;
pub mod error;
pub mod localset_bridge;
pub mod registry;
pub mod skills;
pub mod transport;

// Re-export the primary types at module level for convenience.
// These are used by future tasks (commands, registry, transport).
#[allow(unused_imports)]
pub use client::{
    AcpSdkAdapter, InitializedSession, NexusAcpClient, PromptCompleted, SessionCreated,
};
#[allow(unused_imports)]
pub use error::{AcpError, AcpResult};

// Re-export registry types for commands and other consumers.
// These are used by Task 3 (CLI commands) and Task 4 (transport).
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
