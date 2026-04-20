---
report_kind: qc
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-04-20-v1.6-ws-a-residual-governance"
verdict: "Request Changes"
generated_at: "2026-04-20"
---

# QC1 Review — V1.6 WS-A Residual Governance

## Summary

| Field | Value |
|-------|-------|
| **Plan ID** | `2026-04-20-v1.6-ws-a-residual-governance` |
| **Reviewer** | qc-specialist (#1) |
| **Scope** | `git diff 75a1012..2182cbf` (origin/main → HEAD on feature/v1.6) |
| **Review cwd** | `/Users/bibi/workspace/organizations/42ch/nexus` |
| **Working branch** | `feature/v1.6` |
| **Verdict** | **Request Changes** |
| **Critical** | 1 |
| **Warnings** | 3 |
| **Suggestions** | 4 |

### Scope Delivered

6 commits reviewed:
- `f7379b3` — R1 fix (pause_schedule error logging in cancel path)
- `3b0464b` — R3 fix (dead code removal in Scheduler::tick())
- `68a9367` — R6 fix (session recovery FlowRunner reconstruction)
- `2182cbf` — plan documentation update
- `7960e34` — plan registration (docs)
- `fb71f33` — V1.6 compass addition (docs)

4 implementation files + 1 test file changed in the WS-A scope:
- `crates/nexus42d/src/api/handlers/orchestration/schedules.rs` (R1)
- `crates/nexus-orchestration/src/schedule/supervisor.rs` (R2, + tests)
- `crates/nexus-orchestration/src/scheduler/mod.rs` (R3)
- `crates/nexus-orchestration/src/engine.rs` (R6 + tests)
- `crates/nexus-orchestration/tests/cron_trigger.rs` (R3 test update)

---

## Critical Findings (blocking)

### C1: R2 TOCTOU test does not actually exercise the concurrent race

**Severity**: Critical  
**Files**: `crates/nexus-orchestration/src/schedule/supervisor.rs` (test `r2_resume_toctou_race_returns_current_status`)  
**Acceptance criteria reference**: Compass §4 WS-A evidence — "Unit test: concurrent `resume_schedule` calls produce correct serial outcome (no double-resume)"

**Finding**: The test `r2_resume_toctou_race_returns_current_status` calls `resume_schedule()` twice **sequentially** (not concurrently). The second call fails with `InvalidTransition` error because the status check happens **before** the TOCTOU-vulnerable UPDATE. This means the `rows_affected() == 0` branch is **never actually exercised** in the test.

The test asserts:
```rust
let status2 = sup.resume_schedule("R2-TOCTOU-S1").await;
assert!(status2.is_err(), "second resume should fail...");
```

But the acceptance criteria requires testing that the TOCTOU branch (rows_affected == 0 → return current status) works correctly. To properly test this, you need:
- Two concurrent calls to `resume_schedule()` where both pass the initial status check (paused) before either executes the UPDATE, OR
- A simulated scenario where the UPDATE affects 0 rows despite the status check passing (e.g., via a mock or a second concurrent cancel that fires between the status check and UPDATE)

**Required fix**: Add a truly concurrent test using `tokio::spawn` or `join!` to fire two `resume_schedule()` calls simultaneously. At least one should hit the `rows_affected() == 0` path and return the current status string (not error).

---

## Warnings (non-blocking but important)

### W1: R6 reconstruct_runner creates fresh EngineProxy on each recovery

**Severity**: Warning  
**Files**: `crates/nexus-orchestration/src/engine.rs` (line 576)

**Finding**: `reconstruct_runner()` creates a new `Arc::new(EngineProxy { state: self.state.clone() })` for **every** recovered session. If N sessions recover, N separate EngineProxy instances are created. While `Arc<EngineProxy>` is cheap, this is inconsistent with how `start_session()` and other engine methods create their proxies — they should share the same proxy instance if possible.

**Impact**: Low for pre-1.0 with typical session counts. At scale (dozens of recovered sessions), this creates unnecessary Arc allocations.

**Suggestion**: If the proxy pattern is expected to persist, consider caching or sharing a single `Arc<EngineProxy>` in `GraphFlowEngine`.

### W2: R6 recover_sessions order — runners added before summaries tracked

**Severity**: Warning  
**Files**: `crates/nexus-orchestration/src/engine.rs` (lines 533-557)

**Finding**: The method first iterates to reconstruct runners, then calls `self.state.recover_sessions(summaries).await` to add summaries to the in-memory tracker. If `state.recover_sessions()` fails or panics, runners have already been inserted into `self.state.runners` but the corresponding summaries won't be in the tracker. This creates an orphan runner state.

**Mitigation**: The current design is defensible (runner reconstruction is per-session and wrapped in `if let Err`), and `state.recover_sessions()` is unlikely to fail (it's a simple in-memory insert). However, the ordering should be documented or the two operations should be in a single transactional block.

### W3: R6 reconstruct_runner only handles embedded presets, not filesystem presets

**Severity**: Warning  
**Files**: `crates/nexus-orchestration/src/engine.rs` (line 568)

**Finding**: `reconstruct_runner()` calls `crate::preset::load_embedded_preset()` exclusively. If a session was started with a filesystem-installed preset (once WS3 T6 user-installed presets are implemented), recovery will fail with a warning and the session will remain in the tracker without a runner.

**Impact**: This is acceptable for **current** code (only embedded presets exist). However, when filesystem presets land, this must be updated to use a unified `load_preset()` that searches both embedded and filesystem sources. This is a documented future gap, not a current bug.

**Recommendation**: Add a TODO comment at line 568 noting this limitation for future implementers.

### W4: R3 tick() now returns constant 0 — docstring is misleading

**Severity**: Warning  
**Files**: `crates/nexus-orchestration/src/scheduler/mod.rs` (lines 84-106)

**Finding**: The docstring says "Returns the number of schedules that were found due (not necessarily admitted)." but the implementation now always returns `0`. The docstring should be updated to reflect the new behavior, or the return type should change to `()`.

**Impact**: Misleading documentation. Callers that depend on the return value (e.g., for metrics or logging) will now always get 0.

---

## Suggestions (improvements)

### S1: R2 TOCTOU — duplicated status match arms

**Files**: `crates/nexus-orchestration/src/schedule/supervisor.rs` (lines 699-706, 734-741)

The same 6-arm `match` block is duplicated in both the `should_run` and fallback paths. Consider extracting to a helper:

```rust
fn status_to_str(s: ScheduleStatus) -> String {
    match s {
        ScheduleStatus::Running => "running",
        ScheduleStatus::Pending => "pending",
        // ...
    }.to_string()
}
```

Or use `format!("{:?}", current).to_lowercase()` if `ScheduleStatus` derives `Display`.

### S2: R1 — standalone pause path at line 482 not examined in diff

The plan file states: "Standalone pause path (line 482) already properly propagates errors." This claim should be verified by examining the standalone pause path in the diff. From the diff shown, only the cancel-path pause (line 545) was modified. If line 482 was already correct, this is fine — but the plan should cite the specific code (e.g., `return Err(...)` at that line) as evidence.

### S3: R6 — consider using `tracing::error!` for failed reconstruction

The current `warn!` level is reasonable, but for sessions that cannot recover their runner (especially if the preset was available at session-start time but is now missing), an `error!` might be more appropriate for ops monitoring. The current message clearly explains the degradation, which is good.

### S4: R3 — `pool` field marked `#[allow(dead_code)]` 

The `pool` field on `Scheduler` is retained with `#[allow(dead_code)]` and a comment "Retained for future use." This is fine for now, but should be tracked as a TODO or removed if no concrete use case is expected within the next 1-2 versions.

---

## Files Reviewed

| File | Changes | Review depth |
|------|---------|-------------|
| `crates/nexus42d/src/api/handlers/orchestration/schedules.rs` | R1: +9/-1 lines | Deep — error handling path verified |
| `crates/nexus-orchestration/src/schedule/supervisor.rs` | R2: +38/-4 lines + 158 test lines | Deep — TOCTOU logic and test gaps identified |
| `crates/nexus-orchestration/src/scheduler/mod.rs` | R3: -45/+8 lines | Deep — dead code removal verified, doc issues noted |
| `crates/nexus-orchestration/src/engine.rs` | R6: +147 lines | Deep — reconstruction logic, test coverage, edge cases |
| `crates/nexus-orchestration/tests/cron_trigger.rs` | R3: -6/+5 lines | Medium — test update consistent with R3 change |
| `.agents/plans/2026-04-20-v1.6-ws-a-residual-governance.md` | Plan documentation | Medium — evidence claims reviewed |
| `.agents/knowledge/v1.6-delivery-compass-v1.md` | New compass document | Light — scope validation only |

---

## Acceptance Criteria Verification

| Criteria | Status | Notes |
|----------|--------|-------|
| R1: Log pause error at warn!, cancel not blocked | **PASS** | Fix at line 548-554 correctly uses `if let Err(e)` + `tracing::warn!`. Cancel proceeds regardless. |
| R2: Check rows_affected() after resume UPDATE | **PARTIAL** | Code correctly checks `rows_affected() == 0` in both paths (lines 695, 731). **But test does not exercise concurrent race** (see C1). |
| R3: Remove dead tick() query, delegate to tick_clocked() | **PASS** | Dead query fully removed. `rg` confirms no remaining reference. Imports cleaned. |
| R6: Reconstruct FlowRunner from preset_id + caps + storage | **PASS** | Implementation loads preset, builds wired graph, creates FlowRunner, stores in state. 3 unit tests cover known preset, unknown preset, and terminal session cases. |
| V1.5 QC2 S2 verified | **PASS** | Plan documents outcome: `apply_llm_summarize` takes `[u8; 32]`, capability returns hex string, no production caller exists. |
| `cargo test --workspace` green | **NOTE** | Plan reports 776 passed, 1 pre-existing failure. Unable to verify locally due to permission restrictions. |
| `cargo clippy --all -- -D warnings` clean | **NOTE** | Plan reports clean. Unable to verify locally due to permission restrictions. |
| `cargo +nightly fmt --all -- --check` clean | **NOTE** | Plan reports clean. Unable to verify locally due to permission restrictions. |

---

## Cross-Reviewer Ready Notes

**This reviewer's unique findings** (Reviewer #1 — architecture/maintainability focus):
- **C1**: R2 test does not exercise concurrent TOCTOU race (critical — blocks Accept)
- **W1**: R6 creates per-session EngineProxy instances (architectural concern)
- **W4**: R3 tick() return value now misleadingly documented
- **S1**: R2 duplicated match arms — extraction opportunity

**Findings likely to overlap with other QC reviewers**:
- W2: R6 recover_sessions ordering (correctness reviewer may also flag)
- W3: R6 embedded-only preset loading (correctness reviewer may also flag)
- S4: `#[allow(dead_code)]` pool field (style reviewer may flag)

**Integration risk**: The R6 FlowRunner reconstruction introduces a new public-adjacent method (`reconstruct_runner`) and changes `recover_sessions` behavior. If other workstreams (WS-B DTO decoupling, WS-D multi-preset) touch the engine or preset loader, merge conflicts are likely in `engine.rs`. Recommend sequencing WS-A merge before WS-B/WS-D.

**Migration cost**: Low. The R1/R2/R3 changes are additive or subtractive without API changes. R6 adds internal-only behavior.

---

*Review completed: 2026-04-20. Verdict: Request Changes (1 Critical finding blocks approval).*
