---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-19-v1.52-multi-branch-merge-semantics"
verdict: "Approve"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: performance and reliability risk
- Report Timestamp: 2026-06-19T23:30:00Z

## Scope
- plan_id: `2026-06-19-v1.52-multi-branch-merge-semantics`
- Review range / Diff basis: `b97ec0d9..93416cf8`
- Working branch (verified): `feature/v1.52-multi-branch-merge-semantics`
- Review cwd (verified): `/Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-tb-p1/`
- Files reviewed: 43 files changed, 6862 insertions, 407 deletions
- Commit range: `b97ec0d9..93416cf8`
- Tools run: `cargo test -p nexus-orchestration --lib preset::validation::stage_tests::merge_node`, `cargo test -p nexus-orchestration --lib preset::validation::stage_tests::quorum`, `cargo test -p nexus-orchestration --lib tasks::tests::resolve_labeled`, `cargo test -p nexus-orchestration --test labeled_routing`, `cargo test -p nexus-orchestration --lib preset::tests::all_embedded_presets_pass_strict_validation_gate`, `cargo test -p nexus-orchestration --lib embedded_presets`, `cargo test -p nexus-orchestration --lib` (715/715 passed, 1 ignored), `cargo test -p nexus-orchestration --tests` (all green), `cargo clippy -p nexus-orchestration --lib -- -D warnings` (clean), `cargo clippy -p nexus-orchestration --all-targets` (only pedantic warnings, see findings)

## Reviewer Parallel Review Focus

Performance and reliability of the merge-semantics runtime path introduced by
T-B P1 on top of T-B P0 N-way labeled routing. Specific concerns:

1. Merge node runtime overhead — is the per-fire tracking O(M) or O(M²)?
2. Reachability validator complexity — new cost vs old O(V+E).
3. `check_merge_node_integrity` performance — N merge nodes × M incoming edges.
4. Lock contention — any new locks or deadlock potential under concurrent execution?
5. Memory overhead per merge node — data structure overhead.
6. Quorum check frequency — per-fire? Cached?
7. Backward-compat regression — 6 embedded presets must still pass without
   modification.
8. Failure observability — tracing/log for merge completion? Counters?
9. Cold-path vs hot-path — merge node setup is cold path; runtime tracking is hot path.
10. Edge case: zero incoming edges — validator behavior; runtime behavior.

## Findings

### 🔴 Critical
*(none)*

### 🟡 Warning

**W-QC3-1: Cross-state label duplicates not validated — runtime merge stall for `All` / `Quorum` semantics**

- **Source**: Manual reasoning + diff review of validator vs runtime merge tracking
- **Location**: `crates/nexus-orchestration/src/preset/validation.rs` lines 444-466 (`check_labeled_edge_duplicates`) vs `crates/nexus-orchestration/src/tasks/mod.rs` lines 800-804 (arrival accumulation)
- **Details**:
  - The validator's `check_labeled_edge_duplicates` only inspects labels **within a single state's** `NextTarget::Labeled` edges (`for (i, state) in manifest.states.iter().enumerate()` with per-state `HashSet<&str>`). It does NOT compare labels **across states that converge on the same merge node**.
  - The runtime merge gate (`tasks/mod.rs:858-894`) computes `arrived_count` from a `Vec<String>` of *unique* matched labels — the `arrived.contains(&label)` guard at line 801 dedupes. The condition uses `arrived_count >= self.expected_incoming` (for `All`) or `arrived_count >= n` (for `Quorum`).
  - Concrete failure mode. Consider a preset:
    ```yaml
    - id: branch_a
      next:
        - label: pass          # duplicates with branch_b
          target: merge_x
    - id: branch_b
      next:
        - label: pass          # validator: OK (per-state unique)
          target: merge_x
    - id: merge_x
      merge: { kind: quorum, n: 2, m: 2 }
    ```
    - Validator: `m (2) == incoming (2)` ✓, no per-state dup ✓ → **passes**.
    - Runtime:
      - `branch_a` fires → resolve_labeled_target matches `"pass"` → `_merge_merge_x` → `arrived = ["pass"]` (1 unique).
      - `branch_b` fires → matches `"pass"` → `arrived.contains("pass")` true → **skip push** → `arrived = ["pass"]` (still 1 unique).
      - `merge_x` gate runs → `arrived_count (1) >= n (2)` → **false** → `WaitForInput` forever.
    - Same failure mode for `All`: `arrived_count (1) >= expected_incoming (2)` → false → stall.
  - This contradicts the validator's promise that `Quorum { n, m }` with `m = incoming` will trigger when all sources fire. The validator only checks the *count* of incoming edges, not the *cardinality* of unique labels they would produce.
  - Only `Any` semantics is safe because `arrived_count >= 1` is the threshold and the first arrival triggers regardless of label uniqueness.
- **Risk**: Silent runtime stall ("wait forever") for any preset that legitimately reuses labels across fan-in branches converging on a merge node (e.g., parallel sub-workflows both reporting `"success"` to a consolidation state, or N independent retry judges all returning `"retry"` to a merge point). Failure mode is observable via the `tracing::debug!("merge node waiting for more incoming labeled edges")` log + session-level timeouts, but **not** via preset-validation diagnostics.
- **Impact**: Medium-to-High. While the failure requires a non-trivial pattern (multi-source + duplicate labels + `All`/`Quorum`), the validator currently claims such presets are well-formed. Embedded presets do not exercise this path today, so the regression is latent.
- **Fix (recommended)**:
  - **Option A (preferred, validator-level)**: Extend `check_merge_node_integrity` to collect all labels pointing at each merge node across the whole manifest, and error on duplicate labels targeting the same merge node with non-`Any` merge kind. Pseudocode:
    ```rust
    // In check_merge_node_integrity, before the for-loop over states with merge:
    let mut labels_per_target: HashMap<&str, HashSet<&str>> = HashMap::new();
    for state in &manifest.states {
        if let Some(NextTarget::Labeled(edges)) = &state.next {
            for edge in edges {
                labels_per_target
                    .entry(edge.target.as_str())
                    .or_default()
                    .insert(edge.label.as_str());
            }
        }
        // (GoNogo contributes fixed labels "go"/"nogo", still need cross-state uniqueness check)
    }
    // Then, inside the merge-state loop, if not Any, assert
    // labels_per_target[state.id].len() == incoming.
    ```
  - **Option B (runtime-level)**: Track arrivals by **source state ID** rather than by matched label. Use a `HashSet<String>` of source state IDs accumulated in `_merge_<id>`. This decouples `arrived_count` from label uniqueness and matches the validator's `m = incoming` check semantically.
  - **Option C (less invasive)**: Document the constraint in `MergeKind` docstring (`preset.rs`) and in the spec overlay (`preset-conditional-routing.md` §3.2) — labels targeting a merge node with `All`/`Quorum` must be unique across all sources. Lower confidence; relies on preset authors reading the docs.

  Either A or B should be paired with at least one **integration test** that asserts the chosen behavior end-to-end. qc1's S-QC1-4 already flagged the absence of an end-to-end merge test; this finding compounds that gap with a concrete correctness scenario.

**W-QC3-2: `_merge_<id>` key string allocated per task tick — minor hot-path allocation**

- **Source**: Manual code review of `tasks/mod.rs` lines 859, 799
- **Location**: `crates/nexus-orchestration/src/tasks/mod.rs` line 859 (`format!("_merge_{}", self.id)` in merge gate) and line 799 (`format!("_merge_{target}")` in `resolve_labeled_target`)
- **Details**: The merge key `format!("_merge_{}", self.id)` is reallocated on **every** `Task::run` invocation of a merge node, and again on every labeled arrival. For a merge node that is re-evaluated on each engine tick while waiting for arrivals (graph_flow may re-enter on resume), this is a per-tick heap allocation that is purely a function of the static `self.id`.
- **Risk**: Negligible at current preset scale (≤10 merge nodes, IDs ≤ 32 chars, allocator is cheap). Becomes observable only at very high state-machine tick rates or with thousands of sessions. Not a blocker.
- **Impact**: Low.
- **Fix**: Pre-compute the merge key once in `StateCompositeTask::with_expected_incoming` (or store on the struct as `merge_key: Option<String>`) and reuse. Pure refactor; no semantic change. Optionally also pre-compute `_judge_label` writes (already static). Defer to a follow-up if profiling shows hot allocation pressure; current scale does not justify blocking.

**W-QC3-3: Dead-code warning introduced by this plan — `test_caps()` function left unused**

- **Source**: `cargo clippy -p nexus-orchestration --all-targets` (pedantic lint output)
- **Location**: `crates/nexus-orchestration/src/preset/validation.rs` line 1133 (`fn test_caps()`)
- **Details**: This plan replaced **11** call sites from `validate_preset_semantic(&manifest, &test_caps())` to `validate_preset_semantic(&manifest, &caps)` (with `let caps = CapabilityRegistry::with_builtins();` locally). All 11 callers are now using the built-in registry directly; `test_caps()` itself was **not** deleted. Result: a `dead_code` pedantic warning under the workspace's `clippy::pedantic` config.
- **Risk**: Compile-clean (`warn` not `error`), but adds noise to the workspace's `cargo clippy --all-targets` output and signals unfinished refactoring hygiene. This is a `mstar-coding-behavior` "Simplicity First" / "Surgical Changes" minor violation — the implementer updated the callers but did not finish the cleanup.
- **Impact**: Low (cosmetic). CI's `cargo clippy --all -- -D warnings` is currently masked by **pre-existing** clippy errors in `nexus-narrative` and `nexus-contracts` (not in this plan's scope), so this dead-code warning does not block CI today.
- **Fix**: Delete the now-unused `test_caps()` function (and any related imports if they become unused). Trivial mechanical change. Include in the same fix wave as the reliability fix for W-QC3-1.

### 🟢 Suggestion

**S-QC3-1: Pedantic clippy warnings introduced by this plan — minor cosmetic noise**

- **Source**: `cargo clippy -p nexus-orchestration --all-targets`
- **Locations & counts (all `warn`, none `error` under workspace config):
  - `crates/nexus-orchestration/src/preset/validation.rs` lines 1913, 1958, 1997, 2043, 2087 — 5× `clippy::unnecessary_raw_string_hashes` (raw string `r#""#` literals where `r""` would suffice because the YAML content contains no `"#` sequences; the hashes are unused).
  - `crates/nexus-orchestration/tests/labeled_routing.rs` lines 5, 61, 166 — 3× `clippy::doc_markdown` (the identifier `GoNogo` appears in doc comments without backticks).
- **Risk**: None. Cosmetic only. CI's `cargo clippy --all -- -D warnings` is masked by out-of-scope failures today (see W-QC3-3).
- **Fix**: Auto-fix with `cargo clippy --fix --allow-dirty --allow-staged -p nexus-orchestration`. Trivial.

**S-QC3-2: Add a structured counter for merge-node advances for ops visibility**

- **Source**: Manual review of observability surface in `tasks/mod.rs`
- **Location**: `crates/nexus-orchestration/src/tasks/mod.rs` line 889 (`tracing::info!("merge node condition met, advancing")`)
- **Details**: The current observability is `tracing::info!` on advance and `tracing::debug!` on wait. For production monitoring (e.g. alerting on stalls), a structured counter `_merge_<id>_total_advances` incremented on each successful advance would be more ergonomic than grepping logs.
- **Risk**: None. Pure additive.
- **Fix**: On line 893, add `context.set(format!("_merge_{}_total_advances", self.id), (existing_value + 1)).await;` (read-modify-write) before the `tracing::info!`. Optional in this iteration.

**S-QC3-3: Per-source arrival tracking would make W-QC3-1 fix self-contained**

- **Source**: Cross-cutting with W-QC3-1
- **Location**: `crates/nexus-orchestration/src/tasks/mod.rs` lines 795-804
- **Details**: If Option B from W-QC3-1 is chosen (track by source state ID), `_merge_<id>` accumulates a `HashSet<String>` of *source state IDs that have fired*, decoupling `arrived_count` from label semantics. This makes the merge gate semantically align with the validator's `m == incoming_count` check, and removes the cross-state-label-duplicate failure mode entirely.
- **Risk**: Slight change to the `_merge_<id>` shape. Downstream consumers (none currently) would need to read a `HashSet<String>` instead of a `Vec<String>`. Trivial migration if needed later.
- **Fix**: Pair with W-QC3-1's Option B implementation. This is the cleanest semantic alignment and is the reviewer's preferred remedy.

## Performance & Reliability Analysis (the reviewer's primary lens)

### Cold-path complexity (graph construction)

| Site | Complexity | Notes |
|---|---|---|
| `build_outer_graph` incoming edge count (`loader.rs:912-923`) | **O(V + E)** | Single pass: outer loop over `V` states, inner loop over edges `E`; HashMap insert is O(1) avg. |
| `build_outer_graph` second pass (`loader.rs:925-929`) | **O(V)** | HashMap lookup per state. |
| `build_wired_outer_graph` | Same as above (duplicate of the same logic). | Cold path; runs once per preset load. |
| `check_initial_to_terminal_reachability` (`validation.rs:234`) | **O(V + E)** | Now extended to handle `Labeled` edges; one extra `for edge in labeled` block adds O(degree) per state. |
| `check_labeled_edge_duplicates` (`validation.rs:447`) | **O(V × E_state)** | Per-state HashSet insert over each state's edges. Worst case O(V × max_degree) but in practice bounded by preset size. |
| `check_merge_node_integrity` (`validation.rs:475`) | **O(V + E)** | Pre-pass to count incoming edges + main loop over states. |

**Cold-path verdict**: Linear in V+E. No quadratic blow-up. Suitable for embedded preset validation on every daemon startup. ✅

### Hot-path complexity (runtime per-task-tick)

For each merge node `M` with `m_in` incoming labeled edges and per-tick cost:
1. `context.get(_merge_M_id).await` → DashMap shard lookup + `serde_json::from_value` of `Vec<String>` → O(m_arrived) where `m_arrived ≤ m_in` (cloned)
2. `MergeKind` arm + `arrived_count >= threshold` comparison → O(1)
3. On condition met: `context.set(_merge_M_id, Null)` → O(1)
4. On condition not met: return `WaitForInput` (no further work)

For each labeled-edge arrival into a merge target `M`:
1. `context.get_sync(_merge_M_id)` → O(m_arrived) deserialize + clone
2. `arrived.contains(&label)` → O(m_arrived)
3. `arrived.push(label)` if not present → O(1) amortized
4. `context.set_sync(_merge_M_id, arrived)` → O(m_arrived) write

**Hot-path verdict**: **O(m_arrived)** per arrival where `m_arrived ≤ m_in`. Total merge-gate cost across all arrivals: **O(m_in²)** worst case (each arrival scans the existing set). For typical `m_in ≤ 5`, this is ≤25 string comparisons — negligible. For `m_in = 20`, ≤400 comparisons — still fast (sub-microsecond on any modern CPU). No memory leaks: `arrived` Vec is bounded by unique labels and cleared on condition-met. ✅

### Lock contention

- `context.set_sync` / `context.get_sync` and `context.set` / `context.get` all operate on the same `DashMap<String, Value>` (verified at `~/.cargo/registry/.../graph-flow-0.2.3/src/context.rs:392`). DashMap uses per-shard locks; contention scales with shard count, not concurrent method calls.
- **No new locks introduced** by this plan. The merge gate reuses the same context infrastructure that was already shared across all state-machine operations.
- Sync vs async interleaving: an arrival may call `set_sync` while a merge gate reads via `get().await`. Both touch the same DashMap shard (same key). DashMap's shard locking handles this safely; worst case is a brief shard-level spin. **No deadlock potential** observed. ✅

### Failure modes (observability)

| Failure mode | Detected by | Surface |
|---|---|---|
| No-match in labeled routing | Deterministic `Err(GraphError::TaskExecutionFailed)` | `tracing::warn!` at `tasks/mod.rs:816` + error propagates to engine |
| Merge gate waiting | `NextAction::WaitForInput` return | `tracing::debug!("merge node waiting for more incoming labeled edges")` at line 871 |
| Merge gate advancing | `NextAction::Continue` after gate met | `tracing::info!("merge node condition met, advancing")` at line 889 |
| Quorum stall (cross-state label dup, see W-QC3-1) | Only via `WaitForInput` log + session timeout | `tracing::debug!` per tick; not surfaced as an error |
| Validator failures at load time | `PresetLoadError` propagation | Loud at preset load; rejects the preset before runtime |

**Verdict**: Failure observability is good **except** for the quorum-stall case flagged in W-QC3-1. Consider S-QC3-2 (advance counter) to make stalls more greppable.

### Cold-path vs hot-path optimization correctness

- **Cold path (preset load)**: HashMap pre-computation of incoming edge counts is O(V+E) — appropriate. The `with_expected_incoming` setter is `const fn` — good.
- **Hot path (runtime)**: The merge gate does minimal work — single context read, length comparison, conditional reset. Per-label arrival does one read + one contains + one push + one write. Both are dominated by DashMap I/O, not computation. ✅

### Edge case: zero incoming edges

- **Validator behavior** (`validation.rs:503`): `if incoming < 2` → `MergeIntegrity` error with message "requires at least 2 incoming labeled edges". Loud rejection. ✅
- **Runtime behavior** (defense in depth, in case validator is bypassed): If a `merge:` is declared on a state with 0 incoming edges, `expected_incoming = 0` is set by the loader. The gate condition becomes:
  - `All`: `arrived_count >= 0` → always true → **immediate advance** (no wait)
  - `Any`: `arrived_count >= 1` → false until first arrival, then true → advances on first arrival (safe behavior)
  - `Quorum { n, m }` with `m == 0`: `arrived_count >= n` — if `n >= 1`, never true → wait forever. The validator's `if *m != incoming` check at line 537 already catches this since `0 != 0` is false (passes), but `if *n < 1` at line 518 may not catch `n=1, m=0`. Actually rereading: the validator checks `m != incoming` only when merge is `Quorum`. With `incoming = 0` and `m = 0`, the check passes. With `n = 1, m = 0`, the validator catches `n < 1`? No — `1 < 1` is false, no error. So `Quorum { n: 1, m: 0 }` would pass validation but stall at runtime. Defense-in-depth gap.
- **Verdict**: Validator correctly blocks the common path. The `Quorum { n: 1, m: 0 }` edge case is not caught, but in practice presets with 0-incoming edges cannot form `next:` edges pointing nowhere — the YAML schema and the `>=2 incoming` validator guard reject this before runtime. Not blocking. Defensive runtime check (`expected_incoming > 0` for merge nodes with non-Any semantics) is a possible hardening but not required.

### Backward compatibility regression check

- **All 6 embedded presets** still load + pass strict validation gate:
  - `preset::tests::all_embedded_presets_pass_strict_validation_gate` ✓ (1 test passed)
  - `preset::tests::embedded_novel_writing_parses` ✓
  - `preset::tests::embedded_novel_review_master_loads_and_validates` ✓
  - `preset::tests::embedded_novel_chapter_review_loads_and_validates` ✓
  - `preset::tests::embedded_novel_brainstorm_loads_and_validates` ✓
  - `preset::tests::embedded_memory_augmented_loads_and_validates` ✓
- **`labeled_routing.rs` regression test `all_embedded_presets_still_parse_regression`** ✓ (5/5 passed)
- **`MergeKind::None` default behavior**: Confirmed via `merge_defaults_to_none_when_absent` test in `crates/nexus-contracts/src/local/orchestration/preset.rs` (existing fields like `merge: None` round-trip correctly without breaking older manifests).
- **GoNogo auto-conversion**: Confirmed working via `resolve_labeled_target_gonogo_auto_conversion_*` tests in `tasks/mod.rs`. Legacy binary GoNogo presets continue to route correctly via the labeled path. ✅

### Test coverage of new features

| Feature | Unit tests | Integration tests |
|---|---|---|
| `MergeKind` serde (All/Any/Quorum) | 7 (`parse_merge_*`, `merge_kind_roundtrip_*`, `merge_defaults_to_none_when_absent`) | n/a (type-level) |
| `LabeledNext` serde | 2 (`parse_labeled_next_n_way_*`, `parse_labeled_next_two_way_*`) + 1 backward-compat (`backward_compat_binary_gonogo_still_parses`) | n/a |
| `check_merge_node_integrity` rules | 5 (`merge_node_valid_all_with_2_incoming`, `merge_node_too_few_incoming_errors`, `quorum_n_exceeds_m_errors`, `quorum_n_zero_errors`, `quorum_m_mismatch_errors`) | n/a |
| `check_labeled_edge_duplicates` | 0 (no dedicated test; covered implicitly by other tests) | 0 (gap) |
| `resolve_labeled_target` runtime | 9 (single, multi, no-match, GoNogo auto-conversion × 3, label write, non-Labeled, None) | Covered by `labeled_no_match_does_not_stall_session` |
| Runtime merge gate end-to-end | 0 | **0 (gap, see S-QC1-4)** |
| Cross-state label uniqueness | 0 | 0 (gap, see W-QC3-1) |

**Coverage gap**: The runtime merge gate is exercised only at the unit level (`resolve_labeled_target` writes the right key) — no test confirms the gate actually **fires the `WaitForInput` vs `Continue` decision** correctly under realistic merge scenarios (multiple `llm_judge` sources converging). qc1's S-QC1-4 flagged this independently. The fix for W-QC3-1 should land alongside at least one such integration test.

## Source Trace
- Finding ID: W-QC3-1
- Source Type: manual-reasoning + cross-file review (validator vs runtime)
- Source Reference: `crates/nexus-orchestration/src/preset/validation.rs:447-466` (validator) vs `crates/nexus-orchestration/src/tasks/mod.rs:800-804` (runtime dedup) and `:858-894` (merge gate condition)
- Confidence: High

- Finding ID: W-QC3-2
- Source Type: manual-code-review
- Source Reference: `crates/nexus-orchestration/src/tasks/mod.rs:799, 859`
- Confidence: High (mechanism); Low (perf impact at current scale)

- Finding ID: W-QC3-3
- Source Type: linter (clippy `dead_code`)
- Source Reference: `crates/nexus-orchestration/src/preset/validation.rs:1133`
- Confidence: High

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 3 |
| 🟢 Suggestion | 3 |

**Verdict**: Request Changes

**Rationale**: One unresolved Warning (W-QC3-1) of medium-to-high reliability impact: a non-trivial preset pattern (multiple source states converging on a merge node with `All` or `Quorum` semantics and shared labels) would pass validation but stall indefinitely at runtime. The failure mode is not surfaced as an error — only as repeated `tracing::debug!` waits until session timeout. The fix is well-bounded (validator-level cross-state label-uniqueness check, or runtime-level source-ID tracking) and should land with an integration test exercising the merge gate end-to-end (compounding qc1's S-QC1-4 gap). W-QC3-2 (merge-key allocation) and W-QC3-3 (dead `test_caps`) are minor and can be bundled into the same fix wave. Performance characteristics are otherwise sound (O(V+E) cold path; O(m²) hot path with realistic m ≤ 5; no new locks; DashMap-shard contention only). Backward compatibility preserved (6/6 embedded presets pass).

### Performance/reliability assessment highlights

- ✅ Cold-path: O(V+E) HashMap pre-compute is appropriate; linear in manifest size.
- ✅ Hot-path: O(m_arrived) per arrival and O(1) per gate check; sub-microsecond for typical m ≤ 5.
- ✅ Memory: arrival Vec bounded by unique labels; cleared on condition-met; no leak.
- ✅ Locking: reuses existing DashMap; no new locks; no deadlock risk observed.
- ✅ Backward compat: all 6 embedded presets pass strict validation gate unchanged.
- ✅ Failure observability: deterministic errors for no-match; tracing at info/debug for advance/wait.
- ⚠️ W-QC3-1: validator/runtime mismatch on cross-state label duplicates — preset validation accepts patterns that stall at runtime.
- 🟢 S-QC3-1: pedantic clippy warnings (cosmetic).
- 🟢 S-QC3-2: advance-counter for ops monitoring (optional).
- 🟢 S-QC3-3: source-ID tracking aligns validator semantics with runtime (preferred W-QC3-1 remedy).

## Revalidation

**Type**: Targeted re-review (wave 2, fix commit `3ab67781`)  
**Date**: 2026-06-19  
**Fix scope**: `93416cf8..3ab67781` (fix-wave commit atop qc1/qc3 reports)  
**Files changed**: `crates/nexus-orchestration/src/preset/validation.rs` (+90/-10), `crates/nexus-orchestration/src/tasks/mod.rs` (+70/-3)

### Finding Disposition

#### W-QC3-1: Cross-state label duplicates — validator-level fix ✅ RESOLVED

**Fix**: `check_labeled_edge_duplicates` now performs a **cross-state** check in addition to the existing within-state check. A `HashMap<(&str, &str), &str>` maps `(target, label)` pairs to source state IDs; if any pair appears from two different source states, a `DiagnosticSeverity::Error` + `DiagnosticCategory::DuplicateLabel` diagnostic is produced.

**Evidence**:
- **Diff**: `validation.rs:469-497` — second loop over all states collects `(target, label)` → source ID, errors on collision
- **Test**: `cross_state_label_duplicate_errors` (validation.rs:2112) — creates a preset with two states (`a`, `b`) both emitting `label: foo` → `target: merged` (merge node with `kind: all`), asserts `DuplicateLabel` error with both state names in the message
- **Test run**: `cargo test -p nexus-orchestration --lib cross_state_label_duplicate_errors` → **1 passed, 0 failed**

**Assessment**: Fix correctly prevents the silent-runtime-stall class by rejecting the preset at validation time. The cross-state check covers the full surface area (any two states sharing a label → same merge target). The `DiagnosticCategory::DuplicateLabel` is already mapped to `Error` severity, which blocks preset load — appropriate for a correctness-required invariant.

**Remaining gap (not blocking)**: The fix follows "Option A" (validator-level). "Option B" (source-ID tracking at runtime) was not implemented — so the runtime still dedupes by label string. This is semantically correct **now** because the validator rejects duplicate labels before runtime, but the runtime's dedup-by-label remains a latent implementation detail. Tracked as S-QC3-3 (suggestion, not blocking).

#### W-QC3-2: `format!` per tick — pre-computed `merge_key` ✅ RESOLVED

**Fix**: `StateCompositeTask` gained a `merge_key: String` field, pre-computed once in `from_manifest` (`format!("_merge_{}", state.id)`) and reused in the hot-path merge gate and arrival handler — replacing two `format!(_merge_{})` calls that allocated per tick.

**Evidence**:
- **Diff**: `tasks/mod.rs:634-636` (field added), `tasks/mod.rs:657` (pre-computed in `from_manifest`), `tasks/mod.rs:863-864` (hot path uses `self.merge_key` and `context.get(&self.merge_key)`)
- **Regression**: All 717 lib tests pass; all 13 integration tests pass
- **Clippy**: `cargo clippy -p nexus-orchestration --lib -- -D warnings` → clean

**Assessment**: Straightforward performance hygiene. The field is always populated (`from_manifest` is the only constructor path), and the test constructor (`test_task`) also sets it. No behavioral change.

#### W-QC3-3: Dead `test_caps()` — removed ✅ RESOLVED

**Fix**: The `test_caps()` function (`validation.rs:1161-1163`) was deleted. All 11 call sites already used `CapabilityRegistry::with_builtins()` directly, making `test_caps()` a dead alias.

**Evidence**:
- **Grep**: `grep -n "fn test_caps" crates/nexus-orchestration/src/preset/validation.rs` → **NOT FOUND**
- **Clippy**: `cargo clippy -p nexus-orchestration --lib -- -D warnings` → clean (no dead_code warning)
- **Tests**: All 717 lib tests pass (including the 11 call sites that now use `CapabilityRegistry::with_builtins()`)

**Assessment**: Clean removal. No residual dead code.

### Regression Suite

| Command | Result |
|---------|--------|
| `cargo test -p nexus-orchestration --lib` | **717 passed, 0 failed, 1 ignored** |
| `cargo test -p nexus-orchestration --tests` | **13 passed, 0 failed** |
| `cargo clippy -p nexus-orchestration --lib -- -D warnings` | **clean** |
| `cargo test -p nexus-orchestration --lib cross_state_label_duplicate_errors` | **1 passed** |
| `cargo test -p nexus-orchestration --lib merge_wait_all_default_enforced_when_merge_absent` | **1 passed** |
| Embedded preset regression (`all_embedded_presets_pass_strict_validation_gate`) | **1 passed** |

### Revalidated Verdict

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 0 (all 3 resolved) |
| 🟢 Suggestion | 3 (unchanged, non-blocking: S-QC3-1 cosmetic clippy, S-QC3-2 advance counter, S-QC3-3 source-ID tracking) |

**All 3 Warning findings from qc3 resolved.** No new issues introduced by the fix wave. Tests confirm correctness and backward compatibility. 🟢 Suggestions remain non-blocking.

**Verdict**: **Approve**