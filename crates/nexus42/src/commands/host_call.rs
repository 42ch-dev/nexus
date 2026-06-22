//! `nexus42 host-call` — low-level debugging entry for host tool execution.
//!
//! V1.57 P1: Adds a single debugging subcommand that walks the daemon
//! `host_tool` path: CLI → daemon IPC → `CapabilityRegistry::dispatch` → response.
//!
//! # Intent (debug-only)
//!
//! This subcommand is intended for **debugging and development only**.
//! It bypasses normal CLI UX layers (creator, workspace, preset) and
//! sends a raw tool request directly to the daemon's host tool executor.
//! Admission gates apply identically as for HTTP and worker caller paths.
//!
//! # Exit codes
//!
//! - `0`: tool executed successfully
//! - `1`: admission denied (`NOT_SUPPORTED`, `FORBIDDEN`, `POLICY_BLOCKED`)
//! - `2`: tool error or internal failure

use crate::api::daemon_client::DaemonClient;
use crate::config::CliConfig;
use crate::errors::{CliError, Result};
use clap::Args;
use std::process;

/// Host-call args — debug-only raw tool dispatch.
#[derive(Debug, Args)]
pub struct HostCallArgs {
    /// Tool ID to invoke (e.g. `"nexus.context.whoami"`, `"nexus.work.get"`)
    pub tool_id: String,

    /// Tool arguments as a `JSON` string (e.g. `'{"work_id":"wrk_abc"}'`)
    #[arg(short, long, default_value = "{}")]
    pub args: String,
}

/// Run the host-call subcommand.
///
/// # Errors
///
/// Returns `CliError` if:
/// - `--args` is not valid JSON
/// - The daemon is unreachable
/// - The tool request is denied or fails
pub async fn run(args: HostCallArgs, config: &CliConfig) -> Result<()> {
    let params: serde_json::Value = serde_json::from_str(&args.args)
        .map_err(|e| CliError::Other(format!("--args must be valid JSON: {e}")))?;

    let client = DaemonClient::from_config(config);
    let request_body = build_tool_request(&args.tool_id, &params);

    let response: serde_json::Value = client
        .post(
            "/v1/local/agent-host/internal/tool-executions",
            &request_body,
        )
        .await
        .map_err(|e| {
            CliError::Other(format!("host-call failed for tool '{}': {e}", args.tool_id))
        })?;

    // Print result as JSON
    let output = serde_json::to_string_pretty(&response)
        .map_err(|e| CliError::Other(format!("failed to serialize response: {e}")))?;

    println!("{output}");
    Ok(())
}

/// Build the JSON request body for a host-tool dispatch (V1.58 P2 — R-V157P1-W001).
///
/// Extracted from `run()` so the request-envelope construction is unit-testable
/// without a live daemon. The CLI-side contract is: given a tool ID and parsed
/// parameters, produce the wire request body that the daemon's
/// `/v1/local/agent-host/internal/tool-executions` endpoint expects.
#[must_use]
pub fn build_tool_request(tool_id: &str, params: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "tool_name": tool_id,
        "parameters": params,
    })
}

/// Convenience entry point that exits the process on error with an
/// appropriate exit code.
pub async fn run_or_exit(args: HostCallArgs, config: &CliConfig) {
    match run(args, config).await {
        Ok(()) => process::exit(0),
        Err(e) => {
            let code = if e.to_string().contains("NOT_SUPPORTED")
                || e.to_string().contains("FORBIDDEN")
                || e.to_string().contains("POLICY_BLOCKED")
            {
                1
            } else {
                2
            };
            eprintln!("{e}");
            process::exit(code);
        }
    }
}
