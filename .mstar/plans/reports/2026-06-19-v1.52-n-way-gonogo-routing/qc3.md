---
report_kind: qc
reviewer: qc-specialist-3
reviewer_index: 3
plan_id: "2026-06-19-v1.52-n-way-gonogo-routing"
verdict: "Request Changes"
generated_at: "2026-06-19"
---

# Code Review Report

## Reviewer Metadata

- Reviewer: @qc-specialist-3
- Runtime Agent ID: qc-specialist-3
- Runtime Model: minimax-cn-coding-plan/MiniMax-M3
- Review Perspective: Performance and reliability risk (hot-path overhead, resource lifecycle, unbounded operations, degradation & failure observability)
- Report Timestamp: 2026-06-19

## Scope

- plan_id: 2026-06-19-v1.52-n-way-gonogo-routing
- Review range / Diff basis: b97ec0d9..b21492b3
- Working branch (verified): feature/v1.52-n-way-gonogo-routing
- Review cwd (verified): /Users/bibi/workspace/organizations/42ch/nexus/.worktrees/v1.52-tb-p0
- Files reviewed: 5 source files + 4 harness/plans files (1 net new contract type, 2 orchestration files modified, 2 harness docs updated)
- Commit range (if not identical to Review range line, explain): b97ec0d9..b21492b3 (2 commits; 2425b12b is harness metadata only, b21492b3 is the feature drop)
- Tools run: `cargo test -p nexus-orchestration` (696 passed, 0 failed, 1 ignored — pre-existing network-bound), `cargo clippy --all -- -D warnings` (clean), `cargo +nightly fmt --all --check` (clean), targeted `cargo test -- preset::loader::tests / preset::validation::tests / all_embedded_presets_pass_strict_validation_gate` (all green)

## Findings

### 🔴 Critical

*(none)*

### 🟡 Warning

#### W-QC3-1: Labeled code paths have ZERO test coverage in the orchestration crate

**Scope:** `crates/nexus-orchestration/src/preset/{loader,validation}.rs` and `crates/nexus-orchestration/src/tasks/mod.rs`.

The plan's §4 #6 explicitly required "new test modules for `labeled_next` routing and `reachability_n_way`". The plan's T3 RED step required "Write failing test: preset YAML with 3-way labeled `next` edges loads and wires N conditional edges (TDD RED)". The plan's T5 RED step required three failing tests covering 3-way labeled preset, orphan label, and missing label coverage. **None of these tests were added.**

The only Labeled tests in the entire review range are 3 deserialization tests in `crates/nexus-contracts/src/local/orchestration/preset.rs`:
- `parse_labeled_next_n_way_from_yaml_list`
- `parse_labeled_next_two_way_like_binary_gonogo`
- `backward_compat_binary_gonogo_still_parses`

These cover **serde shape only** at the contract layer. They do NOT exercise the orchestration engine paths:
- `StateCompositeTask::resolve_labeled_target` (`tasks/mod.rs:782-791`) — the runtime hot path for all N-way routing. Has no unit test.
- `check_labeled_edge_duplicates` (`validation.rs:444-463`) — new validation. Has no unit test. `DiagnosticCategory::DuplicateLabel` is added but never asserted on.
- `check_initial_to_terminal_reachability` Labeled branch (`validation.rs:234-242`) — new BFS adjacency construction. Has no test.
- Loader rejection of `Labeled` on non-`llm_judge` state (`loader.rs:532-549`) — has no test (compare: `reject_gonogo_on_non_llm_judge_state` exists for the GoNogo case at `loader.rs:2604`).
- Loader rejection of `Labeled` with unknown target (`loader.rs:541-548`) — has no test.
- `build_outer_graph` / `build_wired_outer_graph` Labeled wiring (`loader.rs:907-916, 970-975`) — has no test. (Compare: `gonogo_next_wires_conditional_edge` exists for GoNogo at `loader.rs:2563`.)

**Reliability impact:** This is the first P0 tri-review block in V1.52 with no test exercising the new variant in the actual orchestration engine. If `resolve_labeled_target` regressed (e.g. returns `GoTo` on the wrong label, off-by-one in label iteration, substring ambiguity), no CI signal would catch it before merge. The plan's T3/T5 TDD contract was not honored.

**Suggested fix:** Add at minimum the following tests before approval:
1. `loader::tests::labeled_next_loads_successfully_on_llm_judge` — 3-way labeled preset on `llm_judge` loads.
2. `loader::tests::reject_labeled_on_non_llm_judge_state` — labeled next on `manual` exit_when is rejected.
3. `loader::tests::reject_labeled_with_unknown_target` — labeled edge to a nonexistent state is rejected.
4. `loader::tests::labeled_next_wires_unconditional_edges_for_reachability` — verify the graph has N edges from the labeled state.
5. `validation::tests::labeled_with_duplicate_label_is_error` — assert `DiagnosticCategory::DuplicateLabel` fires.
6. `validation::tests::labeled_reachability_bfs_uses_all_targets` — assert BFS reaches terminal via labeled targets.
7. `tasks::tests::resolve_labeled_target_matches_first_label` — substring match returns the right `GoTo`.
8. `tasks::tests::resolve_labeled_target_returns_wait_for_input_on_no_match` — no-match behavior.

#### W-QC3-2: Plan scope is partially unmet (deviations from stated design)

**Scope:** Plan `.mstar/plans/2026-06-19-v1.52-n-way-gonogo-routing.md` vs implementation.

Four plan-stated deliverables are not met by the implementation:

1. **Plan T2 #1 (Acceptance Criteria)**: "backward-compat deserialization accepts YAML `{ go: <s>, nogo: <s> }` and maps it to `LabeledNext { label: "go", target: <s> }` + `LabeledNext { label: "nogo", target: <s> }`." **Not implemented.** The implementation kept `NextTarget::GoNogo(GoNogoNext)` as a separate variant. Legacy `{ go, nogo }` YAML deserializes to `GoNogo(GoNogoNext)`, not auto-converted to `LabeledNext` entries. The test `backward_compat_binary_gonogo_still_parses` confirms this: the legacy shape parses as `GoNogo`, not `Labeled`. This is a defensible design choice (avoid forced migration), but it does not match the plan's stated AC.

2. **Plan T4**: "Update judge execution path to write `_judge_label` into context (in addition to existing `_judge_result` boolean for backward compat)." **Not implemented.** No code path writes `_judge_label` to context. The diff comment at `tasks/mod.rs:1041` is misleading: it says "write _judge_label and route via GoTo" but no `_judge_label` write occurs; the code calls `self.resolve_labeled_target(&reason)` directly. If a future feature wants to read `_judge_label` from context (e.g. downstream state templating), the absence will surprise.

3. **Plan T5**: "Add new validation check `check_labeled_edge_coverage`: for each `Labeled` edge, verify all labels referenced in `next` are producible; for each judge state, verify all producible labels have at least one edge." **Not implemented.** Only `check_labeled_edge_duplicates` was added. There is no orphan-label check (labels defined in `next` edges but the judge can never produce them) and no coverage check (judge can produce label X but no edge handles X). Both are reliability gaps: the runtime will silently `WaitForInput` on a non-matching label (see W-QC3-3).

4. **Plan §4 #3 (AC)**: "all 12 existing `judge_next_action` tests pass unchanged." **The number is wrong.** `tasks/mod.rs:3121-3171` contains exactly 6 `judge_next_action_*` tests, not 12. All 6 pass — the AC's intent is met but the count cited is inaccurate. PM should be informed so the AC can be corrected in the plan.

5. **Plan §4 #6 (AC)**: "with new test modules for `labeled_next` routing and `reachability_n_way`." **Not implemented** in `nexus-orchestration`. No such modules exist (only contracts-crate deserialization tests). See W-QC3-1 for the full list of missing tests.

**Reliability impact:** The plan's stated acceptance criteria for backward-compat semantics, judge context exposure, and validation coverage are unmet. A reader of the plan will expect behaviors (orphan label detection, `_judge_label` in context, binary→Labeled auto-conversion) that the implementation does not provide.

**Suggested fix:** Either (a) update the plan's ACs to reflect the simpler design actually shipped and re-justify the deviations, or (b) close the gaps in a follow-up commit before approval. PM decision required.

#### W-QC3-3: Labeled no-match silently stalls state via `WaitForInput`

**Scope:** `crates/nexus-orchestration/src/tasks/mod.rs:782-791` (and parallel throttled path at 1009-1018).

```rust
fn resolve_labeled_target(&self, judge_reason: &str) -> NextAction {
    if let Some(NextTarget::Labeled(edges)) = &self.next {
        for edge in edges {
            if judge_reason.contains(&edge.label) {
                return NextAction::GoTo(edge.target.clone());
            }
        }
    }
    NextAction::WaitForInput
}
```

When no declared label substring matches the judge output, the function returns `NextAction::WaitForInput`. For an `llm_judge` state this is **operationally wrong**:

- The judge has already produced output (the `WaitForInput` is on the task's exit, not the judge's).
- There is no external "Resume" mechanism for `llm_judge` states; the resume path at `tasks/mod.rs:813-819` requires the user/engine to set `_state_<id>_resumed: true` in context, which no `llm_judge` flow does.
- The result: a malformed judge output (judge returns plain text that doesn't contain any declared label) **silently stalls the entire session**. Recovery requires manual intervention — there is no diagnostic in logs unless someone traces `_judge_reason` and sees the mismatch.

**Substring ambiguity risk:** `str::contains` for label matching means a label `"outline"` is matched by judge outputs containing `"outliner"`, `"outline_check"`, etc. With overlapping or substring-related labels, **the FIRST match in declaration order wins**, not the most specific. `check_labeled_edge_duplicates` (`validation.rs:444-463`) only catches exact duplicates, not substring relationships. There is no warning for "label A is a substring of label B", which can lead to silent misrouting.

**Reliability impact:** Two failure modes — (a) no-match stall, (b) substring misrouting — both observable only at runtime, not at preset-load time. The orchestrator's auto-chain runs would be affected (one bad judge output → entire work pipeline blocks).

**Suggested fix:**
1. For no-match: return `NextAction::End` with a clear log line OR define a `default_target: Option<String>` field on the `Labeled` variant that the validator requires when no-match is otherwise fatal. At minimum, log `tracing::error!` with the state's id and the judge's text on no-match.
2. For substring ambiguity: add `check_labeled_substring_conflicts` (severity: Warning) that detects "label A is a substring of label B" within a single state. Preset author gets a load-time warning.
3. Consider token-anchored label matching: e.g. `<label>` in judge output for O(1) lookup and zero ambiguity.

#### W-QC3-4: Reachability BFS wires N unconditional edges that the runner never uses

**Scope:** `crates/nexus-orchestration/src/preset/loader.rs:907-916, 970-975` (and the equivalent for `build_outer_graph`).

```rust
Some(NextTarget::Labeled(ref labeled_edges)) => {
    // V1.52 T-B P0: N-way labeled routing.
    // Each labeled edge gets a regular add_edge for reachability
    // validation. Actual routing is via NextAction::GoTo(target)
    // in StateCompositeTask::judge_next_action, keyed by
    // _judge_label in context.
    for edge in labeled_edges {
        graph.add_edge(&state.id, &edge.target);
    }
}
```

**Performance impact (hot path? cold?):** The `add_edge` calls in `build_outer_graph`/`build_wired_outer_graph` are on the **preset load path** (cold). Each `add_edge` takes a `Mutex<Vec<Edge>>` lock (`graph-flow-0.2.3/src/graph.rs:74`); for a Labeled state with N edges, that's N lock acquisitions. Per preset: sub-millisecond for typical N (2-10). For a "kitchen sink" preset with 1000 labeled edges on one state: 1000 lock acquisitions, ~milliseconds. **Acceptable for cold path; not measurable as a regression for hot path.** The `add_edge` loop is NOT called per execution — only per preset load. The runtime path uses `GoTo(target)` from `resolve_labeled_target` and never calls `find_next_task` for Labeled.

**Reliability impact (deeper concern):**
- The edges added to `graph_flow::Graph.edges` for a Labeled state are **unconditional** and **never read at runtime** for Labeled routing (the engine honors `GoTo(target)` directly, not `find_next_task`).
- However, `graph_flow::Graph::find_next_task` (graph-flow/src/graph.rs:304-316) DOES read these edges — it picks the first unconditional edge as the fallback if no conditional matches. If any other code path (debug tool, future executor) calls `find_next_task` for a Labeled state, it will return the **first declared label's target**, regardless of judge output.
- This is a **latent ambiguity**: the graph appears to support 100-way routing but `find_next_task` would only return the first. Any code that depends on `find_next_task` for Labeled states will misroute.

**Suggested fix:**
- Document this explicitly in the function comment: "Edges added for Labeled states are reachability-validation only; routing is via GoTo. find_next_task() returns the first labeled target as a fallback, not the judge's resolved target."
- If you want to prevent latent misrouting: instead of `add_edge` per label, register a single `add_conditional_edge` with a closure that uses `_judge_label` (the value `resolve_labeled_target` would write to context). This requires `resolve_labeled_target` to set `_judge_label` in context before returning `GoTo` — which brings the design back to the plan's T4 design (see W-QC3-2 item 2).
- Alternatively, accept the design as-is and add a doc comment to the public graph struct warning that `find_next_task` is not a reliable routing API for Labeled states.

#### W-QC3-5: String-matching overhead in `resolve_labeled_target` — O(N × M) per fire

**Scope:** `crates/nexus-orchestration/src/tasks/mod.rs:782-791`.

For each labeled edge, `judge_reason.contains(&edge.label)` scans the judge output. Complexity: **O(N × M)** where N = number of labels, M = judge output length (up to 5KB for an LLM judge).

Concrete estimates:
- Typical N=3 labels, M=5KB: ~15K char comparisons → **sub-microsecond** (negligible).
- Stress N=100 labels, M=5KB: ~500K char comparisons → **sub-millisecond** (still negligible at 100 fires/min = ~50M comparisons/sec).
- Pathological N=1000, M=10KB: ~10M char comparisons per fire → **~10ms per fire** (could matter at 1000 fires/min → 16% CPU on one core).

**Performance impact:** Acceptable for the current scope (typical 2-5 labels). Not a regression compared to GoNogo (which uses a single boolean context read, but judge LLM latency dominates total cost by 1000x). **Suggestion-level concern, not Warning-level on its own** — but flagged here because the plan said labels would be "anchored for O(1) lookup" (implicit in design discussion) and that did not happen.

**Resource lifecycle / unbounded operations:**
- Number of labels per state: not capped by validator or schema. A malformed preset with 100,000 labeled edges would compile, validate (duplicate check is O(N) per state, so 100K × O(1) hashset insert = manageable), but at runtime each `judge_reason.contains` call would scan linearly.
- **No upper bound on label count is enforced.** A author could accidentally create 10,000 labels and never know until runtime.

**Suggested fix:**
1. Add a `MAX_LABELED_EDGES_PER_STATE = 256` (or similar) cap in the loader, with a clear error message.
2. Or, if scaling is the goal, replace `Vec<LabeledNext>` lookup with a `HashMap<&str, &str>` (label → target) precomputed at load time, plus a single Aho-Corasick or regex pass for first match.

### 🟢 Suggestion

- **S-QC3-1: The diff comment at `tasks/mod.rs:1041` is misleading.** It says "write _judge_label and route via GoTo" but no `_judge_label` is written. Either remove the comment fragment or actually write the context key. As written, it is documentation rot.

- **S-QC3-2: The plan §4 #3 AC says "12 existing `judge_next_action` tests" — there are 6.** Plan AC drift is itself a maintainability risk for future review. PM should correct the AC count in a follow-up plan revision.

- **S-QC3-3: `cargo +nightly fmt --all --check` was not explicitly run by the implementer in the diff history (only the standard formatter would be invoked).** It happens to be clean, but a `cargo +nightly fmt --all` invocation is not in the commit message and not verified in CI per the AGENTS.md note. CI is presumed to enforce; the QC3 workflow did not confirm.

- **S-QC3-4: The `Labeled` variant places no constraint on the `label` field's character set.** Authors could use Unicode, control characters, or extremely long strings. Adding a `max_label_length` and a regex check (e.g. `^[a-z][a-z0-9_]*$`) at load time would prevent foot-guns.

- **S-QC3-5: The `add_edge` calls in `build_outer_graph` (line 913) and `build_wired_outer_graph` (line 973) are duplicated.** A small helper `fn wire_labeled_edges(graph: &Graph, source: &str, edges: &[LabeledNext])` would DRY this and centralize the comment about reachability-only semantics.

- **S-QC3-6: Consider exposing a `LabeledEdges::targets() -> impl Iterator<Item = &str>` accessor** so external tooling (e.g. preset linting CLI) can introspect declared label targets without duplicating the match on `NextTarget` variants.

## Source Trace

- Finding W-QC3-1: Manual review of `validation.rs:444-463` (no test in `mod tests` at 1028), `tasks/mod.rs:782-791` (no test after line 3102), `loader.rs:532-549` (no test in `mod tests` after `gonogo_next_wires_conditional_edge` at line 2563). Cross-checked against `cargo test` output: only 3 Labeled tests in the entire diff (all in `nexus-contracts`). Confidence: High.
- Finding W-QC3-2: Comparison of plan `2026-06-19-v1.52-n-way-gonogo-routing.md` §2 #1, §4 #3, §4 #4 (b)(c), §4 #6, T2 #1, T4, T5 against actual implementation. Confidence: High.
- Finding W-QC3-3: `tasks/mod.rs:782-791` — direct read. Resume path at 813-819 does not re-evaluate judge output; requires manual context set. Substring ambiguity confirmed by `str::contains` semantics. Confidence: High.
- Finding W-QC3-4: `graph-flow-0.2.3/src/graph.rs:73-79` (Mutex<Vec<Edge>> push) and `:304-316` (find_next_task returns first unconditional). Loader comments at 911-912 acknowledge reachability-only purpose. Confidence: High.
- Finding W-QC3-5: `tasks/mod.rs:782-791` — direct read. No upper bound on label count in schema or validator. Performance estimate: O(N × M) for `str::contains` per label. Confidence: High (estimate), Medium (pathological-case relevance).
- W-QC3-1 / W-QC3-4 are reinforced by `cargo test` output: 696 passed, 0 failed, 0 Labeled tests in the orchestration crate.

## Summary

| Severity | Count |
|----------|-------|
| 🔴 Critical | 0 |
| 🟡 Warning | 5 |
| 🟢 Suggestion | 6 |

**Verdict**: Request Changes

The implementation is **structurally correct, clippy-clean, fmt-clean, and all existing tests pass** (696 in `nexus-orchestration`, 0 failed). The new `Labeled` variant deserializes correctly and the reachability BFS is implemented for N-way. However, **the new code paths are untested at the orchestration layer** (W-QC3-1), **the plan's stated acceptance criteria are partially unmet** (W-QC3-2), and the runtime behavior has **two silent failure modes** (W-QC3-3) and **one latent routing ambiguity** (W-QC3-4). Performance is acceptable for typical N (W-QC3-5 is a scaling concern, not a current bug). The plan's T3/T5 TDD contract was not honored — fix tests first, then merge.

Per `mstar-review-qc` §门禁规则: unresolved Warning findings block Approve. Verdict: **Request Changes** until at least W-QC3-1 (test coverage) and W-QC3-3 (silent stall) are addressed, or until PM accepts the gaps as residual risk via the residual_findings process per `mstar-plan-artifacts`.
