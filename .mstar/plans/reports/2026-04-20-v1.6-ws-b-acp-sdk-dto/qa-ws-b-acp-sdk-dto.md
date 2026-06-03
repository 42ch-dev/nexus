# QA Report: V1.6 WS-B — ACP SDK DTO Decoupling

**Plan ID**: `2026-04-20-v1.6-ws-b-acp-sdk-dto`
**Branch**: `feature/v1.6`
**Diff range**: `47bfc51..95ce3d2`
**Date**: 2026-04-20

## Scope Tested

All 5 verification commands from Assignment, plus acceptance criteria checklist.

## Verification Commands

### 1. `cargo test --workspace` — ⚠️ 1 pre-existing failure

```
nexus-acp-host: 135 passed; 0 failed; 1 ignored
nexus-sync:     153 passed; 0 failed; 0 ignored
nexus42:        387 passed; 1 failed; 0 ignored
```

**Failing test**: `auth::tests::get_returns_none_for_unknown_creator`
- Error: `Json(Error("trailing characters", line: 10, column: 2))` at `crates/nexus42/src/auth/mod.rs:240`
- **Pre-existing**: Also fails at base commit `47bfc51` (WS-A Done snapshot). Zero auth module changes in `47bfc51..95ce3d2` diff. **Not introduced by WS-B.**

### 2. `cargo clippy --all -- -D warnings` — ✅ PASS

No warnings, no errors. Clean exit.

### 3. `cargo +nightly fmt --all -- --check` — ✅ PASS

No output (no formatting differences).

### 4. `rg "agent.client.protocol" crates/nexus-orchestration/ crates/nexus42/` — ✅ PASS (zero code matches)

Only 2 matches found, both in **comments**:
- `crates/nexus42/Cargo.toml` — comment noting the dependency was REMOVED
- `crates/nexus42/src/commands/acp_worker.rs` — doc comment explaining `!Send` rationale

Zero actual `use` / `mod` / import references to `agent_client_protocol` in nexus42 or nexus-orchestration source.

### 5. `cargo doc --package nexus-acp-host` — ✅ PASS

Generated successfully at `target/doc/nexus_acp_host/index.html`. 3 warnings (all doc-link related, non-blocking):
- Unresolved link to `execute` in `localset_bridge.rs:201`
- Unknown disambiguator in `transport.rs:133`

## Acceptance Criteria Checklist

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | 16 Nexus DTO types defined in `crates/nexus-contracts/src/local/acp/` | ✅ PASS | **22 types** defined in `types.rs`: `NexusSessionId`, `NexusProtocolVersion`, `NexusStopReason`, `NexusAuthMethod`, `NexusAgentInfo`, `NexusAgentCapabilities`, `NexusSessionMode`, `NexusSessionModeState`, `NexusContentBlock`, `NexusTextContent`, `NexusResourceLink`, `NexusMcpServer`, `NexusMcpServerHttp`, `NexusMcpServerSse`, `NexusMcpServerStdio`, `NexusInitializeRequest`, `NexusNewSessionRequest`, `NexusPromptRequest`, `NexusInitializeResponse`, `NexusSessionCreated`, `NexusPromptCompleted`, `NexusCancelResult` |
| 2 | `NexusAcpClient` trait has zero SDK types in signatures | ✅ PASS | All 4 trait methods (`initialize`, `create_session`, `prompt`, `cancel`) use only `Nexus*` DTO types from `nexus_contracts::local::acp` and `AcpResult`. SDK types (`acp::*`) confined to `AcpSdkAdapter` impl blocks and private `FromSdk` conversion functions. |
| 3 | Consumers compile without `agent-client-protocol` dependency | ✅ PASS | `agent-client-protocol` is commented out in `crates/nexus42/Cargo.toml` with `REMOVED` note. Zero `use agent_client_protocol` in `crates/nexus42/src/` or `crates/nexus-orchestration/`. Workspace compiles and links. |
| 4 | `cargo test --workspace` green | ⚠️ PARTIAL | 680/681 tests pass (1 ignored). 1 failure is pre-existing at base commit `47bfc51`, outside WS-B diff scope (`47bfc51..95ce3d2`). No WS-B changes touch `crates/nexus42/src/auth/`. |
| 5 | clippy/fmt clean | ✅ PASS | Both commands exit clean with zero warnings/errors. |

## Findings

### Low
- **Doc link warnings** in `nexus-acp-host`: `localset_bridge.rs:201` references `[execute]` without scope; `transport.rs:133` has `["...]` in doc comment. Non-blocking for compilation, should be cleaned up for doc quality.

### Pre-existing (not in WS-B scope)
- **`auth::tests::get_returns_none_for_unknown_creator`** fails with JSON parse error at `crates/nexus42/src/auth/mod.rs:240`. Confirmed pre-existing by running test at base commit `47bfc51` — same failure. Recommend filing as separate issue or addressing in WS-A residual cleanup.

## Not Tested

- E2E integration with actual ACP agents (requires live agent binary).
- `nexus-platform` consumption of `@42ch/nexus-contracts` npm package (private repo, outside this repo scope).

## Conclusion

**WS-B acceptance criteria: 4/5 fully met, 1/5 partially met (pre-existing test failure outside diff scope).**

All WS-B-specific changes (22 DTO types, clean trait boundary, SDK dependency removal, clippy/fmt/doc) pass. The single test failure predates this diff range and is unrelated to ACP SDK DTO decoupling.
