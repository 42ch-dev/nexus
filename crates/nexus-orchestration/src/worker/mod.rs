//! Worker Manager + JSON-RPC IPC module.
//!
//! Provides:
//! - [`WorkerManager`] — spawn / supervise / shut down worker child processes.
//! - [`WorkerHandle`] — per-worker handle with IPC and shutdown.
//! - [`WorkerSpec`] — describes how to launch a worker.
//! - [`WorkerAgentConfig`] — configuration for one agent slot (WS-E T4).
//! - [`WorkerEvent`] — lifecycle events emitted via broadcast.
//! - [`IpcClient`] — persistent, multiplexed JSON-RPC client.
//! - [`RpcTransport`] — trait for NDJSON-framed transport (combined).
//! - [`RpcTransportRead`] / [`RpcTransportWrite`] — split transport halves.
//! - [`StdioTransport`] — concrete impl over child process pipes.
//! - [`DuplexTransport`] — in-memory mock for tests.
//! - [`call_json_rpc`] — convenience function for one-shot RPC calls.
//!
//! Design: `.mstar/knowledge/specs/orchestration-engine.md` §6.

pub mod ipc;
pub mod manager;
pub mod registry;
pub mod transport;

pub use ipc::{IpcClient, IpcError};
pub use manager::{
    AgentSessionSummary, WorkerAgentConfig, WorkerError, WorkerEvent, WorkerHandle, WorkerManager,
    WorkerSpec,
};
pub use registry::{MockSpawner, WorkerManagerSpawner, WorkerRegistry, WorkerSpawner};
pub use transport::{
    DuplexTransport, RpcTransport, RpcTransportRead, RpcTransportWrite, StdioTransport,
};
