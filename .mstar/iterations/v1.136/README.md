# V1.136 iteration package

Dogfood correction: **Dock squircle** follow-up + **sidebar inline create** (World/Work tabs) + **Light Mode interactive VI** (`brand-cyan-1000` + TransportError link CTAs).

## Product locks (read first)

| Defect | Author ask | Done means | Not Done |
|--------|------------|------------|----------|
| **P0** 【图1】 | macOS Dock **squircle** for `nexus-desktop` | Author Dock eyeball after documented rebuild ritual | Studio/PNG/icns inspection alone |
| **P1** 【图2】/【图3】 | **Inline create** in sidebar 功能区 — World \| Work tabs + title + submit | Sidebar inline form; content = browse/empty only | Two dashed `CreateCardButton` cards; dialog-only create |
| **P2** | Light **interactive** = `brand-cyan-1000` (`#117480`); ink deep-blue for titlebar/links | Studio Tokens/Brand/Components/Surfaces prove SSOT | Neon cyan on light; office deep-blue fills; filled TransportError buttons |

**Terminology:** **功能区** = shell sidebar `panelContent` (`ShellSidebarChrome`) — **not** content dual-pane. **Inline create zone** = local World \| Work tabs + form inside sidebar — **not** card buttons. **Light interactive** = `brand-cyan-1000` fills/selection — neon cyan is **Dark-only**. **Studio** = author-facing SSOT for tokens, brand, components, and surfaces.

## V1.135 residuals carried

| ID | Closes in |
|----|-----------|
| `R-V1135P1-001`, `R-V1135P1-005` | P0 — author Dock squircle |
| `R-V1135P0-001`, `R-V1134P3-001` | P1 — inline-tab sidebar create |

## Artifacts

| Artifact | Path |
|----------|------|
| Compass | [delivery-compass.md](./delivery-compass.md) |
| P0 plan | [../../plans/2026-07-23-v1.136-p0-dock-icon-squircle-followup.md](../../plans/2026-07-23-v1.136-p0-dock-icon-squircle-followup.md) |
| P1 plan | [../../plans/2026-07-23-v1.136-p1-sidebar-inline-create-tabs.md](../../plans/2026-07-23-v1.136-p1-sidebar-inline-create-tabs.md) |
| P2 plan | [../../plans/2026-07-23-v1.136-p2-light-mode-chrome-harmony.md](../../plans/2026-07-23-v1.136-p2-light-mode-chrome-harmony.md) |
| P0 spec (normative pipeline) | [specs/p0-dock-icon-squircle-followup.md](./specs/p0-dock-icon-squircle-followup.md) |
| P1 spec (normative IA) | [specs/p1-sidebar-inline-create-tabs.md](./specs/p1-sidebar-inline-create-tabs.md) |
| P2 spec (normative VI) | [specs/p2-light-mode-interactive-vi.md](./specs/p2-light-mode-interactive-vi.md) |
| P0 RCA guide | `guides/p0-dock-icon-rca.md` — extend V1.135 `guides/p1-dock-icon-rca.md` (implement Task 1; not present until implement) |

## Scale

**M** — exactly **3 Must** business plans (P0 Dock, P1 sidebar IA, P2 light VI); P2 wave 2 after P0/P1 token surface stabilizes.
