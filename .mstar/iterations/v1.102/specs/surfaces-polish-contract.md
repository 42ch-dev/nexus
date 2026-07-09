# Surfaces Polish Contract (V1.102 P2 Stretch)

**Status:** architect-locked (iteration-start §5.2)  
**Plan:** `2026-07-09-v1.102-ui-hygiene`  
**Tier:** Stretch — whole-plan defer to V1.103+ allowed  
**Wire:** Prefer `wire_contracts_changed: false`

## Goal

Make Design Studio Surfaces reviewable by section and polish Control Room / setup chrome residuals collected during V1.102 planning. Deferral does **not** leave Must incomplete.

## Author-facing outcome (if plan runs)

Contributors can jump to Surfaces sections in Studio instead of scrolling one endless page; wizard/shell/picker/daemon chrome reads intentional.

## Defer rule (HARD)

If P0+P1 consume the iteration, PM defers this **entire** plan to V1.103+ with reason in compass/`status.json`. Do not start until P0+P1 automated paths are Done (unless PM capacity exception). No Stretch item is a Must / iteration-incomplete blocker.

## Shared component boundaries (Studio → App parity)

| Surface | Studio locus | App locus | Boundary |
|---------|--------------|-----------|----------|
| **StepIndicator** | `apps/design-studio/src/fixtures/setup-wizard-chrome-fixtures.tsx` | Private helper in `apps/web/src/pages/setup-wizard-page.tsx` | Keep **app-local / fixture-local**; do **not** promote to `@42ch/nexus-ui` this iteration. Visual parity via Studio-first then App edit. |
| **DaemonStatusRegion** | Fixture in `setup-wizard-chrome-fixtures.tsx` | Wizard daemon step chrome in `setup-step-daemon.tsx` (+ related CTAs) | Shared **pattern**, not a package export. |
| **Daemon status strip** | `DaemonStatusStrip` in `apps/design-studio/src/pages/surfaces.tsx` | `apps/web/src/components/layout/daemon-status-bar.tsx` | Studio fixture ↔ App bar; single-line + Restart; stay presentational where possible. |
| **Sidebar nav** | `AppShellFixture` in `surfaces.tsx` | `apps/web/src/components/layout/sidebar.tsx` | Stay within DESIGN `components.sidebar-nav` tokens; quieter inactive / clear selected / parent as group. |
| **AgentPicker chrome** | Studio via `@web-setup/agent-picker` fixtures | `apps/web/src/components/setup/agent-picker.tsx` | **App-shared only** — not `@42ch/nexus-ui`. Chrome polish lands on the shared component so setup + Settings inherit. |
| **Surfaces section menu** | `apps/design-studio` routes only | — | **Studio-only**; never App Settings IA. |

### Surfaces Studio routes (locked when P2 runs)

Prefer deep-linkable section routes under Design Studio (update `apps/design-studio/src/App.tsx` + nav + smoke):

| Route | Section |
|-------|---------|
| `/surfaces` | Index / overview (or redirect to first section — implementer choice; smoke must cover) |
| `/surfaces/setup` | Setup wizard chrome |
| `/surfaces/shell` | App shell / sidebar |
| `/surfaces/agent-picker` | AgentPicker states |
| `/surfaces/daemon` | Daemon strip / status region |

Hash-only in-page anchors are acceptable **only** if deep links remain shareable and smoke-tested; prefer path segments above.

## Bundle

### Hygiene

- `R-V1100P1QC1-W002` — consolidate promoted-primitive lists in `tooling/check-ui-guardrails.sh` (single `PROMOTED_PRIMITIVES` or equivalent).
- Optional V1.101 Studio/AGENTS/index nits if capacity.

### Wizard chrome

1. **Steps:** connector must not paint above step 1; left panel fill vs top-align policy documented + fixed (Studio fixture + App `setup-wizard-page.tsx` `StepIndicator`).
2. **Back:** `ChevronLeft` icon-only tertiary + `aria-label="Back"`; keep horizontal adjacency to Continue; lucide only (no Iconify).
3. **Daemon error region:** Retry centered on top; concise left-aligned small error copy below; shared pattern for wizard status chips; Studio → App.

### Shell / picker / daemon

4. **Sidebar nav aesthetic:** quieter inactive rows; one clear selected treatment; parent as group/disclosure; stay within `sidebar-nav` tokens; Studio → App.
5. **AgentPicker chrome:** Installed as soft Badge; smaller arrow-only outbound icons aligned to text height; hollow dot when installed-unselected, lit when selected; Not installed cards muted opacity; still non-selectable as profile.
6. **Daemon status strip:** single-line footer (no description); left status + Badge; right Restart control; Studio → App `DaemonStatusBar`.

### Surfaces IA (Studio-only)

7. **Surfaces section menu / subpages:** split `/surfaces` per route table above; update Studio nav + smoke tests. **Design Studio only** — not an App Settings IA deliverable.

## Acceptance (Stretch — only if plan runs)

1. Surfaces deep links work in Studio; nav + smoke updated.
2. Wizard Steps / Back / error match the chrome rules above (Studio → App).
3. Sidebar / AgentPicker / daemon strip match the chrome rules above (Studio → App).
4. Guardrails consolidation if capacity remains.
5. No Iconify; prefer no `schemas/` change.
6. Human interactive desktop smoke remains a **separate gate**, not an automated Done blocker.

## Non-Goals

- Treating this plan as Must / iteration-incomplete.
- Shipping Surfaces section menu into App product Settings.
- Full Settings IA, BYOK, AgentPicker package promotion, Iconify, schemas.
- Forced `StatusBadge` solid cutover (P0 Non-Goal; not reopened here).
- Promoting StepIndicator / DaemonStatusRegion / shell chrome to `@42ch/nexus-ui` in this Stretch plan.
