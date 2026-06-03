---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-20-v1.6-ws-b-acp-sdk-dto"
verdict: "Request Changes"
generated_at: "2026-04-20"
---

# QC Review #1 — V1.6 WS-B: ACP SDK DTO Decoupling

## Scope

- **Working branch**: `feature/v1.6`
- **Review cwd / Worktree path**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **plan_id**: `2026-04-20-v1.6-ws-b-acp-sdk-dto`
- **Review range / Diff basis**: `git diff 47bfc51..95ce3d2`
- **Commits reviewed** (5):
  - `f29869f` feat(contracts): define 16 Nexus-owned ACP DTO types in local/acp/
  - `5837e25` feat(acp-host): decouple NexusAcpClient trait from SDK types (T2+T3)
  - `a200dbe` feat(acp-host): verify consumer SDK decoupling (T4+T5)
  - `6249383` style(acp-host): fix clippy warnings and formatting
  - `95ce3d2` docs(plan): update WS-B plan status to InReview; all tasks complete
- **Files changed** (10): `crates/nexus-contracts/src/local/acp/{mod.rs, types.rs}`, `crates/nexus-contracts/src/local/mod.rs`, `crates/nexus-acp-host/src/{client.rs, lib.rs, session_manager.rs, skills.rs}`, `crates/nexus-acp-host/tests/acp_session_lifecycle.rs`, plan + status files.

## Severity Summary

| Severity | Count | Status |
|----------|-------|--------|
| Critical | 2 | Unresolved |
| Warning  | 3 | Unresolved |
| Info     | 4 | Documented |

## Verdict: Request Changes

Two Critical findings must be resolved before merge: a functional bug in the MCP server conversion that silently drops data, and a protocol version conversion that silently degrades non-integer versions.

---

## 1. Critical Findings

### C1: `sdk_new_session_request_from_nexus` silently drops `mcp_servers` field

**File**: `crates/nexus-acp-host/src/client.rs`, line 150-151

```rust
fn sdk_new_session_request_from_nexus(req: NexusNewSessionRequest) -> acp::NewSessionRequest {
    acp::NewSessionRequest::new(req.cwd)
    // mcp_servers field is NEVER converted or passed to the SDK
}
```

The `NexusNewSessionRequest` DTO carries a `mcp_servers: Vec<NexusMcpServer>` field (line 323 of types.rs) with a builder method `mcp_servers()`. Consumers can construct requests with MCP server configurations. However, the conversion function ignores this field entirely — the SDK's `NewSessionRequest::new()` only receives `cwd`.

**Impact**: Any consumer that attaches MCP servers to a new session request will see them silently discarded. The session will start without the intended MCP connections.

**Fix**: Convert each `NexusMcpServer` variant to the corresponding `acp::McpServer` variant and pass them to the SDK builder:

```rust
fn sdk_new_session_request_from_nexus(req: NexusNewSessionRequest) -> acp::NewSessionRequest {
    let mut builder = acp::NewSessionRequest::new(req.cwd);
    for server in req.mcp_servers {
        let sdk_server = match server {
            NexusMcpServer::Http(s) => acp::McpServer::Http(acp::McpServerHttp {
                name: s.name,
                url: s.url,
            }),
            NexusMcpServer::Sse(s) => acp::McpServer::Sse(acp::McpServerSse {
                name: s.name,
                url: s.url,
            }),
            NexusMcpServer::Stdio(s) => acp::McpServer::Stdio(acp::McpServerStdio {
                name: s.name,
                command: s.command,
            }),
        };
        builder = builder.mcp_server(sdk_server);
    }
    builder
}
```

### C2: Protocol version conversion silently degrades non-integer versions

**File**: `crates/nexus-acp-host/src/client.rs`, lines 135-141

```rust
let protocol_version: acp::ProtocolVersion = serde_json::from_value(serde_json::json!(req
    .protocol_version
    .0
    .parse::<u16>()
    .unwrap_or(1)))
.unwrap_or(acp::ProtocolVersion::LATEST);
```

`NexusProtocolVersion` wraps a `String` and can hold any value (e.g., `"1"`, `"1.0"`, `"2.1"`). The conversion `.parse::<u16>().unwrap_or(1)` silently coerces ANY non-integer-parsable string to `1`. A version like `"2.0"` or `"1.1"` would silently become `1` without warning.

The double `serde_json::from_value(json!(u16))` roundtrip is also unnecessary complexity — `acp::ProtocolVersion` likely implements `From<u16>` or has a simpler constructor.

**Impact**: Protocol version negotiation could silently downgrade to v1 when the client intends a higher version, potentially missing capabilities.

**Fix**: Either (a) change `NexusProtocolVersion` to store `u16` internally if only integer versions are supported, or (b) if string versions are intentional (per the doc "for flexibility across SDK versions"), the conversion should either fail explicitly or use proper version-aware logic:

```rust
fn sdk_protocol_version_from_nexus(nexus: &NexusProtocolVersion) -> acp::ProtocolVersion {
    match nexus.0.parse::<u16>() {
        Ok(n) => /* construct from u16 */,
        Err(_) => {
            tracing::warn!("unknown protocol version {:?}, falling back to LATEST", nexus.0);
            acp::ProtocolVersion::LATEST
        }
    }
}
```

---

## 2. Warning Findings

### W1: `NexusContentBlock` conversion drops optional `name` semantics

**File**: `crates/nexus-acp-host/src/client.rs`, line 163

```rust
let builder = acp::ResourceLink::new(r.name.unwrap_or_default(), r.uri);
```

When `name` is `None`, `unwrap_or_default()` passes an empty string `""` to the SDK. The SDK may interpret an empty name differently from an omitted name (e.g., as a literal empty-string name rather than "no name provided"). This is a semantic mismatch.

**Recommendation**: Check whether `acp::ResourceLink` has a builder pattern that allows omitting the name field, or use a sentinel that the SDK recognizes as "not set."

### W2: `NexusContentBlock` has only 2 of likely N SDK variants

**File**: `crates/nexus-contracts/src/local/acp/types.rs`, lines 213-219

The `NexusContentBlock` enum only covers `Text` and `ResourceLink`. The ACP SDK's `ContentBlock` likely has additional variants (e.g., `Image`, `ToolUse`, `ToolResult`). If the agent returns any of these, the prompt conversion would need a fallback.

This is acknowledged in the doc comment ("only Text and ResourceLink variants are needed by Nexus consumers today"), but the `sdk_prompt_request_from_nexus` conversion is one-directional (Nexus → SDK). The reverse direction (SDK → Nexus) doesn't exist yet because consumers don't read content blocks from responses. **If** future work adds SDK→Nexus content block conversion, the missing variants will cause compilation errors or require catch-all handling.

**Recommendation**: Add a `#[allow(clippy::match_wild_err_arm)]` note or a `TODO` comment at the conversion site to flag this for future reviewers.

### W3: Module doc comment contradicts actual placement of conversion functions

**File**: `crates/nexus-contracts/src/local/acp/types.rs`, line 3-4

```
//! Each type has `From<acp_sdk_type>` conversions in the same module
//! (or in `nexus-acp-host/src/client.rs` for types that depend on the SDK crate).
```

The doc says conversions are in "the same module" but ALL SDK↔Nexus conversion functions actually live in `client.rs` (because both SDK types and Nexus DTOs are external, orphan rules prevent `From` impls). The "same module" claim is misleading — there are zero conversion functions in `types.rs` for SDK types.

**Recommendation**: Correct the module doc to accurately describe that all conversion helpers are in `nexus-acp-host/src/client.rs`.

---

## 3. Info / Observations

### I1: Plan says "16 DTO types" but 25 public items are defined

The plan task T1 says "Define 16 Nexus-owned DTO types." The actual `types.rs` defines 25 public items (2 newtypes, 1 enum, 8 structs, 3 request DTOs, 4 response DTOs, 3 MCP server structs, 1 MCP server enum, 1 auth method struct, 1 agent info struct, 1 agent capabilities struct). Counting only distinct domain concepts (not helpers), the number is closer to 20-25. This is not a defect — the extra types (like `NexusTextContent`, `NexusResourceLink`, `NexusMcpServer*`) are necessary for a complete DTO layer — but the plan task description could be more precise.

### I2: `session_manager.rs` and `skills.rs` still import SDK types

`session_manager.rs` imports `agent_client_protocol::SessionId` and `skills.rs` imports `agent_client_protocol::ClientCapabilities` / `FileSystemCapabilities`. These are **outside the WS-B scope** (the diff only fixes the import path from `crate::client::SessionId` → `agent_client_protocol::SessionId` because the old re-export was removed). These files represent residual SDK coupling that should be addressed in a future pass if full SDK decoupling is desired.

### I3: `NexusInitializeRequest::title` field dropped during SDK conversion

**File**: `crates/nexus-acp-host/src/client.rs`, line 145

```rust
builder = builder.client_info(acp::Implementation::new(info.name, info.version));
```

`NexusAgentInfo` has a `title: Option<String>` field, but `acp::Implementation::new()` only takes `name` and `version`. The `title` is silently lost. If the SDK's `Implementation` has a `.title()` builder method, it should be used. If not, this is a known limitation.

### I4: Test coverage is good but has gaps

Roundtrip serde tests exist for: `NexusSessionId`, `NexusProtocolVersion`, `NexusStopReason`, `NexusInitializeRequest`, `NexusAgentInfo`, `NexusPromptRequest`, `NexusInitializeResponse`, `NexusSessionModeState`, `NexusMcpServer` (2 of 3 variants). Missing roundtrip tests for: `NexusAuthMethod`, `NexusAgentCapabilities`, `NexusSessionMode`, `NexusContentBlock`, `NexusTextContent`, `NexusResourceLink`, `NexusNewSessionRequest`, `NexusPromptCompleted`, `NexusCancelResult`, `NexusMcpServer::Sse` variant. Not critical (simple structs with obvious serialization), but worth noting.

---

## 4. Checklist Review

| # | Checklist Item | Status | Notes |
|---|---------------|--------|-------|
| 1 | **Correctness**: DTOs mirror SDK types | ⚠️ Warning | C1 (mcp_servers dropped), I3 (title dropped) |
| 2 | **Trait boundary**: Zero SDK types in `NexusAcpClient` | ✅ Pass | Verified: trait block (lines 183-208) uses only Nexus DTOs |
| 3 | **Consumer decoupling**: No SDK deps in consumers | ✅ Pass | `rg` confirms `agent_client_protocol` only in `nexus-acp-host` |
| 4 | **subscribe() design**: Well-documented and sound | ✅ Pass | Module docs + trait docs explain rationale clearly |
| 5 | **Test coverage**: Roundtrip conversions sufficient | ⚠️ Warning | Core types covered; simple structs lack explicit tests (I4) |
| 6 | **Code quality**: Clean patterns, error handling | ⚠️ Warning | C2 (silent version downgrade), W1 (empty string name), W3 (doc inaccuracy) |
| 7 | **No regressions**: Existing tests pass | ✅ Pass* | Plan evidence states 387 passed; clippy/fmt clean (*not independently verified due to permission constraints) |

---

## 5. Cross-Reviewer Ready Notes

**This reviewer's unique findings**: C1 (mcp_servers silently dropped — functional bug), C2 (protocol version silent degradation), W1 (ResourceLink name semantics), W2 (ContentBlock variant coverage forward risk), W3 (doc accuracy), I3 (title field lost), I4 (test gap analysis).

**Shared/verifiable findings**: Trait boundary cleanliness (C2 in checklist) — all reviewers can verify `pub trait NexusAcpClient` contains no `acp::` or `agent_client_protocol::` references. Consumer decoupling — `rg "agent_client_protocol" crates/{nexus-orchestration,nexus42}/` returns zero matches.

**Integration risk**: LOW for the trait boundary decoupling itself. MEDIUM if C1 is not fixed — any consumer using `mcp_servers` will experience silent data loss.

**Migration cost**: Future SDK migration will need to update conversion functions in `client.rs` (~12 functions) and potentially expand `NexusContentBlock`. The DTO layer is stable and well-structured for this.

---

## 6. Required Actions Before Merge

1. **Fix C1**: Implement `mcp_servers` conversion in `sdk_new_session_request_from_nexus`. Add a test verifying MCP servers round-trip through the conversion.
2. **Fix C2**: Replace silent `unwrap_or(1)` with explicit handling (logging + fallback, or change internal representation to `u16`).
3. **Address W1**: Verify SDK `ResourceLink` semantics for empty name vs omitted name.
4. **Update plan evidence**: After fixes, re-run `cargo clippy --all -- -D warnings` and `cargo test --workspace` and update the plan's Evidence section.
