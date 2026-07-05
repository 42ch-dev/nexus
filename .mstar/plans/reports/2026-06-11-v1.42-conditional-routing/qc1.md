---
report_kind: qc-review
reviewer: qc-specialist
reviewer_index: 1
plan_id: "2026-06-11-v1.42-conditional-routing"
verdict: "Approve"
generated_at: "2026-06-11"
---

# Code Review Report

## Reviewer Metadata
- Reviewer: @qc-specialist
- Runtime Agent ID: qc-specialist
- Runtime Model: volcengine-plan/deepseek-v4-pro
- Review Perspective: Architecture coherence and maintainability risk
- Report Timestamp: 2026-06-11T20:30:00+08:00

## Scope
- plan_id: 2026-06-11-v1.42-conditional-routing
- Review range / Diff basis: merge-base: a7495b17 (P2 status commit) + tip: HEAD of iteration/v1.42 (7daf4b0f) — equivalent to git diff a7495b17...HEAD on .worktrees/v1.42-p2-qc. Covers 7 commits:
  - 5467eaa2 docs(spec): promote preset-conditional-routing to Draft V1.42 (T1)
  - e81412e6 feat(orchestration): add GoNogo conditional next for llm_judge (T2)
  - c8b1cb5c feat(orchestration): executor branch selection for GoNogo (T3)
  - 3153a7bd test(orchestration): hermetic tests for GoNogo conditional routing (T4)
  - de99587b docs(knowledge): update DF-56 row and §3.6.3 with V1.42 P2 shipped evidence (T5)
  - 6c8ca8ca merge(v1.42 P2): PM merge of feature branch into integration
  - 7daf4b0f harness(status): V1.42 P2 → InReview (PM merge complete; QC tri-review pending)
- Working branch (verified): HEAD (detached at 7daf4b0f)
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.42-p2-qc
- Files reviewed: 7 (3 Rust source + 1 spec + 1 tracker + 1 status + 1 plan)
- Commit range: a7495b17..7daf4b0f
- Tools run: cargo test (14 new + 15 regression all pass), cargo clippy (clean), cargo +nightly fmt --all --check (clean)

## Findings

### 🔴 Critical
None.

### 🟡 Warning

#### W-001: Context key string literals not extracted as named constants
- **Location**: `crates/nexus-orchestration/src/preset/loader.rs` lines 870–875, 914–919; `crates/nexus-orchestration/src/tasks/mod.rs` lines 776, 809, 843, 1777, 1821
- **Issue**: The context keys `"_judge_result"` and `"_judge_reason"` are used as bare string literals across two modules (loader.rs and tasks/mod.rs). The conditional edge closure in loader.rs reads `_judge_result` via `ctx.get_sync::<bool>("_judge_result")`, while tasks/mod.rs writes it via `context.set("_judge_result", ...)`. There is no named constant tying these together.
- **Risk**: If the key name is ever changed or a typo is introduced, the conditional edge will silently fall back to the `nogo` branch (due to `unwrap_or(false)`) with no compile-time or runtime error. This is a maintainability risk, not a correctness bug today.
- **Fix**: Define `pub const JUDGE_RESULT_KEY: &str = "_judge_result";` and `pub const JUDGE_REASON_KEY: &str = "_judge_reason";` in a shared location (e.g., `crates/nexus-orchestration/src/preset/mod.rs` or a new `constants.rs`), and reference them from both loader.rs and tasks/mod.rs.

### 🟢 Suggestion

#### S-001: Duplicated conditional edge wiring in build_outer_graph and build_wired_outer_graph
- **Location**: `crates/nexus-orchestration/src/preset/loader.rs` lines 860–877 and 906–923
- **Issue**: The same `match &state.next { ... GoNogo(...) => graph.add_conditional_edge(...) ... }` pattern appears verbatim in both `build_outer_graph` and `build_wired_outer_graph`. The closure `|ctx| ctx.get_sync::<bool>("_judge_result").unwrap_or(false)` is also duplicated.
- **Suggestion**: Extract a helper function `fn wire_state_edges(graph: &mut Graph, states: &[StateDefinition])` that both callers can use. This reduces the risk of future divergence between the two graph builders. Note: this is low-priority since the two functions serve different callers (hermetic tests vs production), but the duplication is unnecessary.

#### S-002: GoNogoNext struct doc could clarify nogo fallback semantics
- **Location**: `crates/nexus-contracts/src/local/orchestration/preset.rs` lines 284–290
- **Issue**: The `GoNogoNext` struct doc says "The nogo branch is taken on NOGO or worker-unavailable", but this is only documented on the struct field, not in the enum variant doc on `NextTarget::GoNogo`. The fallback behavior (absent `_judge_result` → nogo) is documented in the loader comment but not in the type definition.
- **Suggestion**: Add a brief note to the `NextTarget::GoNogo` variant doc: "When `_judge_result` is absent from context (e.g., worker unavailable), the `nogo` branch is taken." This makes the contract visible at the type level without reading loader implementation.

## Source Trace

| Finding ID | Source Type | Source Reference | Confidence |
|------------|-------------|------------------|------------|
| W-001 | manual-reasoning | Grep for `_judge_result` across loader.rs + tasks/mod.rs; no `const` definition found | High |
| S-001 | git-diff | `git diff a7495b17..HEAD -- crates/nexus-orchestration/src/preset/loader.rs` lines 860–877 vs 906–923 | High |
| S-002 | manual-reasoning | Read `crates/nexus-contracts/src/local/orchestration/preset.rs` lines 262–290 | Medium |

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 1 |
| 🟢 Suggestion | 2 |

### Architecture Assessment

The V1.42 P2 minimal slice for DF-56 conditional routing is architecturally sound:

1. **Variant design**: `GoNogoNext` is added as a new variant of the existing `NextTarget` enum (alongside `Linear` and `Conditional`). This is a clean extension — no separate field, no parallel type hierarchy. The `#[serde(untagged)]` deserialization naturally handles the YAML shape discrimination.

2. **Schema extension impact**: The change touches exactly the right layers:
   - `nexus-contracts` (data types): new `GoNogoNext` struct + `NextTarget::GoNogo` variant
   - `nexus-orchestration/loader.rs` (validation + graph wiring): 3 new validation rules + conditional edge wiring in both graph builders
   - `nexus-orchestration/validation.rs` (reachability): GoNogo branches included in adjacency list
   - `nexus-orchestration/tasks/mod.rs` (executor): `judge_next_action` method encapsulates branching logic

3. **Separation of concerns**: The `judge_next_action` method cleanly separates the "should we advance?" decision from the "where do we go?" decision (the latter is handled by the conditional edge in the graph). This is the right decomposition for future multi-branch support.

4. **Backward compatibility**: Linear `next` behavior is unchanged. The `NextTarget::Conditional` variant still rejects with `ConditionalNotYetSupported`. Existing `llm_judge` states without `GoNogo` next continue to work as before (GO advances, NOGO waits).

5. **Spec/code alignment**: The shipped code matches the Draft V1.42 spec exactly — `llm_judge` GO/NOGO → two `next` edges, with full expression-based routing deferred to post-V1.42.

6. **Test architecture**: 14 new tests cover the full matrix: happy path (load + wire), error paths (non-llm_judge, unknown go target, unknown nogo target, expression conditional still rejected), and 6 unit tests for `judge_next_action` covering all combinations of `next` type × judge result. All 15 existing `judge_llm` tests pass — no regression.

**Verdict**: Approve

The one Warning (W-001) is a maintainability concern about string literal context keys, not a correctness or architecture issue. It can be addressed in a future hygiene pass without blocking this merge. The two Suggestions are low-priority improvements.
