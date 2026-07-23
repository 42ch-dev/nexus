# V1.135 iteration package

Dogfood correction: restore **sidebar menu-area** create (not content dual-pane) + Dock icon deep RCA until author squircle confirm.

## Product locks (read first)

| Defect | Author ask | Done means | Not Done |
|--------|------------|------------|----------|
| **P0** 【图1】 | Create in **left shell sidebar menu slot** (`ShellSidebarChrome` `panelContent`) | Sidebar populated; content = browse only | Content dual-pane left create; empty sidebar |
| **P1** 【图2】 | Dock **squircle** for `nexus-desktop` | Author macOS Dock confirm after cache-invalid ritual | Studio VI-004, preview PNG, or opacity metadata alone |

**Terminology:** “功能区” = shell **sidebar menu slot** (`ShellSidebarChrome` `panelContent`) — **not** the content dual-pane left column inside `CreatorHubDualPane`. **Dock Done** = author macOS Dock squircle confirm — **not** Studio VI-004 or PNG opacity metadata alone.

## V1.134 residuals

| ID | Closes in |
|----|-----------|
| `R-V1134P3-001` | P0 — sidebar create, supersede dual-pane accept |
| `R-V1134P1-001` | P1 — author Dock squircle confirm |

## Artifacts

| Artifact | Path |
|----------|------|
| Compass | [delivery-compass.md](./delivery-compass.md) |
| P0 plan | [../../plans/2026-07-23-v1.135-p0-sidebar-menu-create-ia.md](../../plans/2026-07-23-v1.135-p0-sidebar-menu-create-ia.md) |
| P1 plan | [../../plans/2026-07-23-v1.135-p1-dock-icon-squircle-rca.md](../../plans/2026-07-23-v1.135-p1-dock-icon-squircle-rca.md) |
| P0 spec (normative IA) | [specs/p0-sidebar-menu-create-ia.md](./specs/p0-sidebar-menu-create-ia.md) |
| P1 spec (normative pipeline) | [specs/p1-dock-icon-pipeline.md](./specs/p1-dock-icon-pipeline.md) |
| P1 RCA guide | `guides/p1-dock-icon-rca.md` — created by P1 Task 1 (not present until implement) |

## Scale

**M** — exactly **2 Must** business plans; fix-until-done **within** those plans, not unbounded plan count.
