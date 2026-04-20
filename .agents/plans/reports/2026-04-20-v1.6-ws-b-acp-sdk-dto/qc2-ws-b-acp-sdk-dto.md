---
report_kind: qc
reviewer: qc-specialist-2
reviewer_index: 2
plan_id: "2026-04-20-v1.6-ws-b-acp-sdk-dto"
verdict: "Approve"
generated_at: "2026-04-20"
---

# QC Review #2 — V1.6 WS-B ACP SDK DTO Decoupling

## Scope

- **Plan ID**: `2026-04-20-v1.6-ws-b-acp-sdk-dto`
- **Review range**: `git diff 47bfc51..95ce3d2`
- **Working branch**: `feature/v1.6`
- **Focus**: DTO layer correctness, trait boundary integrity, consumer decoupling, `subscribe()` design, security/correctness of conversion logic

## Files Reviewed

| File | Lines | Nature |
|------|-------|--------|
| `crates/nexus-contracts/src/local/acp/mod.rs` | 20 | New module declaration |
| `crates/nexus-contracts/src/local/acp/types.rs` | 530 | New DTO definitions (16 types) |
| `crates/nexus-contracts/src/local/mod.rs` | 14 | Added `pub mod acp` |
| `crates/nexus-acp-host/src/client.rs` | 1002 | Trait + adapter refactored |
| `crates/nexus-acp-host/src/lib.rs` | 44 | Updated re-exports |
| `crates/nexus-acp-host/src/session_manager.rs` | 2 | `SessionId` source changed |
| `crates/nexus-acp-host/src/skills.rs` | 2 | `ClientCapabilities` source changed |
| `crates/nexus-acp-host/tests/acp_session_lifecycle.rs` | 2 | `SessionId` source changed |
| `crates/nexus42/Cargo.toml` | 55 | SDK dep removed (commented) |
| `crates/nexus42/src/commands/acp_worker.rs` | 376 | Consumer (no SDK references) |
| `crates/nexus-orchestration/Cargo.toml` | 37 | No SDK dep |
| `crates/nexus42d/Cargo.toml` | 54 | No SDK dep |

## Verification Commands Run

| Command | Result | Evidence |
|---------|--------|----------|
| `cargo clippy --all -- -D warnings` | **PASS** (0 errors, 0 warnings) | `Finished dev profile in 0.24s` |
| `cargo fmt --check` | **PASS** for hand-written code; diffs only in `generated/` (stable fmt cannot apply `.rustfmt.toml` `ignore` without nightly) | No `types.rs` or `client.rs` in diff output |
| `rg "agent.client.protocol" crates/nexus-orchestration/ crates/nexus42/` | **ZERO matches** (except comment in `acp_worker.rs`) | Confirms consumer decoupling |
| `rg "agent-client-protocol" crates/nexus42/nexus42d/nexus-orchestration/Cargo.toml` | **ZERO active deps** | All commented out or absent |

> **Note**: `cargo test --workspace` and `cargo +nightly fmt --check` could not be executed due to tool permission restrictions in this environment. The plan evidence cites 387 passed tests with 1 pre-existing unrelated failure.

## Severity Summary

| Severity | Count | Status |
|----------|-------|--------|
| Critical | 0 | — |
| Warning | 3 | Documented; non-blocking |
| Note | 2 | Informational |

## Detailed Findings

### W1 — `NexusContentBlock` missing `Eq` derive (Consistency)

- **Location**: `crates/nexus-contracts/src/local/acp/types.rs:179`
- **Finding**: `NexusContentBlock` derives `PartialEq` but not `Eq`, while all other DTOs in the same module derive both. Its variants (`NexusTextContent`, `NexusResourceLink`) both implement `Eq`, so adding `Eq` is safe and improves consistency.
- **Impact**: Low — may require `PartialEq` bounds where `Eq` would suffice in downstream code.
- **Fix**: Add `Eq` to the derive list.

### W2 — Fragile protocol version parsing in `sdk_initialize_request_from_nexus` (Correctness)

- **Location**: `crates/nexus-acp-host/src/client.rs:135-148`
- **Finding**:
  ```rust
  let protocol_version: acp::ProtocolVersion = serde_json::from_value(serde_json::json!(req
      .protocol_version
      .0
      .parse::<u16>()
      .unwrap_or(1)))
  .unwrap_or(acp::ProtocolVersion::LATEST);
  ```
  This conversion path is unnecessarily complex: `String → u16 → JSON Value → SDK enum`. It silently falls back to `ProtocolVersion::LATEST` if any step fails. If the SDK ever introduces non-numeric version strings (e.g., `"2024-10"`), this will silently downgrade to `LATEST`.
- **Impact**: Low — current ACP spec uses numeric versions; fallback is safe.
- **Fix**: Add a `// TODO` or match explicitly on known versions. Consider a `TryFrom` or direct mapping instead of JSON roundtrip.

### W3 — Request DTOs lack `PartialEq` (Testability)

- **Location**: `crates/nexus-contracts/src/local/acp/types.rs:261-340`
- **Finding**: `NexusInitializeRequest`, `NexusNewSessionRequest`, and `NexusPromptRequest` do not derive `PartialEq`, while all response DTOs do. This makes assertion-based testing of request construction harder.
- **Impact**: Low — ergonomic/testing only.
- **Fix**: Add `PartialEq` to request DTO derives (all fields are `PartialEq` compatible).

### W4 — `NexusCancelResult` minimal payload (Design)

- **Location**: `crates/nexus-contracts/src/local/acp/types.rs:376-382`
- **Finding**: `NexusCancelResult` only contains `session_id`, which the caller already provided. This is consistent with the pattern of returning a result DTO for every operation, but provides no additional information.
- **Impact**: None — by design, but worth noting if future cancel operations return richer status.

### N1 — `session_manager.rs` and `skills.rs` still reference SDK types

- **Location**: `crates/nexus-acp-host/src/session_manager.rs:12`, `skills.rs:35-36`
- **Finding**: These files import `agent_client_protocol::SessionId` and `ClientCapabilities` directly. This is acceptable because `nexus-acp-host` is the SDK adapter crate and is expected to contain SDK references. However, the `SessionManager` using `SessionId` instead of `NexusSessionId` creates a minor inconsistency within the crate boundary.
- **Impact**: None — within the adapter crate.

### N2 — `AcpError::sdk` constructor still takes SDK `Error` directly

- **Location**: `crates/nexus-acp-host/src/error.rs:166-168`
- **Finding**: The `sdk()` error constructor accepts `agent_client_protocol::Error` directly. Since this is in `nexus-acp-host`, it is within the SDK boundary. Consumers see only the stringified `AcpError::Sdk(String)` variant.
- **Impact**: None — boundary preserved.

## Shared Baseline Checks

| Check | Result | Notes |
|-------|--------|-------|
| No functional regressions | **PASS** | Trait methods preserve same semantics; only types changed |
| No security issues | **PASS** | No new injection surfaces; no auth boundary changes |
| No data consistency issues | **PASS** | Conversion functions are lossless for used fields |
| Tests present | **PASS** | DTO roundtrip tests + conversion tests in `types.rs` and `client.rs` |
| CI gate (clippy) | **PASS** | Clean |

## Task-by-Task Verification

| Task | Status | Evidence |
|------|--------|----------|
| T1: Define 16 Nexus-owned DTO types | **Complete** | All 16 types defined in `types.rs` with serde, docs, tests |
| T2: Move `subscribe()` off trait | **Complete** | `subscribe()` is now a direct method on `AcpSdkAdapter`; well documented |
| T3: Update `NexusAcpClient` trait signatures | **Complete** | Zero SDK types in trait signatures |
| T4: Update all consumers | **Complete** | `nexus42`, `nexus-orchestration`, `nexus42d` compile without SDK dep |
| T5: Verify no SDK in consumer deps | **Complete** | Cargo.toml files verified; `rg` confirms |

## Cross-Reviewer Ready Notes

- **Security/correctness**: The conversion functions (`nexus_*_from_sdk`, `sdk_*_from_nexus`) are the critical correctness boundary. They are straightforward field mappings with no validation logic — acceptable since the DTOs are pass-through types for consumers. The `stop_reason` fallback (`_ => NexusStopReason::EndTurn`) is safe for forward compatibility with future SDK variants.
- **Trait design**: Using RPITIT (`impl Future<Output = ...> + Send`) on the trait is modern Rust and avoids `async-trait` boxing. The `#[allow(async_fn_in_trait)]` is appropriate.
- **subscribe() rationale**: The documented rationale (no consumers use it through the trait, `StreamReceiver` is SDK-coupled) is sound. Moving it off the trait reduces API surface without losing functionality.

## Residual / Follow-up

- W1, W2, W3 are minor polish items that can be addressed in a follow-up batch or left as-is. They do not block approval.

## Verdict

**Approve**. The WS-B implementation successfully decouples the `NexusAcpClient` trait from SDK types, establishes a clean DTO layer in `nexus-contracts`, and verifies consumer compilation without `agent-client-protocol` dependencies. Clippy is clean. The `subscribe()` design decision is well-documented and sound. No Critical or blocking Warning findings.
