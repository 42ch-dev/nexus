# Badge Soft / Solid Contract (V1.102 P0)

**Status:** architect-locked (iteration-start §5.2)  
**Plan:** `2026-07-09-v1.102-badge-soft-solid`  
**Tier:** Must (P0)  
**Wire:** `wire_contracts_changed: false`

## Problem

Soft status pills have near-invisible borders on light backgrounds. There is no solid/emphasis tone; Studio’s second row under Badge is `VariantLabel` text, not a second Badge style.

## Author-facing outcome

Contributors can compare Soft vs Solid in Studio. Product callers keep default soft; solid is opt-in via `tone`. No forced cutover of existing `StatusBadge` usage.

## API (locked)

```ts
tone?: 'soft' | 'solid'  // default 'soft'
variant?: 'neutral' | 'running' | 'queued' | 'warning' | 'error' | 'preset'
```

### Implementation shape (package)

| Lock | Value |
|------|-------|
| Owner | `@42ch/nexus-ui` `packages/nexus-ui/src/components/badge.tsx` |
| Mechanism | `cva` **`tone`** variant + **`compoundVariants`** for `tone × variant` (soft border strengthen + solid fills) |
| Default | `tone: 'soft'` in `defaultVariants` — omitting `tone` preserves soft callers |
| App wrapper | `apps/web/src/components/ui/badge.tsx` remains a **thin re-export**; no second class tree |
| `StatusBadge` | `apps/web/src/components/status-badge.tsx` continues to render `<Badge variant={…}>` **without** `tone` → soft; **no forced cutover** |

Default `tone="soft"` preserves all existing callers (`StatusBadge`, `ChapterStatusBadge`, `FindingStatusBadge`, etc.).

## Soft (default)

- Keep tinted fill + semantic text.
- **Strengthen borders:** neutral → `gray-alpha-400` (or equivalent); semantic border alpha ≈ **50%** (was ~30%).
- Base: height 24px, `px-2`, `rounded-pill`, `label-12`, `font-semibold`.

## Solid

- Solid semantic fill + **`#ffffff`** / `text-white` text (Button contrast rule).
- No visible border (`border-transparent`).
- Light palette (locked):

| variant | background |
|---------|------------|
| neutral | `gray-1000` |
| running | `green-700` |
| queued | `teal-700` |
| warning | `amber-700` |
| error | `red-800` |
| preset | `purple-700` |

- Dark: map in `DESIGN.dark.md` with sufficient contrast (use existing dark semantic scales; solid text remains white).

## DESIGN SSOT

- Update `DESIGN.md` / `DESIGN.dark.md` `components.badge-status-pill` to document **soft + solid** (prefer nested `.soft` / `.solid` maps under the component key, or explicit soft/solid sibling tables — implementer choice as long as both themes are complete).
- § Badge / Status Pill prose: mention `tone=soft|solid`, default soft.
- No schema / wire impact; token docs only.

## Studio

- `/components` BadgeSection: **Soft** row + **Solid** row (6 variants each).
- Keep `VariantLabel` under columns as labels only.

## Acceptance (Must)

1. Soft borders visibly distinct on light backgrounds per Soft rules above.
2. Solid tone for all six variants with white text and no visible border (light + dark DESIGN maps).
3. Default soft; omitting `tone` does not require caller changes.
4. Studio Soft + Solid matrices present; `VariantLabel` label-only.
5. Package tests + README cover `tone`.
6. No `schemas/` change; no Iconify; no forced `StatusBadge` cutover.

## Out of scope (Non-Goals)

- Forced cutover of product `StatusBadge` (or other callers) to solid.
- Finding-status / memory pill independent token trees (may reuse tone later).
- Schemas / wire changes; Iconify.
- Stretch P2 Surfaces / chrome polish (separate plan).
- Human interactive desktop smoke as an automated Done blocker for this plan.
