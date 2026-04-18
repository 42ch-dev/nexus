//! JSON-RPC client over an [`RpcTransport`] using `jsonrpsee-core` types.
//!
//! Framing is NDJSON via `LinesCodec` on both directions (handled by the
//! transport layer). This module provides [`call_json_rpc`] which:
//! 1. Constructs a JSON-RPC 2.0 request with a numeric `id`.
//! 2. Sends it via the transport.
//! 3. Awaits the matching response keyed by that `id`.
//! 4. Enforces a default 30-second timeout.
//!
//! A full `IpcClient` with pending-request tracking, notifications, and
//! `CancellationToken` support will be layered on top in WS3 when the
//! message catalogue expands beyond the three WS2 methods.
//!
//! Design: `.agents/plans/knowledge/orchestration-engine-v1.md` §6.4.

use crate::worker::transport::RpcTransport;
use serde_json::Value;
use thiserror::Error;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

/// Errors from JSON-RPC calls over the IPC transport.
#[derive(Error, Debug)]
pub enum IpcError {
    #[error("transport error: {0}")]
    Transport(#[from] std::io::Error),
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
// One-shot call_json_rpc helpers
// ---------------------------------------------------------------------------

/// Send a JSON-RPC request and await the response.
///
/// Uses a 30-second default timeout.
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
            transport.send(request_str).await?;

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
