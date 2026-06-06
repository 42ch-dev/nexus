# Preset Conditional Routing (FL-D) — Specification

**Status**: Exploration (pre-normative — **not shipped**)  
**Document class**: Exploration  
**Created**: 2026-06-06  
**Product line**: FL-D (Preset orchestration / Agentic Design Patterns)  
**Tracker**: DF-56 (Conditional routing / branching engine)  
**Scope**: Preset `next.kind: conditional` loader + runtime evaluator (future iteration)  
**Coordinates with**:

- [orchestration-engine.md](orchestration-engine.md) §7.5 — current linear-only contract; this doc is the future normative target when FL-D ships
- [creator-workflow-fl-e.md](creator-workflow-fl-e.md) — linear FL-E stages (shipped V1.34); conditional routing layers beneath, does not replace FL-E enum in first FL-D slice
- [deferred-features-cross-version-tracker.md](../deferred-features-cross-version-tracker.md) — DF-56, DF-29, DF-31

**Historical note**: V1.35 Prepare captured exploration in `archived/knowledge/fl-d-conditional-routing-exploration-v1.35-prepare.md`. This file is the long-term SSOT.

---

## 1. Purpose

Authors need presets that branch on runtime signals (judge outcome, tool result, user input) without spawning separate schedules or manual `creator run stage advance` calls.

Today:

- Preset loader rejects `next.kind: conditional` with `ConditionalNotYetSupported`.
- FL-E ships linear stage enum + explicit `creator run stage advance` (DF-53 auto-chain still open).
- V1.32 validator catches invalid preset graphs at load time for **linear** graphs only.

This spec holds **design axes and dependencies** until a future locked compass authorizes implement. It does **not** authorize code changes while Status remains Exploration.

---

## 2. Current state (post-V1.34)

| Area | State |
| --- | --- |
| Preset loader | Rejects `next.kind: conditional` → `ConditionalNotYetSupported` |
| FL-E stages | Linear enum; explicit `creator run stage advance` |
| Quality gate | V1.32 validator: reachability, terminal consistency, asset sandbox (linear graphs) |
| Open deferrals | DF-29 (`registry.refresh`), DF-31 (`workspace.*`), **DF-56** (routing engine) |

---

## 3. Target semantics (future normative)

When Status advances to **Draft** or **Normative**, orchestration-engine §7.5 defers to this document for the full conditional `next` schema.

Illustrative YAML (from orchestration-engine §7.5 — not loadable today):

```yaml
next:
  kind: conditional
  rules:
    - when: "{{state.brainstorming.output | length > 2000}}"
      to: outlining
    - when: "{{state.brainstorming.output | contains 'unclear'}}"
      to: gathering               # allow re-entry
  default: outlining
```

**Engine rule (proposed):** runtime evaluator chooses branch; agents supply data via tools only — agents do not directly select graph edges.

---

## 4. Design axes (unlocked — future grill required)

| Axis | Options | Recommendation (exploration) |
| --- | --- | --- |
| Graph model | DAG with conditional edges vs FSM vs stage hooks | Defer — needs product + security review |
| User visibility | Hidden in preset vs exposed `creator run branch` | Prefer hidden first; CLI surface later |
| Interaction with FL-E | Replace stage enum vs layer beneath stages | **Layer beneath** — do not break V1.34 linear FL-E spec |
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

**Target timing:** Post-V1.35 (tentative V1.36+ or dedicated FL-D slice). V1.35 compass explicitly **OUT** for implement — see [v1.35 compass Appendix B](../../iterations/v1.35-cli-ia-and-product-polish-delivery-compass-v1.md#appendix-b-v135-fl-d-scope-lock).

---

## 7. Explicit non-goals

| Scope | Rule |
| --- | --- |
| V1.35 | No engine or loader code for conditional routing |
| DF-56 | No tracker closure until implement compass locks |
| DF-53 | `--auto-chain` / `creator run next` semantics remain separate track (V1.35 P4 partial) |
| FL-E replacement | First FL-D slice must not break shipped linear FL-E contract |

---

## 8. References

- PD-08: Preset orchestration + Agentic Design Patterns
- [v1.31-agentic-design-patterns-delivery-compass-v1.md](../../iterations/v1.31-agentic-design-patterns-delivery-compass-v1.md)
- [v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md](../../iterations/v1.34-creator-workflow-and-agent-tools-delivery-compass-v1.md) §1.2 OUT: conditional routing
- External: https://github.com/evoiz/Agentic-Design-Patterns

---

## 9. Change control

| Event | Action |
| --- | --- |
| FL-D compass locks implement | Status → Draft; open implement plan; extend orchestration-engine §7.5 |
| First preset with conditional edges ships | Status → Normative; close DF-56 in deferred tracker |
| Conflict with active FL-E linear spec | FL-E wins until ADR + compass explicitly supersedes |

---

*Exploration SSOT for FL-D / DF-56. Delivery authority remains a future locked compass.*
