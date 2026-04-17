//! Worker Manager + JSON-RPC IPC module.
//!
//! Provides:
//! - [`WorkerManager`] — spawn / supervise / shut down worker child processes.
//! - [`WorkerHandle`] — per-worker handle with IPC and shutdown.
//! - [`WorkerSpec`] — describes how to launch a worker.
//! - [`WorkerEvent`] — lifecycle events emitted via broadcast.
//! - [`RpcTransport`] — trait for NDJSON-framed transport.
//! - [`StdioTransport`] — concrete impl over child process pipes.
//! - [`DuplexTransport`] — in-memory mock for tests.
//! - [`call_json_rpc`] — convenience function for one-shot RPC calls.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §6.

pub mod ipc;
pub mod manager;
pub mod transport;

pub use manager::{WorkerError, WorkerEvent, WorkerHandle, WorkerManager, WorkerSpec};
pub use transport::{DuplexTransport, RpcTransport, StdioTransport};
