---
module: apps/web + crates/nexus-daemon-runtime + schemas
date: 2026-06-30
problem_type: architecture-pattern
category: architecture-patterns
severity: medium
plan_id: V1.77-P-last (compound of V1.67–V1.76 canvas program)
tags: [canvas, write-boundary, occ, conflict-modal, react-flow, design-tokens, local-api]
applies_when: implementing a new graph-editing surface on the Nexus canvas (or any structured-authoring surface with concurrent daemon + human edits)
---

# Canvas Surface Implementation Pattern

**Track**: Knowledge (durable guidance, distilled from the V1.67–V1.76 canvas program — 4 β surfaces: Strategy α/β, Outline+Timeline β, World KB β, World KB Relationships β/γ).

## Context

The Nexus canvas (`apps/web`, React Flow `@xyflow/react`) lets an author steer an AI-autonomous writing process by editing a graph of structured nodes/edges. The daemon owns canonical state (preset YAML, outline markdown, `kb_key_blocks`, `kb_relationships`); the browser/Tauri webview **never writes raw files**. Four surfaces shipped across V1.67–V1.76, each refining the same pattern. V1.75 retired the earlier V1.65 whole-document editor in favor of node-granular canvas edits. This doc distills the reusable implementation contract so the next surface (or any similar structured-authoring surface) lands correctly the first time.

## Guidance (the pattern)

A new canvas surface implements **six coupled layers**. Build them in this order; skipping a layer produces the failures noted.

1. **Schema-backed patch DTOs** (`schemas/local-api/<surface>/`). Each authoring operation is a structured patch request/response + a structured conflict error + a structured validation error. Never a whole-document PUT. The V1.65 whole-document PUT model was retired (V1.75 canvas-pivot) because it could not address or validate individual nodes.
2. **Per-row OCC revision** (`<table>.revision` column + `expected_version` in the patch request). The daemon rejects stale writes with **409 `<Surface>ConflictError`** carrying the current revision + structured locator. Do NOT invent a new OCC scheme per surface — reuse the `kb_key_blocks.revision` / outline `outline_revision:` frontmatter / Strategy `revision:` pattern.
3. **Conflict modal reuse** (UI). Stale writes surface a modal with **Use current / Reapply my edit / Review side-by-side** (side-by-side disabled for same-field conflicts). Flavor the copy with the domain entity (`{node_label}` placeholder → chapter / event / entity / relationship). One modal pattern, four flavors — do not build a fifth.
4. **Non-spatial alternate view** (accessibility). Every spatial graph surface also ships a sortable table/list variant (virtualized for scale) so keyboard + screen-reader users have a non-spatial path. This is a product requirement, not a checkbox.
5. **DESIGN.md token contract**. Each surface adds canvas tokens to `apps/web/DESIGN.md` + `DESIGN.dark.md` (YAML frontmatter SSOT; same token names, light/dark values split). **Token names are preserved verbatim across versions** (V1.69 invariant) — never rename a shipped token; only add new ones.
6. **Draft → Normative promotion discipline**. A new surface starts as a Draft overlay spec; promote to Shipped β at P-last only after QC tri-review + QA Pass. The `canvas-strategy-surface.md` Master holds the cross-surface contract; per-stage details live in Draft overlays until folded in.

## Why This Matters

- **No raw file writes from the webview** is the load-bearing security/correctness boundary. Violating it (e.g., a whole-document PUT) reintroduces the V1.65 race between the human editor and the orchestration co-writer, which the node-granular patch model was built to eliminate.
- **OCC revision** prevents silent overwrites when the daemon (extraction, orchestration) and the author edit concurrently. Last-writer-wins is acceptable **only** for surfaces with a single-author threat model (e.g., findings triage — see `findings-lifecycle.md`); graph surfaces with a daemon co-writer MUST use OCC.
- **Conflict modal reuse** keeps the UX coherent across surfaces and bounds the implementation cost. A fifth bespoke modal would fragment recovery semantics.

## When to Apply

- Adding a new canvas surface (a fifth surface after Strategy/Outline+Timeline/World KB/Relationships).
- Adding a new structured-authoring Local API resource that a daemon co-writer and a human both edit (apply the OCC + conflict-modal layers even off-canvas).
- **Do NOT apply** OCC/conflict-modal to a surface with a genuine single-author threat model (e.g., findings triage, where the producer writes and a single author triages) — last-writer-wins is correct there, and forcing OCC adds cost without value.

## What Didn't Work (V1.65 → V1.75 pivot)

The V1.65 whole-document TipTap outline editor (`chapter-page.tsx` PUT save) could not validate or address individual outline nodes, and it raced with the orchestration co-writer. V1.75 retired it (canvas-pivot) in favor of the V1.72 node-granular canvas. The lesson: a document-editor model is the wrong abstraction for a graph-of-nodes that a daemon also mutates; use structured patch routes from day one.

## Examples (cross-reference)

- **Strategy β** (V1.71): `strategy.patch_state` / `patch_transition` / `patch_prompt_template` + `graphRevision` + `StrategyConflictError`.
- **Outline+Timeline β** (V1.72): `outline.patch_structure` / `patch_chapter` + `timeline.patch_event` + `outline_revision:` markdown frontmatter + `OutlineConflictError`.
- **World KB β** (V1.73): `world_kb.patch_entity` + `promote_candidate` + reuse of `kb_key_blocks.revision` / `kb_extract_jobs.version` + `WorldKbConflictError`.
- **World KB Relationships β/γ** (V1.74/V1.76): `world_kb.patch_relationship` + `kb_relationships.revision` + `needs_review` extraction gate + confidence-weighting.
- **Findings-remediation (V1.77)**: the **counter-example** — last-writer-wins, no OCC, no conflict modal (single-author triage threat model; see `findings-lifecycle.md`).
