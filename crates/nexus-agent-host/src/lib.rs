//! nexus-agent-host — Hybrid managed-only host core for local agent execution.
//!
//! This crate provides the orchestration/facade layer above `nexus-acp-host` and
//! native CLI process adapters. It normalizes ACP providers and native CLI providers
//! behind narrow `ProviderAdapter`, `HostFacade`, and `ProviderDiscovery` traits.
//!
//! # Architecture
//!
//! ```text
//! nexus42 CLI
//!   └─ nexus-daemon-runtime
//!        ├─ lifecycle subsystem: AgentHostSubsystem
//!        ├─ Axum routes: /v1/local/agent-host/*
//!        └─ Arc<dyn HostFacade>
//!             └─ nexus-agent-host (this crate)
//!                  ├─ core: HostManager, SessionRegistry, OpRegistry
//!                  ├─ capability: normalized host operations + negotiation
//!                  ├─ discovery: config + PATH + ACP registry catalog
//!                  ├─ providers/acp: official SDK via nexus-acp-host
//!                  └─ providers/native_cli/claude: Wave 1 native adapter
//! ```
//!
//! # Design Principles
//!
//! - **Managed-only**: every session is host-owned, observable, and cancellable.
//! - **ACP-first**: ACP providers are preferred; native CLIs report honest limited capabilities.
//! - **Reuse `nexus-acp-host`**: no duplicate ACP JSON-RPC implementation.
//! - **Narrow facade**: daemon runtime depends only on `HostFacade` trait.

#![deny(clippy::unwrap_used)]

pub mod capability;
pub mod config;
pub mod core;
pub mod discovery;
pub mod error;
pub mod ids;
pub mod policy;
pub mod providers;
pub mod telemetry;

use async_trait::async_trait;

// Re-export primary types at crate root for convenience.
pub use error::{HostError, HostResult};
pub use ids::{HostOperationId, HostSessionId, ProviderId};

// Re-export key public types from submodules.
pub use core::{HostSession, SessionRegistry, SessionState, TransitionResult};
pub use discovery::ProviderCatalog;
pub use policy::{AdmissionPolicy, HostPermissionResolver, PermissionOutcome};
pub use telemetry::TelemetryContext;

use capability::model::{
    CapabilityDescriptor, CreateSessionRequest, HostEventStream, HostHealth, HostOperation,
    HostStartConfig, LaunchSpec, ManagedSessionHandle, ProbeRequest, ProtocolKind,
    ProviderDescriptor, ProviderHealth,
};
use config::AgentHostConfig;

/// Provider adapter trait — the narrow interface each provider implements.
///
/// ACP providers use `nexus-acp-host` under the hood; native CLI providers
/// manage a subprocess directly. Both produce normalized `HostEvent` streams.
#[async_trait]
pub trait ProviderAdapter: Send + Sync {
    /// Return the static descriptor for this provider.
    fn descriptor(&self) -> ProviderDescriptor;

    /// Probe provider availability (health check).
    ///
    /// Should be lightweight — checks if the provider process can be started
    /// and responds to a basic handshake.
    async fn probe(&self, request: ProbeRequest) -> HostResult<ProviderHealth>;

    /// Launch a new managed session on this provider.
    ///
    /// Creates a provider process (or ACP connection), performs handshake,
    /// and returns a handle for subsequent operations.
    async fn launch(&self, spec: LaunchSpec) -> HostResult<ManagedSessionHandle>;

    /// Execute an operation on an active session.
    ///
    /// Returns a stream of `HostEvent` items. The stream MUST emit exactly
    /// one terminal event (`OpFinished` or `OpFailed`) before ending.
    async fn execute(
        &self,
        session: &ManagedSessionHandle,
        op: HostOperation,
    ) -> HostResult<HostEventStream>;

    /// Cancel an in-progress operation.
    ///
    /// For ACP providers, sends `session/cancel`. For native providers,
    /// terminates the operation (process kill/termination sequence).
    async fn cancel(
        &self,
        session: &ManagedSessionHandle,
        op_id: HostOperationId,
    ) -> HostResult<()>;

    /// Shut down a managed session.
    ///
    /// Cancels any active operation, waits for graceful timeout, then
    /// force-terminates the provider process if needed.
    async fn shutdown(&self, session: ManagedSessionHandle) -> HostResult<()>;

    /// Return the current (possibly runtime-negotiated) capabilities.
    fn capabilities(&self) -> CapabilityDescriptor;
}

/// Host facade — the narrow interface consumed by the daemon runtime.
///
/// The daemon runtime stores `Arc<dyn HostFacade>` and calls methods
/// through this trait only. Provider modules are never imported directly.
#[async_trait]
pub trait HostFacade: Send + Sync {
    /// Start the host with the given configuration.
    ///
    /// Loads config, discovers providers, and prepares for session creation.
    async fn start(&self, config: HostStartConfig) -> HostResult<()>;

    /// Create a new managed session.
    ///
    /// Selects the provider, negotiates capabilities, and launches the session.
    async fn create_session(&self, request: CreateSessionRequest) -> HostResult<HostSession>;

    /// Execute an operation on a session.
    ///
    /// Returns a stream of `HostEvent` items with exactly one terminal event.
    async fn exec(
        &self,
        session_id: HostSessionId,
        op: HostOperation,
    ) -> HostResult<HostEventStream>;

    /// Cancel an active operation.
    ///
    /// Routes to the correct provider adapter based on session ownership.
    async fn cancel(&self, op_id: HostOperationId) -> HostResult<()>;

    /// Get host health status.
    async fn health(&self) -> HostResult<HostHealth>;

    /// Shut down the host and all managed sessions.
    ///
    /// Drains active operations with configured grace timeout.
    async fn shutdown(&self) -> HostResult<()>;
}

/// Provider discovery trait.
///
/// Discovers available providers from config, PATH scan, and ACP registry.
#[async_trait]
pub trait ProviderDiscovery: Send + Sync {
    /// Discover available providers based on the given configuration.
    ///
    /// Returns a catalog of discovered providers with their descriptors,
    /// capabilities, and health status.
    async fn discover(&self, config: &AgentHostConfig) -> HostResult<ProviderCatalog>;
}

/// A single entry in the provider catalog.
#[derive(Debug, Clone)]
pub struct ProviderCatalogEntry {
    /// Provider ID.
    pub provider_id: ProviderId,
    /// Human-readable display name.
    pub display_name: String,
    /// Protocol kind.
    pub protocol_kind: ProtocolKind,
    /// Launch strategy for this provider.
    pub launch: LaunchStrategy,
    /// How this provider was discovered.
    pub source: DiscoverySource,
    /// Trust level of this provider.
    pub trust: TrustLevel,
    /// Static capabilities.
    pub capabilities: CapabilityDescriptor,
    /// Health status (initially probed or unknown).
    pub health: ProviderHealth,
}

/// How a provider was discovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiscoverySource {
    /// Explicitly configured in `config.toml`.
    Config,
    /// Found via PATH scan.
    PathScan,
    /// Found via ACP registry catalog.
    AcpRegistry,
}

/// Trust level for a discovered provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustLevel {
    /// Explicitly configured by user.
    Explicit,
    /// From ACP registry (curated).
    Registry,
    /// Found on local PATH (lowest trust).
    LocalPath,
}

/// Launch strategy for a provider.
#[derive(Debug, Clone)]
pub enum LaunchStrategy {
    /// ACP provider: spawn + SDK connection.
    Acp {
        /// Command to execute.
        command: String,
        /// Command arguments.
        args: Vec<String>,
        /// Environment variables.
        env: std::collections::HashMap<String, String>,
    },
    /// Native CLI provider: managed subprocess.
    NativeCli {
        /// Command to execute.
        command: String,
        /// Command arguments.
        args: Vec<String>,
        /// Environment variables.
        env: std::collections::HashMap<String, String>,
    },
}
