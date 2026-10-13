# Preset Conditional Routing — Pre-Shipment Exploration and Release Snapshot History Historical Notes

**Provenance**: frozen from [`specs/active/orchestration/preset-conditional-routing.md`](../../active/orchestration/preset-conditional-routing.md) on 2026-10-13 during the **v1.216** de-historicization pass. 
**Evidenced supersession date**: 2026-06-06 — the pre-shipment V1.35 exploration was superseded when the minimal V1.42 slice shipped (2026-06-06, named in the live Supersedes line); the release snapshots below carry later provenance (V1.42–V1.179) for completeness. Nothing here is current authority.
**Freeze date**: 2026-10-13

---

## Removed-line ledger (verbatim, in original order)

Every line removed from the live document by the v1.216 pass is reproduced below, verbatim and in original order (118 lines). Each block is anchored to the line number the text occupied in the pre-freeze document. Still-true statements were reworded in place in the live spec; their superseded wording appears here.

### at pre-freeze line ~3

```markdown
**Status**: Shipped (V1.42 P2 — 2026-06-12; `llm_judge` GO/NOGO → two `next` edges; V1.52 T-B P0 — 2026-06-19; N-way labeled routing; V1.52 T-B P1 — 2026-06-19; multi-branch merge semantics; V1.56 P2 — 2026-06-22; arbitrary stage conditional + expression routing + converge nodes; V1.56 P3 — 2026-06-22; registry.refresh conditional edges + workspace branch inputs; **V1.179 P2 — 2026-08-27; DR-06 bounded joins** — `timeout_ms` / `on_timeout` on `merge:`/`converge:` states (additive fields; normative §3.3.3))  
**Document class**: Feature line (conditional routing — minimal slice → N-way labeled → merge semantics → full expression routing → data-source integration)  
```

### at pre-freeze line ~6

```markdown
**Last updated**: 2026-09-29 (authority clarified; V1.179 P2 DR-06 bounded joins remain shipped, §3.3.3)
```

### at pre-freeze line ~8

```markdown
**Scope**: Preset `next.kind: conditional` loader + runtime evaluator (shipped)  
**Supersedes**: `fl-d-conditional-routing-exploration-v1.35-prepare.md` (pre-shipment exploration superseded 2026-06-06; rationale preserved in §4–§7)  
```

### at pre-freeze line ~12

```markdown
- [orchestration-engine.md](../../active/orchestration/orchestration-engine.md) §7.5 — delegates conditional-routing semantics to this file as the shipped normative SSOT, including DR-06 bounded joins (V1.179 P2); it is not a linear-only contract awaiting a future shipment
- [creator-workflow.md](../../active/creator/creator-workflow.md) — linear creator workflow stages (shipped V1.34); conditional routing layers beneath, does not replace FL-E enum in the first ship slice
```

### at pre-freeze line ~15

```markdown
This file is the shipped normative SSOT for conditional routing. DR-06 bounded-join validation is implemented in `crates/nexus-preset/src/loader.rs`, runtime deadlines/rerouting in `crates/nexus-orchestration/src/tasks/mod.rs`, and deterministic coverage in `crates/nexus-orchestration/tests/join_timeout_e2e.rs`. Pre-shipment rationale and release snapshots below are explicitly historical, not pending implementation gates.
```

### at pre-freeze line ~21

```markdown
Authors need presets that branch on runtime signals (judge outcome, tool result, user input) without spawning separate schedules or manual dispatch (historically `creator run <preset_id>`; that CLI runner was retired in v1.193 P2).

**Historical V1.42 P2 minimal-slice snapshot** (2026-06-11; later shipped semantics are in §3):

- `llm_judge` states with `next: { go: <state>, nogo: <state> }` now accepted by loader.
- Graph wires a conditional edge using `_judge_result` from context.
- GO → `go` target; NOGO or worker-unavailable → `nogo` target.
- Only valid on `exit_when: { kind: llm_judge }` states. Full expression-based conditional routing remains post-V1.42 (see §3.6.3).
- 

Historical pre-V1.42 state:

- Preset loader rejected `next.kind: conditional` with `ConditionalNotYetSupported`.
- Shipped creator workflow uses linear stage enum + explicit `creator run <preset_id>` dispatch (DF-53 auto-chain still open).
- V1.32 validator catches invalid preset graphs at load time for **linear** graphs only.

```

### at pre-freeze line ~21

```markdown

## 2. Historical state (V1.42 P2 shipped snapshot)

This table records the initial slice, not today's loader limits or CLI surface. Expression routing shipped in V1.56 (§3.3); `creator run` was retired in v1.193 P2.

| Area | State |
| --- | --- |
| Preset loader | Accepts `next: { go: <state>, nogo: <state> }` on `llm_judge` states; still rejects expression-based `ConditionalNotYetSupported` |
| Creator workflow stages | Linear enum; explicit `creator run <preset_id>` dispatch |
| Quality gate | V1.32 validator: reachability, terminal consistency, asset sandbox (linear graphs) |
| Conditional edge runtime | `graph_flow::add_conditional_edge` reads `_judge_result` from context |
| Open deferrals | DF-29 (`registry.refresh`), DF-31 (`workspace.*`), **DF-56** (full expression routing — post-V1.42) |
```

### at pre-freeze line ~27

```markdown
Orchestration-engine §7.5 delegates the full conditional `next` schema and merge/converge semantics to this section. N-way routing, expression routing, and DR-06 bounded joins are shipped; the release labels below identify their provenance, not future targets.
```

### at pre-freeze line ~29

```markdown
### 3.1 N-way labeled routing (V1.52 T-B P0 — Normative)
```

### at pre-freeze line ~67

```markdown
### 3.2 Merge semantics (V1.52 T-B P1 — Normative)
```

### at pre-freeze line ~96

```markdown
When `merge:` is absent on a state with multiple incoming labeled edges, the default is `wait-all`. States with 0 incoming labeled edges are not merge nodes; a state with exactly 1 incoming labeled edge is still join-gated by the implicit wait-all rule (it waits for that single arrival). Reconciliation with §3.3.3 (v1.179 QC): bounded-join fields are accepted from that same ≥1-incoming threshold — at `expected_incoming == 1` the `timeout_ms` budget is meaningful, bounding the wait for the single edge — while explicit `merge:` states additionally require ≥2 incoming edges (§3.2.3) and explicit `converge:` states accept the fields at any predecessor count (0 predecessors skips the gate per §3.3.3).
```

### at pre-freeze line ~110

```markdown
   Bounded-join opt-in (DR-06, v1.179): if the merge state sets
```

### at pre-freeze line ~132

```markdown
### 3.3 Arbitrary stage-level conditional `next` + expression routing (V1.56 P2 — Normative)
```

### at pre-freeze line ~165

```markdown
**Null comparison semantics** (V1.56 P2 fix-wave, M-001): follows JSON equality semantics:
```

### at pre-freeze line ~171

```markdown
**Absent vs null vs empty string** (V1.58 P2 — R-V156P2-M001 clarification): the expression evaluator treats three field states distinctly:
```

### at pre-freeze line ~178

```markdown
**Depth limit** (V1.56 P2 fix-wave, W-003): expression nesting depth is bounded by `MAX_EXPR_DEPTH = 32` to prevent stack overflow from deeply-nested `when:` expressions in user-installable presets. Exceeding the depth returns `ExprError::DepthExceeded`. Depth=32 succeeds; depth=33 returns an error; depth=1000 does not panic.
```

### at pre-freeze line ~180

```markdown
**Expression caching** (V1.56 P2 fix-wave, M-004): compiled expression ASTs are cached per task at construction time and reused across transitions. Parsing happens once per preset load, not once per transition.
```

### at pre-freeze line ~182

```markdown
**Error propagation** (V1.56 P2 fix-wave, M-006): expression evaluation errors are propagated as `TaskExecutionFailed` (not silently swallowed). Runtime failures surface to the caller instead of skipping the branch.
```

### at pre-freeze line ~205

```markdown
**Runtime enforcement** (V1.56 P2 fix-wave, H-001/W-002): the convergence gate runs in `StateCompositeTask::run()` after the merge node gate. Arrivals are tracked via the `_converge_arrivals_{state_id}` context key, populated by source states when they route to a converge target. The converge gate check:
```

### at pre-freeze line ~213

```markdown
**Predecessor tracking** (V1.56 P2 fix-wave): converge predecessors are discovered at graph build time by scanning all states' `next` targets. States that are converge targets have their predecessor sets populated in `StateCompositeTask::converge_predecessors`. Source states call `record_converge_arrival()` when routing to a converge target via `resolve_expression_target()` or `resolve_labeled_target()`.
```

### at pre-freeze line ~215

```markdown
**Loader validation** (V1.56 P2 fix-wave, M-002; V1.58 P2 — R-V156P2-M002 reconciliation):
```

### at pre-freeze line ~217

```markdown
  - Converge states with 0 predecessors: **gate is skipped, state advances immediately** (no validation error). This handles the orphan-converge edge case gracefully — the state behaves as a normal non-converge state. The predecessor set is empty, so `converge_predecessors.is_empty()` short-circuits the gate check in `StateCompositeTask::run()`. (Previous spec text said "validation error (orphan)"; reconciled to match tested implementation in V1.58 P2.)
```

### at pre-freeze line ~222

```markdown
**Bounded joins — `timeout_ms` / `on_timeout`** (DR-06, v1.179 — Normative):
```

### at pre-freeze line ~241

```markdown
  deadline immediately (F-001, v1.179 QC fix).
```

### at pre-freeze line ~260

```markdown
  wait-all merge per §3.2 — W-1 parity fix, v1.179 QC: the loader
```

### at pre-freeze line ~264

```markdown
> Historical note: this paragraph previously deferred a
> `wait_for_all_timeout_seconds` field (default 3600 s) on `ConvergeConfig`
> (V1.58 P2 — R-V156P2-L003). That name is RETIRED — v1.179 DR-06 (plan
> `2026-08-27-v1.179-p2-reliability-convergence`) shipped the state-level
> additive `timeout_ms`/`on_timeout` pair above instead, keeping
> `ConvergeConfig`/`MergeKind` shapes unchanged.
```

### at pre-freeze line ~266

```markdown
### 3.4 Registry and workspace context fields (V1.56 P3 — Normative)
```

### at pre-freeze line ~272

```markdown
When a state's conditional `next` references `_context.registry_refresh.*`, the runtime invokes the `nexus.registry.refresh` capability (shipped in V1.56 P1) and feeds its output into the expression context as a nested `registry_refresh` object with these fields:
```

### at pre-freeze line ~330

```markdown
## 4. Historical design axes (pre-shipment exploration)

§4–§7 preserve the pre-shipment V1.35 exploration, dependencies, proposed sequencing, and non-goals. They do not override shipped §3 or define current CLI entries; `creator run` was retired in v1.193 P2.

| Axis | Options | Recommendation (exploration) |
| --- | --- | --- |
| Graph model | DAG with conditional edges vs FSM vs stage hooks | Defer — needs product + security review |
| User visibility | Hidden in preset vs exposed `creator run branch` | Prefer hidden first; CLI surface later |
| Interaction with linear stages | Replace stage enum vs layer beneath stages | **Layer beneath** — do not break V1.34 linear creator-workflow spec |
| Agent tools | Agent chooses branch vs engine chooses | **Engine chooses** |
| Validation | Extend V1.32 validator (reachability + cycle detection on conditional edges) | Required before any ship |

```

### at pre-freeze line ~330

```markdown

## 5. Historical dependencies before implementation

1. Close or cap V1.33 **critical** residuals (security/auth on memory, judge.llm correctness).
2. DF-47 production caller wiring (agent tool path stable).
3. CLI IA stable (V1.35 five-group IA) so new commands are not added atop a moving surface.
4. Promote this doc from Exploration → Draft with locked compass; update orchestration-engine §7.5 cross-link.

```

### at pre-freeze line ~330

```markdown

## 6. Historical proposed iteration shape (non-binding)

| Phase | Deliverable |
| --- | --- |
| Spec | This document → Status **Normative**; orchestration-engine §7.5 points here |
| Engine | Loader + runtime evaluator for `next.kind: conditional` |
| Presets | 1–2 reference presets (reflection branch, research retry) |
| CLI | Optional; prefer preset-driven first |
| QA | Validator + hermetic e2e branch coverage |

**Target timing:** Post-V1.35. V1.35 compass explicitly **OUT** for implement — see v1.35 compass Appendix B.

```

### at pre-freeze line ~330

```markdown

## 7. Historical explicit non-goals (V1.35 exploration)

| Scope | Rule |
| --- | --- |
| V1.35 | No engine or loader code for conditional routing |
| DF-56 | No tracker closure until implement compass locks |
| DF-53 | `--auto-chain` / `creator run next` semantics remain separate track (V1.35 P4 partial) |
| Linear workflow replacement | First conditional-routing slice must not break shipped linear creator-workflow contract |

```

### at pre-freeze line ~330

```markdown

- 
-  §1.2 OUT: conditional routing
## 9. Historical change control (promotion record)

| Event | Action |
| --- | --- |
| V1.42 P2 implement compass locks | Status → Draft V1.42; open implement plan; ship minimal `llm_judge` GO/NOGO slice ✅ (2026-06-11) |
| First preset with conditional edges ships | Status → Normative; close DF-56 in deferred tracker |
| Conflict with active linear creator-workflow spec | Linear workflow spec wins until ADR + compass explicitly supersedes |

```

### at pre-freeze line ~338

```markdown

*Normative SSOT for preset conditional routing. V1.42 Draft established minimal `llm_judge` GO/NOGO; V1.52 promoted N-way labeled routing + multi-branch merge semantics; V1.56 promoted expression grammar + context namespace. All overlays promoted to Normative (V1.158 P3). V1.179 P2 added the DR-06 bounded-join amendment (§3.3.3).*
```
