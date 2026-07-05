---
report_kind: qc-consolidated
plan_id: "2026-04-06-acp-sdk-bridge"
generated_at: "2026-04-07"
consolidated_by: "@project-manager"
---

# QC Consolidated Decision: ACP SDK Bridge (Plan B)

## Decision: Request Changes

## Blocking Items (Must Fix Before Merge)

| ID | Severity | Location | Issue | Source |
|----|----------|----------|-------|--------|
| QC1-C1 | **Critical** | `localset_bridge.rs:241` | `.unwrap()` on `Mutex::lock()` in `Drop::drop()` — violates `#[deny(clippy::unwrap_used)]` and can panic during cleanup | QC-#1 |
| Q3-C1 | **Critical** | `localset_bridge.rs:233` | Unbounded `handle.join()` in `Drop` — if thread is stuck, `drop()` hangs forever | QC-#3 |
| Q3-C2 | **Critical** | `localset_bridge.rs` | Race condition in shutdown leader election using `Arc::strong_count` — not atomic with drop | QC-#3 |
| QC1-H1 | **High** | `acp/mod.rs` | `#[deny(clippy::unwrap_used)]` not applied to `acp` module — inconsistent with Plan A hardening | QC-#1 |
| QC1-H2 | **High** | `localset_bridge.rs` | Thread spawn failure silently ignored — bridge in invalid state | QC-#1 |
| Q3-H1 | **High** | `client.rs` | Fire-and-forget task in `with_connection` — no error propagation | QC-#3 |

## Non-Blocking Findings (Residuals)

| ID | Severity | Source | Issue | Decision |
|----|----------|--------|-------|----------|
| QC1-M1 | Medium | QC-#1 | Channel capacity hardcoded at 16 | Accept — reasonable default |
| QC1-M2 | Medium | QC-#1 | No timeout on request processing | Defer V1.1 |
| QC1-M3 | Medium | QC-#1 | Bridge assumes single active request | Accept — CLI is single-threaded |
| QC2-L1..L6 | Low | QC-#2 | API documentation, subscribe() placeholder, etc. | Defer V1.1 |
| QC2-W1..W3 | Warning | QC-#2 | Connection error visibility, error chain depth | Defer V1.1 |
| Q3-M1..M3 | Medium | QC-#3 | Graceful period too short, signal forwarding | Defer V1.1 |
| Q3-L1..L2 | Low | QC-#3 | Unused code, test helper duplication | Defer V1.1 |

## Assigned Fix Owners
- **@fullstack-dev-2** — fix all 6 blocking items on `feature/v2.0-acp-sdk-bridge`

## Fix Guidance

1. **QC1-C1**: Replace `.unwrap()` with `.expect("bridge shutdown: mutex poisoned")` or handle poison gracefully
2. **Q3-C1**: Add timeout to `handle.join()` — e.g., `handle.join(timeout).unwrap_or_else(...)` or spawn a watchdog
3. **Q3-C2**: Replace `Arc::strong_count` race with explicit `AtomicBool` shutdown flag
4. **QC1-H1**: Add `#[deny(clippy::unwrap_used)]` to `acp/mod.rs` or to each sub-module
5. **QC1-H2**: Handle `thread::spawn` failure — return `Err(AcpError::BridgeFailed)` from `LocalSetBridge::new()`
6. **Q3-H1**: Store task handle and check/join it in Drop

## Next Step
Fix 6 blocking items → re-verify → QC re-review → QA verification → merge to main
