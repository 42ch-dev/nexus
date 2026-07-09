# V1.102 Iteration Workspace

Iteration-scoped contracts and guides for **V1.102 — Badge Tone, Thin Settings & Surfaces Polish**.

## Product Throughline

V1.102 makes status pills readable in soft and solid tones, gives authors a thin Settings host for the existing AgentPicker, and optionally polishes Design Studio Surfaces for reviewability.

| Tier | What ships | Incomplete if missing? |
|------|------------|------------------------|
| **Must** | P0 Badge soft/solid; P1 thin Settings host (DF-70 slice A) | **Yes** |
| **Stretch** | P2 UI hygiene + Surfaces polish | **No** — whole-plan defer to V1.103+ allowed |

Order: **P0 → P1 → P2**. Do not start P2 until P0+P1 automated paths are Done (unless PM capacity exception). Each UI-visual plan: **Studio visual → App wiring**.

## Studio-first process (HARD)

See `guides/studio-first-visual-then-app.md`. For every UI-visual change: Design Studio fixtures and visual acceptance first, then App functional wiring.

**Smoke:** Interactive desktop smoke is a **separate human gate** — not an automated Done / CI blocker. Automated Done ≠ smoke Done.

## Files

| Path | Purpose |
|------|---------|
| `specs/badge-soft-solid-contract.md` | **Must / P0** — Badge `tone` soft/solid + soft border contrast |
| `specs/settings-thin-host.md` | **Must / P1** — DF-70 slice A thin Settings host for AgentPicker |
| `specs/surfaces-polish-contract.md` | **Stretch / P2** — Surfaces menu (**Studio-only**) + chrome polish bundle |
| `guides/studio-first-visual-then-app.md` | Process note: Studio visual → App wiring |

## Boundaries

- Prefer `wire_contracts_changed: false`.
- AgentPicker stays app-shared at `apps/web/src/components/setup/agent-picker.tsx` — **not** `@42ch/nexus-ui`.
- Settings = **thin host only**: route **`/settings`**, nav label **Settings** (sidebar footer utility), persist **`setAgentProfile`**; fuller IA deferred.
- Badge = package `tone` via cva/compoundVariants; DESIGN soft+solid; no `StatusBadge` cutover; no schemas.
- Surfaces section menu = **Design Studio only** (Stretch `/surfaces/...`); not App Settings IA.
- Icons: existing `lucide-react` only — **no Iconify**.
- No forced `StatusBadge` → solid cutover.

## Architect locks

See compass § Architecture Locks and `specs/*.md` (status: architect-locked). Compass remains `status: active` until PM lock.

## Cross-links

| Artifact | Path |
|----------|------|
| Delivery compass | [`../v1.102-delivery-compass.md`](../v1.102-delivery-compass.md) |
| P0 plan | [`.mstar/plans/2026-07-09-v1.102-badge-soft-solid.md`](../../plans/2026-07-09-v1.102-badge-soft-solid.md) |
| P1 plan | [`.mstar/plans/2026-07-09-v1.102-settings-shell.md`](../../plans/2026-07-09-v1.102-settings-shell.md) |
| P2 plan (Stretch) | [`.mstar/plans/2026-07-09-v1.102-ui-hygiene.md`](../../plans/2026-07-09-v1.102-ui-hygiene.md) |
