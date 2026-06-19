# Preset Conditional Routing — Specification

**Status**: Shipped (V1.42 P2 — 2026-06-12; `llm_judge` GO/NOGO → two `next` edges; V1.52 T-B P0 — 2026-06-19; N-way labeled routing; full DF-56 roadmap in deferred tracker §3.6.3)  
**Document class**: Feature line (conditional routing — minimal slice → N-way labeled)  
**Created**: 2026-06-06  
**Last updated**: 2026-06-19 (V1.52 T-B P0: N-way labeled routing shipped, §3.1 replaced placeholder with shipped spec)
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
