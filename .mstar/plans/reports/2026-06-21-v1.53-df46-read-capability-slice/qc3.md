---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: 2026-06-21-v1.53-df46-read-capability-slice
working_branch: feature/v1.53-df46-read-capability-slice
review_cwd: main worktree
review_range: e7b369d4..4507e58e
focus: performance/reliability
date: 2026-06-20
generated_at: 2026-06-20
verdict: Approve with Notes
---

# QC #3 Review — V1.53 P1 DF-46 Read Slice (performance/reliability)

## Summary

Reviewed the single P1 commit `4507e58e` on `feature/v1.53-df46-read-capability-slice`. The change adds five read-heavy `nexus.*` tools through the P0 registry seam and closes three P0 residuals by backfilling registry parity tests. Overall, the slice is mechanically sound: all 13 registry rows are present, the allowlist/registry bijection invariant is enforced by tests, and the new handlers follow the existing read-only admission pattern. Build, test, lint, and format gates all pass.

From a performance/reliability perspective, the main issue is `nexus.timeline.recent.get`, which fetches every timeline event for a world into memory and then slices client-side. For long-running worlds this is an unbounded allocation in the daemon process and should be capped server-side. The other notable item is the continued per-dispatch allocation cost of `host_tool_registry()` (the P0 deferred optimization `R-V153P0QC3-001`); adding five more tools did not regress the situation, but it also did not improve it. No blocking resource leaks or concurrency regressions were found.

## Verification evidence

```bash
cd /Users/bibi/workspace/organizations/42ch/nexus
git checkout feature/v1.53-df46-read-capability-slice
# HEAD: 4507e58efdada73155f2ba0afcd0ccbe60dfe87e

git log --oneline e7b369d4..4507e58e
# 4507e58e feat(v1.53-p1): add 5 read-heavy nexus.* tools + close 3 P0 residuals

git diff --stat e7b369d4..4507e58e
# crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs | 655 +++++
# crates/nexus-daemon-runtime/src/capability_registry.rs             | 247 +++++

cargo test -p nexus-daemon-runtime --lib capability_registry -- --test-threads=8
# test result: ok. 10 passed; 0 failed; 0 ignored

cargo test -p nexus-daemon-runtime --lib host_tool_executor -- --test-threads=8
# test result: ok. 26 passed; 0 failed; 0 ignored

cargo test -p nexus-daemon-runtime -- --test-threads=8
# test result: ok. 34 passed; 0 failed; 0 ignored
# Doc-tests: ok. 1 passed; 0 failed; 1 ignored

cargo check -p nexus-daemon-runtime
# Finished `dev` profile

cargo clippy -p nexus-daemon-runtime -- -D warnings
# Finished `dev` profile

cargo +nightly fmt --all -- --check
# (no output)
```

## Findings

### Blocking / High severity

None.

### Medium severity

#### R-V153P1QC3-001: `nexus.timeline.recent.get` loads entire timeline into memory before slicing

`execute_timeline_recent_get` calls `gw.get_timeline(world_id, None)` and then applies the user `limit` in Rust:

```rust
// crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1221-1232
let all_events =
    gw.get_timeline(world_id, None)
        .await
        .map_err(...)?;

// Return the most recent `limit` events
let recent: Vec<_> = all_events.iter().rev().take(limit).rev().cloned().collect();
```

`SqliteNarrativeGateway::get_timeline` (`crates/nexus-local-db/src/narrative_gateway.rs:213-273`) issues a `fetch_all` with no `LIMIT` clause:

```rust
sqlx::query_as!(TimelineEventRow,
    r#"SELECT ... FROM narrative_timeline_events WHERE world_id = ? ORDER BY branch_id ASC, sequence_no ASC"#
)
.fetch_all(&*self.pool)
```

The table has a `(world_id, branch_id, sequence_no)` index, so the query itself is efficient, but for a world with many timeline events the daemon will hold the full result set in memory, allocate a matching `Vec<TimelineEventRow>`, convert it to `Vec<TimelineEvent>`, reverse it, clone the tail, and serialize that. The default `limit` is 20, so the waste can be arbitrarily large. This confirms the open question from the fullstack-dev handoff.

**Fix direction:** Add a `limit: Option<usize>` parameter to `NarrativeGateway::get_timeline` and push the cap into the SQL `LIMIT` clause. The P1 handler can pass `limit` through, preserving the current semantics without the unbounded allocation.

### Low severity

#### R-V153P1QC3-002: `host_tool_registry()` still allocates a fresh registry on every dispatch

`HostToolExecutor::registry_dispatch` and `execute_daemon_health` both call `crate::capability_registry::host_tool_registry()`:

```rust
// crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:387
let reg = crate::capability_registry::host_tool_registry();
let dispatch_result = reg.dispatch(req, state, &creator_id).await;

// crates/nexus-daemon-runtime/src/api/handlers/host_tool_executor.rs:1326
let reg = crate::capability_registry::host_tool_registry();
serde_json::json!({
    ...
    "registry_size": reg.len(),
    "registry_ids": reg.ids().collect::<Vec<_>>(),
    ...
})
```

`host_tool_registry()` constructs a new `CapabilityRegistry`, 13 `CapabilityRow`s, their `Vec<AdmissionGate>`s, and a `HashMap<&'static str, CapabilityRow>` on every call. With 13 rows the absolute cost is still small (microseconds), but it is pure overhead and grows linearly with each new tool. This is the same deferred optimization identified in P0 (`R-V153P0QC3-001`); P1 did not regress it, but it also did not address it.

**Fix direction:** Replace the constructor with a `once_cell::Lazy<CapabilityRegistry>` or `std::sync::LazyLock` singleton. Because the registry contains only `&'static` data, this is safe and removes per-dispatch allocation entirely.

#### R-V153P1QC3-003: `nexus.kb_snapshot.read` relies on a runtime-formatted sqlx query

The handler delegates to `SqliteKbStore::list_by_world`, which uses a runtime `sqlx::query_as::<_, KeyBlockRow>()` with `format!` to inject the `LIMIT` constant:

```rust
// crates/nexus-local-db/src/kb_store.rs:427-451
let rows = sqlx::query_as::<_, KeyBlockRow>(&format!(
    r"SELECT ... FROM kb_key_blocks
       WHERE world_id = ?
         AND status NOT IN ('deleted', 'merged', 'deprecated')
       ORDER BY created_at ASC
       LIMIT {LIST_BY_WORLD_LIMIT}"
))
.bind(world_id)
.fetch_all(&*self.pool)
```

The `world_id` bind parameter is safe, and the result is capped at 500 rows by `LIST_BY_WORLD_LIMIT`, so connection-pool exhaustion and memory blow-up are bounded. However, the query is not compile-time checked and the `format!` usage is a mild maintainability/audit concern. This is pre-existing behavior that P1 now exposes through a registry tool; it does not block the slice but should be tracked.

**Fix direction:** Once the sqlx offline schema includes the provenance columns, convert this to `sqlx::query_as!` and bind `LIMIT` via a constant expression or a checked literal.

### Nit / observation

- **`nexus.manuscript.chapter.get` uses a runtime query for an indexed lookup.** `nexus_local_db::work_chapters::get_chapter` uses `sqlx::query()` against the composite PK `(work_id, volume, chapter)`. The query is parameterized and the table has the proper primary key and supporting indexes (`idx_work_chapters_next_volume_aware`), so performance is fine. It is pre-existing code, not introduced by P1.
- **`execute_daemon_health` hardcodes `"pool_healthy": true`.** This is cosmetic; the value does not actually check the pool. Given the tool is read-only and informational, this is acceptable for P1 but should be replaced with a real health probe if observability agents start relying on it.
- **New P1 tests are hermetic.** All new E2E tests use `create_test_workspace()` and clean up via `drop(tmp)`. Running the suite with `--test-threads=8` produced no flakes across multiple runs.
- **No resource leaks observed.** Handlers do not acquire locks, open files, or subscribe to channels. The `SqliteKbStore` constructed per call wraps an `Arc<SqlitePool>`; the `NarrativeGateway` is an `Arc` clone from `WorkspaceState`.

## Verdict

**Approve with Notes**

The P1 slice is functionally correct, test-covered, lint-clean, and introduces no blocking performance or reliability regressions. The only material concern is the unbounded `nexus.timeline.recent.get` memory footprint, which is a confirmed MEDIUM finding. The registry-allocation and runtime-query items are LOW and match the P0 deferred-optimization posture. I recommend approving with a residual tracking the timeline `LIMIT` fix for the next plan or a targeted follow-up.
