---
module: apps/web (Outline canvas event inspector)
date: 2026-09-30
problem_type: ui_bug
category: ui-bugs
severity: medium
plan_id: 2026-09-30-v1.201-p2-correctness-sweep
tags:
  - draft-state
  - race-guard
  - generation-tagging
  - inspector
  - bound-world
  - stale-response
symptoms:
  - Editing a form value while its bind/save mutation is in flight silently cleared the newer value on success
  - Retyping the field back to the originally issued value before the response still deleted the draft
  - One control's completion cleared another control's independently authored draft (incl. pre-existing drafts)
  - A bound-World switch mid-flight let the stale completion clear the new scope's draft
root_cause: The success callback identified "its" draft by value equality with the issued value; value equality is not request identity — any later edit through the issued value, a second control holding the same value, or a scope transition defeats it, and one callback cleared both controls' slots.
resolution_type: code_fix
related_components:
  - apps/web Outline canvas
---

# Draft-state races on in-flight mutations: tag by (control, scope) generation

## Symptoms

Changing a form value while its bind/save mutation is in flight silently cleared the newer value on success — in three distinct shapes that each needed its own regression: (1) retype the field back to the originally issued value before the response lands (equality-with-issued-value guards pass, newer draft deleted); (2) two controls (picker + manual entry) sharing one draft store — a completion from one control cleared the other's independently authored draft, including a pre-existing draft authored before the click; (3) the owning scope (bound World) switched mid-flight and the stale completion cleared the new scope's draft.

## Root Cause

The success callback identified "its" draft by value equality (`liveValue === issuedValue`). Value equality is not request identity: any later edit that passes through the issued value, any second control holding the same value, and any scope transition defeats it. The component also cleared both controls' entries from one callback, so one request disposed of drafts it did not own.

## Resolution Type

code_fix

## Solution

- Maintain a monotonic **generation counter per (control, scope) draft slot**, bumped by a single mutation seam (`setWorldEventDraft(control, eventId, value)`) used by every onChange.
- At issue time, capture the **originating control's** generation (plus scope, e.g. `boundWorldId`) as the request tag.
- The success callback releases **only the issuing control's slot, only while its generation is unchanged**; scope transitions bump all generations monotonically (never reset — a reset re-collides at generation 1) so stale completions from a former scope cannot consume drafts of the new scope.
- On scope switch, drop scope-specific selections (picker options from the former World become non-actionable) while preserving scope-agnostic drafts.
- Regressions for each shape: retype-back-to-issued; other-control pre-existing draft survives an unrelated success; crossed-World completion; same-ID across controls; cross-event isolation.

## Why This Works

Generation counters turn "is this response still about the value the user sees?" from a value comparison into an identity comparison: any intervening edit — including through the issued value — bumps the counter and invalidates the in-flight completion. Keying per control and per scope keeps independent authoring surfaces from suppressing each other's cleanup.

## Prevention

Whenever a mutation callback writes back to form/draft state, ask: can the user edit this state (or switch its scope) before the response lands? If yes, capture a generation/scope tag at issue time and release only the matching slot. Equality-with-issued-value guards are never sufficient.

## Examples

- `apps/web/src/components/canvas/outline-canvas/inspectors/event-inspector.tsx` — `draftGenerationRef` + `issuedGenerations` + boundWorldId-transition invalidation (v1.201 P2, `_default/002/R4` + FX-B; QC Approved after 2 fix rounds + crossed-World finding).
