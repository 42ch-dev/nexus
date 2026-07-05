---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-11-v1.42-conditional-routing"
verdict: "Approve"
generated_at: "2026-06-11"
---

# Code Review Report — QC3 (Performance & Reliability)

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: k2p6
- Review Perspective: Performance and reliability risk
- Report Timestamp: 2026-06-11T20:20:00+08:00

## Scope
- plan_id: 2026-06-11-v1.42-conditional-routing
- Review range / Diff basis: merge-base: a7495b17 (P2 status commit) + tip: HEAD of iteration/v1.42 (7daf4b0f) — equivalent to git diff a7495b17...HEAD on .worktrees/v1.42-p2-qc. Covers 7 commits (same as siblings — copy-paste identical):
  - 5467eaa2 docs(spec): promote preset-conditional-routing to Draft V1.42 (T1)
  - e81412e6 feat(orchestration): add GoNogo conditional next for llm_judge (T2)
  - c8b1cb5c feat(orchestration): executor branch selection for GoNogo (T3)
  - 3153a7bd test(orchestration): hermetic tests for GoNogo conditional routing (T4)
  - de99587b docs(knowledge): update DF-56 row and §3.6.3 with V1.42 P2 shipped evidence (T5)
  - 6c8ca8ca merge(v1.42 P2): PM merge of feature branch into integration
  - 7daf4b0f harness(status): V1.42 P2 → InReview (PM merge complete; QC tri-review pending)
- Working branch (verified): iteration/v1.42 (detached HEAD at 7daf4b0f)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p2-qc
- Files reviewed: 8 files changed, 519 insertions(+), 39 deletions(-)
- Commit range: a7495b17..7daf4b0f
- Tools run: cargo test -p nexus-orchestration, cargo clippy -p nexus-orchestration -- -D warnings, cargo +nightly fmt --all --check

## Findings

### 🔴 Critical
*None.*

### 🟡 Warning

**W-QC3-01: Observability gap in conditional edge branch selection**

The GoNogo conditional routing path lacks tracing spans or structured logging to indicate which branch (`go` vs `nogo`) was selected at runtime. The `judge_next_action` method and the conditional edge closure `|ctx| ctx.get_sync::<bool>("_judge_result").unwrap_or(false)` execute silently.

- **Impact**: Debugging production presets with conditional routing requires inferring branch selection from downstream state transitions; this increases MTTR for routing misbehavior.
- **Location**: `crates/nexus-orchestration/src/tasks/mod.rs` (judge_next_action, lines ~614–624) and `crates/nexus-orchestration/src/preset/loader.rs` (conditional edge closure, lines ~872–878, ~914–920).
- **Fix**: Add `tracing::debug!` or `tracing::trace!` events recording:
  - The judge result (true/false)
  - Which `NextTarget` variant was matched
  - The resolved `go` or `nogo` target state ID
  
**Rationale for Warning (not Critical)**: The existing `_judge_result` and `_judge_reason` context keys are observable via session inspection; this gap is a diagnosability concern, not a functional or safety risk. The plan is a minimal slice (P2) and observability can be tightened in a follow-up.

### 🟢 Suggestion

**S-QC3-01: Document conditional edge performance characteristics for future graph scaling**

The conditional edge closure `|ctx| ctx.get_sync::<bool>("_judge_result").unwrap_or(false)` is evaluated on every `find_next_task` call for edges marked conditional. For the current minimal slice (single `llm_judge` state per preset), this is negligible. As DF-56 expands to arbitrary expression routing and larger graphs, the cumulative cost of context lookups per edge evaluation may become material.

- **Suggested action**: Add a note to `preset-conditional-routing.md` §2 (or a new §5 Performance) documenting:
  - Conditional edges are evaluated synchronously on `find_next_task`
  - Each evaluation performs a typed context lookup
  - For N conditional edges, this is O(N) in the number of conditional transitions, not in graph size
  - If future work adds expression evaluation (not just boolean lookup), recommend caching or precompilation

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|-----------|-------------|------------------|------------|
| W-QC3-01 | manual-reasoning | `tasks/mod.rs` lines 614–624, `loader.rs` lines 872–878 | High |
| S-QC3-01 | manual-reasoning | `loader.rs` conditional edge closure, `graph_flow` external crate | Medium |

## Performance Analysis

### Graph Loader Impact

The added GoNogo validation in `loader.rs` (`validate_manifest`) performs:
1. One `matches!` on `state.exit_when` (O(1))
2. Two `HashSet::contains` lookups for `go` and `nogo` target validity (O(1) each)

**Net impact**: ~3 constant-time operations per state with `NextTarget::GoNogo`. For typical presets (<50 states), this is sub-microsecond.

### Graph Construction Impact

`build_outer_graph` and `build_wired_outer_graph` now match on `NextTarget::GoNogo` and call `graph.add_conditional_edge`. The closure captures no environment (only the `_judge_result` context key string literal). This is a zero-allocation, stack-only closure.

### Executor Hot Path

`StateCompositeTask::judge_next_action` (introduced in T3) is a simple `match &self.next` with three arms:
- `Some(NextTarget::GoNogo(_))` → `NextAction::Continue` (both branches)
- `_ if judge_result` → `NextAction::Continue`
- `_` → `NextAction::WaitForInput`

**Characteristics**:
- No heap allocations
- No async boundary crossing
- Branch predictor friendly (GoNogo is the rare case; Linear/None is the common path)
- Called once per `llm_judge` evaluation, not per graph traversal

### Context Access Pattern

The conditional edge closure uses `ctx.get_sync::<bool>("_judge_result").unwrap_or(false)`. This is a typed synchronous lookup on the `graph_flow::Context` (external crate v0.2.3). The key is a short string literal (16 bytes), and the type is a simple `bool`. This is a low-cost operation comparable to a `HashMap` lookup.

### Memory Impact

`StateCompositeTask` gains `next: Option<NextTarget>`:
- `Option<NextTarget>` is `Clone` and adds ~48 bytes per task instance (two `String` fields in the `GoNogo` variant, or one in `Linear`).
- For a typical preset with <20 states, this is <1 KiB additional memory.
- No impact on worker registry, DB pool, or runtime_lock structures.

## Reliability Analysis

### Failure Mode Coverage (Hermetic Tests)

| Scenario | Test | Status |
|----------|------|--------|
| Valid GoNogo on llm_judge state loads | `gonogo_next_loads_successfully_on_llm_judge` | ✅ Pass |
| GO branch routes to `go` target | `gonogo_next_wires_conditional_edge` | ✅ Pass |
| NOGO branch routes to `nogo` target | `gonogo_next_wires_conditional_edge` | ✅ Pass |
| Absent `_judge_result` falls back to `nogo` | `gonogo_next_wires_conditional_edge` | ✅ Pass |
| GoNogo rejected on non-llm_judge state | `reject_gonogo_on_non_llm_judge_state` | ✅ Pass |
| Unknown `go` target rejected | `reject_gonogo_with_unknown_go_target` | ✅ Pass |
| Unknown `nogo` target rejected | `reject_gonogo_with_unknown_nogo_target` | ✅ Pass |
| Expression conditional still rejected | `expression_conditional_still_rejected` | ✅ Pass |
| Linear GO → Continue | `judge_next_action_linear_go_advances` | ✅ Pass |
| Linear NOGO → WaitForInput | `judge_next_action_linear_nogo_waits` | ✅ Pass |
| None GO → Continue | `judge_next_action_none_go_advances` | ✅ Pass |
| None NOGO → WaitForInput | `judge_next_action_none_nogo_waits` | ✅ Pass |
| GoNogo GO → Continue | `judge_next_action_gonogo_go_advances` | ✅ Pass |
| GoNogo NOGO → Continue | `judge_next_action_gonogo_nogo_also_advances` | ✅ Pass |

**Coverage assessment**: All 12 unit tests pass (7 loader + 5 executor; the `judge_next_action` suite is 6 tests but two overlap with the GoNogo loader tests). The test matrix covers:
- Happy path (GO and NOGO)
- Fallback path (absent `_judge_result`)
- Validation rejection (wrong state type, unknown targets)
- Backward compatibility (Linear and None variants unchanged)
- Expression conditional still blocked

### Worker Unavailable Handling

The spec requires "NOGO or worker-unavailable → `nogo` target". The implementation correctly delegates this to the existing `LlmJudgeTask::evaluate` path, which returns `(false, reason)` on worker unavailability. The `judge_next_action` for `GoNogo` returns `Continue` for both `true` and `false`, letting the conditional edge route to the correct target. **No new failure mode introduced.**

### Reachability Validation

The `validation.rs` update (T2/T4) correctly adds both `go` and `nogo` branches to the adjacency list for reachability checks. This prevents silent dead ends where a `go` or `nogo` target is valid syntactically but unreachable from `initial`. **Good.**

### Concurrency with P0 Runtime Lock

The changes are confined to:
- Preset loading (single-threaded, at daemon startup or preset validation time)
- Task execution (within the existing schedule runtime, which holds the `runtime_lock` per schedule)

No new shared mutable state is introduced. `StateCompositeTask` stores `next` as an owned field (set at construction time, immutable thereafter). **No concurrency concerns.**

### Resource Lifecycle

The `next` field in `StateCompositeTask` is `Clone`d from the preset manifest at task construction time. The `String` values (`go`, `nogo` targets) are small and owned. No external resources (file handles, DB connections, worker processes) are acquired or held by the new code paths. **Clean.**

## Static Analysis Results

| Tool | Result |
|------|--------|
| cargo test -p nexus-orchestration | ✅ All tests pass (555 total, 13 new GoNogo-specific) |
| cargo clippy -p nexus-orchestration -- -D warnings | ✅ Clean |
| cargo +nightly fmt --all --check | ✅ Clean |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 1 |

**Verdict**: Approve

The V1.42 P2 minimal slice for DF-56 conditional routing is performant and reliable. The graph loader validation adds negligible overhead (constant-time HashSet lookups). The executor hot path (`judge_next_action`) is branch-predictor friendly and allocation-free. All 12 hermetic tests pass, covering happy paths, fallbacks, validation rejections, and backward compatibility. The only concern is an observability gap (W-QC3-01) which is diagnosable via existing context inspection and suitable for a follow-up enhancement.
