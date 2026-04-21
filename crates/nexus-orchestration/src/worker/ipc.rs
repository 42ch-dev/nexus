//! JSON-RPC client over an [`RpcTransport`] using `jsonrpsee-core` types.
//!
//! Framing is NDJSON via `LinesCodec` on both directions (handled by the
//! transport layer). This module provides:
//!
//! - [`IpcClient`] — persistent, multiplexed client that supports concurrent
//!   in-flight requests via a background reader task and pending-request map.
//! - [`call_json_rpc`] and friends — backward-compatible one-shot helpers
//!   (deprecated in favour of `IpcClient`).
//!
//! ## Design
//!
//! `IpcClient` splits the transport into separate read and write halves:
//! - The **write half** is shared via `Arc<Mutex<>>` so that multiple callers
//!   can send requests concurrently.
//! - The **read half** is owned exclusively by the background reader task.
//!
//! Responses are routed to the correct pending caller via the JSON-RPC `id`.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §6.4.

use crate::worker::transport::{RpcTransport, RpcTransportRead, RpcTransportWrite};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{oneshot, Mutex};
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

// ---------------------------------------------------------------------------
// Type alias for clippy::type_complexity
// ---------------------------------------------------------------------------

/// Map from JSON-RPC request `id` to the oneshot channel that delivers
/// the parsed response (or error) back to the caller.
type PendingMap = HashMap<u64, oneshot::Sender<Result<Value, IpcError>>>;

/// Shared pending map behind an async Mutex.
type SharedPending = Arc<Mutex<PendingMap>>;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from JSON-RPC calls over the IPC transport.
#[derive(Debug, thiserror::Error)]
pub enum IpcError {
    #[error("transport error: {0}")]
    Transport(String),
    #[error("response parse error: {0}")]
    ParseError(String),
    #[error("JSON-RPC error response: {code} — {message}")]
    RpcError { code: i64, message: String },
    #[error("request timed out after {timeout_ms}ms")]
    Timeout { timeout_ms: u64 },
    #[error("transport closed (EOF)")]
    Eof,
    #[error("cancelled")]
    Cancelled,
    #[error("internal: {0}")]
    Internal(String),
}

// ---------------------------------------------------------------------------
// Default timeout
// ---------------------------------------------------------------------------

/// Default request timeout: 30 seconds.
const DEFAULT_TIMEOUT_MS: u64 = 30_000;

// ---------------------------------------------------------------------------
// IpcClient — persistent, multiplexed JSON-RPC client
// ---------------------------------------------------------------------------

/// Persistent JSON-RPC client that supports concurrent in-flight requests.
///
/// A background reader task continuously reads NDJSON lines from the transport
/// and dispatches responses to the correct pending caller via the JSON-RPC `id`.
///
/// # Concurrency
///
/// Multiple callers can call [`IpcClient::call`] concurrently. The transport
/// write side is serialised via an internal `Mutex`. The reader task is the
/// sole consumer of the read half — no locking needed.
///
/// # Shutdown
///
/// Dropping the `IpcClient` (or calling [`IpcClient::close`]) cancels the
/// background reader task via a [`CancellationToken`] and aborts all pending
/// requests with [`IpcError::Cancelled`].
pub struct IpcClient {
    /// The transport write half (shared via Mutex for concurrent sends).
    writer: Arc<Mutex<Box<dyn RpcTransportWrite>>>,
    /// Pending requests keyed by JSON-RPC `id`.
    pending: SharedPending,
    /// Monotonically increasing request ID.
    next_id: AtomicU64,
    /// Cancellation token — fires on close/drop, cancels the reader task.
    cancel: CancellationToken,
    /// Handle to the background reader task.
    reader_task: Option<tokio::task::JoinHandle<()>>,
}

impl IpcClient {
    /// Create a new `IpcClient` from a combined transport.
    ///
    /// The transport is split into read and write halves internally.
    /// A background reader task is spawned to dispatch responses.
    pub fn new(transport: Box<dyn RpcTransport>) -> Self {
        let (reader, writer) = transport.split();
        Self::from_split(reader, writer)
    }

    /// Create a new `IpcClient` from pre-split transport halves.
    ///
    /// This is useful when the caller already has separate read/write halves.
    pub fn from_split(
        mut reader: Box<dyn RpcTransportRead>,
        writer: Box<dyn RpcTransportWrite>,
    ) -> Self {
        let cancel = CancellationToken::new();
        let pending: SharedPending = Arc::new(Mutex::new(HashMap::new()));
        let writer = Arc::new(Mutex::new(writer));

        let reader_cancel = cancel.clone();
        let reader_pending = pending.clone();

        let reader_task = tokio::spawn(async move {
            loop {
                tokio::select! {
                    _ = reader_cancel.cancelled() => {
                        debug!("IpcClient reader loop cancelled");
                        break;
                    }
                    result = reader.recv() => {
                        match result {
                            Some(line) => {
                                dispatch_response(&line, &reader_pending).await;
                            }
                            None => {
                                debug!("IpcClient reader loop: EOF, transport closed");
                                break;
                            }
                        }
                    }
                }
            }

            // Drain remaining pending requests — they will never be answered.
            let mut map = reader_pending.lock().await;
            for (_, tx) in map.drain() {
                let _ = tx.send(Err(IpcError::Eof));
            }
        });

        Self {
            writer,
            pending,
            next_id: AtomicU64::new(1),
            cancel,
            reader_task: Some(reader_task),
        }
    }

    /// Send a JSON-RPC request and await the response.
    ///
    /// This method is safe to call concurrently from multiple tasks. Each call
    /// gets a unique `id` and its own `oneshot` channel for the response.
    ///
    /// Uses the default 30-second timeout.
    pub async fn call(&self, method: &str, params: Value) -> Result<Value, IpcError> {
        self.call_with_timeout(
            method,
            params,
            std::time::Duration::from_millis(DEFAULT_TIMEOUT_MS),
        )
        .await
    }

    /// Send a JSON-RPC request with a custom timeout.
    pub async fn call_with_timeout(
        &self,
        method: &str,
        params: Value,
        timeout: std::time::Duration,
    ) -> Result<Value, IpcError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);

        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });

        let request_str = serde_json::to_string(&request)
            .map_err(|e| IpcError::Internal(format!("serialize request: {e}")))?;

        let (tx, rx) = oneshot::channel();

        // Register the pending sender.
        {
            let mut map = self.pending.lock().await;
            map.insert(id, tx);
        }

        // Send the request over the transport write half.
        {
            let mut writer = self.writer.lock().await;
            writer
                .send(request_str)
                .await
                .map_err(|e| IpcError::Transport(e.to_string()))?;
        }

        // Await the response with timeout and cancellation.
        tokio::select! {
            _ = self.cancel.cancelled() => {
                // Clean up pending entry.
                let mut map = self.pending.lock().await;
                map.remove(&id);
                Err(IpcError::Cancelled)
            }
            result = tokio::time::timeout(timeout, rx) => {
                match result {
                    Ok(Ok(response)) => response,
                    Ok(Err(_)) => {
                        // oneshot sender dropped — response never dispatched.
                        // This typically means the reader task ended (EOF).
                        let mut map = self.pending.lock().await;
                        map.remove(&id);
                        Err(IpcError::Eof)
                    }
                    Err(_) => {
                        // Timeout.
                        let mut map = self.pending.lock().await;
                        map.remove(&id);
                        Err(IpcError::Timeout { timeout_ms: timeout.as_millis() as u64 })
                    }
                }
            }
        }
    }

    /// Send a JSON-RPC notification (no `id`, no response expected).
    ///
    /// Notifications are "fire and forget" — the server should not reply.
    pub async fn notify(&self, method: &str, params: Value) -> Result<(), IpcError> {
        let request = serde_json::json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });

        let request_str = serde_json::to_string(&request)
            .map_err(|e| IpcError::Internal(format!("serialize notification: {e}")))?;

        let mut writer = self.writer.lock().await;
        writer
            .send(request_str)
            .await
            .map_err(|e| IpcError::Transport(e.to_string()))?;

        Ok(())
    }

    /// Cancel all pending requests and stop the background reader task.
    ///
    /// After calling `close`, any subsequent `call` or `notify` will return
    /// [`IpcError::Cancelled`] or [`IpcError::Transport`].
    pub async fn close(&mut self) {
        self.cancel.cancel();

        // Abort all pending senders so callers don't hang.
        {
            let mut map = self.pending.lock().await;
            map.drain().for_each(|(_, tx)| {
                let _ = tx.send(Err(IpcError::Cancelled));
            });
        }

        if let Some(handle) = self.reader_task.take() {
            let _ = handle.await;
        }
    }

    /// Return a clone of the cancellation token.
    ///
    /// Callers can use this to detect when the client is shutting down.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// Check whether the client has been closed.
    pub fn is_closed(&self) -> bool {
        self.cancel.is_cancelled()
    }
}

impl Drop for IpcClient {
    fn drop(&mut self) {
        self.cancel.cancel();
        if let Some(handle) = self.reader_task.take() {
            handle.abort();
        }
    }
}

// ---------------------------------------------------------------------------
// Response dispatch helper
// ---------------------------------------------------------------------------

/// Parse a JSON-RPC response line and dispatch it to the correct pending caller.
async fn dispatch_response(line: &str, pending: &SharedPending) {
    let val: Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => {
            warn!(error = %e, "failed to parse JSON-RPC response line");
            return;
        }
    };

    // Extract the `id` field.
    let id = match val.get("id").and_then(|v| v.as_u64()) {
        Some(id) => id,
        None => {
            // Could be a notification from the server — ignore.
            debug!("received JSON-RPC message without id, ignoring");
            return;
        }
    };

    // Build the result.
    let result = if let Some(error) = val.get("error") {
        let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
        let message = error
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("unknown error")
            .to_string();
        Err(IpcError::RpcError { code, message })
    } else {
        Ok(val.get("result").cloned().unwrap_or(Value::Null))
    };

    // Remove from pending and send.
    let mut map = pending.lock().await;
    if let Some(tx) = map.remove(&id) {
        let _ = tx.send(result);
    } else {
        debug!(id, "received response for unknown request id");
    }
}

// ---------------------------------------------------------------------------
// Backward-compatible one-shot call_json_rpc helpers
// ---------------------------------------------------------------------------

/// Send a JSON-RPC request and await the response.
///
/// Uses a 30-second default timeout.
///
/// **Note:** For concurrent multiplexed requests, prefer [`IpcClient`].
pub async fn call_json_rpc(
    transport: &mut dyn RpcTransport,
    method: &str,
    params: Value,
) -> Result<Value, IpcError> {
    call_json_rpc_with_timeout_and_cancel(
        transport,
        method,
        params,
        std::time::Duration::from_secs(30),
        &CancellationToken::new(),
    )
    .await
}

/// Send a JSON-RPC request with explicit timeout.
pub async fn call_json_rpc_with_timeout(
    transport: &mut dyn RpcTransport,
    method: &str,
    params: Value,
    timeout: std::time::Duration,
) -> Result<Value, IpcError> {
    call_json_rpc_with_timeout_and_cancel(
        transport,
        method,
        params,
        timeout,
        &CancellationToken::new(),
    )
    .await
}

/// Send a JSON-RPC request with explicit timeout and cancellation token.
pub async fn call_json_rpc_with_timeout_and_cancel(
    transport: &mut dyn RpcTransport,
    method: &str,
    params: Value,
    timeout: std::time::Duration,
    cancel: &CancellationToken,
) -> Result<Value, IpcError> {
    let request = serde_json::json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params,
    });
    let request_str = serde_json::to_string(&request)
        .map_err(|e| IpcError::Internal(format!("serialize request: {e}")))?;

    tokio::select! {
        _ = cancel.cancelled() => {
            Err(IpcError::Cancelled)
        }
        result = async {
            transport
                .send(request_str)
                .await
                .map_err(|e| IpcError::Transport(e.to_string()))?;

            let response_str = tokio::time::timeout(timeout, transport.recv())
                .await
                .map_err(|_| IpcError::Timeout { timeout_ms: timeout.as_millis() as u64 })?
                .ok_or(IpcError::Eof)?;

            let val: Value = serde_json::from_str(&response_str)
                .map_err(|e| IpcError::ParseError(e.to_string()))?;

            // Check for JSON-RPC error response.
            if let Some(error) = val.get("error") {
                let code = error.get("code").and_then(|c| c.as_i64()).unwrap_or(-1);
                let message = error
                    .get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("unknown error")
                    .to_string();
                return Err(IpcError::RpcError { code, message });
            }

            Ok(val.get("result").cloned().unwrap_or(Value::Null))
        } => {
            result
        }
    }
}
