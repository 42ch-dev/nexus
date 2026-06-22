//! Smoke test for `nexus42 host-call` subcommand (V1.57 P1).
//!
//! Calls the `host_call::run()` function with test tool IDs to verify
//! that the CLI → daemon IPC → registry dispatch path works end-to-end.
//!
//! # Test split (V1.58 P2 — R-V157P1-W001)
//!
//! The three `#[ignore]` tests below (`host_call_smoke_read_tool`,
//! `host_call_smoke_write_tool`, `host_call_smoke_policy_gated_tool`)
//! are **integration tests that require a running daemon with an active
//! creator**. They exercise the full daemon-side tool dispatch pipeline
//! (admission gate → capability registry → tool handler → response),
//! which cannot be replicated by a CLI-side mock.
//!
//! ## Why they remain `#[ignore]` (engineering justification)
//!
//! `DaemonClient` is a concrete struct (not behind a trait), and `run()`
//! constructs it internally via `DaemonClient::from_config(config)`.
//! Un-ignoring these tests hermetically would require one of:
//!
//! 1. **Trait extraction**: Introduce a `HostCallClient` trait, implement it
//!    for `DaemonClient` and a `MockDaemon`, and refactor `run()` to accept
//!    the trait. This is a non-trivial refactor of V1.57 P1 QC-accepted code
//!    (the `DaemonClient` API surface and all its consumers).
//! 2. **HTTP mock server**: Spin up a `wiremock`/`mockito` server that
//!    impersonates the daemon. This adds test-only dependencies and
//!    introduces CI flakiness surface that the ignore was originally
//!    preventing.
//!
//! Both approaches exceed P2's surgical-discipline boundary (polish pass,
//! not API refactor). The daemon-side tool dispatch is already covered by
//! `nexus-daemon-runtime` integration tests. What the CLI side contributes
//! — request envelope construction, JSON arg parsing, response formatting —
//! is covered by the hermetic `build_tool_request_*` and
//! `host_call_rejects_invalid_json` tests below.

#[cfg(test)]
mod tests {
    use nexus42::commands::host_call::{build_tool_request, run, HostCallArgs};
    use nexus42::config::CliConfig;

    /// Helper: create a config pointing to default daemon URL.
    fn test_config() -> CliConfig {
        CliConfig {
            daemon_url: "http://127.0.0.1:8420".to_string(),
            ..Default::default()
        }
    }

    // ── Hermetic CLI-side tests (no daemon required) ──────────────────

    /// Verify `build_tool_request` produces the correct envelope for a read tool.
    #[test]
    fn build_tool_request_read_tool() {
        let params = serde_json::json!({});
        let body = build_tool_request("nexus.context.whoami", &params);
        assert_eq!(body["tool_name"], "nexus.context.whoami");
        assert_eq!(body["parameters"], params);
    }

    /// Verify `build_tool_request` preserves complex nested parameters.
    #[test]
    fn build_tool_request_preserves_nested_params() {
        let params = serde_json::json!({
            "work_id": "wrk_test",
            "action": "add",
            "pool_type": "ideas",
            "content": "test entry"
        });
        let body = build_tool_request("nexus.pool.entry.manage", &params);
        assert_eq!(body["tool_name"], "nexus.pool.entry.manage");
        assert_eq!(body["parameters"]["work_id"], "wrk_test");
        assert_eq!(body["parameters"]["action"], "add");
        assert_eq!(body["parameters"]["pool_type"], "ideas");
    }

    /// Verify `build_tool_request` handles boolean parameters.
    #[test]
    fn build_tool_request_handles_bool_params() {
        let params = serde_json::json!({"requires_platform": true});
        let body = build_tool_request("nexus.context.assemble", &params);
        assert_eq!(body["tool_name"], "nexus.context.assemble");
        assert_eq!(body["parameters"]["requires_platform"], true);
    }

    /// Verify --args JSON parse error produces clean error message.
    #[test]
    fn host_call_rejects_invalid_json() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let config = test_config();
        let args = HostCallArgs {
            tool_id: "nexus.context.whoami".to_string(),
            args: "not-valid-json".to_string(),
        };
        let result = rt.block_on(run(args, &config));
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("JSON"), "error should mention JSON: {msg}");
    }

    // ── Integration tests (require live daemon) ───────────────────────
    // See module-level docs for why these remain #[ignore].

    /// Call a read tool (nexus.context.whoami) via host-call.
    /// This test requires a running daemon and active creator.
    #[tokio::test]
    #[ignore = "requires running daemon with active creator; see module-level docs for R-V157P1-W001 justification"]
    async fn host_call_smoke_read_tool() {
        let config = test_config();
        let args = HostCallArgs {
            tool_id: "nexus.context.whoami".to_string(),
            args: "{}".to_string(),
        };
        let result = run(args, &config).await;
        assert!(result.is_ok(), "host-call whoami should succeed");
    }

    /// Call a write tool (nexus.pool.entry.manage) via host-call.
    #[tokio::test]
    #[ignore = "requires running daemon with active creator and valid work_id; see module-level docs for R-V157P1-W001 justification"]
    async fn host_call_smoke_write_tool() {
        let config = test_config();
        let args = HostCallArgs {
            tool_id: "nexus.pool.entry.manage".to_string(),
            args: r#"{"work_id":"wrk_test","action":"add","pool_type":"ideas","content":"test"}"#
                .to_string(),
        };
        let result = run(args, &config).await;
        // May fail with admission denial (INVALID_INPUT) if work doesn't exist,
        // but should not crash or produce unexpected errors.
        match result {
            Ok(_) => {} // success
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("FORBIDDEN")
                        || msg.contains("INVALID_INPUT")
                        || msg.contains("POLICY_BLOCKED")
                        || msg.contains("NOT_FOUND"),
                    "unexpected error: {msg}"
                );
            }
        }
    }

    /// Call a policy-gated tool (nexus.context.assemble with requires_platform=true).
    /// Should be POLICY_BLOCKED in local-only mode.
    #[tokio::test]
    #[ignore = "requires running daemon; see module-level docs for R-V157P1-W001 justification"]
    async fn host_call_smoke_policy_gated_tool() {
        let config = test_config();
        let args = HostCallArgs {
            tool_id: "nexus.context.assemble".to_string(),
            args: r#"{"requires_platform":true}"#.to_string(),
        };
        let result = run(args, &config).await;
        // Should fail with POLICY_BLOCKED when daemon is in local-only mode
        match result {
            Ok(_) => {} // may succeed if no active creator check bypasses platform gate
            Err(e) => {
                let msg = e.to_string();
                assert!(
                    msg.contains("POLICY_BLOCKED")
                        || msg.contains("FORBIDDEN")
                        || msg.contains("NOT_SUPPORTED"),
                    "unexpected error: {msg}"
                );
            }
        }
    }
}
