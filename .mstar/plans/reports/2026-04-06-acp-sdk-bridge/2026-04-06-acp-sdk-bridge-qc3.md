---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-04-06-acp-sdk-bridge"
verdict: Request Changes
generated_at: "2026-04-07"
---

# QC Report #3 — ACP SDK Bridge Implementation

**Reviewer**: QC-#3 (Thread Safety, Resource Management, Security)
**Branch**: `feature/v2.0-acp-sdk-bridge`
**Commits**: `ebdf3aa` (test), `0e734d5` (feat)
**Files Changed**: 4 files, +813/-107 lines

---

## Summary

The `LocalSetBridge` correctly bridges `!Send` ACP SDK futures to the async tokio world, but has **two blocking issues**: (1) `Drop::drop` blocks forever with no timeout on `handle.join()`, creating a DoS surface; (2) a race condition in the shutdown-leader election can cause double-shutdown. Additionally, `AcpSdkAdapter::with_connection` spawns fire-and-forget async tasks with no completion tracking.

---

## Findings

| ID | Severity | Location | Description | Suggestion |
|----|----------|----------|-------------|------------|
| Q3-C1 | **Critical** | `localset_bridge.rs:241-246` | `Drop::drop` calls `handle.join()` with no timeout — if the LocalSet thread hangs (infinite loop, blocking I/O), Drop blocks forever, creating a DoS vector | Wrap `join()` in a timeout (e.g., 5s), then `detach()` or `std::thread::park()` the thread if it won't exit |
| Q3-C2 | **Critical** | `localset_bridge.rs:233` | `Arc::strong_count(&self.thread_handle) == 1` check outside the `thread_handle` lock — two clones dropped simultaneously can both see `strong_count == 1` and both attempt shutdown, causing double-send on channel | Move the count check inside the lock, or use `Arc::strong_count` atomically before locking |
| Q3-H1 | **High** | `client.rs:363-418` | `with_connection()` spawns a `tokio::spawn` fire-and-forget task that calls `bridge_clone.execute()`. If `AcpSdkAdapter` is dropped before this task completes, the connection setup is abandoned with no cleanup | Track the spawned `JoinHandle` and wait for it in an explicit `shutdown()` method, or at minimum document that callers must wait before dropping |
| Q3-H2 | **High** | `localset_bridge.rs:238` | `self.request_tx.try_send(None)` — if the channel is full, the shutdown signal is silently dropped; the thread may never exit | Use `try_send(None).is_ok()` to detect failure and force `detach()` the thread, then log a warning |
| Q3-M1 | **Medium** | `localset_bridge.rs:125-129` | `while let Some(Some(request)) = request_rx.recv().await` silently drops `None` (shutdown) on each iteration after the first — the loop will exit on the first `None` but multiple clones could each send `None` | Unclear if intentional; document that shutdown sends exactly one `None` per bridge lifetime |
| Q3-M2 | **Medium** | `localset_bridge.rs:167` | Unbounded `oneshot::channel` for response — if the caller drops the future before receiving, the response is silently dropped; the LocalSet thread continues processing | Consider adding a timeout at the `execute()` caller level (already provided via `execute_with_timeout`) |
| Q3-M3 | **Medium** | `client.rs:589-602` | `subscribe()` returns an empty `async_broadcast` receiver — no connection to the actual SDK stream | Document that this is a stub returning empty receiver; integration requires wiring the actual `SdkConnection::stream_receiver` |
| Q3-L1 | **Low** | `commands/agent.rs:496-502` | `interactive_prompt_loop` logs a note that prompts are not sent — V1.0 users will be confused | Either implement a basic prompt path or remove the misleading loop and show "not implemented" |
| Q3-L2 | **Low** | `transport.rs:226-236` | `AcpSession::is_running()` only checks `child.id().is_some()` — does not distinguish between running and zombie states | Improve to attempt `try_wait()` to get actual process state |
| Q3-W1 | **Warning** | `localset_bridge.rs:102` | mpsc channel buffer of 16 — if 16 requests are queued and the LocalSet thread stalls, new requests will be rejected (channel full) | Add `execute_with_timeout` as the primary API and document buffer implications |

---

## Review Checklist

| Item | Status | Evidence |
|------|--------|----------|
| LocalSetBridge Drop handles in-flight requests gracefully | ⚠️ Partial | Drop sends `None` but join has no timeout (Q3-C1) |
| Thread join has a timeout to prevent hanging on shutdown | ❌ Fail | `handle.join()` at line 242 has no timeout |
| No potential for deadlock between sync and async channels | ⚠️ Partial | No deadlock observed, but unbounded oneshot could accumulate |
| Agent subprocess is killed on bridge drop / session close | ✅ Pass | `kill_on_drop(true)` in transport.rs:157; `AcpSession::shutdown` handles SIGTERM→SIGKILL |
| Ctrl+C (SIGINT) during prompt loop triggers cleanup | ✅ Pass | `setup_cancel_handler` in commands/agent.rs:393-409 spawns `ctrl_c()` handler |
| Channel buffer sizes are bounded | ✅ Pass | `mpsc::channel(16)` bounded; oneshot is 1:1 |
| No use of `unsafe` without `// SAFETY:` comment | ✅ Pass | No `unsafe` blocks in changed files |

---

## Cross-Reviewer Ready Notes

**Expected runtime impact if Q3-C1 is not fixed**: HIGH — Any `LocalSetBridge` dropped while its thread is stuck causes the calling code to hang forever. This could freeze the CLI on exit, hang tokio runtime shutdown, or block any `drop` of an `AcpSdkAdapter` that contains a stuck bridge.

**Rollback urgency**: HIGH — The blocking issue (infinite join) affects a core primitive (`LocalSetBridge`). If the branch is merged with Q3-C1 unfixed, any session that triggers a bridge drop while the thread is in a bad state will hang. Recommend fixing before merge.

**Items also flagged by QC-#1 or QC-#2** (to verify cross-reviewer alignment): If other reviewers flag `mpsc::channel(16)` as too small or the fire-and-forget task in `with_connection`, those overlap with Q3-H1 and Q3-W1.

---

## Gate Recommendation

**Verdict**: `Request Changes`

**Blocking conditions**:
- Q3-C1: Unbounded `handle.join()` in `Drop` — must add timeout
- Q3-C2: Race condition in shutdown leader election — must fix `Arc::strong_count` check

**Non-blocking but required before merge**:
- Q3-H1: Fire-and-forget task in `with_connection` — needs tracking or documentation
- Q3-H2: `try_send(None)` can silently fail — need error handling

**Acceptable as residuals** (medium/low):
- Q3-M3: `subscribe()` stub — documented as incomplete
- Q3-L1: `interactive_prompt_loop` stub — user-visible confusion risk
- Q3-L2: `is_running()` imprecise state check — low impact
- Q3-W1: Channel buffer size — document and monitor

---

*Report generated by @qc-specialist-3 — Thread Safety, Resource Management, Security focus*
