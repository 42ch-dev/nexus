# V1.101 Iteration Workspace

Iteration-scoped contracts and guides for **V1.101 — Desktop Setup Polish, Agent Picker & Select Stretch**.

## Product Throughline

V1.101 makes first-run agent choice and setup wizard chrome trustworthy after V1.100 clean-state launch (local-first ACP client: surface the author’s existing agents, no BYOK).

| Tier | What ships | Incomplete if missing? |
|------|------------|------------------------|
| **Must** | P0 Agent detection + AgentPicker; P1 wizard chrome (BACK / STEPS / POLL) | **Yes** |
| **Stretch** | P2 `Select` promotion only | **No** — whole-plan defer to V1.102 allowed |

Order: **P0 → P1 → P2**. Each plan: **Studio visual → App wiring**.

## Studio-first process (HARD)

See `guides/studio-first-visual-then-app.md`. For every UI-visual change: Design Studio fixtures and visual acceptance first, then App functional wiring.

**Smoke:** Interactive desktop smoke is a **separate human gate** — not an automated Done / CI blocker. Automated Done ≠ human smoke Done.

## Files

| Path | Purpose |
|------|---------|
| `specs/agent-picker-and-detection.md` | **Must / P0** — AgentPicker card-grid, scan→select, empty/error/custom-launch, outbound install/docs, Settings-reusable without shipping Settings (DF-70) |
| `specs/setup-wizard-ui-polish.md` | **Must / P1** — Back button, Steps indicator, daemon poll/subscription timing |
| `specs/select-promotion-contract.md` | **Stretch / P2** — `Select` promotion (deferrable; not a Must gate) |
| `guides/studio-first-visual-then-app.md` | Process note for visual vs logic tracks and smoke separation |

## Boundaries

- This workspace holds V1.101 iteration-scoped contracts and review-chain edits.
- Long-lived normative updates belong in `.mstar/specs/` only when the review chain promotes and locks them.
- Reusable implementation knowledge is promoted to `.mstar/knowledge/` only during iteration-close compound.
- **Settings shell is out of scope (DF-70).** Product still requires `AgentPicker` to be mountable later without wizard routing — reusability is in scope; Settings routes are not.
- **AgentPicker placement (architect-locked):** `apps/web/src/components/setup/agent-picker.tsx` — app-shared; **not** `@42ch/nexus-ui` this iteration.
- **Scan→UI (architect-locked):** Class B PATH/env → enrich daemon/sidecar process `PATH`; **no** `schemas/` change. Schema proposals = hard stop → PM/architect.
- Prefer `wire_contracts_changed: false`.

## Cross-links

| Artifact | Path |
|----------|------|
| Delivery compass | [`../v1.101-delivery-compass.md`](../v1.101-delivery-compass.md) |
| P0 plan | [`.mstar/plans/2026-07-09-v1.101-agent-detection-picker.md`](../../plans/2026-07-09-v1.101-agent-detection-picker.md) |
| P1 plan | [`.mstar/plans/2026-07-09-v1.101-setup-wizard-ui-polish.md`](../../plans/2026-07-09-v1.101-setup-wizard-ui-polish.md) |
| P2 plan (Stretch) | [`.mstar/plans/2026-07-09-v1.101-select-component-promotion.md`](../../plans/2026-07-09-v1.101-select-component-promotion.md) |
| Durable scan contract | [`.mstar/specs/desktop-shell.md`](../../specs/desktop-shell.md) §14 |
| Design Studio product contract | [`.mstar/specs/design-studio.md`](../../specs/design-studio.md) |
| Deferred Settings shell | DF-70 in [deferred-features tracker](../../knowledge/deferred-features-cross-version-tracker.md) |

## Prepare Readiness

- Product scope, non-goals, Must/Stretch split, and P0→P1→P2 order are documented in the compass and main plans.
- Architect §5.2 locks: AgentPicker app-shared path; scan Class A/B/C (Class B = PATH enrichment, no `schemas/`); P1 Poll = timing/subscription only; P2 Select = presentational package boundary.
- P0 may touch scan→UI / daemon env; prefer no schema change. Schema proposals block implement → architect/PM.
- P1 follows P0; frontend-only polish of existing wizard chrome; Poll is timing/subscription, not event-bus rewrite.
- P2 is Stretch and may be deferred whole after P0+P1 automated paths without marking Must incomplete.
- Writing §5.3: terminology and cross-links aligned; no product/architecture reopen.
