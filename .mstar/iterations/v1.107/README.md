# V1.107 Iteration Workspace

Iteration-scoped contracts and guides for **V1.107 — Studio UI Tune**.

**Compass:** [`v1.107/delivery-compass.md`](../v1.107/delivery-compass.md) (`status: locked`)

## Story

| Phase | What we deliver | User-visible outcome |
|-------|-----------------|----------------------|
| Paint (Must) | FB-000 Studio Tailwind content | Matrices and wizard chrome actually render in Studio |
| Visual (Must) | FB-001..011 primitives + wizard IA | Badge/Button/Select/MainBanner/AgentPicker trustworthy in Studio then App |
| Coverage (Must) | FB-012..016 Toast + shell/Settings + badges/backlog | Studio↔App less drift; domain badges previewable; Surfaces backlog indexed |

**Iteration complete when:** P0 Must plan Done (FB-000..016 or documented residual deferrals for 016 backlog-only scope).

| Tier | Plans | Iteration incomplete if missing? |
|------|-------|----------------------------------|
| **Must** | P0 studio-ui-tune | **Yes** |

## Specs

| Path | Tier | Purpose |
|------|------|---------|
| `specs/studio-ui-tune.md` | Must / P0 | FB-000..016 — §5.1 product-complete |
| `specs/author-surfaces-backlog.md` | Index (016) | Deferred author Surfaces triggers |

## Guides

| Path | Purpose |
|------|---------|
| `guides/studio-first-invariant.md` | Carry-forward studio-first invariant |

## Locks (summary — §5.1 product lock)

- **Invariant:** No App-first visual invention for author-facing chrome.
- **Studio content:** `setup/**` + `packages/nexus-ui/src/**` scanned (FB-000).
- **Wizard IA:** compact single-column; no custom launch; shared workspace field.
- **Toast:** App adopts package (closes `R-V1106P0-001`).
- **Wire:** Prefer `wire_contracts_changed: false`.
- **Out:** DF-70 BYOK; package-promoting Dialog/Tabs/Table/States.

## Prepare status (Phase 1)

| Gate | P0 |
|------|----|
| specify | **done** (§5.1 PM) |
| clarify | **done** (§5.1 PM — grill-me locked) |
| plan | **done** (§5.1 PM — tasks + AC product-complete) |
| architect | **done** (§5.2) |
| writing | **done** (§5.3) |
