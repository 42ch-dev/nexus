---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-22-v1.56-df56-independent-slice"
verdict: "Request Changes"
generated_at: "2026-06-21"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: deepseek/deepseek-v4-pro
- Review Perspective: Performance & reliability risk (expression evaluator cache/perf, merge-point concurrency, runtime reliability, observability, regression)
- Report Timestamp: 2026-06-21T23:30:00Z

## Scope
- plan_id: 2026-06-22-v1.56-df56-independent-slice
- Review range / Diff basis: a457a8ee..4da874db
- Working branch (verified): iteration/v1.56
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus
- Files reviewed: 7
- Commit range: a457a8ee..4da874db (2 commits: `ee678812` implement + `4da874db` merge)
- Tools run: git diff, grep, read, cargo test -p nexus-orchestration --lib, cargo clippy -p nexus-orchestration -- -D warnings

## Findings

### 🔴 Critical

#### H-001 — Merge-point runtime implementation missing (AC gap)

**Location**: `crates/nexus-orchestration/src/tasks/mod.rs` — `StateCompositeTask` struct (line 618–644)

**Finding**: The `ConvergeConfig` / `ConvergeStrategy` wire types (`wait_for_all`, `first_completed`, `any`) are defined in `nexus-contracts` and the `converge` field exists on `StateDefinition`, but **no runtime merge-point logic is implemented in the orchestration engine**:

- `StateCompositeTask` struct has no `converge` field (no `ConvergeConfig` stored, no `ConvergeStrategy` evaluated).
- `from_manifest()` does not read `state.converge` (line 651–667).
- `resolve_expression_target()` does not track merge arrivals — unlike `resolve_labeled_target()` which records `_merge_<target>` keys, expression-routed branches have no arrival tracking.
- All 10 test fixtures in `tasks/mod.rs` set `converge: None`.
- The `Task::run` method has no converge gate (unlike the existing `merge` gate at line 967–1007 for labeled-edge merging).

**AC impact**: Plan AC states "Merge points accept multiple incoming edges; wait-for-all and first-arrival semantics are configurable and tested." This is **not met** — merge points are spec'd and typed but not implemented at runtime.

**Evidence**: `grep -rn "converge\|ConvergeStrategy\|wait_for_all\|first_completed" crates/nexus-orchestration/src/` returns only test fixture `converge: None` entries and a comment; zero runtime references to `ConvergeStrategy` variants.

**Fix**: Either (a) implement runtime converge logic in `StateCompositeTask`: store `converge` field, add a converge gate in `run()` analogous to the merge gate (lines 967–1007), implement `wait_for_all` (count arrivals), `first_completed` (first triggers, cancel others), and `any` (idempotent); add arrival tracking to `resolve_expression_target()` — OR (b) if converge is intentionally deferred, update the plan AC to explicitly scope it out and add a residual `R-V156P2-H001` noting the deferred merge-point work.

#### H-002 — Throttled llm_judge + Conditional/Branches next causes TaskExecutionFailed

**Location**: `crates/nexus-orchestration/src/tasks/mod.rs`, lines 1197–1215 (throttle path), lines 799–803 (`resolve_labeled_target` rejection)

**Finding**: When `exit_when: llm_judge` has a `min_interval` throttle and the previous judge result is reused, the throttle path matches `Conditional`/`Branches` in its `next` variant check (lines 1198–1205) and **calls `resolve_labeled_target()`** instead of returning `Continue`. `resolve_labeled_target` explicitly rejects `Conditional`/`Branches` with `TaskExecutionFailed` (lines 799–803).

**Trigger**: Valid preset with `exit_when: {kind: llm_judge, min_interval: ...}` + `next: {kind: conditional, ...}` or `next: {branches: ...}` — when the throttle activates, the state machine **hard-fails**.

**Root cause**: The throttle path was extended to include `Conditional`/`Branches` in the match arm (goes to `resolve_labeled_target`) but should instead return `NextAction::Continue` to let step 2.5 (line 1265–1273) call `resolve_expression_target()` for expression-based routing.

**Fix**: In the throttle `return Ok(TaskResult::new(...))` block (lines 1192–1215), split the `matches!` arm: for `Labeled`/`GoNogo` → call `resolve_labeled_target()`; for `Conditional`/`Branches` → return `NextAction::Continue` (or call `resolve_expression_target()` directly). Add a test exercising the throttled-judge + expression-routing combination.

### 🟡 Warning

#### M-001 — Expression AST not cached; re-parsed on every state transition

**Location**: `crates/nexus-orchestration/src/tasks/mod.rs`, line 865 — `crate::preset::expr::parse(&rule.when)`

**Finding**: Every call to `resolve_expression_target()` re-parses each `when` expression from its string form. For a preset with N branches and M transitions through the conditional state, parsing happens N×M times. The expression AST is immutable per preset load — parsing should happen once at preset load time and the AST stored on the task struct.

**Impact**: Linear in branch count per transition; negligible for 2–3 branches but compounds for deeply-branched graphs. No latency measurement exists (see M-005).

**Fix**: Parse expressions in `from_manifest()` (or a dedicated compile step), store `Vec<(crate::preset::expr::Expr, String)>` on `StateCompositeTask`, and evaluate pre-parsed ASTs at runtime.

#### M-002 — `build_context_json` allocates new `serde_json::Map` per evaluation

**Location**: `crates/nexus-orchestration/src/tasks/mod.rs`, lines 906–937

**Finding**: Every call to `resolve_expression_target()` builds a fresh `serde_json::Map` + `Value::Object` from known context keys. For long-running runs (10000+ state transitions), this accumulates unnecessary heap allocations. No reuse or pre-allocation strategy.

**Fix**: Consider a `SmallVec`-backed approach or reusing a pre-sized map. At minimum, pre-size the map with `with_capacity(known_keys.len())`.

#### M-003 — No integration tests for `resolve_expression_target` routing

**Location**: `crates/nexus-orchestration/src/tasks/mod.rs` — test module

**Finding**: The expression parser/evaluator has 38 unit tests in `expr.rs`, but **there are zero integration tests** that exercise `resolve_expression_target()` end-to-end through `Task::run()`. No test verifies:
- A state with `next: conditional` reaches the correct target based on expression evaluation
- A state with `next: branches` (Form B) correctly selects among >2 branches
- Expression evaluation failure falls back to `default`
- Interaction between expression routing and merge-point arrivals (see H-001)

**Fix**: Add integration tests in `tasks/mod.rs` that construct `StateCompositeTask` instances with `NextTarget::Conditional`/`Branches`, populate context, call `run()`, and assert `NextAction::GoTo(target)`.

#### M-004 — Expression evaluation failure is silent to operators

**Location**: `crates/nexus-orchestration/src/tasks/mod.rs`, lines 886–899

**Finding**: When expression parsing or evaluation fails within `resolve_expression_target()`, the branch is **silently skipped** with only a `tracing::warn!` log. If all branches fail, the `default` target is used. The operator has **no runtime visibility** into why routing chose the default path — the warn log goes to the tracing subscriber, which may not be visible in production. No context field is set (e.g., `_routing_error`) for downstream observability.

**Fix**: Write a structured error to context (e.g., `_routing_error` with parse/eval details) or include the failure reason in the task result response. Consider a `tracing::error!` for the all-branches-failed case.

#### M-005 — No benchmark coverage for expression evaluation (R-V156P1-L005 precedent)

**Location**: `crates/nexus-orchestration/` — no `benches/` directory

**Finding**: The V1.56 P1 residual `R-V156P1-L005` explicitly requires benchmarks for latency-sensitive code paths. P2 adds expression parsing/evaluation (hot path — called on every conditional state transition) with **zero benchmarks**. No criterion or iai benchmarks exist for:
- Expression parse latency (N-branch depth)
- Expression evaluate latency (nested field access, boolean short-circuit)
- `build_context_json` snapshot cost
- `resolve_expression_target` end-to-end latency

**Fix**: Add `criterion` benchmarks in `crates/nexus-orchestration/benches/` covering at minimum: parse + evaluate for 1/5/20-branch expressions, deeply nested field access (5+ levels), and boolean short-circuit paths.

#### M-006 — `build_context_json` hardcoded key list excludes user-set context values

**Location**: `crates/nexus-orchestration/src/tasks/mod.rs`, lines 913–928

**Finding**: The `known_keys` array only includes fixed orchestration keys (`_judge_result`, `_judge_reason`, etc.). Values set by `context_update` hooks or enter actions (e.g., arbitrary user-defined keys like `my_custom_field`) are **not visible** to expression evaluation. This limits expression expressiveness — the spec says expressions can reference "context state, stage output, and work metadata" but only a subset of context state is available.

**Fix**: Either (a) expose all context keys to expression evaluation (iterate over context entries rather than hardcoded keys), or (b) document the limitation and add a mechanism for users to declare which context keys expressions may reference in the preset YAML.

### 🟢 Suggestion

#### S-001 — Pre-parse expressions at preset load time

Parse-once strategy complements M-001. Store `Vec<ParsedRule>` on `StateCompositeTask` where each `ParsedRule` holds the pre-parsed `Expr` AST and the target string. Evaluate directly without re-parsing.

#### S-002 — Add `tracing::instrument` spans for expression routing latency

Add `#[tracing::instrument]` to `resolve_expression_target()` and `build_context_json()` with `level = "debug"`. This provides latency observability without requiring separate benchmarks for every code review.

#### S-003 — Fix doc comment on `resolve_expression_target`

The doc comment (line 848–850) says "Returns `Err` if no expression matches and no default is set" but the function signature returns `NextAction`, not `Result`. The fallback to `default` is always applied (line 903). Update the comment to match the implementation.

#### S-004 — `build_context_json` pre-size the map

Use `serde_json::Map::with_capacity(known_keys.len())` instead of `serde_json::Map::new()` to avoid reallocation during insertion.

#### S-005 — Expression evaluator: add `Display` impl for `Expr`

The `Expr` enum derives `Debug` but not `Display`. A `Display` impl would improve `tracing` log readability for expression trees in debug/warn messages.

## Source Trace
- Finding ID: H-001
- Source Type: manual-reasoning + git-diff
- Source Reference: `grep -rn "converge\|ConvergeStrategy" crates/nexus-orchestration/src/` — zero runtime references
- Confidence: High

- Finding ID: H-002
- Source Type: manual-reasoning + git-diff
- Source Reference: `tasks/mod.rs` lines 1197–1215 (throttle path) vs lines 799–803 (resolve_labeled_target rejection)
- Confidence: High

- Finding ID: M-001
- Source Type: manual-reasoning
- Source Reference: `tasks/mod.rs` line 865 — `crate::preset::expr::parse(&rule.when)` inside loop
- Confidence: High

- Finding ID: M-003
- Source Type: git-diff
- Source Reference: `grep "fn.*expression\|fn.*conditional_route\|fn.*branch" tasks/mod.rs` — no integration tests
- Confidence: High

## Summary
| Severity | Count |
|----------|-------|
| 🔴 Critical | 2 |
| 🟡 Warning | 6 |
| 🟢 Suggestion | 5 |

**Verdict**: Request Changes

**Rationale**: Two critical findings — H-001 (merge-point runtime missing, AC gap) and H-002 (throttled judge + Conditional/Branches routing causes hard failure). Additionally, six warnings covering expression caching, test coverage, observability, and benchmark compliance. The expression evaluator itself (parse/evaluate logic) is well-tested at the unit level (38 tests), and V1.42 GoNogo backward compatibility is preserved (verified via `cargo test -p nexus-orchestration --lib` — 813 passed, 0 failed).
