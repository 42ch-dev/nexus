# Studio-First: Visual Then App (V1.101)

> **Promoted to:** `.mstar/knowledge/architecture-patterns/ui-component-promotion-workflow.md` § V1.101 Extension (compound V1.101). This file remains the iteration workspace snapshot.

## Rule

For every UI-visual change in this iteration:

1. **Design Studio** — build or update fixtures that demonstrate the visual states.
2. **Visual acceptance** — Studio checks / human visual pass as required by the plan.
3. **App wiring** — only after (1)+(2), connect daemon/scan/persistence/poll logic in `apps/web` / desktop.

Visual experience problems and interaction-logic problems are solved on **separate tracks**. Do not mix “make it look right” and “make scan work” in the same unchecked task blob.

## Must vs Stretch (do not confuse)

| Plan | Tier | Studio → App required? |
|------|------|------------------------|
| P0 AgentPicker | **Must** | Yes |
| P1 wizard chrome | **Must** | Yes |
| P2 Select | **Stretch** (deferrable) | Yes **if** the plan runs; skip entirely if deferred |

Order: **P0 → P1 → P2**. Deferring P2 never blocks Must automated Done.

## Automated vs human smoke

| Gate | What it proves | Blocks automated Done? |
|------|----------------|------------------------|
| Studio visual + Vitest/CI | Fixtures, unit/integration of wiring | **Yes** (for the plan under review) |
| Interactive macOS desktop smoke | Real PATH agents, real wizard feel | **No** — separate human gate |

Do **not** put interactive smoke steps inside the automated QA drive checklist as hard blockers. Do **not** leave Must residuals open solely “waiting for human smoke” when automated evidence already closes the product bar — PM may still schedule smoke for PATH confirmation and record disposition separately.

PM schedules human smoke after automated paths land. Automated Done ≠ human smoke Done.

## Applies to

- P0 AgentPicker visuals (`apps/web/src/components/setup/agent-picker.tsx`, Studio via gallery alias) → then scan/select/custom-launch wiring (+ Class B PATH enrichment if RCA requires)
- P1 wizard chrome visuals → then poll/subscription wiring (timing/dedup only)
- P2 Select (if run) → Studio → presentational package → app consumers

## Architect locks (do not reopen in implement)

| Topic | Lock |
|-------|------|
| AgentPicker placement | App-shared `apps/web/src/components/setup/agent-picker.tsx` — not `@42ch/nexus-ui` |
| Scan PATH/env | Enrich daemon/sidecar process PATH; no `schemas/` change |
| P1 Poll | Subscription/timing only |
| P2 Select | Presentational package boundary only |
