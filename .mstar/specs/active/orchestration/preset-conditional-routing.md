# Preset Conditional Routing — Specification

**Status**: Shipped — normative SSOT for preset conditional routing: `llm_judge` GO/NOGO and N-way labeled routing, multi-branch merge semantics, arbitrary stage-level conditional `next` + expression routing + converge nodes, registry/workspace context fields, and DR-06 bounded joins (§3.3.3)  
**Document class**: Feature line (conditional routing)  
**Created**: 2026-06-06  
**Last updated**: 2026-09-29  
**Tracker**: DF-56 (conditional routing / branching engine)  
**Scope**: Preset `next.kind: conditional` loader + runtime evaluator  
**Supersedes**: `fl-d-conditional-routing-exploration-v1.35-prepare.md` (pre-shipment exploration; the superseded rationale is frozen in the archive record)  
**Coordinates with**:

- [orchestration-engine.md](orchestration-engine.md) §7.5 — delegates conditional-routing semantics to this file as the shipped normative SSOT, including DR-06 bounded joins
- [creator-workflow.md](../creator/creator-workflow.md) — linear creator workflow stages; conditional routing layers beneath, does not replace the FL-E enum

This file is the shipped normative SSOT for conditional routing. DR-06 bounded-join validation is implemented in `crates/nexus-preset/src/loader.rs`, runtime deadlines/rerouting in `crates/nexus-orchestration/src/tasks/mod.rs`, and deterministic coverage in `crates/nexus-orchestration/tests/join_timeout_e2e.rs`.

---

## 1. Purpose

Authors need presets that branch on runtime signals (judge outcome, tool result, user input) without spawning separate schedules or manual dispatch.

---

## 3. Shipped normative semantics

Orchestration-engine §7.5 delegates the full conditional `next` schema and merge/converge semantics to this section. N-way routing, expression routing, and DR-06 bounded joins are shipped.

### 3.1 N-way labeled routing

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

### 3.2 Merge semantics

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

When `merge:` is absent on a state with multiple incoming labeled edges, the default is `wait-all`. States with 0 incoming labeled edges are not merge nodes; a state with exactly 1 incoming labeled edge is still join-gated by the implicit wait-all rule (it waits for that single arrival). Reconciliation with §3.3.3: bounded-join fields are accepted from that same ≥1-incoming threshold — at `expected_incoming == 1` the `timeout_ms` budget is meaningful, bounding the wait for the single edge — while explicit `merge:` states additionally require ≥2 incoming edges (§3.2.3) and explicit `converge:` states accept the fields at any predecessor count (0 predecessors skips the gate per §3.3.3).

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
   Bounded-join opt-in (DR-06): if the merge state sets
   `timeout_ms`, the wait is bounded — deadline expiry reroutes to
   `on_timeout` or fails with the typed `converge_timeout:` error
   (`gate=merge`); see §3.3.3 "Bounded joins" for the normative table.

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

### 3.3 Arbitrary stage-level conditional `next` + expression routing

Extends the conditional routing engine from `llm_judge`-only to **arbitrary state kinds**, adds a simple expression grammar for rule-based routing, and defines explicit converge (merge-point) state nodes.

#### §3.3.1 Arbitrary stage-level conditional `next`

Any state kind (`produce`, `review`, `manual`, `rule`, `timer`, `graph_complete`, custom states) can declare a conditional `next` using either the legacy `kind: conditional` form or the expression-based `branches` form. The loader does not restrict conditional routing to `llm_judge` states.

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

**Null comparison semantics** (fix-wave M-001): follows JSON equality semantics:
  - `null == null` → `true`
  - `null != "x"` → `true` (non-null value not equal to null)
  - `null > 0` → type error (no numeric comparison with null)
  - Bare `null` reference → falsey

**Absent vs null vs empty string** (R-V156P2-M001 clarification): the expression evaluator treats three field states distinctly:
  - **Absent** (key not in context): resolves to `null`. `_context.missing == null` → `true`. `_context.missing` (bare truthy) → `false`.
  - **Explicit `null`** (key present with JSON `null`): identical to absent — both produce `null` at the value layer, so `null == null` → `true`.
  - **Empty string `""`** (key present with `""`): distinct from null. `_context.text == null` → `false` (string is not null). `_context.text` (bare truthy) → `false` (empty string is falsey per truthy rules). However `_context.text != null` → `true`.

Preset authors should use `== null` / `!= null` to test for field presence, and avoid bare truthy checks when distinguishing absent from empty-string.

**Depth limit** (fix-wave W-003): expression nesting depth is bounded by `MAX_EXPR_DEPTH = 32` to prevent stack overflow from deeply-nested `when:` expressions in user-installable presets. Exceeding the depth returns `ExprError::DepthExceeded`. Depth=32 succeeds; depth=33 returns an error; depth=1000 does not panic.

**Expression caching** (fix-wave M-004): compiled expression ASTs are cached per task at construction time and reused across transitions. Parsing happens once per preset load, not once per transition.

**Error propagation** (fix-wave M-006): expression evaluation errors are propagated as `TaskExecutionFailed` (not silently swallowed). Runtime failures surface to the caller instead of skipping the branch.

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

**Runtime enforcement** (fix-wave H-001/W-002): the convergence gate runs in `StateCompositeTask::run()` after the merge node gate. Arrivals are tracked via the `_converge_arrivals_{state_id}` context key, populated by source states when they route to a converge target. The converge gate check:
  1. Reads the arrivals from the converge key
  2. Compares against the number of known predecessors
  3. For `wait_for_all`: all predecessors must have arrived
  4. For `first_completed` / `any`: at least one predecessor must have arrived
  5. If unsatisfied: returns `NextAction::WaitForInput`
  6. If satisfied: clears the key and advances to enter actions

**Predecessor tracking** (fix-wave): converge predecessors are discovered at graph build time by scanning all states' `next` targets. States that are converge targets have their predecessor sets populated in `StateCompositeTask::converge_predecessors`. Source states call `record_converge_arrival()` when routing to a converge target via `resolve_expression_target()` or `resolve_labeled_target()`.

**Loader validation** (fix-wave M-002; R-V156P2-M002 reconciliation):
  - Converge states must not be terminal
  - Converge states with 0 predecessors: **gate is skipped, state advances immediately** (no validation error). This handles the orphan-converge edge case gracefully — the state behaves as a normal non-converge state. The predecessor set is empty, so `converge_predecessors.is_empty()` short-circuits the gate check in `StateCompositeTask::run()`.
  - Converge states with 1 predecessor produce a warning (consider linear transition)

**DAG enforcement**: cycles remain rejected at load time. Acyclic paths through converge nodes (e.g. `A → M → B`, `C → M → B` where M waits for both A and C) are allowed.

**Bounded joins — `timeout_ms` / `on_timeout`** (DR-06 — Normative):
join states may declare a bounded-join deadline. ONE field pair on the
**state** serves BOTH join gates (merge §3.2 and converge §3.3.3):

| Field | Type | Default | Description |
|---|---|---|---|
| `timeout_ms` | `u64` (optional) | absent | Deadline in ms, measured from the join state's first waiting tick. Absent = unbounded wait (pre-DR-06 behaviour, byte-identical). |
| `on_timeout` | `string` (optional) | absent | State ID to reroute to when the deadline fires. Must resolve to a state in this preset (loader-enforced). |

Semantics:
- The first waiting tick records `_join_wait_start_{state_id}` in context;
  subsequent ticks compare elapsed wall time against `timeout_ms`.
  `timeout_ms` is ONE state-level budget shared by BOTH gates on the state
  (merge §3.2 and converge §3.3.3): a merge-success tick whose converge
  gate still has to wait does NOT restart the budget.
- The wait-start key is cleared when the deadline fires (below) AND when
  the join LEAVES — every present gate passed and processing proceeds — so
  a same-session re-entry (runtime `GoTo` loop; the DAG load only rejects
  static cycles) starts a fresh budget instead of firing the stale
  deadline immediately (F-001).
- `timeout_ms: 0` is legal and means the deadline is already elapsed: the
  FIRST waiting tick immediately reroutes (with `on_timeout`) or fails
  typed (without). `on_timeout` without `timeout_ms` is inert (no
  deadline ever fires).
- Deadline exceeded **with** a resolvable `on_timeout`: the arrivals key
  (`_merge_{id}` / `_converge_arrivals_{id}`) and the wait-start key are
  cleared, a note is written to `_join_timeout_note`, and the run reroutes
  to the target state (`NextAction::GoTo` — the target re-enters through
  the normal task path, so its enter actions run).
- Deadline exceeded **without** `on_timeout` (or unresolvable): the task
  fails with `GraphError::TaskExecutionFailed` whose message begins with
  the typed discriminator `converge_timeout:` and names
  `gate=<merge|converge>`, `state_id`, `arrived`, `expected`, `elapsed_ms`
  (single discriminator for both gates — there is no sibling
  `merge_timeout` code).
- Loader fails closed: `timeout_ms`/`on_timeout` on a state the runtime
  does not treat as a join is a load-time error. A join state carries
  `merge:`, `converge:`, OR has ≥1 incoming labeled/GoNogo edge (implicit
  wait-all merge per §3.2 — W-1 parity fix: the loader
  validation and the graph wiring share one edge-count scan). `on_timeout`
  naming an unknown state is a load-time error.

The `wait_for_all_timeout_seconds` field name (a `ConvergeConfig` field with a default 3600 s budget) is **retired**: the state-level additive `timeout_ms`/`on_timeout` pair above is the bounded-join contract, and `ConvergeConfig`/`MergeKind` shapes are unchanged.

### 3.4 Registry and workspace context fields

Extends the expression grammar's context namespace with two new sub-objects — `registry_refresh` and `workspace` — enabling conditional routing decisions driven by capability registry output and workspace session state.

#### §3.4.1 `registry_refresh` context fields

When a state's conditional `next` references `_context.registry_refresh.*`, the runtime invokes the `nexus.registry.refresh` capability and feeds its output into the expression context as a nested `registry_refresh` object with these fields:

| Context field | Type | Source | Description |
|---|---|---|---|
| `_context.registry_refresh.source` | `String` | `RegistryRefreshOutput.source` | `"synthetic"`, `"cdn"`, or `"synthetic_fallback"` |
| `_context.registry_refresh.snapshot_version` | `String` | `RegistryRefreshOutput.snapshotVersion` | e.g. `"2026-06-22.v1"` |
| `_context.registry_refresh.capability_count` | `Number` | `RegistryRefreshOutput.capabilityCount` | Number of registered capabilities in the snapshot |
| `_context.registry_refresh.fallback_reason` | `String` | `RegistryRefreshOutput.fallbackReason` | Empty on success; typed `CdnError` variant string when fallback occurred |
| `_context.registry_refresh.retry_count` | `Number` | `RegistryRefreshOutput.retryCount` | Retries actually attempted (0 for synthetic, up to `maxRetries` for CDN) |

**Invocation rule**: the runtime invokes the `registry.refresh` capability once, before evaluating the first branch expression for a state. The result is stored in the context for the duration of the state transition and re-read on subsequent re-entries. If the capability registry is unavailable, the runtime injects a minimal synthetic default (`source: "synthetic"`, `capability_count: 31`, `retry_count: 0`, `fallback_reason: ""`).

**Field naming**: capability output uses camelCase JSON fields (`capabilityCount`, `snapshotVersion`); the expression context maps these to `snake_case` for consistency with the expression grammar (`capability_count`, `snapshot_version`).

**Example**:
```yaml
next:
  branches:
    - when: "_context.registry_refresh.source == 'synthetic'"
      target: synthetic_handling_state
    - when: "_context.registry_refresh.capability_count > 50"
      target: high_capability_state
  default: standard_state
```

#### §3.4.2 `workspace` context fields

When a state's conditional `next` references `_context.workspace.*`, the runtime queries the workspace session state and injects it into the expression context as a nested `workspace` object with these fields:

| Context field | Type | Source | Description |
|---|---|---|---|
| `_context.workspace.session_id` | `String` | Active workspace session ID | `"ws_<uuid>"` or `""` when no session is active |
| `_context.workspace.conflict_detected` | `Bool` | OCC hash conflict status | `true` when the last commit attempt detected a content hash mismatch |
| `_context.workspace.changes_applied` | `Number` | Count of changes in the session | Number of `ChangeEntry` items applied in the current workspace session |
| `_context.workspace.workspace_root` | `String` | Canonical workspace root path | Absolute path to the workspace creative root |

**Invocation rule**: the runtime reads the workspace session state from the active workspace context. If no explicit workspace state has been set (e.g., in test harnesses), a minimal default is injected (`session_id: ""`, `conflict_detected: false`, `changes_applied: 0`, `workspace_root: ""`). In production, the orchestration engine populates these fields from the `WorkspaceSessionManager` before evaluating branches.

**OCC semantics**: `conflict_detected` is `true` when a `workspace.commit` operation encountered a `SessionError::HashConflict`. This allows presets to route to conflict-resolution states after a failed commit attempt.

**Example**:
```yaml
next:
  branches:
    - when: "_context.workspace.conflict_detected"
      target: resolve_conflict_state
    - when: "_context.workspace.changes_applied > 3"
      target: review_large_change
  default: continue_state
```

#### §3.4.3 Context dependency scanning

The runtime scans compiled expression ASTs for `registry_refresh` and `workspace` field accesses at construction time (in `build_expr_cache`). The dependency flags (`needs_registry_refresh`, `needs_workspace`) control whether the corresponding capability or session query is invoked before branch evaluation. Expressions that reference neither sub-object skip the invocation entirely — no unnecessary I/O.

**Precedence**: when both `registry_refresh` and `workspace` fields are referenced in the same expression, both are injected. Field name collisions within each sub-object are detected at expression parse time; the expression grammar treats them as distinct nested paths.

---

## 8. References

- PD-08: Preset orchestration + Agentic Design Patterns (deferred tracker product line)
- External: https://github.com/evoiz/Agentic-Design-Patterns

---

*Normative SSOT for preset conditional routing. N-way labeled routing, multi-branch merge semantics, the expression grammar with registry/workspace context fields, and DR-06 bounded joins are shipped (§3).*
