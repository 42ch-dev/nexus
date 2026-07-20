# Spec — `@web-*` / `@42ch/nexus-ui` clarity (V1.128 P3)

**Status:** product-reviewed, architect-locked, writing-hygiene done  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)  
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1128-4  
**Plan:** [`2026-07-20-v1.128-p3-web-alias-clarity`](../../../plans/2026-07-20-v1.128-p3-web-alias-clarity.md)  
**Wire contracts:** `wire_contracts_changed: false`  
**Related specs:** [P0 — Surfaces chrome IA](surfaces-chrome-ia.md) (no extracts) · [P1 — NLE Timeline canvas](nle-timeline-canvas.md) (`@web-canvas/*`) · [P2 — Create vs Controller shell](creator-create-controller-shell.md) (`@web-layout/creator-shell-content`)

## Problem

Gallery Surfaces copy and many `@web-*` import paths read like package names, so authors assume everything should come from `@42ch/nexus-ui`. In reality `@web-*` are **Design Studio Vite/tsconfig aliases** bridging to `apps/web` presentational extracts; `@42ch/nexus-ui` holds only **promoted** primitives after Studio visual acceptance.

## User value

**Before:** A maintainer copying imports from Studio fixtures assumes `@web-shell/*` or `@web-canvas/*` are published npm packages — or tries to “fix” confusion by mass-promoting extracts.

**After:** Every Surfaces page that cites an import shows whether it is an **App extract** or a **promoted primitive**; durable AGENTS/spec docs repeat the two-tier story. The maintainer knows promotion is deliberate and optional, not a V1.128 deliverable.

## Normative decisions

1. **Two-tier model stays (Must):** do **not** mass-migrate all `@web-*` into `@42ch/nexus-ui` this iteration. P3 success = **clarity**, not package consolidation.
2. **Visible labeling (Must):** Surfaces pages that import `@web-*` must show clear source badges/copy, e.g.:
   - “App presentational extract (`@web-…`)”
   - “Promoted primitive (`@42ch/nexus-ui`)”
   Consistent pattern across Shell, Canvas, Selection, and any P0–P2 surfaces touched.
3. **Docs (Must):** Update `apps/design-studio/AGENTS.md`, `packages/nexus-ui/AGENTS.md`, and `.mstar/specs/design-studio.md` (or pointer section) with an explicit two-tier table — naming cannot be misread as “all UI is nexus-ui”.
4. **Optional tiny promotion (Should — default N/A):** only if a component **already** meets studio-first promotion criteria before V1.128 starts — must be listed in this plan’s promotion list. **Default: zero promotions.** Mass promotion is a **Non-Goal**.

### Labeling beats promotion

| Approach | V1.128 priority |
|----------|-----------------|
| Gallery badges + AGENTS/spec docs | **Must** |
| Rename aliases globally | Out of scope |
| Promote ≥1 extract per surface | Optional at most one total |
| Promote all `@web-*` | **Forbidden** (Non-Goal) |

### Architect decisions (Seat 2)

| Decision | Lock |
|----------|------|
| Runs last | **After P0 → P1 → P2** so badges cover landed surfaces |
| Extract ownership | P3 labels only — P1 owns `@web-canvas/*` NLE chrome; P2 owns `@web-layout/creator-shell-content`; P0 owns no extracts |
| Alias roots unchanged | `@web-*` Vite/tsconfig aliases remain (`apps/design-studio/vite.config.ts:38–57`) — labeling, not rename |
| Wire contracts | `wire_contracts_changed: false` |

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1128-4a | On Surfaces pages citing `@web-*`, reads badge/copy identifying **App presentational extract** (not an npm package name) |
| AC-V1128-4b | On Surfaces pages citing `@42ch/nexus-ui`, reads badge/copy identifying **promoted primitive** |
| AC-V1128-4c | In `apps/design-studio/AGENTS.md`, `packages/nexus-ui/AGENTS.md`, and design-studio spec, finds explicit two-tier explanation — no “import everything from nexus-ui” claim |
| AC-V1128-4d | No requirement to change import paths en masse — existing `@web-*` aliases continue to work |

## Out of scope

Collapsing shell/canvas product extracts into the publishable package in one shot; global alias rename; promotion waves without plan list entry.
