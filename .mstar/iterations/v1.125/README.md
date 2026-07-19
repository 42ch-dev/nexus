# V1.125 — Control Room dogfood + World-first IA

Iteration package for V1.125. Compass: [`delivery-compass.md`](delivery-compass.md).

**Status:** Phase 1 Review & Edit (product-manager seat 1 → architect seat 2 → **writing-specialist seat 3 done** → PM lock pending)

## Scope (one sentence)

Restore Control Room dogfood trust (daemon gate, agent instant-apply, orchestration honest load UX) and pivot Creator sidebar to Worlds-first experience-driven IA — deferring selection-submenu shell, canvas Brief/Narrative axis, and Schedule→cron UX to V1.126+.

## Plans (3 business; M budget)

| plan_id | Path | AC |
|---------|------|-----|
| P0 — Shell daemon gate + agent instant-apply | [`../../plans/2026-07-19-v1.125-shell-daemon-agent.md`](../../plans/2026-07-19-v1.125-shell-daemon-agent.md) | AC-V1125-1, AC-V1125-2 |
| P1 — Orchestration repair + Memory move | [`../../plans/2026-07-19-v1.125-orchestration-repair-memory.md`](../../plans/2026-07-19-v1.125-orchestration-repair-memory.md) | AC-V1125-3, AC-V1125-4 |
| P2 — Creation World-first IA first slice | [`../../plans/2026-07-19-v1.125-creation-world-first-ia.md`](../../plans/2026-07-19-v1.125-creation-world-first-ia.md) | AC-V1125-5 |

## Documents

| Path | Author (Prepare) | Purpose | Status |
|------|------------------|---------|--------|
| [`delivery-compass.md`](delivery-compass.md) | project-manager (seat 1) + architect (seat 2) | Iteration SSOT — scope, ACs, architecture locks | active |
| [`specs/shell-daemon-agent.md`](specs/shell-daemon-agent.md) | product + architect (seats 1–2); writing-specialist (seat 3) | P0 daemon gate + agent instant-apply | product-reviewed, architect-reviewed, writing-hygiene done |
| [`specs/orchestration-repair-memory.md`](specs/orchestration-repair-memory.md) | product + architect (seats 1–2); writing-specialist (seat 3) | P1 orchestration load UX + Memory move | product-reviewed, architect-reviewed, writing-hygiene done |
| [`specs/creation-world-first-ia.md`](specs/creation-world-first-ia.md) | product + architect (seats 1–2); writing-specialist (seat 3) | P2 Worlds-first Creator sidebar + empty CTAs | product-reviewed, architect-reviewed, writing-hygiene done |

## Boundary

- **Do not** add new `{KNOWLEDGE_DIR}/` docs in Phase 1 Review chain.
- Shipped Master [`web-ui.md`](../../specs/web-ui.md) receives a **minimal** §29.17.4 pointer only (Creator list-mode sidebar; §29.18 Canvas Timeline default unchanged).
- Promote package → knowledge at iteration-close via `mstar-compound`.
