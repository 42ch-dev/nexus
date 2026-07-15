# V1.108 Iteration Workspace

Iteration-scoped contracts and guides for **V1.108 — Canvas Outline Spatial + UI Shell SSOT**.

**Compass:** [`v1.108/delivery-compass.md`](../v1.108/delivery-compass.md) (`status: locked`)

## Story

| Track | What we deliver | User-visible outcome |
|-------|-----------------|----------------------|
| P0 Canvas Must | Outline spatial React Flow parity (C1) | Authors shape outline/timeline on a graph with alt toggle + foreshadow min |
| P1 UI Must | Shell SSOT + AgentPicker FBs + Work CTAs + Studio canvas | Settings/Studio chrome unified; canvas discoverable + previewable |

**Iteration complete when:** Both Must plans Done (or non-blocking residuals documented).

| Tier | Plans | Iteration incomplete if missing? |
|------|-------|----------------------------------|
| **Must** | P0 `canvas-outline-spatial`; P1 `ui-shell-ssot` | **Yes** (either missing) |

## Phase 1 progress (§5 Review & Edit)

| Step | Role | P0 Canvas | P1 UI |
|------|------|-----------|-------|
| §5.1 | product-manager | **done** — spec + plan product-complete | **done** — spec + plan product-complete |
| §5.2 | architect | **done** — architecture locks in primary specs | **done** — architecture locks in primary specs |
| §5.3 | writing-specialist | **done** — Voice & Content + corpus hygiene | **done** — Voice & Content + corpus hygiene |
| §5.4 | project-manager lock | pending (`status: active`) | pending |

## Specs

| Path | Tier | FB IDs | Purpose |
|------|------|--------|---------|
| `specs/canvas-outline-spatial.md` | Must / P0 | FB-C1-000..006 | C1 Outline spatial + alt + tokens + foreshadow min |
| `specs/ui-shell-ssot.md` | Must / P1 | FB-UI-001..010 | Shell SSOT, AgentPicker, Work CTAs, Studio canvas, KB honesty |

## Guides

| Path | Purpose |
|------|---------|
| `guides/studio-first-invariant.md` | Carry-forward studio-first invariant |

## Locks (summary — grill-me)

- Dual Must; C1 Canvas; UI FB-UI-001..010 (001..008 UI review + 009 Work CTAs + 010 KB honesty)
- Branch: `main` → `iteration/v1.108` → PR `main`
- Wire prefer `false` both plans
- Studio-first carry-forward from V1.106/V1.107
