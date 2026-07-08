# V1.99 Iteration Workspace

Iteration-scoped workspace for **Design System Deepening**.

## Product Thread

V1.99 validates one product-development loop:

1. **Design Studio first** — compose pure View fixtures quickly without daemon/Tauri packaging.
2. **Promote only reusable primitives** — move accepted presentational components into `@42ch/nexus-ui` when they are app-state-free.
3. **Integrate in Web** — keep real data, routing, daemon transport, and product behavior in `apps/web`.
4. **Codify only what proved useful** — update repo/package guidance during the iteration; defer reusable knowledge promotion to iteration-close.

This workspace is evidence for that loop. It should not become a parallel product spec tree or a substitute for implementation plans.

## Specs

| Document | Purpose | Status |
| --- | --- | --- |
| [`specs/component-promotion-boundary.md`](specs/component-promotion-boundary.md) | Draft package-boundary rules, package API constraints, first-batch recommendation, and product rationale for promoting pure presentational UI primitives into `@42ch/nexus-ui` | Draft |

## Guides

| Document | Purpose | Status |
| --- | --- | --- |
| [`guides/studio-first-ui-workflow.md`](guides/studio-first-ui-workflow.md) | Draft workflow, `/surfaces` fixture boundary, and evidence template for visual iteration in `apps/design-studio` before package promotion and `apps/web` integration | Draft |

## Boundary

This workspace is iteration-scoped. Do not promote content to `.mstar/knowledge/` during iteration-start; use `mstar-compound` at iteration-close after the workflow has been validated.
