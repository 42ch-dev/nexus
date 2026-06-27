//! Shared Work-resolution utility for `creator` subcommands.
//!
//! Extracted from `run.rs` and `works/mod.rs` to eliminate the duplicate
//! `resolve_work_id` implementations (QC1 W-3).

use crate::api::DaemonClient;
use crate::errors::{CliError, Result};

/// Resolve an optional `work_id` to a concrete ID.
///
/// If `work_id` is `Some(id)`, returns it directly.
/// If `work_id` is `None`, queries the daemon pool for the active Work
/// (`GET /v1/local/works?limit=1&status=active`) and returns its `work_id`.
///
/// # Errors
///
/// Returns [`CliError::Config`] if `work_id` was `None` and no active Work
/// exists in the pool.
pub async fn resolve_active_work_id(
    client: &DaemonClient,
    work_id: Option<String>,
) -> Result<String> {
    if let Some(id) = work_id {
        return Ok(id);
    }
    let resp: serde_json::Value = client
        .get::<serde_json::Value>("/v1/local/works?limit=1&status=active")
        .await?;
    resp.get("works")
        .and_then(|v| v.as_array())
        .and_then(|arr| arr.first())
        .and_then(|w| w.get("work_id"))
        .and_then(|v| v.as_str())
        .map(String::from)
        .ok_or_else(|| {
            CliError::Config(
                "No active Work found. Specify <work_id> or run \
                 `nexus42 creator works use <work_id>`."
                    .to_string(),
            )
        })
}
