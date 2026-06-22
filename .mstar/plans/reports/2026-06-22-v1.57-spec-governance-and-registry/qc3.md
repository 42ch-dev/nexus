---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.57-spec-governance-and-registry"
verdict: "Approve with comments"
generated_at: "2026-06-22"
---

# QC3 Review — V1.57 P0 Spec Governance & Registry

## Reviewer Metadata

- **Reviewer**: @qc-specialist-3 (Reviewer #3, performance/reliability)
- **Runtime Agent ID**: qc-specialist-3
- **Review Focus**: performance/reliability
- **Report Timestamp**: 2026-06-22T23:45:00Z

## Scope

- **plan_id**: `2026-06-22-v1.57-spec-governance-and-registry`
- **Review range / Diff basis**: `merge-base: 4ab34c6c (P-1 sign-off commit)`, `tip: 56d459ec (P0 merge commit)` — equivalent to `git diff 4ab34c6c..56d459ec`
- **Working branch (verified)**: `iteration/v1.57` @ `eae09e74`
- **Review cwd (verified)**: `/Users/bibi/workspace/organizations/42ch/nexus`
- **Files reviewed**: 5 (3 spec .md files, 2 Rust source files)
- **Commit range**: `4ab34c6c..56d459ec` (3 commits: `43e4d89d` T1-T3 specs, `7e9932ce` T5-T6 invariant + field drops, `56d459ec` merge)
- **Tools run**: `cargo test -p nexus-orchestration --lib tasks` (×3 flakiness), `cargo test -p nexus-daemon-runtime --lib capability_registry` (×3 flakiness), `cargo test -p nexus-daemon-runtime` (full perf check), `cargo test -p nexus-orchestration --test capability_registry`, `cargo clippy -p nexus-orchestration -- -D warnings`, `cargo clippy -p nexus-daemon-runtime -- -D warnings`, `cargo +nightly fmt -p nexus-orchestration -- --check`, `cargo +nightly fmt -p nexus-daemon-runtime -- --check`

---

## Acceptance Criteria Checklist

| # | AC | Status | Evidence |
|---|----|--------|----------|
| AC1 | `agent-nexus-tool-bridge.md` header updated: Master class, status annotated | ✅ Met | Diff confirms `Document class: Master`, `Status: Master (V1.57 P-last promote draft; body content Master-ready; header rename in P-last)` |
| AC2 | Cross-references in `acp-capability-set.md` §0.5, `capability-registry.md` §4, `acp-client-tech-spec.md` updated | ✅ Met | §0.5 adds bridge pointer; `capability-registry.md` §4 boundary note updated; `acp-client-tech-spec.md` not changed (no bridge ref to update) |
| AC3 | `acp-capability-set.md` §4 replaced with roster table (36 rows) | ⚠️ Partial | Roster exists with 40 `nexus.*` capability rows + 1 `game_bible.*` entry (§4.3). Plan expects 36; actual is 40. See qc3-002. |
| AC4 | Roster status tags: 35 shipped, 3 scaffold-equivalent, 2 OUT, 0 deferred-to-V2.0+ | ⚠️ Partial | Actual counts: 18 shipped, 3 scaffold-equivalent, 2 OUT, 0 deferred, 17 catalog-only. Plan's "35 shipped" does not match roster reality. See qc3-002. |
| AC5 | All 35 implemented IDs have handler bindings in `capability/builtins/` | ❌ Not in scope | T4 (handler migration) was **not executed** in P0. The `capability/builtins/` directory already existed pre-P0 (P1 territory). No handler migration in P0 diff. See qc3-001. |
| AC6 | Each migrated handler has `CapabilityRegistryRow` with all 7 fields | ❌ Not in scope | Dependent on AC5 (T4 not done). See qc3-001. |
| AC7 | Each migrated handler has ≥1 success + ≥1 failure test via `CapabilityRegistry::dispatch()` | ❌ Not in scope | Dependent on AC5 (T4 not done). No per-ID test vectors added in P0 diff. See qc3-001. |
| AC8 | `R-V156P3-S003` field drops: unused capability fields removed; no regression | ✅ Met | `registry_output_to_context` now maps all 9 `RegistryRefreshOutput` fields (was 5); 4 previously dropped fields (`cache_age_ms`, `generated_at`, `fetch_timeout_ms`, `max_retries`) reinstated. No test regression. |
| AC9 | Catalog ↔ registry cross-validation test passes | ✅ Met | `catalog_registry_invariant_all_ids_present` passes in 0.00s. 4 catalog-only IDs flagged as INFO-level gaps (expected). |
| AC10 | `cargo test -p nexus-orchestration --test capability_registry` passes | ✅ Met | 4/4 integration tests pass in 0.00s. |
| AC11 | `cargo clippy -p nexus-orchestration -- -D warnings` passes | ✅ Met | Clean output; 0 warnings. |
| AC12 | `cargo +nightly fmt -p nexus-orchestration -- --check` passes | ⚠️ Partial | 5 files have pre-existing fmt diffs (8 diffs total). **None** of the fmt violations are in files or lines touched by P0. See qc3-003. |

**AC Summary**: 7 Met, 3 Partial, 3 Not in scope.

---

## Findings

### 🟡 Warning (Medium)

| ID | Severity | Title | Scope | Rationale | Suggested Action |
|----|----------|-------|-------|-----------|------------------|
| qc3-001 | medium | AC5-AC7 not delivered: handler migration and per-ID test vectors absent from P0 merge | `plan.md` §2 AC5-AC7; tasks T4-T5 | T4 ("Migrate handler bindings from `host_tool_executor.rs` to `capability/builtins/`") and T5 ("Add test vectors per ID") are explicitly listed as P0 tasks but were **not executed** in the P0 diff range. The 3 P0 commits cover only T1-T3 (spec governance) + T5 (partially: invariant test) + T6 (field drops). The `capability/builtins/` directory existed pre-P0 at the P-1 baseline (23 files under it). No new handler bindings or per-ID test vectors appear in the diff. This leaves AC5, AC6, and AC7 unmet, and reduces the P0 deliverable to a spec-only wave. | (a) If intentional deferral to P1: update plan AC checklist to mark AC5-AC7 as "deferred to P1" and add a carry-forward note; or (b) If oversight: implement T4-T5 in a follow-up P0 fix-wave before concluding this P0. |
| qc3-002 | medium | AC3/AC4 roster count mismatch: 40 capability rows in roster vs 36 expected per plan | `acp-capability-set.md` §4 roster table; plan.md §2 AC3-AC4 | The §4 roster contains **40 `nexus.*` capability rows** (plus 1 `game_bible.*` entry in §4.3): 18 shipped, 3 scaffold-equivalent, 2 OUT, 17 catalog-only. The plan states "36 rows" (AC3) with "35 = shipped" (AC4). The actual counts diverge significantly. The 17 `catalog-only` entries (logical contracts with no runtime binding) were likely not counted in the plan's "35 implemented" figure, but they are present in the roster. This could cause downstream confusion when P1/P3 reference the roster for handler binding targets. | (a) Verify the intended total (36 vs 40) with PM; update AC3/AC4 in plan if 40 is correct; (b) Or reduce roster to exactly 36 by merging or removing catalog-only entries. |

### 🟢 Suggestion (Low)

| ID | Severity | Title | Scope | Rationale | Suggested Action |
|----|----------|-------|-------|-----------|------------------|
| qc3-003 | low | AC12 partial: nightly fmt has pre-existing issues in non-P0 files | 5 files: `capability/builtins/mod.rs:44`, `capability/builtins/registry.rs:542`, `capability/mod.rs:296`, `tasks/mod.rs` (lines 2620,2660,2976,3025), `tests/novel_review_master.rs:172` | Nightly fmt check for `nexus-orchestration` fails with 8 diffs across 5 files. All violations are in files **not touched by P0** (the `capability/builtins/` directory pre-existed at P-1 baseline; the `tasks/mod.rs` issues are in test code at lines 2600+, well outside P0's change at lines 1227-1277). The `nexus-daemon-runtime` crate (which P0 actually touched) passes fmt cleanly. This is a pre-existing code hygiene issue, not a P0 regression. | No action required for P0 approval. PM should note these as pre-existing and schedule a fmt cleanup pass (low priority). |
| qc3-004 | low | Cross-validation test emits INFO-level warnings for 4 catalog-only IDs | `capability_registry.rs:995-1001` | The `catalog_registry_invariant_all_ids_present` test logs: `"catalog ids not yet in registry (future P1+ scope): [nexus.runtime.health, nexus.trace.correlation, nexus.world.state.query, nexus.workspace.paths]"`. These 4 IDs are correctly marked `catalog-only` in the roster — they are logical contracts with no runtime handler. The test correctly treats them as non-blocking (eprintln, not assert failure). However, the `is_likely_host_tool` match list at lines 977-989 names only 10 IDs; 4 of those 10 are missing from the registry. This match list will need maintenance as the registry grows. | Consider refactoring `is_likely_host_tool` into a data-driven lookup (e.g., derive from the roster's "Status" column) rather than a hardcoded match list. |
| qc3-005 | low | `registry_output_to_context` field-drop reintroduction adds 4 extra JSON field reads | `tasks/mod.rs:1250-1265` | The R-V156P3-S003 fix adds 4 additional `.get().cloned().unwrap_or(Null)` calls to the hot-path function `registry_output_to_context`. Each is an O(1) HashMap lookup on a small JSON object (~9 keys). The output JSON grows from 5 to 9 fields. Total overhead is below measurement threshold (<1 µs per call). No observable perf regression. | None required. Noted for completeness. If this function becomes a bottleneck in the future, consider deserializing to a typed struct instead of manual field-by-field extraction. |

---

## Detailed Notes

### Performance Analysis

#### 1. Cross-validation test (`catalog_registry_invariant_all_ids_present`) runtime cost

- **Runtime**: 0.00s (instant; below test harness resolution)
- **Algorithm**: O(N) where N = number of catalog rows + registry IDs (N ≈ 40). Linear scan of catalog markdown file via `.lines().filter_map()`, HashSet-based comparison.
- **Scalability**: Will not scale linearly with catalog growth because it reads the entire markdown file each run. However, at N=40, the cost is negligible. If the catalog grows to 100+ entries, the test would still complete in <10ms.
- **Hot path impact**: **None** — this is a `#[test]` function, compiled only for `cfg(test)`. It is never linked into production binaries.
- **Verdict**: O(N) but negligible at current scale. No concern.

#### 2. Test vector runtime cost

- **Per-ID test vectors**: **Not implemented** in P0. T4 (handler migration) and T5 (per-ID test vectors) are not in the P0 diff.
- **Existing test suite**: 77 tasks tests (0.00s), 11 capability_registry tests (0.16s), 4 integration tests (0.00s). Total orchestration test wall time < 1s.
- **If T5 were implemented**: 35 IDs × ≥2 tests each = ≥70 new tests. Estimated overhead ~0.5-1s additional wall time. Remains within acceptable CI budget.
- **Verdict**: N/A for this review (not delivered).

#### 3. Field-drop reintroduction runtime (`registry_output_to_context`)

- **Change**: 4 fields added: `cache_age_ms`, `generated_at`, `fetch_timeout_ms`, `max_retries`
- **Cost per call**: 4 × `serde_json::Value::get()` → O(1) HashMap lookups on ~9-key object. Each `.cloned()` creates a cloned `Value` (ref-counted internally for strings/numbers). Total per-call cost ≈ 0.1-0.5 µs.
- **JSON output**: Output grows from 5 keys to 9 keys. `serde_json::json!()` macro creates a new Map with 9 entries.
- **Call frequency**: Called from schedule-driven orchestration paths (preset state transitions). Not a tight loop.
- **Verdict**: Negligible overhead. No perf regression.

#### 4. Catalog ↔ registry dispatch overhead

- **Roster lookup**: The `CapabilityRegistry::dispatch` function does NOT read the catalog markdown file. The roster is a spec artifact, consumed only at compile time (cross-validation test) and by human readers.
- **Runtime dispatch path**: Uses `host_tool_registry()` → in-memory HashMap lookup → `dispatch()`. No catalog file I/O.
- **Verdict**: Zero runtime overhead from the catalog roster.

#### 5. Test count regression

| Test suite | Before P0 | After P0 | Change |
|-----------|-----------|----------|--------|
| `nexus-orchestration --lib tasks` | 77 | 77 | 0 |
| `nexus-daemon-runtime --lib capability_registry` | 10 | 11 | +1 (renamed + improved test) |
| `nexus-orchestration --test capability_registry` | 4 | 4 | 0 |

- **No test inflation**: The only net-new test is the renamed `catalog_registry_invariant_all_ids_present` (replacing the previous `registry_ids_have_catalog_rows` with a more comprehensive check and reduced known-gaps list). This is not a duplicate — it's an improvement.
- **CI impact**: None. All test suites complete in <5s combined.

#### 6. Flakiness Audit

| Test suite | Run 1 | Run 2 | Run 3 | Flaky? |
|-----------|-------|-------|-------|--------|
| `nexus-orchestration --lib tasks` (77 tests) | ✅ 0 failed | ✅ 0 failed | ✅ 0 failed | No |
| `nexus-daemon-runtime --lib capability_registry` (11 tests) | ✅ 0 failed | ✅ 0 failed | ✅ 0 failed | No |

All 3 consecutive runs of both suites pass with 0 failures. No flakiness detected.

#### 7. Full daemon-runtime perf check

- **`cargo test -p nexus-daemon-runtime`**: 34 unit tests + 2 doc tests. 34 passed, 1 doc-test ignored. Wall time: 38.48s (includes compilation from cold cache). Runtime after compilation: 4.82s (unit) + 0.17s (doc). No regression compared to expected baseline.
- **`cargo clippy -p nexus-daemon-runtime -- -D warnings`**: Pass.
- **`cargo +nightly fmt -p nexus-daemon-runtime -- --check`**: Pass (clean).

### Scope & Coverage

The P0 merge delivers **T1-T3 (spec governance) + T5-partial (cross-validation test) + T6 (field drops)**. The following plan items are **not delivered**:

- **T4**: Handler migration from `host_tool_executor.rs` to `capability/builtins/`
- **T5 (full)**: Per-ID success + failure test vectors dispatched through `CapabilityRegistry::dispatch()`

This means P0 is primarily a **spec-level wave** with a single code fix (field drops) and one improved test. The heavy implementation tasks (T4-T5) were deferred or belong to P1's scope.

### Cross-reference: known residual `R-V156P3-S003`

The field-drop fix (T6) correctly absorbs `R-V156P3-S003`. The `registry_output_to_context` function now maps all 9 `RegistryRefreshOutput` fields where previously only 5 were mapped. The 4 reinstated fields use the same safe `.get().cloned().unwrap_or(Null)` pattern as existing fields. No regression in existing tests.

---

## Verdict

**Approve with comments**

**Rationale**: P0's delivered changes are clean, performant, and meet the spec-governance objectives (T1-T3, cross-validation test, field-drop fix). No performance regressions, no flaky tests, no correctness issues. The three medium findings (qc3-001: AC5-AC7 not delivered, qc3-002: roster count mismatch) are scope-alignment gaps — they affect plan traceability but not code quality. The low findings are all observational (pre-existing fmt, INFO-level test logging, negligible overhead).

**Blockers**: None. No critical findings.

**Recommended follow-up before `Done`**:
1. PM to clarify whether AC5-AC7 (handler migration + per-ID tests) are deferred to P1 or need a P0 fix-wave.
2. PM to reconcile roster count (40 actual vs 36 per plan) and update AC3/AC4 accordingly.
3. Noted: pre-existing nightly fmt issues in 5 non-P0 files — low-priority hygiene.
