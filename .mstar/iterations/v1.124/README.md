# V1.124 — Studio-first Gap Closure: Timeline · Layer · Tokens

Iteration package for V1.124. Compass: [`delivery-compass.md`](delivery-compass.md).

## Scope (one sentence)

Close the V1.122/V1.123 studio-first gap: Timeline visuals and tokens shipped in App/CSS without a daemon-free Studio review path; this iteration makes them **gallery-verifiable**, audits sibling unrepresented surfaces in product priority order, and records promotion decisions that serve cross-app reuse (not tidiness).

## Plans (4 business; L budget)

| plan_id | Path | Status |
|---------|------|--------|
| P0 — Studio-first Timeline fixtures + AGENTS.md policy land | [`../../../plans/2026-07-19-v1.124-p0-studio-timeline-fixtures.md`](../../../plans/2026-07-19-v1.124-p0-studio-timeline-fixtures.md) | Todo |
| P1 — Studio Tokens gallery completion | [`../../../plans/2026-07-19-v1.124-p1-studio-tokens-gallery-completion.md`](../../../plans/2026-07-19-v1.124-p1-studio-tokens-gallery-completion.md) | Todo |
| P2 — Unrepresented surface audit + fixtures | [`../../../plans/2026-07-19-v1.124-p2-unrepresented-surface-audit.md`](../../../plans/2026-07-19-v1.124-p2-unrepresented-surface-audit.md) | Todo |
| P3 — Promotion classification audit + decisions | [`../../../plans/2026-07-19-v1.124-p3-promotion-classification-audit.md`](../../../plans/2026-07-19-v1.124-p3-promotion-classification-audit.md) | Todo |

## Documents

| Path | Author (Prepare) | Purpose |
|------|------------------|---------|
| `delivery-compass.md` | project-manager | Iteration SSOT (this package) |
| `specs/studio-fixture-acceptance-criteria.md` | product-manager (seat 1) + architect §8 (seat 2) | **Product contract** — F1–F9 + architectural testability / F4 extract map |
| `specs/studio-timeline-fixture-boundaries.md` | architect (Phase 1 seat 2) | **Locked** Timeline extract path + per-kind accent map (P0 consumes) |
| `specs/tokens-gallery-audit.md` | architect (Phase 1 seat 2) | **Locked** nine-token delta + gallery IA + recurrence skeleton (P1 consumes) |
| `specs/surface-audit-checklist.md` | architect (Phase 1 seat 2) | **Locked** P2 pre-classify + extract/alias decisions; Execute deepens full file walk |
| `guides/studio-first-policy-rollforward.md` | writing-specialist (optional — declined as redundant; policy + F1–F9 + recurrence gate already cover the ground) | How root `AGENTS.md` UI Component Policy applies to backlog |

## Foundation artifact (pre-iteration)

Root [`AGENTS.md`](../../../AGENTS.md) § "UI Component Policy (Studio-first)" — already on disk (uncommitted); lands with **P0 Task 1** (policy before fixtures). Operationalizes the policy motivated by the gap this iteration closes.

## Product priority (P2 fixtures)

1. Global Timeline overview  
2. Layer breadcrumb  
3. Conflict-modal family  
4. Alt-view toggles  
5. Defer-by-default: idea-input, canvas-nav-commands  

## Authority

- Root [`AGENTS.md`](../../../AGENTS.md) § UI Component Policy
- [`apps/design-studio/AGENTS.md`](../../../apps/design-studio/AGENTS.md) — Studio import boundaries
- [`packages/nexus-ui/AGENTS.md`](../../../packages/nexus-ui/AGENTS.md) — Promotion rules
- [`.mstar/knowledge/architecture-patterns/ui-component-promotion-workflow.md`](../../knowledge/architecture-patterns/ui-component-promotion-workflow.md) — Canonical V1.106 studio-first workflow
- [`.mstar/specs/design-studio.md`](../../specs/design-studio.md) — Studio spec
