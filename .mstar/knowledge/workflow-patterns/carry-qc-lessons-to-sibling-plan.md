---
module: process
date: 2026-08-10
problem_type: workflow-pattern
category: workflow-patterns
severity: low
plan_id: 2026-08-10-v1.156-p2-work-timeline-brief-layer
tags: [process, qc, sibling-plan, carry-forward, fix-wave, proactive]
applies_when: An iteration ships multiple sibling plans that mirror each other (same pattern, different surface/scope) and the first plan's QC produces reusable lessons
---

# Carry QC Lessons to Sibling Plans Proactively

## Context

V1.156 shipped two sibling Canvas-layer plans that mirror each other: **P1** (World Timeline gains a Moment layer) and **P2** (Work Timeline gains a Brief layer). Both complete the 3×2 Brief/Narrative/Moment matrix; both are frontend-only projection extensions following the same adapter + orchestrator + inspector pattern (see `architecture-patterns/three-layer-timeline-projection.md`).

P1 ran first and its plan QC tri produced a fix-wave with four reusable findings:
- **W-1**: projected-layer nodes were selectable but the inspector dispatch fell through to the generic KB inspector → fired a guaranteed-failing `kb.patch_entity` write (PD-3 read-only violation).
- **W-2**: the alt-view/summarize path crashed on the new node kind (`block_type.localeCompare` on a node type without `block_type`).
- **F-3**: the fixture/data prop was missing from the projection `useMemo` deps → stale layer until a layer swap.
- **F-4**: lifting a `?layer=` URL restriction repurposed the old invalid-layer test slot → the null-branch went unpinned.

## Guidance

When an iteration has **sibling plans** (same pattern, different surface/scope) running sequentially, **bake the first plan's QC findings into the later plans' task briefs proactively** — do not wait for each plan to rediscover the same defect class in its own QC fix-wave.

1. **After the first sibling's QC fix-wave, extract the lessons as a checklist** keyed to the shared pattern, not the specific surface. (e.g., "read-only inspector for projected nodes", "alt-view crash-safety on new node kinds", "data prop in projection memo deps", "invalid-?layer fallback test when lifting a layer restriction".)

2. **Add a "Apply &lt;prev-plan&gt;'s QC lessons proactively" section to each later sibling's implementer brief**, naming each lesson and the specific application. Phrase as imperatives ("read-only inspector — no write path from projected nodes"), not as history.

3. **The implementer applies them from the start** — the brief is the contract. This is cheaper than a fix-wave: a fix-wave costs one implement dispatch + three targeted re-reviews; proactive application costs zero extra dispatches.

4. **The later plan's QC still runs the full tri** — proactive application reduces, not replaces, review. QC verifies the lessons were actually applied (and may still find surface-specific issues the lessons didn't cover).

## Why This Matters

In V1.156, P2 (the sibling) baked in all four P1 lessons proactively. P2's QC tri then found only **one** converged issue (a surface-specific graph-query status gate the lessons didn't cover) — a single fix-wave of one item, vs P1's fix-wave of four items including a PD-3 violation. The proactive carry-forward turned a likely 4-finding fix-wave into a 1-finding fix-wave and avoided a second PD-3 violation.

The pattern generalizes beyond Canvas layers: any iteration with mirror plans (e.g., two transport adapters, two export formats, two CLI subcommands sharing a parser) benefits. The key precondition is **shared structure** — if the plans don't share a pattern, the lessons don't transfer.

## When to Apply

- An iteration has ≥2 sibling plans sharing an implementation pattern (mirror plans).
- The first plan's QC fix-wave produced findings tied to the **shared pattern** (not surface-specific quirks).
- The sibling plans run sequentially (later plans' briefs can still be amended).

## When NOT to Apply

- Findings are surface-specific (a quirk of one surface's existing code, not the shared pattern).
- Only one plan in the iteration (no sibling to carry to).
- Plans run fully in parallel (briefs already dispatched before the first QC returns) — in that case, note the lesson for a future iteration's similar work instead.

## Examples

### V1.156 P2 brief — proactive P1 lessons

P2's T2 orchestrator brief included a section "Apply P1's QC lessons proactively (baked into the brief)" listing all four (read-only inspector, memo deps, alt-view crash-safety, invalid-layer test). The implementer applied them from the start. P2's QC tri (all three seats) verified the lessons were applied and found only the graph-query-status gap (surface-specific, not a pattern carry-forward).

P1 (no prior sibling) went through the full fix-wave discovery; P2 (with the carry-forward) needed one item.

## References

- V1.156 compass `delivery-compass.md` (P1+P2 sibling structure; local)
- V1.156 P1 QC consolidated `qc-consolidated.md` (the four lessons; local)
- Three-layer projection pattern: `architecture-patterns/three-layer-timeline-projection.md`
- Residual findings sweep playbook (adjacent iteration-level QC pattern): `workflow-patterns/residual-findings-sweep-playbook.md`
