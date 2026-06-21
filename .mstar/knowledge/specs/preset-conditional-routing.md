# Preset Conditional Routing — Specification

**Status**: Shipped (V1.42 P2 — 2026-06-12; `llm_judge` GO/NOGO → two `next` edges; V1.52 T-B P0 — 2026-06-19; N-way labeled routing; V1.52 T-B P1 — 2026-06-19; multi-branch merge semantics; full DF-56 roadmap in deferred tracker §3.6.3)  
**Document class**: Feature line (conditional routing — minimal slice → N-way labeled → merge semantics)  
**Created**: 2026-06-06  
**Last updated**: 2026-06-19 (V1.52 T-B P1: merge semantics shipped, §3.2 added)
**Tracker**: DF-56 (conditional routing / branching engine)  
**Scope**: Preset `next.kind: conditional` loader + runtime evaluator (future iteration)  
**Coordinates with**:

- [orchestration-engine.md](orchestration-engine.md) §7.5 — current linear-only contract; this doc is the future normative target when conditional routing ships
- [creator-workflow.md](creator-workflow.md) — linear creator workflow stages (shipped V1.34); conditional routing layers beneath, does not replace FL-E enum in the first ship slice
- [deferred-features-cross-version-tracker.md](../deferred-features-cross-version-tracker.md) — DF-56, DF-29, DF-31

**Historical note**: V1.35 Prepare captured exploration in `archived/knowledge/fl-d-conditional-routing-exploration-v1.35-prepare.md`. This file is the long-term SSOT.

---

## 1. Purpose

Authors need presets that branch on runtime signals (judge outcome, tool result, user input) without spawning separate schedules or a manual `creator run <preset_id>` dispatch.

**V1.42 P2 shipped minimal slice** (2026-06-11):

- `llm_judge` states with `next: { go: <state>, nogo: <state> }` now accepted by loader.
- Graph wires a conditional edge using `_judge_result` from context.
- GO → `go` target; NOGO or worker-unavailable → `nogo` target.
- Only valid on `exit_when: { kind: llm_judge }` states. Full expression-based conditional routing remains post-V1.42 (see §3.6.3).
- Plan: [2026-06-11-v1.42-conditional-routing.md](../../plans/2026-06-11-v1.42-conditional-routing.md).

Pre-V1.42 state:

- Preset loader rejected `next.kind: conditional` with `ConditionalNotYetSupported`.
- Shipped creator workflow uses linear stage enum + explicit `creator run <preset_id>` dispatch (DF-53 auto-chain still open).
- V1.32 validator catches invalid preset graphs at load time for **linear** graphs only.

---

## 2. Current state (V1.42 P2 shipped)

| Area | State |
| --- | --- |
| Preset loader | Accepts `next: { go: <state>, nogo: <state> }` on `llm_judge` states; still rejects expression-based `ConditionalNotYetSupported` |
| Creator workflow stages | Linear enum; explicit `creator run <preset_id>` dispatch |
| Quality gate | V1.32 validator: reachability, terminal consistency, asset sandbox (linear graphs) |
| Conditional edge runtime | `graph_flow::add_conditional_edge` reads `_judge_result` from context |
| Open deferrals | DF-29 (`registry.refresh`), DF-31 (`workspace.*`), **DF-56** (full expression routing — post-V1.42) |

---

## 3. Target semantics (future normative)

When Status advances to **Draft** or **Normative**, orchestration-engine §7.5 defers to this document for the full conditional `next` schema.

### 3.1 N-way labeled routing (Draft V1.52 overlay — shipped T-B P0)

**Status**: Draft (V1.52 T-B P0 shipped — implemented in plan `2026-06-19-v1.52-n-way-gonogo-routing`)
**Authoring plan**: `2026-06-19-v1.52-n-way-gonogo-routing`
**Promotes to Normative**: P-last of V1.52

N-way labeled routing generalizes the binary GO/NOGO routing into multi-label routing for `llm_judge` states. The judge returns a label string (e.g. `"outline"`, `"research"`, `"abandon"`), and the runtime selects the first matching `LabeledNext` edge.

#### §3.1.1 Wire format: `LabeledNext`

```yaml
next:
  - label: outline
    target: outlining
  - label: research
    target: gathering
  - label: abandon
    target: archive
```

Each `LabeledNext` has:
- `label: String` — the label string the judge must output to select this edge
- `target: String` — the target state ID when the judge returns this label

The legacy binary `{ go: <s>, nogo: <s> }` shape (GoNogo) is auto-converted at runtime: `resolve_labeled_target` treats GoNogo edges as labeled edges with labels `"go"` and `"nogo"`. Legacy presets are therefore reachable via either routing API (boolean `_judge_result` or labeled string matching).

#### §3.1.2 Runtime: `resolve_labeled_target`

When `next` contains `Labeled` (or `GoNogo` via auto-conversion), the orchestration runtime calls `StateCompositeTask::resolve_labeled_target(context, judge_reason)`:

1. Collects all (label, target) pairs from the state's `next` edges, sorted by **descending label length** (to prevent shorter labels like `"go"` from matching as substrings of longer labels like `"nogo"`).
2. Scans the judge's output text (`_judge_reason` in context) for each label string using substring matching (`contains()`).
3. On first match: writes the matched label to `context._judge_label` and returns `NextAction::GoTo(target)`.
4. On no match: logs `tracing::warn!` with the known labels and a judge output excerpt, then returns `Err(GraphError::TaskExecutionFailed(...))` — **deterministic branch fail** (no silent `WaitForInput` stall). The error message includes the list of known labels and the judge output excerpt.
5. On non-`Labeled` / non-`GoNogo` next (e.g., `Linear`, `None`): returns `Ok(NextAction::WaitForInput)`.

**Backward compatibility**: legacy GoNogo presets continue to use the boolean `_judge_result` path via `judge_next_action(result)`. The auto-conversion allows the same presets to also work with the labeled path if called from `resolve_labeled_target`.

**Reachability**: the preset loader adds simple edges (`add_edge`) for each `Labeled` target, so the existing BFS reachability validator covers all labeled branches without needing separate conditional edge wiring. Duplicate labels within the same state are caught by `check_labeled_edge_duplicates` at validation time.

**Substring matching caveat**: matching uses `String::contains()` (substring containment). Authors should choose labels that are unlikely to appear as substrings of unrelated words. The descending-length sort mitigates the most common case (e.g., `"nogo"` checked before `"go"`). A future iteration may add word-boundary or exact matching.

### 3.2 Merge semantics (Draft V1.52 overlay — shipped T-B P1)

**Status**: Draft (V1.52 T-B P1 shipped — implemented in plan `2026-06-19-v1.52-multi-branch-merge-semantics`)
**Authoring plan**: `2026-06-19-v1.52-multi-branch-merge-semantics`
**Promotes to Normative**: P-last of V1.52

When multiple `LabeledNext` edges from different `llm_judge` states converge on a single state, the orchestration engine uses merge semantics to decide when to advance to that state.

#### §3.2.1 Merge node declaration

States declare merge semantics via the `merge:` field, using an internally-tagged enum (`kind`):

```yaml
# wait-all (default when merge: is absent)
merge:
  kind: all

# advance on first arrival
merge:
  kind: any

# quorum: at least n of m arrivals
merge:
  kind: quorum
  n: 2
  m: 3
```

Three merge modes:
- **`all` (default)**: advance only when ALL incoming labeled edges have produced their target label.
- **`any`**: advance on FIRST incoming labeled edge result.
- **`quorum N/M`**: advance when at least N of M incoming edges have produced their target.

When `merge:` is absent on a state with multiple incoming labeled edges, the default is `wait-all`. States with ≤1 incoming labeled edge are not merge nodes.

#### §3.2.2 Runtime tracking

On each labeled edge match (`resolve_labeled_target`), the runtime writes the matched label to context key `_merge_<target_state_id>` as a JSON array of unique label strings. When the engine enters a merge node, the `StateCompositeTask`:

1. Reads `_merge_<state_id>` from context.
2. Counts unique arrivals.
3. Evaluates the merge condition:
   - `All`: `arrived_count >= expected_incoming`
   - `Any`: `arrived_count >= 1`
   - `Quorum { n, .. }`: `arrived_count >= n`
4. If satisfied: clears the context key and processes enter actions normally.
5. If not satisfied: returns `NextAction::WaitForInput` — the engine will re-enter the state when the next labeled edge arrives.

**Incoming count discovery**: the loader pre-computes incoming labeled edge counts per state during graph construction. The `expected_incoming` field on `StateCompositeTask` is populated at build time.

#### §3.2.3 Validator integration

The semantic validator (`check_merge_node_integrity`) enforces:
- Each state with `merge:` must have ≥2 incoming labeled edges (from `LabeledNext` or `GoNogo` targets).
- `quorum N/M`: N ≥ 1, N ≤ M, and M must equal the actual incoming labeled edge count.
- Merge nodes must have at least one outgoing edge.

Violations produce `DiagnosticCategory::MergeIntegrity` errors and block preset loading.

#### §3.2.4 Backward compatibility

- States without `merge:` field default to `wait-all`.
- Existing binary `GoNogo` + N-way `Labeled` presets continue to work without modification — the `merge:` field is additive.
- `GoNogo` edges are also counted as incoming labeled edges for merge node purposes (labels `"go"` and `"nogo"`).

### 3.3 Arbitrary stage-level conditional `next` + expression routing (Draft V1.56 P2 — shipped in plan `2026-06-22-v1.56-df56-independent-slice`)

**Status**: Draft (V1.56 P2 shipped)
**Authoring plan**: `2026-06-22-v1.56-df56-independent-slice`
**Promotes to Normative**: P-last of V1.56

Extends the conditional routing engine from `llm_judge`-only to **arbitrary state kinds**, adds a simple expression grammar for rule-based routing, and defines explicit converge (merge-point) state nodes.

#### §3.3.1 Arbitrary stage-level conditional `next`

Any state kind (`produce`, `review`, `manual`, `rule`, `timer`, `graph_complete`, custom states) can now declare a conditional `next` using either the legacy `kind: conditional` form or the new expression-based `branches` form. The loader no longer restricts conditional routing to `llm_judge` states.

**Loader validation**: target state IDs in all branches and the default must reference valid states. The runtime evaluator reads `_state_result` or user-set context values for non-judge states.

#### §3.3.2 Expression / rule-based routing (Form B)

In addition to the legacy `kind: conditional` form (Form A, preserved), presets may use the new **Form B** multi-branch expression routing:

```yaml
next:
  branches:
    - when: "_context.score > 80"
      target: approved
    - when: "_context.status == 'review'"
      target: review_queue
  default: rejected
```

**Expression grammar**:
  - **Comparisons**: `==`, `!=`, `>`, `<`, `>=`, `<=`
  - **Boolean**: `&&`, `||`, `!`, parens
  - **Field access**: `_context.<dotted.path>` resolves against `graph_flow::Context` JSON values (e.g. `_context.score`, `_context.data.x`)
  - **Literals**: numbers (integer/float), single/double-quoted strings, `true`/`false`, `null`
  - **Truthy check**: bare field references (e.g. `_context.flag`) evaluate as truthy (non-null, non-false, non-zero, non-empty)

**Evaluation**: first-match semantics — branches are evaluated in declaration order; the first branch with a `when` expression that evaluates to `true` wins. If no branch matches, the `default` target is used. Missing context fields resolve to `null`.

**Null comparison semantics** (V1.56 P2 fix-wave, M-001): follows JSON equality semantics:
  - `null == null` → `true`
  - `null != "x"` → `true` (non-null value not equal to null)
  - `null > 0` → type error (no numeric comparison with null)
  - Bare `null` reference → falsey

**Depth limit** (V1.56 P2 fix-wave, W-003): expression nesting depth is bounded by `MAX_EXPR_DEPTH = 32` to prevent stack overflow from deeply-nested `when:` expressions in user-installable presets. Exceeding the depth returns `ExprError::DepthExceeded`. Depth=32 succeeds; depth=33 returns an error; depth=1000 does not panic.

**Expression caching** (V1.56 P2 fix-wave, M-004): compiled expression ASTs are cached per task at construction time and reused across transitions. Parsing happens once per preset load, not once per transition.

**Error propagation** (V1.56 P2 fix-wave, M-006): expression evaluation errors are propagated as `TaskExecutionFailed` (not silently swallowed). Runtime failures surface to the caller instead of skipping the branch.

**Safety**: expressions are locally evaluated in-process; no code injection, no scripting engine, no external service calls.

#### §3.3.3 Multi-branch graphs with merge points

States may declare a `converge` config to act as a dedicated **merge-point (converge) node** that explicitly accepts multiple incoming edges:

```yaml
states:
  - id: merged
    converge:
      strategy: wait_for_all   # default
    enter: []
    exit_when: { kind: manual }
    next: done
```

**Converge strategies**:
  - **`wait_for_all`** (default): wait for all incoming edges to arrive before advancing
  - **`first_completed`**: advance on first arrival; subsequent arrivals are ignored
  - **`any`**: idempotent — advance on first arrival; re-run after gate pass continues immediately

**Runtime enforcement** (V1.56 P2 fix-wave, H-001/W-002): the convergence gate runs in `StateCompositeTask::run()` after the merge node gate. Arrivals are tracked via the `_converge_arrivals_{state_id}` context key, populated by source states when they route to a converge target. The converge gate check:
  1. Reads the arrivals from the converge key
  2. Compares against the number of known predecessors
  3. For `wait_for_all`: all predecessors must have arrived
  4. For `first_completed` / `any`: at least one predecessor must have arrived
  5. If unsatisfied: returns `NextAction::WaitForInput`
  6. If satisfied: clears the key and advances to enter actions

**Predecessor tracking** (V1.56 P2 fix-wave): converge predecessors are discovered at graph build time by scanning all states' `next` targets. States that are converge targets have their predecessor sets populated in `StateCompositeTask::converge_predecessors`. Source states call `record_converge_arrival()` when routing to a converge target via `resolve_expression_target()` or `resolve_labeled_target()`.

**Loader validation** (V1.56 P2 fix-wave, M-002):
  - Converge states must not be terminal
  - Converge states with 0 predecessors produce a validation error (orphan)
  - Converge states with 1 predecessor produce a warning (consider linear transition)

**DAG enforcement**: cycles remain rejected at load time. Acyclic paths through converge nodes (e.g. `A → M → B`, `C → M → B` where M waits for both A and C) are allowed.

---

## 4. Design axes (unlocked — future grill required)

| Axis | Options | Recommendation (exploration) |
| --- | --- | --- |
| Graph model | DAG with conditional edges vs FSM vs stage hooks | Defer — needs product + security review |
| User visibility | Hidden in preset vs exposed `creator run branch` | Prefer hidden first; CLI surface later |
| Interaction with linear stages | Replace stage enum vs layer beneath stages | **Layer beneath** — do not break V1.34 linear creator-workflow spec |
| Agent tools | Agent chooses branch vs engine chooses | **Engine chooses** |
| Validation | Extend V1.32 validator (reachability + cycle detection on conditional edges) | Required before any ship |

---

## 5. Dependencies before implement

1. Close or cap V1.33 **critical** residuals (security/auth on memory, judge.llm correctness).
2. DF-47 production caller wiring (agent tool path stable).
3. CLI IA stable (V1.35 five-group IA) so new commands are not added atop a moving surface.
4. Promote this doc from Exploration → Draft with locked compass; update orchestration-engine §7.5 cross-link.

---

## 6. Suggested future iteration shape (non-binding)

| Phase | Deliverable |
| --- | --- |
| Spec | This document → Status **Normative**; orchestration-engine §7.5 points here |
| Engine | Loader + runtime evaluator for `next.kind: conditional` |
| Presets | 1–2 reference presets (reflection branch, research retry) |
| CLI | Optional; prefer preset-driven first |
| QA | Validator + hermetic e2e branch coverage |

**Target timing:** Post-V1.35. V1.35 compass explicitly **OUT** for implement — see [v1.35 compass Appendix B](../../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md#appendix-b-v135-fl-d-scope-lock).

---

## 7. Explicit non-goals

| Scope | Rule |
| --- | --- |
| V1.35 | No engine or loader code for conditional routing |
| DF-56 | No tracker closure until implement compass locks |
| DF-53 | `--auto-chain` / `creator run next` semantics remain separate track (V1.35 P4 partial) |
| Linear workflow replacement | First conditional-routing slice must not break shipped linear creator-workflow contract |

---

## 8. References

- PD-08: Preset orchestration + Agentic Design Patterns (deferred tracker product line)
- [v1.31-agentic-design-patterns-delivery-compass-v1.md](../../iterations/v1.31-agentic-design-patterns-delivery-compass-v1.md)
- [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) §1.2 OUT: conditional routing
- External: https://github.com/evoiz/Agentic-Design-Patterns

---

## 9. Change control

| Event | Action |
| --- | --- |
| V1.42 P2 implement compass locks | Status → Draft V1.42; open implement plan; ship minimal `llm_judge` GO/NOGO slice ✅ (2026-06-11) |
| First preset with conditional edges ships | Status → Normative; close DF-56 in deferred tracker |
| Conflict with active linear creator-workflow spec | Linear workflow spec wins until ADR + compass explicitly supersedes |

---

*Draft V1.42 SSOT for preset conditional routing (DF-56 minimal slice). Full routing authority remains a future locked compass.*
