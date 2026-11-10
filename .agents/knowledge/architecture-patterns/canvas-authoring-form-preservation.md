---
module: apps/web + crates/nexus-core + crates/nexus-knowledge
date: 2026-10-01
problem_type: architecture_pattern
category: architecture-patterns
severity: high
plan_id: 2026-10-01-v1.203-p2-mental-field-authoring
tags:
  - canvas
  - form-state
  - lossless-seeding
  - dirty-tracking
  - raw-json-fallback
  - occ-conflict
  - validation-mapping
  - own-property
applies_when:
  - building an edit form over stored opaque-JSON carriers (canvas inspectors, module bags)
  - wiring daemon 422 field errors onto compacted array editors
  - gating writes on an authorization/kind predicate
---

# Canvas Authoring-Form Preservation Discipline

## Context

Canvas inspectors edit opaque-JSON module carriers (`modules.mental` / `modules.belief` / `modules.observation`) whose write contract is a **whole-first-level-value upsert**: `POST /v1/daemon/worlds/{world_id}/kb/patch-entity` → `world_kb::patch_entity` → `NexusAdapter::orchestrate_upsert`. A provided key replaces the whole first-level value; unspecified siblings are preserved; clearing writes the empty container (`[]` / `{}`), never a key deletion; `audience` governance rides the same patch revision and is omitted when unchanged; every write carries `expected_version` (409 → the existing conflict-modal flow). The daemon validates the authored dialects post-extraction, pre-write (`validate_authored_modules` on both create and update paths, a lens on the authored keys only — unknown keys round-trip verbatim) and rejects with 422 string entries in the frozen prefix grammar `modules.<dialect>[.<index>].<field>: <reason>` (`<index>` only for array dialects; `<field>` mirrors the validator member name exactly).

Every rule below was forced by a concrete loss/blocked-write found in review of that write path. The discipline generalizes to any form seeded from stored data the form cannot fully represent.

## Guidance

1. **Never normalize stored data into form state.** Seeding first asks *is this value representable by the structured editor?* A stored value that is not (non-array where the editor edits rows, an array containing a non-object row, a shape the fields cannot round-trip) seeds a **lossless raw-JSON fallback**: the form holds the complete stored value verbatim as text and renders a raw editor, not a coerced structured view. Structured seeding keeps only fully representable values.
2. **Dirty reflects author edits, not seed normalization.** Raw-mode dirty is a text comparison against `JSON.stringify(stored)` — an untouched nonrepresentable seed is never dirty, so it never enters the patch and never needs "repair". A form that must *change* data just to save it is corrupting on every save.
3. **Authorization gating covers dirty-detection AND patch emission, not just the rendered controls.** Hiding inputs in JSX is not a gate: the dirty computation takes the kind predicate (`isHolderKind`) so a non-holder can never report `modules` dirty, and submit gates the emission block (`isHolderKind && dirty.includes('modules')`) plus the 409 `dirtyDialects` capture under the same predicate. A title-only save on a non-holder entity emits `{title}` and nothing else.
4. **Build, validate, and emit only dirty dialects.** Submit computes the exact per-dialect dirty predicates *before* constructing any value, then constructs/validates/emits just those (`wantMental` / `wantBelief`). An untouched sibling must not veto the write: an untouched raw-seeded scalar `modules.belief` previously blocked a mental-only save because both dialects were built unconditionally and the raw builder rejects non-arrays. A *dirty* sibling with invalid JSON still blocks the whole write; with both dirty, either failing means no partial mutation. The 409 reapply builder stays restricted to the captured dirty dialect set for the same reason.
5. **Map 422 wire indices through a wire→form translation, never directly onto form rows.** Array editors compact on emission (blank/cleared rows contribute nothing). Each builder returns a `wireToForm` index map — for every emitted wire element, the index of its contributing form row — and the 422 mapper translates `modules.belief.<i>.<field>` through this submission's map; a wire index with no contributing form row stays section-level verbatim. Split the reason on the first `": "` only (reasons may themselves contain `": "`), and only for entries starting with `modules.`; everything else renders at section level. The map is request-local (captured per submission), never form state.
6. **Construct arbitrary-JSON-key objects with own-property semantics.** Build carrier values with `Object.fromEntries` (or a null-prototype object). Plain `value[key] = …` invokes the inherited `__proto__` setter for a legal stored `"__proto__"` key — the key silently vanishes from JSON serialization, so a save drops authored data while the form still displays it. Seed-side, `Object.entries`/`JSON.parse` already yield the own key; the construction side is where it is lost.

## Why This Matters

The stored module bag is the author's data and the daemon's source of truth; the form is a *lossy lens* over it. Each rule above closes a distinct silent-failure class: normalization-on-seed destroys data before the author touches anything (1, 2); render-only gating lets unauthorized dialects ride an unrelated save (3); unconditional sibling construction lets untouched data veto or join a write (4); naive index mapping blames the wrong row, dead-ending the author on an unfixable-looking error (5); prototype-setter construction drops legal keys at the exact moment of save (6). Together they are the difference between "editor over a carrier" and "editor that owns the carrier".

## When to Apply

- Any new canvas/inspector editing surface over a stored JSON carrier (a fourth authored dialect, a new module bag, a Work-scoped editor).
- Any form whose 422 mapping must survive array compaction or whose values contain keys the form did not author.
- Review heuristic: for each form field family ask "what happens to a stored value this form cannot represent, on a save that did not touch this family?"

## What Didn't Work

- **Structured-only seeding**: normalizing a stored `[42, {proposition: "keep me"}]` belief array into row state silently shortened it; the save then *wrote the shortened array back*.
- **Unconditional dialect construction**: building+validating every dialect on every save let an untouched non-array seed veto an unrelated mental-only edit.
- **`value[key] = parsed` assembly**: silently dropped a stored `"__proto__"` member from the serialized patch while the UI kept showing it.
- **Direct wire-index → form-row mapping**: with a blank first row compacted out, the daemon's `modules.belief.0.order` error landed on the wrong form row.

## Examples

- World KB entity inspector (`apps/web/src/components/canvas/world-kb/entity-inspector.tsx`): `formFromEntity` representability check + `beliefRawText` fallback; text-compared `beliefsDirty`; `Object.fromEntries` mental builder; `isHolderKind`-gated `dirtyFields` + emission; `wantMental`/`wantBelief` submit; `beliefWireToForm` + `mapValidationEntry` 422 translation.
- World Timeline event inspector (`timeline-inspector.tsx`): the same raw-fallback discipline for `modules.observation` (non-representable `access`/observer shapes seed the complete stored value), gated to `layoutHint === 'event'` nodes.
- Daemon seam (`crates/nexus-core/src/world_kb.rs` + `crates/nexus-knowledge/src/world_kb/knowledge_entry.rs`): the dialect lens validators and the shared `module_error` prefix formatter the SPA grammar maps onto.

## See also

- [canvas-surface-implementation-pattern.md](canvas-surface-implementation-pattern.md) — the surface-level coupled layers (patch DTOs, OCC revision, conflict modal) this discipline operates inside
- [field-level-error-envelope-for-generated-dtos.md](../api-design/field-level-error-envelope-for-generated-dtos.md) — the daemon-side field-vocabulary pattern the frozen prefix grammar extends
- [spoke-op-gate-at-adapter-boundary.md](spoke-op-gate-at-adapter-boundary.md) — why the dialect validators live in pure crates and are composed at the patch path, not in storage
