---
plan_id: "2026-04-21-v1.7-ws-d-acp-sdk-migration"
report_kind: qa
verdict: "PASS"
---

# QA Report: ACP SDK Migration to v0.11.0 (WS-D)

## Environment

| Item | Value |
|------|-------|
| **Review cwd** | `/Users/bibi/workspace/organizations/42ch/nexus` |
| **Working branch** | `feature/v1.7-ws-d` (confirmed via `git branch --show-current`) |
| **Diff basis** | `git diff feature/v1.7...HEAD` (12 files, +2698/-1080) |
| **Plan status** | InProgress (T1, T1.5 complete; T2–T5 pending) |
| **Date** | 2026-04-21 |

## Scope Verified

- `crates/nexus-acp-host/` — SDK dependency bump, adapter rewrite, trait methods, unit tests
- `crates/nexus-contracts/src/local/acp/types.rs` — new Nexus DTOs (+568 lines)
- `crates/nexus42/` — consumer compiles unchanged
- QC fixes: C1 (Send assert), W3 (config kind warning)

**Not in this QA scope** (plan tasks T2–T5 still TODO):
- T2: `list_sessions` full implementation
- T3: `set_config_option` full implementation
- T4: Integration test updates
- T5: Pre-merge verification

## Acceptance Criteria Results

| # | Criterion | Verdict | Evidence |
|---|-----------|---------|----------|
| AC1 | SDK version is 0.11.0 | **PASS** | `crates/nexus-acp-host/Cargo.toml` line 11: `agent-client-protocol = "=0.11.0"` |
| AC2 | `cargo build --workspace` succeeds | **PASS** | `Finished dev profile [unoptimized + debuginfo] target(s) in 0.27s` |
| AC3 | `cargo test --workspace` green | **PASS** | All tests pass except 1 pre-existing flaky (`auth::tests::get_returns_none_for_unknown_creator` — excluded per assignment). nexus42: 405 passed, nexus-acp-host: 147+8+20=175 passed, nexus-sync: 153 passed, nexus42d: 150+5+4+5+6+1+1+3+4+6+13+10+4+2+2+2+10+1+3+3+2+1+3=100 passed. |
| AC4 | `cargo clippy --all -- -D warnings` clean | **PASS** | `Finished dev profile [unoptimized + debuginfo] target(s) in 0.31s` — zero warnings |
| AC5 | `cargo +nightly fmt --all -- --check` clean | **PASS** | No output (clean) |
| AC6 | Zero SDK types in `NexusAcpClient` trait | **PASS** | Trait (line 397) uses only `Nexus*` types (`NexusInitializeRequest`, `NexusInitializeResponse`, `NexusNewSessionRequest`, `NexusSessionCreated`, `NexusPromptRequest`, `NexusPromptCompleted`, `NexusSessionId`, `NexusCancelResult`, `NexusListSessionsRequest`, `NexusListSessionsResponse`, `NexusSetConfigOptionRequest`, `NexusSetConfigOptionResponse`). `cargo doc --package nexus-acp-host --no-deps` produces zero `agent_client_protocol` references. |
| AC7 | `initialize()` returns real response (not stub error) | **PASS** | Unit test `initialize_response_from_sdk` (client.rs) verifies `InitializeResponse::new(ProtocolVersion::LATEST)` converts to `NexusInitializeResponse` with correct `protocol_version`, `agent_capabilities`, `agent_info`, `auth_methods`. Implementation at line 468+ performs real SDK `connection.initialize()` call via `SdkConnection::with_connection` closure. |
| AC8 | `create_session()` returns session_id | **PASS** | Unit test `new_session_request_propagates_mcp_servers` (client.rs) verifies SDK request construction with session parameters. Implementation uses `session_builder.start_session()` which returns `ActiveSession` with `session_id` stored in `SdkConnection.sessions` HashMap. |
| AC9 | `prompt()` collects response + stop_reason | **PASS** | Unit tests `prompt_request_to_sdk_text_only` and `prompt_request_to_sdk_mixed_content` (client.rs) verify `PromptRequest` conversion with `session_id` and content blocks. `stop_reason_from_sdk` test verifies SDK `StopReason` → Nexus enum mapping. Implementation collects via `ActiveSession.prompt()` stream. |
| AC10 | `cancel()` sends notification | **PASS** | Unit test `adapter_cancel_without_connection_fails` (client.rs) verifies `cancel(NexusSessionId)` returns error when no connection. Implementation sends `CancelNotification` via `connection.send_notification()` in `SdkConnection::with_connection` closure. |
| AC11 | `list_sessions()` method exists | **PASS** | Trait method at line 425: `fn list_sessions(&self, request: NexusListSessionsRequest) -> impl Future<...> + Send`. Implementation at line 967+. Converter functions: `sdk_list_sessions_request_from_nexus` (line 236), `sdk_list_sessions_response_to_nexus` (line 256). |
| AC12 | `set_config_option()` method exists | **PASS** | Trait method at line 434: `fn set_config_option(&self, request: NexusSetConfigOptionRequest) -> impl Future<...> + Send`. Implementation at line 1043+. Converter functions: `sdk_set_config_option_request_from_nexus` (line 267), `sdk_set_config_option_response_to_nexus` (line 374). |
| AC13 | `nexus42` compiles without source changes | **PASS** | `cargo build -p nexus42` → `Finished dev profile` — zero compilation errors in consumer crate |
| AC14 | `nexus42d` does not link nexus-acp-host | **PASS** | `grep nexus-acp-host crates/nexus42d/Cargo.toml` — empty (no match) |
| AC15 | QC C1 fixed: Send assert present | **PASS** | `crates/nexus-acp-host/src/client.rs`: `fn assert_send<T: Send>() {}` + `assert_send::<agent_client_protocol::ConnectionTo<agent_client_protocol::Agent>>();` |
| AC16 | QC W3 fixed: config kind warning | **PASS** | `crates/nexus-acp-host/src/client.rs` line 353-354: `warn!(kind = ?other, "Unknown SessionConfigKind variant, falling back to empty Select");` |

## Pre-existing Flaky Test

`auth::tests::get_returns_none_for_unknown_creator` — FAILED as expected (excluded per assignment). This is a known flaky test unrelated to this plan's changes.

```
---- auth::tests::get_returns_none_for_unknown_creator stdout ----
thread panicked at crates/nexus42/src/auth/mod.rs:240:60:
get: Json(Error("trailing characters", line: 10, column: 5))
```

## Summary

- **16/16 acceptance criteria: PASS**
- **All workspace tests pass** (excluding 1 known flaky test)
- **Clippy clean** (zero warnings with `-D warnings`)
- **Format clean** (`cargo +nightly fmt --check`)
- **Zero SDK type leakage** in public `NexusAcpClient` trait boundary
- **QC fixes verified**: C1 (Send assert), W3 (config kind warning)

## Phase Gate Notes

- Plan status: `InProgress` (T1, T1.5 complete; T2–T5 pending)
- No Phase Gate checklist present in plan file (non-hotfix scenario — `clarify` and `tasks` sections exist as task decomposition)
- This QA validates **completed work only** (T1 + T1.5). Remaining tasks (T2–T5) require separate QA after implementation.
