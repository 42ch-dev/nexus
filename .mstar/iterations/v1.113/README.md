# V1.113 Iteration Workspace

**Direction (locked):** i18n Completion + UI Normalization + Tech Debt Paydown

## Product story

Finish what V1.112 started: zh-CN authors should get full Control Room product-page
localization (not chrome-only), disabled/listbox UI should follow DESIGN.md tokens,
and the residual/test slate should be quieter before the next feature wave.

## Specs

| Spec | Purpose | Priority | Depends on |
|------|---------|----------|------------|
| `specs/i18n-completion.md` | Migrate remaining ~110 hardcoded Control Room strings + catalog hygiene | Must (iteration Done gate) | — |
| `specs/ui-normalization.md` | DESIGN.md token gaps + disabled opacity-50 / listbox sweep | Must | — (parallel with P0) |
| `specs/tech-debt-paydown.md` | Design-studio test fix + ≥10 residual closures + status hygiene | Should | P0 |

## Plans

| Plan | File |
|------|------|
| P0 i18n completion | `.mstar/plans/2026-07-12-v1.113-i18n-completion.md` |
| P1 UI normalization | `.mstar/plans/2026-07-12-v1.113-ui-normalization.md` |
| P2 tech debt paydown | `.mstar/plans/2026-07-12-v1.113-tech-debt-paydown.md` |

Compass: `../v1.113/delivery-compass.md`

## Context snapshot

- V1.112 shipped frontend i18n foundation + primary UI migration for en/zh-CN (PR #142)
- ~110 remaining hardcoded strings in 9 page/dialog files
- 2 DESIGN.md token gaps + disabled `opacity-50` pattern in 6+ components
- ~7 pre-existing design-studio test failures (likely i18n-related; per R-P0-006)
- ~87 open tech debt residuals (1 medium, 35 low, 51 nit) — P2 closes ≥10, not all

## Non-goals (iteration)

- New canvas layout engines / new author loops
- Languages beyond en / zh-CN; CLI localization; wire contract changes
- Full residual slate wipe
