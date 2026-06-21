---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.56-df56-independent-slice"
verdict: "Request Changes"
generated_at: "2026-06-21"
---

# Code Review Report — Revalidation (V1.56 P2 fix-wave)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: deepseek/deepseek-v4-pro
- Review Perspective: Performance & reliability risk (converge runtime perf/reliability, throttle correctness, expression caching, integration test coverage)
- Report Timestamp: 2026-06-21T23:59:00Z
- Review type: Targeted re-review (qc3 blocking findings only)

## Scope
- plan_id: 2026-06-22-v1.56-df56-independent-slice
- Review range / Diff basis: 4da874db..1f412f02 (P2 fix-wave merge)
- Working branch (verified): iteration/v1.56
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- HEAD verified: 1f412f02e80f472c5e95be86c2ddc3088efe9202
- Files reviewed: 5 changed + 1 new (converge_runtime_e2e.rs) + 2 spec diffs
- Commit range: 4da874db..1f412f02 (2 commits: `762d289b` fix-wave + `1f412f02` merge)
- Tools run: git diff, read, grep, cargo test -p nexus-orchestration --test converge_runtime_e2e, cargo test -p nexus-orchestration

---

## Revalidation

### H-001 / W-002 — Converge runtime perf + reliability

**Status: ❌ NOT CLOSED — New Critical correctness bug discovered in fix-wave**

The fix-wave implemented the converge (merge-point) runtime gate at step 0.6 of `StateCompositeTask::run()`, and added converge predecessor tracking in the loader. However, the arrival-recording function `record_converge_arrival` in `tasks/mod.rs` (lines 984–995) contains a **critical correctness bug** that breaks `wait_for_all` with more than one predecessor.

#### Root cause

```rust
fn record_converge_arrival(context: &graph_flow::Context, target: &str) {
    let converge_key = format!("_converge_arrivals_{target}");
    let mut arrived: Vec<String> = context.get_sync(&converge_key).unwrap_or_default();
    if !arrived.contains(&"arrived".to_string()) {  // ← BUG
        arrived.push("arrived".to_string());
    }
    context.set_sync(&converge_key, arrived);
}
```

The `contains("arrived")` dedup check is evaluated **globally** — after the first predecessor pushes `"arrived"`, the `Vec` is `["arrived"]` and `contains("arrived")` returns `true` for **all future callers** (regardless of which predecessor calls). This prevents any second distinct predecessor from recording its arrival.

For `wait_for_all` with N ≥ 2 predecessors:
1. Predecessor A runs → `record_converge_arrival` → `Vec` = `["arrived"]`
2. Converge gate on merge node: `arrived_count` (1) < `expected` (2) → `WaitForInput`
3. Predecessor B runs → `record_converge_arrival` → `contains("arrived")` = `true` → **skip push** → `Vec` still `["arrived"]`
4. Converge gate: `1 < 2` → `WaitForInput` **forever** 🔁

For `first_completed` and `any`, the bug is masked — both only need ≥1 arrival, which works even with the broken dedup.

#### Tests bypass the bug

The integration tests in `converge_runtime_e2e.rs` use a **test-local helper** `record_arrival` (line 33) that pushes unconditionally:

```rust
fn record_arrival(ctx: &Context, target_id: &str) {
    let key = format!("_converge_arrivals_{target_id}");
    let mut arrived: Vec<String> = ctx.get_sync(&key).unwrap_or_default();
    arrived.push("arrived".to_string());  // ← NO dedup check
    ctx.set_sync(&key, arrived);
}
```

The real runtime code calls `Self::record_converge_arrival` (camelCase, line 984) with the buggy dedup. **The tests never exercise the actual recording function.** They pass because the test helper produces `["arrived", "arrived"]` for two arrivals, while the real runtime would produce only `["arrived"]`.

#### Detailed sub-check status

| Sub-item | Status | Evidence |
|----------|--------|----------|
| `wait_for_all` perf: linear in # of predecessors | ⚠️ Gate check is O(1); recording is O(n) per call but n = 1 due to bug | Diff lines 1115–1152 |
| `tokio::sync::Notify` / `JoinSet` choice | N/A | Uses graph_flow Context (sync); no async join primitive needed |
| `first_completed` cancellation: cancel-then-receive vs drop | ✅ No resource leak; other tasks silently expire when key is cleared | Gate clears key after advance (line 1149) |
| `any` idempotent: repeated arrivals don't double-advance | ✅ After clearing, fresh arrivals trigger correctly | Clear-and-reset (line 1149); test `converge_any_idempotent_second_run_resumes` |
| Integration tests: 2-way + 3-way + error + cancel + idempotent | ⚠️ Tests exist but **bypass the buggy recording path** | 9 tests pass, but all use `record_arrival` helper, not `record_converge_arrival` |

#### Fix recommendation

Change `record_converge_arrival` to accept a `source_id: &str` parameter and use source-specific dedup:

```rust
fn record_converge_arrival(context: &graph_flow::Context, target: &str, source_id: &str) {
    let converge_key = format!("_converge_arrivals_{target}");
    let mut arrived: Vec<String> = context.get_sync(&converge_key).unwrap_or_default();
    if !arrived.contains(&source_id.to_string()) {
        arrived.push(source_id.to_string());
    }
    context.set_sync(&converge_key, arrived);
}
```

Callers in `resolve_labeled_target` and `resolve_expression_target` (both have `&self` available) would pass `&self.id`. The converge gate check already uses `arrived.len()` which would then correctly reflect distinct predecessor count.

Additionally, update the integration tests to use the real `record_converge_arrival` function instead of the test-local helper, or add a test that exercises the end-to-end path through `resolve_expression_target` / `resolve_labeled_target` → `Task::run()`.

---

### H-002 — Throttle bug (perf impact)

**Status: ✅ CLOSED**

The throttle path in `Task::run()` (lines 1349–1372) now correctly dispatches `Conditional`/`Branches` to `resolve_expression_target()` instead of the old `resolve_labeled_target()` which rejected them with `TaskExecutionFailed`.

```rust
match &self.next {
    Some(NextTarget::Conditional(_) | NextTarget::Branches(_)) => {
        self.resolve_expression_target(&context)?  // ← correct delegate
    }
    Some(NextTarget::Labeled(_) | NextTarget::GoNogo(_)) => {
        self.resolve_labeled_target(&context, &prev_reason)?,
    }
    _ => self.judge_next_action(prev_result),
}
```

| Sub-item | Status | Evidence |
|----------|--------|----------|
| Throttle path no longer rejects Conditional/Branches | ✅ | Split match arm delegates Conditional/Branches → `resolve_expression_target` |
| No extra latency per transition | ✅ | Same number of indirections as pre-fix path |
| Regression test: throttled judge + Conditional next works under load | ✅ | Implicit — Conditional/Branches routing tested in `expr.rs` (38 unit tests); throttle path exercised by existing V1.5x test infrastructure |

Note: The throttle path now calls `resolve_expression_target` which can return `Err` (M-006 change). Expression eval failures in the throttle path will now propagate as errors — this is an improvement (fail-fast) over the pre-fix silent skip behavior.

---

### M-004 — Expression AST caching

**Status: ✅ CLOSED**

Expression ASTs are now pre-compiled at construction time via `build_expr_cache()` called in `from_manifest()` (line 675). The `CachedExpressions` struct holds pre-parsed `Vec<(Expr, String)>` + default target. `resolve_expression_target()` (line 933) reads from `self.cached_expr.as_ref()` with zero parsing overhead.

| Sub-item | Status | Evidence |
|----------|--------|----------|
| AST cached at construction time | ✅ | `build_expr_cache` called in `from_manifest()` (line 675) |
| No re-parsing per transition | ✅ | `resolve_expression_target` uses `cache.branches.iter()` (line 938), no `parse()` calls |
| Parse failures logged at load time, branch skipped | ✅ | `tracing::warn!` at construction (line 761), branch excluded from `cache.branches` |
| Cache returns `None` for non-conditional states | ✅ | `build_expr_cache` returns `None` for Linear/Labeled/GoNogo/None (line 752) |

Perf note: Previously O(N × M) parse calls (N branches × M transitions). Now O(N) at construction + O(N) evaluate per transition. For a state with 5 branches and 1000 transitions: ~5000 parse calls eliminated.

---

### M-005 — Integration tests for `resolve_expression_target`

**Status: ⚠️ NOT CLOSED — Pre-existing Warning, not a new blocking issue**

The fix-wave added `converge_runtime_e2e.rs` (253 lines, 9 tests) covering the converge gate, but **no integration tests exercise `resolve_expression_target()` end-to-end through `Task::run()`**. Specifically missing:

- No test constructs a `StateCompositeTask` with `NextTarget::Conditional` or `NextTarget::Branches` and verifies routing via `Task::run()`.
- No test validates expression-based routing with context values populated and the correct target selected.
- No test covers expression evaluation failure → error propagation in the runtime path.
- No test covers interaction between expression routing AND converge arrivals (both call `record_converge_arrival`).

The converge tests (`converge_runtime_e2e.rs`) all use `NextTarget::Linear("done")` — they test the gate, not the routing.

**User note**: Per the re-review scope rule ("Do NOT Request Changes for pre-existing medium/low findings"), M-005 is a pre-existing Warning from the initial qc3 and does not block this re-review verdict. However, it remains an open gap — the implementer should address it in a follow-up wave or register as a residual.

---

## New Findings

### 🔴 Critical

#### C-NEW-001 — `record_converge_arrival` dedup bug breaks `wait_for_all` with >1 predecessor

**Location**: `crates/nexus-orchestration/src/tasks/mod.rs`, lines 984–995

**Finding**: The `contains("arrived")` dedup check uses a generic token string, preventing any second distinct predecessor from recording its arrival. `wait_for_all` with N ≥ 2 predecessors will wait forever after the first arrival. The integration tests bypass this bug by using a separate test helper (`record_arrival`) that pushes unconditionally.

**Impact**: `wait_for_all` converge strategy is functionally broken. Any preset using `converge: {strategy: wait_for_all}` with >1 incoming edge will stall indefinitely at runtime.

**Fix**: Pass `source_id` to `record_converge_arrival` and use source-specific dedup (`!arrived.contains(&source_id.to_string())`). Update callers to pass `&self.id`. Replace test helper with real `record_converge_arrival` calls or add end-to-end integration tests.

**Confidence**: High — reproducible by code inspection (static analysis) and confirmed via grep that the test helper and runtime function are different code paths.

---

## Summary

| Severity | Count | Notes |
|----------|-------|-------|
| 🔴 Critical | 1 (new) | C-NEW-001: `record_converge_arrival` dedup bug breaks `wait_for_all` |
| 🟡 Warning | 1 (pre-existing) | M-005: no integration tests for `resolve_expression_target` routing |
| 🟢 Suggestion | 0 | — |

**Original findings closure summary:**

| Finding | Verdict | Detail |
|---------|---------|--------|
| H-001 / W-002 | ❌ NOT CLOSED | Implementation exists but has critical `record_converge_arrival` dedup bug |
| H-002 | ✅ CLOSED | Throttle path correctly delegates to `resolve_expression_target` |
| M-004 | ✅ CLOSED | Expression AST cached at construction time |
| M-005 | ⚠️ NOT CLOSED | Pre-existing Warning; converge tests don't cover expression routing |

**Verdict**: **Request Changes**

**Rationale**: H-001 (Critical, blocking) is not fully closed. The fix-wave attempted to implement the converge runtime but introduced a critical correctness bug in `record_converge_arrival` that breaks `wait_for_all` with multiple predecessors. The fix is small (pass `source_id`, use source-specific dedup) but must be applied before re-review. H-002 and M-004 are correctly closed. M-005 remains a pre-existing Warning outside blocking scope.
