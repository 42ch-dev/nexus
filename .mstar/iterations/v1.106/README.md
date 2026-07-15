# V1.106 Iteration Workspace

Iteration-scoped contracts and guides for **V1.106 — UI Continuity & Studio-First Pipeline**.

**Compass:** [`v1.106/delivery-compass.md`](../v1.106/delivery-compass.md) (`status: active`)

## Story

| Phase | What we deliver | User-visible outcome |
|-------|-----------------|----------------------|
| Pipeline (P0 Must) | DESIGN + Studio Surfaces + keep-web lock | Preview launch/banner/toast in Studio; DESIGN documents Tabs/States/form/launch chrome |
| First-launch (P1 Must) | Shared AgentPicker in wizard; Workspace/Done layout | Wizard Agent matches Settings; Done feels celebratory, not empty |
| Continuity (P2 Stretch) | Badge/Select/Settings Advanced/AgentPicker chrome | Clearer badges, Select inset, simpler Settings nav — if capacity allows |

**Iteration complete when:** P0 + P1 automated gates pass. P2 may remain open.

| Tier | Plans | Iteration incomplete if missing? |
|------|-------|----------------------------------|
| **Must** | P0 studio-first-pipeline; P1 first-launch-polish | **Yes** |
| **Stretch** | P2 ui-continuity | **No** |

## Specs

| Path | Tier | Purpose |
|------|------|---------|
| `specs/studio-first-pipeline.md` | Must / P0 | SP-001..003 — pipeline Done + DESIGN/Surfaces checklist |
| `specs/first-launch-polish.md` | Must / P1 | FB-003/004 + V1.105 residual seeds |
| `specs/ui-continuity.md` | Stretch / P2 | FB-001/002/005/006 |

## Guides

| Path | Purpose |
|------|---------|
| `guides/studio-first-invariant.md` | Locked invariant: 需求 → Studio↔DESIGN.md → App |

## Locks (summary — §5.1 product lock)

- **Invariant:** No App-first visual invention for author-facing chrome.
- **Pipeline Done:** DESIGN + Studio fixture + promote/keep-web — not forced package promotion.
- **keep-web (V1.106):** Dialog, Tabs, Table, States.
- **AgentPicker:** App-shared; Settings + wizard same module.
- **Wire:** Prefer `wire_contracts_changed: false`.
- **Out:** DF-70 BYOK; Radix Select; package-promoting Dialog/Tabs/Table/States/AgentPicker/SettingsShell.

## Prepare status (Phase 1)

| Gate | P0 | P1 | P2 |
|------|----|----|-----|
| specify / clarify / plan | done | done | done |
| architect | done | done | done |
| writing | done | done | done |
