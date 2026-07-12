# UI Normalization Spec (V1.113 P1)

## User value

Disabled controls and listbox height behave consistently across Control Room UI
and themes. Authors get predictable affordances; implementers stop inventing
`opacity-50` / `max-h-[320px]` magic numbers.

## Problem

V1.111 QC found 2 DESIGN.md token gaps (R-V1111P3QC1-S001). Code-first
exploration found the `opacity-50` pattern used inline in 6+ component files for
**disabled** states, and `max-h-[320px]` hardcoded in the command palette.

## Scope

### Token additions (DESIGN.md frontmatter)

DESIGN.md and DESIGN.dark.md are at **repo root** (not `apps/web/`).

1. `components.sidebar-nav` disabled tokens (opacity + text color as needed)
2. `components.listbox.maxHeight` - max-height for listbox containers (command palette, etc.)
3. Shared disabled opacity token under `components.states.disabled`

Tokens are **additive only**. Mirror names in DESIGN.dark.md (values may match
light when opacity is theme-independent).

### Component sweep (disabled-state only)

Replace disabled-state `opacity-50` / `disabled:opacity-50` with the shared
token utility class; tokenize listbox max-height:

| File | Current | Target |
|------|---------|--------|
| sidebar.tsx | `opacity-50` (disabled) | `opacity-disabled` |
| edge-inspector.tsx | `disabled:opacity-50` | `disabled:opacity-disabled` |
| suggested-relationships-pane.tsx | `disabled:opacity-50` | `disabled:opacity-disabled` |
| conflict-modal-base.tsx | `disabled:opacity-50` | `disabled:opacity-disabled` |
| chapter-outline-content-editor.tsx | `disabled:opacity-50` | `disabled:opacity-disabled` |
| chapters-page.tsx | `opacity-50` (disabled) | `opacity-disabled` |
| command-palette.tsx | `max-h-[320px]` | `max-h-listbox` |

Wire tokens through `tailwind.config.ts` + `extendTailwindMerge` (V1.94 lesson).
The `extendTailwindMerge` config lives in `packages/nexus-ui/src/lib/cn.ts`.

## Acceptance criteria

- [ ] DESIGN.md SSOT contains the three token groups above
- [ ] Tailwind utilities exist and do not break build
- [ ] Sweep files use shared tokens; residual R-V1111P3QC1-S001 closable
- [ ] Grep audit: no remaining **disabled-state** `opacity-50` or listbox
  `max-h-[320px]` in `apps/web/src/**` for the targeted pattern

## Non-goals

- No existing token renamed or removed (additive only)
- No DESIGN.md body restructuring (frontmatter only)
- Decorative (non-disabled) opacity left alone unless clearly the same pattern
- i18n migration (owned by P0)
- Independent of P0 — may implement in parallel

## Dependency

- **blocked_by:** none (parallel-safe with P0)
