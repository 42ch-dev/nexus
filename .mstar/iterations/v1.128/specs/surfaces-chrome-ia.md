# Spec — Surfaces chrome IA (V1.128 P0)

**Status:** product-reviewed, architect-locked, writing-hygiene done  
**Document class:** Iteration package working spec (not `{SPECS_DIR}` Master)  
**Compass:** [`../delivery-compass.md`](../delivery-compass.md) — AC-V1128-1  
**Plan:** [`2026-07-20-v1.128-p0-surfaces-chrome-ia`](../../../plans/2026-07-20-v1.128-p0-surfaces-chrome-ia.md)  
**Wire contracts:** `wire_contracts_changed: false`  
**Related specs:** [P3 — `@web-*` / `@42ch/nexus-ui` clarity](web-alias-clarity.md) (P0 owns no new `@web-*` extracts; P3 labels Surfaces pages)

## Problem

Design Studio Surfaces uses a horizontal pill strip for section switching (slow scan, easy to miss sections). Selection Submenu’s Agent dialog fixture paints a full-viewport overlay that blocks reopening other variants — a maintainer cannot dogfood the gallery. Banner remains in Studio Surfaces after V1.125 removed `MainBanner` from live App chrome, creating false expectations.

## User value

**Before:** A maintainer clicks through horizontal pills to compare Surfaces fixtures, hits a dead Banner section that App no longer ships, and gets trapped behind a full-screen Selection Agent overlay.

**After:** The maintainer scans all Surfaces sections from a left sidebar in one glance, sees no Banner ghost chrome, and exercises Selection Agent dialog open/close/reopen without losing access to sibling variants — enabling fast visual review before App wiring.

## Normative decisions

1. **Surfaces section nav** is a persistent **left sidebar** listing all Surfaces sections (Overview, Setup, Shell, AgentPicker, Canvas, Daemon, Launch, Selection Submenu — **no Banner**). Active section highlighted; click navigates nested routes under `/surfaces/*`.
2. **Banner retired from Studio (Must):** remove `/surfaces/banner` route, `SURFACES_SECTIONS` entry, `MainBannerFixtures` page usage, and related tests. Do not resurrect App `MainBanner`. This is non-negotiable — Banner was removed from App in V1.125; Studio must match.
3. **Selection Agent/dialog variant (Must):** must render inside a **scoped relative host** (fixed-height frame) with explicit **Open / reopen** control — mirror `conflict-modal-fixtures.tsx` `InlineModalHost` pattern (`relative min-h-[120px]` + Open button; see `apps/design-studio/src/fixtures/conflict-modal-fixtures.tsx:84–104`). Remove page-blocking overlay (`selection-submenu-fixtures.tsx:177–178` `fixed inset-0` is the defect to fix). No `fixed inset-0` page-blocking overlay that covers sibling gallery content. Author must be able to close, see other variants, and reopen.

### Architect decisions (Seat 2)

| Decision | Lock |
|----------|------|
| Scope boundary | Studio-only (`apps/design-studio`); no App wiring; no `@web-*` new extracts |
| Selection overlay pattern | `InlineModalHost`-style scoped frame — evidence: `conflict-modal-fixtures.tsx:94` |
| Wire contracts | `wire_contracts_changed: false` |

## Acceptance (author-observable)

| ID | Author sees / does |
|----|-------------------|
| AC-V1128-1a | Opens Design Studio `/surfaces/*` and switches sections via a **persistent left sidebar**; active section is visually highlighted; content area updates without horizontal pill strip |
| AC-V1128-1b | Cannot find Banner in Surfaces nav, routes, or fixtures — `/surfaces/banner` returns 404 or redirect; grep-equivalent: no Banner entry in Studio Surfaces |
| AC-V1128-1c | On Selection Submenu, opens Agent/dialog variant inside a framed host, closes it, **sees and clicks other variants**, then reopens Agent/dialog without page reload |

## Priority

| Item | Priority |
|------|----------|
| Banner retire (AC-1b) | **Must** |
| Selection reopen without block (AC-1c) | **Must** |
| Left sidebar (AC-1a) | **Must** |

## Out of scope

Product Selection submenu behavior beyond Studio fixture hygiene; App MainBanner (already unmounted V1.125).
