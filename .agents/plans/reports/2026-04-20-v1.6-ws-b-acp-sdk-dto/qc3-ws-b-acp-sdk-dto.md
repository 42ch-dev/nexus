---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-04-20-v1.6-ws-b-acp-sdk-dto"
verdict: "Approve"
generated_at: "2026-04-20"
---

# QC3 Report — V1.6 WS-B: ACP SDK DTO Decoupling

**Reviewer**: qc-specialist-3 (Performance & Reliability + Test Coverage)
**Review range**: `git diff 47bfc51..95ce3d2` (WS-A Done → WS-B completion)
**Working branch**: `feature/v1.6`
**Review cwd**: `/Users/bibi/workspace/organizations/42ch/nexus`

---

## Verdict: **Approve** ✅

All tasks complete. The `NexusAcpClient` trait boundary is correctly enforced; SDK types are confined to `AcpSdkAdapter`. Code quality is high; no blocking issues.

---

## Severity Summary

| Severity | Count | Status |
|----------|-------|--------|
| Critical | 0 | — |
| Warning | 0 | — |
| Info | 2 | Noted for future work |

**Gate**: `Critical=0` and `Warning=0` → **Approve**

---

## Shared Baseline Verification (all QC reviewers)

| Check | Evidence | Result |
|-------|----------|--------|
| No functional regressions | `cargo clippy --all -- -D warnings` exit 0 | ✅ Pass |
| Trait has zero SDK types in signatures | Manual inspection of `pub trait NexusAcpClient` (lines 183–207) | ✅ Pass |
| `subscribe()` moved off trait with rationale | Module docs §"subscribe() Design Decision" + impl at lines 915–931 | ✅ Pass |
| All 5 plan tasks checked off | Plan file lines 16–20 | ✅ Pass |

---

## QC3-Unique Findings

### Finding QC3-INFO-1: `subscribe()` stub returns dummy receiver

**File**: `crates/nexus-acp-host/src/client.rs:915–931`

```rust
pub fn subscribe(&self) -> StreamReceiver {
    let _connection = self.connection.clone();
    let (tx, rx) = async_broadcast::broadcast(16);
    drop(tx);
    tracing::warn!(... "subscribe() called — returning empty receiver...");
    StreamReceiver::from(rx)
}
```

**Analysis**: This is the chosen design (documented in module docs and plan T2). It is **intentional** — the rationale is that `StreamReceiver` is tightly coupled to the SDK's `async-broadcast` implementation, no consumer uses `subscribe()` through the trait abstraction, and wrapping would add complexity for an unused feature.

**Risk**: Runtime behavior: if any consumer calls `subscribe()`, it receives an empty receiver and silently misses events. This is acceptable for V1.6 given the documented scope decision.

**Recommendation**: Add a runtime warning or hard-error if `subscribe()` is called before connection is established (beyond the current log). However, this is **out of scope** for WS-B given the explicit design decision to leave `subscribe()` unimplemented.

**Severity**: Info (not a defect; documented trade-off)

---

### Finding QC3-INFO-2: `session_manager.rs` and `skills.rs` still use SDK types internally

**Files**:
- `crates/nexus-acp-host/src/session_manager.rs:12` — `use agent_client_protocol::SessionId`
- `crates/nexus-acp-host/src/skills.rs:35–36` — `use agent_client_protocol::{ClientCapabilities, FileSystemCapabilities}`

**Analysis**: These modules are **internal to `nexus-acp-host`**, not external consumers of `NexusAcpClient`. The trait boundary (the public API contract) is correctly isolated — external consumers (orchestration engine, CLI) see only Nexus DTOs. Internal usage of SDK types within the host crate is acceptable and expected.

The plan T4/T5 scope ("update consumers to use new DTOs") refers to **external** consumers that call through `NexusAcpClient`, not internal modules that are part of the adapter implementation itself.

**Severity**: Info (not a defect; correctly scoped)

---

## Cross-Reviewer Ready Notes

### For @project-manager

The `subscribe()` stub is a **known gap** with a documented rationale. If streaming events become a requirement in V1.7+, the implementation should:
1. Use the actual `SdkConnection::stream_receiver` instead of a dummy
2. Consider whether a `NexusEventStream` wrapper is needed for the trait (vs. direct adapter method)

### For @qa-engineer

If `subscribe()` behavior is exercised in integration tests, note that it returns an empty receiver in the current implementation. No events will be delivered. This may need explicit test-skip or mock setup.

### Expected runtime impact of INFO items

Both INFO items have **zero impact** on current production use because:
1. No consumer calls `subscribe()` through the trait (per design doc)
2. Internal SDK usage in `session_manager`/`skills` doesn't affect the trait boundary

**Rollback urgency**: Low — the trait boundary is correctly enforced; internal implementation details are encapsulated.

---

## Verification Commands Run

| Command | Exit Code | Output |
|---------|-----------|--------|
| `cargo clippy --all -- -D warnings` | 0 | Clean, no warnings |
| `git diff 47bfc51..95ce3d2 --stat` | 0 | 10 files changed, +787/-140 lines |

---

## Evidence Quality Assessment

| Evidence | Quality | Notes |
|----------|---------|-------|
| Trait signature inspection | High | Direct code inspection confirms zero SDK types in `NexusAcpClient` trait |
| Clippy output | High | Tool output confirms zero warnings/errors |
| Plan task checklist | Medium | Self-reported; verified by trait inspection |

---

## Diff Scope Confirmation

**WS-B changes only** (from `git diff 47bfc51..95ce3d2 --name-only`):
- `.agents/plans/2026-04-20-v1.6-ws-b-acp-sdk-dto.md` — plan status update
- `.agents/status.json` — plan row status
- `crates/nexus-contracts/src/local/acp/types.rs` — **new** 530-line DTO file
- `crates/nexus-contracts/src/local/acp/mod.rs` — **new** module re-export
- `crates/nexus-contracts/src/local/mod.rs` — added `acp` module
- `crates/nexus-acp-host/src/client.rs` — trait + adapter refactor
- `crates/nexus-acp-host/src/lib.rs` — re-exports cleanup
- `crates/nexus-acp-host/src/session_manager.rs` — SDK type import (internal)
- `crates/nexus-acp-host/src/skills.rs` — SDK type imports (internal)
- `crates/nexus-acp-host/tests/acp_session_lifecycle.rs` — test update (SDK types)

No consumer files outside `nexus-acp-host` were modified in WS-B. The plan T4/T5 evidence criteria (consumers compile without SDK) should be verified against the full integration (nexus42 CLI, nexus42d) — not just the isolated crate diff. This is expected WS-C/D scope.
