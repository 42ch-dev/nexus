# Studio-First: Visual Then App (V1.102)

## Rule

For every UI-visual change in this iteration:

1. **Design Studio** — build or update fixtures that demonstrate the visual states.
2. **Visual acceptance** — Studio checks / human visual pass as required by the plan.
3. **App wiring** — only after (1)+(2), connect routing/persistence/daemon behavior in `apps/web` / desktop.

## Must vs Stretch

| Plan | Tier | Studio → App required? |
|------|------|------------------------|
| P0 Badge soft/solid | **Must** | Yes (Studio matrix before/with package) |
| P1 Thin Settings host | **Must** | Yes for Settings chrome; AgentPicker already exists |
| P2 Surfaces polish | **Stretch** | Yes **if** the plan runs; skip entirely if deferred |

Do not start P2 until P0+P1 automated paths are Done (unless PM documents a capacity exception). Surfaces section menu is **Studio-only**.

## Automated vs human smoke

| Gate | Blocks automated Done? |
|------|------------------------|
| Studio visual + Vitest/CI | **Yes** (for the plan under review, when that plan runs) |
| Interactive macOS desktop smoke | **No** — separate human gate |

Automated Done ≠ smoke Done. Schedule human smoke after automated Must paths land; do not list smoke as a Must checkbox that blocks plan Done.

## Architect locks (do not reopen in implement)

| Topic | Lock |
|-------|------|
| Badge API | `tone?: 'soft' \| 'solid'`, default soft; implement via cva + `compoundVariants` on `@42ch/nexus-ui` Badge |
| Badge DESIGN | Soft + solid maps in `DESIGN.md` / `DESIGN.dark.md`; no schemas |
| StatusBadge | No forced solid cutover |
| Settings route | **`/settings`** under `SetupGate` → `RootLayout` in `apps/web/src/App.tsx`; page `settings-page.tsx` |
| Settings nav | Label **Settings**; lucide `Settings`; sidebar **footer utility** above `FooterProfiles` + `MOBILE_NAV` |
| Settings persistence | **`DesktopCapabilities.setAgentProfile`** (same as setup `finish()`); scan via `useScanAgents` |
| Settings slice | Thin host only (DF-70 A) — one route + one AgentPicker page; no `/settings/*` |
| AgentPicker placement | App-shared path — not nexus-ui |
| Icons | lucide-react only — no Iconify |
| Wire | Prefer no `schemas/` change (`wire_contracts_changed: false`) |
| Surfaces menu | Studio-only routes under `/surfaces/...` (Stretch P2) |
| P2 shared chrome | StepIndicator / DaemonStatusRegion stay app+fixture local; AgentPicker chrome on shared app component |
