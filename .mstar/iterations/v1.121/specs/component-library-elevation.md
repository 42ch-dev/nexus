# P1 Spec — Component Library Elevation

> Iteration: V1.121 “The Literary Engine”. Primary spec for plan `2026-07-17-v1.121-component-library-elevation`.
> Compass: S2. **Must** — elevating pages without the library creates one-off visual drift; P2/P3 inherit tactility from here.

## Problem statement

Components are token-correct but tactile-poor: no hover depth on interactive surfaces, flat focus and disabled treatments, leftover hardcoded values (`dialog.tsx` overlay `bg-black/40` despite the V1.111 `scrim` token; `states.tsx` raw color-mix arbitrary classes; calc/minmax arbitrary widths), and the studio gallery lacks a systematic states matrix. Authors feel every control as “flat UI kit,” not a premium writing instrument. The library must absorb v0.4 (elevation, motion, display tier, ink atmosphere) so P2/P3 surfaces inherit premium tactility **by construction**.

## User value

| Who | Outcome when P1 ships |
| --- | --- |
| Authors | Controls feel intentional — hover depth, calm focus, coherent disabled/loading states |
| Maintainers | Zero unexplained arbitrary values in the ui layer; gallery verifies every variant × state in both themes |
| P2/P3 implementers | Elevated primitives ready; no re-solving elevation/scrim/focus per page |

## Scope

- **`@42ch/nexus-ui` promoted components:** Button, Badge, Card, Input, Label, Textarea, Select, Toast.
- **keep-web `apps/web/src/components/ui/`:** Dialog, Sheet, Tabs, Table, States (Spinner/LoadingState/EmptyState/ErrorState).
- **Badge family (hardcoded sweep):** `status-badge`, `task-kind-badge`, `maturation-indicators`.
- **design-studio components gallery:** variants × states matrix (default/hover/focus/disabled/loading where applicable) in light + dark.

## Design intent

- **Elevation becomes interactive language.** Resting cards `elevation-1`; interactive cards hover to `elevation-2` + `translateY(-1px)`; popovers/menus `elevation-3`; dialogs/sheets `elevation-4`. Motion 160ms `ease-standard`, reduced-motion safe.
- **Focus ring stays two-layer** (V1.94 contract) but re-verified against ink dark surfaces (inner ring token may need the dark `background-100` value — confirm AA on new surfaces).
- **Disabled:** keep V1.113 `opacity.disabled` + gray-100/700 fills; re-check on ink surfaces.
- **Scrim convergence:** Dialog/Sheet overlays use `bg-scrim` token — remove `bg-black/40` (command palette scrim lands with P2 shell work; same token).
- **Typography voice discipline:** components stay sans (interface voice). Card titles may opt into display serif **only** when the card presents a creative entity (work/world) — **additive** `Card.Title` opt-in per the **prop-based** contract authored in P0 spec T8: a new optional `voice?: 'interface' | 'content'` prop on `CardTitle` (`packages/nexus-ui/src/components/card.tsx`), default `'interface'` preserving the current `text-heading-16 font-heading` treatment exactly; `voice="content"` swaps to `font-display text-display-20 tracking-tight`. Existing call sites compile and render identically (no breaking change; no CVA variant on `Card` root). This is not an API redesign. The opt-in is **greppable** (`voice="content"`) and reserved for creative-entity cards (work/world/brand-page). Class-only opt-in was rejected (drift risk — every call site reinvents the recipe). No serif in Button/Badge/Input/Table/Tabs.
- **States polish:** `states.tsx` color-mix arbitrary classes → token-backed error surface tokens (P0 provides); EmptyState headline may use display serif (content voice) per P0 gallery — **string content unchanged**.

## Hardcoded-value sweep (ui layer)

Convert to tokens or documented exceptions (target: zero unexplained arbitrary values):

| File | Item |
|------|------|
| `ui/dialog.tsx` | `bg-black/40` → `bg-scrim`; `w-[calc(100%-2rem)]`, `max-h-[85vh]` → dialog tokens or documented |
| `ui/sheet.tsx` | `w-[min(100vw,280px)]` → sheet width token |
| `ui/states.tsx` | raw `bg-[color-mix…]` / `border-[color-mix…]` → error-surface tokens from P0 |
| `components/status-badge.tsx`, `components/memory/task-kind-badge.tsx`, `components/reading/maturation-indicators.tsx` | color-mix arbitrary → P0 badge tint tokens |

## Acceptance criteria

- AC-P1-1 — All promoted + keep-web components consume v0.4 elevation/motion/scrim tokens; interactive elevation recipes implemented per component table in DESIGN.md body.
- AC-P1-2 — Zero hardcoded overlay/hex/raw-color-mix arbitrary values in `apps/web/src/components/ui/` and the three badge files above (grep evidence); remaining arbitrary values documented as exceptions on the plan.
- AC-P1-3 — Focus/disabled/AA re-verified on ink dark surfaces; two-layer ring AA ≥ 3:1 in both themes (evidence table in DESIGN.md or plan).
- AC-P1-4 — Studio components gallery renders variants × states matrix for Button, Badge, Card, Input, Select, Textarea, Tabs, Table, Dialog, States in light + dark; gallery builds and tests green.
- AC-P1-5 — `Card.Title` content-voice **additive** opt-in implemented per P0 spec T8 (`voice?: 'interface' | 'content'` prop on `CardTitle`, default `'interface'`; existing call sites unchanged); no serif in interface components (grep evidence).
- AC-P1-6 — `packages/nexus-ui`, `apps/web`, `apps/design-studio` typecheck + vitest + builds green; no component API **breaking** changes.
- AC-P1-7 — tailwind-merge regression (P0 T7, at `packages/nexus-ui/src/lib/cn.ts`) still green with any new classes introduced here; `tooling/check-ui-guardrails.sh` `check_cn_parity` still green.

## Non-goals

- No package promotion (keep-web → nexus-ui) decisions.
- No new components; no prop renames/removals (additive opt-ins only).
- No app-surface adoption beyond gallery fixtures (P2/P3 do surfaces).
- No i18n/copy rewrites; no wire/daemon changes; no desktop native chrome.

## Interfaces

- `packages/nexus-ui/src/components/*` (CVA variants — extend, don't restructure).
- `apps/web/src/components/ui/*`.
- `apps/design-studio/src/pages/components.tsx` (+ new states matrix section).
- DESIGN.md `components.*` frontmatter (P0 v0.4) as the value source.

## Validation plan

- Vitest per touched component; studio gallery screenshots (light/dark, states matrix); grep sweeps; AA spot-checks recorded.
