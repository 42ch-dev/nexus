//! Smoke test for `nexus42 host-call` subcommand (V1.57 P1).
//!
//! Calls the `host_call::run()` function with test tool IDs to verify
//! that the CLI → daemon IPC → registry dispatch path works end-to-end.

#[cfg(test)]
mod tests {
    use nexus42::commands::host_call::{run, HostCallArgs};
    use nexus42::config::CliConfig;

    /// Helper: create a config pointing to default daemon URL.
    fn test_config() -> CliConfig {
        CliConfig {
            daemon_url: "http://127.0.0.1:8420".to_string(),
            ..Default::default()
        }
    }

    /// Call a read tool (nexus.context.whoami) via host-call.
    /// This test requires a running daemon and active creator.
    #[tokio::test]
    #[ignore = "requires running daemon with active creator"]
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
    #[ignore = "requires running daemon with active creator and valid work_id"]
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
    #[ignore = "requires running daemon"]
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
}
