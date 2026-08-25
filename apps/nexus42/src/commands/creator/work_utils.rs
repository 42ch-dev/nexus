//! Shared Work-resolution utility for `creator` subcommands.
//!
//! Extracted from `run.rs` and `works/mod.rs` to eliminate the duplicate
//! `resolve_work_id` implementations (QC1 W-3). Hosts the shared
//! `query_path` URL builder (F-5) and the bounded file reader (qc3 S-002).

use crate::api::DaemonClient;
use crate::errors::{CliError, Result};

/// Build a daemon path with URL-encoded query pairs.
///
/// Shared house pattern (previously hand-rolled in `reading/mod.rs`,
/// `world/fork.rs`, `world/kb/daemon.rs`, and `rules_runtime.rs` — F-5):
/// parse a dummy base, set the real path, append query pairs, and splice
/// the encoded query back onto the path.
///
/// # Panics
///
/// Panics if `base` is not a valid URL path (it is always a literal
/// `/v1/daemon/...` string at the call sites).
#[must_use]
pub fn query_path(base: &str, pairs: &[(&str, &str)]) -> String {
    if pairs.is_empty() {
        return base.to_string();
    }
    let mut url = url::Url::parse("http://localhost").expect("valid base");
    url.set_path(base);
    for (key, value) in pairs {
        url.query_pairs_mut().append_pair(key, value);
    }
    let q = url.query().unwrap_or("");
    format!("{base}?{q}")
}

/// Read a file into a string with a client-side size cap (qc3 S-002).
///
/// Rejects an oversized input with a named CLI error *before* the read
/// (instead of unbounded `read_to_string`) so a `--content-file`/`--file`
/// pointed at a gigantic file by accident fails fast, matching the daemon's
/// preset/content caps.
///
/// # Errors
///
/// Returns a named `CliError::Other` when the file cannot be read or its
/// size exceeds `max_bytes`.
pub fn read_file_bounded(path: &str, max_bytes: usize, flag_name: &str) -> Result<String> {
    let meta = std::fs::metadata(path)
        .map_err(|e| CliError::Other(format!("cannot read {flag_name} '{path}': {e}")))?;
    let size = usize::try_from(meta.len())
        .map_err(|_| CliError::Other(format!("{flag_name} '{path}' is too large to read")))?;
    if size > max_bytes {
        return Err(CliError::Other(format!(
            "{flag_name} '{path}' is {size} bytes, exceeding the {max_bytes}-byte limit; \
             trim the file or inline the content with the text flag"
        )));
    }
    std::fs::read_to_string(path)
        .map_err(|e| CliError::Other(format!("cannot read {flag_name} '{path}': {e}")))
}

/// Resolve an optional `work_id` to a concrete ID.
///
/// If `work_id` is `Some(id)`, returns it directly.
/// If `work_id` is `None`, queries the daemon pool for the active Work
/// (`GET /v1/daemon/works?limit=1&status=active`) and returns its `work_id`.
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
        .get::<serde_json::Value>("/v1/daemon/works?limit=1&status=active")
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
