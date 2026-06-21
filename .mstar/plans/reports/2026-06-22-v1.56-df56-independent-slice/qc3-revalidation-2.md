---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.56-df56-independent-slice"
verdict: "Approve"
generated_at: "2026-06-21"
---

# Code Review Report — Revalidation Round 2 (V1.56 P2 fix-wave-2)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: deepseek/deepseek-v4-pro
- Review Perspective: Performance & reliability risk (converge runtime perf/reliability, dedup correctness)
- Report Timestamp: 2026-06-21T23:59:00Z
- Review type: Targeted re-review round 2 (qc3 blocking finding C-NEW-001 only)

## Scope
- plan_id: 2026-06-22-v1.56-df56-independent-slice
- Review range / Diff basis: 1f412f02..1eee1a7c (P2 fix-wave-2 merge)
- Working branch (verified): iteration/v1.56
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- HEAD verified: 1eee1a7c
- Files reviewed: 2 (crates/nexus-orchestration/src/tasks/mod.rs, crates/nexus-orchestration/tests/converge_runtime_e2e.rs)
- Commit range: 49c2b3b7 (qc3 revalidation report) → 984139fa (fix C-NEW-001) → 9b7f73f7 (qc2 revalidation) → 1eee1a7c (merge)
- Tools run: git diff, read, grep, cargo test -p nexus-orchestration --test converge_runtime_e2e, cargo test -p nexus-orchestration, cargo clippy -p nexus-orchestration -- -D warnings

---

## Revalidation (Round 2)

### C-NEW-001 — `record_converge_arrival` dedup bug

**Status: ✅ CLOSED**

The fix-wave-2 correctly addresses all aspects of C-NEW-001. Below is the per-criterion evidence.

#### Criterion-by-criterion verification

| # | Criterion | Status | Evidence |
|---|-----------|--------|----------|
| 1 | Uses per-source `HashSet` tracking (NOT single token) | ✅ | `HashSet<String>` replaces `Vec<String>`; `arrived.insert(source_id.to_string())` replaces `arrived.push("arrived".to_string())` |
| 2 | Function signature has `source_id: &str` parameter | ✅ | `pub fn record_converge_arrival(context: &graph_flow::Context, target: &str, source_id: &str)` |
| 3 | All 3 call sites updated to pass source_id | ✅ | All 3 call sites now pass `&self.id` (lines 894, 947, 976 of `tasks/mod.rs`) |
| 4 | `wait_for_all` gate checks per-source arrival count vs expected | ✅ | `arrived.len()` (unique source count) compared to `self.converge_predecessors.len()` (expected predecessor count) |
| 5 | Test helper bypass REMOVED — tests use real runtime path | ✅ | `record_arrival(ctx, target)` → `converge_arrive(ctx, target, source_id)` calling `StateCompositeTask::record_converge_arrival` |
| 6 | Regression test: 3 predecessors any order → all recorded → advances | ✅ | `converge_dedup_three_distinct_predecessors_all_recorded` (arrivals y, x, z; 2/3→wait, 3/3→advance) |
| 7 | Regression test: same source_id twice → second deduped (idempotent) | ✅ | `converge_dedup_same_source_twice_idempotent` (a×2→still 1/2, a+b→2/2→advance) |

#### Detailed analysis

**1. Per-source HashSet tracking** (criterion 1, 2)

The old code:
```rust
let mut arrived: Vec<String> = context.get_sync(&converge_key).unwrap_or_default();
if !arrived.contains(&"arrived".to_string()) {
    arrived.push("arrived".to_string());
}
```

The new code:
```rust
let mut arrived: std::collections::HashSet<String> =
    context.get_sync(&converge_key).unwrap_or_default();
if arrived.insert(source_id.to_string()) {
    context.set_sync(&converge_key, arrived);
}
```

`HashSet::insert()` returns `true` on new insertion, `false` on duplicate — providing automatic, correct dedup at the data structure level. The old code's `contains("arrived")` check was a global gate that blocked all arrivals after the first.

**2. All call sites updated** (criterion 3)

| Call site (line) | Before | After |
|---|---|---|
| `resolve_labeled_target` (894) | `Self::record_converge_arrival(context, target);` | `Self::record_converge_arrival(context, target, &self.id);` |
| `resolve_expression_target` branch match (947) | `Self::record_converge_arrival(context, target);` | `Self::record_converge_arrival(context, target, &self.id);` |
| `resolve_expression_target` default fallback (976) | `Self::record_converge_arrival(context, &cache.default);` | `Self::record_converge_arrival(context, &cache.default, &self.id);` |

All three pass `&self.id` — the task ID of the predecessor. This aligns with `converge_predecessors` (a `HashSet<String>` of predecessor task IDs populated at graph build time), so the gate comparison `arrived.len() == expected` correctly gates on per-predecessor arrival.

**3. Gate check correctness** (criterion 4)

```rust
let arrived: std::collections::HashSet<String> =
    context.get(&self.converge_key).await.unwrap_or_default();
let arrived_count = arrived.len();
let expected = self.converge_predecessors.len();

let condition_met = match converge_config.strategy {
    ConvergeStrategy::WaitForAll => arrived_count >= expected,
    ConvergeStrategy::FirstCompleted | ConvergeStrategy::Any => arrived_count >= 1,
};
```

- `HashSet::len()` returns the count of **unique** entries — exactly the distinct predecessor count.
- `WaitForAll`: advances when unique arrivals ≥ expected predecessor count.
- `FirstCompleted` / `Any`: advances when ≥ 1 unique arrival exists (correct; prior bug also masked here since only ≥1 was needed).
- After advancing, `converge_key` is cleared (`context.set(&self.converge_key, serde_json::Value::Null).await`), preventing stale state.

**4. Test helper bypass removed** (criterion 5)

Old test helper (bypassed real runtime):
```rust
fn record_arrival(ctx: &Context, target_id: &str) {
    let key = format!("_converge_arrivals_{target_id}");
    let mut arrived: Vec<String> = ctx.get_sync(&key).unwrap_or_default();
    arrived.push("arrived".to_string());
    ctx.set_sync(&key, arrived);
}
```

New test helper (delegates to real runtime):
```rust
fn converge_arrive(ctx: &Context, target_id: &str, source_id: &str) {
    StateCompositeTask::record_converge_arrival(ctx, target_id, source_id);
}
```

All existing tests updated to use `converge_arrive` with explicit source IDs (e.g., `converge_arrive(&ctx, "merge_2", "a")`). The tests now exercise the exact same code path as production.

**5. New regression tests** (criteria 6, 7)

Both tests pass (`cargo test -p nexus-orchestration --test converge_runtime_e2e` — all 11 tests pass):
- `converge_dedup_three_distinct_predecessors_all_recorded`: Arrivals in non-canonical order (y, x, z); verifies 2/3 → `WaitForInput`, 3/3 → `Continue`.
- `converge_dedup_same_source_twice_idempotent`: "a" arrives twice → still 1/2 → `WaitForInput`; "b" arrives → 2/2 → `Continue`.

#### Key consistency (write/read)

- **Write path**: `record_converge_arrival` writes to `_converge_arrivals_{target}` where `target` is the converge target state ID.
- **Read path**: Gate check reads from `self.converge_key` which is `format!("_converge_arrivals_{}", state.id)`.
- **Match**: The converge gate runs on the converge target state, so `target` (write) == `state.id` (read) for any correctly configured converge node.

#### Sync/async consistency

- `record_converge_arrival` uses `context.get_sync()` / `context.set_sync()` (synchronous, called from `resolve_*_target` methods within `Task::run()`).
- Gate check uses `context.get().await` / `context.set().await` (async, called from `Task::run()` which is async).
- These operate on the same underlying key-value store; `_sync` variants are blocking convenience wrappers. This pattern was already present pre-fix and is unchanged by fix-wave-2.

#### Function visibility

`record_converge_arrival` is now `pub` — necessary for the test helper `converge_arrive` to call it. This is the correct visibility for a testable internal function. No other crate depends on this signature (verified via `grep` for `record_converge_arrival` across the workspace).

#### Performance impact

| Metric | Before (buggy) | After (fixed) |
|--------|---------------|---------------|
| Arrival recording | O(n) — `Vec::contains()` linear scan (n ≤ 1 due to bug) | O(1) — `HashSet::insert()` |
| Gate check | O(1) — `Vec::len()` | O(1) — `HashSet::len()` |
| Memory per converge node | O(n) — n arrival tokens stored | O(n) — n source_id strings stored |
| Context serialization | Vec<String> → JSON array | HashSet<String> → JSON array (same wire format) |

The fix is a strict performance improvement: `HashSet::insert()` (O(1) amortized) replaces `Vec::contains()` (O(n)). Context storage size is equivalent (n strings in both cases). Wire format after JSON serialization is unchanged.

#### CI validation

- `cargo test -p nexus-orchestration --test converge_runtime_e2e`: **11/11 passed** (including 2 new regression tests)
- `cargo test -p nexus-orchestration` (all tests): **all passed**
- `cargo clippy -p nexus-orchestration -- -D warnings`: **clean** (no warnings)

---

## New Findings

### 🔴 Critical

None.

### 🟡 Warning

None (introduced by fix-wave-2).

### 🟢 Suggestion

None.

*Note: M-005 (no integration tests for `resolve_expression_target` routing) remains a pre-existing Warning from initial qc3, outside this targeted re-review scope per the "Do NOT Request Changes for pre-existing medium/low findings" rule.*

---

## Summary

| Severity | Count | Notes |
|----------|-------|-------|
| 🔴 Critical | 0 | C-NEW-001 closed by fix-wave-2 |
| 🟡 Warning | 0 | No new warnings introduced |
| 🟢 Suggestion | 0 | — |

**C-NEW-001 closure summary:**

| Criterion | Verdict |
|-----------|---------|
| 1. Per-source HashSet tracking | ✅ |
| 2. `source_id: &str` parameter | ✅ |
| 3. All 3 call sites updated | ✅ |
| 4. `wait_for_all` gate condition correct | ✅ |
| 5. Test helper bypass removed | ✅ |
| 6. Regression test: 3 distinct predecessors | ✅ |
| 7. Regression test: same source deduped | ✅ |

**Verdict**: **Approve**

**Rationale**: C-NEW-001 is demonstrably closed. The fix-wave-2 correctly replaces the global token-based dedup with per-source `HashSet` tracking, updates all call sites, removes the test helper bypass, and adds two targeted regression tests that exercise both the many-distinct-predecessor and idempotent-duplicate scenarios. All 11 converge tests pass; clippy is clean. No new Critical or Warning findings introduced.
