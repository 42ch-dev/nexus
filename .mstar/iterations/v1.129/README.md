# V1.129 — Usability bug-sweep (Profile create + honest transport errors)

Iteration package for V1.129. Compass + per-plan specs (product-reviewed, architect-locked, writing-hygiene done). Direction locked: make Profile create work, speak honest transport errors, close dogfood-visible nits only.

## Documents

| Path | Purpose |
|------|---------|
| [`delivery-compass.md`](delivery-compass.md) | Iteration SSOT (frontmatter `status`, branch policy, plans, author-observable ACs, non-goals) |
| [`specs/profile-create-reliability.md`](specs/profile-create-reliability.md) | P0 — footer Add creator succeeds; dialog classified failures + CTAs |
| [`specs/transport-error-ux.md`](specs/transport-error-ux.md) | P1 — same transport language app-wide (`TransportErrorBlock`, Studio-first → `@42ch/nexus-ui`) |
| [`specs/dogfood-nit-closeout.md`](specs/dogfood-nit-closeout.md) | P2 — visible nit subset only (Delete on submenu R-V1126P0-T2-001; i18n R-P1-001 + triage) |

Plans live in `.mstar/plans/`:

- `2026-07-21-v1.129-p0-profile-create-reliability.md`
- `2026-07-21-v1.129-p1-transport-error-ux.md`
- `2026-07-21-v1.129-p2-dogfood-nit-closeout.md`

Branch policy: `iteration_base_branch=main`, `spec_integration_branch=iteration/v1.129`, `target_branch=main`.
