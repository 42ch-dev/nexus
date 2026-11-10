---
module: process
date: 2026-08-10
problem_type: workflow_issue
category: workflow-patterns
severity: low
tags:
  - process
  - qc
  - sibling-plan
  - carry-forward
  - fix-wave
  - proactive
applies_when:
  - "An iteration ships multiple sibling plans that mirror each other (same pattern, different surface/scope) and the first plan's QC produces reusable lessons"
---

# Carry QC Lessons to Sibling Plans Proactively

## Context

An iteration often ships **sibling plans** that mirror each other — same pattern, different surface/scope — and run sequentially. The first plan's QC tri-review produces a fix-wave whose findings are frequently tied to the **shared pattern**, not to the specific surface (a read-only inspector that falls through to a write path; a crash on a newly introduced node kind; a projection memo missing a data prop; an unpinned fallback when a restriction is lifted). Those findings are reusable for the later siblings.

## Guidance

When an iteration has **sibling plans** (same pattern, different surface/scope) running sequentially, **bake the first plan's QC findings into the later plans' task briefs proactively** — do not wait for each plan to rediscover the same defect class in its own QC fix-wave.

1. **After the first sibling's QC fix-wave, extract the lessons as a checklist** keyed to the shared pattern, not the specific surface. (e.g., "read-only inspector for projected nodes", "alt-view crash-safety on new node kinds", "data prop in projection memo deps", "invalid-?layer fallback test when lifting a layer restriction".)

2. **Add a "Apply &lt;prev-plan&gt;'s QC lessons proactively" section to each later sibling's implementer brief**, naming each lesson and the specific application. Phrase as imperatives ("read-only inspector — no write path from projected nodes"), not as history.

3. **The implementer applies them from the start** — the brief is the contract. This is cheaper than a fix-wave: a fix-wave costs one implement dispatch + three targeted re-reviews; proactive application costs zero extra dispatches.

4. **The later plan's QC still runs the full tri** — proactive application reduces, not replaces, review. QC verifies the lessons were actually applied (and may still find surface-specific issues the lessons didn't cover).

## Why This Matters

In practice, the sibling that carried the lessons forward proactively needed only **one** converged fix item, versus the first plan's fix-wave of several items including a read-only violation. The proactive carry-forward turned a likely multi-finding fix-wave into a single-finding one and avoided repeating the same defect class.

The pattern generalizes beyond Canvas layers: any iteration with mirror plans (e.g., two transport adapters, two export formats, two CLI subcommands sharing a parser) benefits. The key precondition is **shared structure** — if the plans don't share a pattern, the lessons don't transfer.

## When to Apply

- An iteration has ≥2 sibling plans sharing an implementation pattern (mirror plans).
- The first plan's QC fix-wave produced findings tied to the **shared pattern** (not surface-specific quirks).
- The sibling plans run sequentially (later plans' briefs can still be amended).

## When NOT to Apply

- Findings are surface-specific (a quirk of one surface's existing code, not the shared pattern).
- Only one plan in the iteration (no sibling to carry to).
- Plans run fully in parallel (briefs already dispatched before the first QC returns) — in that case, note the lesson for a future iteration's similar work instead.

## References

- Three-layer projection pattern: `architecture-patterns/three-layer-timeline-projection.md`
