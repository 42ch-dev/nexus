# V1.128 — Design Studio + UI dogfood feedback

Iteration package — `delivery-compass.md` + specs. Not `{KNOWLEDGE_DIR}/`. Worthy content is **promoted** at iteration-close via `mstar-compound`.

**Status:** completed (2026-07-20). Integration branch `iteration/v1.128` pending squash-merge PR to `main`.

**Phase 1 Review & Edit chain: COMPLETE** (PM seat 1 → architect seat 2 → writing-specialist seat 3).

- **Seat 1 (product-manager):** User value + author-observable ACs on compass and all specs; P0 Banner/Selection Must; P1 dogfood scope cap; P2 Create page CTAs + Back semantics; P3 labeling over promotion.
- **Seat 2 (architect):** All four specs + plans architect-locked. P2 Back → clear selection → Create page; P2 selection → `CreatorEntitySelectionContext`; P1 RF → no DnD expansion. Serial order P0→P1→P2→P3 confirmed.
- **Seat 3 (writing-specialist):** Status lines normalized; terminology aligned (Create page, Controller Panel stub, `@web-*`, `@42ch/nexus-ui`); P0–P3 cross-links; compass + README polished. Deferred-features tracker unchanged.

## Documents

| Document | Kind | Description | Status |
|----------|------|-------------|--------|
| [delivery-compass.md](delivery-compass.md) | compass | Scope, plans, branch policy, iteration-close summaries | completed |
| [specs/surfaces-chrome-ia.md](specs/surfaces-chrome-ia.md) | spec (P0) | Surfaces sidebar, Banner retire, Selection overlay | product-reviewed, architect-locked, writing-hygiene done |
| [specs/nle-timeline-canvas.md](specs/nle-timeline-canvas.md) | spec (P1) | NLE Timeline Studio → App | product-reviewed, architect-locked, writing-hygiene done |
| [specs/creator-create-controller-shell.md](specs/creator-create-controller-shell.md) | spec (P2) | Create page vs Controller Panel stub | product-reviewed, architect-locked, writing-hygiene done |
| [specs/web-alias-clarity.md](specs/web-alias-clarity.md) | spec (P3) | `@web-*` vs `@42ch/nexus-ui` clarity | product-reviewed, architect-locked, writing-hygiene done |

## Plans

- [`2026-07-20-v1.128-p0-surfaces-chrome-ia`](../../plans/2026-07-20-v1.128-p0-surfaces-chrome-ia.md)
- [`2026-07-20-v1.128-p1-nle-timeline-canvas`](../../plans/2026-07-20-v1.128-p1-nle-timeline-canvas.md)
- [`2026-07-20-v1.128-p2-creator-create-controller-shell`](../../plans/2026-07-20-v1.128-p2-creator-create-controller-shell.md)
- [`2026-07-20-v1.128-p3-web-alias-clarity`](../../plans/2026-07-20-v1.128-p3-web-alias-clarity.md)

## Promotion log (filled at iteration-close)

| Source | Promoted to | Date | Notes |
|--------|-------------|------|-------|
| P2 creator-create-controller-shell | `knowledge/architecture-patterns/creator-shell-content-mode-pattern.md` | 2026-07-20 | New — context SSOT + Create/Controller modes |
| P3 web-alias-clarity (+ P1 NLE overlay lesson) | `knowledge/architecture-patterns/ui-component-promotion-workflow.md` § V1.128 Extension | 2026-07-20 | Updated — two-tier import model + badges |
| P1 nle-timeline-canvas (RF-free overlay) | *(folded into ui-component-promotion-workflow V1.128 extension)* | 2026-07-20 | Skipped standalone — overlaps canvas knowledge |
