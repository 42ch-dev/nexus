//! nexus-agent-host — Hybrid managed-only host core for local agent execution.
//!
//! This crate provides the orchestration/facade layer above `nexus-acp-host` and
//! native CLI process adapters. It normalizes ACP providers and native CLI providers
//! behind narrow `ProviderAdapter` and `HostFacade` traits.
//!
//! # Architecture
//!
//! ```text
//! nexus42 CLI
//!   └─ nexus-daemon-runtime
//!        ├─ lifecycle subsystem: AgentHostSubsystem
//!        ├─ Axum routes: /v1/daemon/agent-host/*
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
// Test modules use ergonomic `.unwrap()`/`.expect()`; production keeps the strict deny.
#![cfg_attr(
    test,
    allow(clippy::unwrap_used, clippy::unwrap_in_result, clippy::expect_used)
)]

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
pub use core::{HostManager, HostSession, SessionRegistry, SessionState, TransitionResult};
pub use discovery::ProviderCatalog;
pub use policy::{AdmissionPolicy, HostPermissionResolver, PermissionOutcome};
pub use telemetry::TelemetryContext;

use capability::model::{
    CapabilityDescriptor, CreateSessionRequest, HostEventStream, HostHealth, HostOperation,
    HostStartConfig, LaunchSpec, ManagedSessionHandle, ProbeRequest, ProtocolKind,
    ProviderDescriptor, ProviderHealth,
};

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

    /// Shut down a single managed session by ID.
    ///
    /// The host remains running; only the specified session is stopped.
    /// Returns `HostError` if the session is not found.
    async fn shutdown_session(&self, session_id: HostSessionId) -> HostResult<()>;

    /// List all active managed sessions.
    ///
    /// Returns snapshot of sessions currently tracked by the host.
    async fn list_sessions(&self) -> HostResult<Vec<HostSession>>;

    /// Get the provider catalog.
    ///
    /// Returns provider entries discovered from config, PATH scan, and ACP registry.
    async fn provider_catalog(&self) -> HostResult<ProviderCatalog>;

    /// Subscribe to host events for a specific session.
    ///
    /// Returns a broadcast receiver yielding [`HostEvent`] items.
    /// The SSE endpoint consumes this to relay real-time events to clients.
    fn subscribe_events(
        &self,
        session_id: HostSessionId,
    ) -> tokio::sync::broadcast::Receiver<capability::model::HostEvent>;
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

#[cfg(test)]
pub(crate) mod test_support {
    //! Shared serialization for tests that touch the process-global
    //! environment (see [`PROCESS_ENV_LOCK`]).

    /// Serializes tests that mutate the process-global environment with
    /// tests that spawn fixture children.
    ///
    /// On Unix, fixture shebangs resolve `python3` through process PATH at
    /// spawn time. On Windows, `fixture_launch` generates a `.cmd` wrapper
    /// using an absolute interpreter; it also isolates fixture spawns from
    /// `discovery::path_scan` tests that replace process `PATH` /
    /// `DSH_RUNTIME_BIN` (`PathGuard` / `DshEnvGuard`). Otherwise fixtures
    /// fail before producing output, surfaced as flaky `stream_closed` /
    /// missing-REQ_LOG failures. Every env-mutating test AND every
    /// python-spawning provider test takes this lock so the groups never
    /// overlap.
    ///
    /// Async-aware (`tokio::sync::Mutex`) so tests that must keep the lock
    /// across `.await` points do not hold a blocking guard: async tests take
    /// it with `.lock().await`, sync `#[test]` fns with `.blocking_lock()`.
    pub static PROCESS_ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    /// Directory holding the Windows `fixture_launch` `.cmd` shims.
    ///
    /// Statics are never dropped, so a `TempDir` in a static never removes its
    /// directory. A fixed path reuses one directory across test processes and
    /// the first launch sweeps it ([`sweep_shims`]), so leftover wrappers do
    /// not accumulate. A CRT `atexit` hook would remove it at exit instead, but
    /// that FFI call is `unsafe` and the workspace lints forbid `unsafe_code`.
    ///
    /// shortcut: the directory survives the process until the next run's sweep,
    /// and two test processes sharing one temp dir would sweep each other's
    /// shims — upgrade if that ever runs on one machine.
    #[cfg(windows)]
    static SHIM_DIR: std::sync::LazyLock<std::path::PathBuf> = std::sync::LazyLock::new(|| {
        let dir = std::env::temp_dir().join("nexus-agent-host-fixture-shims");
        if let Some(diagnostic) = sweep_shims(&dir) {
            tracing::warn!("{diagnostic}");
        }
        dir
    });

    /// Remove a shim directory, returning one diagnostic line when the
    /// removal failed for any reason other than the directory being absent.
    ///
    /// Never panics and never locks. The failure is returned rather than
    /// printed here so it can be pinned by a test: a stale `.cmd` wrapper
    /// surviving at a reused path would otherwise be spawned by a later
    /// `fixture_launch` while the sweep looked clean. `remove_dir_all` reports
    /// the first entry it could not delete.
    #[cfg(windows)]
    fn sweep_shims(dir: &std::path::Path) -> Option<String> {
        sweep_diagnostic(dir, std::fs::remove_dir_all(dir))
    }

    #[cfg(windows)]
    fn sweep_diagnostic(dir: &std::path::Path, removed: std::io::Result<()>) -> Option<String> {
        match removed {
            Ok(()) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => Some(format!(
                "fixture shim sweep failed for {}: {error}",
                dir.display()
            )),
        }
    }

    /// Write the `.cmd` wrapper for `fixture` into `shim_dir`, creating the
    /// directory when it is missing (a previous sweep may have removed it).
    #[cfg(windows)]
    fn write_shim(shim_dir: &std::path::Path, fixture: &std::path::Path) -> std::path::PathBuf {
        std::fs::create_dir_all(shim_dir).expect("recreate fixture shim dir");
        let interpreter = python_path();
        let name = fixture
            .file_stem()
            .expect("fixture filename")
            .to_string_lossy();
        let shim = shim_dir.join(format!("{name}.cmd"));
        let body = format!(
            "@echo off\r\n\"{}\" \"{}\" %*\r\n",
            interpreter.display(),
            fixture.display()
        );
        std::fs::write(&shim, body).expect("write fixture shim");
        shim
    }

    /// Resolve a Python fixture to a program the current platform can spawn.
    /// Callers must hold `PROCESS_ENV_LOCK` throughout construction: on
    /// Windows, PATH-dependent `python_path()` discovery happens before this
    /// function returns, not only when the fixture is spawned.
    pub fn fixture_launch(fixture: &str) -> std::path::PathBuf {
        let fixture = std::path::Path::new(fixture);
        #[cfg(unix)]
        {
            fixture.to_path_buf()
        }
        #[cfg(windows)]
        {
            use std::sync::Mutex;
            static SHIM_LOCK: Mutex<()> = Mutex::new(());
            let _guard = SHIM_LOCK.lock().expect("fixture shim lock");
            write_shim(&SHIM_DIR, fixture)
        }
    }

    #[cfg(windows)]
    fn python_path() -> std::path::PathBuf {
        use std::sync::LazyLock;
        static PYTHON: LazyLock<std::path::PathBuf> = LazyLock::new(|| {
            for (program, args) in [
                ("py", vec!["-3", "-c", "import sys; print(sys.executable)"]),
                ("python", vec!["-c", "import sys; print(sys.executable)"]),
            ] {
                if let Ok(output) = std::process::Command::new(program).args(args).output() {
                    if output.status.success() {
                        if let Ok(path) = String::from_utf8(output.stdout) {
                            let path = std::path::PathBuf::from(path.trim());
                            if path.is_absolute() {
                                return path;
                            }
                        }
                    }
                }
            }
            panic!("Python is unavailable through `py -3` and `python`");
        });
        PYTHON.clone()
    }

    /// Pins the shim lifecycle: `fixture_launch` writes into `SHIM_DIR`, a
    /// sweep removes a shim directory, and the next write recreates it.
    ///
    /// The sweep is exercised on a private directory, never on `SHIM_DIR`:
    /// other tests capture a shim path at provider construction and spawn it
    /// later, so deleting the shared directory here would strand them. The
    /// once-per-process sweep inside `SHIM_DIR`'s initializer cannot be
    /// asserted from within the same process anyway (it runs on first use).
    #[cfg(windows)]
    #[test]
    fn fixture_shims_survive_sweep_through_recreation() {
        let fixture = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/native_protocol/mock_claude_cli.py"
        );
        let _guard = PROCESS_ENV_LOCK.blocking_lock();
        let shim = fixture_launch(fixture);
        assert!(
            shim.starts_with(&*SHIM_DIR),
            "shim outside the shim dir: {}",
            shim.display()
        );
        assert!(shim.is_file(), "shim not written: {}", shim.display());

        let private = tempfile::tempdir().expect("temp dir");
        let shims = private.path().join("shims");
        let shim = write_shim(&shims, std::path::Path::new(fixture));
        assert!(shim.is_file(), "shim not written: {}", shim.display());
        assert!(
            sweep_shims(&shims).is_none(),
            "a removable shim dir must not report a sweep failure"
        );
        assert!(!shims.exists(), "sweep left the shim dir behind");
        let shim = write_shim(&shims, std::path::Path::new(fixture));
        assert!(shim.is_file(), "write did not recreate: {}", shim.display());
    }

    /// Pins that a failed sweep is reported instead of swallowed: an
    /// undeletable entry (simulated as a permission error, e.g. a wrapper
    /// another process still holds open) yields one diagnostic line naming
    /// the directory, while an absent directory stays silent.
    #[cfg(windows)]
    #[test]
    fn sweep_reports_removal_failure() {
        let dir = std::path::Path::new(r"C:\Temp\nexus-agent-host-fixture-shims");
        let denied = std::io::Error::from(std::io::ErrorKind::PermissionDenied);
        let diagnostic = sweep_diagnostic(dir, Err(denied)).expect("failure must be reported");
        assert!(
            diagnostic.contains(&dir.display().to_string()),
            "diagnostic omits the shim dir: {diagnostic}"
        );
        assert!(
            diagnostic.contains("denied"),
            "diagnostic omits the cause: {diagnostic}"
        );
        assert!(
            sweep_diagnostic(dir, Err(std::io::ErrorKind::NotFound.into())).is_none(),
            "an absent shim dir is a clean sweep"
        );
        assert!(sweep_diagnostic(dir, Ok(())).is_none());
    }
}
