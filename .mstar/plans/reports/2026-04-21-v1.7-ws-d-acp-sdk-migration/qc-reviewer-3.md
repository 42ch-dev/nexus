---
plan_id: "2026-04-21-v1.7-ws-d-acp-sdk-migration"
reviewer: 3
verdict: "Approve"
critical_count: 0
warning_count: 1
suggestion_count: 3
generated_at: "2026-04-21"
---

# QC Review Report — ACP SDK Migration v0.10.4→v0.11.0

## Scope

- **Plan ID**: `2026-04-21-v1.7-ws-d-acp-sdk-migration`
- **Diff basis**: `git diff feature/v1.7...HEAD`
- **Files reviewed**:
  - `crates/nexus-acp-host/src/client.rs` (1688 lines)
  - `crates/nexus-contracts/src/local/acp/types.rs` (1098 lines, 568 new)
  - `crates/nexus-acp-host/Cargo.toml` (version pin change)
  - `crates/nexus-acp-host/tests/acp_session_lifecycle.rs`

## Verification Evidence

- **clippy**: `cargo clippy --package nexus-acp-host` — **PASS** (no warnings)
- **fmt**: `cargo fmt --all -- --check` — **PASS**
- **dead_code scan**: `rg "TODO|FIXME|HACK|XXX"` in `crates/nexus-acp-host/src/` — **CLEAN**
- **cargo test**: Not run due to integration-test infrastructure requirements (pgvector, neo4j, redis containers)

---

## Findings

### R3-W1: Dead conversion helper `sdk_prompt_request_from_nexus` (Warning)

**File**: `crates/nexus-acp-host/src/client.rs:207-223`

**Evidence**:
```rust
#[allow(dead_code)]
fn sdk_prompt_request_from_nexus(req: NexusPromptRequest) -> PromptRequest {
```

The function is never called. The `prompt()` implementation (line 800-811) builds a text string from content blocks and uses `active_session.send_prompt(&prompt_text)` directly, bypassing this converter.

**Impact**: Dead code increases maintenance surface. If this helper was intended for future use (the streaming API replacement noted in the module docs), it should either be removed or its future role documented.

**Recommendation**: Either remove `sdk_prompt_request_from_nexus` if truly unused, or add a `// TODO(v2): used by upcoming streaming API` comment explaining why it's retained.

---

### R3-S1: Catch-all pattern in `nexus_stop_reason_from_sdk` silently swallows future variants (Suggestion)

**File**: `crates/nexus-acp-host/src/client.rs:99-108`

```rust
fn nexus_stop_reason_from_sdk(reason: &StopReason) -> NexusStopReason {
    match reason {
        StopReason::EndTurn => NexusStopReason::EndTurn,
        StopReason::MaxTokens => NexusStopReason::MaxTokens,
        StopReason::MaxTurnRequests => NexusStopReason::MaxTurnRequests,
        StopReason::Refusal => NexusStopReason::Refusal,
        StopReason::Cancelled => NexusStopReason::Cancelled,
        _ => NexusStopReason::EndTurn, // fallback for future variants  ← R3-S1
    }
}
```

**Impact**: If ACP SDK adds new `StopReason` variants in a future release, this fallback silently maps them to `EndTurn` rather than failing at compile time or returning an `Unknown(Variant)` wrapper.

**Recommendation**: Consider adding a `NexusStopReason::Unknown(String)` variant to preserve the unknown variant information, or use a `#[non_exhaustive]` approach to force explicit handling.

---

### R3-S2: `NexusAuthMethod` discards non-Agent variant data (Suggestion)

**File**: `crates/nexus-acp-host/src/client.rs:110-123`

```rust
fn nexus_auth_method_from_sdk(method: &AuthMethod) -> NexusAuthMethod {
    match method {
        AuthMethod::Agent(agent) => NexusAuthMethod {
            id: agent.id.to_string(),
            name: agent.name.clone(),
            description: agent.description.clone(),
        },
        _ => NexusAuthMethod {
            id: "unknown".to_string(),
            name: "unknown".to_string(),
            description: None,
        },
    }
}
```

**Impact**: If SDK adds new `AuthMethod` variants (e.g., `OAuth`, `APIKey`), all non-Agent variants collapse to `"unknown"`. This may be intentional (simplified view per the type docs), but it's a silent data loss.

**Recommendation**: Document the intentionality of this simplification, or add an `Unknown` variant to `NexusAuthMethod` if future-proofing is desired.

---

### R3-S3: Session content accumulation not yet surfaced (Suggestion)

**File**: `crates/nexus-acp-host/src/client.rs:855-864`

```rust
SessionMessage::SessionMessage(_dispatch) => {
    // Streaming content update — logged but not accumulated
    // in NexusPromptCompleted (content consumption is handled
    // by the caller via subscribe/future streaming API).
    tracing::trace!(...);
}
```

**Context**: The module header (lines 27-33) documents that `subscribe()` was removed and will be replaced with a proper streaming API. The current `prompt()` implementation reads streaming updates but only extracts `StopReason`, discarding content.

**Impact**: Low immediate impact (documented as known gap), but callers expecting content in `NexusPromptCompleted` will be surprised.

**Recommendation**: This is pre-existing design debt, not introduced by this migration. No action required in this PR, but track as technical debt if not already recorded.

---

## Strengths

1. **Excellent module documentation** (`client.rs` lines 1-52) explains the adapter pattern, DTO boundary, threading model, and SDK v0.11.0 architectural changes clearly.

2. **Clean DTO design** in `types.rs`: proper newtype wrappers (`NexusSessionId`, `NexusProtocolVersion`), builder patterns, `#[serde(skip_serializing_if = "Option::is_none")]` for optional fields, and `#[serde(transparent)]` for newtype wrappers.

3. **Comprehensive conversion helpers**: 12+ conversion functions following consistent naming (`nexus_X_from_sdk`, `sdk_X_from_nexus`).

4. **Good test coverage**: Unit tests for all conversion helpers, roundtrip serialization tests for all DTOs, and negative tests for adapter error paths (`*_without_connection_fails`).

5. **SDK version pin** (`=0.11.0` in `Cargo.toml`) prevents silent minor version upgrades.

6. **No TODO/FIXME/HACK markers** in reviewed code.

7. **Consistent error handling**: All trait methods return `AcpResult<T>` with proper error propagation via `crate::AcpError::sdk()` and `crate::AcpError::connection_failed()`.

---

## Cross-Reviewer Ready Notes

| Finding | Severity | Runtime Impact | Rollback Urgency |
|---------|----------|----------------|------------------|
| R3-W1 dead helper | Warning | None (unused) | Low |
| R3-S1 catch-all | Suggestion | Future variant silent mapping | Low |
| R3-S2 AuthMethod data loss | Suggestion | Non-agent auth silently ignored | Low |
| R3-S3 content discard | Suggestion (pre-existing) | Content not returned | Medium (documented) |

**Other reviewers may wish to verify**:
- Whether `sdk_prompt_request_from_nexus` was intentionally kept for a future streaming API (R3-W1)
- Whether the `#[allow(dead_code)]` on `sdk_prompt_request_from_nexus` was intentional or an oversight

---

## Verdict

**Approve** — No blocking issues found. The migration is well-structured with clear separation between SDK types and Nexus DTOs, comprehensive documentation, and good test coverage. The single warning (dead conversion helper) does not block merge but should be addressed in a follow-up.
